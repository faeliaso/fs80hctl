use crate::domain::{MatchScore, RawFrame};
use crate::minutiae::extract_enhanced_minutiae;
use crate::quality::{QualityReport, evaluate_quality};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Lowest correspondence support accepted by the forensic visual comparator.
pub const MIN_FORENSIC_PAIR_SUPPORT: usize = 6;

/// Minimum absolute number of matched minutiae required for biometric verification.
pub const MIN_MATCHED_PAIRS: usize = 10;

const MIN_SEGMENT_LEN: f32 = 20.0;
const SEGMENT_LEN_TOLERANCE: f32 = 4.0;
const MAX_SCALE_DEVIATION: f32 = 0.12;
const SEGMENT_ANGLE_TOLERANCE_DEG: f32 = 25.0;
const TOLERANCE_BOX_RADIUS: f32 = 14.0;
const TOLERANCE_BOX_ANGLE_DEG: f32 = 20.0;
const POSE_CLUSTER_TOP_K: usize = 10;
const POSE_CLUSTER_TRANS_TOL: f32 = 14.0;
const POSE_CLUSTER_ANGLE_TOL: f32 = 10.0;
const MIN_CONSISTENCY_RATIO: f32 = 0.5;
const MAX_SHEAR_LOG: f32 = 0.35;

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
    pub alignment_rotation_deg: f32,
    pub alignment_translation: (f32, f32),
    pub consistency_ratio: f32,
    pub is_verified: bool,
    pub warnings: Vec<String>,
}

/// Extracts minutiae from a [`RawFrame`] after running the capture quality gatekeeper,
/// contextual Gabor enhancement, Zhang-Suen thinning, and Crossing Number extraction.
pub fn extract_minutiae(frame: &RawFrame) -> Vec<Minutia> {
    extract_minutiae_checked(frame).unwrap_or_default()
}

/// Evaluates frame quality first and returns either extracted minutiae or the
/// rejected [`QualityReport`] for recapture diagnostics.
pub fn extract_minutiae_checked(frame: &RawFrame) -> Result<Vec<Minutia>, QualityReport> {
    let quality = evaluate_quality(frame);
    if !quality.accepted {
        return Err(quality);
    }
    Ok(extract_enhanced_minutiae(frame.as_image()))
}

fn angle_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs().rem_euclid(360.0);
    d.min(360.0 - d)
}

fn signed_angle_diff_deg(target: f32, source: f32) -> f32 {
    (target - source + 540.0).rem_euclid(360.0) - 180.0
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    len: f32,
    angle_deg: f32,
    rel_a1: f32,
    rel_a2: f32,
    mid_x: f32,
    mid_y: f32,
}

