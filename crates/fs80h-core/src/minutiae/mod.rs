//! Dimension-independent ridge skeleton and crossing-number minutia extraction.

use crate::{GrayscaleImage, Minutia, MinutiaKind};

fn index(width: usize, x: usize, y: usize) -> usize {
    y * width + x
}

/// Produces a locally thresholded ridge map. Both ridge polarities are accepted
/// by selecting the darker of each pixel's local neighborhood.
fn binarize(image: &GrayscaleImage) -> Vec<bool> {
    let (width, height) = (image.width(), image.height());
    let mut out = vec![false; width * height];
    for y in 8..height.saturating_sub(8) {
        for x in 8..width.saturating_sub(8) {
            let mut sum = 0u32;
            for yy in y - 8..=y + 8 {
                for xx in x - 8..=x + 8 {
                    sum += image.pixels()[index(width, xx, yy)] as u32;
                }
            }
            let mean = sum as f32 / 289.0;
            out[index(width, x, y)] = (image.pixels()[index(width, x, y)] as f32) < mean - 8.0;
        }
    }
    out
}

fn neighbors(mask: &[bool], width: usize, x: usize, y: usize) -> [bool; 8] {
    [
        mask[index(width, x, y - 1)],
        mask[index(width, x + 1, y - 1)],
        mask[index(width, x + 1, y)],
        mask[index(width, x + 1, y + 1)],
        mask[index(width, x, y + 1)],
        mask[index(width, x - 1, y + 1)],
        mask[index(width, x - 1, y)],
        mask[index(width, x - 1, y - 1)],
    ]
}

