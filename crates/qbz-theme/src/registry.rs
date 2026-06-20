//! The theme registry: `ThemeId` -> fully-materialized [`ThemeColors`].
//!
//! P1 materialized the four existing themes (Dark / OLED / Tokyo Night /
//! System-fallback). P2 (this file) transcribes the remaining **standard**
//! (non-accessibility) themes from `src/app.css` — every value cites the
//! `qbz-nix-docs/.../01-tauri-themes-inventory.md` table (which was read 1:1
//! from `src/app.css`). P3 will add the redesigned accessibility rows.
//!
//! No CSS cascade on the Slint side: every row is FULLY materialized. Tauri
//! themes that OMIT tokens (e.g. `light` omits the accent trio; `oled`/
//! `breeze-dark`/`adwaita-dark` omit whole danger/warning families) inherit
//! those from `:root` Dark — so the omissions are resolved against `dark()` at
//! transcription time, here, not at runtime.
//!
//! Derived (no Tauri parity) tokens for the standard rows:
//!   - `success` family: NEW. Tauri has no `--success`. We use the project green
//!     `#3fae6a` for dark themes (matches P1) and a darker `#1f8a4c` for light
//!     themes (so success text clears >=3:1 on a light surface), with the same
//!     0.1/0.3/0.2 tint shape for bg/border/hover. Polished in P4.
//!   - `focus_ring`: NEW (WCAG 2.4.7). Uses the theme accent (high-visibility,
//!     matches P1). Polished in P4.
//!   - `favorite`: the loved-heart uses the theme `danger` hue (matches P1).
//!   - `danger_bg/border/hover`, `warning_*`: Tauri expresses these as `rgba()`
//!     tints of the solid hue at 0.1/0.3/0.2 (dracula uses 0.15/0.4/0.25). We
//!     bake the same straight-alpha overlays so they composite identically.
//!   - `border_muted`: legacy Slint-only token (no Tauri var). Polarity-aware
//!     translucent edge (white ~22% on dark, black ~22% on light).
//!   - `surface_hover`: alpha-based hover overlay, polarity-aware (white ~6% on
//!     dark, black ~6% on light), distinct from the opaque theme `--bg-hover`.

use crate::color::{relative_luminance, Rgba};
use crate::colors::{alpha_ramp, ThemeColors};
use crate::id::ThemeId;

/// Legacy translucent edge values the existing 4 Slint dark themes used directly,
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
        // --- Core (P1 + the standard Light) ---
        ThemeId::Dark => dark(),
        ThemeId::Oled => oled(),
        ThemeId::TokyoNight => tokyo_night(),
        ThemeId::System => dark(),
        ThemeId::Light => light(),
        // --- Dark (branded / community) ---
        ThemeId::Warm => warm(),
        ThemeId::Nord => nord(),
        ThemeId::Dracula => dracula(),
        ThemeId::CatppuccinMocha => catppuccin_mocha(),
        ThemeId::BreezeDark => breeze_dark(),
        ThemeId::AdwaitaDark => adwaita_dark(),
        ThemeId::Aurora => aurora(),
        ThemeId::Ikari => ikari(),
        ThemeId::Ayanami => ayanami(),
        ThemeId::Iscariot => iscariot(),
        ThemeId::Stratego => stratego(),
        ThemeId::Rumi => rumi(),
        ThemeId::Zoey => zoey(),
        ThemeId::Mira => mira(),
        ThemeId::Frost => frost(),
        ThemeId::Langley => langley(),
        // --- Light (branded / community) ---
        ThemeId::Alucard => alucard(),
        ThemeId::RosePineDawn => rose_pine_dawn(),
        ThemeId::BreezeLight => breeze_light(),
        ThemeId::AdwaitaLight => adwaita_light(),
        ThemeId::DuotoneSnow => duotone_snow(),
        ThemeId::SnowStorm => snow_storm(),
        ThemeId::Kurosaki => kurosaki(),
        // --- Accessibility (REDESIGNED): P3 fills these. Fall back to a safe
        // fully-populated row until then.
        ThemeId::WcagLight
        | ThemeId::WcagDark
        | ThemeId::HighContrast
        | ThemeId::HighContrastLight
        | ThemeId::Colorblind => dark(),
    }
}

// ---------------------------------------------------------------------------
// Shared builder for the standard themes
// ---------------------------------------------------------------------------

/// The Tauri token set for one standard theme, as read from doc 01. Only the
/// named hues are carried here; the derived families (success/focus/favorite),
/// the polarity-driven alpha ramp + translucent edges, and the status tints are
/// materialized by [`StdSpec::build`].
#[derive(Clone, Copy)]
struct StdSpec {
    // surfaces (--bg-*)
    bg_primary: Rgba,
    bg_secondary: Rgba,
    bg_tertiary: Rgba,
    bg_hover: Rgba,
    // text (--text-*)
    text_primary: Rgba,
    text_secondary: Rgba,
    text_muted: Rgba,
    text_disabled: Rgba,
    // accent (--accent-* + --btn-primary-text)
    accent: Rgba,
    accent_hover: Rgba,
    accent_pressed: Rgba,
    accent_text: Rgba,
    // status hues (--danger / --warning); families derived as rgba() tints
    danger: Rgba,
    warning: Rgba,
    /// Tint fractions for the danger/warning bg/border/hover families.
    /// Standard themes use (0.1, 0.3, 0.2); dracula uses (0.15, 0.4, 0.25).
    tint_bg: f32,
    tint_border: f32,
    tint_hover: f32,
    // borders (--border-*)
    border_subtle: Rgba,
    border_strong: Rgba,
}

impl StdSpec {
    /// Default status-tint fractions (every theme except dracula).
    const TINT_BG: f32 = 0.1;
    const TINT_BORDER: f32 = 0.3;
    const TINT_HOVER: f32 = 0.2;

