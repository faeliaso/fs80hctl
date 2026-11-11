use crate::domain::{HEIGHT, MatchScore, RawFrame, WIDTH};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Lowest correspondence support accepted by the forensic visual comparator.
/// Below this limit, isolated boundary endings can form a coincidental rigid
/// transform and must not be presented as a biometric match.
pub const MIN_FORENSIC_PAIR_SUPPORT: usize = 6;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MinutiaKind {
    RidgeEnding,
    Bifurcation,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Minutia {
    pub x: f32,
    pub y: f32,
    pub angle_deg: f32,
    pub quality: f32,
    pub kind: MinutiaKind,
}
#[derive(Clone, Debug, Serialize)]
pub struct MatchReport {
    pub score: MatchScore,
    pub matched_minutiae: usize,
    pub probe_minutiae: usize,
    pub reference_minutiae: usize,
    pub warnings: Vec<String>,
}

pub fn extract_minutiae(frame: &RawFrame) -> Vec<Minutia> {
    let mut out = Vec::new();
    let pixels = frame.pixels();
    for y in (16..HEIGHT as usize - 16).step_by(16) {
        for x in (16..WIDTH as usize - 16).step_by(16) {
            let mut sum = 0.0;
            let mut sum_sq = 0.0;
            let mut gx = 0.0;
            let mut gy = 0.0;
            let mut n = 0.0;
            for yy in y - 8..y + 8 {
                let row_offset = yy * WIDTH as usize;
                for xx in x - 8..x + 8 {
                    if let Some(&p_u8) = pixels.get(row_offset + xx) {
                        let p = p_u8 as f32;
                        sum += p;
                        sum_sq += p * p;
                        if let (Some(&p_right), Some(&p_left)) = (
                            pixels.get(row_offset + xx + 1),
                            pixels.get(row_offset + xx - 1),
                        ) {
                            gx += p_right as f32 - p_left as f32;
                        }
                        if let (Some(&p_down), Some(&p_up)) = (
                            pixels.get((yy + 1) * WIDTH as usize + xx),
                            pixels.get((yy - 1) * WIDTH as usize + xx),
                        ) {
                            gy += p_down as f32 - p_up as f32;
                        }
                        n += 1.0;
                    }
                }
            }
            let variance = (sum_sq / n - (sum / n).powi(2)).max(0.0);
            if variance >= 80.0 && (gx.abs() + gy.abs()) > 50.0 {
                let angle = (gy.atan2(gx) + PI / 2.0) * 180.0 / PI;
                // This lightweight extractor does not skeletonize ridges.  The local
                // orientation classification is deliberately deterministic and is
                // surfaced as an annotation aid, not as an AFIS-grade assertion.
                let kind = if gx.abs() >= gy.abs() {
                    MinutiaKind::RidgeEnding
                } else {
                    MinutiaKind::Bifurcation
                };
                out.push(Minutia {
                    x: x as f32,
                    y: y as f32,
                    angle_deg: angle.rem_euclid(180.0),
                    quality: (variance / 500.0).clamp(0.0, 1.0),
                    kind,
                });
            }
        }
    }
    out
}

/// Returns deterministic one-to-one homologous minutia indices using the same
/// geometric tolerances as [`compare_minutiae_sets`].
pub fn match_minutiae_pairs(probe: &[Minutia], reference: &[Minutia]) -> Vec<(usize, usize)> {
    let mut used = vec![false; reference.len()];
    let mut pairs = Vec::new();
    for (probe_index, a) in probe.iter().enumerate() {
        if let Some((reference_index, _)) = reference
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                !used[*i]
                    && a.kind == b.kind
                    && ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt() <= 15.0
                    && angle_distance(a.angle_deg, b.angle_deg) <= 25.0
            })
            .min_by(|(_, x), (_, y)| {
                let dx = (a.x - x.x).powi(2) + (a.y - x.y).powi(2);
                let dy = (a.x - y.x).powi(2) + (a.y - y.y).powi(2);
                dx.partial_cmp(&dy).unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            used[reference_index] = true;
            pairs.push((probe_index, reference_index));
        }
    }
    pairs
}

