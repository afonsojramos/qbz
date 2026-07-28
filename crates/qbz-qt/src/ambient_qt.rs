//! App-wide dynamic ambient background ("modo Cider", phase 14) — the
//! palette half of the feature. Ports the album-art triad extraction the
//! Slint pushes to `shader_underlay::set_palette` on every track change
//! (crates/qbz/src/playback.rs:1590-1643 → crates/qbz/src/immersive.rs):
//!
//! - primary / secondary: `spectrum_colors` — coverage-weighted hue
//!   histogram over the cover's CHROMATIC pixels (24 bins, one vote per
//!   pixel, L in 0.10..=0.93, S >= 0.08, >= 4 chromatic pixels else the
//!   teal/purple default). Primary = the most abundant hue cluster;
//!   secondary = a second genuine cluster >= ~45° away and >= 35% of the
//!   peak, else the same hue deeper (never a fabricated rotation).
//! - accent: `lyrics_accent_color` — the dominant hue COMPLEMENTED (+180°),
//!   vivid mid-bright.
//!
//! Input = the CURRENT now-playing artwork (the same cached cover the bar
//! shows), recomputed on track change from playback_qt's refresh — 1:1 the
//! Slint drive point. Decode + downscale run on a spawned task; only the
//! three hex strings cross to the UI thread.
//!
//! POC-NOTEs:
//! - The Slint renders the ambient scene with a custom WGSL shader (four
//!   domain-warped album-colored metaballs + grain + audio breathe). The
//!   POC renders the same orbit model as additive canvas gradients
//!   (qml/AmbientField.qml): no fbm warp, no true metaball fusion, no
//!   grain, no level_smooth breathe (the POC has no FFT tap), so the field
//!   is a close-but-simpler read of the same concept.
//! - The "blurred art" mode (route A, ImmersiveAtmosphere) is NOT
//!   implemented; a persisted "blurred" pref maps to the ambient look.
//! - The Slint gates the whole feature to the wgpu renderer tier; the
//!   POC's canvas path is renderer-agnostic, so it is always available.

use cxx_qt_lib::QString;

// ---------------------------------------------------------------------------
// HSL helpers (immersive.rs rgb_to_hsl / hsl_to_rgb, ported 1:1)
// ---------------------------------------------------------------------------

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1.0e-6 {
        return (0.0, 0.0, l);
    }
    let s = (d / (1.0 - (2.0 * l - 1.0).abs())).clamp(0.0, 1.0);
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h.rem_euclid(360.0), s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

// ---------------------------------------------------------------------------
// Palette extraction (immersive.rs spectrum_colors / lyrics_accent_color)
// ---------------------------------------------------------------------------

/// Coverage-weighted dominant hue bin over the 16x16 sample (shared by both
/// extractors). Returns (best bin, best score, chromatic count).
fn dominant_hue_bin(img: &image::RgbaImage) -> (usize, f32, u32) {
    const BINS: usize = 24;
    let mut hist = [0.0f32; BINS];
    let mut chromatic = 0u32;
    for px in img.pixels() {
        let (h, s, l) = rgb_to_hsl(px[0], px[1], px[2]);
        if !(0.10..=0.93).contains(&l) || s < 0.08 {
            continue;
        }
        let bin = ((h / 360.0 * BINS as f32) as usize).min(BINS - 1);
        hist[bin] += 1.0;
        chromatic += 1;
    }
    let score_at = |i: usize| {
        hist[i] + 0.5 * (hist[(i + BINS - 1) % BINS] + hist[(i + 1) % BINS])
    };
    let mut best_i = 0usize;
    let mut best = -1.0f32;
    for (i, _) in hist.iter().enumerate() {
        let sc = score_at(i);
        if sc > best {
            best = sc;
            best_i = i;
        }
    }
    (best_i, best, chromatic)
}

/// primary + secondary (spectrum_colors).
fn spectrum_colors(img: &image::RgbaImage) -> ((u8, u8, u8), (u8, u8, u8)) {
    const BINS: usize = 24;
    let default = ((0, 220, 200), (150, 50, 255));
    let (best_i, best, chromatic) = dominant_hue_bin(img);
    if chromatic < 4 {
        return default;
    }
    let primary_hue = (best_i as f32 + 0.5) * (360.0 / BINS as f32);

    // Rebuild the histogram scoring for the secondary search (kept 1:1 with
    // the Slint, which recomputes scores per candidate).
    let mut hist = [0.0f32; BINS];
    for px in img.pixels() {
        let (h, s, l) = rgb_to_hsl(px[0], px[1], px[2]);
        if !(0.10..=0.93).contains(&l) || s < 0.08 {
            continue;
        }
        let bin = ((h / 360.0 * BINS as f32) as usize).min(BINS - 1);
        hist[bin] += 1.0;
    }
    let score_at = |i: usize| {
        hist[i] + 0.5 * (hist[(i + BINS - 1) % BINS] + hist[(i + 1) % BINS])
    };
    let mut sec_i: Option<usize> = None;
    let mut sec_best = 0.0f32;
    for i in 0..BINS {
        let circ = (i as i32 - best_i as i32).rem_euclid(BINS as i32);
        let dist = circ.min(BINS as i32 - circ);
        if dist < 3 {
            continue;
        }
        let sc = score_at(i);
        if sc > sec_best {
            sec_best = sc;
            sec_i = Some(i);
        }
    }

    let primary = hsl_to_rgb(primary_hue, 0.85, 0.58);
    let secondary = match sec_i.filter(|_| sec_best >= best * 0.35) {
        Some(si) => {
            let sec_hue = (si as f32 + 0.5) * (360.0 / BINS as f32);
            hsl_to_rgb(sec_hue, 0.88, 0.62)
        }
        None => hsl_to_rgb(primary_hue, 0.95, 0.40),
    };
    (primary, secondary)
}

