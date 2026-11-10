pub mod frame;
pub mod protocol;
pub mod transport;

pub use frame::{FrameAssembler, FrameError, parse_raw_frame};
pub use protocol::{
    CommandOpcode, ProtocolError, ResponsePacket, UsbCommand, parse_response_packet,
};
pub use transport::{MockScenario, MockTransport, RusbTransport, TransportError, UsbTransport};

use fs80h_core::{DeviceInfo, FingerprintScanner, RawFrame};
use futronic_fs80h_rs as native;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriverError {
    #[error(transparent)]
    Native(#[from] native::ScannerError),

    #[error(transparent)]
    Core(#[from] fs80h_core::CoreError),

    #[error(transparent)]
    Transport(#[from] TransportError),

    #[error(transparent)]
    Frame(#[from] frame::FrameError),

    #[error(transparent)]
    Protocol(#[from] protocol::ProtocolError),

    #[error("timed out waiting for finger on scanner after {0:?}")]
    Timeout(Duration),
}

fn device_info(value: &native::DeviceInfo) -> DeviceInfo {
    DeviceInfo {
        vendor_id: value.vendor_id,
        product_id: value.product_id,
        bus_number: value.bus_number,
        address: value.address,
        model: if value.product_id == 0x0025 { 81 } else { 80 },
    }
}

/// Discovers connected physical Futronic scanners on the USB bus, or reports simulated
/// devices if a mock scenario is configured via environment variable `FS80H_MOCK_FIXTURE`.
pub fn list_devices() -> Result<Vec<DeviceInfo>, DriverError> {
    if let Ok(scenario) = std::env::var("FS80H_MOCK_FIXTURE").and_then(|v| {
        v.parse::<MockScenario>()
            .map_err(|_| std::env::VarError::NotPresent)
    }) {
        return list_devices_with_mock(Some(scenario));
    }
    list_devices_with_mock(None)
}

/// Discovers devices with explicit mock scenario control.
pub fn list_devices_with_mock(mock: Option<MockScenario>) -> Result<Vec<DeviceInfo>, DriverError> {
    if let Some(scenario) = mock {
        return match scenario {
            MockScenario::Disconnect => Ok(Vec::new()),
            _ => Ok(vec![DeviceInfo {
                vendor_id: 0x1491,
                product_id: 0x0020,
                bus_number: 1,
                address: 2,
                model: 80,
            }]),
        };
    }

    Ok(native::list_devices()?
        .into_iter()
        .map(|d| DeviceInfo {
            vendor_id: d.vendor_id,
            product_id: d.product_id,
            bus_number: d.id.bus_number,
            address: d.id.address,
            model: if d.product_id == 0x0025 { 81 } else { 80 },
        })
        .collect())
}

/// Type-safe biometric scanner device parameterized by its USB transport abstraction.
pub struct Device<T: UsbTransport> {
    transport: T,
    info: DeviceInfo,
}

impl Device<RusbTransport> {
    /// Opens the first available physical Futronic scanner on the USB bus.
    pub fn open_first() -> Result<Self, DriverError> {
        let transport = RusbTransport::open_first()?;
        let info = device_info(transport.info());
        Ok(Self { transport, info })
    }

    /// Opens a specific physical Futronic scanner identified by bus and address.
    pub fn open(info: DeviceInfo) -> Result<Self, DriverError> {
        let id = native::DeviceId::new(info.bus_number, info.address);
        let transport = RusbTransport::open_by_id(&id)?;
        Ok(Self { transport, info })
    }
}

impl Device<MockTransport> {
    /// Instantiates a virtual mock scanner device for automated testing.
    pub fn mock(transport: MockTransport) -> Self {
        let info = DeviceInfo {
            vendor_id: 0x1491,
            product_id: 0x0020,
            bus_number: 1,
            address: 2,
            model: 80,
        };
        Self { transport, info }
    }
}

impl<T: UsbTransport + 'static> Device<T> {
    /// Converts a concrete generic device into a dynamic trait-object boxed device.
    pub fn boxed(self) -> Device<Box<dyn UsbTransport>> {
        Device {
            transport: Box::new(self.transport),
            info: self.info,
        }
    }
}

impl<T: UsbTransport> Device<T> {
    /// Constructs a device from any transport and device metadata.
    pub fn new(transport: T, info: DeviceInfo) -> Self {
        Self { transport, info }
    }

    /// Returns device metadata.
    pub fn device_info(&self) -> Option<DeviceInfo> {
        Some(self.info.clone())
    }

    /// Reference to underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Mutable reference to underlying transport.
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Captures a biometric frame using calibrated exposure dose.
    pub fn capture_with_dose(&mut self, dose: u8) -> Result<RawFrame, DriverError> {
        // Trigger bulk command opcode GET_IMAGE_VARIABLE_DOSE (0x6A)
        let cmd = UsbCommand::new(CommandOpcode::GetImageVariableDose, Some(dose));
        self.transport
            .write_bulk(0x01, &cmd.to_bytes(), Duration::from_millis(2500))?;

        let mut assembler = FrameAssembler::new();
        let chunk_timeout = Duration::from_millis(1500);
        let mut chunk_buf = [0u8; 4096];

        while !assembler.is_complete() {
            let n = self
                .transport
                .read_bulk(0x82, &mut chunk_buf, chunk_timeout)?;
            if n == 0 {
                break;
            }
            if let Some(valid_slice) = chunk_buf.get(..n) {
                assembler.append_chunk(valid_slice)?;
            }
        }

        Ok(assembler.finalize()?)
    }

    /// Waits for finger presence on the sensor surface with contact stabilization.
    pub fn wait_for_finger<F>(
        &mut self,
        timeout: Duration,
        min_quality: u8,
        dose: Option<u8>,
        mut on_poll: F,
    ) -> Result<RawFrame, DriverError>
    where
        F: FnMut(&fs80h_core::FingerPresence, Duration),
    {
        let start = Instant::now();
        let poll_interval = Duration::from_millis(150);

        while start.elapsed() < timeout {
            let frame = match dose {
                Some(d) => self.capture_with_dose(d)?,
                None => self.capture()?,
            };
            let presence = fs80h_core::evaluate_presence(&frame);
            on_poll(&presence, start.elapsed());

            if presence.is_present && presence.quality.score >= min_quality {
                return Ok(frame);
            }
            std::thread::sleep(poll_interval);
        }

        Err(DriverError::Timeout(timeout))
    }
}

impl<T: UsbTransport> FingerprintScanner for Device<T> {
    type Error = DriverError;

    fn capture(&mut self) -> Result<RawFrame, Self::Error> {
        self.capture_with_dose(166)
    }
}

/// Dynamic trait-object scanner compatible with both real physical USB hardware
/// and simulated virtual mock fixtures.
pub type FutronicScanner = Device<Box<dyn UsbTransport>>;

impl FutronicScanner {
    /// Opens the first available physical USB scanner.
    pub fn open_first() -> Result<Self, DriverError> {
        Ok(Device::<RusbTransport>::open_first()?.boxed())
    }

    /// Opens a specific physical USB scanner.
    pub fn open(info: DeviceInfo) -> Result<Self, DriverError> {
        Ok(Device::<RusbTransport>::open(info)?.boxed())
    }

    /// Creates a virtual mock scanner device for automated testing.
    pub fn new_mock(transport: MockTransport) -> Self {
        Device::<MockTransport>::mock(transport).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs80h_core::FRAME_LEN;

    #[test]
    fn test_mock_device_capture_match() {
        let mock = MockTransport::new(MockScenario::Match);
        let mut device = Device::mock(mock);
        let frame = device.capture().expect("capture match");
        assert_eq!(frame.pixels().len(), FRAME_LEN);
        let quality = fs80h_core::evaluate_quality(&frame);
        assert!(quality.score > 70);
    }

    #[test]
    fn test_mock_device_capture_disconnect() {
        let mock = MockTransport::new(MockScenario::Disconnect);
        let mut device = Device::mock(mock);
        let result = device.capture();
        assert!(result.is_err());
        match result.unwrap_err() {
            DriverError::Transport(TransportError::DeviceDisconnected) => {}
            other => panic!("expected DeviceDisconnected, got: {other:?}"),
        }
    }

    #[test]
    fn test_mock_device_wait_for_finger_match() {
        let mock = MockTransport::new(MockScenario::Match);
        let mut device = Device::mock(mock);
        let mut poll_count = 0;
        let frame = device
            .wait_for_finger(Duration::from_millis(500), 50, None, |_, _| {
                poll_count += 1;
            })
            .expect("wait for finger should succeed immediately for match scenario");
        assert!(poll_count >= 1);
        assert_eq!(frame.pixels().len(), FRAME_LEN);
    }

    #[test]
    fn test_mock_device_wait_for_finger_empty_timeout() {
        let mock = MockTransport::new(MockScenario::Empty);
        let mut device = Device::mock(mock);
        let result = device.wait_for_finger(Duration::from_millis(100), 50, None, |_, _| {});
        assert!(result.is_err());
        match result.unwrap_err() {
            DriverError::Timeout(_) => {}
            other => panic!("expected Timeout, got: {other:?}"),
        }
    }
}
