//! Theme controller (phase 19) — the Slint `crates/qbz/src/theme.rs` port:
//! the persisted `ui_prefs.theme` slug resolves through the SAME `qbz-theme`
//! registry (`ThemeId`/`palette()`), and the materialized `ThemeColors` are
//! serialized to ONE JSON document that QbzTheme.qml binds to — the Qt
//! equivalent of `theme::push_colors` (live switch, no restart).
//!
//! Synthetic slugs (theme.rs): "auto" (AutoSource::System regeneration —
//! falls back to OLED when headless generation fails) and "custom"
//! (`<data_dir>/qbz/custom_theme.json`, the SAME file the Slint custom-theme
//! editor writes; read-only apply here — POC-NOTE: no token editor). The
//! "system" registry theme resolves via the OS palette in Slint; the POC
//! maps it to the Dark palette (POC-NOTE).

use cxx_qt_lib::QString;
use qbz_theme::{Rgba, ThemeColors, ThemeId};
use serde::Serialize;

// ---------------------------------------------------------------------------
// ui_prefs.json keys (theme + theme_filter) — additive patch like
// settings_qt.rs so every other Slint key survives.
// ---------------------------------------------------------------------------

fn prefs_path() -> Option<std::path::PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("ui_prefs.json"))
}

fn read_pref(key: &str) -> Option<serde_json::Value> {
    let path = prefs_path()?;
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get(key).cloned())
}

fn write_pref(key: &str, value: serde_json::Value) {
    let Some(path) = prefs_path() else {
        return;
    };
    let mut doc: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = doc.as_object_mut() {
        obj.insert(key.to_string(), value);
        if let Ok(text) = serde_json::to_string_pretty(&doc) {
            let _ = std::fs::write(&path, text);
        }
    }
}

/// The persisted theme slug (ui_prefs.rs default: "oled").
pub fn current_slug() -> String {
    read_pref("theme")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "oled".to_string())
}

/// The dropdown filter (theme.rs: 0 All / 1 Dark / 2 Light; default 0).
pub fn theme_filter() -> i32 {
    read_pref("theme_filter")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32
}

pub fn set_theme_filter(index: i32) {
    write_pref("theme_filter", serde_json::json!(index.clamp(0, 2)));
}

/// Persist a new slug (additive); the caller republishes `themeJson`.
pub fn set_theme(slug: &str) {
    write_pref("theme", serde_json::Value::String(slug.to_string()));
    log::info!("[qbz-qt] theme -> {slug}");
}

// ---------------------------------------------------------------------------
// Slug -> ThemeColors (theme.rs apply_theme / auto / custom startup dispatch)
// ---------------------------------------------------------------------------

fn custom_theme_path() -> Option<std::path::PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("custom_theme.json"))
}

fn custom_colors() -> ThemeColors {
    let base = custom_theme_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str::<qbz_theme::CustomThemeBase>(&text).ok())
        .unwrap_or_else(qbz_theme::CustomThemeBase::default_oled);
    qbz_theme::theme_from_base(&base)
}

fn auto_colors() -> ThemeColors {
    match qbz_theme::generate_auto_theme(&qbz_theme::AutoSource::System) {
        Ok(c) => c,
        Err(e) => {
            // Headless / no DE portal: keep the row selectable but fall back
            // to the default dark tokens (POC-NOTE).
            log::warn!("[qbz-qt] auto theme generation failed ({e}); falling back to OLED");
            qbz_theme::palette(qbz_theme::default_theme_id())
        }
    }
}

/// Resolve a slug to its materialized colors (unknown -> OLED, theme.rs
/// `id_for_slug` fallback).
pub fn colors_for_slug(slug: &str) -> ThemeColors {
    match slug {
        "auto" => auto_colors(),
        "custom" => custom_colors(),
        // POC-NOTE: "system" reads the OS palette in Slint; the POC maps it
        // to the Dark registry palette.
        "system" => qbz_theme::palette(ThemeId::Dark),
        other => ThemeId::from_slug(other)
            .map(qbz_theme::palette)
            .unwrap_or_else(|| qbz_theme::palette(qbz_theme::default_theme_id())),
    }
}

// ---------------------------------------------------------------------------
// ThemeColors -> QML token JSON (theme::push_colors equivalent)
// ---------------------------------------------------------------------------

/// QML color literal: "#aarrggbb" (Qt ARGB — the notation QbzTheme.qml
/// already uses).
fn argb(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c.a, c.r, c.g, c.b)
}

fn with_alpha(c: Rgba, a: u8) -> String {
    argb(Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a,
    })
}

/// The alpha-ramp percents (colors.rs:12-17) — indices into the 24 tiers.
const ALPHA_PCTS: [u32; 24] = [
    4, 5, 6, 8, 10, 12, 15, 18, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95,
];

