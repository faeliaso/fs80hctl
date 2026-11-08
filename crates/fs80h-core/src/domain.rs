use crate::quality::QualityReport;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use thiserror::Error;

pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 480;
pub const DPI: u16 = 500;
pub const FRAME_LEN: usize = (WIDTH * HEIGHT) as usize;
pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error(
        "invalid FS80H frame: expected 320x480 L8/500 DPI ({FRAME_LEN} bytes), got {width}x{height} {dpi} DPI ({len} bytes)"
    )]
    InvalidFrame {
        width: u32,
        height: u32,
        dpi: u16,
        len: usize,
    },
    #[error("unsupported image format; expected an 8-bit grayscale 320x480 PNG")]
    InvalidPng,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Png(#[from] png::DecodingError),
    #[error(transparent)]
    Encoding(#[from] png::EncodingError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawFrame {
    pixels: Vec<u8>,
}

impl RawFrame {
    pub fn new(pixels: Vec<u8>) -> Result<Self, CoreError> {
        if pixels.len() != FRAME_LEN {
            return Err(CoreError::InvalidFrame {
                width: WIDTH,
                height: HEIGHT,
                dpi: DPI,
                len: pixels.len(),
            });
        }
        Ok(Self { pixels })
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn sha256(&self) -> String {
        sha256_bytes(&self.pixels)
    }
    /// Retorna uma cópia do frame com polaridade invertida (255 - pixel).
    ///
    /// O sensor óptico entrega cristas claras sobre fundo escuro.
    /// A representação AFIS padrão da indústria utiliza cristas escuras sobre fundo claro.
    #[must_use]
    pub fn inverted(&self) -> Self {
        let pixels = self.pixels.iter().map(|&p| u8::MAX - p).collect();
        Self { pixels }
    }
    pub fn from_png(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let decoder = png::Decoder::new(fs::File::open(path)?);
        let mut reader = decoder.read_info()?;
        let mut data = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut data)?;
        if info.width != WIDTH
            || info.height != HEIGHT
            || info.color_type != png::ColorType::Grayscale
            || info.bit_depth != png::BitDepth::Eight
        {
            return Err(CoreError::InvalidPng);
        }
        Self::new(data[..info.buffer_size()].to_vec())
    }
    pub fn write_png(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        let file = fs::File::create(path)?;
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: 19_685,
            yppu: 19_685,
            unit: png::Unit::Meter,
        }));
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&self.pixels)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub bus_number: u8,
    pub address: u8,
    pub model: u8,
}

pub trait FingerprintScanner: Send {
    type Error: std::error::Error + Send + Sync + 'static;
    fn capture(&mut self) -> Result<RawFrame, Self::Error>;
}

#[derive(Clone, Debug, Serialize)]
pub struct CaptureManifest {
    pub schema_version: u8,
    pub captured_at_utc: DateTime<Utc>,
    pub sha256_raw_frame: String,
    pub raw_frame_bytes: usize,
    pub width: u32,
    pub height: u32,
    pub dpi: u16,
    pub quality: QualityReport,
    pub tool_version: String,
    pub driver_abi_version: Option<u32>,
    pub device: Option<DeviceInfo>,
    pub standard_afis: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct DerivedManifest {
    pub schema_version: u8,
    pub sha256_derived_artifact: String,
    pub sha256_raw_frame: String,
    pub format: String,
    pub transformations: Vec<String>,
}
impl DerivedManifest {
    pub fn write_adjacent(&self, artifact: impl AsRef<Path>) -> Result<(), CoreError> {
        fs::write(
            format!("{}.meta.json", artifact.as_ref().display()),
            serde_json::to_vec_pretty(self)?,
        )?;
        Ok(())
    }
}

impl CaptureManifest {
    pub fn for_frame(
        frame: &RawFrame,
        quality: QualityReport,
        tool_version: impl Into<String>,
        driver_abi_version: Option<u32>,
        device: Option<DeviceInfo>,
    ) -> Self {
        Self::for_frame_with_afis(
            frame,
            quality,
            tool_version,
            driver_abi_version,
            device,
            true,
        )
    }

    pub fn for_frame_with_afis(
        frame: &RawFrame,
        quality: QualityReport,
        tool_version: impl Into<String>,
        driver_abi_version: Option<u32>,
        device: Option<DeviceInfo>,
        standard_afis: bool,
    ) -> Self {
        Self {
            schema_version: 1,
            captured_at_utc: Utc::now(),
            sha256_raw_frame: frame.sha256(),
            raw_frame_bytes: FRAME_LEN,
            width: WIDTH,
            height: HEIGHT,
            dpi: DPI,
            quality,
            tool_version: tool_version.into(),
            driver_abi_version,
            device,
            standard_afis,
        }
    }
    pub fn write_adjacent(&self, png_path: impl AsRef<Path>) -> Result<(), CoreError> {
        let name = format!("{}.meta.json", png_path.as_ref().display());
        fs::write(name, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_frame_invalid_buffer_size() {
        assert!(RawFrame::new(vec![0; 100]).is_err());
        assert!(RawFrame::new(vec![0; FRAME_LEN - 1]).is_err());
        assert!(RawFrame::new(vec![0; FRAME_LEN + 1]).is_err());
    }

    #[test]
    fn test_raw_frame_valid_creation_and_hash() {
        let bytes = vec![128u8; FRAME_LEN];
        let frame = RawFrame::new(bytes.clone()).expect("valid frame");
        assert_eq!(frame.pixels(), &bytes[..]);
        assert_eq!(frame.sha256(), sha256_bytes(&bytes));
    }

    #[test]
    fn test_raw_frame_inverted() {
        let mut bytes = vec![0u8; FRAME_LEN];
        bytes[0] = 10;
        bytes[1] = 250;
        let frame = RawFrame::new(bytes).unwrap();
        let inv = frame.inverted();
        assert_eq!(inv.pixels()[0], 245);
        assert_eq!(inv.pixels()[1], 5);
        assert_eq!(inv.inverted().pixels(), frame.pixels());
    }

    #[test]
    fn test_raw_frame_png_roundtrip() {
        let mut pixels = vec![0u8; FRAME_LEN];
        for (i, p) in pixels.iter_mut().enumerate() {
            *p = (i % 256) as u8;
        }
        let frame = RawFrame::new(pixels).expect("valid frame");

        let tmp_dir =
            std::env::temp_dir().join(format!("fs80h-domain-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let png_path = tmp_dir.join("test_roundtrip.png");

        frame.write_png(&png_path).expect("png write");
        let decoded = RawFrame::from_png(&png_path).expect("png read");

        assert_eq!(frame.pixels(), decoded.pixels());
        assert_eq!(frame.sha256(), decoded.sha256());

        let _ = std::fs::remove_file(&png_path);
        let _ = std::fs::remove_dir(&tmp_dir);
    }

    #[test]
    fn test_capture_manifest_fields() {
        let frame = RawFrame::new(vec![100u8; FRAME_LEN]).unwrap();
        let quality = crate::quality::evaluate_quality(&frame);
        let manifest = CaptureManifest::for_frame(&frame, quality, "0.1.0", None, None);

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.width, WIDTH);
        assert_eq!(manifest.height, HEIGHT);
        assert_eq!(manifest.dpi, DPI);
        assert_eq!(manifest.raw_frame_bytes, FRAME_LEN);
        assert_eq!(manifest.sha256_raw_frame, frame.sha256());
    }
}
