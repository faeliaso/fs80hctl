//! Block-oriented Gabor enhancement for 500-DPI fingerprint frames.

use crate::{HEIGHT, RawFrame, WIDTH};
#[cfg(feature = "parallel")]
use rayon::prelude::*;

const BLOCK: usize = 16;
const RADIUS: i32 = 4;
const RIDGE_FREQUENCY: f32 = 1.0 / 8.0;

fn orientation(frame: &RawFrame, left: usize, top: usize) -> f32 {
    let mut gxx = 0.0f32;
    let mut gyy = 0.0f32;
    let mut gxy = 0.0f32;
    let pixels = frame.pixels();
    for y in (top.max(1))..(top + BLOCK).min(HEIGHT as usize - 1) {
        for x in (left.max(1))..(left + BLOCK).min(WIDTH as usize - 1) {
            let at = |xx: usize, yy: usize| pixels[yy * WIDTH as usize + xx] as f32;
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
    0.5 * (2.0 * gxy).atan2(gxx - gyy) + std::f32::consts::FRAC_PI_2
}

fn kernel(theta: f32) -> Vec<f32> {
    let mut values = Vec::with_capacity(((RADIUS * 2 + 1).pow(2)) as usize);
    let (sin, cos) = theta.sin_cos();
    for y in -RADIUS..=RADIUS {
        for x in -RADIUS..=RADIUS {
            let x_theta = x as f32 * cos + y as f32 * sin;
            let y_theta = -x as f32 * sin + y as f32 * cos;
            let envelope = (-0.5 * (x_theta.powi(2) / 16.0 + y_theta.powi(2) / 9.0)).exp();
            values.push(envelope * (2.0 * std::f32::consts::PI * RIDGE_FREQUENCY * x_theta).cos());
        }
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    for value in &mut values {
        *value -= mean;
    }
    values
}

/// Enhances ridge texture using local Sobel orientation and a 9x9 Gabor kernel.
pub fn enhance_gabor(frame: &RawFrame) -> RawFrame {
    let block_cols = WIDTH as usize / BLOCK;
    let orientations: Vec<f32> = (0..(WIDTH as usize / BLOCK * HEIGHT as usize / BLOCK))
        .map(|index| {
            orientation(
                frame,
                (index % block_cols) * BLOCK,
                (index / block_cols) * BLOCK,
            )
        })
        .collect();
    let filters: Vec<Vec<f32>> = orientations.iter().copied().map(kernel).collect();
    let pixels = frame.pixels();
    let process_row = |y: usize| -> Vec<f32> {
        let mut row = vec![0.0; WIDTH as usize];
        for (x, value) in row.iter_mut().enumerate() {
            if x < RADIUS as usize
                || y < RADIUS as usize
                || x + RADIUS as usize >= WIDTH as usize
                || y + RADIUS as usize >= HEIGHT as usize
            {
                *value = pixels[y * WIDTH as usize + x] as f32;
                continue;
            }
            let block = (y / BLOCK) * block_cols + (x / BLOCK);
            let filter = &filters[block];
            let mut response = 0.0;
            for ky in -RADIUS..=RADIUS {
                for kx in -RADIUS..=RADIUS {
                    let sample = pixels
                        [(y as i32 + ky) as usize * WIDTH as usize + (x as i32 + kx) as usize]
                        as f32
                        - 127.5;
                    response += sample
                        * filter[((ky + RADIUS) * (RADIUS * 2 + 1) + (kx + RADIUS)) as usize];
                }
            }
            *value = response;
        }
        row
    };
    #[cfg(feature = "parallel")]
    let response: Vec<f32> = (0..HEIGHT as usize)
        .into_par_iter()
        .flat_map_iter(process_row)
        .collect();
    #[cfg(not(feature = "parallel"))]
    let response: Vec<f32> = (0..HEIGHT as usize).flat_map(process_row).collect();
    let (min, max) = response
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        });
    let scale = if max > min { 255.0 / (max - min) } else { 1.0 };
    RawFrame::new(
        response
            .into_iter()
            .map(|value| ((value - min) * scale).clamp(0.0, 255.0) as u8)
            .collect(),
    )
    .expect("Gabor enhancement preserves FS80H frame dimensions")
}
