use fs80h_core::{
    RawFrame, annotate_frame, compare_frames_image, create_forensic_report, extract_minutiae,
    match_minutiae_pairs,
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

fn temporary_png(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "fs80hctl-forensic-{}-{name}.png",
        std::process::id()
    ))
}

#[test]
fn annotation_writes_a_valid_rgb_png() {
    let frame = RawFrame::from_file(fixture("probe.png")).expect("fixture frame");
    let image = annotate_frame(&frame, &extract_minutiae(&frame));
    let output = temporary_png("annotate");
    image.save(&output).expect("annotation output");
    let decoded = image::open(&output).expect("valid PNG").to_rgb8();
    assert_eq!(decoded.dimensions(), (320, 480));
    assert!(decoded.pixels().any(|pixel| pixel.0[0] != pixel.0[1]));
    let _ = std::fs::remove_file(output);
}

#[test]
fn comparison_writes_side_by_side_rgb_png() {
    let probe = RawFrame::from_file(fixture("probe.png")).expect("probe fixture");
    let reference = RawFrame::from_file(fixture("reference_match.png")).expect("reference fixture");
    let probe_minutiae = extract_minutiae(&probe);
    let reference_minutiae = extract_minutiae(&reference);
    let image = compare_frames_image(
        &probe,
        &reference,
        &probe_minutiae,
        &reference_minutiae,
        &match_minutiae_pairs(&probe_minutiae, &reference_minutiae),
    );
    let output = temporary_png("compare");
    image.save(&output).expect("comparison output");
    let decoded = image::open(&output).expect("valid PNG").to_rgb8();
    assert_eq!(decoded.dimensions(), (664, 480));
    let _ = std::fs::remove_file(output);
}

#[test]
fn report_serializes_required_json_schema() {
    let path = fixture("probe.png");
    let frame = RawFrame::from_file(&path).expect("fixture frame");
    let value: serde_json::Value =
        serde_json::to_value(create_forensic_report(&path, &frame)).expect("report serializes");
    assert_eq!(value["forensic_report_version"], "1.0");
    assert!(value["target_image"]["hashes"]["sha256"].is_string());
    assert!(value["target_image"]["hashes"]["blake3"].is_string());
    assert!(value["liveness_analysis"]["liveness_score"].is_u64());
    assert!(value["minutiae_summary"]["total_extracted"].is_u64());
}