    /// Materialize a complete [`ThemeColors`] row. `is_light` is the corrected
    /// (luminance-derived) polarity — it drives the alpha ramp base (black on
    /// light, white on dark), the translucent edge/hover bases, and the derived
    /// `success` hue. NOTE: do NOT trust the Tauri `type` flag for this; pass the
    /// real luminance (Frost/Langley are registered light but are dark canvases).
    fn build(self, is_light: bool) -> ThemeColors {
        // success: NEW token, no Tauri parity. Theme-appropriate green that
        // clears >=3:1 on the theme surface; darker on light themes. Polished P4.
        let success = if is_light {
            Rgba::rgb(0x1f, 0x8a, 0x4c)
        } else {
            Rgba::rgb(0x3f, 0xae, 0x6a)
        };

        // Polarity-aware translucent edges (legacy Slint-only tokens). On light
        // themes a white hairline is invisible, so flip the base to black.
        let (eh, eg, eb) = if is_light { (0, 0, 0) } else { (255, 255, 255) };
        let surface_hover = Rgba::rgba(eh, eg, eb, 0x10); // ~6%
        let border_muted = Rgba::rgba(eh, eg, eb, 0x38); // ~22%

        ThemeColors {
            surface_main: self.bg_primary,
            surface_card: self.bg_secondary,
            surface_elevated: self.bg_tertiary,
            surface_hover,
            bg_hover: self.bg_hover,

            text_primary: self.text_primary,
            text_secondary: self.text_secondary,
            text_muted: self.text_muted,
            text_disabled: self.text_disabled,

            accent: self.accent,
            accent_hover: self.accent_hover,
            accent_pressed: self.accent_pressed,
            accent_text: self.accent_text,

            danger: self.danger,
            danger_bg: with_alpha(self.danger, self.tint_bg),
            danger_border: with_alpha(self.danger, self.tint_border),
            danger_hover: with_alpha(self.danger, self.tint_hover),

            warning: self.warning,
            warning_bg: with_alpha(self.warning, self.tint_bg),
            warning_border: with_alpha(self.warning, self.tint_border),
            warning_hover: with_alpha(self.warning, self.tint_hover),

            success,
            success_bg: with_alpha(success, self.tint_bg),
            success_border: with_alpha(success, self.tint_border),
            success_hover: with_alpha(success, self.tint_hover),

            // Standard rows feed the theme `--border-subtle` hex (NOT the legacy
            // translucent hairline the 4 P1 rows kept).
            border_subtle: self.border_subtle,
            border_muted,
            border_strong: self.border_strong,

            focus_ring: self.accent, // = accent (no Tauri token; new)

            favorite: self.danger, // loved-heart uses danger red
            card_shadow: LEGACY_CARD_SHADOW,

            alpha: alpha_ramp(is_light),
        }
    }
}

impl Default for StdSpec {
    /// All-black placeholder; every field is overwritten per theme. The default
    /// only exists so theme functions can use struct-update syntax for the tint
    /// fractions without repeating them.
    fn default() -> Self {
        let z = Rgba::rgb(0, 0, 0);
        StdSpec {
            bg_primary: z,
            bg_secondary: z,
            bg_tertiary: z,
            bg_hover: z,
            text_primary: z,
            text_secondary: z,
            text_muted: z,
            text_disabled: z,
            accent: z,
            accent_hover: z,
            accent_pressed: z,
            accent_text: z,
            danger: z,
            warning: z,
            tint_bg: StdSpec::TINT_BG,
            tint_border: StdSpec::TINT_BORDER,
            tint_hover: StdSpec::TINT_HOVER,
            border_subtle: z,
            border_strong: z,
        }
    }
}

/// True when a `bg-primary` reads as light (luminance >= 0.5). Drives polarity
/// for the standard rows. Matches `lib::is_light` (which calls through
/// `palette()`), but used internally to avoid a recursive `palette()` call.
fn bg_is_light(bg_primary: Rgba) -> bool {
    relative_luminance(bg_primary) >= 0.5
}

// ---------------------------------------------------------------------------
// Core themes (P1 originals + standard Light)
// ---------------------------------------------------------------------------

/// `:root` Dark — the base every other theme inherits omissions from.
/// All hex values cite `src/app.css :root` via the inventory doc.
fn dark() -> ThemeColors {
    let danger = Rgba::rgb(0xef, 0x44, 0x44); // --danger
    let warning = Rgba::rgb(0xfb, 0xbf, 0x24); // --warning
    let success = Rgba::rgb(0x3f, 0xae, 0x6a); // NEW (project green)
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

/// `light` — core Light theme. OMITS the accent trio (inherits the Dark blue
/// `#4285F4` family from `:root`); alpha base flips to black. (doc 01 §light)
fn light() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0xff, 0xff, 0xff),
        bg_secondary: Rgba::rgb(0xf5, 0xf5, 0xf5),
        bg_tertiary: Rgba::rgb(0xe8, 0xe8, 0xe8),
        bg_hover: Rgba::rgb(0xf0, 0xf0, 0xf0),
        text_primary: Rgba::rgb(0x0f, 0x0f, 0x0f),
        text_secondary: Rgba::rgb(0x44, 0x44, 0x44),
        text_muted: Rgba::rgb(0x66, 0x66, 0x66),
        text_disabled: Rgba::rgb(0x99, 0x99, 0x99),
        // accent trio inherited from :root Dark:
        accent: Rgba::rgb(0x42, 0x85, 0xf4),
        accent_hover: Rgba::rgb(0x5a, 0x9b, 0xf4),
        accent_pressed: Rgba::rgb(0x32, 0x75, 0xe4),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff), // --btn-primary-text
        // light defines its own danger/warning hues (darker):
        danger: Rgba::rgb(0xdc, 0x26, 0x26),
        warning: Rgba::rgb(0xd9, 0x77, 0x06),
        // light uses 0.1/0.3/0.15 in app.css (hover is 0.15 not 0.2). Keep faithful.
        tint_hover: 0.15,
        border_subtle: Rgba::rgb(0xe0, 0xe0, 0xe0),
        border_strong: Rgba::rgb(0xcc, 0xcc, 0xcc),
        ..Default::default()
    };
    s.build(true)
}

// ---------------------------------------------------------------------------
// Dark (branded / community) themes
// ---------------------------------------------------------------------------

