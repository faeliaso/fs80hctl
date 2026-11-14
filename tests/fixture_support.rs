use fs80h_core::{HEIGHT, RawFrame, WIDTH};
use std::path::Path;

pub fn synthetic(seed_offset: f32, dx_shift: f32, dy_shift: f32, noisy: bool) -> RawFrame {
    let w = WIDTH as usize;
    let h = HEIGHT as usize;
    let mut pixels = vec![245u8; w * h];
    let cx = 150.0f32 + dx_shift;
    let cy = 200.0f32 + dy_shift;

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
            let lx = x as f32 - dx_shift;
            let ly = y as f32 - dy_shift;
            let dx = (x as f32 - cx) / 135.0;
            let dy = (y as f32 - cy) / 180.0;
            let r2 = dx * dx + dy * dy;
            if r2 > 0.95 {
                continue;
            }
            let is_impostor = seed_offset.abs() > 0.1;
            let mut phase = if is_impostor {
                (lx * 0.34 - ly * 0.86 + seed_offset) * 1.0 - 2.6 * (dx * dx + 0.5 * dy * dy)
            } else {
                (lx * 0.74 + ly * 0.26 + seed_offset) * 1.0 + 2.2 * (dx * dx - dy * dy)
            };
            for (idx, &(px, py, sign)) in dislocations.iter().enumerate() {
                let (mx, my, msign) = if is_impostor {
                    let nx = 75.0 + ((px * 1.37 + (idx as f32) * 29.0) % 165.0);
                    let ny = 75.0 + ((py * 1.19 + (idx as f32) * 37.0) % 250.0);
                    (nx, ny, -sign)
                } else {
                    (px, py, sign)
                };
                let vx = lx - mx;
                let vy = ly - my;
                phase += msign * vy.atan2(vx);
            }
            let env = ((0.95 - r2) * 5.0).clamp(0.0, 1.0);
            let mut p = 128.0 + 85.0 * env * phase.sin();
            if noisy {
                p = 128.0 + 20.0 * phase.sin() + (((x * 31 + y * 17) % 13) as f32 - 6.0) * 1.5;
            }
            pixels[y * w + x] = p.clamp(10.0, 245.0) as u8;
        }
    }
    RawFrame::new(pixels).unwrap()
}

pub fn write_set(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    synthetic(0.0, 0.0, 0.0, false)
        .write_png(dir.join("probe.png"))
        .unwrap();
    synthetic(0.0, 5.0, 4.0, false)
        .write_png(dir.join("reference_match.png"))
        .unwrap();
    synthetic(17.0, 0.0, 0.0, false)
        .write_png(dir.join("reference_mismatch.png"))
        .unwrap();
    synthetic(0.0, 0.0, 0.0, true)
        .write_png(dir.join("degraded_noisy.png"))
        .unwrap();
}
