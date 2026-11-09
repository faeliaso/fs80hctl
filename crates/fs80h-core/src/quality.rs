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
        warnings.push("baixo contraste local; recapture com melhor contato".into());
    }
    if saturated > 0.20 {
        warnings.push(
            "saturação excessiva; imagem possivelmente estourada ou sem contato uniforme".into(),
        );
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
        assert!(q.warnings.iter().any(|w| w.contains("baixo contraste")));
        assert!(q.warnings.iter().any(|w| w.contains("saturação excessiva")));
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
}