fn warm() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x2b, 0x1a, 0x14),
        bg_secondary: Rgba::rgb(0x3a, 0x24, 0x1a),
        bg_tertiary: Rgba::rgb(0x4a, 0x2f, 0x23),
        bg_hover: Rgba::rgb(0x5b, 0x3a, 0x2e),
        text_primary: Rgba::rgb(0xf5, 0xe9, 0xe2),
        text_secondary: Rgba::rgb(0xd8, 0xc3, 0xb7),
        text_muted: Rgba::rgb(0xbf, 0xa3, 0x96),
        text_disabled: Rgba::rgb(0x8d, 0x73, 0x67),
        accent: Rgba::rgb(0xe5, 0x98, 0x66),
        accent_hover: Rgba::rgb(0xf0, 0xa7, 0x7b),
        accent_pressed: Rgba::rgb(0xd8, 0x86, 0x52),
        accent_text: Rgba::rgb(0x00, 0x00, 0x00),
        danger: Rgba::rgb(0xbf, 0x4f, 0x4f),
        warning: Rgba::rgb(0xd6, 0xa9, 0x4f),
        border_subtle: Rgba::rgb(0x4a, 0x2f, 0x23),
        border_strong: Rgba::rgb(0x5b, 0x3a, 0x2e),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn nord() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x1d, 0x22, 0x30),
        bg_secondary: Rgba::rgb(0x2a, 0x2f, 0x3c),
        bg_tertiary: Rgba::rgb(0x32, 0x38, 0x4c),
        bg_hover: Rgba::rgb(0x3c, 0x42, 0x56),
        text_primary: Rgba::rgb(0xec, 0xec, 0xec),
        text_secondary: Rgba::rgb(0xc6, 0xc6, 0xc6),
        text_muted: Rgba::rgb(0x99, 0x99, 0xa3),
        text_disabled: Rgba::rgb(0x6f, 0x6f, 0x7b),
        accent: Rgba::rgb(0x35, 0x84, 0xe4),
        accent_hover: Rgba::rgb(0x5f, 0x9e, 0xe6),
        accent_pressed: Rgba::rgb(0x1a, 0x5f, 0xb4),
        accent_text: Rgba::rgb(0x24, 0x1f, 0x31),
        danger: Rgba::rgb(0xc0, 0x1c, 0x28),
        warning: Rgba::rgb(0xf5, 0xc2, 0x11),
        border_subtle: Rgba::rgb(0x2a, 0x2f, 0x3c),
        border_strong: Rgba::rgb(0x32, 0x38, 0x4c),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn dracula() -> ThemeColors {
    // NON-STANDARD tint fractions: bg .15 / border .4 / hover .25.
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x28, 0x2a, 0x36),
        bg_secondary: Rgba::rgb(0x21, 0x22, 0x2c),
        bg_tertiary: Rgba::rgb(0x34, 0x37, 0x46),
        bg_hover: Rgba::rgb(0x44, 0x47, 0x5a),
        text_primary: Rgba::rgb(0xf8, 0xf8, 0xf2),
        text_secondary: Rgba::rgb(0xe2, 0xe2, 0xdc),
        text_muted: Rgba::rgb(0x62, 0x72, 0xa4),
        text_disabled: Rgba::rgb(0x44, 0x47, 0x5a),
        accent: Rgba::rgb(0xbd, 0x93, 0xf9),
        accent_hover: Rgba::rgb(0xff, 0x79, 0xc6),
        accent_pressed: Rgba::rgb(0x8b, 0xe9, 0xfd),
        accent_text: Rgba::rgb(0x28, 0x2a, 0x36),
        danger: Rgba::rgb(0xff, 0x55, 0x55),
        warning: Rgba::rgb(0xff, 0xb8, 0x6c),
        tint_bg: 0.15,
        tint_border: 0.4,
        tint_hover: 0.25,
        border_subtle: Rgba::rgb(0x34, 0x37, 0x46),
        border_strong: Rgba::rgb(0x44, 0x47, 0x5a),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn catppuccin_mocha() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x1e, 0x1e, 0x2e),
        bg_secondary: Rgba::rgb(0x18, 0x18, 0x25),
        bg_tertiary: Rgba::rgb(0x11, 0x11, 0x1b),
        bg_hover: Rgba::rgb(0x31, 0x32, 0x44),
        text_primary: Rgba::rgb(0xcd, 0xd6, 0xf4),
        text_secondary: Rgba::rgb(0xba, 0xc2, 0xde),
        text_muted: Rgba::rgb(0xa6, 0xad, 0xc8),
        text_disabled: Rgba::rgb(0x73, 0x79, 0x94),
        accent: Rgba::rgb(0xcb, 0xa6, 0xf7),
        accent_hover: Rgba::rgb(0x89, 0xb4, 0xfa),
        accent_pressed: Rgba::rgb(0xf3, 0x8b, 0xa8),
        accent_text: Rgba::rgb(0x1e, 0x1e, 0x2e),
        danger: Rgba::rgb(0xf3, 0x8b, 0xa8),
        warning: Rgba::rgb(0xf9, 0xe2, 0xaf),
        border_subtle: Rgba::rgb(0x31, 0x32, 0x44),
        border_strong: Rgba::rgb(0x45, 0x47, 0x5a),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

/// breeze-dark — OMITS the danger/warning families AND the alpha scale →
/// inherits them from `:root` Dark. We materialize the inherited danger/warning
/// hues (red `#ef4444`, amber `#fbbf24`) explicitly.
fn breeze_dark() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x14, 0x16, 0x18),
        bg_secondary: Rgba::rgb(0x20, 0x23, 0x26),
        bg_tertiary: Rgba::rgb(0x29, 0x2c, 0x30),
        bg_hover: Rgba::rgb(0x31, 0x36, 0x3b),
        text_primary: Rgba::rgb(0xff, 0xff, 0xff),
        text_secondary: Rgba::rgb(0xfc, 0xfc, 0xfc),
        text_muted: Rgba::rgb(0xa1, 0xa9, 0xb1),
        text_disabled: Rgba::rgb(0x31, 0x36, 0x3b),
        accent: Rgba::rgb(0x3d, 0xae, 0xe9),
        accent_hover: Rgba::rgb(0x9b, 0x59, 0xb6),
        accent_pressed: Rgba::rgb(0x1d, 0x99, 0xf3),
        accent_text: Rgba::rgb(0x14, 0x16, 0x18),
        // inherited from :root Dark:
        danger: Rgba::rgb(0xef, 0x44, 0x44),
        warning: Rgba::rgb(0xfb, 0xbf, 0x24),
        border_subtle: Rgba::rgb(0x29, 0x2c, 0x30),
        border_strong: Rgba::rgb(0x31, 0x36, 0x3b),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

