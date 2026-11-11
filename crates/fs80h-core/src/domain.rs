use crate::quality::QualityReport;
use chrono::{DateTime, Utc};
use image::codecs::jpeg::{JpegEncoder, PixelDensity};
use image::{ExtendedColorType, ImageFormat as RasterImageFormat};
use openjp2::{
    ColorSpace, Decoder, EncodeOptions, Encoder, Format as Jpeg2000Format, Image, ImageComponent,
};
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    #[default]
    Png,
    Tiff,
    Pgm,
    Raw,
    Wsq,
    Jpeg2000,
    Jpeg,
    Bmp,
}

impl std::fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Png => write!(f, "png"),
            Self::Tiff => write!(f, "tiff"),
            Self::Pgm => write!(f, "pgm"),
            Self::Raw => write!(f, "raw"),
            Self::Wsq => write!(f, "wsq"),
            Self::Jpeg2000 => write!(f, "jp2"),
            Self::Jpeg => write!(f, "jpeg"),
            Self::Bmp => write!(f, "bmp"),
        }
    }
}

impl std::str::FromStr for ImageFormat {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "png" => Ok(Self::Png),
            "tiff" | "tif" => Ok(Self::Tiff),
            "pgm" => Ok(Self::Pgm),
            "raw" | "dat" => Ok(Self::Raw),
            "wsq" => Ok(Self::Wsq),
            "jp2" | "jpeg2000" => Ok(Self::Jpeg2000),
            "jpeg" | "jpg" => Ok(Self::Jpeg),
            "bmp" => Ok(Self::Bmp),
            other => Err(CoreError::UnsupportedFormat(other.to_string())),
        }
    }
}

