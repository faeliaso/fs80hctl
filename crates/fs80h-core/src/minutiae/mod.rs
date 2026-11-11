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

fn branch_support(mask: &[bool], width: usize, x: usize, y: usize, minimum: usize) -> bool {
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
            return false;
        }
        previous = current;
        current = next[0];
    }
    true
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

/// Extracts sparse ridge endings and bifurcations from a grayscale image.
pub fn extract_skeleton_minutiae(image: &GrayscaleImage) -> Vec<Minutia> {
    let (width, height) = (image.width(), image.height());
    if width < 32 || height < 32 {
        return Vec::new();
    }
    let mut mask = binarize(image);
    thin(&mut mask, width, height);
    let mut out = Vec::new();
    for y in 20..height - 20 {
        for x in 20..width - 20 {
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
            if kind == MinutiaKind::RidgeEnding && !branch_support(&mask, width, x, y, 10) {
                continue;
            }
            if kind == MinutiaKind::Bifurcation && !bifurcation_support(&mask, width, x, y) {
                continue;
            }
            if out
                .iter()
                .any(|m: &Minutia| (m.x - x as f32).hypot(m.y - y as f32) < 14.0)
            {
                continue;
            }
            let gx = image.pixels()[index(width, x + 1, y)] as f32
                - image.pixels()[index(width, x - 1, y)] as f32;
            let gy = image.pixels()[index(width, x, y + 1)] as f32
                - image.pixels()[index(width, x, y - 1)] as f32;
            out.push(Minutia {
                x: x as f32,
                y: y as f32,
                angle_deg: gy.atan2(gx).to_degrees().rem_euclid(180.0),
                quality: 0.8,
                kind,
            });
        }
    }
    out
}
