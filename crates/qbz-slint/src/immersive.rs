//! ImmersiveView visual glue.
//!
//! The Tauri background's "full" mode is Kawarp (Kawase blur + domain warp).
//! Slint/femtovg cannot run that shader directly, so this module produces the
//! same source material as Tauri's atmosphere texture: a tiny artwork color
//! field scaled up, blurred, saturated, warmed, and vignetted. The Slint layer
//! animates two copies in opposite directions to approximate the warp.

use image::{imageops, Rgba, RgbaImage};
use slint::Color;

/// Generate a 128x128 atmospheric image from decoded RGBA artwork pixels.
/// Mirrors `src/lib/immersive/utils/texture-loader.ts::generateAtmosphere`.
pub fn generate_atmosphere(pixels: &[u8], width: u32, height: u32) -> Option<(Vec<u8>, u32, u32)> {
    let src = RgbaImage::from_raw(width, height, pixels.to_vec())?;
    let tiny = imageops::resize(&src, 8, 8, imageops::FilterType::Triangle);
    let scaled = imageops::resize(&tiny, 128, 128, imageops::FilterType::CatmullRom);
    let blurred = imageops::blur(&scaled, 16.0);
    let adjusted = color_adjust(blurred);
    let final_img = vignette(adjusted, 0.20);
    Some((final_img.into_raw(), 128, 128))
}

/// AlbumReactive glow color: the most saturated non-extreme 8x8 sample.
pub fn glow_color(pixels: &[u8], width: u32, height: u32) -> Color {
    let Some(src) = RgbaImage::from_raw(width, height, pixels.to_vec()) else {
        return Color::from_argb_u8(0x59, 100, 100, 255);
    };
    let tiny = imageops::resize(&src, 8, 8, imageops::FilterType::Triangle);
    let mut best_sat = 0.0f32;
    let mut best = (100u8, 100u8, 255u8);

    for px in tiny.pixels() {
        let r = px[0] as f32;
        let g = px[1] as f32;
        let b = px[2] as f32;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let lum = (max + min) / 2.0;
        let sat = if (max - min).abs() < f32::EPSILON {
            0.0
        } else if lum > 127.0 {
            (max - min) / (510.0 - max - min).max(1.0)
        } else {
            (max - min) / (max + min).max(1.0)
        };
        if lum > 50.0 && lum < 220.0 && sat > best_sat {
            best_sat = sat;
            best = (px[0], px[1], px[2]);
        }
    }

    Color::from_argb_u8(0x59, best.0, best.1, best.2)
}

/// Two vivid bar colors for the Spectrum visualizer, derived from the artwork
/// and guaranteed to read against a BLACK background. The bars sit on solid
/// black, so a dark/desaturated cover (e.g. a near-black Metallica album) would
/// vanish; in that case we fall back to the NEGATIVE (inverted) color so the
/// bars pop. Returns (primary at the base, secondary at the tip).
pub fn spectrum_colors(pixels: &[u8], width: u32, height: u32) -> (Color, Color) {
    let default = (
        Color::from_rgb_u8(0, 220, 200),
        Color::from_rgb_u8(150, 50, 255),
    );
    let Some(src) = RgbaImage::from_raw(width, height, pixels.to_vec()) else {
        return default;
    };
    let tiny = imageops::resize(&src, 8, 8, imageops::FilterType::Triangle);

    // HSL-ish saturation (same measure as glow_color).
    let sat = |r: f32, g: f32, b: f32| -> f32 {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        if (max - min).abs() < f32::EPSILON {
            0.0
        } else if (max + min) / 2.0 > 127.0 {
            (max - min) / (510.0 - max - min).max(1.0)
        } else {
            (max - min) / (max + min).max(1.0)
        }
    };

    let mut cands: Vec<(f32, (u8, u8, u8))> = Vec::new();
    let (mut ar, mut ag, mut ab, mut n) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for px in tiny.pixels() {
        let (r, g, b) = (px[0] as f32, px[1] as f32, px[2] as f32);
        ar += r;
        ag += g;
        ab += b;
        n += 1.0;
        cands.push((sat(r, g, b), (px[0], px[1], px[2])));
    }
    cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let n = n.max(1.0);
    let avg = ((ar / n) as u8, (ag / n) as u8, (ab / n) as u8);

    let hue_diff = |a: (u8, u8, u8), b: (u8, u8, u8)| -> i32 {
        (a.0 as i32 - b.0 as i32).abs()
            + (a.1 as i32 - b.1 as i32).abs()
            + (a.2 as i32 - b.2 as i32).abs()
    };

    // Primary = most saturated usable sample; else the NEGATIVE of the average
    // (handles black/grey covers -> bright, visible bars).
    let top = cands.first().copied().unwrap_or((0.0, avg));
    let (primary, secondary) = if top.0 > 0.12 {
        let prim = top.1;
        let sec = cands
            .iter()
            .skip(1)
            .find(|c| hue_diff(c.1, prim) > 90)
            .map(|c| c.1)
            .unwrap_or((prim.1, prim.2, prim.0)); // channel-rotate for a distinct stop
        (prim, sec)
    } else {
        let neg = (255 - avg.0, 255 - avg.1, 255 - avg.2);
        (neg, (neg.2, neg.0, neg.1))
    };

    (
        ensure_visible_on_black(primary),
        ensure_visible_on_black(secondary),
    )
}

