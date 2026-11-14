//! Contextual Gabor enhancement for 500-DPI fingerprint images.
//!
//! Pipeline:
//! 1. Block-aware intensity normalization (stabilizes dry/wet ridges without amplifying flat background).
//! 2. Sobel local orientation field with doubled-angle Gaussian smoothing.
//! 3. Adaptive local ridge frequency estimation via oriented x-signature projection.
//! 4. Contextual 9x9 Gabor filtering (100% pure Rust, deterministic, optional `rayon`).

use crate::{GrayscaleImage, RawFrame};
#[cfg(feature = "parallel")]
use rayon::prelude::*;

const BLOCK: usize = 16;
const RADIUS: i32 = 5;
const ORIENTATION_BINS: usize = 36;
const DEFAULT_RIDGE_FREQUENCY: f32 = 1.0 / 12.0;
const MIN_RIDGE_FREQUENCY: f32 = 1.0 / 20.0;
const MAX_RIDGE_FREQUENCY: f32 = 1.0 / 5.0;
const BACKGROUND_VARIANCE_FLOOR: f32 = 25.0;

/// Normalizes local intensity in 16x16 blocks to stabilize dry and wet captures.
/// Flat background blocks (variance below `BACKGROUND_VARIANCE_FLOOR`) are kept neutral.
fn normalize_intensity(pixels: &[u8], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![128.0f32; w * h];
    let block_cols = w.div_ceil(BLOCK);
    let block_rows = h.div_ceil(BLOCK);
    const TARGET_STD: f32 = 48.0;

    for by in 0..block_rows {
        let y0 = by * BLOCK;
        let y1 = (y0 + BLOCK).min(h);
        for bx in 0..block_cols {
            let x0 = bx * BLOCK;
            let x1 = (x0 + BLOCK).min(w);
            let mut sum = 0.0f32;
            let mut sum_sq = 0.0f32;
            let mut count = 0.0f32;
            for y in y0..y1 {
                let row = y * w;
                for x in x0..x1 {
                    let p = pixels[row + x] as f32;
                    sum += p;
                    sum_sq += p * p;
                    count += 1.0;
                }
            }
            if count == 0.0 {
                continue;
            }
            let mean = sum / count;
            let var = (sum_sq / count - mean * mean).max(0.0);
            if var < BACKGROUND_VARIANCE_FLOOR {
                for y in y0..y1 {
                    let row = y * w;
                    for x in x0..x1 {
                        out[row + x] = 128.0;
                    }
                }
                continue;
            }
            let gain = (TARGET_STD / var.sqrt()).clamp(0.4, 3.0);
            for y in y0..y1 {
                let row = y * w;
                for x in x0..x1 {
                    let p = pixels[row + x] as f32;
                    out[row + x] = (128.0 + (p - mean) * gain).clamp(0.0, 255.0);
                }
            }
        }
    }
    out
}

/// Applies a gentle 3x3 Gaussian pre-smoothing to attenuate sweat pores before Gabor filtering.
fn gaussian_smooth_3x3(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut dst = src.to_vec();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let acc = src[(y - 1) * w + x - 1]
                + 2.0 * src[(y - 1) * w + x]
                + src[(y - 1) * w + x + 1]
                + 2.0 * src[y * w + x - 1]
                + 4.0 * src[y * w + x]
                + 2.0 * src[y * w + x + 1]
                + src[(y + 1) * w + x - 1]
                + 2.0 * src[(y + 1) * w + x]
                + src[(y + 1) * w + x + 1];
            dst[y * w + x] = acc / 16.0;
        }
    }
    dst
}

/// Computes smoothed doubled-angle orientation tensors `(cos2, sin2)` and ridge normal
/// orientations per 16x16 block.
fn compute_smoothed_orientations(
    norm: &[f32],
    w: usize,
    h: usize,
    block_cols: usize,
    block_rows: usize,
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut cos2 = vec![0.0f32; block_cols * block_rows];
    let mut sin2 = vec![0.0f32; block_cols * block_rows];

    for by in 0..block_rows {
        let top = by * BLOCK;
        let bottom = (top + BLOCK).min(h.saturating_sub(1));
        for bx in 0..block_cols {
            let left = bx * BLOCK;
            let right = (left + BLOCK).min(w.saturating_sub(1));
            let mut gxx = 0.0f32;
            let mut gyy = 0.0f32;
            let mut gxy = 0.0f32;
            for y in top.max(1)..bottom {
                for x in left.max(1)..right {
                    let at = |xx: usize, yy: usize| norm[yy * w + xx];
                    let gx = at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)
                        - at(x - 1, y - 1)
                        - 2.0 * at(x - 1, y)
                        - at(x - 1, y + 1);
                    let gy = at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)
                        - at(x - 1, y - 1)
                        - 2.0 * at(x, y - 1)
                        - at(x + 1, y - 1);
                    gxx += gx * gx;
                    gyy += gy * gy;
                    gxy += gx * gy;
                }
            }
            let idx = by * block_cols + bx;
            cos2[idx] = gxx - gyy;
            sin2[idx] = 2.0 * gxy;
        }
    }

    let mut smooth_c = vec![0.0f32; block_cols * block_rows];
    let mut smooth_s = vec![0.0f32; block_cols * block_rows];
    let mut orientations = vec![0.0f32; block_cols * block_rows];
    let weights = [[1.0f32, 2.0, 1.0], [2.0, 4.0, 2.0], [1.0, 2.0, 1.0]];
    for by in 0..block_rows {
        for bx in 0..block_cols {
            let mut sc = 0.0f32;
            let mut ss = 0.0f32;
            for dy in -1i32..=1 {
                let ny = (by as i32 + dy).clamp(0, block_rows as i32 - 1) as usize;
                for dx in -1i32..=1 {
                    let nx = (bx as i32 + dx).clamp(0, block_cols as i32 - 1) as usize;
                    let w_k = weights[(dy + 1) as usize][(dx + 1) as usize];
                    let n_idx = ny * block_cols + nx;
                    sc += w_k * cos2[n_idx];
                    ss += w_k * sin2[n_idx];
                }
            }
            let idx = by * block_cols + bx;
            smooth_c[idx] = sc;
            smooth_s[idx] = ss;
            orientations[idx] = 0.5 * ss.atan2(sc);
        }
    }
    (smooth_c, smooth_s, orientations)
}

