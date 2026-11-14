//! Dimension-independent ridge skeleton and crossing-number minutia extraction.
//!
//! Pipeline:
//! 1. Foreground finger-mask segmentation with boundary erosion to reject border artifacts.
//! 2. Adaptive local binarization and small-island / noise removal.
//! 3. Zhang-Suen morphological thinning to 1-pixel ridge skeletons.
//! 4. Crossing Number (`CN == 1` RidgeEnding, `CN == 3` Bifurcation) with continuous
//!    rotation-invariant angles in `[0, 360)`.

use crate::enhance::enhance_gabor_image;
use crate::{GrayscaleImage, Minutia, MinutiaKind};

const FOREGROUND_BLOCK: usize = 8;

fn index(width: usize, x: usize, y: usize) -> usize {
    y * width + x
}

/// Computes a boolean foreground mask at pixel resolution by evaluating 16x16 block
/// local variance and eroding 1 block at the finger/background boundary.
fn compute_foreground_mask(image: &GrayscaleImage) -> Vec<bool> {
    let (w, h) = (image.width(), image.height());
    let pixels = image.pixels();
    let bcols = w.div_ceil(FOREGROUND_BLOCK);
    let brows = h.div_ceil(FOREGROUND_BLOCK);
    let mut active_blocks = vec![false; bcols * brows];

    for by in 0..brows {
        let y0 = by * FOREGROUND_BLOCK;
        let y1 = (y0 + FOREGROUND_BLOCK).min(h);
        for bx in 0..bcols {
            let x0 = bx * FOREGROUND_BLOCK;
            let x1 = (x0 + FOREGROUND_BLOCK).min(w);
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
            if count > 0.0 {
                let mean = sum / count;
                let var = (sum_sq / count - mean * mean).max(0.0);
                active_blocks[by * bcols + bx] = var >= 35.0;
            }
        }
    }

    // Erode 1 block on the boundary between finger and background (and image edges)
    // so artificial ridge endings on the contact perimeter are excluded.
    let mut eroded_blocks = vec![false; bcols * brows];
    for by in 1..brows.saturating_sub(1) {
        for bx in 1..bcols.saturating_sub(1) {
            let all_active = (-1i32..=1).all(|dy| {
                (-1i32..=1).all(|dx| {
                    let ny = (by as i32 + dy) as usize;
                    let nx = (bx as i32 + dx) as usize;
                    active_blocks[ny * bcols + nx]
                })
            });
            eroded_blocks[by * bcols + bx] = all_active;
        }
    }

    let mut mask = vec![false; w * h];
    for y in 0..h {
        let by = (y / FOREGROUND_BLOCK).min(brows - 1);
        for x in 0..w {
            let bx = (x / FOREGROUND_BLOCK).min(bcols - 1);
            mask[y * w + x] = eroded_blocks[by * bcols + bx];
        }
    }
    mask
}

/// Produces a locally thresholded ridge map using a 17x17 integral-like local mean.
fn binarize(image: &GrayscaleImage) -> Vec<bool> {
    let (width, height) = (image.width(), image.height());
    let pixels = image.pixels();
    let mut out = vec![false; width * height];
    for y in 8..height.saturating_sub(8) {
        for x in 8..width.saturating_sub(8) {
            let mut sum = 0u32;
            for yy in y - 8..=y + 8 {
                let row = yy * width;
                for xx in x - 8..=x + 8 {
                    sum += pixels[row + xx] as u32;
                }
            }
            let mean = sum as f32 / 289.0;
            out[index(width, x, y)] = (pixels[index(width, x, y)] as f32) < mean - 2.0;
        }
    }
    out
}

