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