/// Estimates local ridge frequency (cycles/pixel) for each block by projecting
/// normalized intensities orthogonal to the ridge orientation.
fn estimate_block_frequencies(
    norm: &[f32],
    orientations: &[f32],
    w: usize,
    h: usize,
    block_cols: usize,
    block_rows: usize,
) -> Vec<f32> {
    let mut freqs = vec![DEFAULT_RIDGE_FREQUENCY; block_cols * block_rows];
    const WIN_HALF: i32 = 16;
    const TANGENT_HALF: i32 = 6;

    for by in 0..block_rows {
        let cy = (by * BLOCK + BLOCK / 2).min(h.saturating_sub(1)) as f32;
        for bx in 0..block_cols {
            let cx = (bx * BLOCK + BLOCK / 2).min(w.saturating_sub(1)) as f32;
            let theta = orientations[by * block_cols + bx];
            let (sin_t, cos_t) = theta.sin_cos();

            let mut signature = [0.0f32; (WIN_HALF * 2 + 1) as usize];
            for (k_idx, k) in (-WIN_HALF..=WIN_HALF).enumerate() {
                let mut sum = 0.0f32;
                let mut count = 0.0f32;
                for d in -TANGENT_HALF..=TANGENT_HALF {
                    let sx = (cx + k as f32 * cos_t - d as f32 * sin_t).round() as i32;
                    let sy = (cy + k as f32 * sin_t + d as f32 * cos_t).round() as i32;
                    if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                        sum += norm[sy as usize * w + sx as usize];
                        count += 1.0;
                    }
                }
                signature[k_idx] = if count > 0.0 { sum / count } else { 128.0 };
            }

            // Detect local peaks in the 1D x-signature to measure inter-ridge distance.
            let mut peaks = Vec::new();
            for i in 1..(signature.len() - 1) {
                if signature[i] > signature[i - 1]
                    && signature[i] >= signature[i + 1]
                    && signature[i] > 132.0
                {
                    peaks.push(i);
                }
            }
            if peaks.len() >= 2 {
                let span = (peaks[peaks.len() - 1] - peaks[0]) as f32;
                let avg_period = span / (peaks.len() - 1) as f32;
                if (5.0..=20.0).contains(&avg_period) {
                    freqs[by * block_cols + bx] =
                        (1.0 / avg_period).clamp(MIN_RIDGE_FREQUENCY, MAX_RIDGE_FREQUENCY);
                }
            }
        }
    }
    freqs
}

