use crate::domain::{HEIGHT, RawFrame, WIDTH};
use serde::Serialize;
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Minutia {
    pub x: f32,
    pub y: f32,
    pub angle_deg: f32,
    pub quality: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct MatchReport {
    pub score: f32,
    pub matched_minutiae: usize,
    pub probe_minutiae: usize,
    pub reference_minutiae: usize,
    pub warnings: Vec<String>,
}

pub fn extract_minutiae(frame: &RawFrame) -> Vec<Minutia> {
    let mut out = Vec::new();
    for y in (16..HEIGHT as usize - 16).step_by(16) {
        for x in (16..WIDTH as usize - 16).step_by(16) {
            let mut sum = 0.0;
            let mut sum_sq = 0.0;
            let mut gx = 0.0;
            let mut gy = 0.0;
            let mut n = 0.0;
            for yy in y - 8..y + 8 {
                for xx in x - 8..x + 8 {
                    let p = frame.pixels()[yy * WIDTH as usize + xx] as f32;
                    sum += p;
                    sum_sq += p * p;
                    gx += frame.pixels()[yy * WIDTH as usize + xx + 1] as f32
                        - frame.pixels()[yy * WIDTH as usize + xx - 1] as f32;
                    gy += frame.pixels()[(yy + 1) * WIDTH as usize + xx] as f32
                        - frame.pixels()[(yy - 1) * WIDTH as usize + xx] as f32;
                    n += 1.0;
                }
            }
            let variance = (sum_sq / n - (sum / n).powi(2)).max(0.0);
            if variance >= 80.0 && (gx.abs() + gy.abs()) > 50.0 {
                let angle = (gy.atan2(gx) + PI / 2.0) * 180.0 / PI;
                out.push(Minutia {
                    x: x as f32,
                    y: y as f32,
                    angle_deg: angle.rem_euclid(180.0),
                    quality: (variance / 500.0).clamp(0.0, 1.0),
                });
            }
        }
    }
    out
}
fn angle_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs().rem_euclid(180.0);
    d.min(180.0 - d)
}
pub fn compare_frames(probe: &RawFrame, reference: &RawFrame) -> MatchReport {
    let p = extract_minutiae(probe);
    let r = extract_minutiae(reference);
    let mut used = vec![false; r.len()];
    let mut matched = 0;
    for a in &p {
        if let Some((i, _)) = r
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                !used[*i]
                    && ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt() <= 15.0
                    && angle_distance(a.angle_deg, b.angle_deg) <= 25.0
            })
            .min_by(|(_, x), (_, y)| {
                let dx = (a.x - x.x).powi(2) + (a.y - x.y).powi(2);
                let dy = (a.x - y.x).powi(2) + (a.y - y.y).powi(2);
                dx.partial_cmp(&dy).unwrap()
            })
        {
            used[i] = true;
            matched += 1;
        }
    }
    let denom = p.len().max(r.len());
    let score = if denom == 0 {
        0.0
    } else {
        matched as f32 / denom as f32 * 100.0
    };
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
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("few minutiae"))
        );
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
}
