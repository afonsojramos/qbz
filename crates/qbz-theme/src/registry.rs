//! The theme registry: `ThemeId` -> fully-materialized [`ThemeColors`].
//!
//! P1 materializes only the four existing themes (Dark / OLED / Tokyo Night /
//! System-fallback). Every value for the standard themes is transcribed from
//! `src/app.css` (see `qbz-nix-docs/.../01-tauri-themes-inventory.md`). The new
//! token families (danger/warning/success bg+border+hover, border-strong,
//! focus-ring, the full 24-tier alpha ramp) are DERIVED here so the four P1
//! rows are complete and look unchanged for the already-consumed tokens.
//!
//! P2/P3 extend `palette()` with the remaining standard + redesigned a11y rows.

use crate::color::Rgba;
use crate::colors::{alpha_ramp, ThemeColors};
use crate::id::ThemeId;

/// Legacy translucent edge values the existing 4 Slint themes used directly,
/// reproduced 1:1 so P1 stays pixel-identical:
///   surface-hover  = ~6% white  (#ffffff10)
///   border-subtle  = ~8% white  (#ffffff14)
///   border-muted   = ~22% white (#ffffff38)
///   card-shadow    = rgba(0,0,0,0.4) (#00000066)
const LEGACY_SURFACE_HOVER: Rgba = Rgba::rgba(255, 255, 255, 0x10);
const LEGACY_BORDER_SUBTLE: Rgba = Rgba::rgba(255, 255, 255, 0x14);
const LEGACY_BORDER_MUTED: Rgba = Rgba::rgba(255, 255, 255, 0x38);
const LEGACY_CARD_SHADOW: Rgba = Rgba::rgba(0, 0, 0, 0x66);

/// Resolve a theme id to its concrete color set.
///
/// `System` has no static palette — at runtime the Slint side follows the OS
/// (std-widgets `Palette`) for the tokens it overrides, exactly as before. This
/// returns the Dark set as a safe fallback for any caller that needs a concrete
/// struct for `System` (it is NOT what paints the System theme; that stays the
/// `is-system` path in `theme.slint`).
pub fn palette(id: ThemeId) -> ThemeColors {
    match id {
        ThemeId::Dark => dark(),
        ThemeId::Oled => oled(),
        ThemeId::TokyoNight => tokyo_night(),
        ThemeId::System => dark(),
        // P2/P3: remaining standard + redesigned a11y rows. Until then, fall
        // back to Dark so the registry always returns a fully-populated struct.
        _ => dark(),
    }
}

/// `:root` Dark — the base every other theme inherits omissions from.
/// All hex values cite `src/app.css :root` via the inventory doc.
fn dark() -> ThemeColors {
    // Danger / warning solid hues from :root; their bg/border/hover were rgba()
    // tints of the hue at 0.1/0.3/0.2. We bake those as straight-alpha overlays
    // of the solid hue so they composite identically.
    let danger = Rgba::rgb(0xef, 0x44, 0x44); // --danger
    let warning = Rgba::rgb(0xfb, 0xbf, 0x24); // --warning
    // Success is NEW (no Tauri parity). Use the project green (#3fae6a) and the
    // same 0.1/0.3/0.2 tint shape so toasts/banners can stop hardcoding hex.
    let success = Rgba::rgb(0x3f, 0xae, 0x6a);
    ThemeColors {
        surface_main: Rgba::rgb(0x0f, 0x0f, 0x0f),     // --bg-primary
        surface_card: Rgba::rgb(0x1a, 0x1a, 0x1a),     // --bg-secondary
        surface_elevated: Rgba::rgb(0x2a, 0x2a, 0x2a), // --bg-tertiary
        surface_hover: LEGACY_SURFACE_HOVER,
        bg_hover: Rgba::rgb(0x1f, 0x1f, 0x1f), // --bg-hover

        text_primary: Rgba::rgb(0xff, 0xff, 0xff),   // --text-primary
        text_secondary: Rgba::rgb(0xcc, 0xcc, 0xcc), // --text-secondary
        text_muted: Rgba::rgb(0x88, 0x88, 0x88),     // --text-muted
        text_disabled: Rgba::rgb(0x55, 0x55, 0x55),  // --text-disabled

        accent: Rgba::rgb(0x42, 0x85, 0xf4),         // --accent-primary
        accent_hover: Rgba::rgb(0x5a, 0x9b, 0xf4),   // --accent-hover
        accent_pressed: Rgba::rgb(0x32, 0x75, 0xe4), // --accent-active
        accent_text: Rgba::rgb(0xff, 0xff, 0xff),    // --btn-primary-text

        danger,
        danger_bg: with_alpha(danger, 0.1),
        danger_border: with_alpha(danger, 0.3),
        danger_hover: with_alpha(danger, 0.2),

        warning,
        warning_bg: with_alpha(warning, 0.1),
        warning_border: with_alpha(warning, 0.3),
        warning_hover: with_alpha(warning, 0.2),

        success,
        success_bg: with_alpha(success, 0.1),
        success_border: with_alpha(success, 0.3),
        success_hover: with_alpha(success, 0.2),

        border_subtle: LEGACY_BORDER_SUBTLE,
        border_muted: LEGACY_BORDER_MUTED,
        border_strong: Rgba::rgb(0x3a, 0x3a, 0x3a), // --border-strong

        focus_ring: Rgba::rgb(0x42, 0x85, 0xf4), // = accent (no Tauri token; new)

        favorite: danger, // the loved-heart uses danger red
        card_shadow: LEGACY_CARD_SHADOW,

        alpha: alpha_ramp(false), // dark theme -> white-based overlays
    }
}

