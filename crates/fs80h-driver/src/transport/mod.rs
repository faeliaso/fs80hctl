pub mod mock;
pub mod rusb_impl;

pub use mock::{MockScenario, MockTransport};
pub use rusb_impl::RusbTransport;

use std::time::Duration;
use thiserror::Error;

/// Typed errors occurring during USB transport operations.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("USB transfer timed out")]
    Timeout,

    #[error("USB device disconnected")]
    DeviceDisconnected,

    #[error("USB endpoint halted / pipe error")]
    Pipe,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("USB transport error: {0}")]
    Usb(String),
}

/// Abstract byte-level and control USB communication contract.
pub trait UsbTransport: Send + Sync {
    /// Sends a command or control packet to the device bulk out endpoint.
    fn write_bulk(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, TransportError>;

    /// Reads response data or biometric frame packets via bulk transfer.
    fn read_bulk(
        &mut self,
        endpoint: u8,
        buffer: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, TransportError>;

    /// Executes sensor calibration, diode, or register control transfers.
    fn control_transfer(
        &mut self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, TransportError>;
}

impl<T: UsbTransport + ?Sized> UsbTransport for Box<T> {
    fn write_bulk(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, TransportError> {
        (**self).write_bulk(endpoint, data, timeout)
    }

    fn read_bulk(
        &mut self,
        endpoint: u8,
        buffer: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, TransportError> {
        (**self).read_bulk(endpoint, buffer, timeout)
    }

    fn control_transfer(
        &mut self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, TransportError> {
        (**self).control_transfer(request_type, request, value, index, data, timeout)
    }
}