/// adwaita-dark — OMITS the danger/warning families + alpha scale → inherits
/// from `:root` Dark.
fn adwaita_dark() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x1d, 0x1d, 0x20),
        bg_secondary: Rgba::rgb(0x22, 0x22, 0x26),
        bg_tertiary: Rgba::rgb(0x28, 0x28, 0x2c),
        bg_hover: Rgba::rgb(0x2e, 0x2e, 0x32),
        text_primary: Rgba::rgb(0xff, 0xff, 0xff),
        text_secondary: Rgba::rgb(0xff, 0xff, 0xff),
        text_muted: Rgba::rgb(0xb3, 0xb3, 0xb8),
        text_disabled: Rgba::rgb(0x2e, 0x2e, 0x32),
        accent: Rgba::rgb(0x35, 0x84, 0xe4),
        accent_hover: Rgba::rgb(0x1c, 0x71, 0xd8),
        accent_pressed: Rgba::rgb(0x1a, 0x5f, 0xb4),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff),
        // inherited from :root Dark:
        danger: Rgba::rgb(0xef, 0x44, 0x44),
        warning: Rgba::rgb(0xfb, 0xbf, 0x24),
        border_subtle: Rgba::rgb(0x28, 0x28, 0x2c),
        border_strong: Rgba::rgb(0x2e, 0x2e, 0x32),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn aurora() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x2e, 0x34, 0x40),
        bg_secondary: Rgba::rgb(0x3b, 0x42, 0x52),
        bg_tertiary: Rgba::rgb(0x43, 0x4c, 0x5e),
        bg_hover: Rgba::rgb(0x4c, 0x56, 0x6a),
        text_primary: Rgba::rgb(0xd8, 0xde, 0xe9),
        text_secondary: Rgba::rgb(0xe5, 0xe9, 0xf0),
        text_muted: Rgba::rgb(0xb4, 0x8e, 0xad),
        text_disabled: Rgba::rgb(0x4c, 0x56, 0x6a),
        accent: Rgba::rgb(0xa3, 0xbe, 0x8c),
        accent_hover: Rgba::rgb(0xeb, 0xcb, 0x8b),
        accent_pressed: Rgba::rgb(0xd0, 0x87, 0x70),
        accent_text: Rgba::rgb(0x2e, 0x34, 0x40),
        danger: Rgba::rgb(0xbf, 0x61, 0x6a),
        warning: Rgba::rgb(0xeb, 0xcb, 0x8b),
        border_subtle: Rgba::rgb(0x4c, 0x56, 0x6a),
        border_strong: Rgba::rgb(0x43, 0x4c, 0x5e),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn ikari() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x1c, 0x12, 0x39),
        bg_secondary: Rgba::rgb(0x24, 0x1a, 0x48),
        bg_tertiary: Rgba::rgb(0x30, 0x24, 0x58),
        bg_hover: Rgba::rgb(0x3c, 0x2f, 0x71),
        text_primary: Rgba::rgb(0xe8, 0xe6, 0xf2),
        text_secondary: Rgba::rgb(0xc6, 0xc2, 0xd8),
        text_muted: Rgba::rgb(0x95, 0x8f, 0xb5),
        text_disabled: Rgba::rgb(0x57, 0x4b, 0x79),
        accent: Rgba::rgb(0x7e, 0xda, 0x53),
        accent_hover: Rgba::rgb(0xa5, 0xf0, 0x66),
        accent_pressed: Rgba::rgb(0xd5, 0x8e, 0x27),
        accent_text: Rgba::rgb(0x1c, 0x12, 0x39),
        danger: Rgba::rgb(0xd8, 0x4a, 0x4a),
        warning: Rgba::rgb(0xe5, 0x9b, 0x2f),
        border_subtle: Rgba::rgb(0x30, 0x24, 0x58),
        border_strong: Rgba::rgb(0x3c, 0x2f, 0x71),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn ayanami() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x0f, 0x25, 0x3f),
        bg_secondary: Rgba::rgb(0x16, 0x3e, 0x60),
        bg_tertiary: Rgba::rgb(0x21, 0x4f, 0x7d),
        bg_hover: Rgba::rgb(0x2d, 0x63, 0x9f),
        text_primary: Rgba::rgb(0xf2, 0xf0, 0xe5),
        text_secondary: Rgba::rgb(0xd6, 0xd2, 0xc2),
        text_muted: Rgba::rgb(0x95, 0xa4, 0xb7),
        text_disabled: Rgba::rgb(0x2d, 0x63, 0x9f),
        accent: Rgba::rgb(0xe5, 0xb8, 0x2e),
        accent_hover: Rgba::rgb(0xf0, 0xcd, 0x63),
        accent_pressed: Rgba::rgb(0xcf, 0xa2, 0x2e),
        accent_text: Rgba::rgb(0x0f, 0x25, 0x3f),
        danger: Rgba::rgb(0xc0, 0x39, 0x2b),
        warning: Rgba::rgb(0xd8, 0x9b, 0x1c),
        border_subtle: Rgba::rgb(0x21, 0x4f, 0x7d),
        border_strong: Rgba::rgb(0x2d, 0x63, 0x9f),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn iscariot() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x2a, 0x10, 0x2a),
        bg_secondary: Rgba::rgb(0x38, 0x13, 0x3b),
        bg_tertiary: Rgba::rgb(0x45, 0x18, 0x46),
        bg_hover: Rgba::rgb(0x5d, 0x20, 0x60),
        text_primary: Rgba::rgb(0xf4, 0xea, 0xf5),
        text_secondary: Rgba::rgb(0xcf, 0xaa, 0xcb),
        text_muted: Rgba::rgb(0xa2, 0x78, 0xa6),
        text_disabled: Rgba::rgb(0x5d, 0x20, 0x60),
        accent: Rgba::rgb(0xe9, 0x4f, 0x94),
        accent_hover: Rgba::rgb(0xff, 0x7a, 0xbf),
        accent_pressed: Rgba::rgb(0xc9, 0x45, 0xa3),
        accent_text: Rgba::rgb(0x2a, 0x10, 0x2a),
        danger: Rgba::rgb(0xc0, 0x39, 0x2b),
        warning: Rgba::rgb(0xe5, 0xb6, 0x4b),
        border_subtle: Rgba::rgb(0x38, 0x13, 0x3b),
        border_strong: Rgba::rgb(0x5d, 0x20, 0x60),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn stratego() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x0a, 0x0a, 0x0b),
        bg_secondary: Rgba::rgb(0x14, 0x14, 0x18),
        bg_tertiary: Rgba::rgb(0x1d, 0x1e, 0x22),
        bg_hover: Rgba::rgb(0x28, 0x2a, 0x30),
        text_primary: Rgba::rgb(0xec, 0xe6, 0xd6),
        text_secondary: Rgba::rgb(0xb5, 0xaf, 0xa0),
        text_muted: Rgba::rgb(0x8a, 0x85, 0x7a),
        text_disabled: Rgba::rgb(0x4a, 0x48, 0x42),
        accent: Rgba::rgb(0xed, 0x2f, 0x3d),
        accent_hover: Rgba::rgb(0xf7, 0x4a, 0x58),
        accent_pressed: Rgba::rgb(0xc4, 0x1e, 0x2a),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff),
        danger: Rgba::rgb(0xe6, 0x39, 0x46),
        warning: Rgba::rgb(0xc4, 0xa5, 0x6a),
        border_subtle: Rgba::rgb(0x2a, 0x2a, 0x30),
        border_strong: Rgba::rgb(0x3a, 0x3a, 0x42),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn rumi() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x00, 0x00, 0x00),
        bg_secondary: Rgba::rgb(0x0d, 0x0d, 0x0d),
        bg_tertiary: Rgba::rgb(0x1a, 0x1a, 0x1a),
        bg_hover: Rgba::rgb(0x33, 0x33, 0x33),
        text_primary: Rgba::rgb(0xf0, 0xf0, 0xf0),
        text_secondary: Rgba::rgb(0xb2, 0xb2, 0xb2),
        text_muted: Rgba::rgb(0x80, 0x80, 0x80),
        text_disabled: Rgba::rgb(0x5a, 0x5a, 0x5a),
        accent: Rgba::rgb(0xe5, 0x8f, 0x24),
        accent_hover: Rgba::rgb(0xf0, 0xa5, 0x3c),
        accent_pressed: Rgba::rgb(0xcc, 0x7a, 0x12),
        accent_text: Rgba::rgb(0x00, 0x00, 0x00),
        danger: Rgba::rgb(0xe7, 0x4c, 0x3c),
        warning: Rgba::rgb(0xf3, 0x9c, 0x12),
        border_subtle: Rgba::rgb(0x1a, 0x1a, 0x1a),
        border_strong: Rgba::rgb(0x33, 0x33, 0x33),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn zoey() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x15, 0x1e, 0x2d),
        bg_secondary: Rgba::rgb(0x0e, 0x14, 0x1e),
        bg_tertiary: Rgba::rgb(0x10, 0x1a, 0x2a),
        bg_hover: Rgba::rgb(0x1b, 0x29, 0x3e),
        text_primary: Rgba::rgb(0xe0, 0xe2, 0xd5),
        text_secondary: Rgba::rgb(0xb5, 0xb7, 0xaa),
        text_muted: Rgba::rgb(0x8e, 0x90, 0x80),
        text_disabled: Rgba::rgb(0x60, 0x63, 0x54),
        accent: Rgba::rgb(0x46, 0xb4, 0xd3),
        accent_hover: Rgba::rgb(0x5c, 0xc0, 0xd9),
        accent_pressed: Rgba::rgb(0x3a, 0x97, 0xb6),
        accent_text: Rgba::rgb(0x15, 0x1e, 0x2d),
        danger: Rgba::rgb(0xbf, 0x61, 0x6a),
        warning: Rgba::rgb(0xd0, 0x87, 0x70),
        border_subtle: Rgba::rgb(0x0e, 0x14, 0x1e),
        border_strong: Rgba::rgb(0x1b, 0x29, 0x3e),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn mira() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x10, 0x18, 0x20),
        bg_secondary: Rgba::rgb(0x14, 0x1a, 0x28),
        bg_tertiary: Rgba::rgb(0x1d, 0x26, 0x35),
        bg_hover: Rgba::rgb(0x2a, 0x34, 0x48),
        text_primary: Rgba::rgb(0xe5, 0xe5, 0xe5),
        text_secondary: Rgba::rgb(0xb0, 0xb3, 0xc6),
        text_muted: Rgba::rgb(0x8a, 0x8d, 0xa0),
        text_disabled: Rgba::rgb(0x5c, 0x5e, 0x72),
        accent: Rgba::rgb(0xd9, 0x46, 0x85),
        accent_hover: Rgba::rgb(0xff, 0x00, 0x7f),
        accent_pressed: Rgba::rgb(0xff, 0xd7, 0x00), // intentional yellow
        accent_text: Rgba::rgb(0x10, 0x18, 0x20),
        danger: Rgba::rgb(0xc5, 0x30, 0x32),
        warning: Rgba::rgb(0xff, 0xd7, 0x00),
        border_subtle: Rgba::rgb(0x20, 0x2c, 0x3d),
        border_strong: Rgba::rgb(0x34, 0x40, 0x5a),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

