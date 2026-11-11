use fs80h_core::{GrayscaleImage, extract_skeleton_minutiae};

#[test]
fn crossing_number_extracts_sparse_points_from_synthetic_ridges() {
    let mut pixels = vec![255; 64 * 64];
    for y in 8..56 {
        pixels[y * 64 + 32] = 0;
    }
    let image = GrayscaleImage::new(64, 64, pixels).unwrap();
    assert!(extract_skeleton_minutiae(&image).len() <= 2);
}

#[test]
fn extraction_rejects_candidates_near_the_partial_impression_boundary() {
    let mut pixels = vec![255; 96 * 96];
    // A short line ends inside the fixed boundary exclusion band. It must not
    // appear as a usable forensic minutia.
    for y in 4..48 {
        pixels[y * 96 + 16] = 0;
    }
    let image = GrayscaleImage::new(96, 96, pixels).unwrap();
    assert!(extract_skeleton_minutiae(&image).is_empty());
}

#[test]
#[ignore = "requires externally supplied public dataset via SAMPLES_DIR"]
fn public_dataset_tiff_smoke_test() {
    let root = std::env::var("SAMPLES_DIR").expect("SAMPLES_DIR is required");
    let image =
        GrayscaleImage::from_tiff(std::path::Path::new(&root).join("DB1/101_1.tif")).unwrap();
    assert_eq!((image.width(), image.height()), (388, 374));
    assert!(!extract_skeleton_minutiae(&image).is_empty());
}