/// OLED Black — inherits everything from Dark except backgrounds + borders.
/// The legacy Slint OLED only overrode the three surfaces; keep that exactly,
/// inherit the rest from `dark()`.
fn oled() -> ThemeColors {
    ThemeColors {
        surface_main: Rgba::rgb(0x00, 0x00, 0x00),     // --bg-primary
        surface_card: Rgba::rgb(0x0a, 0x0a, 0x0a),     // --bg-secondary
        surface_elevated: Rgba::rgb(0x1a, 0x1a, 0x1a), // --bg-tertiary
        bg_hover: Rgba::rgb(0x11, 0x11, 0x11),         // --bg-hover (oled)
        border_strong: Rgba::rgb(0x2a, 0x2a, 0x2a),    // --border-strong (oled)
        ..dark()
    }
}

/// Tokyo Night — full recolor. Surfaces/text/accent transcribed from the legacy
/// Slint ternary (which matches `src/app.css [data-theme="tokyo-night"]`).
fn tokyo_night() -> ThemeColors {
    let danger = Rgba::rgb(0xdb, 0x4b, 0x4b); // --danger
    let warning = Rgba::rgb(0xe0, 0xaf, 0x68); // --warning
    let success = Rgba::rgb(0x3f, 0xae, 0x6a);
    ThemeColors {
        surface_main: Rgba::rgb(0x1a, 0x1b, 0x26),     // --bg-primary
        surface_card: Rgba::rgb(0x16, 0x16, 0x1e),     // --bg-secondary
        surface_elevated: Rgba::rgb(0x1c, 0x1d, 0x29), // --bg-tertiary
        surface_hover: LEGACY_SURFACE_HOVER,
        bg_hover: Rgba::rgb(0x20, 0x23, 0x30), // --bg-hover

        text_primary: Rgba::rgb(0xa9, 0xb1, 0xd6),   // --text-primary
        text_secondary: Rgba::rgb(0x78, 0x7c, 0x99), // --text-secondary
        text_muted: Rgba::rgb(0x54, 0x5c, 0x7e),     // --text-muted
        text_disabled: Rgba::rgb(0x3d, 0x42, 0x5e),  // --text-disabled

        accent: Rgba::rgb(0x7a, 0xa2, 0xf7),         // --accent-primary
        accent_hover: Rgba::rgb(0x7d, 0xcf, 0xff),   // --accent-hover
        accent_pressed: Rgba::rgb(0xbb, 0x9a, 0xf7), // --accent-active
        accent_text: Rgba::rgb(0x1a, 0x1b, 0x26),    // --btn-primary-text

        danger,
        danger_bg: with_alpha(danger, 0.1),
        danger_border: with_alpha(danger, 0.3),
        danger_hover: with_alpha(danger, 0.2),

        warning,
        warning_bg: with_alpha(warning, 0.1),
        warning_border: with_alpha(warning, 0.3),
        warning_hover: with_alpha(warning, 0.2),

        success,
        success_bg: with_alpha(success, 0.1),
        success_border: with_alpha(success, 0.3),
        success_hover: with_alpha(success, 0.2),

        border_subtle: LEGACY_BORDER_SUBTLE,
        border_muted: LEGACY_BORDER_MUTED,
        border_strong: Rgba::rgb(0x20, 0x23, 0x30), // --border-strong

        focus_ring: Rgba::rgb(0x7a, 0xa2, 0xf7), // = accent

        favorite: danger,
        card_shadow: LEGACY_CARD_SHADOW,

        alpha: alpha_ramp(false), // dark theme -> white-based overlays
    }
}