/// frost — registered `type:light` in Tauri but a DARK Nord-polar canvas
/// (`#2e3440`). Polarity is luminance-derived, so it correctly resolves to a
/// white alpha base. (doc 01 §frost; corrected light/dark flag.)
fn frost() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x2e, 0x34, 0x40),
        bg_secondary: Rgba::rgb(0x3b, 0x42, 0x52),
        bg_tertiary: Rgba::rgb(0x43, 0x4c, 0x5e),
        bg_hover: Rgba::rgb(0x4c, 0x56, 0x6a),
        text_primary: Rgba::rgb(0xd8, 0xde, 0xe9),
        text_secondary: Rgba::rgb(0xe5, 0xe9, 0xf0),
        text_muted: Rgba::rgb(0x8f, 0xbc, 0xbb),
        text_disabled: Rgba::rgb(0x4c, 0x56, 0x6a),
        accent: Rgba::rgb(0x88, 0xc0, 0xd0),
        accent_hover: Rgba::rgb(0x81, 0xa1, 0xc1),
        accent_pressed: Rgba::rgb(0x5e, 0x81, 0xac),
        accent_text: Rgba::rgb(0x2e, 0x34, 0x40),
        danger: Rgba::rgb(0xbf, 0x61, 0x6a),
        warning: Rgba::rgb(0xd0, 0x87, 0x70),
        border_subtle: Rgba::rgb(0x4c, 0x56, 0x6a),
        border_strong: Rgba::rgb(0x43, 0x4c, 0x5e),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

