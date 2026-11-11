use fs80h_core::{
    RawFrame, benchmark_matching, decode_wsq, encode_wsq, enhance_gabor, evaluate_quality,
};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("tests/fixtures")
        .join(name)
}

fn correlation(left: &[u8], right: &[u8]) -> f64 {
    let count = left.len() as f64;
    let left_mean = left.iter().map(|&value| value as f64).sum::<f64>() / count;
    let right_mean = right.iter().map(|&value| value as f64).sum::<f64>() / count;
    let (numerator, left_sum, right_sum) =
        left.iter()
            .zip(right)
            .fold((0.0, 0.0, 0.0), |(n, lx, rx), (&a, &b)| {
                let da = a as f64 - left_mean;
                let db = b as f64 - right_mean;
                (n + da * db, lx + da * da, rx + db * db)
            });
    numerator / (left_sum * right_sum).sqrt()
}

#[test]
fn png_wsq_png_round_trip_preserves_structure() {
    let original = RawFrame::from_file(fixture("probe.png")).expect("probe fixture");
    let wsq = encode_wsq(&original, 0.75, 500).expect("encode WSQ");
    let recovered = decode_wsq(&wsq).expect("decode WSQ");
    assert_eq!(recovered.pixels().len(), original.pixels().len());
    assert!(correlation(original.pixels(), recovered.pixels()) > 0.95);
}

#[test]
fn gabor_enhancement_increases_quality_score_for_degraded_fixture() {
    let input = RawFrame::from_file(fixture("degraded_noisy.png")).expect("degraded fixture");
    let enhanced = enhance_gabor(&input);
    assert_eq!(enhanced.pixels().len(), input.pixels().len());
    assert!(evaluate_quality(&enhanced).score > evaluate_quality(&input).score);
}

#[test]
fn matching_benchmark_small_pool_completes() {
    let frame = RawFrame::from_file(fixture("probe.png")).expect("probe fixture");
    assert!(benchmark_matching(&frame, 1_000) > 0);
}