fn build_segments(minutiae: &[Minutia]) -> Vec<Segment> {
    let mut segs = Vec::new();
    for (i1, m1) in minutiae.iter().enumerate() {
        for (i2, m2) in minutiae.iter().enumerate() {
            if i1 == i2 {
                continue;
            }
            let dx = m2.x - m1.x;
            let dy = m2.y - m1.y;
            let len = dx.hypot(dy);
            if len < MIN_SEGMENT_LEN {
                continue;
            }
            let angle_deg = dy.atan2(dx).to_degrees().rem_euclid(360.0);
            segs.push(Segment {
                len,
                angle_deg,
                rel_a1: signed_angle_diff_deg(m1.angle_deg, angle_deg),
                rel_a2: signed_angle_diff_deg(m2.angle_deg, angle_deg),
                mid_x: 0.5 * (m1.x + m2.x),
                mid_y: 0.5 * (m1.y + m2.y),
            });
        }
    }
    segs.sort_by(|a, b| {
        a.len
            .partial_cmp(&b.len)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    segs
}

#[derive(Clone, Debug)]
struct AlignmentHypothesis {
    dtheta_deg: f32,
    tx: f32,
    ty: f32,
    pairs: Vec<(usize, usize)>,
    mean_dist: f32,
}

fn transform_minutiae(probe: &[Minutia], dtheta_deg: f32, tx: f32, ty: f32) -> Vec<Minutia> {
    let (sin, cos) = dtheta_deg.to_radians().sin_cos();
    probe
        .iter()
        .map(|m| Minutia {
            x: m.x * cos - m.y * sin + tx,
            y: m.x * sin + m.y * cos + ty,
            angle_deg: (m.angle_deg + dtheta_deg).rem_euclid(360.0),
            ..*m
        })
        .collect()
}

/// Returns deterministic one-to-one homologous minutia indices using tolerance boxes.
pub fn match_minutiae_pairs(probe: &[Minutia], reference: &[Minutia]) -> Vec<(usize, usize)> {
    match_minutiae_pairs_with_error(probe, reference).0
}

fn match_minutiae_pairs_with_error(
    probe: &[Minutia],
    reference: &[Minutia],
) -> (Vec<(usize, usize)>, f32) {
    let mut used = vec![false; reference.len()];
    let mut pairs = Vec::new();
    let mut dist_sum = 0.0f32;
    let r2_limit = TOLERANCE_BOX_RADIUS * TOLERANCE_BOX_RADIUS;

    for (probe_index, a) in probe.iter().enumerate() {
        if let Some((reference_index, d2)) = reference
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                if used[i] {
                    return None;
                }
                let d2 = (a.x - b.x).powi(2) + (a.y - b.y).powi(2);
                if d2 <= r2_limit
                    && angle_distance(a.angle_deg, b.angle_deg) <= TOLERANCE_BOX_ANGLE_DEG
                {
                    // Prefer same-kind matches, while tolerating RidgeEnding<->Bifurcation
                    // flips caused by finger pressure differences on optical sensors.
                    let kind_penalty = if a.kind == b.kind { 0.0 } else { 16.0 };
                    Some((i, d2 + kind_penalty))
                } else {
                    None
                }
            })
            .min_by(|(_, d2_x), (_, d2_y)| {
                d2_x.partial_cmp(d2_y).unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            used[reference_index] = true;
            pairs.push((probe_index, reference_index));
            dist_sum += d2.sqrt();
        }
    }

    let mean_dist = if pairs.is_empty() {
        f32::INFINITY
    } else {
        dist_sum / pairs.len() as f32
    };
    (pairs, mean_dist)
}

/// Estimates 2x2 affine singular values (s1, s2) from matched pairs and checks
/// whether scale and shear remain within physically plausible skin deformation bounds.
fn check_affine_deformation(
    aligned_probe: &[Minutia],
    reference: &[Minutia],
    pairs: &[(usize, usize)],
) -> bool {
    if pairs.len() < 6 {
        return true;
    }

    let n = pairs.len() as f64;
    let (mut pmx, mut pmy, mut rmx, mut rmy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for &(pi, ri) in pairs {
        pmx += aligned_probe[pi].x as f64;
        pmy += aligned_probe[pi].y as f64;
        rmx += reference[ri].x as f64;
        rmy += reference[ri].y as f64;
    }
    pmx /= n;
    pmy /= n;
    rmx /= n;
    rmy /= n;

    // Solve least-squares 2x2 affine A: [r_c] = A * [p_c]
    let (mut s_xx, mut s_xy, mut s_yy) = (0.0f64, 0.0f64, 0.0f64);
    let (mut t_xx, mut t_xy, mut t_yx, mut t_yy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for &(pi, ri) in pairs {
        let px = aligned_probe[pi].x as f64 - pmx;
        let py = aligned_probe[pi].y as f64 - pmy;
        let rx = reference[ri].x as f64 - rmx;
        let ry = reference[ri].y as f64 - rmy;
        s_xx += px * px;
        s_xy += px * py;
        s_yy += py * py;
        t_xx += rx * px;
        t_xy += rx * py;
        t_yx += ry * px;
        t_yy += ry * py;
    }

    let det_p = s_xx * s_yy - s_xy * s_xy;
    if det_p.abs() < 1e-3 {
        // Collinear points cannot support a 2D rigid match reliably
        return false;
    }

    let inv_xx = s_yy / det_p;
    let inv_xy = -s_xy / det_p;
    let inv_yy = s_xx / det_p;

    let a11 = t_xx * inv_xx + t_xy * inv_xy;
    let a12 = t_xx * inv_xy + t_xy * inv_yy;
    let a21 = t_yx * inv_xx + t_yy * inv_xy;
    let a22 = t_yx * inv_xy + t_yy * inv_yy;

    // Singular values of 2x2 matrix A via eigenvalues of A^T * A
    let m11 = a11 * a11 + a21 * a21;
    let m12 = a11 * a12 + a21 * a22;
    let m22 = a12 * a12 + a22 * a22;
    let trace = m11 + m22;
    let disc = ((m11 - m22).powi(2) + 4.0 * m12 * m12).sqrt();
    let lambda1 = ((trace + disc) * 0.5).max(0.0);
    let lambda2 = ((trace - disc) * 0.5).max(0.0);
    let s1 = lambda1.sqrt();
    let s2 = lambda2.sqrt();

    if s2 < 0.70 || s1 > 1.35 {
        return false;
    }
    let scale_geo = (s1 * s2).sqrt();
    if !(0.85..=1.18).contains(&scale_geo) {
        return false;
    }
    let shear = (s1 / s2.max(1e-6)).ln().abs() as f32;
    shear <= MAX_SHEAR_LOG
}

fn evaluate_alignment_hypotheses(
    probe: &[Minutia],
    reference: &[Minutia],
) -> Vec<AlignmentHypothesis> {
    if probe.len() < 2 || reference.len() < 2 {
        return Vec::new();
    }

    let p_segs = build_segments(probe);
    let r_segs = build_segments(reference);
    if p_segs.is_empty() || r_segs.is_empty() {
        return Vec::new();
    }

    // Stage 1: Accumulate all compatible segment-pair pose votes into coarse Hough bins
    // so no valid alignment is ever dropped by an early loop cutoff.
    use std::collections::HashMap;
    #[derive(Clone, Copy)]
    struct RawPose {
        dtheta_deg: f32,
        tx: f32,
        ty: f32,
    }
    let mut bins: HashMap<(i32, i32, i32), Vec<RawPose>> = HashMap::new();

    for ps in p_segs.iter().rev() {
        let min_len = ps.len - SEGMENT_LEN_TOLERANCE;
        let max_len = ps.len + SEGMENT_LEN_TOLERANCE;
        let start = r_segs.partition_point(|rs| rs.len < min_len);

        for rs in &r_segs[start..] {
            if rs.len > max_len {
                break;
            }
            let scale = rs.len / ps.len;
            if (scale - 1.0).abs() > MAX_SCALE_DEVIATION {
                continue;
            }
            if angle_distance(ps.rel_a1, rs.rel_a1) > SEGMENT_ANGLE_TOLERANCE_DEG
                || angle_distance(ps.rel_a2, rs.rel_a2) > SEGMENT_ANGLE_TOLERANCE_DEG
            {
                continue;
            }

            let dtheta_deg = signed_angle_diff_deg(rs.angle_deg, ps.angle_deg);
            let (sin, cos) = dtheta_deg.to_radians().sin_cos();
            let rot_mx = ps.mid_x * cos - ps.mid_y * sin;
            let rot_my = ps.mid_x * sin + ps.mid_y * cos;
            let tx = rs.mid_x - rot_mx;
            let ty = rs.mid_y - rot_my;

            let bin_key = (
                (tx / 12.0).round() as i32,
                (ty / 12.0).round() as i32,
                (dtheta_deg / 10.0).round() as i32,
            );
            bins.entry(bin_key)
                .or_default()
                .push(RawPose { dtheta_deg, tx, ty });
        }
    }

    // Stage 2: Rank coarse bins by vote count and evaluate up to 16 poses from each
    // of the top 24 bins deterministically.
    let mut ranked_bins: Vec<((i32, i32, i32), Vec<RawPose>)> = bins.into_iter().collect();
    ranked_bins.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));

    let mut hypotheses = Vec::new();
    for (_, poses) in ranked_bins.into_iter().take(24) {
        for pose in poses.into_iter().take(16) {
            let aligned = transform_minutiae(probe, pose.dtheta_deg, pose.tx, pose.ty);
            let (pairs, mean_dist) = match_minutiae_pairs_with_error(&aligned, reference);
            if !pairs.is_empty() {
                hypotheses.push(AlignmentHypothesis {
                    dtheta_deg: pose.dtheta_deg,
                    tx: pose.tx,
                    ty: pose.ty,
                    pairs,
                    mean_dist,
                });
            }
        }
    }

    hypotheses.sort_by(|a, b| {
        b.pairs.len().cmp(&a.pairs.len()).then_with(|| {
            a.mean_dist
                .partial_cmp(&b.mean_dist)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    hypotheses
}

/// Deterministically selects the rigid alignment hypothesis (full 360° rotation + translation)
/// with the most geometrically consistent one-to-one pairs.
pub fn match_minutiae_pairs_aligned(
    probe: &[Minutia],
    reference: &[Minutia],
) -> Vec<(usize, usize)> {
    evaluate_alignment_hypotheses(probe, reference)
        .into_iter()
        .next()
        .map(|h| h.pairs)
        .unwrap_or_default()
}

fn count_overlap_minutiae(aligned_probe: &[Minutia], reference: &[Minutia]) -> (usize, usize) {
    if aligned_probe.is_empty() || reference.is_empty() {
        return (0, 0);
    }

    let bbox = |pts: &[Minutia]| -> (f32, f32, f32, f32) {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for m in pts {
            min_x = min_x.min(m.x);
            min_y = min_y.min(m.y);
            max_x = max_x.max(m.x);
            max_y = max_y.max(m.y);
        }
        (min_x, min_y, max_x, max_y)
    };

    let (px0, py0, px1, py1) = bbox(aligned_probe);
    let (rx0, ry0, rx1, ry1) = bbox(reference);

    let ox0 = px0.max(rx0) - TOLERANCE_BOX_RADIUS;
    let oy0 = py0.max(ry0) - TOLERANCE_BOX_RADIUS;
    let ox1 = px1.min(rx1) + TOLERANCE_BOX_RADIUS;
    let oy1 = py1.min(ry1) + TOLERANCE_BOX_RADIUS;

    if ox0 > ox1 || oy0 > oy1 {
        return (0, 0);
    }

    let ov_p = aligned_probe
        .iter()
        .filter(|m| m.x >= ox0 && m.x <= ox1 && m.y >= oy0 && m.y <= oy1)
        .count();
    let ov_r = reference
        .iter()
        .filter(|m| m.x >= ox0 && m.x <= ox1 && m.y >= oy0 && m.y <= oy1)
        .count();
    (ov_p, ov_r)
}

pub fn compare_minutiae_sets(p: &[Minutia], r: &[Minutia]) -> MatchReport {
    let mut warnings = vec![
        "result is a technical similarity indicator; not civil identification, forensic expert report, or liveness verification".into(),
    ];
    if p.len() < MIN_MATCHED_PAIRS || r.len() < MIN_MATCHED_PAIRS {
        warnings.push("few minutiae extracted; recapture recommended".into());
    }

    let hypotheses = evaluate_alignment_hypotheses(p, r);
    let Some(best) = hypotheses.first() else {
        return MatchReport {
            score: MatchScore::new(0.0).unwrap(),
            matched_minutiae: 0,
            probe_minutiae: p.len(),
            reference_minutiae: r.len(),
            alignment_rotation_deg: 0.0,
            alignment_translation: (0.0, 0.0),
            consistency_ratio: 0.0,
            is_verified: false,
            warnings,
        };
    };

    let top_k = hypotheses.len().min(POSE_CLUSTER_TOP_K);
    let consistent_count = hypotheses[..top_k]
        .iter()
        .filter(|h| {
            let trans_dist = (h.tx - best.tx).hypot(h.ty - best.ty);
            let rot_dist = angle_distance(h.dtheta_deg, best.dtheta_deg);
            trans_dist <= POSE_CLUSTER_TRANS_TOL && rot_dist <= POSE_CLUSTER_ANGLE_TOL
        })
        .count();
    let consistency_ratio = consistent_count as f32 / top_k.max(1) as f32;

    let aligned_probe = transform_minutiae(p, best.dtheta_deg, best.tx, best.ty);
    let deformation_valid = check_affine_deformation(&aligned_probe, r, &best.pairs);

    let matched = best.pairs.len();
    let (ov_p, ov_r) = count_overlap_minutiae(&aligned_probe, r);
    let denom = ((ov_p.max(matched).max(1) as f32) * (ov_r.max(matched).max(1) as f32)).sqrt();

    let q_pair = if matched == 0 {
        0.0
    } else {
        best.pairs
            .iter()
            .map(|&(pi, ri)| 0.5 * (p[pi].quality + r[ri].quality))
            .sum::<f32>()
            / matched as f32
    }
    .clamp(0.0, 1.0);

    let mut raw_score = (matched as f32 / denom) * q_pair * consistency_ratio * 100.0;

    if matched < MIN_MATCHED_PAIRS {
        let support_penalty = (matched as f32 / MIN_MATCHED_PAIRS as f32).powi(2);
        raw_score *= support_penalty;
        warnings.push(format!(
            "insufficient matched minutiae ({matched} < {MIN_MATCHED_PAIRS}); match rejected"
        ));
    }
    if consistency_ratio < MIN_CONSISTENCY_RATIO {
        warnings.push(
            "inconsistent geometric alignment across pose clusters; possible impostor".into(),
        );
    }
    if !deformation_valid {
        raw_score *= 0.25;
        warnings.push("excessive non-linear shear/scale deformation; match rejected".into());
    }

    let is_verified = matched >= MIN_MATCHED_PAIRS
        && consistency_ratio >= MIN_CONSISTENCY_RATIO
        && deformation_valid
        && raw_score >= 30.0;

    let score = MatchScore::new(raw_score.clamp(0.0, 100.0))
        .unwrap_or_else(|_| MatchScore::new(0.0).unwrap());

    MatchReport {
        score,
        matched_minutiae: matched,
        probe_minutiae: p.len(),
        reference_minutiae: r.len(),
        alignment_rotation_deg: best.dtheta_deg,
        alignment_translation: (best.tx, best.ty),
        consistency_ratio,
        is_verified,
        warnings,
    }
}

pub fn compare_frames(probe: &RawFrame, reference: &RawFrame) -> MatchReport {
    let mut gatekeeper_warnings = Vec::new();
    let p = match extract_minutiae_checked(probe) {
        Ok(m) => m,
        Err(q) => {
            gatekeeper_warnings.extend(q.warnings.into_iter().map(|w| format!("probe: {w}")));
            Vec::new()
        }
    };
    let r = match extract_minutiae_checked(reference) {
        Ok(m) => m,
        Err(q) => {
            gatekeeper_warnings.extend(q.warnings.into_iter().map(|w| format!("reference: {w}")));
            Vec::new()
        }
    };

    let mut report = compare_minutiae_sets(&p, &r);
    if !gatekeeper_warnings.is_empty() {
        report.is_verified = false;
        report.warnings.extend(gatekeeper_warnings);
    }
    report
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
            let matched = report.is_verified && *report.score >= threshold;
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
            let matched = report.is_verified && *report.score >= threshold;
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
        if report.is_verified && *report.score >= *threshold {
            Some((idx, report.score))
        } else {
            None
        }
    })
    .collect()
}

/// Exercises the deterministic minutia comparison against a logical 1:N pool.
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
    use crate::domain::{FRAME_LEN, HEIGHT, WIDTH};

    /// Generates a deterministic synthetic fingerprint-like ridge field with
    /// multiple ridge endings and bifurcations (core + arch + phase dislocations).
    fn synthetic_fingerprint(seed_offset: f32) -> RawFrame {
        let w = WIDTH as usize;
        let h = HEIGHT as usize;
        let mut pixels = vec![245u8; FRAME_LEN];
        let cx = 150.0f32;
        let cy = 200.0f32;

        // Dislocation centers that create real ridge endings and bifurcations
        let dislocations: [(f32, f32, f32); 22] = [
            (90.0, 85.0, 1.0),
            (145.0, 80.0, -1.0),
            (200.0, 90.0, 1.0),
            (80.0, 125.0, -1.0),
            (130.0, 120.0, 1.0),
            (175.0, 125.0, -1.0),
            (215.0, 130.0, 1.0),
            (85.0, 165.0, 1.0),
            (135.0, 165.0, -1.0),
            (185.0, 170.0, 1.0),
            (220.0, 175.0, -1.0),
            (80.0, 210.0, -1.0),
            (130.0, 215.0, 1.0),
            (180.0, 215.0, -1.0),
            (220.0, 220.0, 1.0),
            (85.0, 255.0, 1.0),
            (135.0, 260.0, -1.0),
            (185.0, 260.0, 1.0),
            (215.0, 265.0, -1.0),
            (100.0, 300.0, -1.0),
            (150.0, 305.0, 1.0),
            (195.0, 300.0, -1.0),
        ];

        for y in 0..h {
            for x in 0..w {
                let dx = (x as f32 - cx) / 135.0;
                let dy = (y as f32 - cy) / 180.0;
                let r2 = dx * dx + dy * dy;
                if r2 > 0.95 {
                    continue;
                }
                let mut phase = (x as f32 * 0.74 + y as f32 * 0.26 + seed_offset) * 1.0
                    + 2.2 * (dx * dx - dy * dy);
                for &(px, py, sign) in &dislocations {
                    let vx = x as f32 - (px + seed_offset * 0.3);
                    let vy = y as f32 - py;
                    phase += sign * vy.atan2(vx);
                }
                let env = ((0.95 - r2) * 5.0).clamp(0.0, 1.0);
                let val = 128.0 + 85.0 * env * phase.sin();
                pixels[y * w + x] = val.clamp(10.0, 245.0) as u8;
            }
        }
        RawFrame::new(pixels).unwrap()
    }

    fn transform_minutia_set(set: &[Minutia], angle_deg: f32, dx: f32, dy: f32) -> Vec<Minutia> {
        let (sin, cos) = angle_deg.to_radians().sin_cos();
        set.iter()
            .map(|m| Minutia {
                x: m.x * cos - m.y * sin + dx,
                y: m.x * sin + m.y * cos + dy,
                angle_deg: (m.angle_deg + angle_deg).rem_euclid(360.0),
                ..*m
            })
            .collect()
    }

    #[test]
    fn test_matcher_empty_frames_zero_minutiae() {
        let frame1 = RawFrame::new(vec![128u8; FRAME_LEN]).unwrap();
        let frame2 = RawFrame::new(vec![128u8; FRAME_LEN]).unwrap();
        let report = compare_frames(&frame1, &frame2);
        assert_eq!(report.score, 0.0);
        assert!(!report.is_verified);
        assert_eq!(report.probe_minutiae, 0);
        assert_eq!(report.reference_minutiae, 0);
        assert!(report.warnings.iter().any(|w| w.contains("few minutiae")));
    }

    #[test]
    fn test_matcher_identical_pattern_symmetry() {
        let frame1 = synthetic_fingerprint(0.0);
        let frame2 = synthetic_fingerprint(0.0);

        let report1 = compare_frames(&frame1, &frame2);
        let report2 = compare_frames(&frame2, &frame1);

        assert!(report1.is_verified, "warnings: {:?}", report1.warnings);
        assert!(report1.score >= 70.0, "score: {}", report1.score);
        assert_eq!(report1.score, report2.score);
        assert_eq!(report1.matched_minutiae, report1.probe_minutiae);
    }

    #[test]
    fn test_matcher_rotation_45_and_90_with_translation() {
        let frame = synthetic_fingerprint(0.0);
        let base_minutiae = extract_minutiae(&frame);
        assert!(
            base_minutiae.len() >= MIN_MATCHED_PAIRS,
            "expected >= {MIN_MATCHED_PAIRS} minutiae, got {}",
            base_minutiae.len()
        );

        for (rot, dx, dy) in [(45.0f32, 32.0f32, -18.0f32), (90.0f32, -24.0f32, 41.0f32)] {
            let rotated = transform_minutia_set(&base_minutiae, rot, dx, dy);
            let report = compare_minutiae_sets(&rotated, &base_minutiae);
            assert!(
                report.is_verified,
                "rot {rot}° failed verification: score={}, matched={}, consistency={}, warnings={:?}",
                report.score, report.matched_minutiae, report.consistency_ratio, report.warnings
            );
            assert!(
                report.score >= 65.0,
                "rot {rot}° expected high score, got {}",
                report.score
            );
            assert!(
                report.consistency_ratio >= 0.5,
                "rot {rot}° consistency {}",
                report.consistency_ratio
            );
        }
    }

    #[test]
    fn test_matcher_rejects_impostor_with_coincidental_points() {
        // Build two distinct minutia sets that share a few coincidental points
        // under random local alignments, but lack global geometric consistency.
        let genuine: Vec<Minutia> = (0..22)
            .map(|i| Minutia {
                x: 40.0 + (i % 5) as f32 * 45.0,
                y: 50.0 + (i / 5) as f32 * 60.0,
                angle_deg: (i as f32 * 37.0).rem_euclid(360.0),
                quality: 0.85,
                kind: if i % 2 == 0 {
                    MinutiaKind::RidgeEnding
                } else {
                    MinutiaKind::Bifurcation
                },
            })
            .collect();

        let impostor: Vec<Minutia> = (0..22)
            .map(|i| Minutia {
                x: 55.0 + ((i * 73) % 210) as f32,
                y: 45.0 + ((i * 109) % 290) as f32,
                angle_deg: ((i as f32 * 83.0) + 19.0).rem_euclid(360.0),
                quality: 0.85,
                kind: if i % 3 == 0 {
                    MinutiaKind::RidgeEnding
                } else {
                    MinutiaKind::Bifurcation
                },
            })
            .collect();

        let report = compare_minutiae_sets(&impostor, &genuine);
        assert!(
            !report.is_verified,
            "impostor must be rejected: score={}, matched={}, consistency={}",
            report.score, report.matched_minutiae, report.consistency_ratio
        );
        assert!(
            report.score < 35.0,
            "impostor score too high: {}",
            report.score
        );
    }

    #[test]
    fn test_matcher_rejects_low_quality_and_few_minutiae() {
        let low_quality = RawFrame::new(vec![200u8; FRAME_LEN]).unwrap();
        let good = synthetic_fingerprint(0.0);
        let report = compare_frames(&low_quality, &good);
        assert!(!report.is_verified);
        assert_eq!(report.score, 0.0);
        assert!(report.warnings.iter().any(|w| w.contains("recapture")));

        // Fewer than 10 minutiae must also be rejected even if identical
        let tiny_set: Vec<Minutia> = (0..6)
            .map(|i| Minutia {
                x: 60.0 + i as f32 * 30.0,
                y: 120.0 + (i % 2) as f32 * 25.0,
                angle_deg: 45.0,
                quality: 0.9,
                kind: MinutiaKind::RidgeEnding,
            })
            .collect();
        let tiny_report = compare_minutiae_sets(&tiny_set, &tiny_set);
        assert!(!tiny_report.is_verified);
        assert!(
            tiny_report
                .warnings
                .iter()
                .any(|w| w.contains("few minutiae") || w.contains("recapture"))
        );
    }

    #[test]
    fn test_matcher_gallery_ranking() {
        let probe = synthetic_fingerprint(0.0);
        let exact_match = synthetic_fingerprint(0.0);
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
        assert!(best.score >= 70.0);
        assert!(best.matched);
    }

    #[test]
    fn test_matcher_one_to_n() {
        let probe = synthetic_fingerprint(0.0);
        let exact_match = synthetic_fingerprint(0.0);
        let empty_frame = RawFrame::new(vec![128u8; FRAME_LEN]).unwrap();

        let candidates = vec![empty_frame, exact_match];
        let threshold = MatchScore::new(40.0).unwrap();
        let matches = match_one_to_n(&probe, &candidates, threshold);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, 1);
        assert!(*matches[0].1 >= 70.0);
    }
}