/// Removes small isolated foreground islands (e.g. pores/speckles) smaller than `min_pixels`.
fn remove_small_islands(mask: &mut [bool], width: usize, height: usize, min_pixels: usize) {
    let mut visited = vec![false; width * height];
    let mut stack = Vec::new();
    let mut component = Vec::new();

    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let start = index(width, x, y);
            if !mask[start] || visited[start] {
                continue;
            }
            visited[start] = true;
            stack.clear();
            component.clear();
            stack.push((x, y));

            while let Some((cx, cy)) = stack.pop() {
                component.push(index(width, cx, cy));
                for ny in cy.saturating_sub(1)..=(cy + 1).min(height - 1) {
                    for nx in cx.saturating_sub(1)..=(cx + 1).min(width - 1) {
                        let nidx = index(width, nx, ny);
                        if mask[nidx] && !visited[nidx] {
                            visited[nidx] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }

            if component.len() < min_pixels {
                for &idx in &component {
                    mask[idx] = false;
                }
            }
        }
    }
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

/// Fills 1-pixel and 2-pixel sweat-pore pinholes inside binary ridges and smooths
/// isolated single-pixel noise spikes before thinning.
fn fill_pinholes(mask: &mut [bool], width: usize, height: usize) {
    let snapshot = mask.to_vec();
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let idx = index(width, x, y);
            let n_true = neighbors(&snapshot, width, x, y)
                .into_iter()
                .filter(|&v| v)
                .count();
            if !snapshot[idx] && n_true >= 6 {
                mask[idx] = true;
            } else if snapshot[idx] && n_true <= 1 {
                mask[idx] = false;
            }
        }
    }
}