fn kernel(theta: f32, ridge_frequency: f32) -> Vec<f32> {
    let mut values = Vec::with_capacity(((RADIUS * 2 + 1).pow(2)) as usize);
    let (sin, cos) = theta.sin_cos();
    for y in -RADIUS..=RADIUS {
        for x in -RADIUS..=RADIUS {
            let x_theta = x as f32 * cos + y as f32 * sin;
            let y_theta = -x as f32 * sin + y as f32 * cos;
            let envelope = (-0.5 * (x_theta.powi(2) / 12.0 + y_theta.powi(2) / 20.0)).exp();
            values.push(envelope * (2.0 * std::f32::consts::PI * ridge_frequency * x_theta).cos());
        }
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    for value in &mut values {
        *value -= mean;
    }
    values
}

/// Enhances ridge texture of an arbitrary [`GrayscaleImage`] using intensity normalization,
/// smoothed Sobel orientation, adaptive ridge frequency, and a contextual 9x9 Gabor filter.
pub fn enhance_gabor_image(image: &GrayscaleImage) -> GrayscaleImage {
    let (w, h) = (image.width(), image.height());
    let r = RADIUS as usize;
    if w <= r * 2 || h <= r * 2 {
        return image.clone();
    }

    let pixels = image.pixels();
    let norm = gaussian_smooth_3x3(&normalize_intensity(pixels, w, h), w, h);
    let block_cols = w.div_ceil(BLOCK);
    let block_rows = h.div_ceil(BLOCK);
    let (smooth_c, smooth_s, orientations) =
        compute_smoothed_orientations(&norm, w, h, block_cols, block_rows);
    let frequencies =
        estimate_block_frequencies(&norm, &orientations, w, h, block_cols, block_rows);

    // Precompute quantized orientation filters per block frequency for fast pixel-level
    // bilinear orientation interpolation without block-boundary artifacts.
    let block_filter_bank: Vec<Vec<Vec<f32>>> = frequencies
        .iter()
        .map(|&freq| {
            (0..ORIENTATION_BINS)
                .map(|bin| {
                    let theta = (bin as f32 / ORIENTATION_BINS as f32) * std::f32::consts::PI
                        - std::f32::consts::FRAC_PI_2;
                    kernel(theta, freq)
                })
                .collect()
        })
        .collect();

    let process_row = |y: usize| -> Vec<f32> {
        let mut row = vec![0.0; w];
        if y < r || y + r >= h {
            return row;
        }
        let fy = (y as f32 / BLOCK as f32 - 0.5).clamp(0.0, (block_rows - 1) as f32);
        let by0 = (fy.floor() as usize).min(block_rows - 1);
        let by1 = (by0 + 1).min(block_rows - 1);
        let wy = fy - by0 as f32;

        for (x, value) in row.iter_mut().enumerate() {
            if x < r || x + r >= w {
                continue;
            }
            let fx = (x as f32 / BLOCK as f32 - 0.5).clamp(0.0, (block_cols - 1) as f32);
            let bx0 = (fx.floor() as usize).min(block_cols - 1);
            let bx1 = (bx0 + 1).min(block_cols - 1);
            let wx = fx - bx0 as f32;

            let i00 = by0 * block_cols + bx0;
            let i01 = by0 * block_cols + bx1;
            let i10 = by1 * block_cols + bx0;
            let i11 = by1 * block_cols + bx1;

            let c = (1.0 - wy) * ((1.0 - wx) * smooth_c[i00] + wx * smooth_c[i01])
                + wy * ((1.0 - wx) * smooth_c[i10] + wx * smooth_c[i11]);
            let s = (1.0 - wy) * ((1.0 - wx) * smooth_s[i00] + wx * smooth_s[i01])
                + wy * ((1.0 - wx) * smooth_s[i10] + wx * smooth_s[i11]);
            let theta = 0.5 * s.atan2(c);
            let bin = (((theta + std::f32::consts::FRAC_PI_2) / std::f32::consts::PI)
                * ORIENTATION_BINS as f32)
                .round()
                .rem_euclid(ORIENTATION_BINS as f32) as usize
                % ORIENTATION_BINS;

            let block_y = (y / BLOCK).min(block_rows - 1);
            let block_x = (x / BLOCK).min(block_cols - 1);
            let block = block_y * block_cols + block_x;
            let filter = &block_filter_bank[block][bin];
            let mut response = 0.0;
            for ky in -RADIUS..=RADIUS {
                let sy = (y as i32 + ky) as usize;
                for kx in -RADIUS..=RADIUS {
                    let sx = (x as i32 + kx) as usize;
                    let sample = norm[sy * w + sx] - 128.0;
                    response += sample
                        * filter[((ky + RADIUS) * (RADIUS * 2 + 1) + (kx + RADIUS)) as usize];
                }
            }
            *value = response;
        }
        row
    };

    #[cfg(feature = "parallel")]
    let response: Vec<f32> = (0..h).into_par_iter().flat_map_iter(process_row).collect();
    #[cfg(not(feature = "parallel"))]
    let response: Vec<f32> = (0..h).flat_map(process_row).collect();

    // Normalize response using robust standard deviation so edge outliers do not
    // compress interior ridge contrast.
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut count = 0.0f64;
    for y in r..(h - r) {
        let row = &response[y * w..(y + 1) * w];
        for &v in &row[r..(w - r)] {
            let vf = v as f64;
            sum += vf;
            sum_sq += vf * vf;
            count += 1.0;
        }
    }
    let mean = if count > 0.0 { sum / count } else { 0.0 };
    let std = if count > 0.0 {
        (sum_sq / count - mean * mean).max(1e-6).sqrt()
    } else {
        1.0
    };
    let scale = (55.0 / std) as f32;
    let mean_f = mean as f32;
    let mut out = vec![255u8; w * h];
    for y in r..(h - r) {
        for x in r..(w - r) {
            let idx = y * w + x;
            out[idx] = (128.0 + (response[idx] - mean_f) * scale).clamp(0.0, 255.0) as u8;
        }
    }
    GrayscaleImage::new(w, h, out).expect("Gabor enhancement preserves image dimensions")
}

/// Enhances ridge texture of a [`RawFrame`] using contextual Gabor filtering.
pub fn enhance_gabor(frame: &RawFrame) -> RawFrame {
    let enhanced = enhance_gabor_image(frame.as_image());
    RawFrame::new(enhanced.into_pixels())
        .expect("Gabor enhancement preserves HU20 frame dimensions")
}
