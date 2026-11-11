use crate::domain::{HEIGHT, RawFrame, WIDTH};
use crate::matcher::{Minutia, MinutiaKind};
use image::{Rgb, RgbImage};

pub const RED: Rgb<u8> = Rgb([255, 0, 0]);
pub const GREEN: Rgb<u8> = Rgb([0, 255, 0]);
const GREY: Rgb<u8> = Rgb([120, 120, 120]);
const YELLOW: Rgb<u8> = Rgb([255, 215, 0]);

fn set_pixel(image: &mut RgbImage, x: i32, y: i32, color: Rgb<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < image.width() && (y as u32) < image.height() {
        image.put_pixel(x as u32, y as u32, color);
    }
}

/// Draws an integer Bresenham line, clipping points outside the target image.
pub fn draw_line(image: &mut RgbImage, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: Rgb<u8>) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        set_pixel(image, x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// Draws a midpoint-circle outline using only integer raster operations.
pub fn draw_circle(image: &mut RgbImage, cx: i32, cy: i32, radius: i32, color: Rgb<u8>) {
    let mut x = radius;
    let mut y = 0;
    let mut err = 1 - radius;
    while x >= y {
        for (px, py) in [
            (x, y),
            (y, x),
            (-y, x),
            (-x, y),
            (-x, -y),
            (-y, -x),
            (y, -x),
            (x, -y),
        ] {
            set_pixel(image, cx + px, cy + py, color);
        }
        y += 1;
        if err < 0 {
            err += 2 * y + 1;
        } else {
            x -= 1;
            err += 2 * (y - x) + 1;
        }
    }
}

pub fn frame_to_rgb(frame: &RawFrame) -> RgbImage {
    RgbImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let pixel = frame.pixels()[(y * WIDTH + x) as usize];
        Rgb([pixel, pixel, pixel])
    })
}

fn color_for(minutia: &Minutia) -> Rgb<u8> {
    match minutia.kind {
        MinutiaKind::RidgeEnding => RED,
        MinutiaKind::Bifurcation => GREEN,
    }
}

pub fn draw_minutia(
    image: &mut RgbImage,
    minutia: &Minutia,
    offset_x: i32,
    color: Option<Rgb<u8>>,
) {
    let x = minutia.x.round() as i32 + offset_x;
    let y = minutia.y.round() as i32;
    let color = color.unwrap_or_else(|| color_for(minutia));
    draw_circle(image, x, y, 3, color);
    let radians = minutia.angle_deg.to_radians();
    let end_x = x + (radians.cos() * 8.0).round() as i32;
    let end_y = y + (radians.sin() * 8.0).round() as i32;
    draw_line(image, x, y, end_x, end_y, color);
}

pub fn annotate_frame(frame: &RawFrame, minutiae: &[Minutia]) -> RgbImage {
    let mut image = frame_to_rgb(frame);
    for minutia in minutiae {
        draw_minutia(&mut image, minutia, 0, None);
    }
    image
}

/// Renders two source frames with a neutral separator and homologous links.
pub fn compare_frames_image(
    probe: &RawFrame,
    reference: &RawFrame,
    probe_minutiae: &[Minutia],
    reference_minutiae: &[Minutia],
    pairs: &[(usize, usize)],
) -> RgbImage {
    const GAP: u32 = 24;
    let mut output = RgbImage::from_pixel(WIDTH * 2 + GAP, HEIGHT, Rgb([245, 245, 245]));
    let probe_image = frame_to_rgb(probe);
    let reference_image = frame_to_rgb(reference);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            output.put_pixel(x, y, *probe_image.get_pixel(x, y));
            output.put_pixel(x + WIDTH + GAP, y, *reference_image.get_pixel(x, y));
        }
    }
    for y in 0..HEIGHT {
        output.put_pixel(WIDTH + GAP / 2, y, Rgb([80, 80, 80]));
    }
    let mut paired_probe = vec![false; probe_minutiae.len()];
    let mut paired_reference = vec![false; reference_minutiae.len()];
    for &(a, b) in pairs {
        if let (Some(left), Some(right)) = (probe_minutiae.get(a), reference_minutiae.get(b)) {
            paired_probe[a] = true;
            paired_reference[b] = true;
            let x0 = left.x.round() as i32;
            let y0 = left.y.round() as i32;
            let x1 = right.x.round() as i32 + (WIDTH + GAP) as i32;
            let y1 = right.y.round() as i32;
            draw_line(&mut output, x0, y0, x1, y1, YELLOW);
        }
    }
    for (index, minutia) in probe_minutiae.iter().enumerate() {
        draw_minutia(
            &mut output,
            minutia,
            0,
            Some(if paired_probe[index] { GREEN } else { GREY }),
        );
    }
    for (index, minutia) in reference_minutiae.iter().enumerate() {
        draw_minutia(
            &mut output,
            minutia,
            (WIDTH + GAP) as i32,
            Some(if paired_reference[index] { GREEN } else { GREY }),
        );
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::FRAME_LEN;

    #[test]
    fn midpoint_circle_and_bresenham_change_rgb_pixels() {
        let mut image = RgbImage::from_pixel(20, 20, Rgb([0, 0, 0]));
        draw_circle(&mut image, 10, 10, 3, RED);
        draw_line(&mut image, 0, 0, 19, 19, GREEN);
        assert_eq!(*image.get_pixel(10, 7), RED);
        assert_eq!(*image.get_pixel(0, 0), GREEN);
        let frame = RawFrame::new(vec![128; FRAME_LEN]).unwrap();
        assert_eq!(frame_to_rgb(&frame).dimensions(), (WIDTH, HEIGHT));
    }
}
