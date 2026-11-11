//! Finger-minutiae record (FMR) interchange codecs.
//!
//! This module intentionally separates the normalized record model from image
//! processing.  It implements the common six-byte minutia representation used
//! by the 2005/2004 family and the 2011/2009 family.  Validation is structural;
//! it does not claim AFIS performance or certification.

use crate::{DPI, HEIGHT, MinutiaKind, RawFrame, WIDTH, extract_skeleton_minutiae};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAGIC: &[u8; 4] = b"FMR\0";
const HEADER_LEN: usize = 22;
const MAX_FINGERS: usize = 255;
const MAX_MINUTIAE: usize = 255;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateFormat {
    Iso19794_2_2005,
    Iso19794_2_2011,
    Ansi378_2004,
    Ansi378_2009,
    Ansi378_2009Am1,
}

impl TemplateFormat {
    pub const fn version(self) -> [u8; 4] {
        match self {
            Self::Iso19794_2_2005 | Self::Ansi378_2004 => *b" 20\0",
            _ => *b"030\0",
        }
    }
    pub const fn is_legacy(self) -> bool {
        matches!(self, Self::Iso19794_2_2005 | Self::Ansi378_2004)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum FingerPosition {
    Unknown = 0,
    RightThumb = 1,
    RightIndex = 2,
    RightMiddle = 3,
    RightRing = 4,
    RightLittle = 5,
    LeftThumb = 6,
    LeftIndex = 7,
    LeftMiddle = 8,
    LeftRing = 9,
    LeftLittle = 10,
    RightFour = 13,
    LeftFour = 14,
    Thumbs = 15,
    RightIndexMiddle = 40,
    RightMiddleRing = 41,
    RightRingLittle = 42,
    LeftIndexMiddle = 43,
    LeftMiddleRing = 44,
    LeftRingLittle = 45,
    BothIndexes = 46,
    RightIndexMiddleRing = 47,
    RightMiddleRingLittle = 48,
    LeftIndexMiddleRing = 49,
    LeftMiddleRingLittle = 50,
}

impl TryFrom<u8> for FingerPosition {
    type Error = TemplateError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::RightThumb),
            2 => Ok(Self::RightIndex),
            3 => Ok(Self::RightMiddle),
            4 => Ok(Self::RightRing),
            5 => Ok(Self::RightLittle),
            6 => Ok(Self::LeftThumb),
            7 => Ok(Self::LeftIndex),
            8 => Ok(Self::LeftMiddle),
            9 => Ok(Self::LeftRing),
            10 => Ok(Self::LeftLittle),
            13 => Ok(Self::RightFour),
            14 => Ok(Self::LeftFour),
            15 => Ok(Self::Thumbs),
            40 => Ok(Self::RightIndexMiddle),
            41 => Ok(Self::RightMiddleRing),
            42 => Ok(Self::RightRingLittle),
            43 => Ok(Self::LeftIndexMiddle),
            44 => Ok(Self::LeftMiddleRing),
            45 => Ok(Self::LeftRingLittle),
            46 => Ok(Self::BothIndexes),
            47 => Ok(Self::RightIndexMiddleRing),
            48 => Ok(Self::RightMiddleRingLittle),
            49 => Ok(Self::LeftIndexMiddleRing),
            50 => Ok(Self::LeftMiddleRingLittle),
            _ => Err(TemplateError::InvalidField("finger position")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ImpressionType {
    LivePlain = 0,
    LiveRolled = 1,
    NonLivePlain = 2,
    NonLiveRolled = 3,
    LiveSwipe = 8,
    Unknown = 29,
}
impl TryFrom<u8> for ImpressionType {
    type Error = TemplateError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::LivePlain),
            1 => Ok(Self::LiveRolled),
            2 => Ok(Self::NonLivePlain),
            3 => Ok(Self::NonLiveRolled),
            8 => Ok(Self::LiveSwipe),
            29 => Ok(Self::Unknown),
            _ => Err(TemplateError::InvalidField("impression type")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateMinutiaKind {
    RidgeEnding,
    Bifurcation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateMinutia {
    pub x: u16,
    pub y: u16,
    pub angle: u8,
    pub quality: u8,
    pub kind: TemplateMinutiaKind,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SingularPoint {
    Core { x: u16, y: u16, angle: Option<u8> },
    Delta { x: u16, y: u16, angle: Option<u8> },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FingerView {
    pub position: FingerPosition,
    pub view: u8,
    pub impression: ImpressionType,
    pub quality: u8,
    pub width: u16,
    pub height: u16,
    pub x_resolution: u16,
    pub y_resolution: u16,
    pub minutiae: Vec<TemplateMinutia>,
    pub singular_points: Vec<SingularPoint>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    pub product_owner: u16,
    pub product_type: u16,
    pub device_id: u16,
    pub fingers: Vec<FingerView>,
}

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("invalid FMR template: {0}")]
    Invalid(&'static str),
    #[error("invalid template field: {0}")]
    InvalidField(&'static str),
    #[error("template length mismatch: declared {declared}, actual {actual}")]
    Length { declared: usize, actual: usize },
    #[error("unsupported FMR version")]
    Version,
    #[error("base64: {0}")]
    Base64(#[from] base64::DecodeError),
}

impl Template {
    pub fn from_frame(
        frame: &RawFrame,
        position: FingerPosition,
        impression: ImpressionType,
    ) -> Self {
        let minutiae = extract_skeleton_minutiae(frame.as_image())
            .into_iter()
            .filter_map(|m| {
                let x = m.x.round();
                let y = m.y.round();
                if !(0.0..=16383.0).contains(&x) || !(0.0..=65535.0).contains(&y) {
                    return None;
                }
                Some(TemplateMinutia {
                    x: x as u16,
                    y: y as u16,
                    angle: ((m.angle_deg / 2.0).round() as i32).rem_euclid(180) as u8,
                    quality: (m.quality * 100.0).round().clamp(0.0, 100.0) as u8,
                    kind: if m.kind == MinutiaKind::RidgeEnding {
                        TemplateMinutiaKind::RidgeEnding
                    } else {
                        TemplateMinutiaKind::Bifurcation
                    },
                })
            })
            .take(MAX_MINUTIAE)
            .collect();
        Self {
            product_owner: 0,
            product_type: 0,
            device_id: 0,
            fingers: vec![FingerView {
                position,
                view: 0,
                impression,
                quality: 0,
                width: WIDTH as u16,
                height: HEIGHT as u16,
                x_resolution: DPI,
                y_resolution: DPI,
                minutiae,
                singular_points: vec![],
            }],
        }
    }
    pub fn validate(&self, format: TemplateFormat) -> Result<(), TemplateError> {
        if self.fingers.is_empty() || self.fingers.len() > MAX_FINGERS {
            return Err(TemplateError::Invalid("finger count"));
        }
        for f in &self.fingers {
            if !f.singular_points.is_empty() {
                return Err(TemplateError::Invalid(
                    "singular-point extensions are not yet serializable",
                ));
            }
            if f.width == 0
                || f.height == 0
                || f.x_resolution == 0
                || f.y_resolution == 0
                || f.minutiae.len() > MAX_MINUTIAE
            {
                return Err(TemplateError::Invalid("finger view"));
            }
            if format.is_legacy() && (f.position as u8) > 10 {
                return Err(TemplateError::InvalidField(
                    "multi-finger position is not supported by legacy formats",
                ));
            }
            for m in &f.minutiae {
                if m.x >= 16384 || m.x >= f.width || m.y >= f.height || m.angle > 179 {
                    return Err(TemplateError::Invalid("minutia"));
                }
            }
        }
        Ok(())
    }
    pub fn to_bytes(&self, format: TemplateFormat) -> Result<Vec<u8>, TemplateError> {
        self.validate(format)?;
        let body_len: usize = self.fingers.iter().map(|f| 14 + f.minutiae.len() * 6).sum();
        let total = HEADER_LEN + body_len;
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&format.version());
        out.extend_from_slice(&(total as u32).to_be_bytes());
        out.extend_from_slice(&self.product_owner.to_be_bytes());
        out.extend_from_slice(&self.product_type.to_be_bytes());
        out.extend_from_slice(&self.device_id.to_be_bytes());
        out.extend_from_slice(&[0, 0]);
        out.push(self.fingers.len() as u8);
        out.push(0);
        for f in &self.fingers {
            out.extend_from_slice(&[f.position as u8, f.view, f.impression as u8, f.quality]);
            out.extend_from_slice(&f.width.to_be_bytes());
            out.extend_from_slice(&f.height.to_be_bytes());
            out.extend_from_slice(&f.x_resolution.to_be_bytes());
            out.extend_from_slice(&f.y_resolution.to_be_bytes());
            out.push(f.minutiae.len() as u8);
            out.push(0);
            for m in &f.minutiae {
                let t = if m.kind == TemplateMinutiaKind::RidgeEnding {
                    1u16
                } else {
                    2u16
                };
                out.extend_from_slice(&(t << 14 | m.x).to_be_bytes());
                out.extend_from_slice(&m.y.to_be_bytes());
                out.push(m.angle);
                out.push(m.quality);
            }
        }
        Ok(out)
    }
    pub fn parse(format: TemplateFormat, bytes: &[u8]) -> Result<Self, TemplateError> {
        if bytes.len() < HEADER_LEN || &bytes[..4] != MAGIC {
            return Err(TemplateError::Invalid("magic/header"));
        }
        if bytes[4..8] != format.version() {
            return Err(TemplateError::Version);
        }
        let declared = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
        if declared != bytes.len() {
            return Err(TemplateError::Length {
                declared,
                actual: bytes.len(),
            });
        }
        let mut p = 12;
        let u16at = |b: &[u8], i: usize| u16::from_be_bytes([b[i], b[i + 1]]);
        let product_owner = u16at(bytes, p);
        p += 2;
        let product_type = u16at(bytes, p);
        p += 2;
        let device_id = u16at(bytes, p);
        p += 2;
        p += 2;
        let count = bytes[p] as usize;
        p += 2;
        if count == 0 {
            return Err(TemplateError::Invalid("finger count"));
        }
        let mut fingers = Vec::with_capacity(count);
        for _ in 0..count {
            if p + 12 > bytes.len() {
                return Err(TemplateError::Invalid("truncated finger view"));
            }
            let position = FingerPosition::try_from(bytes[p])?;
            let view = bytes[p + 1];
            let impression = ImpressionType::try_from(bytes[p + 2])?;
            let quality = bytes[p + 3];
            let width = u16at(bytes, p + 4);
            let height = u16at(bytes, p + 6);
            let xr = u16at(bytes, p + 8);
            let yr = u16at(bytes, p + 10);
            let n = bytes[p + 12] as usize;
            p += 14;
            if p + n * 6 > bytes.len() {
                return Err(TemplateError::Invalid("truncated minutiae"));
            }
            let mut minutiae = Vec::with_capacity(n);
            for _ in 0..n {
                let packed = u16at(bytes, p);
                let kind = match packed >> 14 {
                    1 => TemplateMinutiaKind::RidgeEnding,
                    2 => TemplateMinutiaKind::Bifurcation,
                    _ => return Err(TemplateError::Invalid("minutia type")),
                };
                minutiae.push(TemplateMinutia {
                    kind,
                    x: packed & 0x3fff,
                    y: u16at(bytes, p + 2),
                    angle: bytes[p + 4],
                    quality: bytes[p + 5],
                });
                p += 6;
            }
            fingers.push(FingerView {
                position,
                view,
                impression,
                quality,
                width,
                height,
                x_resolution: xr,
                y_resolution: yr,
                minutiae,
                singular_points: vec![],
            });
        }
        if p != bytes.len() {
            return Err(TemplateError::Invalid(
                "trailing bytes or unsupported extension",
            ));
        }
        let result = Self {
            product_owner,
            product_type,
            device_id,
            fingers,
        };
        result.validate(format)?;
        Ok(result)
    }
    pub fn to_base64(&self, format: TemplateFormat) -> Result<String, TemplateError> {
        Ok(STANDARD.encode(self.to_bytes(format)?))
    }
    pub fn from_base64(format: TemplateFormat, text: &str) -> Result<Self, TemplateError> {
        Self::parse(format, &STANDARD.decode(text)?)
    }
}
