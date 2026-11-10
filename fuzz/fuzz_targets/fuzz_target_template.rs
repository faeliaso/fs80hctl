#![no_main]

use libfuzzer_sys::fuzz_target;
use fs80h_core::{
    evaluate_presence, evaluate_quality, extract_minutiae, GrayscaleImage, QualityScore, RawFrame,
    WIDTH, HEIGHT,
};

fuzz_target!(|data: &[u8]| {
    // Fuzz PNG decoding
    let _ = RawFrame::from_png_bytes(data);

    // Fuzz PGM decoding
    let _ = RawFrame::from_pgm_bytes(data);

    // Fuzz GrayscaleImage constructor
    let _ = GrayscaleImage::new(WIDTH as usize, HEIGHT as usize, data.to_vec());

    // Fuzz quality and minutiae processing if valid frame size
    if let Ok(frame) = RawFrame::new(data.to_vec()) {
        let _ = evaluate_quality(&frame);
        let _ = evaluate_presence(&frame);
        let _ = extract_minutiae(&frame);
    }

    if let Some(&first) = data.first() {
        let _ = QualityScore::new(first);
    }
});