/// Deterministically selects the translation hypothesis with the most
/// geometrically consistent one-to-one pairs. This is the translation stage of
/// robust alignment; callers can evolve it to full rotation-aware RANSAC.
pub fn match_minutiae_pairs_aligned(
    probe: &[Minutia],
    reference: &[Minutia],
) -> Vec<(usize, usize)> {
    let mut best = Vec::new();
    for anchor in probe {
        for reference_anchor in reference {
            if anchor.kind != reference_anchor.kind
                || angle_distance(anchor.angle_deg, reference_anchor.angle_deg) > 25.0
            {
                continue;
            }
            let rotation = (reference_anchor.angle_deg - anchor.angle_deg).to_radians();
            let (sin, cos) = rotation.sin_cos();
            let rotated_x = anchor.x * cos - anchor.y * sin;
            let rotated_y = anchor.x * sin + anchor.y * cos;
            let (tx, ty) = (
                reference_anchor.x - rotated_x,
                reference_anchor.y - rotated_y,
            );
            let shifted: Vec<Minutia> = probe
                .iter()
                .map(|m| Minutia {
                    x: m.x * cos - m.y * sin + tx,
                    y: m.x * sin + m.y * cos + ty,
                    angle_deg: (m.angle_deg + rotation.to_degrees()).rem_euclid(180.0),
                    ..*m
                })
                .collect();
            let pairs = match_minutiae_pairs(&shifted, reference);
            if pairs.len() > best.len() {
                best = pairs;
            }
        }
    }
    best
}

fn angle_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs().rem_euclid(180.0);
    d.min(180.0 - d)
}

pub fn compare_minutiae_sets(p: &[Minutia], r: &[Minutia]) -> MatchReport {
    let matched = match_minutiae_pairs(p, r).len();
    let denom = p.len().max(r.len());
    let raw_score = if denom == 0 {
        0.0
    } else {
        matched as f32 / denom as f32 * 100.0
    };
    let score =
        MatchScore::new(raw_score.clamp(0.0, 100.0)).unwrap_or(MatchScore::new(0.0).unwrap());
    let mut warnings = vec![
        "result is a technical similarity indicator; not civil identification, forensic expert report, or liveness verification".into(),
    ];
    if p.len() < 10 || r.len() < 10 {
        warnings.push("few minutiae extracted; recapture recommended".into());
    }
    MatchReport {
        score,
        matched_minutiae: matched,
        probe_minutiae: p.len(),
        reference_minutiae: r.len(),
        warnings,
    }
}

pub fn compare_frames(probe: &RawFrame, reference: &RawFrame) -> MatchReport {
    let p = extract_minutiae(probe);
    let r = extract_minutiae(reference);
    compare_minutiae_sets(&p, &r)
}

