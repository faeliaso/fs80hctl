use crate::domain::{HEIGHT, QualityScore, RawFrame, WIDTH};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QualityReport {
    pub score: QualityScore,
    pub accepted: bool,
    pub mean_local_variance: f64,
    pub saturated_ratio: f64,
    pub warnings: Vec<String>,
}

pub fn evaluate_quality(frame: &RawFrame) -> QualityReport {
    let pixels = frame.pixels();
    let block_row_count = (HEIGHT as usize) / 16;
    let block_col_count = (WIDTH as usize) / 16;

    #[cfg(feature = "parallel")]
    let (variance_sum, blocks) = (0..block_row_count)
        .into_par_iter()
        .flat_map_iter(|by_idx| {
            let by = by_idx * 16;
            (0..block_col_count).map(move |bx_idx| {
                let bx = bx_idx * 16;
                let mut sum = 0.0f64;
                let mut sum_sq = 0.0f64;
                let n = 256.0f64;
                for y in by..by + 16 {
                    let row_start = y * (WIDTH as usize) + bx;
                    if let Some(row) = pixels.get(row_start..row_start + 16) {
                        for &p in row {
                            let p_f = p as f64;
                            sum += p_f;
                            sum_sq += p_f * p_f;
                        }
                    }
                }
                (sum_sq / n - (sum / n).powi(2)).max(0.0)
            })
        })
        .fold(|| (0.0f64, 0usize), |(s, c), v| (s + v, c + 1))
        .reduce(|| (0.0f64, 0usize), |(s1, c1), (s2, c2)| (s1 + s2, c1 + c2));

    #[cfg(not(feature = "parallel"))]
    let (variance_sum, blocks) = {
        let mut variance_sum = 0.0f64;
        let mut blocks = 0usize;
        for by_idx in 0..block_row_count {
            let by = by_idx * 16;
            for bx_idx in 0..block_col_count {
                let bx = bx_idx * 16;
                let mut sum = 0.0f64;
                let mut sum_sq = 0.0f64;
                let n = 256.0f64;
                for y in by..by + 16 {
                    let row_start = y * (WIDTH as usize) + bx;
                    if let Some(row) = pixels.get(row_start..row_start + 16) {
                        for &p in row {
                            let p_f = p as f64;
                            sum += p_f;
                            sum_sq += p_f * p_f;
                        }
                    }
                }
                variance_sum += (sum_sq / n - (sum / n).powi(2)).max(0.0);
                blocks += 1;
            }
        }
        (variance_sum, blocks)
    };

    let variance = if blocks == 0 {
        0.0
    } else {
        variance_sum / blocks as f64
    };

    #[cfg(feature = "parallel")]
    let saturated_count = pixels.par_iter().filter(|&&p| p <= 4 || p >= 251).count();

    #[cfg(not(feature = "parallel"))]
    let saturated_count = pixels.iter().filter(|&&p| p <= 4 || p >= 251).count();

    let saturated = saturated_count as f64 / pixels.len() as f64;
    let contrast_score = (variance / 500.0 * 100.0).clamp(0.0, 100.0);
    let saturation_score = ((1.0 - saturated / 0.35) * 100.0).clamp(0.0, 100.0);
    let score_val = (contrast_score * 0.7 + saturation_score * 0.3).round() as u8;
    let score = QualityScore::new(score_val).unwrap_or(QualityScore::new(0).unwrap());
    let mut warnings = Vec::new();
    if variance < 80.0 {
        warnings.push("low local contrast; recapture with better contact".into());
    }
    if saturated > 0.20 {
        warnings.push("excessive saturation; image possibly washed out or uneven contact".into());
    }
    let accepted = variance >= 80.0 && saturated <= 0.20;
    QualityReport {
        score,
        accepted,
        mean_local_variance: variance,
        saturated_ratio: saturated,
        warnings,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FingerPresence {
    pub is_present: bool,
    pub contact_ratio: f64,
    pub quality: QualityReport,
}

pub fn evaluate_presence(frame: &RawFrame) -> FingerPresence {
    let quality = evaluate_quality(frame);
    // When no finger touches the prism, local variance is near 0 or saturation is extreme.
    // Contact is detected when local contrast is present and saturation is non-total.
    let is_present = quality.mean_local_variance >= 40.0 && quality.saturated_ratio < 0.40;
    let contact_ratio = (quality.mean_local_variance / 200.0).clamp(0.0, 1.0);
    FingerPresence {
        is_present,
        contact_ratio,
        quality,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::FRAME_LEN;

    #[test]
    fn test_quality_uniform_white_frame() {
        let frame = RawFrame::new(vec![255u8; FRAME_LEN]).unwrap();
        let q = evaluate_quality(&frame);
        assert!(!q.accepted);
        assert_eq!(q.mean_local_variance, 0.0);
        assert_eq!(q.saturated_ratio, 1.0);
        assert!(q.warnings.iter().any(|w| w.contains("low local contrast")));
        assert!(
            q.warnings
                .iter()
                .any(|w| w.contains("excessive saturation"))
        );
    }

    #[test]
    fn test_quality_uniform_black_frame() {
        let frame = RawFrame::new(vec![0u8; FRAME_LEN]).unwrap();
        let q = evaluate_quality(&frame);
        assert!(!q.accepted);
        assert_eq!(q.mean_local_variance, 0.0);
        assert_eq!(q.saturated_ratio, 1.0);
    }

    #[test]
    fn test_quality_alternating_ridge_pattern() {
        let mut pixels = vec![0u8; FRAME_LEN];
        for (i, p) in pixels.iter_mut().enumerate() {
            let col = i % (WIDTH as usize);
            *p = if (col / 4).is_multiple_of(2) { 40 } else { 200 };
        }
        let frame = RawFrame::new(pixels).unwrap();
        let q = evaluate_quality(&frame);
        assert!(q.accepted);
        assert!(q.score > 70);
        assert!(q.mean_local_variance >= 80.0);
        assert_eq!(q.saturated_ratio, 0.0);
        assert!(q.warnings.is_empty());
    }

    #[test]
    fn test_presence_detection() {
        let empty_frame = RawFrame::new(vec![255u8; FRAME_LEN]).unwrap();
        let p_empty = evaluate_presence(&empty_frame);
        assert!(!p_empty.is_present);
        assert_eq!(p_empty.contact_ratio, 0.0);

        let mut pixels = vec![0u8; FRAME_LEN];
        for (i, p) in pixels.iter_mut().enumerate() {
            let col = i % (WIDTH as usize);
            *p = if (col / 4).is_multiple_of(2) { 40 } else { 200 };
        }
        let ridge_frame = RawFrame::new(pixels).unwrap();
        let p_finger = evaluate_presence(&ridge_frame);
        assert!(p_finger.is_present);
        assert!(p_finger.contact_ratio > 0.5);
        assert!(p_finger.quality.accepted);
    }
}
