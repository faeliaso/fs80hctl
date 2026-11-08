mod fixture_support;
use fixture_support::*;
use fs80h_core::{RawFrame, compare_frames, evaluate_quality};

#[test]
fn deterministic_fixture_pipeline_without_hardware() {
    let dir = std::env::temp_dir().join(format!("fs80hctl-fixtures-{}", std::process::id()));
    write_set(&dir);
    let probe = RawFrame::from_png(dir.join("probe.png")).unwrap();
    let matching = RawFrame::from_png(dir.join("reference_match.png")).unwrap();
    let mismatch = RawFrame::from_png(dir.join("reference_mismatch.png")).unwrap();
    let degraded = RawFrame::from_png(dir.join("degraded_noisy.png")).unwrap();
    let a = compare_frames(&probe, &matching);
    let b = compare_frames(&probe, &matching);
    assert_eq!(a.score, b.score);
    assert!(a.score >= 40.0, "match score {}", a.score);
    assert!(compare_frames(&probe, &mismatch).score < 40.0);
    assert!(!a.warnings.is_empty());
    assert!(
        !evaluate_quality(&degraded).accepted
            || evaluate_quality(&degraded).score < evaluate_quality(&probe).score
    );
}

#[test]
fn frame_hash_is_stable() {
    let frame = synthetic(20.0, 0.0, 0.0, false);
    assert_eq!(frame.sha256(), synthetic(20.0, 0.0, 0.0, false).sha256());
}
