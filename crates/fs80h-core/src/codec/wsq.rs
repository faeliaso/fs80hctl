//! Pure-Rust WSQ adapters for validated FS80H grayscale frames.

use crate::{CoreError, DPI, HEIGHT, RawFrame, WIDTH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WsqError {
    #[error("WSQ codec error: {0}")]
    Codec(String),
    #[error("WSQ image must be {WIDTH}x{HEIGHT}, got {width}x{height}")]
    Dimensions { width: usize, height: usize },
    #[error(transparent)]
    Frame(#[from] CoreError),
}

pub fn encode_wsq(frame: &RawFrame, bitrate: f32, dpi: u16) -> Result<Vec<u8>, WsqError> {
    if !bitrate.is_finite() || bitrate <= 0.0 {
        return Err(WsqError::Codec(
            "bitrate must be finite and greater than zero".into(),
        ));
    }
    let options = wsq_codec::EncodeOptions::default()
        .with_bitrate(bitrate)
        .with_ppi(Some(dpi as u32))
        .with_threads(0);
    wsq_codec::encode(frame.pixels(), WIDTH as usize, HEIGHT as usize, &options)
        .map_err(|error| WsqError::Codec(error.to_string()))
}

pub fn decode_wsq(data: &[u8]) -> Result<RawFrame, WsqError> {
    let options = wsq_codec::DecodeOptions::default()
        .with_max_pixels(Some((WIDTH * HEIGHT) as usize))
        .with_threads(0);
    let image = wsq_codec::decode_with(data, &options)
        .map_err(|error| WsqError::Codec(error.to_string()))?;
    if image.width != WIDTH as usize || image.height != HEIGHT as usize {
        return Err(WsqError::Dimensions {
            width: image.width,
            height: image.height,
        });
    }
    RawFrame::new(image.pixels).map_err(WsqError::from)
}

/// Returns the WSQ-recorded resolution, defaulting to FS80H's 500 DPI when absent.
pub fn wsq_dpi(data: &[u8]) -> Result<u16, WsqError> {
    let info = wsq_codec::read_info(data).map_err(|error| WsqError::Codec(error.to_string()))?;
    Ok(info.ppi.unwrap_or(DPI as u32).min(u16::MAX as u32) as u16)
}
