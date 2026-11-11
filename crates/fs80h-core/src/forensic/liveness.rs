use crate::domain::{HEIGHT, RawFrame, WIDTH};
use rustfft::{FftPlanner, num_complex::Complex32};
use serde::{Deserialize, Serialize};

const WINDOW: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LivenessClassification {
    GenuineLiveFinger,
    SpoofDetected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LivenessReport {
    pub liveness_score: u8,
    pub classification: LivenessClassification,
    /// Proportion of non-DC spectral energy in the expected 6–12 px ridge band.
    pub fft_ridge_band_energy: f32,
    /// Gradient orientation concentration in textured image regions.
    pub directional_coherence: f32,
    /// Combined spectral/directional indicator retained for the forensic report.
    pub fft_energy_coherence: f32,
    pub windows_analyzed: usize,
    pub methodology: &'static str,
}

fn fft_power_band(window: &[u8]) -> (f32, f32) {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(WINDOW);
    let mut values: Vec<Complex32> = window
        .iter()
        .map(|&value| Complex32::new(value as f32 - 127.5, 0.0))
        .collect();
    let (rows, _) = values.as_chunks_mut::<WINDOW>();
    for row in rows {
        fft.process(row);
    }
    let mut column = vec![Complex32::new(0.0, 0.0); WINDOW];
    for x in 0..WINDOW {
        for y in 0..WINDOW {
            column[y] = values[y * WINDOW + x];
        }
        fft.process(&mut column);
        for y in 0..WINDOW {
            values[y * WINDOW + x] = column[y];
        }
    }
    let mut total = 0.0;
    let mut biological_band = 0.0;
    for y in 0..WINDOW {
        let fy = y.min(WINDOW - y) as f32;
        for x in 0..WINDOW {
            let fx = x.min(WINDOW - x) as f32;
            let radius = (fx * fx + fy * fy).sqrt();
            if radius < 1.0 {
                continue;
            }
            let energy = values[y * WINDOW + x].norm_sqr();
            total += energy;
            // 128 / period: 6–12 px corresponds to radial FFT bins 10.7–21.3.
            if (WINDOW as f32 / 12.0..=WINDOW as f32 / 6.0).contains(&radius) {
                biological_band += energy;
            }
        }
    }
    (biological_band, total)
}

fn directional_coherence(frame: &RawFrame) -> f32 {
    let pixels = frame.pixels();
    let mut sum_cos = 0.0f32;
    let mut sum_sin = 0.0f32;
    let mut sum_weight = 0.0f32;
    for y in 1..HEIGHT as usize - 1 {
        for x in 1..WIDTH as usize - 1 {
            let at = |xx: usize, yy: usize| pixels[yy * WIDTH as usize + xx] as f32;
            let gx = at(x + 1, y) - at(x - 1, y);
            let gy = at(x, y + 1) - at(x, y - 1);
            let weight = (gx * gx + gy * gy).sqrt();
            if weight < 12.0 {
                continue;
            }
            let theta2 = 2.0 * gy.atan2(gx);
            sum_cos += weight * theta2.cos();
            sum_sin += weight * theta2.sin();
            sum_weight += weight;
        }
    }
    if sum_weight == 0.0 {
        0.0
    } else {
        (sum_cos.hypot(sum_sin) / sum_weight).clamp(0.0, 1.0)
    }
}

/// Runs a deterministic, static texture PAD heuristic. It is a screening aid,
/// not a substitute for ISO/IEC 30107-3 laboratory validation on a sensor.
pub fn analyze_liveness(frame: &RawFrame) -> LivenessReport {
    let mut band_energy = 0.0;
    let mut total_energy = 0.0;
    let mut windows = 0;
    for top in (0..=HEIGHT as usize - WINDOW).step_by(WINDOW) {
        for left in (0..=WIDTH as usize - WINDOW).step_by(WINDOW) {
            let mut window = Vec::with_capacity(WINDOW * WINDOW);
            for y in top..top + WINDOW {
                window.extend_from_slice(
                    &frame.pixels()[y * WIDTH as usize + left..y * WIDTH as usize + left + WINDOW],
                );
            }
            let mean = window.iter().map(|&v| v as f32).sum::<f32>() / window.len() as f32;
            let variance = window
                .iter()
                .map(|&v| (v as f32 - mean).powi(2))
                .sum::<f32>()
                / window.len() as f32;
            if variance < 25.0 {
                continue;
            }
            let (band, total) = fft_power_band(&window);
            band_energy += band;
            total_energy += total;
            windows += 1;
        }
    }
    let ridge_band_energy = if total_energy == 0.0 {
        0.0
    } else {
        (band_energy / total_energy).clamp(0.0, 1.0)
    };
    let coherence = directional_coherence(frame);
    let combined = (ridge_band_energy * 0.60 + coherence * 0.40).clamp(0.0, 1.0);
    let score = (combined * 100.0).round() as u8;
    let classification = if score < 65 {
        LivenessClassification::SpoofDetected
    } else {
        LivenessClassification::GenuineLiveFinger
    };
    LivenessReport {
        liveness_score: score,
        classification,
        fft_ridge_band_energy: ridge_band_energy,
        directional_coherence: coherence,
        fft_energy_coherence: combined,
        windows_analyzed: windows,
        methodology: "static FFT ridge-band and gradient-direction coherence screening; requires sensor-specific PAD validation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::FRAME_LEN;

    #[test]
    fn uniform_frame_is_deterministically_rejected() {
        let frame = RawFrame::new(vec![127; FRAME_LEN]).unwrap();
        let first = analyze_liveness(&frame);
        let second = analyze_liveness(&frame);
        assert_eq!(first.liveness_score, second.liveness_score);
        assert_eq!(first.classification, LivenessClassification::SpoofDetected);
    }
}