/// Removes short skeleton spurs (length <= `max_len` pixels) that end at a
/// junction (`CN >= 3`), preventing sweat-pore hairs from creating false bifurcations.
fn prune_spurs(mask: &mut [bool], width: usize, height: usize, max_len: usize) {
    for _ in 0..2 {
        let mut to_remove = Vec::new();
        for y in 2..height.saturating_sub(2) {
            for x in 2..width.saturating_sub(2) {
                let idx = index(width, x, y);
                if !mask[idx] {
                    continue;
                }
                let n = neighbors(mask, width, x, y);
                let cn = (0..8).filter(|&i| n[i] != n[(i + 1) % 8]).count() / 2;
                if cn != 1 {
                    continue;
                }

                let mut path = vec![(x, y)];
                let mut prev = (x, y);
                let mut curr = (x, y);
                let mut hits_junction = false;

                for _ in 0..max_len {
                    let next: Vec<(usize, usize)> = (curr.1 - 1..=curr.1 + 1)
                        .flat_map(|yy| (curr.0 - 1..=curr.0 + 1).map(move |xx| (xx, yy)))
                        .filter(|&(xx, yy)| {
                            (xx, yy) != curr && (xx, yy) != prev && mask[index(width, xx, yy)]
                        })
                        .collect();
                    if next.len() != 1 {
                        if next.len() > 1 {
                            hits_junction = true;
                        }
                        break;
                    }
                    let nxt = next[0];
                    let nn = neighbors(mask, width, nxt.0, nxt.1);
                    let nxt_cn = (0..8).filter(|&i| nn[i] != nn[(i + 1) % 8]).count() / 2;
                    if nxt_cn >= 3 {
                        hits_junction = true;
                        break;
                    }
                    path.push(nxt);
                    prev = curr;
                    curr = nxt;
                }

                if hits_junction {
                    for (px, py) in path {
                        to_remove.push(index(width, px, py));
                    }
                }
            }
        }
        if to_remove.is_empty() {
            break;
        }
        for idx in to_remove {
            mask[idx] = false;
        }
    }
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

/// Validates a bifurcation has 3 distinct unambiguous branches of length >= 5
/// and returns the canonical [0, 360) angle bisecting the two splitting arms.
fn bifurcation_angle_deg(mask: &[bool], width: usize, x: usize, y: usize) -> Option<f32> {
    let branches: Vec<_> = (y - 1..=y + 1)
        .flat_map(|yy| (x - 1..=x + 1).map(move |xx| (xx, yy)))
        .filter(|&(xx, yy)| (xx, yy) != (x, y) && mask[index(width, xx, yy)])
        .collect();
    if branches.len() != 3 {
        return None;
    }

    let mut dirs = [(0.0f32, 0.0f32); 3];
    for (idx, &(bx, by)) in branches.iter().enumerate() {
        let mut previous = (x, y);
        let mut current = (bx, by);
        for step in 0..5 {
            let next: Vec<_> = (current.1 - 1..=current.1 + 1)
                .flat_map(|yy| (current.0 - 1..=current.0 + 1).map(move |xx| (xx, yy)))
                .filter(|&(xx, yy)| {
                    (xx, yy) != current && (xx, yy) != previous && mask[index(width, xx, yy)]
                })
                .collect();
            if next.len() != 1 {
                if step >= 2 && !next.is_empty() {
                    break;
                }
                return None;
            }
            previous = current;
            current = next[0];
        }
        let dx = current.0 as f32 - x as f32;
        let dy = current.1 as f32 - y as f32;
        let norm = dx.hypot(dy);
        if norm == 0.0 {
            return None;
        }
        dirs[idx] = (dx / norm, dy / norm);
    }

    // Find the two arms with the smallest angular separation (largest dot product);
    // their mean direction is the canonical bifurcation orientation in [0, 360).
    let pairs = [(0usize, 1usize), (0, 2), (1, 2)];
    let (a, b) = pairs
        .into_iter()
        .max_by(|&(i1, j1), &(i2, j2)| {
            let dot1 = dirs[i1].0 * dirs[j1].0 + dirs[i1].1 * dirs[j1].1;
            let dot2 = dirs[i2].0 * dirs[j2].0 + dirs[i2].1 * dirs[j2].1;
            dot1.partial_cmp(&dot2).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or((0, 1));

    let mx = dirs[a].0 + dirs[b].0;
    let my = dirs[a].1 + dirs[b].1;
    let angle = my.atan2(mx).to_degrees().rem_euclid(360.0);
    Some(angle)
}

/// Estimates whether a candidate belongs to a coherent ridge field rather than
/// a noisy, broken edge.
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
    // Preserve high-curvature core/delta minutiae where local orientation turns rapidly,
    // while still penalizing flat or incoherent noise regions.
    ((0.35 + 0.65 * coherence) * (variance / 450.0).sqrt().min(1.0)).clamp(0.0, 1.0)
}

/// Extracts sparse ridge endings and bifurcations from a grayscale image with
/// continuous `[0, 360)` orientation angles.
pub fn extract_skeleton_minutiae(image: &GrayscaleImage) -> Vec<Minutia> {
    let (width, height) = (image.width(), image.height());
    if width < 32 || height < 32 {
        return Vec::new();
    }
    let fg_mask = compute_foreground_mask(image);
    let mut mask = binarize(image);
    remove_small_islands(&mut mask, width, height, 8);
    fill_pinholes(&mut mask, width, height);
    thin(&mut mask, width, height);
    prune_spurs(&mut mask, width, height, 4);

    const EDGE_MARGIN: usize = 20;
    const MIN_RIDGE_QUALITY: f32 = 0.25;
    let mut out = Vec::new();
    for y in EDGE_MARGIN..height.saturating_sub(EDGE_MARGIN) {
        for x in EDGE_MARGIN..width.saturating_sub(EDGE_MARGIN) {
            let idx = index(width, x, y);
            if !mask[idx] || !fg_mask[idx] {
                continue;
            }
            let n = neighbors(&mask, width, x, y);
            let cn = (0..8).filter(|&i| n[i] != n[(i + 1) % 8]).count() / 2;
            let (kind, angle_deg) = match cn {
                1 => {
                    let Some((end_x, end_y)) = trace_branch(&mask, width, x, y, 5) else {
                        continue;
                    };
                    let angle = ((end_y as f32 - y as f32)
                        .atan2(end_x as f32 - x as f32)
                        .to_degrees())
                    .rem_euclid(360.0);
                    (MinutiaKind::RidgeEnding, angle)
                }
                3 => {
                    let Some(angle) = bifurcation_angle_deg(&mask, width, x, y) else {
                        continue;
                    };
                    (MinutiaKind::Bifurcation, angle)
                }
                _ => continue,
            };

            let quality = local_ridge_quality(image, x, y);
            if quality < MIN_RIDGE_QUALITY {
                continue;
            }
            if out
                .iter()
                .any(|m: &Minutia| (m.x - x as f32).hypot(m.y - y as f32) < 10.0)
            {
                continue;
            }
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

/// Runs contextual Gabor enhancement followed by morphological skeleton minutia extraction.
pub fn extract_enhanced_minutiae(image: &GrayscaleImage) -> Vec<Minutia> {
    let enhanced = enhance_gabor_image(image);
    extract_skeleton_minutiae(&enhanced)
}