impl ImageFormat {
    pub fn from_path_extension(path: impl AsRef<Path>) -> Option<Self> {
        let ext = path.as_ref().extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "png" => Some(Self::Png),
            "tiff" | "tif" => Some(Self::Tiff),
            "pgm" => Some(Self::Pgm),
            "raw" | "dat" => Some(Self::Raw),
            "wsq" => Some(Self::Wsq),
            "jp2" => Some(Self::Jpeg2000),
            "jpeg" | "jpg" => Some(Self::Jpeg),
            "bmp" => Some(Self::Bmp),
            _ => None,
        }
    }

    pub fn detect(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let p = path.as_ref();
        if let Ok(mut f) = fs::File::open(p) {
            use std::io::Read;
            let mut magic = [0u8; 12];
            if let Ok(n) = f.read(&mut magic) {
                if n >= 8 && &magic[0..8] == b"\x89PNG\r\n\x1a\n" {
                    return Ok(Self::Png);
                }
                if n >= 4 && (&magic[0..4] == b"II*\x00" || &magic[0..4] == b"MM\x00*") {
                    return Ok(Self::Tiff);
                }
                if n >= 2 && &magic[0..2] == b"P5" {
                    return Ok(Self::Pgm);
                }
                if n >= 4 && magic[0..4] == [0xff, 0xa0, 0xff, 0xa8] {
                    return Ok(Self::Wsq);
                }
                if n >= 12 && magic[0..12] == [0, 0, 0, 12, b'j', b'P', b' ', b' ', 13, 10, 135, 10]
                {
                    return Ok(Self::Jpeg2000);
                }
                if n >= 2 && magic[0..2] == [0xff, 0x4f] {
                    return Ok(Self::Jpeg2000);
                }
                if n >= 3 && magic[0..3] == [0xff, 0xd8, 0xff] {
                    return Ok(Self::Jpeg);
                }
                if n >= 2 && &magic[0..2] == b"BM" {
                    return Ok(Self::Bmp);
                }
            }
        }
        if let Some(fmt) = Self::from_path_extension(p) {
            return Ok(fmt);
        }
        if fs::metadata(p)
            .map(|m| m.len() == FRAME_LEN as u64)
            .unwrap_or(false)
        {
            return Ok(Self::Raw);
        }
        Err(CoreError::UnsupportedFormat(format!(
            "unable to determine format for {}",
            p.display()
        )))
    }
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
    #[error("invalid PGM file: {0}")]
    InvalidPgm(String),
    #[error(transparent)]
    Tiff(#[from] tiff::TiffError),
    #[error("unsupported or unknown image format: {0}")]
    UnsupportedFormat(String),
    #[error("invalid quality score: {0} (must be 0..=100)")]
    InvalidQualityScore(u8),
    #[error("invalid match score: {0} (must be 0.0..=100.0)")]
    InvalidMatchScore(f32),
    #[error("image dimension calculation overflow")]
    DimensionOverflow,
    #[error("buffer size mismatch: expected {expected} bytes, got {actual}")]
    BufferSizeMismatch { expected: usize, actual: usize },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Png(#[from] png::DecodingError),
    #[error(transparent)]
    Encoding(#[from] png::EncodingError),
    #[error(transparent)]
    Raster(#[from] image::ImageError),
    #[error("image codec error: {0}")]
    Codec(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Biometric quality score normalized between 0 and 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QualityScore(u8);

impl QualityScore {
    /// Constructs a validated quality score.
    pub fn new(score: u8) -> Result<Self, CoreError> {
        if score <= 100 {
            Ok(Self(score))
        } else {
            Err(CoreError::InvalidQualityScore(score))
        }
    }

    /// Access primitive integer score.
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

impl std::ops::Deref for QualityScore {
    type Target = u8;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for QualityScore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<QualityScore> for u8 {
    fn from(q: QualityScore) -> Self {
        q.0
    }
}

impl PartialEq<u8> for QualityScore {
    fn eq(&self, other: &u8) -> bool {
        self.0 == *other
    }
}

impl PartialEq<QualityScore> for u8 {
    fn eq(&self, other: &QualityScore) -> bool {
        *self == other.0
    }
}

impl PartialOrd<u8> for QualityScore {
    fn partial_cmp(&self, other: &u8) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialOrd<QualityScore> for u8 {
    fn partial_cmp(&self, other: &QualityScore) -> Option<std::cmp::Ordering> {
        self.partial_cmp(&other.0)
    }
}

/// Biometric match similarity score normalized between 0.0 and 100.0.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MatchScore(f32);

impl MatchScore {
    /// Constructs a validated match score.
    pub fn new(score: f32) -> Result<Self, CoreError> {
        if (0.0..=100.0).contains(&score) {
            Ok(Self(score))
        } else {
            Err(CoreError::InvalidMatchScore(score))
        }
    }

    /// Access primitive float score.
    pub const fn as_f32(self) -> f32 {
        self.0
    }
}

impl std::ops::Deref for MatchScore {
    type Target = f32;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for MatchScore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}", self.0)
    }
}

impl From<MatchScore> for f32 {
    fn from(m: MatchScore) -> Self {
        m.0
    }
}

impl PartialEq<f32> for MatchScore {
    fn eq(&self, other: &f32) -> bool {
        self.0 == *other
    }
}

impl PartialEq<MatchScore> for f32 {
    fn eq(&self, other: &MatchScore) -> bool {
        *self == other.0
    }
}

impl PartialOrd<f32> for MatchScore {
    fn partial_cmp(&self, other: &f32) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialOrd<MatchScore> for f32 {
    fn partial_cmp(&self, other: &MatchScore) -> Option<std::cmp::Ordering> {
        self.partial_cmp(&other.0)
    }
}

/// Optical scanning resolution in dots per inch (e.g. 500 DPI for FS80H).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Dpi(u32);

impl Dpi {
    pub const fn new(dpi: u32) -> Self {
        Self(dpi)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl std::ops::Deref for Dpi {
    type Target = u32;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for Dpi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Contiguous 8-bit grayscale image buffer with strictly validated dimensions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrayscaleImage {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl GrayscaleImage {
    /// Creates a new grayscale image validating that pixel buffer matches width * height.
    pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> Result<Self, CoreError> {
        let expected_len = width
            .checked_mul(height)
            .ok_or(CoreError::DimensionOverflow)?;
        if pixels.len() != expected_len {
            return Err(CoreError::BufferSizeMismatch {
                expected: expected_len,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    pub fn as_slice_2d(&self) -> &[u8] {
        &self.pixels
    }

    /// Loads an 8-bit grayscale TIFF without imposing FS80H frame dimensions.
    /// This is intended for offline analysis datasets; scanner capture remains
    /// represented by the dimension-validated [`RawFrame`].
    pub fn from_tiff(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let file = fs::File::open(path)?;
        let mut decoder = tiff::decoder::Decoder::new(file)?;
        let (width, height) = decoder.dimensions()?;
        let pixels = match decoder.read_image()? {
            tiff::decoder::DecodingResult::U8(pixels) => pixels,
            _ => {
                return Err(CoreError::UnsupportedFormat(
                    "expected 8-bit grayscale TIFF".into(),
                ));
            }
        };
        Self::new(width as usize, height as usize, pixels)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawFrame {
    image: GrayscaleImage,
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
        let image = GrayscaleImage::new(WIDTH as usize, HEIGHT as usize, pixels)?;
        Ok(Self { image })
    }

    /// Constructs a RawFrame from a validated GrayscaleImage.
    pub fn from_image(image: GrayscaleImage) -> Result<Self, CoreError> {
        if image.width() != WIDTH as usize || image.height() != HEIGHT as usize {
            return Err(CoreError::InvalidFrame {
                width: image.width() as u32,
                height: image.height() as u32,
                dpi: DPI,
                len: image.pixels().len(),
            });
        }
        Ok(Self { image })
    }

    /// Reference to underlying GrayscaleImage.
    pub fn as_image(&self) -> &GrayscaleImage {
        &self.image
    }

    /// Converts into underlying GrayscaleImage.
    pub fn into_image(self) -> GrayscaleImage {
        self.image
    }

    pub fn pixels(&self) -> &[u8] {
        self.image.pixels()
    }

    pub fn sha256(&self) -> String {
        sha256_bytes(self.image.pixels())
    }

    /// Returns a copy of the frame with inverted polarity (255 - pixel).
    ///
    /// The optical sensor delivers light ridges on a dark background.
    /// Standard industry AFIS representation uses dark ridges on a light background.
    #[must_use]
    pub fn inverted(&self) -> Self {
        let pixels = self.image.pixels().iter().map(|&p| u8::MAX - p).collect();
        let image = GrayscaleImage {
            width: self.image.width,
            height: self.image.height,
            pixels,
        };
        Self { image }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let p = path.as_ref();
        let format = ImageFormat::detect(p)?;
        match format {
            ImageFormat::Png => Self::from_png(p),
            ImageFormat::Tiff => Self::from_tiff(p),
            ImageFormat::Pgm => Self::from_pgm(p),
            ImageFormat::Raw => Self::from_raw_bytes_file(p),
            ImageFormat::Wsq => crate::decode_wsq(&fs::read(p)?)
                .map_err(|error| CoreError::Codec(error.to_string())),
            ImageFormat::Jpeg2000 => Self::from_jpeg2000(p),
            ImageFormat::Jpeg => Self::from_jpeg(p),
            ImageFormat::Bmp => Self::from_bmp(p),
        }
    }

    pub fn write_format(
        &self,
        path: impl AsRef<Path>,
        format: ImageFormat,
    ) -> Result<(), CoreError> {
        match format {
            ImageFormat::Png => self.write_png(path),
            ImageFormat::Tiff => self.write_tiff(path),
            ImageFormat::Pgm => self.write_pgm(path),
            ImageFormat::Raw => self.write_raw_bytes(path),
            ImageFormat::Wsq => self.write_wsq(path),
            ImageFormat::Jpeg2000 => self.write_jpeg2000(path),
            ImageFormat::Jpeg => self.write_jpeg(path),
            ImageFormat::Bmp => self.write_bmp(path),
        }
    }

    pub fn from_png_bytes(bytes: &[u8]) -> Result<Self, CoreError> {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
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

    pub fn from_png(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let bytes = fs::read(path)?;
        Self::from_png_bytes(&bytes)
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
        writer.write_image_data(self.pixels())?;
        Ok(())
    }

    pub fn from_jpeg(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        Self::from_raster_bytes(&fs::read(path)?, RasterImageFormat::Jpeg)
    }

    pub fn write_jpeg(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        let mut encoded = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut encoded, 95);
        encoder.set_pixel_density(PixelDensity::dpi(DPI));
        encoder.encode(self.pixels(), WIDTH, HEIGHT, ExtendedColorType::L8)?;
        fs::write(path, encoded)?;
        Ok(())
    }

    fn from_raster_bytes(bytes: &[u8], format: RasterImageFormat) -> Result<Self, CoreError> {
        let image = image::load_from_memory_with_format(bytes, format)?;
        if image.color() != image::ColorType::L8 {
            return Err(CoreError::Codec("expected an 8-bit grayscale image".into()));
        }
        if image.width() != WIDTH || image.height() != HEIGHT {
            return Err(CoreError::InvalidFrame {
                width: image.width(),
                height: image.height(),
                dpi: DPI,
                len: image.as_bytes().len(),
            });
        }
        Self::new(image.into_luma8().into_raw())
    }

    pub fn write_wsq(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        let encoded = crate::encode_wsq(self, 0.75, DPI)
            .map_err(|error| CoreError::Codec(error.to_string()))?;
        fs::write(path, encoded)?;
        Ok(())
    }

    pub fn from_jpeg2000(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let data = fs::read(path)?;
        let format = Jpeg2000Format::detect(&data)
            .ok_or_else(|| CoreError::Codec("unrecognized JPEG 2000 stream".into()))?;
        let decoded = Decoder::new(format)
            .and_then(|mut decoder| decoder.decode(data))
            .map_err(|error| CoreError::Codec(error.to_string()))?;
        let component = decoded
            .components
            .first()
            .ok_or_else(|| CoreError::Codec("JPEG 2000 image has no grayscale component".into()))?;
        if decoded.width != WIDTH
            || decoded.height != HEIGHT
            || decoded.components.len() != 1
            || component.width != WIDTH
            || component.height != HEIGHT
            || component.precision != 8
            || component.signed
        {
            return Err(CoreError::InvalidFrame {
                width: decoded.width,
                height: decoded.height,
                dpi: DPI,
                len: component.data.len(),
            });
        }
        let pixels = component
            .data
            .iter()
            .map(|&sample| {
                u8::try_from(sample)
                    .map_err(|_| CoreError::Codec("JPEG 2000 sample is outside 8-bit range".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(pixels)
    }

    pub fn write_jpeg2000(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        let image = Image::new(
            WIDTH,
            HEIGHT,
            ColorSpace::Greyscale,
            vec![
                ImageComponent::new(
                    WIDTH,
                    HEIGHT,
                    8,
                    false,
                    self.pixels()
                        .iter()
                        .map(|&pixel| i32::from(pixel))
                        .collect(),
                )
                .map_err(|error| CoreError::Codec(error.to_string()))?,
            ],
        )
        .map_err(|error| CoreError::Codec(error.to_string()))?;
        let mut encoded = Encoder::encode(
            &image,
            &EncodeOptions {
                format: Jpeg2000Format::Jp2,
                irreversible: false,
                rates: vec![0.0],
                ..Default::default()
            },
        )
        .map_err(|error| CoreError::Codec(error.to_string()))?;
        insert_jp2_resolution_box(&mut encoded)?;
        fs::write(path, encoded)?;
        Ok(())
    }

    pub fn from_bmp(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let bytes = fs::read(path)?;
        if bytes.len() < 14 + 40 + 256 * 4 || &bytes[0..2] != b"BM" {
            return Err(CoreError::Codec("invalid BMP header".into()));
        }
        let read_u16 = |offset| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        let read_i32 = |offset| i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let read_u32 = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let data_offset = read_u32(10) as usize;
        let dib_size = read_u32(14) as usize;
        let width = read_i32(18);
        let height = read_i32(22);
        if dib_size < 40
            || width != WIDTH as i32
            || height.unsigned_abs() != HEIGHT
            || read_u16(26) != 1
            || read_u16(28) != 8
            || read_u32(30) != 0
        {
            return Err(CoreError::Codec(
                "expected an uncompressed 320x480 8-bit grayscale BMP".into(),
            ));
        }
        let palette_start = 14 + dib_size;
        if bytes.len() < palette_start + 256 * 4 {
            return Err(CoreError::Codec(
                "BMP grayscale palette is truncated".into(),
            ));
        }
        for value in 0..256usize {
            let entry = palette_start + value * 4;
            if bytes[entry] != value as u8
                || bytes[entry + 1] != value as u8
                || bytes[entry + 2] != value as u8
            {
                return Err(CoreError::Codec(
                    "BMP palette is not 8-bit grayscale".into(),
                ));
            }
        }
        let stride = (WIDTH as usize + 3) & !3;
        if data_offset
            .checked_add(stride * HEIGHT as usize)
            .is_none_or(|end| end > bytes.len())
        {
            return Err(CoreError::Codec("BMP pixel data is truncated".into()));
        }
        let mut pixels = vec![0; FRAME_LEN];
        for row in 0..HEIGHT as usize {
            let source_row = if height > 0 {
                HEIGHT as usize - 1 - row
            } else {
                row
            };
            let source = data_offset + source_row * stride;
            pixels[row * WIDTH as usize..(row + 1) * WIDTH as usize]
                .copy_from_slice(&bytes[source..source + WIDTH as usize]);
        }
        Self::new(pixels)
    }

    pub fn write_bmp(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        const HEADER_LEN: usize = 14 + 40 + 256 * 4;
        let file_len = HEADER_LEN + FRAME_LEN;
        let mut bytes = vec![0; file_len];
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&(file_len as u32).to_le_bytes());
        bytes[10..14].copy_from_slice(&(HEADER_LEN as u32).to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&(WIDTH as i32).to_le_bytes());
        bytes[22..26].copy_from_slice(&(HEIGHT as i32).to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&8u16.to_le_bytes());
        bytes[34..38].copy_from_slice(&(FRAME_LEN as u32).to_le_bytes());
        bytes[38..42].copy_from_slice(&19_685i32.to_le_bytes());
        bytes[42..46].copy_from_slice(&19_685i32.to_le_bytes());
        bytes[46..50].copy_from_slice(&256u32.to_le_bytes());
        for value in 0..256usize {
            let entry = 54 + value * 4;
            bytes[entry..entry + 3].fill(value as u8);
        }
        for row in 0..HEIGHT as usize {
            let destination_row = HEIGHT as usize - 1 - row;
            let destination = HEADER_LEN + destination_row * WIDTH as usize;
            bytes[destination..destination + WIDTH as usize]
                .copy_from_slice(&self.pixels()[row * WIDTH as usize..(row + 1) * WIDTH as usize]);
        }
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn write_pgm(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        let mut header = format!("P5\n{} {}\n255\n", WIDTH, HEIGHT).into_bytes();
        header.extend_from_slice(self.pixels());
        fs::write(path, header)?;
        Ok(())
    }

    pub fn from_pgm(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let data = fs::read(path)?;
        Self::from_pgm_bytes(&data)
    }

    pub fn from_pgm_bytes(data: &[u8]) -> Result<Self, CoreError> {
        let mut i = 0;

        let skip_ws_and_comments = |data: &[u8], mut idx: usize| -> usize {
            while idx < data.len() {
                match data.get(idx) {
                    Some(b) if b.is_ascii_whitespace() => {
                        idx += 1;
                    }
                    Some(b'#') => {
                        idx += 1;
                        while idx < data.len() {
                            if data.get(idx) == Some(&b'\n') {
                                break;
                            }
                            idx += 1;
                        }
                    }
                    _ => break,
                }
            }
            idx
        };

        let read_token = |data: &[u8], mut idx: usize| -> Result<(String, usize), CoreError> {
            idx = skip_ws_and_comments(data, idx);
            let start = idx;
            while idx < data.len() {
                match data.get(idx) {
                    Some(b) if !b.is_ascii_whitespace() && *b != b'#' => {
                        idx += 1;
                    }
                    _ => break,
                }
            }
            if start == idx {
                return Err(CoreError::InvalidPgm(
                    "unexpected end of file in header".into(),
                ));
            }
            let slice = data
                .get(start..idx)
                .ok_or_else(|| CoreError::InvalidPgm("slice range out of bounds".into()))?;
            let token = std::str::from_utf8(slice)
                .map_err(|_| CoreError::InvalidPgm("invalid UTF-8 in header".into()))?
                .to_string();
            Ok((token, idx))
        };

        let (magic, next_i) = read_token(data, i)?;
        if magic != "P5" {
            return Err(CoreError::InvalidPgm(format!(
                "expected magic P5, got {magic}"
            )));
        }
        i = next_i;

        let (w_str, next_i) = read_token(data, i)?;
        let width: u32 = w_str
            .parse()
            .map_err(|_| CoreError::InvalidPgm(format!("invalid width {w_str}")))?;
        i = next_i;

        let (h_str, next_i) = read_token(data, i)?;
        let height: u32 = h_str
            .parse()
            .map_err(|_| CoreError::InvalidPgm(format!("invalid height {h_str}")))?;
        i = next_i;

        let (max_str, next_i) = read_token(data, i)?;
        let maxval: u32 = max_str
            .parse()
            .map_err(|_| CoreError::InvalidPgm(format!("invalid maxval {max_str}")))?;
        i = next_i;

        if width != WIDTH || height != HEIGHT || maxval != 255 {
            return Err(CoreError::InvalidPgm(format!(
                "expected {WIDTH}x{HEIGHT} maxval 255, got {width}x{height} maxval {maxval}"
            )));
        }

        match data.get(i) {
            Some(b) if b.is_ascii_whitespace() => {}
            _ => {
                return Err(CoreError::InvalidPgm(
                    "missing whitespace after maxval".into(),
                ));
            }
        }
        i += 1;

        let payload = data
            .get(i..)
            .ok_or_else(|| CoreError::InvalidPgm("missing image payload".into()))?;
        if payload.len() != FRAME_LEN {
            return Err(CoreError::InvalidPgm(format!(
                "expected {FRAME_LEN} bytes payload, got {}",
                payload.len()
            )));
        }

        Self::new(payload.to_vec())
    }

    pub fn write_tiff(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        let file = fs::File::create(path)?;
        let mut encoder = tiff::encoder::TiffEncoder::new(file)?;
        let mut image = encoder.new_image::<tiff::encoder::colortype::Gray8>(WIDTH, HEIGHT)?;

        image
            .encoder()
            .write_tag(tiff::tags::Tag::ResolutionUnit, 2u16)?; // 2 = Inch
        image.encoder().write_tag(
            tiff::tags::Tag::XResolution,
            tiff::encoder::Rational {
                n: DPI as u32,
                d: 1,
            },
        )?;
        image.encoder().write_tag(
            tiff::tags::Tag::YResolution,
            tiff::encoder::Rational {
                n: DPI as u32,
                d: 1,
            },
        )?;
        image.encoder().write_tag(
            tiff::tags::Tag::Software,
            format!("fs80hctl {}", env!("CARGO_PKG_VERSION")).as_str(),
        )?;
        image
            .encoder()
            .write_tag(tiff::tags::Tag::Make, "Futronic")?;
        image.encoder().write_tag(tiff::tags::Tag::Model, "FS80H")?;

        image.write_data(self.pixels())?;
        Ok(())
    }

    pub fn from_tiff(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let file = fs::File::open(path)?;
        let mut decoder = tiff::decoder::Decoder::new(file)?;
        let (width, height) = decoder.dimensions()?;
        if width != WIDTH || height != HEIGHT {
            return Err(CoreError::InvalidFrame {
                width,
                height,
                dpi: DPI,
                len: (width * height) as usize,
            });
        }
        let decoded = decoder.read_image()?;
        let pixels = match decoded {
            tiff::decoder::DecodingResult::U8(data) => {
                if data.len() != FRAME_LEN {
                    return Err(CoreError::InvalidFrame {
                        width,
                        height,
                        dpi: DPI,
                        len: data.len(),
                    });
                }
                data
            }
            _ => {
                return Err(CoreError::UnsupportedFormat(
                    "expected 8-bit grayscale TIFF".into(),
                ));
            }
        };
        Self::new(pixels)
    }

    pub fn write_raw_bytes(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        fs::write(path, self.pixels())?;
        Ok(())
    }

    pub fn from_raw_bytes_file(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let data = fs::read(path)?;
        Self::new(data)
    }
}

fn insert_jp2_resolution_box(bytes: &mut Vec<u8>) -> Result<(), CoreError> {
    let mut offset = 0usize;
    while offset.checked_add(8).is_some_and(|end| end <= bytes.len()) {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        if length < 8
            || offset
                .checked_add(length)
                .is_none_or(|end| end > bytes.len())
        {
            return Err(CoreError::Codec("invalid JP2 box layout".into()));
        }
        if &bytes[offset + 4..offset + 8] == b"jp2h" {
            let resolution_box = [
                0, 0, 0, 26, b'r', b'e', b's', b' ', 0, 0, 0, 18, b'r', b'e', b's', b'c', 0x01,
                0xf4, 0, 1, 0x01, 0xf4, 0, 1, 0, 0,
            ];
            let expanded = length
                .checked_add(resolution_box.len())
                .ok_or_else(|| CoreError::Codec("JP2 header size overflow".into()))?;
            bytes[offset..offset + 4].copy_from_slice(&(expanded as u32).to_be_bytes());
            bytes.splice(offset + length..offset + length, resolution_box);
            return Ok(());
        }
        offset += length;
    }
    Err(CoreError::Codec("JP2 header box is missing".into()))
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    #[serde(default)]
    pub format: ImageFormat,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DerivedManifest {
    pub schema_version: u8,
    pub sha256_derived_artifact: String,
    pub sha256_raw_frame: String,
    pub format: String,
    pub transformations: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Manifest {
    Capture(CaptureManifest),
    Derived(DerivedManifest),
}

impl Manifest {
    pub fn read_adjacent(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let name = format!("{}.meta.json", path.as_ref().display());
        let data = fs::read(name)?;
        Ok(serde_json::from_slice(&data)?)
    }

    pub fn sha256_raw_frame(&self) -> &str {
        match self {
            Self::Capture(c) => &c.sha256_raw_frame,
            Self::Derived(d) => &d.sha256_raw_frame,
        }
    }
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
        Self::for_frame_with_afis_and_format(
            frame,
            quality,
            tool_version,
            driver_abi_version,
            device,
            true,
            ImageFormat::Png,
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
        Self::for_frame_with_afis_and_format(
            frame,
            quality,
            tool_version,
            driver_abi_version,
            device,
            standard_afis,
            ImageFormat::Png,
        )
    }

    pub fn for_frame_with_afis_and_format(
        frame: &RawFrame,
        quality: QualityReport,
        tool_version: impl Into<String>,
        driver_abi_version: Option<u32>,
        device: Option<DeviceInfo>,
        standard_afis: bool,
        format: ImageFormat,
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
            format,
        }
    }

    pub fn write_adjacent(&self, image_path: impl AsRef<Path>) -> Result<(), CoreError> {
        let name = format!("{}.meta.json", image_path.as_ref().display());
        fs::write(name, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn read_adjacent(image_path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let name = format!("{}.meta.json", image_path.as_ref().display());
        let data = fs::read(name)?;
        Ok(serde_json::from_slice(&data)?)
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
    fn test_raw_frame_pgm_roundtrip() {
        let mut pixels = vec![0u8; FRAME_LEN];
        for (i, p) in pixels.iter_mut().enumerate() {
            *p = (i % 256) as u8;
        }
        let frame = RawFrame::new(pixels).expect("valid frame");

        let tmp_dir =
            std::env::temp_dir().join(format!("fs80h-domain-pgm-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let pgm_path = tmp_dir.join("test_roundtrip.pgm");

        frame.write_pgm(&pgm_path).expect("pgm write");
        let decoded = RawFrame::from_pgm(&pgm_path).expect("pgm read");

        assert_eq!(frame.pixels(), decoded.pixels());
        assert_eq!(frame.sha256(), decoded.sha256());

        let _ = std::fs::remove_file(&pgm_path);
        let _ = std::fs::remove_dir(&tmp_dir);
    }

    #[test]
    fn test_raw_frame_tiff_roundtrip() {
        let mut pixels = vec![0u8; FRAME_LEN];
        for (i, p) in pixels.iter_mut().enumerate() {
            *p = (i % 256) as u8;
        }
        let frame = RawFrame::new(pixels).expect("valid frame");

        let tmp_dir =
            std::env::temp_dir().join(format!("fs80h-domain-tiff-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let tiff_path = tmp_dir.join("test_roundtrip.tiff");

        frame.write_tiff(&tiff_path).expect("tiff write");
        let decoded = RawFrame::from_tiff(&tiff_path).expect("tiff read");

        assert_eq!(frame.pixels(), decoded.pixels());
        assert_eq!(frame.sha256(), decoded.sha256());

        let _ = std::fs::remove_file(&tiff_path);
        let _ = std::fs::remove_dir(&tmp_dir);
    }

    #[test]
    fn test_raw_frame_raw_roundtrip() {
        let mut pixels = vec![0u8; FRAME_LEN];
        for (i, p) in pixels.iter_mut().enumerate() {
            *p = (i % 256) as u8;
        }
        let frame = RawFrame::new(pixels).expect("valid frame");

        let tmp_dir =
            std::env::temp_dir().join(format!("fs80h-domain-raw-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let raw_path = tmp_dir.join("test_roundtrip.raw");

        frame.write_raw_bytes(&raw_path).expect("raw write");
        let decoded = RawFrame::from_raw_bytes_file(&raw_path).expect("raw read");

        assert_eq!(frame.pixels(), decoded.pixels());
        assert_eq!(frame.sha256(), decoded.sha256());

        let _ = std::fs::remove_file(&raw_path);
        let _ = std::fs::remove_dir(&tmp_dir);
    }

    #[test]
    fn test_raw_frame_from_file_auto_detect() {
        let frame = RawFrame::new(vec![42u8; FRAME_LEN]).unwrap();
        let tmp_dir =
            std::env::temp_dir().join(format!("fs80h-detect-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);

        let p_png = tmp_dir.join("sample.png");
        let p_tiff = tmp_dir.join("sample.tiff");
        let p_pgm = tmp_dir.join("sample.pgm");
        let p_raw = tmp_dir.join("sample.raw");

        frame.write_png(&p_png).unwrap();
        frame.write_tiff(&p_tiff).unwrap();
        frame.write_pgm(&p_pgm).unwrap();
        frame.write_raw_bytes(&p_raw).unwrap();

        assert_eq!(
            RawFrame::from_file(&p_png).unwrap().sha256(),
            frame.sha256()
        );
        assert_eq!(
            RawFrame::from_file(&p_tiff).unwrap().sha256(),
            frame.sha256()
        );
        assert_eq!(
            RawFrame::from_file(&p_pgm).unwrap().sha256(),
            frame.sha256()
        );
        assert_eq!(
            RawFrame::from_file(&p_raw).unwrap().sha256(),
            frame.sha256()
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_extended_codecs_round_trip() {
        let pixels = (0..FRAME_LEN)
            .map(|index| ((index / WIDTH as usize + index % WIDTH as usize) % 256) as u8)
            .collect();
        let frame = RawFrame::new(pixels).unwrap();
        let tmp_dir = tempfile::tempdir().unwrap();

        let bmp = tmp_dir.path().join("sample.bmp");
        frame.write_bmp(&bmp).unwrap();
        assert_eq!(RawFrame::from_file(&bmp).unwrap().sha256(), frame.sha256());

        let jp2 = tmp_dir.path().join("sample.jp2");
        frame.write_jpeg2000(&jp2).unwrap();
        assert_eq!(RawFrame::from_file(&jp2).unwrap().sha256(), frame.sha256());

        let jpeg = tmp_dir.path().join("sample.jpeg");
        frame.write_jpeg(&jpeg).unwrap();
        let decoded = RawFrame::from_file(&jpeg).unwrap();
        assert_eq!(decoded.pixels().len(), FRAME_LEN);
        let mae = frame
            .pixels()
            .iter()
            .zip(decoded.pixels())
            .map(|(&left, &right)| left.abs_diff(right) as u64)
            .sum::<u64>() as f64
            / FRAME_LEN as f64;
        assert!(mae < 3.0, "JPEG mean absolute error was {mae}");
    }

    #[test]
    fn test_capture_manifest_fields() {
        let frame = RawFrame::new(vec![100u8; FRAME_LEN]).unwrap();
        let quality = crate::quality::evaluate_quality(&frame);
        let manifest = CaptureManifest::for_frame_with_afis_and_format(
            &frame,
            quality,
            "0.1.0",
            None,
            None,
            true,
            ImageFormat::Tiff,
        );

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.width, WIDTH);
        assert_eq!(manifest.height, HEIGHT);
        assert_eq!(manifest.dpi, DPI);
        assert_eq!(manifest.raw_frame_bytes, FRAME_LEN);
        assert_eq!(manifest.sha256_raw_frame, frame.sha256());
        assert_eq!(manifest.format, ImageFormat::Tiff);
    }

    #[test]
    fn test_raw_frame_png_bytes_roundtrip() {
        let frame = RawFrame::new(vec![128u8; FRAME_LEN]).unwrap();
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, WIDTH, HEIGHT);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(frame.pixels()).unwrap();
        }
        let recovered = RawFrame::from_png_bytes(&png_bytes).unwrap();
        assert_eq!(recovered.sha256(), frame.sha256());
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_arbitrary_pgm_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let _ = RawFrame::from_pgm_bytes(&bytes);
        }

        #[test]
        fn test_arbitrary_png_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let _ = RawFrame::from_png_bytes(&bytes);
        }

        #[test]
        fn test_arbitrary_quality_score_never_panic(score in any::<u8>()) {
            let res = QualityScore::new(score);
            if score <= 100 {
                assert!(res.is_ok());
                assert_eq!(res.unwrap().as_u8(), score);
            } else {
                assert!(res.is_err());
            }
        }

        #[test]
        fn test_arbitrary_match_score_never_panic(score in any::<f32>()) {
            let res = MatchScore::new(score);
            if (0.0..=100.0).contains(&score) {
                assert!(res.is_ok());
            } else {
                assert!(res.is_err());
            }
        }

        #[test]
        fn test_arbitrary_grayscale_image_never_panic(
            w in 0..100usize,
            h in 0..100usize,
            pixels in proptest::collection::vec(any::<u8>(), 0..500)
        ) {
            let _ = GrayscaleImage::new(w, h, pixels);
        }
    }
}
