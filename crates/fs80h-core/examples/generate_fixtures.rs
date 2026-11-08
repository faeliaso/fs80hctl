use fs80h_core::{HEIGHT, RawFrame, WIDTH};
use std::{f32::consts::PI, path::Path};

fn synthetic(angle_deg: f32, dx: f32, dy: f32, noisy: bool) -> RawFrame {
    let angle = angle_deg * PI / 180.0;
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let phase = ((x as f32 - dx) * angle.cos() + (y as f32 - dy) * angle.sin()) / 7.0;
            let mut pixel = 128.0 + 70.0 * phase.sin();
            if noisy {
                pixel += (((x * 31 + y * 17) % 37) as f32 - 18.0) * 5.0;
            }
            pixels.push(pixel.clamp(0.0, 255.0) as u8);
        }
    }
    RawFrame::new(pixels).expect("synthetic dimensions are fixed")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "tests/fixtures".into());
    let dir = Path::new(&output);
    std::fs::create_dir_all(dir)?;
    synthetic(20.0, 0.0, 0.0, false).write_png(dir.join("probe.png"))?;
    synthetic(20.0, 5.0, 4.0, false).write_png(dir.join("reference_match.png"))?;
    synthetic(110.0, 0.0, 0.0, false).write_png(dir.join("reference_mismatch.png"))?;
    synthetic(20.0, 0.0, 0.0, true).write_png(dir.join("degraded_noisy.png"))?;
    Ok(())
}