/// Straight-alpha overlay of an opaque hue at `frac` opacity (0.0..=1.0).
/// Used to reproduce Tauri's `rgba(hue, frac)` danger/warning/success tints.
const fn with_alpha(c: Rgba, frac: f32) -> Rgba {
    let a = (frac * 255.0 + 0.5) as u8;
    Rgba::rgba(c.r, c.g, c.b, a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::ALPHA_COUNT;

    #[test]
    fn p1_rows_are_fully_populated() {
        for id in [ThemeId::Dark, ThemeId::Oled, ThemeId::TokyoNight, ThemeId::System] {
            let c = palette(id);
            // Spot-check a few fields are non-degenerate / the ramp is full.
            assert_eq!(c.alpha.len(), ALPHA_COUNT);
            assert_ne!(c.surface_main, Rgba::rgba(0, 0, 0, 0));
            assert_ne!(c.text_primary, Rgba::rgba(0, 0, 0, 0));
            assert_ne!(c.accent, Rgba::rgba(0, 0, 0, 0));
            // every alpha tier carries some opacity
            assert!(c.alpha.iter().all(|a| a.a > 0));
        }
    }

    #[test]
    fn dark_matches_root_css() {
        let c = palette(ThemeId::Dark);
        assert_eq!(c.surface_main, Rgba::rgb(0x0f, 0x0f, 0x0f));
        assert_eq!(c.surface_card, Rgba::rgb(0x1a, 0x1a, 0x1a));
        assert_eq!(c.surface_elevated, Rgba::rgb(0x2a, 0x2a, 0x2a));
        assert_eq!(c.text_primary, Rgba::rgb(0xff, 0xff, 0xff));
        assert_eq!(c.accent, Rgba::rgb(0x42, 0x85, 0xf4));
        assert_eq!(c.border_strong, Rgba::rgb(0x3a, 0x3a, 0x3a));
        assert_eq!(c.favorite, c.danger);
    }

    #[test]
    fn oled_overrides_only_surfaces_and_borders() {
        let d = palette(ThemeId::Dark);
        let o = palette(ThemeId::Oled);
        assert_eq!(o.surface_main, Rgba::rgb(0, 0, 0));
        assert_eq!(o.surface_card, Rgba::rgb(0x0a, 0x0a, 0x0a));
        assert_eq!(o.surface_elevated, Rgba::rgb(0x1a, 0x1a, 0x1a));
        assert_eq!(o.bg_hover, Rgba::rgb(0x11, 0x11, 0x11));
        assert_eq!(o.border_strong, Rgba::rgb(0x2a, 0x2a, 0x2a));
        // inherited from Dark:
        assert_eq!(o.accent, d.accent);
        assert_eq!(o.text_primary, d.text_primary);
        assert_eq!(o.danger, d.danger);
    }

    #[test]
    fn tokyo_legacy_values_preserved() {
        let c = palette(ThemeId::TokyoNight);
        assert_eq!(c.surface_main, Rgba::rgb(0x1a, 0x1b, 0x26));
        assert_eq!(c.surface_card, Rgba::rgb(0x16, 0x16, 0x1e));
        assert_eq!(c.surface_elevated, Rgba::rgb(0x1c, 0x1d, 0x29));
        assert_eq!(c.text_primary, Rgba::rgb(0xa9, 0xb1, 0xd6));
        assert_eq!(c.accent, Rgba::rgb(0x7a, 0xa2, 0xf7));
        assert_eq!(c.accent_text, Rgba::rgb(0x1a, 0x1b, 0x26));
    }

    #[test]
    fn legacy_alpha_aliases_unchanged() {
        // The exact translucent values the old Slint Theme exposed.
        for id in [ThemeId::Dark, ThemeId::Oled, ThemeId::TokyoNight] {
            let c = palette(id);
            assert_eq!(c.surface_hover, Rgba::rgba(255, 255, 255, 0x10));
            assert_eq!(c.border_subtle, Rgba::rgba(255, 255, 255, 0x14));
            assert_eq!(c.border_muted, Rgba::rgba(255, 255, 255, 0x38));
            assert_eq!(c.card_shadow, Rgba::rgba(0, 0, 0, 0x66));
            // legacy fixed-white alpha steps consumed by the miniplayer:
            assert_eq!(c.alpha_pct(8), Rgba::rgba(255, 255, 255, 0x14));
            assert_eq!(c.alpha_pct(10), Rgba::rgba(255, 255, 255, 0x1a));
            assert_eq!(c.alpha_pct(12), Rgba::rgba(255, 255, 255, 0x1f));
            assert_eq!(c.alpha_pct(18), Rgba::rgba(255, 255, 255, 0x2e));
            assert_eq!(c.alpha_pct(55), Rgba::rgba(255, 255, 255, 0x8c));
            assert_eq!(c.alpha_pct(65), Rgba::rgba(255, 255, 255, 0xa6));
            assert_eq!(c.alpha_pct(70), Rgba::rgba(255, 255, 255, 0xb3));
            assert_eq!(c.alpha_pct(75), Rgba::rgba(255, 255, 255, 0xbf));
        }
    }
}
