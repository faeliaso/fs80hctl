use crate::domain::{HEIGHT, QualityScore, RawFrame, WIDTH};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

const BLOCK_SIZE: usize = 16;
const MIN_ACTIVE_BLOCK_VARIANCE: f64 = 45.0;
const MIN_MEAN_VARIANCE: f64 = 80.0;
const MIN_USABLE_AREA_RATIO: f64 = 0.30;
const MIN_RIDGE_CONTRAST: f64 = 25.0;
const MIN_SHARPNESS: f64 = 12.0;
const MAX_SATURATED_RATIO: f64 = 0.45;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QualityReport {
    pub score: QualityScore,
    pub accepted: bool,
    pub mean_local_variance: f64,
    pub saturated_ratio: f64,
    #[serde(default)]
    pub usable_area_ratio: f64,
    #[serde(default)]
    pub ridge_contrast: f64,
    #[serde(default)]
    pub sharpness: f64,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
struct BlockMetrics {
    variance: f64,
    contrast: f64,
    sharpness: f64,
    active: bool,
}

fn evaluate_block(pixels: &[u8], bx: usize, by: usize) -> BlockMetrics {
    let w = WIDTH as usize;
    let h = HEIGHT as usize;
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut min_p = 255u8;
    let mut max_p = 0u8;
    let mut grad_sum = 0.0f64;
    let mut grad_count = 0usize;
    let mut sat_count = 0usize;
    let mut n = 0usize;

    for y in by..(by + BLOCK_SIZE).min(h) {
        let row_start = y * w;
        for x in bx..(bx + BLOCK_SIZE).min(w) {
            let p = pixels[row_start + x];
            let p_f = p as f64;
            sum += p_f;
            sum_sq += p_f * p_f;
            min_p = min_p.min(p);
            max_p = max_p.max(p);
            if p <= 4 || p >= 251 {
                sat_count += 1;
            }
            if x > 0 && x + 1 < w && y > 0 && y + 1 < h {
                let gx = pixels[row_start + x + 1] as f64 - pixels[row_start + x - 1] as f64;
                let gy = pixels[(y + 1) * w + x] as f64 - pixels[(y - 1) * w + x] as f64;
                grad_sum += gx.hypot(gy);
                grad_count += 1;
            }
            n += 1;
        }
    }

    if n == 0 {
        return BlockMetrics {
            variance: 0.0,
            contrast: 0.0,
            sharpness: 0.0,
            active: false,
        };
    }

    let n_f = n as f64;
    let variance = (sum_sq / n_f - (sum / n_f).powi(2)).max(0.0);
    let contrast = (max_p.saturating_sub(min_p)) as f64;
    let sharpness = if grad_count > 0 {
        grad_sum / grad_count as f64
    } else {
        0.0
    };
    let saturated_ratio = sat_count as f64 / n_f;
    let active = variance >= MIN_ACTIVE_BLOCK_VARIANCE && saturated_ratio < 0.80;

    BlockMetrics {
        variance,
        contrast,
        sharpness,
        active,
    }
}

pub fn evaluate_quality(frame: &RawFrame) -> QualityReport {
    let pixels = frame.pixels();
    let block_row_count = (HEIGHT as usize) / BLOCK_SIZE;
    let block_col_count = (WIDTH as usize) / BLOCK_SIZE;

    #[cfg(feature = "parallel")]
    let metrics: Vec<BlockMetrics> = (0..block_row_count)
        .into_par_iter()
        .flat_map_iter(|by_idx| {
            let by = by_idx * BLOCK_SIZE;
            (0..block_col_count).map(move |bx_idx| evaluate_block(pixels, bx_idx * BLOCK_SIZE, by))
        })
        .collect();

    #[cfg(not(feature = "parallel"))]
    let metrics: Vec<BlockMetrics> = (0..block_row_count)
        .flat_map(|by_idx| {
            let by = by_idx * BLOCK_SIZE;
            (0..block_col_count).map(move |bx_idx| evaluate_block(pixels, bx_idx * BLOCK_SIZE, by))
        })
        .collect();

    let total_blocks = metrics.len().max(1);
    let variance = metrics.iter().map(|m| m.variance).sum::<f64>() / total_blocks as f64;

    let active_blocks: Vec<&BlockMetrics> = metrics.iter().filter(|m| m.active).collect();
    let usable_area_ratio = active_blocks.len() as f64 / total_blocks as f64;

    let (ridge_contrast, sharpness) = if active_blocks.is_empty() {
        let c = metrics.iter().map(|m| m.contrast).sum::<f64>() / total_blocks as f64;
        let s = metrics.iter().map(|m| m.sharpness).sum::<f64>() / total_blocks as f64;
        (c, s)
    } else {
        let c = active_blocks.iter().map(|m| m.contrast).sum::<f64>() / active_blocks.len() as f64;
        let s = active_blocks.iter().map(|m| m.sharpness).sum::<f64>() / active_blocks.len() as f64;
        (c, s)
    };

    #[cfg(feature = "parallel")]
    let saturated_count = pixels.par_iter().filter(|&&p| p <= 4 || p >= 251).count();

    #[cfg(not(feature = "parallel"))]
    let saturated_count = pixels.iter().filter(|&&p| p <= 4 || p >= 251).count();

    let saturated = saturated_count as f64 / pixels.len().max(1) as f64;

    let contrast_score = (variance / 500.0 * 100.0).clamp(0.0, 100.0);
    let saturation_score = ((1.0 - saturated / 0.55) * 100.0).clamp(0.0, 100.0);
    let area_score = (usable_area_ratio / 0.70 * 100.0).clamp(0.0, 100.0);
    let sharpness_score = (sharpness / 40.0 * 100.0).clamp(0.0, 100.0);

    let score_val = (contrast_score * 0.45
        + saturation_score * 0.25
        + area_score * 0.15
        + sharpness_score * 0.15)
        .round() as u8;
    let score = QualityScore::new(score_val).unwrap_or_else(|_| QualityScore::new(0).unwrap());

    let mut warnings = Vec::new();
    if variance < MIN_MEAN_VARIANCE || ridge_contrast < MIN_RIDGE_CONTRAST {
        warnings
            .push("low local contrast or dry ridge pattern; recapture with better contact".into());
    }
    if saturated > MAX_SATURATED_RATIO {
        warnings.push(
            "excessive saturation; image possibly too wet, washed out, or uneven contact".into(),
        );
    }
    if usable_area_ratio < MIN_USABLE_AREA_RATIO {
        warnings.push("partial finger contact area; center finger firmly and recapture".into());
    }
    if !active_blocks.is_empty() && sharpness < MIN_SHARPNESS {
        warnings.push("blurry ridge edges detected; hold finger steady and recapture".into());
    }

    let accepted = variance >= MIN_MEAN_VARIANCE
        && ridge_contrast >= MIN_RIDGE_CONTRAST
        && saturated <= MAX_SATURATED_RATIO
        && usable_area_ratio >= MIN_USABLE_AREA_RATIO
        && sharpness >= MIN_SHARPNESS;

    QualityReport {
        score,
        accepted,
        mean_local_variance: variance,
        saturated_ratio: saturated,
        usable_area_ratio,
        ridge_contrast,
        sharpness,
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
    let is_present = quality.mean_local_variance >= 40.0
        && quality.saturated_ratio < 0.40
        && quality.usable_area_ratio >= 0.15;
    let contact_ratio = ((quality.mean_local_variance / 200.0).min(1.0) * 0.6
        + quality.usable_area_ratio * 0.4)
        .clamp(0.0, 1.0);
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
        assert_eq!(q.usable_area_ratio, 0.0);
        assert!(q.warnings.iter().any(|w| w.contains("low local contrast")));
        assert!(
            q.warnings
                .iter()
                .any(|w| w.contains("excessive saturation"))
        );
        assert!(
            q.warnings
                .iter()
                .any(|w| w.contains("partial finger contact"))
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
        assert!(q.usable_area_ratio > 0.9);
        assert!(q.warnings.is_empty());
    }

    #[test]
    fn test_quality_rejects_partial_and_blurry_frames() {
        // Partial frame: only a tiny 48x48 patch has ridges, rest is flat white
        let mut partial_pixels = vec![240u8; FRAME_LEN];
        for y in 16..64 {
            for x in 16..64 {
                partial_pixels[y * WIDTH as usize + x] =
                    if (x / 4).is_multiple_of(2) { 40 } else { 200 };
            }
        }
        let partial = RawFrame::new(partial_pixels).unwrap();
        let q_partial = evaluate_quality(&partial);
        assert!(!q_partial.accepted);
        assert!(q_partial.usable_area_ratio < 0.30);

        // Blurry frame: very smooth low-amplitude gradient
        let mut blurry_pixels = vec![128u8; FRAME_LEN];
        for y in 0..HEIGHT as usize {
            for x in 0..WIDTH as usize {
                let val = 128.0 + 18.0 * (x as f64 / 18.0).sin();
                blurry_pixels[y * WIDTH as usize + x] = val as u8;
            }
        }
        let blurry = RawFrame::new(blurry_pixels).unwrap();
        let q_blurry = evaluate_quality(&blurry);
        assert!(!q_blurry.accepted);
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