/// Guarantee a color reads against black: if it is too dark, invert it
/// (negative); if it is still dim, lift it toward a visible brightness.
fn ensure_visible_on_black((r, g, b): (u8, u8, u8)) -> Color {
    let luma = |r: u8, g: u8, b: u8| 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    let (mut r, mut g, mut b) = (r, g, b);
    if luma(r, g, b) < 70.0 {
        r = 255 - r;
        g = 255 - g;
        b = 255 - b;
    }
    let l = luma(r, g, b);
    if l > 0.0 && l < 110.0 {
        let k = 110.0 / l;
        r = (r as f32 * k).min(255.0) as u8;
        g = (g as f32 * k).min(255.0) as u8;
        b = (b as f32 * k).min(255.0) as u8;
    }
    Color::from_rgb_u8(r, g, b)
}

fn color_adjust(mut img: RgbaImage) -> RgbaImage {
    let mut min_r = 255.0f32;
    let mut min_g = 255.0f32;
    let mut min_b = 255.0f32;
    let mut max_r = 0.0f32;
    let mut max_g = 0.0f32;
    let mut max_b = 0.0f32;
    let mut total_brightness = 0.0f32;
    let count = (img.width() * img.height()).max(1) as f32;

    for px in img.pixels_mut() {
        let (r, g, b) = saturate_brightness_contrast(px[0], px[1], px[2]);
        px[0] = r;
        px[1] = g;
        px[2] = b;

        let rf = r as f32;
        let gf = g as f32;
        let bf = b as f32;
        min_r = min_r.min(rf);
        min_g = min_g.min(gf);
        min_b = min_b.min(bf);
        max_r = max_r.max(rf);
        max_g = max_g.max(gf);
        max_b = max_b.max(bf);
        total_brightness += (rf + gf + bf) / 3.0;
    }

    let norm_strength = (total_brightness / count / 80.0).min(1.0);
    let target_min = 18.0f32;
    let target_range = 232.0f32;
    let range_r = (max_r - min_r).max(1.0);
    let range_g = (max_g - min_g).max(1.0);
    let range_b = (max_b - min_b).max(1.0);

    for px in img.pixels_mut() {
        let r = px[0] as f32;
        let g = px[1] as f32;
        let b = px[2] as f32;
        let norm_r = target_min + ((r - min_r) / range_r) * target_range;
        let norm_g = target_min + ((g - min_g) / range_g) * target_range;
        let norm_b = target_min + ((b - min_b) / range_b) * target_range;
        let lift_r = (r * 1.5).min(255.0);
        let lift_g = (g * 1.5).min(255.0);
        let lift_b = (b * 1.5).min(255.0);

        px[0] = (lift_r + (norm_r - lift_r) * norm_strength).mul_add(1.08, 0.0).min(255.0) as u8;
        px[1] = (lift_g + (norm_g - lift_g) * norm_strength).min(255.0) as u8;
        px[2] = (lift_b + (norm_b - lift_b) * norm_strength).min(255.0) as u8;
    }

    img
}

fn saturate_brightness_contrast(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let mut rf = r as f32;
    let mut gf = g as f32;
    let mut bf = b as f32;
    let gray = (rf + gf + bf) / 3.0;
    let sat = 2.45;
    rf = gray + (rf - gray) * sat;
    gf = gray + (gf - gray) * sat;
    bf = gray + (bf - gray) * sat;

    let brightness = 0.92;
    let contrast = 1.18;
    let adjust = |v: f32| ((v * brightness - 128.0) * contrast + 128.0).clamp(0.0, 255.0) as u8;
    (adjust(rf), adjust(gf), adjust(bf))
}

fn vignette(mut img: RgbaImage, intensity: f32) -> RgbaImage {
    let w = img.width() as f32;
    let h = img.height() as f32;
    let cx = w / 2.0;
    let cy = h / 2.0;
    let inner = w.min(h) * 0.20;
    let outer = w.min(h) * 0.70;

    for (x, y, px) in img.enumerate_pixels_mut() {
        let dx = x as f32 - cx;
        let dy = y as f32 - cy;
        let d = (dx * dx + dy * dy).sqrt();
        let t = ((d - inner) / (outer - inner)).clamp(0.0, 1.0);
        let factor = 1.0 - intensity * t;
        *px = Rgba([
            (px[0] as f32 * factor) as u8,
            (px[1] as f32 * factor) as u8,
            (px[2] as f32 * factor) as u8,
            px[3],
        ]);
    }
    img
}
