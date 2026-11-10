use super::{TransportError, UsbTransport};
use futronic_fs80h_rs as native;
use std::time::Duration;

/// Production USB transport utilizing native USB driver communication.
pub struct RusbTransport {
    device: native::FutronicDevice,
    pending_buffer: Vec<u8>,
    buffer_offset: usize,
}

impl RusbTransport {
    /// Opens the first detected physical Futronic scanner.
    pub fn open_first() -> Result<Self, native::ScannerError> {
        let device = native::FutronicDevice::open_first()?;
        Ok(Self {
            device,
            pending_buffer: Vec::new(),
            buffer_offset: 0,
        })
    }

    /// Opens a specific physical Futronic scanner by bus and address ID.
    pub fn open_by_id(id: &native::DeviceId) -> Result<Self, native::ScannerError> {
        let device = native::FutronicDevice::open_by_id(id)?;
        Ok(Self {
            device,
            pending_buffer: Vec::new(),
            buffer_offset: 0,
        })
    }

    /// Access the underlying native device info.
    pub fn info(&self) -> &native::DeviceInfo {
        self.device.info()
    }

    /// Direct native frame capture.
    pub fn capture_frame(&mut self) -> Result<native::RawFrame, native::ScannerError> {
        self.device.capture_frame()
    }

    /// Direct native frame capture with calibrated exposure dose.
    pub fn capture_frame_with_dose(
        &mut self,
        dose: u8,
    ) -> Result<native::RawFrame, native::ScannerError> {
        self.device.capture_frame_with_dose(dose)
    }
}

impl UsbTransport for RusbTransport {
    fn write_bulk(
        &mut self,
        _endpoint: u8,
        data: &[u8],
        _timeout: Duration,
    ) -> Result<usize, TransportError> {
        // Trigger capture based on opcode GET_IMAGE_VARIABLE_DOSE (0x6A)
        if !data.is_empty() && data[0] == 0x6A {
            let dose = if data.len() > 1 { data[1] } else { 166 };
            let frame = self
                .device
                .capture_frame_with_dose(dose)
                .map_err(|e| TransportError::Usb(e.to_string()))?;
            self.pending_buffer = frame.into_bytes();
            self.buffer_offset = 0;
            return Ok(data.len());
        }

        // Generic commands or other sensor triggers
        Ok(data.len())
    }

    fn read_bulk(
        &mut self,
        _endpoint: u8,
        buffer: &mut [u8],
        _timeout: Duration,
    ) -> Result<usize, TransportError> {
        if self.buffer_offset >= self.pending_buffer.len() {
            return Ok(0);
        }

        let remaining = self.pending_buffer.len() - self.buffer_offset;
        let to_copy = remaining.min(buffer.len());
        buffer[..to_copy].copy_from_slice(
            &self.pending_buffer[self.buffer_offset..self.buffer_offset + to_copy],
        );
        self.buffer_offset += to_copy;
        Ok(to_copy)
    }

    fn control_transfer(
        &mut self,
        _request_type: u8,
        _request: u8,
        _value: u16,
        _index: u16,
        _data: &mut [u8],
        _timeout: Duration,
    ) -> Result<usize, TransportError> {
        Ok(0)
    }
}
