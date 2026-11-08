use fs80h_core::{DeviceInfo, FingerprintScanner, RawFrame};
use futronic_fs80h_rs as native;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriverError {
    #[error(transparent)]
    Native(#[from] native::ScannerError),
    #[error(transparent)]
    Core(#[from] fs80h_core::CoreError),
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

pub fn list_devices() -> Result<Vec<DeviceInfo>, DriverError> {
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

pub struct FutronicScanner {
    device: native::FutronicDevice,
    info: DeviceInfo,
}

impl FutronicScanner {
    pub fn open_first() -> Result<Self, DriverError> {
        let device = native::FutronicDevice::open_first()?;
        let info = device_info(device.info());
        Ok(Self { device, info })
    }

    pub fn open(info: DeviceInfo) -> Result<Self, DriverError> {
        let device = native::FutronicDevice::open_by_id(&native::DeviceId::new(info.bus_number, info.address))?;
        Ok(Self { device, info })
    }

    pub fn device_info(&self) -> Option<DeviceInfo> { Some(self.info.clone()) }
}

impl FingerprintScanner for FutronicScanner {
    type Error = DriverError;

    fn capture(&mut self) -> Result<RawFrame, Self::Error> {
        Ok(RawFrame::new(self.device.capture_frame()?.into_bytes())?)
    }

    fn auto_capture(&mut self, _timeout_ms: u64) -> Result<RawFrame, Self::Error> {
        // Workflow-level polling moved out of the hardware driver. The CLI keeps
        // this trait entry point while its acquisition crate is introduced.
        self.capture()
    }
}