#[derive(Clone, Debug, Serialize)]
pub struct GalleryMatchCandidate {
    pub reference_path: String,
    pub score: MatchScore,
    pub matched: bool,
    pub report: MatchReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct GalleryMatchReport {
    pub probe_minutiae: usize,
    pub total_candidates: usize,
    pub matches: Vec<GalleryMatchCandidate>,
    pub best_match: Option<GalleryMatchCandidate>,
    pub threshold_met: bool,
}

pub fn compare_probe_to_gallery(
    probe: &RawFrame,
    gallery: &[(String, RawFrame)],
    threshold: f32,
) -> GalleryMatchReport {
    let probe_minutiae = extract_minutiae(probe);
    let probe_count = probe_minutiae.len();

    #[cfg(feature = "parallel")]
    let mut matches: Vec<GalleryMatchCandidate> = gallery
        .par_iter()
        .map(|(path, ref_frame)| {
            let ref_m = extract_minutiae(ref_frame);
            let report = compare_minutiae_sets(&probe_minutiae, &ref_m);
            let matched = *report.score >= threshold;
            GalleryMatchCandidate {
                reference_path: path.clone(),
                score: report.score,
                matched,
                report,
            }
        })
        .collect();

    #[cfg(not(feature = "parallel"))]
    let mut matches: Vec<GalleryMatchCandidate> = gallery
        .iter()
        .map(|(path, ref_frame)| {
            let ref_m = extract_minutiae(ref_frame);
            let report = compare_minutiae_sets(&probe_minutiae, &ref_m);
            let matched = *report.score >= threshold;
            GalleryMatchCandidate {
                reference_path: path.clone(),
                score: report.score,
                matched,
                report,
            }
        })
        .collect();

    matches.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let best_match = matches.first().cloned();
    let threshold_met = best_match.as_ref().map(|b| b.matched).unwrap_or(false);

    GalleryMatchReport {
        probe_minutiae: probe_count,
        total_candidates: gallery.len(),
        matches,
        best_match,
        threshold_met,
    }
}

/// Matches a single probe frame concurrently against N candidate frames.
pub fn match_one_to_n(
    probe: &RawFrame,
    candidates: &[RawFrame],
    threshold: MatchScore,
) -> Vec<(usize, MatchScore)> {
    let probe_m = extract_minutiae(probe);

    #[cfg(feature = "parallel")]
    let iter = candidates.par_iter().enumerate();

    #[cfg(not(feature = "parallel"))]
    let iter = candidates.iter().enumerate();

    iter.filter_map(|(idx, candidate)| {
        let ref_m = extract_minutiae(candidate);
        let report = compare_minutiae_sets(&probe_m, &ref_m);
        if *report.score >= *threshold {
            Some((idx, report.score))
        } else {
            None
        }
    })
    .collect()
}

/// Exercises the deterministic minutia comparison against a logical 1:N pool.
/// The immutable template is shared, avoiding multi-gigabyte allocations for a
/// synthetic benchmark while retaining the matching work in each iteration.
pub fn benchmark_matching(frame: &RawFrame, templates: usize) -> usize {
    let minutiae = extract_minutiae(frame);
    #[cfg(feature = "parallel")]
    return (0..templates)
        .into_par_iter()
        .map(|_| compare_minutiae_sets(&minutiae, &minutiae).matched_minutiae)
        .sum();
    #[cfg(not(feature = "parallel"))]
    (0..templates)
        .map(|_| compare_minutiae_sets(&minutiae, &minutiae).matched_minutiae)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::FRAME_LEN;

    #[test]
    fn test_matcher_empty_frames_zero_minutiae() {
        let frame1 = RawFrame::new(vec![128u8; FRAME_LEN]).unwrap();
        let frame2 = RawFrame::new(vec![128u8; FRAME_LEN]).unwrap();
        let report = compare_frames(&frame1, &frame2);
        assert_eq!(report.score, 0.0);
        assert_eq!(report.probe_minutiae, 0);
        assert_eq!(report.reference_minutiae, 0);
        assert!(report.warnings.iter().any(|w| w.contains("few minutiae")));
    }

    #[test]
    fn test_matcher_identical_pattern_symmetry() {
        let mut pixels = vec![0u8; FRAME_LEN];
        for (i, p) in pixels.iter_mut().enumerate() {
            let x = (i % WIDTH as usize) as f32;
            let y = (i / WIDTH as usize) as f32;
            let val = ((x / 10.0).sin() * (y / 10.0).cos() * 100.0 + 128.0) as u8;
            *p = val;
        }
        let frame1 = RawFrame::new(pixels.clone()).unwrap();
        let frame2 = RawFrame::new(pixels).unwrap();

        let report1 = compare_frames(&frame1, &frame2);
        let report2 = compare_frames(&frame2, &frame1);

        assert_eq!(report1.score, 100.0);
        assert_eq!(report2.score, 100.0);
        assert_eq!(report1.matched_minutiae, report1.probe_minutiae);
    }

    #[test]
    fn test_matcher_gallery_ranking() {
        let mut pixels = vec![0u8; FRAME_LEN];
        for (i, p) in pixels.iter_mut().enumerate() {
            let x = (i % WIDTH as usize) as f32;
            let y = (i / WIDTH as usize) as f32;
            let val = ((x / 10.0).sin() * (y / 10.0).cos() * 100.0 + 128.0) as u8;
            *p = val;
        }
        let probe = RawFrame::new(pixels.clone()).unwrap();
        let exact_match = RawFrame::new(pixels).unwrap();
        let empty_frame = RawFrame::new(vec![128u8; FRAME_LEN]).unwrap();

        let gallery = vec![
            ("empty.png".to_string(), empty_frame),
            ("exact.png".to_string(), exact_match),
        ];

        let report = compare_probe_to_gallery(&probe, &gallery, 40.0);
        assert_eq!(report.total_candidates, 2);
        assert!(report.threshold_met);
        let best = report.best_match.expect("best candidate expected");
        assert_eq!(best.reference_path, "exact.png");
        assert_eq!(best.score, 100.0);
        assert!(best.matched);
    }

    #[test]
    fn test_matcher_one_to_n() {
        let mut pixels = vec![0u8; FRAME_LEN];
        for (i, p) in pixels.iter_mut().enumerate() {
            let x = (i % WIDTH as usize) as f32;
            let y = (i / WIDTH as usize) as f32;
            let val = ((x / 10.0).sin() * (y / 10.0).cos() * 100.0 + 128.0) as u8;
            *p = val;
        }
        let probe = RawFrame::new(pixels.clone()).unwrap();
        let exact_match = RawFrame::new(pixels).unwrap();
        let empty_frame = RawFrame::new(vec![128u8; FRAME_LEN]).unwrap();

        let candidates = vec![empty_frame, exact_match];
        let threshold = MatchScore::new(40.0).unwrap();
        let matches = match_one_to_n(&probe, &candidates, threshold);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, 1);
        assert_eq!(*matches[0].1, 100.0);
    }
}