/// langley — registered `type:light` in Tauri but a DEEP-MAROON dark canvas
/// (`#2c0a0a`). Luminance-derived polarity -> white alpha base. (doc 01 §langley)
fn langley() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x2c, 0x0a, 0x0a),
        bg_secondary: Rgba::rgb(0x3a, 0x0e, 0x0e),
        bg_tertiary: Rgba::rgb(0x4c, 0x14, 0x13),
        bg_hover: Rgba::rgb(0x71, 0x1c, 0x1c),
        text_primary: Rgba::rgb(0xf2, 0xda, 0xda),
        text_secondary: Rgba::rgb(0xd9, 0xa3, 0xa3),
        text_muted: Rgba::rgb(0xa9, 0x7b, 0x7b),
        text_disabled: Rgba::rgb(0x7a, 0x3d, 0x3d),
        accent: Rgba::rgb(0xe6, 0x7e, 0x22),
        accent_hover: Rgba::rgb(0xf3, 0x9c, 0x4d),
        accent_pressed: Rgba::rgb(0xd8, 0x6b, 0x1f),
        accent_text: Rgba::rgb(0x2c, 0x0a, 0x0a),
        danger: Rgba::rgb(0xc0, 0x39, 0x2b),
        warning: Rgba::rgb(0xe5, 0xa6, 0x3d),
        border_subtle: Rgba::rgb(0x3a, 0x0e, 0x0e),
        border_strong: Rgba::rgb(0x4c, 0x14, 0x13),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

// ---------------------------------------------------------------------------
// Light (branded / community) themes
// ---------------------------------------------------------------------------

/// alucard — light/cream theme (`#fffbeb` canvas). Luminance-derived -> black
/// alpha base. (doc 01 §alucard.)
fn alucard() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0xff, 0xfb, 0xeb),
        bg_secondary: Rgba::rgb(0xef, 0xed, 0xdc),
        bg_tertiary: Rgba::rgb(0xec, 0xe9, 0xdf),
        bg_hover: Rgba::rgb(0xcf, 0xcf, 0xde),
        text_primary: Rgba::rgb(0x1f, 0x1f, 0x1f),
        text_secondary: Rgba::rgb(0x6c, 0x66, 0x4b),
        text_muted: Rgba::rgb(0x9b, 0x92, 0x75),
        text_disabled: Rgba::rgb(0xbc, 0xba, 0xb3),
        accent: Rgba::rgb(0x64, 0x4a, 0xc9),
        accent_hover: Rgba::rgb(0xa3, 0x14, 0x4d),
        accent_pressed: Rgba::rgb(0x03, 0x6a, 0x96),
        accent_text: Rgba::rgb(0xff, 0xfb, 0xeb),
        danger: Rgba::rgb(0xcb, 0x3a, 0x2a),
        warning: Rgba::rgb(0xa3, 0x4d, 0x14),
        border_subtle: Rgba::rgb(0xec, 0xe9, 0xdf),
        border_strong: Rgba::rgb(0xde, 0xdc, 0xcf),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn rose_pine_dawn() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0xfa, 0xf4, 0xed),
        bg_secondary: Rgba::rgb(0xf4, 0xed, 0xe8),
        bg_tertiary: Rgba::rgb(0xdf, 0xda, 0xd9),
        bg_hover: Rgba::rgb(0xce, 0xca, 0xcd),
        text_primary: Rgba::rgb(0x57, 0x52, 0x79),
        text_secondary: Rgba::rgb(0x79, 0x75, 0x93),
        text_muted: Rgba::rgb(0x98, 0x93, 0xa5),
        text_disabled: Rgba::rgb(0xb5, 0xae, 0xbc),
        accent: Rgba::rgb(0xd7, 0x82, 0x7e),
        accent_hover: Rgba::rgb(0xe5, 0xa4, 0x78),
        accent_pressed: Rgba::rgb(0x28, 0x69, 0x83),
        accent_text: Rgba::rgb(0x57, 0x52, 0x79),
        danger: Rgba::rgb(0xb4, 0x63, 0x7a),
        warning: Rgba::rgb(0xea, 0x9d, 0x34),
        border_subtle: Rgba::rgb(0xce, 0xca, 0xcd),
        border_strong: Rgba::rgb(0xdf, 0xda, 0xd9),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn breeze_light() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0xff, 0xff, 0xff),
        bg_secondary: Rgba::rgb(0xf2, 0xf2, 0xf2),
        bg_tertiary: Rgba::rgb(0xe5, 0xe5, 0xe5),
        bg_hover: Rgba::rgb(0xdc, 0xdc, 0xdc),
        text_primary: Rgba::rgb(0x31, 0x36, 0x3b),
        text_secondary: Rgba::rgb(0x5c, 0x61, 0x66),
        text_muted: Rgba::rgb(0x7d, 0x81, 0x86),
        text_disabled: Rgba::rgb(0xa1, 0xa5, 0xa9),
        accent: Rgba::rgb(0x1d, 0x99, 0xf3),
        accent_hover: Rgba::rgb(0x3d, 0xae, 0xe9),
        accent_pressed: Rgba::rgb(0x00, 0x78, 0xd4),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff),
        danger: Rgba::rgb(0xc3, 0x27, 0x2b),
        warning: Rgba::rgb(0xf5, 0x97, 0x00),
        border_subtle: Rgba::rgb(0xd0, 0xd4, 0xd8),
        border_strong: Rgba::rgb(0xb7, 0xbd, 0xc2),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn adwaita_light() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0xff, 0xff, 0xff),
        bg_secondary: Rgba::rgb(0xf6, 0xf5, 0xf4),
        bg_tertiary: Rgba::rgb(0xea, 0xe9, 0xe7),
        bg_hover: Rgba::rgb(0xdc, 0xd9, 0xd7),
        text_primary: Rgba::rgb(0x24, 0x1f, 0x31),
        text_secondary: Rgba::rgb(0x5f, 0x5b, 0x6b),
        text_muted: Rgba::rgb(0x7f, 0x7b, 0x8c),
        text_disabled: Rgba::rgb(0xb1, 0xae, 0xbc),
        accent: Rgba::rgb(0x1e, 0x78, 0xe4),
        accent_hover: Rgba::rgb(0x3f, 0x8e, 0xf0),
        accent_pressed: Rgba::rgb(0x15, 0x5a, 0x9c),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff),
        danger: Rgba::rgb(0xc0, 0x1c, 0x28),
        warning: Rgba::rgb(0xf5, 0xc2, 0x11),
        border_subtle: Rgba::rgb(0xdc, 0xd9, 0xd7),
        border_strong: Rgba::rgb(0xc6, 0xc2, 0xcf),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn duotone_snow() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0xff, 0xff, 0xff),
        bg_secondary: Rgba::rgb(0xf8, 0xf9, 0xfa),
        bg_tertiary: Rgba::rgb(0xef, 0xf1, 0xf5),
        bg_hover: Rgba::rgb(0xe6, 0xe8, 0xec),
        text_primary: Rgba::rgb(0x4a, 0x59, 0x6e),
        text_secondary: Rgba::rgb(0x6b, 0x73, 0x8a),
        text_muted: Rgba::rgb(0x8c, 0x95, 0xa8),
        text_disabled: Rgba::rgb(0xb0, 0xb4, 0xc1),
        accent: Rgba::rgb(0x4a, 0x82, 0xd8),
        accent_hover: Rgba::rgb(0x6b, 0x9b, 0xe0),
        accent_pressed: Rgba::rgb(0x3a, 0x6f, 0xc2),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff),
        danger: Rgba::rgb(0xd3, 0x7e, 0x7e),
        warning: Rgba::rgb(0xc0, 0x9c, 0x4a),
        border_subtle: Rgba::rgb(0xdf, 0xe3, 0xe8),
        border_strong: Rgba::rgb(0xc9, 0xce, 0xd4),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn snow_storm() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0xec, 0xef, 0xf4),
        bg_secondary: Rgba::rgb(0xe5, 0xe9, 0xf0),
        bg_tertiary: Rgba::rgb(0xd8, 0xde, 0xe9),
        bg_hover: Rgba::rgb(0xcb, 0xd5, 0xe0),
        text_primary: Rgba::rgb(0x2e, 0x34, 0x40),
        text_secondary: Rgba::rgb(0x3b, 0x42, 0x52),
        text_muted: Rgba::rgb(0x43, 0x4c, 0x5e),
        text_disabled: Rgba::rgb(0x4c, 0x56, 0x6a),
        accent: Rgba::rgb(0x5e, 0x81, 0xac),
        accent_hover: Rgba::rgb(0x81, 0xa1, 0xc1),
        accent_pressed: Rgba::rgb(0x88, 0xc0, 0xd0),
        accent_text: Rgba::rgb(0x2e, 0x34, 0x40),
        danger: Rgba::rgb(0xbf, 0x61, 0x6a),
        warning: Rgba::rgb(0xd0, 0x87, 0x70),
        border_subtle: Rgba::rgb(0xd8, 0xde, 0xe9),
        border_strong: Rgba::rgb(0xe5, 0xe9, 0xf0),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

