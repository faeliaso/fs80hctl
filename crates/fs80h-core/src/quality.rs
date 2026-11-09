use crate::domain::{HEIGHT, RawFrame, WIDTH};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QualityReport {
    pub score: u8,
    pub accepted: bool,
    pub mean_local_variance: f64,
    pub saturated_ratio: f64,
    pub warnings: Vec<String>,
}

pub fn evaluate_quality(frame: &RawFrame) -> QualityReport {
    let mut variance_sum = 0.0;
    let mut blocks = 0usize;
    for by in (0..HEIGHT as usize).step_by(16) {
        for bx in (0..WIDTH as usize).step_by(16) {
            let mut sum = 0.0;
            let mut sum_sq = 0.0;
            let mut n = 0.0;
            for y in by..(by + 16).min(HEIGHT as usize) {
                for x in bx..(bx + 16).min(WIDTH as usize) {
                    let p = frame.pixels()[y * WIDTH as usize + x] as f64;
                    sum += p;
                    sum_sq += p * p;
                    n += 1.0;
                }
            }
            variance_sum += (sum_sq / n - (sum / n).powi(2)).max(0.0);
            blocks += 1;
        }
    }
    let variance = variance_sum / blocks as f64;
    let saturated = frame
        .pixels()
        .iter()
        .filter(|&&p| p <= 4 || p >= 251)
        .count() as f64
        / frame.pixels().len() as f64;
    let contrast_score = (variance / 500.0 * 100.0).clamp(0.0, 100.0);
    let saturation_score = ((1.0 - saturated / 0.35) * 100.0).clamp(0.0, 100.0);
    let score = (contrast_score * 0.7 + saturation_score * 0.3).round() as u8;
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
