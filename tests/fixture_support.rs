use fs80h_core::{HEIGHT, RawFrame, WIDTH};
use std::f32::consts::PI;
use std::path::Path;

pub fn synthetic(angle_deg: f32, dx: f32, dy: f32, noisy: bool) -> RawFrame {
    let a = angle_deg * PI / 180.0;
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let phase = ((x as f32 - dx) * a.cos() + (y as f32 - dy) * a.sin()) / 7.0;
            let mut p = 128.0 + 70.0 * phase.sin();
            if noisy {
                let n = (((x * 31 + y * 17) % 37) as f32 - 18.0) * 5.0;
                p += n;
            }
            pixels.push(p.clamp(0.0, 255.0) as u8)
        }
    }
    RawFrame::new(pixels).unwrap()
}
pub fn write_set(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    synthetic(20.0, 0.0, 0.0, false)
        .write_png(dir.join("probe.png"))
        .unwrap();
    synthetic(20.0, 5.0, 4.0, false)
        .write_png(dir.join("reference_match.png"))
        .unwrap();
    synthetic(110.0, 0.0, 0.0, false)
        .write_png(dir.join("reference_mismatch.png"))
        .unwrap();
    synthetic(20.0, 0.0, 0.0, true)
        .write_png(dir.join("degraded_noisy.png"))
        .unwrap();
}