/// accent (lyrics_accent_color): dominant hue complemented +180°.
fn lyrics_accent_color(img: &image::RgbaImage) -> (u8, u8, u8) {
    let default = (0x3f, 0xd9, 0xc8);
    let (best_i, _best, chromatic) = dominant_hue_bin(img);
    if chromatic < 4 {
        return default;
    }
    let dominant_hue = (best_i as f32 + 0.5) * (360.0 / 24.0);
    let accent_hue = (dominant_hue + 180.0).rem_euclid(360.0);
    hsl_to_rgb(accent_hue, 0.85, 0.62)
}

fn hex(c: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img_from(pixels: &[(u8, u8, u8)]) -> image::RgbaImage {
        let mut img = image::RgbaImage::new(16, 16);
        for (i, px) in img.pixels_mut().enumerate() {
            let (r, g, b) = pixels[i % pixels.len()];
            *px = image::Rgba([r, g, b, 255]);
        }
        img
    }

    #[test]
    fn dominant_hue_wins_over_speck() {
        // 3/4 red-ish + 1/4 blue-ish: the coverage-weighted primary must be
        // the red hue (not the more saturated speck).
        let img = img_from(&[
            (200, 30, 30), (200, 30, 30), (200, 30, 30), (30, 30, 220),
        ]);
        let (primary, _secondary) = spectrum_colors(&img);
        let (h, _s, _l) = rgb_to_hsl(primary.0, primary.1, primary.2);
        assert!(h < 15.0 || h > 345.0, "primary hue {h} should be red-ish");
    }

    #[test]
    fn grey_cover_falls_back_to_defaults() {
        let img = img_from(&[(128, 128, 128)]);
        let (primary, secondary) = spectrum_colors(&img);
        assert_eq!(primary, (0, 220, 200));
        assert_eq!(secondary, (150, 50, 255));
        assert_eq!(lyrics_accent_color(&img), (0x3f, 0xd9, 0xc8));
    }

    #[test]
    fn accent_complements_the_dominant_hue() {
        let img = img_from(&[(200, 30, 30), (200, 30, 30), (200, 30, 30), (210, 40, 40)]);
        let accent = lyrics_accent_color(&img);
        let (h, _s, _l) = rgb_to_hsl(accent.0, accent.1, accent.2);
        assert!((170.0..=190.0).contains(&h), "accent hue {h} should be cyan-ish");
    }

    #[test]
    fn secondary_stays_on_album_for_mono_covers() {
        // Single-hue cover: the secondary must be the SAME hue, deeper —
        // never a fabricated rotation (immersive.rs comment).
        let img = img_from(&[(220, 40, 120), (200, 30, 110)]);
        let (primary, secondary) = spectrum_colors(&img);
        let (ph, _, _) = rgb_to_hsl(primary.0, primary.1, primary.2);
        let (sh, _, sl) = rgb_to_hsl(secondary.0, secondary.1, secondary.2);
        let dist = (ph - sh).abs().min(360.0 - (ph - sh).abs());
        assert!(dist < 30.0, "mono cover: secondary hue {sh} vs primary {ph}");
        assert!(sl < 0.45, "mono cover: secondary should be deeper (L={sl})");
    }
}


/// Recompute the ambient triad from the current track's artwork and publish
/// it to the bridge (ambientPrimary/Secondary/Accent). Called on track
/// change from playback_qt's now-playing refresh. A cover that is not on
/// disk yet is downloaded first (the Slint fetches+decodes itself the same
/// way); a failed download leaves the previous triad in place (the Slint
/// default-until-resolved behavior).
pub fn update_for_artwork(artwork_url: &str) {
    if artwork_url.is_empty() {
        return;
    }
    let url = artwork_url.to_string();
    crate::spawn(async move {
        if crate::artwork_qt::cached_path(&url).is_empty() {
            crate::artwork_qt::download_missing(vec![url.clone()]).await;
        }
        let path = crate::artwork_qt::cached_path(&url);
        if path.is_empty() {
            return;
        }
        let path = path.trim_start_matches("file://").to_string();
        let triad = tokio::task::spawn_blocking(move || {
            let bytes = std::fs::read(&path).ok()?;
            let img = image::load_from_memory(&bytes).ok()?;
            let tiny = image::imageops::resize(
                &img.to_rgba8(),
                16,
                16,
                image::imageops::FilterType::Triangle,
            );
            let (primary, secondary) = spectrum_colors(&tiny);
            let accent = lyrics_accent_color(&tiny);
            Some((hex(primary), hex(secondary), hex(accent)))
        })
        .await
        .ok()
        .flatten();
        if let Some((primary, secondary, accent)) = triad {
            log::info!("[qbz-qt] ambient palette: {primary} / {secondary} / {accent}");
            crate::ui(move |mut b| {
                b.as_mut().set_ambient_primary(QString::from(primary.as_str()));
                b.as_mut()
                    .set_ambient_secondary(QString::from(secondary.as_str()));
                b.as_mut().set_ambient_accent(QString::from(accent.as_str()));
            });
        }
    });
}