fn kurosaki() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0xfb, 0xf9, 0xf2),
        bg_secondary: Rgba::rgb(0xf3, 0xf0, 0xe8),
        bg_tertiary: Rgba::rgb(0xe8, 0xe2, 0xd4),
        bg_hover: Rgba::rgb(0xe1, 0xda, 0xc8),
        text_primary: Rgba::rgb(0x26, 0x24, 0x24),
        text_secondary: Rgba::rgb(0x54, 0x4d, 0x48),
        text_muted: Rgba::rgb(0x82, 0x7d, 0x78),
        text_disabled: Rgba::rgb(0xb3, 0xad, 0xa7),
        accent: Rgba::rgb(0xd5, 0xbe, 0x58),
        accent_hover: Rgba::rgb(0xe8, 0xce, 0x66),
        accent_pressed: Rgba::rgb(0xb4, 0x9f, 0x45),
        accent_text: Rgba::rgb(0x26, 0x24, 0x24),
        danger: Rgba::rgb(0xc0, 0x39, 0x2b),
        warning: Rgba::rgb(0xd8, 0x9b, 0x1c),
        border_subtle: Rgba::rgb(0xe4, 0xdc, 0xb4),
        border_strong: Rgba::rgb(0xd1, 0xc7, 0xaa),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
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
    use crate::colors::{alpha_byte, ALPHA_COUNT};
    use crate::id::ALL;

    /// Sentinel for "field was never set" — the all-zero opaque/transparent
    /// black the `StdSpec::default()` placeholder uses. A fully-materialized row
    /// must not leave a meaningful color at this sentinel by accident; we instead
    /// assert the named hero tokens are the EXACT transcribed values per theme in
    /// the dedicated tests below, and assert global completeness here.
    fn fully_populated(c: &ThemeColors) {
        assert_eq!(c.alpha.len(), ALPHA_COUNT);
        // every alpha tier carries opacity
        assert!(c.alpha.iter().all(|a| a.a > 0));
        // status families are present (non-degenerate alpha)
        assert!(c.danger_bg.a > 0 && c.danger_border.a > 0 && c.danger_hover.a > 0);
        assert!(c.warning_bg.a > 0 && c.warning_border.a > 0 && c.warning_hover.a > 0);
        assert!(c.success_bg.a > 0 && c.success_border.a > 0 && c.success_hover.a > 0);
        // surfaces/text/accent are opaque
        assert_eq!(c.surface_main.a, 255);
        assert_eq!(c.text_primary.a, 255);
        assert_eq!(c.accent.a, 255);
        assert_eq!(c.accent_text.a, 255);
        assert_eq!(c.border_strong.a, 255);
        assert_eq!(c.focus_ring.a, 255);
    }

    #[test]
    fn every_registered_theme_is_fully_populated() {
        for &id in ALL {
            let c = palette(id);
            fully_populated(&c);
        }
    }

    #[test]
    fn p1_rows_are_fully_populated() {
        for id in [ThemeId::Dark, ThemeId::Oled, ThemeId::TokyoNight, ThemeId::System] {
            let c = palette(id);
            assert_eq!(c.alpha.len(), ALPHA_COUNT);
            assert_ne!(c.surface_main, Rgba::rgba(0, 0, 0, 0));
            assert_ne!(c.text_primary, Rgba::rgba(0, 0, 0, 0));
            assert_ne!(c.accent, Rgba::rgba(0, 0, 0, 0));
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
        // The exact translucent values the old Slint Theme exposed (dark themes).
        for id in [ThemeId::Dark, ThemeId::Oled, ThemeId::TokyoNight] {
            let c = palette(id);
            assert_eq!(c.surface_hover, Rgba::rgba(255, 255, 255, 0x10));
            assert_eq!(c.border_subtle, Rgba::rgba(255, 255, 255, 0x14));
            assert_eq!(c.border_muted, Rgba::rgba(255, 255, 255, 0x38));
            assert_eq!(c.card_shadow, Rgba::rgba(0, 0, 0, 0x66));
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

    // --- Standard themes: spot-check exact transcribed hero values ---------

    #[test]
    fn light_core_values() {
        let c = palette(ThemeId::Light);
        assert_eq!(c.surface_main, Rgba::rgb(0xff, 0xff, 0xff));
        assert_eq!(c.text_primary, Rgba::rgb(0x0f, 0x0f, 0x0f));
        // accent trio inherited from :root Dark:
        assert_eq!(c.accent, Rgba::rgb(0x42, 0x85, 0xf4));
        assert_eq!(c.accent_text, Rgba::rgb(0xff, 0xff, 0xff));
        assert_eq!(c.danger, Rgba::rgb(0xdc, 0x26, 0x26));
        assert_eq!(c.border_subtle, Rgba::rgb(0xe0, 0xe0, 0xe0));
        // light theme -> black alpha base
        assert_eq!(c.alpha_pct(8), Rgba::rgba(0, 0, 0, 0x14));
        // light hover tint is 0.15 (faithful to app.css)
        assert_eq!(c.danger_hover, with_alpha(Rgba::rgb(0xdc, 0x26, 0x26), 0.15));
    }

    #[test]
    fn dracula_nonstandard_tints() {
        let c = palette(ThemeId::Dracula);
        assert_eq!(c.surface_main, Rgba::rgb(0x28, 0x2a, 0x36));
        assert_eq!(c.accent, Rgba::rgb(0xbd, 0x93, 0xf9));
        let danger = Rgba::rgb(0xff, 0x55, 0x55);
        assert_eq!(c.danger_bg, with_alpha(danger, 0.15));
        assert_eq!(c.danger_border, with_alpha(danger, 0.4));
        assert_eq!(c.danger_hover, with_alpha(danger, 0.25));
    }

    #[test]
    fn breeze_dark_inherits_root_status_hues() {
        let c = palette(ThemeId::BreezeDark);
        // danger/warning inherited from :root Dark
        assert_eq!(c.danger, Rgba::rgb(0xef, 0x44, 0x44));
        assert_eq!(c.warning, Rgba::rgb(0xfb, 0xbf, 0x24));
        assert_eq!(c.accent, Rgba::rgb(0x3d, 0xae, 0xe9));
    }

    #[test]
    fn frost_langley_are_dark_polarity() {
        // Both are registered type:light in Tauri but are DARK canvases.
        let frost = palette(ThemeId::Frost);
        let langley = palette(ThemeId::Langley);
        // white alpha base (dark polarity), NOT black:
        assert_eq!(frost.alpha_pct(8), Rgba::rgba(255, 255, 255, 0x14));
        assert_eq!(langley.alpha_pct(8), Rgba::rgba(255, 255, 255, 0x14));
        assert!(!bg_is_light(frost.surface_main));
        assert!(!bg_is_light(langley.surface_main));
    }

    #[test]
    fn alucard_is_light_polarity() {
        let c = palette(ThemeId::Alucard);
        assert_eq!(c.surface_main, Rgba::rgb(0xff, 0xfb, 0xeb));
        // black alpha base (light polarity):
        assert_eq!(c.alpha_pct(8), Rgba::rgba(0, 0, 0, 0x14));
        assert!(bg_is_light(c.surface_main));
        // success on a light theme is the darker green:
        assert_eq!(c.success, Rgba::rgb(0x1f, 0x8a, 0x4c));
    }

    #[test]
    fn light_themes_use_black_alpha_base() {
        for id in [
            ThemeId::Light,
            ThemeId::Alucard,
            ThemeId::RosePineDawn,
            ThemeId::BreezeLight,
            ThemeId::AdwaitaLight,
            ThemeId::DuotoneSnow,
            ThemeId::SnowStorm,
            ThemeId::Kurosaki,
        ] {
            let c = palette(id);
            assert_eq!(c.alpha_pct(8), Rgba::rgba(0, 0, 0, 0x14), "{id:?} should be black-base");
            assert_eq!(c.surface_hover, Rgba::rgba(0, 0, 0, 0x10), "{id:?} hover base");
        }
    }

    #[test]
    fn dark_themes_use_white_alpha_base() {
        for id in [
            ThemeId::Warm,
            ThemeId::Nord,
            ThemeId::Dracula,
            ThemeId::CatppuccinMocha,
            ThemeId::BreezeDark,
            ThemeId::AdwaitaDark,
            ThemeId::Aurora,
            ThemeId::Ikari,
            ThemeId::Ayanami,
            ThemeId::Iscariot,
            ThemeId::Stratego,
            ThemeId::Rumi,
            ThemeId::Zoey,
            ThemeId::Mira,
            ThemeId::Frost,
            ThemeId::Langley,
        ] {
            let c = palette(id);
            assert_eq!(c.alpha_pct(8), Rgba::rgba(255, 255, 255, 0x14), "{id:?} should be white-base");
        }
    }

    #[test]
    fn standard_theme_focus_ring_equals_accent() {
        for id in [
            ThemeId::Warm,
            ThemeId::Nord,
            ThemeId::Stratego,
            ThemeId::Alucard,
            ThemeId::Kurosaki,
        ] {
            let c = palette(id);
            assert_eq!(c.focus_ring, c.accent, "{id:?} focus_ring should equal accent");
            assert_eq!(c.favorite, c.danger, "{id:?} favorite should equal danger");
        }
    }

    #[test]
    fn alpha_byte_helper_matches_with_alpha() {
        // sanity: with_alpha(.., 0.1) == alpha_byte(10)
        let c = Rgba::rgb(0x10, 0x20, 0x30);
        assert_eq!(with_alpha(c, 0.1).a, alpha_byte(10));
    }
}
