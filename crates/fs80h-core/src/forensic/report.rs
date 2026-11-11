use crate::{
    DPI, RawFrame, analyze_liveness, evaluate_quality, extract_minutiae, matcher::MinutiaKind,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForensicReport {
    pub forensic_report_version: &'static str,
    pub timestamp_utc: DateTime<Utc>,
    pub target_image: TargetImage,
    pub quality_metrics: QualityMetrics,
    pub liveness_analysis: crate::LivenessReport,
    pub minutiae_summary: MinutiaeSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetImage {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub dpi: u16,
    pub hashes: ImageHashes,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageHashes {
    pub sha256: String,
    pub blake3: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QualityMetrics {
    pub overall_score: u8,
    pub nfiq2_compatible_score: u8,
    pub contact_area_percentage: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MinutiaeSummary {
    pub total_extracted: usize,
    pub ridge_endings: usize,
    pub bifurcations: usize,
}

pub fn create_forensic_report(path: &Path, frame: &RawFrame) -> ForensicReport {
    let quality = evaluate_quality(frame);
    let minutiae = extract_minutiae(frame);
    let ridge_endings = minutiae
        .iter()
        .filter(|m| m.kind == MinutiaKind::RidgeEnding)
        .count();
    let (rows, _) = frame.pixels().as_chunks::<{ 16 * 320 }>();
    let contact_blocks = rows
        .iter()
        .filter(|rows| {
            let mean = rows.iter().map(|&v| v as f32).sum::<f32>() / rows.len() as f32;
            rows.iter().map(|&v| (v as f32 - mean).powi(2)).sum::<f32>() / rows.len() as f32 >= 80.0
        })
        .count();
    ForensicReport {
        forensic_report_version: "1.0",
        timestamp_utc: Utc::now(),
        target_image: TargetImage {
            path: path.display().to_string(),
            width: crate::WIDTH,
            height: crate::HEIGHT,
            dpi: DPI,
            hashes: ImageHashes {
                sha256: frame.sha256(),
                blake3: blake3::hash(frame.pixels()).to_hex().to_string(),
            },
        },
        quality_metrics: QualityMetrics {
            overall_score: quality.score.as_u8(),
            nfiq2_compatible_score: quality.score.as_u8(),
            contact_area_percentage: contact_blocks as f32 / 30.0 * 100.0,
        },
        liveness_analysis: analyze_liveness(frame),
        minutiae_summary: MinutiaeSummary {
            total_extracted: minutiae.len(),
            ridge_endings,
            bifurcations: minutiae.len() - ridge_endings,
        },
    }
}