/// Zhang-Suen thinning, bounded to keep malformed input from consuming time.
fn thin(mask: &mut [bool], width: usize, height: usize) {
    for _ in 0..128 {
        let mut changed = false;
        for phase in 0..2 {
            let mut remove = Vec::new();
            for y in 1..height - 1 {
                for x in 1..width - 1 {
                    if !mask[index(width, x, y)] {
                        continue;
                    }
                    let n = neighbors(mask, width, x, y);
                    let count = n.iter().filter(|&&p| p).count();
                    let transitions = (0..8).filter(|&i| !n[i] && n[(i + 1) % 8]).count();
                    let p2 = n[0];
                    let p4 = n[2];
                    let p6 = n[4];
                    let p8 = n[6];
                    let guard = if phase == 0 {
                        !(p2 && p4 && p6) && !(p4 && p6 && p8)
                    } else {
                        !(p2 && p4 && p8) && !(p2 && p6 && p8)
                    };
                    if (2..=6).contains(&count) && transitions == 1 && guard {
                        remove.push(index(width, x, y));
                    }
                }
            }
            for i in remove {
                mask[i] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Follows a single skeleton branch from an ending. Returns its final point
/// only when the branch stays unambiguous for the requested distance.
fn trace_branch(
    mask: &[bool],
    width: usize,
    x: usize,
    y: usize,
    minimum: usize,
) -> Option<(usize, usize)> {
    let mut previous = (x, y);
    let mut current = (x, y);
    for _ in 0..minimum {
        let mut next = Vec::new();
        for yy in current.1 - 1..=current.1 + 1 {
            for xx in current.0 - 1..=current.0 + 1 {
                if (xx, yy) != current && (xx, yy) != previous && mask[index(width, xx, yy)] {
                    next.push((xx, yy));
                }
            }
        }
        if next.len() != 1 {
            return None;
        }
        previous = current;
        current = next[0];
    }
    Some(current)
}

fn bifurcation_support(mask: &[bool], width: usize, x: usize, y: usize) -> bool {
    let branches: Vec<_> = (y - 1..=y + 1)
        .flat_map(|yy| (x - 1..=x + 1).map(move |xx| (xx, yy)))
        .filter(|&(xx, yy)| (xx, yy) != (x, y) && mask[index(width, xx, yy)])
        .collect();
    branches.len() == 3
        && branches.into_iter().all(|(bx, by)| {
            let mut previous = (x, y);
            let mut current = (bx, by);
            for _ in 0..8 {
                let next: Vec<_> = (current.1 - 1..=current.1 + 1)
                    .flat_map(|yy| (current.0 - 1..=current.0 + 1).map(move |xx| (xx, yy)))
                    .filter(|&(xx, yy)| {
                        (xx, yy) != current && (xx, yy) != previous && mask[index(width, xx, yy)]
                    })
                    .collect();
                if next.len() != 1 {
                    return false;
                }
                previous = current;
                current = next[0];
            }
            true
        })
}

/// Estimates whether a candidate belongs to a coherent ridge field rather than
/// a noisy, broken edge. Fingerprint ridges have a concentrated local gradient
/// direction; paper texture and the contact boundary do not. The calculation
/// uses the doubled angle representation, so ridge directions 180° apart are
/// treated as identical.
fn local_ridge_quality(image: &GrayscaleImage, x: usize, y: usize) -> f32 {
    const RADIUS: usize = 8;
    let width = image.width();
    let pixels = image.pixels();
    let mut magnitude_sum = 0.0_f32;
    let mut cos_sum = 0.0_f32;
    let mut sin_sum = 0.0_f32;
    let mut intensity_sum = 0.0_f32;
    let mut intensity_sq_sum = 0.0_f32;
    let mut samples = 0.0_f32;

    for yy in y - RADIUS..=y + RADIUS {
        for xx in x - RADIUS..=x + RADIUS {
            let gx =
                pixels[index(width, xx + 1, yy)] as f32 - pixels[index(width, xx - 1, yy)] as f32;
            let gy =
                pixels[index(width, xx, yy + 1)] as f32 - pixels[index(width, xx, yy - 1)] as f32;
            let magnitude = gx.hypot(gy);
            if magnitude > 0.0 {
                let angle = gy.atan2(gx);
                cos_sum += magnitude * (2.0 * angle).cos();
                sin_sum += magnitude * (2.0 * angle).sin();
                magnitude_sum += magnitude;
            }
            let intensity = pixels[index(width, xx, yy)] as f32;
            intensity_sum += intensity;
            intensity_sq_sum += intensity * intensity;
            samples += 1.0;
        }
    }

    if magnitude_sum == 0.0 {
        return 0.0;
    }
    let coherence = cos_sum.hypot(sin_sum) / magnitude_sum;
    let variance = (intensity_sq_sum / samples - (intensity_sum / samples).powi(2)).max(0.0);
    // Both terms deliberately saturate: a crisp ridge field scores highly,
    // while a high-contrast isolated artifact cannot pass on contrast alone.
    (coherence * (variance / 600.0).sqrt().min(1.0)).clamp(0.0, 1.0)
}

/// Extracts sparse ridge endings and bifurcations from a grayscale image.
pub fn extract_skeleton_minutiae(image: &GrayscaleImage) -> Vec<Minutia> {
    let (width, height) = (image.width(), image.height());
    if width < 32 || height < 32 {
        return Vec::new();
    }
    let mut mask = binarize(image);
    thin(&mut mask, width, height);
    // Contact boundaries create artificial ridge endings. A larger exclusion
    // band is intentional: images are partial impressions, not rolled prints.
    const EDGE_MARGIN: usize = 32;
    const MIN_RIDGE_QUALITY: f32 = 0.55;
    let mut out = Vec::new();
    for y in EDGE_MARGIN..height - EDGE_MARGIN {
        for x in EDGE_MARGIN..width - EDGE_MARGIN {
            if !mask[index(width, x, y)] {
                continue;
            }
            let n = neighbors(&mask, width, x, y);
            let cn = (0..8).filter(|&i| n[i] != n[(i + 1) % 8]).count() / 2;
            let kind = match cn {
                1 => MinutiaKind::RidgeEnding,
                3 => MinutiaKind::Bifurcation,
                _ => continue,
            };
            let branch_end = if kind == MinutiaKind::RidgeEnding {
                let Some(end) = trace_branch(&mask, width, x, y, 10) else {
                    continue;
                };
                Some(end)
            } else {
                None
            };
            if kind == MinutiaKind::Bifurcation && !bifurcation_support(&mask, width, x, y) {
                continue;
            }
            let quality = local_ridge_quality(image, x, y);
            if quality < MIN_RIDGE_QUALITY {
                continue;
            }
            if out
                .iter()
                .any(|m: &Minutia| (m.x - x as f32).hypot(m.y - y as f32) < 18.0)
            {
                continue;
            }
            let angle_deg = if let Some((end_x, end_y)) = branch_end {
                // The ridge tangent is stable across illumination changes;
                // Sobel's normal is not. It is therefore the appropriate
                // orientation for a ridge-ending correspondence.
                ((end_y as f32 - y as f32)
                    .atan2(end_x as f32 - x as f32)
                    .to_degrees())
                .rem_euclid(180.0)
            } else {
                let gx = image.pixels()[index(width, x + 1, y)] as f32
                    - image.pixels()[index(width, x - 1, y)] as f32;
                let gy = image.pixels()[index(width, x, y + 1)] as f32
                    - image.pixels()[index(width, x, y - 1)] as f32;
                gy.atan2(gx).to_degrees().rem_euclid(180.0)
            };
            out.push(Minutia {
                x: x as f32,
                y: y as f32,
                angle_deg,
                quality,
                kind,
            });
        }
    }
    out
}