#[derive(Serialize)]
struct ThemeTokens {
    // 30 named colors (ThemeColors contract).
    #[serde(rename = "surfaceMain")]
    surface_main: String,
    #[serde(rename = "surfaceCard")]
    surface_card: String,
    #[serde(rename = "surfaceElevated")]
    surface_elevated: String,
    #[serde(rename = "surfaceHover")]
    surface_hover: String,
    #[serde(rename = "bgHover")]
    bg_hover: String,
    #[serde(rename = "textPrimary")]
    text_primary: String,
    #[serde(rename = "textSecondary")]
    text_secondary: String,
    #[serde(rename = "textMuted")]
    text_muted: String,
    #[serde(rename = "textDisabled")]
    text_disabled: String,
    accent: String,
    #[serde(rename = "accentHover")]
    accent_hover: String,
    #[serde(rename = "accentPressed")]
    accent_pressed: String,
    #[serde(rename = "accentText")]
    accent_text: String,
    danger: String,
    #[serde(rename = "dangerBg")]
    danger_bg: String,
    #[serde(rename = "dangerBorder")]
    danger_border: String,
    #[serde(rename = "dangerHover")]
    danger_hover: String,
    warning: String,
    #[serde(rename = "warningBg")]
    warning_bg: String,
    #[serde(rename = "warningBorder")]
    warning_border: String,
    #[serde(rename = "warningHover")]
    warning_hover: String,
    success: String,
    #[serde(rename = "successBg")]
    success_bg: String,
    #[serde(rename = "successBorder")]
    success_border: String,
    #[serde(rename = "successHover")]
    success_hover: String,
    #[serde(rename = "borderSubtle")]
    border_subtle: String,
    #[serde(rename = "borderMuted")]
    border_muted: String,
    #[serde(rename = "borderStrong")]
    border_strong: String,
    #[serde(rename = "focusRing")]
    focus_ring: String,
    favorite: String,
    #[serde(rename = "cardShadow")]
    card_shadow: String,
    // 24 alpha tiers (alpha4..alpha95, white-based dark / black-based light).
    alpha: Vec<String>,
    // Ambient-layer derivations (phase-14 tokens, now theme-derived):
    // chrome surface-card @50%, frosted panel surface-main @22%, thin bars
    // surface-main @30%, hairline = alpha tier 10%.
    #[serde(rename = "surfaceCardA50")]
    surface_card_a50: String,
    #[serde(rename = "surfaceMainA22")]
    surface_main_a22: String,
    #[serde(rename = "surfaceMainA30")]
    surface_main_a30: String,
    #[serde(rename = "frostBorder")]
    frost_border: String,
    // Flags (Theme.is-dark).
    #[serde(rename = "isDark")]
    is_dark: bool,
}

fn tokens_for(colors: &ThemeColors) -> ThemeTokens {
    let is_dark = qbz_theme::relative_luminance(colors.surface_main) < 0.5;
    ThemeTokens {
        surface_main: argb(colors.surface_main),
        surface_card: argb(colors.surface_card),
        surface_elevated: argb(colors.surface_elevated),
        surface_hover: argb(colors.surface_hover),
        bg_hover: argb(colors.bg_hover),
        text_primary: argb(colors.text_primary),
        text_secondary: argb(colors.text_secondary),
        text_muted: argb(colors.text_muted),
        text_disabled: argb(colors.text_disabled),
        accent: argb(colors.accent),
        accent_hover: argb(colors.accent_hover),
        accent_pressed: argb(colors.accent_pressed),
        accent_text: argb(colors.accent_text),
        danger: argb(colors.danger),
        danger_bg: argb(colors.danger_bg),
        danger_border: argb(colors.danger_border),
        danger_hover: argb(colors.danger_hover),
        warning: argb(colors.warning),
        warning_bg: argb(colors.warning_bg),
        warning_border: argb(colors.warning_border),
        warning_hover: argb(colors.warning_hover),
        success: argb(colors.success),
        success_bg: argb(colors.success_bg),
        success_border: argb(colors.success_border),
        success_hover: argb(colors.success_hover),
        border_subtle: argb(colors.border_subtle),
        border_muted: argb(colors.border_muted),
        border_strong: argb(colors.border_strong),
        focus_ring: argb(colors.focus_ring),
        favorite: argb(colors.favorite),
        card_shadow: argb(colors.card_shadow),
        alpha: colors.alpha.iter().map(|c| argb(*c)).collect(),
        surface_card_a50: with_alpha(colors.surface_card, 0x80),
        surface_main_a22: with_alpha(colors.surface_main, 0x38),
        surface_main_a30: with_alpha(colors.surface_main, 0x4d),
        frost_border: argb(colors.alpha[ALPHA_PCTS.iter().position(|p| *p == 10).unwrap_or(4)]),
        is_dark,
    }
}

/// The current theme as the QML token document.
pub fn theme_json() -> String {
    let colors = colors_for_slug(&current_slug());
    serde_json::to_string(&tokens_for(&colors)).unwrap_or_else(|_| "{}".into())
}

/// Re-resolve + republish after a slug change (the push_colors moment).
pub fn publish_theme() {
    let json = theme_json();
    let slug = current_slug();
    crate::shell_bridge::ui(move |mut b| {
        b.as_mut().set_theme_json(QString::from(json.as_str()));
        b.as_mut().set_theme_slug(QString::from(slug.as_str()));
    });
}

// ---------------------------------------------------------------------------
// The dropdown catalog (theme.rs dropdown_labels + the 2 synthetic rows)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ThemeEntry {
    label: String,
    slug: String,
    #[serde(rename = "isLight")]
    is_light: bool,
}

/// All 36 registry themes + "Auto (dynamic)" + "Custom" (theme.rs:128-142,
/// 217-227 — the synthetics only show under the All filter; QML filters).
pub fn theme_list_json() -> String {
    let mut entries: Vec<ThemeEntry> = qbz_theme::theme_list()
        .into_iter()
        .map(|e| ThemeEntry {
            label: e.display_name.to_string(),
            slug: e.slug.to_string(),
            is_light: e.is_light,
        })
        .collect();
    entries.push(ThemeEntry {
        label: "Auto (dynamic)".to_string(),
        slug: "auto".to_string(),
        is_light: false,
    });
    entries.push(ThemeEntry {
        label: "Custom".to_string(),
        slug: "custom".to_string(),
        is_light: false,
    });
    serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into())
}
