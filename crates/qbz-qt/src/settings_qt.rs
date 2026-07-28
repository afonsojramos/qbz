//! Settings > Audio + Playback controller — Slint-free port of
//! `crates/qbz/src/settings.rs` onto the SAME public backend stores:
//! `AudioSettingsStore` (get/set + `Player::reload_settings` /
//! `Player::reinit_device` apply — the audio backend is PROTECTED: only
//! these public calls), `PlaybackPreferencesStore`, the shared
//! `ui_prefs.json` (streaming quality), and the QConnect key/value DB
//! (rusqlite, same file as the Slint app).
//!
//! Also owns device enumeration (`BackendManager::create_backend(type)
//! .enumerate_devices()` — public) with the Tauri ALSA section grouping,
//! and the cross-setting cascades from settings.rs (dac-passthrough,
//! streaming-only, backend switch).
//!
//! POC-NOTEs:
//! - Detected device limit row (#638): the probe + its cache live in
//!   `crate::device_cap` (Slint glue) — not wired; the row stays hidden.
//! - HiFi Wizard, JACK banner (no JACK in the POC build), settings
//!   export/import, the bit-perfect force-100 volume cascade: not wired.
//! - qconnect startup/device-name persist to the SAME qconnect_settings.db
//!   (wired) but do NOT drive a live QConnect service (none in the POC) —
//!   they take effect on the next connection, like upstream.

use std::sync::{Arc, Mutex, OnceLock};

use cxx_qt_lib::QString;
use qbz_app::settings::playback::{
    AutoplayMode, PlaybackPreferencesState, PlaybackPreferencesStore,
};
use qbz_app::shell::AppRuntime;
use qbz_audio::backend::{AlsaPlugin, AudioBackendType, BackendManager};
use qbz_audio::settings::{AudioSettingsState, AudioSettingsStore};
use qbz_core::LoggingAdapter;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Stores (shared files with the Slint app)
// ---------------------------------------------------------------------------

static AUDIO: OnceLock<AudioSettingsState> = OnceLock::new();
static PLAYBACK: OnceLock<PlaybackPreferencesState> = OnceLock::new();

fn audio() -> &'static AudioSettingsState {
    AUDIO.get_or_init(|| {
        AudioSettingsState::new().unwrap_or_else(|e| {
            log::warn!("[qbz-qt] audio settings store unavailable: {e}");
            AudioSettingsState::new_empty()
        })
    })
}

fn playback() -> &'static PlaybackPreferencesState {
    PLAYBACK.get_or_init(|| {
        PlaybackPreferencesState::new().unwrap_or_else(|e| {
            log::warn!("[qbz-qt] playback preferences store unavailable: {e}");
            PlaybackPreferencesState::new_empty()
        })
    })
}

fn with_audio<T>(f: impl FnOnce(&AudioSettingsStore) -> Result<T, String>) -> Result<T, String> {
    let guard = audio().store.lock().map_err(|_| "audio store lock poisoned".to_string())?;
    let store = guard.as_ref().ok_or_else(|| "audio settings store not open".to_string())?;
    f(store)
}

fn with_playback<T>(
    f: impl FnOnce(&PlaybackPreferencesStore) -> Result<T, String>,
) -> Result<T, String> {
    let guard = playback()
        .store
        .lock()
        .map_err(|_| "playback preferences lock poisoned".to_string())?;
    let store = guard
        .as_ref()
        .ok_or_else(|| "playback preferences store not open".to_string())?;
    f(store)
}

// ---------------------------------------------------------------------------
// ui_prefs.json (streaming quality) — shared file, patched key-by-key so
// every OTHER Slint key survives.
// ---------------------------------------------------------------------------

fn prefs_path() -> Option<std::path::PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("ui_prefs.json"))
}

pub fn streaming_quality() -> String {
    let Some(path) = prefs_path() else {
        return "hires_plus".to_string();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("streaming_quality").and_then(|q| q.as_str().map(str::to_string)))
        .unwrap_or_else(|| "hires_plus".to_string())
}

fn save_streaming_quality(key: &str) {
    let Some(path) = prefs_path() else {
        return;
    };
    let mut value: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "streaming_quality".to_string(),
            serde_json::Value::String(key.to_string()),
        );
        if let Ok(text) = serde_json::to_string_pretty(&value) {
            let _ = std::fs::write(&path, text);
        }
    }
}

// ---------------------------------------------------------------------------
// ui_prefs.json (window chrome, phase 12) — the Slint `use_system_title_bar`
// pref (crates/qbz/src/ui_prefs.rs): SAME shared file, additive key patch so
// every other Slint key survives. Default TRUE on Linux (the Slint default
// is `!macos` — Linux keeps the system decorations). Applied at startup
// only: decorations negotiate at surface creation on Wayland, so a toggle
// takes effect on the next launch (restart semantics, 1:1 Slint).
// ---------------------------------------------------------------------------

pub fn use_system_title_bar() -> bool {
    let Some(path) = prefs_path() else {
        return true;
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("use_system_title_bar").and_then(|q| q.as_bool()))
        .unwrap_or(true)
}

/// Flip + persist the pref; returns the new value (for the menu state).
pub fn toggle_system_title_bar() -> bool {
    let next = !use_system_title_bar();
    if let Some(path) = prefs_path() {
        let mut value: serde_json::Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "use_system_title_bar".to_string(),
                serde_json::Value::Bool(next),
            );
            if let Ok(text) = serde_json::to_string_pretty(&value) {
                let _ = std::fs::write(&path, text);
            }
        }
    }
    next
}

// ---------------------------------------------------------------------------
// ui_prefs.json (app-wide ambient background, phase 14) — the Slint
// `app_background` enum key: "off" | "ambient" | "blurred"
// (crates/qbz/src/ui_prefs.rs; default "off", the owner's store carries
// "ambient"). Same additive key patch.
// ---------------------------------------------------------------------------

/// Mode index, ui_prefs.rs `app_background_index` semantics, except the POC
/// has no blurred-art route: 0 = off, 1 = on (ambient look) for BOTH
/// "ambient" and "blurred" (see ambient_qt.rs POC-NOTE).
pub fn app_background_mode() -> i32 {
    let Some(path) = prefs_path() else {
        return 0;
    };
    let key = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| {
            v.get("app_background")
                .and_then(|q| q.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| "off".to_string());
    if key == "off" { 0 } else { 1 }
}

/// Live-tuning knobs (Slint AppearanceState defaults; the QBZ_BG_* envs are
/// the same dev knobs the Slint seeds at startup).
pub fn ambient_dim() -> f32 {
    std::env::var("QBZ_BG_DIM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.35)
}
pub fn ambient_surface_alpha() -> f32 {
    std::env::var("QBZ_BG_SURFACE_ALPHA")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.5)
}
pub fn ambient_bar_alpha() -> f32 {
    std::env::var("QBZ_BG_BAR_ALPHA")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.3)
}

/// Flip + persist off <-> "ambient" (the owner's mode; "blurred" is not a
/// POC route). Returns the new mode index (0/1).
pub fn toggle_ambient_background() -> i32 {
    let next = app_background_mode() == 0;
    if let Some(path) = prefs_path() {
        let mut value: serde_json::Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "app_background".to_string(),
                serde_json::Value::String(if next { "ambient" } else { "off" }.to_string()),
            );
            if let Ok(text) = serde_json::to_string_pretty(&value) {
                let _ = std::fs::write(&path, text);
            }
        }
    }
    if next { 1 } else { 0 }
}

// ---------------------------------------------------------------------------
// ui_prefs.json (now-playing bar mode, phase 18) — the Slint `npb_mode` key:
// "new" | "classic" | "small" | "large" (ui_prefs.rs:357-360; maps to
// ShellState.npb-mode 0/1/2/3). Same additive key patch.
// ---------------------------------------------------------------------------

/// The persisted mode as a bridge index (unknown keys fall back to "new").
pub fn npb_mode_index() -> i32 {
    let Some(path) = prefs_path() else {
        return 0;
    };
    let key = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("npb_mode").and_then(|q| q.as_str().map(str::to_string)))
        .unwrap_or_else(|| "new".to_string());
    match key.as_str() {
        "classic" => 1,
        "small" => 2,
        "large" => 3,
        _ => 0,
    }
}

fn npb_mode_key(index: i32) -> &'static str {
    match index {
        1 => "classic",
        2 => "small",
        3 => "large",
        _ => "new",
    }
}

/// Persist a mode index (0-3) to the shared key; returns the index.
pub fn set_npb_mode(index: i32) -> i32 {
    let index = index.clamp(0, 3);
    if let Some(path) = prefs_path() {
        let mut value: serde_json::Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "npb_mode".to_string(),
                serde_json::Value::String(npb_mode_key(index).to_string()),
            );
            if let Ok(text) = serde_json::to_string_pretty(&value) {
                let _ = std::fs::write(&path, text);
            }
        }
    }
    index
}

// ---------------------------------------------------------------------------
// Large-NPB dock prefs (the cover's eye toggle + the band's mode cycle)
// ---------------------------------------------------------------------------

/// Whether the Large dock shows its FFT band. Defaults ON (the Slint dock's
/// `large-visualizer-on` default) — the eye button is always visible while it
/// is off, so an accidental hide is one click from being undone.
pub fn large_visualizer_on() -> bool {
    let Some(path) = prefs_path() else {
        return true;
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| {
            v.get("large_visualizer_on")
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(true)
}

/// Persist the band visibility; returns the stored value.
pub fn set_large_visualizer_on(on: bool) -> bool {
    write_pref("large_visualizer_on", serde_json::Value::Bool(on));
    on
}

/// The band's render mode: 0 Bars / 1 Waveform / 2 Energy.
pub fn large_spectrum_mode() -> i32 {
    let Some(path) = prefs_path() else {
        return 0;
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("large_spectrum_mode").and_then(serde_json::Value::as_i64))
        .map(|m| (m as i32).clamp(0, 2))
        .unwrap_or(0)
}

/// Persist the band mode (clamped 0-2); returns the stored index.
pub fn set_large_spectrum_mode(mode: i32) -> i32 {
    let mode = mode.clamp(0, 2);
    write_pref(
        "large_spectrum_mode",
        serde_json::Value::Number(mode.into()),
    );
    mode
}

/// Additive single-key patch of ui_prefs.json (the npb_mode pattern: read,
/// insert, write back — never clobber keys other surfaces own).
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

/// The "Show track playing context" pref (Playback settings; feeds the
/// SongCard layers icon — SettingsState.show-context-icon).
pub fn show_context_icon() -> bool {
    with_playback(|s| s.get_preferences().map(|p| p.show_context_icon)).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Appearance + Integrations prefs (phase 19) — generic ui_prefs.json
// accessors (SAME additive patch discipline) + the per-user stores the
// Appearance/Integrations panels read (tray, My-QBZ branding).
// ---------------------------------------------------------------------------

pub fn pref_bool(key: &str, default: bool) -> bool {
    let Some(path) = prefs_path() else {
        return default;
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get(key).and_then(|q| q.as_bool()))
        .unwrap_or(default)
}

pub fn pref_str(key: &str, default: &str) -> String {
    let Some(path) = prefs_path() else {
        return default.to_string();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get(key).and_then(|q| q.as_str().map(str::to_string)))
        .unwrap_or_else(|| default.to_string())
}

pub fn save_pref(key: &str, value: serde_json::Value) {
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

// Tray store (qbz_app::settings::tray — per-user tray_settings.db, the SAME
// file the Slint tray_settings.rs glue writes).
static TRAY: OnceLock<qbz_app::settings::tray::TraySettingsState> = OnceLock::new();

fn tray() -> &'static qbz_app::settings::tray::TraySettingsState {
    TRAY.get_or_init(|| {
        let state = qbz_app::settings::tray::TraySettingsState::new_empty();
        if let Some(dir) = crate::sidebar_qt::user_dir() {
            if let Err(e) = state.init_at(&dir) {
                log::warn!("[qbz-qt] tray settings store unavailable: {e}");
            }
        }
        state
    })
}

// My-QBZ branding (myqbz_prefs.rs — per-user myqbz_branding.json; the label
// row in LIBRARY & VISUALS).
fn myqbz_label() -> String {
    crate::sidebar_qt::user_dir()
        .map(|d| d.join("myqbz_branding.json"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("label").and_then(|q| q.as_str().map(str::to_string)))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "My QBZ".to_string())
}

fn save_myqbz_label(label: &str) {
    let Some(path) = crate::sidebar_qt::user_dir().map(|d| d.join("myqbz_branding.json")) else {
        return;
    };
    let mut doc: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = doc.as_object_mut() {
        obj.insert(
            "label".to_string(),
            serde_json::Value::String(label.trim().to_string()),
        );
        if let Ok(text) = serde_json::to_string_pretty(&doc) {
            let _ = std::fs::write(&path, text);
        }
    }
}

pub const STREAMING_QUALITY_KEYS: &[&str] = &["mp3", "cd", "hires", "hires_plus"];
pub const STREAMING_QUALITY_LABELS: &[&str] = &["MP3", "CD Quality", "Hi-Res", "Hi-Res+"];

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

const DSD_MODE_LABELS: &[&str] = &[
    "Convert to PCM (works everywhere)",
    "DoP — DSD over PCM (bit-perfect)",
    "Native DSD (kernel support required)",
];
const DSD_MODE_VALUES: &[&str] = &["convert", "dop", "native"];
const ALSA_PLUGIN_LABELS: &[&str] = &["hw (Direct Hardware)", "plughw (Auto-convert)", "pcm (Most compatible)"];
const ALSA_PLUGIN_VALUES: &[AlsaPlugin] = &[AlsaPlugin::Hw, AlsaPlugin::PlugHw, AlsaPlugin::Pcm];
const RETRY_BEHAVIOR_LABELS: &[&str] = &["Ask me", "Always try lowest quality", "Always skip track"];
const RETRY_BEHAVIOR_VALUES: &[&str] = &["ask", "always_fallback", "always_skip"];
const QCONNECT_STARTUP_LABELS: &[&str] = &["Remember state", "On by default", "Off by default"];
const QCONNECT_STARTUP_VALUES: &[&str] = &["remember_last", "on", "off"];
// Appearance option tables (AppearanceSettings.slint / ui_prefs.rs).
const APP_BACKGROUND_LABELS: &[&str] = &["Off", "Ambient", "Blurred art"];
const APP_BACKGROUND_VALUES: &[&str] = &["off", "ambient", "blurred"];
const LANGUAGE_LABELS: &[&str] = &[
    "Auto", "English", "Español", "Français", "Deutsch", "Português", "Русский", "日本語",
    "Nederlands",
];
const LANGUAGE_VALUES: &[&str] = &["auto", "en", "es", "fr", "de", "pt", "ru", "ja", "nl"];
const UI_SCALE_LABELS: &[&str] = &["Extra small", "Small", "Default", "Large", "Extra large"];
const UI_SCALE_VALUES: &[&str] = &["xs", "small", "default", "large", "xl"];
const IMMERSIVE_SEARCH_LABELS: &[&str] =
    &["Disabled", "Replace current queue", "Play next", "Add to queue"];
const IMMERSIVE_SEARCH_VALUES: &[&str] = &["disabled", "replace", "next", "queue"];
const IMMERSIVE_VIEW_LABELS: &[&str] =
    &["Remember last", "Album Reactive", "Static", "Coverflow", "Spectrum", "Lyrics", "Queue"];
const IMMERSIVE_VIEW_VALUES: &[&str] =
    &["remember", "reactive", "static", "coverflow", "spectrum", "lyrics", "queue"];
const MINI_VIEW_LABELS: &[&str] =
    &["Remember last used", "Micro", "Compact", "Artwork", "Queue", "Lyrics"];
const MINI_VIEW_VALUES: &[&str] = &["remember", "micro", "compact", "artwork", "queue", "lyrics"];
const STARTUP_PAGE_LABELS: &[&str] = &["Home", "Where you left off"];
const STARTUP_PAGE_VALUES: &[&str] = &["home", "remember"];
const WC_POSITION_LABELS: &[&str] = &["Left", "Right"];
const WC_POSITION_VALUES: &[&str] = &["left", "right"];
const RENDERER_LABELS: &[&str] =
    &["Auto (recommended)", "GPU", "GPU (compatibility)", "Software"];
const RENDERER_VALUES: &[&str] = &["auto", "wgpu", "gl", "software"];
const TRAY_ICON_LABELS: &[&str] = &["Auto", "Mono light", "Mono dark", "Color"];
const TRAY_ICON_VALUES: &[&str] = &["auto", "mono-light", "mono-dark", "color"];

fn index_of(values: &[&str], key: &str, default: usize) -> i32 {
    values.iter().position(|v| *v == key).unwrap_or(default) as i32
}

/// Preferred-GPU dropdown (AppearanceSettings.slint): "Auto (recommended)"
/// + detected adapter names built in Rust. POC-NOTE: no adapter enumeration
/// in the POC — the list is Auto + the currently persisted adapter (so the
/// owner's real value shows selected); picking it again is a no-op.
fn gpu_power_choice() -> (Vec<String>, i32) {
    let persisted = pref_str("gpu_power", "auto");
    let mut opts = vec![qbz_i18n::t("Auto (recommended)")];
    if persisted != "auto" && !persisted.is_empty() {
        opts.push(persisted);
        (opts, 1)
    } else {
        (opts, 0)
    }
}

#[derive(Clone, Default, Serialize)]
pub struct DeviceOption {
    pub label: String,
    pub bp: bool,
    pub group: String,
}

#[derive(Default, Serialize)]
pub struct SettingsDoc {
    // Audio
    #[serde(rename = "streamingQualities")]
    pub streaming_qualities: Vec<String>,
    #[serde(rename = "streamingQualityIndex")]
    pub streaming_quality_index: i32,
    pub backends: Vec<String>,
    #[serde(rename = "backendIndex")]
    pub backend_index: i32,
    #[serde(rename = "backendIsAlsa")]
    pub backend_is_alsa: bool,
    #[serde(rename = "backendIsPipewire")]
    pub backend_is_pipewire: bool,
    #[serde(rename = "backendIsJack")]
    pub backend_is_jack: bool,
    pub devices: Vec<DeviceOption>,
    #[serde(rename = "deviceIndex")]
    pub device_index: i32,
    #[serde(rename = "alsaPlugins")]
    pub alsa_plugins: Vec<String>,
    #[serde(rename = "alsaPluginIndex")]
    pub alsa_plugin_index: i32,
    #[serde(rename = "alsaPluginIsHw")]
    pub alsa_plugin_is_hw: bool,
    #[serde(rename = "alsaHardwareVolume")]
    pub alsa_hardware_volume: bool,
    #[serde(rename = "dsdModes")]
    pub dsd_modes: Vec<String>,
    #[serde(rename = "dsdModeIndex")]
    pub dsd_mode_index: i32,
    #[serde(rename = "limitQualityToDevice")]
    pub limit_quality_to_device: bool,
    #[serde(rename = "exclusiveMode")]
    pub exclusive_mode: bool,
    #[serde(rename = "reserveDac")]
    pub reserve_dac: bool,
    #[serde(rename = "dacPassthrough")]
    pub dac_passthrough: bool,
    #[serde(rename = "pwForceBitperfect")]
    pub pw_force_bitperfect: bool,
    #[serde(rename = "allowQualityFallback")]
    pub allow_quality_fallback: bool,
    #[serde(rename = "syncAudioOnStartup")]
    pub sync_audio_on_startup: bool,
    #[serde(rename = "skipSinkSwitch")]
    pub skip_sink_switch: bool,
    // Playback
    #[serde(rename = "continuePlayback")]
    pub continue_playback: bool,
    #[serde(rename = "showContextIcon")]
    pub show_context_icon: bool,
    pub gapless: bool,
    /// Settable via settingsBool("normalization", …) but never published
    /// back, so both now-playing bars had to shadow the toggle locally.
    pub normalization: bool,
    #[serde(rename = "persistSession")]
    pub persist_session: bool,
    #[serde(rename = "resumePosition")]
    pub resume_position: bool,
    #[serde(rename = "streamUncached")]
    pub stream_uncached: bool,
    #[serde(rename = "bufferSeconds")]
    pub buffer_seconds: i32,
    #[serde(rename = "streamingOnly")]
    pub streaming_only: bool,
    #[serde(rename = "retryBehaviors")]
    pub retry_behaviors: Vec<String>,
    #[serde(rename = "retryBehaviorIndex")]
    pub retry_behavior_index: i32,
    #[serde(rename = "qconnectStartupModes")]
    pub qconnect_startup_modes: Vec<String>,
    #[serde(rename = "qconnectStartupIndex")]
    pub qconnect_startup_index: i32,
    #[serde(rename = "qconnectDeviceName")]
    pub qconnect_device_name: String,
    #[serde(rename = "qconnectDeviceNameDefault")]
    pub qconnect_device_name_default: String,
    // Appearance (phase 19; the ui_prefs defaults mirror ui_prefs.rs)
    #[serde(rename = "albumHeaderGradient")]
    pub album_header_gradient: bool,
    #[serde(rename = "appBackgroundModes")]
    pub app_background_modes: Vec<String>,
    #[serde(rename = "appBackgroundIndex")]
    pub app_background_index: i32,
    #[serde(rename = "autoThemeSourceIndex")]
    pub auto_theme_source_index: i32,
    #[serde(rename = "intelligentSearch")]
    pub intelligent_search: bool,
    pub languages: Vec<String>,
    #[serde(rename = "languageIndex")]
    pub language_index: i32,
    #[serde(rename = "uiScales")]
    pub ui_scales: Vec<String>,
    #[serde(rename = "uiScaleIndex")]
    pub ui_scale_index: i32,
    #[serde(rename = "immersiveSearchActions")]
    pub immersive_search_actions: Vec<String>,
    #[serde(rename = "immersiveSearchActionIndex")]
    pub immersive_search_action_index: i32,
    #[serde(rename = "immersiveDefaultViews")]
    pub immersive_default_views: Vec<String>,
    #[serde(rename = "immersiveDefaultViewIndex")]
    pub immersive_default_view_index: i32,
    #[serde(rename = "navInSidebar")]
    pub nav_in_sidebar: bool,
    #[serde(rename = "navHeaderCompact")]
    pub nav_header_compact: bool,
    #[serde(rename = "myQbzLabel")]
    pub my_qbz_label: String,
    #[serde(rename = "sidebarPlaylistCollage")]
    pub sidebar_playlist_collage: bool,
    #[serde(rename = "localLibraryTrackArtwork")]
    pub local_library_track_artwork: bool,
    #[serde(rename = "playIndicatorAnimation")]
    pub play_indicator_animation: bool,
    #[serde(rename = "invertSwipeNavigation")]
    pub invert_swipe_navigation: bool,
    #[serde(rename = "inAppToasts")]
    pub in_app_toasts: bool,
    #[serde(rename = "systemNotifications")]
    pub system_notifications: bool,
    #[serde(rename = "windowTitleShow")]
    pub window_title_show: bool,
    #[serde(rename = "useSystemTitleBar")]
    pub use_system_title_bar: bool,
    #[serde(rename = "hideTitleBar")]
    pub hide_title_bar: bool,
    #[serde(rename = "wcPositions")]
    pub wc_positions: Vec<String>,
    #[serde(rename = "wcPositionIndex")]
    pub wc_position_index: i32,
    #[serde(rename = "showWindowControls")]
    pub show_window_controls: bool,
    #[serde(rename = "showVolumeSteppers")]
    pub show_volume_steppers: bool,
    #[serde(rename = "miniDefaultViews")]
    pub mini_default_views: Vec<String>,
    #[serde(rename = "miniDefaultViewIndex")]
    pub mini_default_view_index: i32,
    #[serde(rename = "startupPages")]
    pub startup_pages: Vec<String>,
    #[serde(rename = "startupPageIndex")]
    pub startup_page_index: i32,
    #[serde(rename = "showPurchases")]
    pub show_purchases: bool,
    #[serde(rename = "navTbPurchases")]
    pub nav_tb_purchases: bool,
    pub renderers: Vec<String>,
    #[serde(rename = "rendererIndex")]
    pub renderer_index: i32,
    #[serde(rename = "gpuPowers")]
    pub gpu_powers: Vec<String>,
    #[serde(rename = "gpuPowerIndex")]
    pub gpu_power_index: i32,
    // Appearance > SYSTEM TRAY (tray_settings.db)
    #[serde(rename = "trayEnable")]
    pub tray_enable: bool,
    #[serde(rename = "trayCloseToTray")]
    pub tray_close_to_tray: bool,
    #[serde(rename = "trayIconThemes")]
    pub tray_icon_themes: Vec<String>,
    #[serde(rename = "trayIconThemeIndex")]
    pub tray_icon_theme_index: i32,
    // Integrations (phase 19)
    #[serde(rename = "showRecommendations")]
    pub show_recommendations: bool,
    #[serde(rename = "musicbrainzEnabled")]
    pub musicbrainz_enabled: bool,
    #[serde(rename = "scrobbleEnabled")]
    pub scrobble_enabled: bool,
    #[serde(rename = "scrobbleUiCollapsed")]
    pub scrobble_ui_collapsed: bool,
    #[serde(rename = "lastfmEnabled")]
    pub lastfm_enabled: bool,
    #[serde(rename = "lastfmAuthed")]
    pub lastfm_authed: bool,
    #[serde(rename = "lastfmUsername")]
    pub lastfm_username: String,
    #[serde(rename = "lastfmAuthUrl")]
    pub lastfm_auth_url: String,
    #[serde(rename = "lastfmBusy")]
    pub lastfm_busy: bool,
    #[serde(rename = "listenbrainzEnabled")]
    pub listenbrainz_enabled: bool,
    #[serde(rename = "listenbrainzAuthed")]
    pub listenbrainz_authed: bool,
    #[serde(rename = "listenbrainzUsername")]
    pub listenbrainz_username: String,
    #[serde(rename = "listenbrainzBusy")]
    pub listenbrainz_busy: bool,
    #[serde(rename = "integrationsStatusText")]
    pub integrations_status_text: String,
    #[serde(rename = "integrationsStatusKind")]
    pub integrations_status_kind: i32,
    #[serde(rename = "discordEnabled")]
    pub discord_enabled: bool,
}

/// Index -> value maps the select handlers resolve against.
static MAPS: Mutex<(Vec<AudioBackendType>, Vec<String>)> = Mutex::new((Vec::new(), Vec::new()));

/// settings.rs `alsa_section` — Tauri dropdown sectioning for ALSA rows.
fn alsa_section(id: &str, is_default: bool, label: &str) -> usize {
    let id_l = id.to_ascii_lowercase();
    if id.is_empty() || id_l == "default" || is_default {
        0 // Defaults
    } else if id_l.starts_with("hw:")
        || id_l.starts_with("iec958:")
        || id_l.starts_with("front:card=")
        || label.to_ascii_lowercase().contains("bit-perfect")
    {
        1 // Bit-perfect (Hardware / Digital)
    } else if id_l.starts_with("plughw:") {
        2 // Plugin Hardware
    } else {
        3 // Other Outputs
    }
}

fn device_is_bit_perfect(backend: AudioBackendType, device: &qbz_audio::AudioDevice) -> bool {
    match backend {
        AudioBackendType::Alsa => {
            let label = device.description.as_deref().unwrap_or(&device.name);
            alsa_section(&device.id, device.is_default, label) == 1
        }
        AudioBackendType::PipeWire => device.is_hardware,
        _ => false,
    }
}

/// Enumerate output devices for a backend (settings.rs `enumerate_devices`
/// with the ALSA regrouping). Blocking — call off the async executor's
/// fast path (runs inside spawn_blocking by the caller).
fn enumerate_devices(backend: AudioBackendType) -> (Vec<DeviceOption>, Vec<String>) {
    let mut rows = vec![DeviceOption {
        label: qbz_i18n::t("System default"),
        bp: false,
        group: String::new(),
    }];
    let mut ids = vec![String::new()];
    match BackendManager::create_backend(backend).and_then(|b| b.enumerate_devices()) {
        Ok(devices) => {
            for d in devices {
                let label = match d.description.as_deref() {
                    Some(desc) if !desc.is_empty() => desc.to_string(),
                    _ => d.name.clone(),
                };
                ids.push(d.id.clone());
                rows.push(DeviceOption {
                    bp: device_is_bit_perfect(backend, &d),
                    label,
                    group: String::new(),
                });
            }
        }
        Err(e) => log::warn!("[qbz-qt] device enumeration failed: {e}"),
    }

    if backend == AudioBackendType::Alsa {
        // Stable sort by section; the section header lands on each section's
        // first row (settings.rs `group_alsa_devices`). rows[i] aligns with
        // ids[i] (both lead with the synthetic "System default"/"" entry).
        let section_labels = [
            qbz_i18n::t("Defaults"),
            qbz_i18n::t("Bit-perfect (Hardware / Digital)"),
            qbz_i18n::t("Plugin Hardware"),
            qbz_i18n::t("Other Outputs"),
        ];
        let mut indexed: Vec<(usize, DeviceOption, String)> = rows
            .into_iter()
            .zip(ids.iter().cloned())
            .enumerate()
            .map(|(i, (row, id))| (alsa_section(&id, i == 0, &row.label), row, id))
            .collect();
        indexed.sort_by_key(|(section, _, _)| *section);
        // Rebuild ids in the SAME order (they're the index map).
        let mut out_rows = Vec::with_capacity(indexed.len());
        let mut out_ids = Vec::with_capacity(indexed.len());
        let mut prev: Option<usize> = None;
        for (section, mut row, id) in indexed {
            if prev != Some(section) {
                prev = Some(section);
                row.group = section_labels[section].clone();
            }
            out_rows.push(row);
            out_ids.push(id);
        }
        (out_rows, out_ids)
    } else {
        (rows, ids)
    }
}

fn backend_label(t: AudioBackendType) -> String {
    match t {
        AudioBackendType::PipeWire => "PipeWire".to_string(),
        AudioBackendType::Alsa => "ALSA".to_string(),
        AudioBackendType::Pulse => "PulseAudio".to_string(),
        AudioBackendType::SystemDefault => qbz_i18n::t("System default"),
        AudioBackendType::Jack => "JACK".to_string(),
    }
}

/// Build + publish the full snapshot (settings.rs `build_snapshot`).
pub async fn publish_snapshot() {
    let audio_settings = with_audio(|s| s.get_settings()).unwrap_or_default();
    let prefs = with_playback(|s| s.get_preferences()).unwrap_or_default();
    let streaming_key = streaming_quality();

    // The now-playing stamp's two output LEDs are a pure function of the
    // audio settings (settings.rs `output_labels`, mirrored onto
    // NowPlayingState by `apply_snapshot`). Every settings change already
    // funnels through here, so they refresh with the settings and never poll.
    crate::output_labels::publish(&audio_settings);

    let doc = tokio::task::spawn_blocking(move || {
        let backend_types = BackendManager::available_backends();
        let current_backend = audio_settings.backend_type.unwrap_or_default();
        let backend_index = backend_types
            .iter()
            .position(|t| *t == current_backend)
            .unwrap_or(0);
        let active_backend = backend_types
            .get(backend_index)
            .copied()
            .unwrap_or_default();

        let (devices, ids) = enumerate_devices(active_backend);
        let device_index = match &audio_settings.output_device {
            None => 0,
            Some(id) => ids.iter().position(|d| d == id).unwrap_or(0),
        };

        let alsa_plugin = audio_settings.alsa_plugin.unwrap_or(AlsaPlugin::Hw);
        let alsa_plugin_index = ALSA_PLUGIN_VALUES
            .iter()
            .position(|p| *p == alsa_plugin)
            .unwrap_or(0);
        let retry_behavior_index = RETRY_BEHAVIOR_VALUES
            .iter()
            .position(|v| *v == audio_settings.quality_fallback_behavior)
            .unwrap_or(0);
        let qconnect_startup = qconnect_load_startup_mode();
        let scrobble_snap = crate::integrations_qt::scrobble_settings();
        let integ_ui = crate::integrations_qt::ui_snapshot();
        let qconnect_startup_index = QCONNECT_STARTUP_VALUES
            .iter()
            .position(|v| *v == qconnect_startup)
            .unwrap_or(QCONNECT_STARTUP_VALUES.len() - 1);
        let streaming_index = STREAMING_QUALITY_KEYS
            .iter()
            .position(|k| *k == streaming_key)
            .unwrap_or(STREAMING_QUALITY_KEYS.len() - 1);

        let mut maps = MAPS.lock().unwrap();
        maps.0 = backend_types.clone();
        maps.1 = ids;

        SettingsDoc {
            streaming_qualities: STREAMING_QUALITY_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            streaming_quality_index: streaming_index as i32,
            backends: std::iter::once(qbz_i18n::t("Auto"))
                .chain(backend_types.iter().map(|t| backend_label(*t)))
                .collect(),
            backend_index: backend_index as i32 + 1,
            backend_is_alsa: active_backend == AudioBackendType::Alsa,
            backend_is_pipewire: active_backend == AudioBackendType::PipeWire,
            backend_is_jack: active_backend == AudioBackendType::Jack,
            devices,
            device_index: device_index as i32,
            alsa_plugins: ALSA_PLUGIN_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            alsa_plugin_index: alsa_plugin_index as i32,
            alsa_plugin_is_hw: alsa_plugin == AlsaPlugin::Hw,
            alsa_hardware_volume: audio_settings.alsa_hardware_volume,
            dsd_modes: DSD_MODE_LABELS.iter().map(|l| qbz_i18n::t(l)).collect(),
            dsd_mode_index: DSD_MODE_VALUES
                .iter()
                .position(|v| *v == audio_settings.dsd_mode)
                .unwrap_or(0) as i32,
            limit_quality_to_device: audio_settings.limit_quality_to_device,
            exclusive_mode: audio_settings.exclusive_mode,
            reserve_dac: audio_settings.reserve_dac_while_running,
            dac_passthrough: audio_settings.dac_passthrough,
            pw_force_bitperfect: audio_settings.pw_force_bitperfect,
            allow_quality_fallback: audio_settings.allow_quality_fallback,
            sync_audio_on_startup: audio_settings.sync_audio_on_startup,
            skip_sink_switch: audio_settings.skip_sink_switch,
            continue_playback: prefs.autoplay_mode == AutoplayMode::ContinueWithinSource,
            show_context_icon: prefs.show_context_icon,
            gapless: audio_settings.gapless_enabled,
            normalization: audio_settings.normalization_enabled,
            persist_session: prefs.persist_session,
            resume_position: prefs.resume_playback_position,
            stream_uncached: audio_settings.stream_first_track,
            buffer_seconds: audio_settings.stream_buffer_seconds as i32,
            streaming_only: audio_settings.streaming_only,
            retry_behaviors: RETRY_BEHAVIOR_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            retry_behavior_index: retry_behavior_index as i32,
            qconnect_startup_modes: QCONNECT_STARTUP_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            qconnect_startup_index: qconnect_startup_index as i32,
            qconnect_device_name: qconnect_load_device_name().unwrap_or_default(),
            qconnect_device_name_default: qconnect_default_name(),
            album_header_gradient: pref_bool("album_header_gradient", true),
            app_background_modes: APP_BACKGROUND_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            app_background_index: APP_BACKGROUND_VALUES
                .iter()
                .position(|v| *v == pref_str("app_background", "off"))
                .unwrap_or(0) as i32,
            auto_theme_source_index: index_of(
                &["system", "wallpaper", "image"],
                &pref_str("auto_theme_source", "system"),
                0,
            ),
            intelligent_search: pref_bool("intelligent_search", true),
            languages: LANGUAGE_LABELS.iter().map(|l| qbz_i18n::t(l)).collect(),
            language_index: index_of(LANGUAGE_VALUES, &pref_str("language", "auto"), 0),
            ui_scales: UI_SCALE_LABELS.iter().map(|l| qbz_i18n::t(l)).collect(),
            ui_scale_index: index_of(UI_SCALE_VALUES, &pref_str("ui_scale", "default"), 2),
            immersive_search_actions: IMMERSIVE_SEARCH_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            immersive_search_action_index: index_of(
                IMMERSIVE_SEARCH_VALUES,
                &pref_str("immersive_search_action", "replace"),
                1,
            ),
            immersive_default_views: IMMERSIVE_VIEW_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            immersive_default_view_index: index_of(
                IMMERSIVE_VIEW_VALUES,
                &pref_str("immersive_default_view", "remember"),
                0,
            ),
            nav_in_sidebar: pref_bool("nav_in_sidebar", true),
            nav_header_compact: pref_bool("nav_header_compact", true),
            my_qbz_label: myqbz_label(),
            sidebar_playlist_collage: pref_bool("sidebar_playlist_collage", true),
            local_library_track_artwork: pref_bool("local_library_track_artwork", false),
            play_indicator_animation: pref_bool("play_indicator_animation", false),
            invert_swipe_navigation: pref_bool("invert_swipe_navigation", false),
            in_app_toasts: pref_bool("in_app_toasts", true),
            system_notifications: pref_bool("system_notifications", true),
            window_title_show: pref_bool("window_title_show", false),
            use_system_title_bar: use_system_title_bar(),
            hide_title_bar: pref_bool("hide_title_bar", false),
            wc_positions: WC_POSITION_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            wc_position_index: index_of(WC_POSITION_VALUES, &pref_str("wc_position", "right"), 1),
            show_window_controls: pref_bool("show_window_controls", true),
            show_volume_steppers: pref_bool("show_volume_steppers", false),
            mini_default_views: MINI_VIEW_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            mini_default_view_index: index_of(
                MINI_VIEW_VALUES,
                &pref_str("mini_default_view", "remember"),
                0,
            ),
            startup_pages: STARTUP_PAGE_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            startup_page_index: index_of(STARTUP_PAGE_VALUES, &pref_str("startup_page", "home"), 0),
            show_purchases: pref_bool("show_purchases", false),
            nav_tb_purchases: pref_bool("nav_tb_purchases", false),
            renderers: RENDERER_LABELS.iter().map(|l| qbz_i18n::t(l)).collect(),
            renderer_index: index_of(RENDERER_VALUES, &pref_str("renderer", "auto"), 0),
            gpu_powers: gpu_power_choice().0,
            gpu_power_index: gpu_power_choice().1,
            tray_enable: tray().get_settings().map(|t| t.enable_tray).unwrap_or(true),
            tray_close_to_tray: tray()
                .get_settings()
                .map(|t| t.close_to_tray)
                .unwrap_or(false),
            tray_icon_themes: TRAY_ICON_LABELS
                .iter()
                .map(|l| qbz_i18n::t(l))
                .collect(),
            tray_icon_theme_index: tray()
                .get_settings()
                .map(|t| index_of(TRAY_ICON_VALUES, &t.tray_icon_theme, 0))
                .unwrap_or(0),
            show_recommendations: crate::integrations_qt::show_recommendations(),
            musicbrainz_enabled: pref_bool("musicbrainz_enabled", true),
            scrobble_enabled: scrobble_snap.enabled,
            scrobble_ui_collapsed: scrobble_snap.ui_collapsed,
            lastfm_enabled: scrobble_snap.lastfm_enabled,
            lastfm_authed: scrobble_snap.lastfm_is_authed(),
            lastfm_username: scrobble_snap.lastfm_username.clone(),
            lastfm_auth_url: integ_ui.lastfm_auth_url.clone(),
            lastfm_busy: integ_ui.lastfm_busy,
            listenbrainz_enabled: scrobble_snap.listenbrainz_enabled,
            listenbrainz_authed: scrobble_snap.listenbrainz_is_authed(),
            listenbrainz_username: scrobble_snap.listenbrainz_username.clone(),
            listenbrainz_busy: integ_ui.listenbrainz_busy,
            integrations_status_text: integ_ui.status_text.clone(),
            integrations_status_kind: integ_ui.status_kind,
            discord_enabled: crate::integrations_qt::discord_enabled(),
        }
    })
    .await
    .unwrap_or_default();

    let json = serde_json::to_string(&doc).unwrap_or_else(|_| "{}".into());
    crate::ui(move |mut b| {
        b.as_mut().set_settings_json(QString::from(json.as_str()));
    });
}

// ---------------------------------------------------------------------------
// Apply (settings.rs `apply_audio`) — the ONLY player touchpoints.
// ---------------------------------------------------------------------------

/// What a change requires of the live player (settings.rs `Apply`).
enum Apply {
    None,
    Reload,
    Reinit,
}

fn apply_audio(runtime: &Arc<AppRuntime<LoggingAdapter>>, apply: Apply) {
    let reinit = match apply {
        Apply::None => return,
        Apply::Reload => false,
        Apply::Reinit => true,
    };
    let fresh = match with_audio(|s| s.get_settings()) {
        Ok(s) => s,
        Err(e) => {
            log::error!("[qbz-qt] re-read audio settings failed: {e}");
            return;
        }
    };
    let player = runtime.core().player();
    if let Err(e) = player.reload_settings(fresh.clone()) {
        log::error!("[qbz-qt] player.reload_settings failed: {e}");
    }
    if reinit {
        if let Err(e) = player.reinit_device(fresh.output_device.clone()) {
            log::error!("[qbz-qt] player.reinit_device failed: {e}");
        }
    }
    log::info!("[qbz-qt] audio settings applied to player (reinit={reinit})");
}

// ---------------------------------------------------------------------------
// Handlers (settings.rs handle_bool / handle_select / handle_slider /
// handle_string, including the cascades)
// ---------------------------------------------------------------------------

pub async fn settings_bool(runtime: &Arc<AppRuntime<LoggingAdapter>>, key: &str, value: bool) {
    // Cross-setting cascades (settings.rs) — force dependents off first.
    let mut cascaded = false;
    match key {
        "dac-passthrough" if value => {
            if with_audio(|s| s.set_skip_sink_switch(false)).is_ok() {
                cascaded = true;
            }
        }
        "dac-passthrough" => {
            if with_audio(|s| s.set_pw_force_bitperfect(false)).is_ok() {
                cascaded = true;
            }
        }
        "streaming-only" if value => {
            if with_audio(|s| s.set_gapless_enabled(false)).is_ok() {
                cascaded = true;
            }
        }
        _ => {}
    }

    let outcome: Result<Apply, String> = match key {
        "limit-quality-to-device" => {
            with_audio(|s| s.set_limit_quality_to_device(value)).map(|_| Apply::Reload)
        }
        "alsa-hardware-volume" => {
            with_audio(|s| s.set_alsa_hardware_volume(value)).map(|_| Apply::Reinit)
        }
        "exclusive-mode" => {
            with_audio(|s| s.set_exclusive_mode(value)).map(|_| Apply::Reinit)
        }
        "reserve-dac" => {
            with_audio(|s| s.set_reserve_dac_while_running(value)).map(|_| Apply::Reload)
        }
        "dac-passthrough" => {
            with_audio(|s| s.set_dac_passthrough(value)).map(|_| Apply::Reinit)
        }
        "pw-force-bitperfect" => {
            with_audio(|s| s.set_pw_force_bitperfect(value)).map(|_| Apply::Reload)
        }
        "allow-quality-fallback" => {
            with_audio(|s| s.set_allow_quality_fallback(value)).map(|_| Apply::Reload)
        }
        "sync-audio-on-startup" => {
            with_audio(|s| s.set_sync_audio_on_startup(value)).map(|_| Apply::Reload)
        }
        "skip-sink-switch" => {
            with_audio(|s| s.set_skip_sink_switch(value)).map(|_| Apply::Reinit)
        }
        "gapless" => with_audio(|s| s.set_gapless_enabled(value)).map(|_| Apply::Reload),
        "normalization" => {
            with_audio(|s| s.set_normalization_enabled(value)).map(|_| Apply::Reload)
        }
        "stream-uncached" => {
            with_audio(|s| s.set_stream_first_track(value)).map(|_| Apply::Reload)
        }
        "streaming-only" => {
            with_audio(|s| s.set_streaming_only(value)).map(|_| Apply::Reload)
        }
        "continue-playback" => {
            let mode = if value {
                AutoplayMode::ContinueWithinSource
            } else {
                AutoplayMode::PlayTrackOnly
            };
            with_playback(|s| s.set_autoplay_mode(mode)).map(|_| Apply::None)
        }
        "show-context-icon" => {
            with_playback(|s| s.set_show_context_icon(value)).map(|_| Apply::None)
        }
        "persist-session" => {
            with_playback(|s| s.set_persist_session(value)).map(|_| Apply::None)
        }
        "resume-position" => {
            with_playback(|s| s.set_resume_playback_position(value)).map(|_| Apply::None)
        }
        // --- Appearance (phase 19): plain ui_prefs bools (+ live bridge
        // side-effects where the POC already has the consumer).
        "album-header-gradient" => {
            save_pref("album_header_gradient", serde_json::json!(value));
            Ok(Apply::None)
        }
        "intelligent-search" => {
            save_pref("intelligent_search", serde_json::json!(value));
            crate::ui(move |mut b| b.as_mut().set_intelligent_search(value));
            Ok(Apply::None)
        }
        "nav-in-sidebar" => {
            save_pref("nav_in_sidebar", serde_json::json!(value));
            Ok(Apply::None)
        }
        "nav-header-compact" => {
            save_pref("nav_header_compact", serde_json::json!(value));
            Ok(Apply::None)
        }
        "sidebar-playlist-collage" => {
            save_pref("sidebar_playlist_collage", serde_json::json!(value));
            Ok(Apply::None)
        }
        "local-library-track-artwork" => {
            save_pref("local_library_track_artwork", serde_json::json!(value));
            // Republish so the local lists repaint live; the bridge property is
            // read at boot otherwise and the toggle would need a restart.
            crate::local_album_actions::publish_track_artwork();
            Ok(Apply::None)
        }
        "play-indicator-animation" => {
            save_pref("play_indicator_animation", serde_json::json!(value));
            Ok(Apply::None)
        }
        "invert-swipe-navigation" => {
            save_pref("invert_swipe_navigation", serde_json::json!(value));
            Ok(Apply::None)
        }
        "in-app-toasts" => {
            save_pref("in_app_toasts", serde_json::json!(value));
            Ok(Apply::None)
        }
        "system-notifications" => {
            save_pref("system_notifications", serde_json::json!(value));
            Ok(Apply::None)
        }
        "window-title-show" => {
            save_pref("window_title_show", serde_json::json!(value));
            Ok(Apply::None)
        }
        "use-system-title-bar" => {
            save_pref("use_system_title_bar", serde_json::json!(value));
            // Restart semantics (1:1 Slint): the APPLIED mode is untouched;
            // only the menu/row state flips live.
            log::info!("[qbz-qt] use_system_title_bar -> {value} (applies on next launch)");
            crate::shell_bridge::ui(move |mut b| b.as_mut().set_system_title_bar_pref(value));
            Ok(Apply::None)
        }
        "hide-title-bar" => {
            save_pref("hide_title_bar", serde_json::json!(value));
            Ok(Apply::None)
        }
        "show-window-controls" => {
            save_pref("show_window_controls", serde_json::json!(value));
            Ok(Apply::None)
        }
        "show-volume-steppers" => {
            save_pref("show_volume_steppers", serde_json::json!(value));
            Ok(Apply::None)
        }
        "show-purchases" => {
            save_pref("show_purchases", serde_json::json!(value));
            Ok(Apply::None)
        }
        "nav-tb-purchases" => {
            save_pref("nav_tb_purchases", serde_json::json!(value));
            Ok(Apply::None)
        }
        "tray-enable" => tray()
            .set_enable_tray(value)
            .map_err(|e| e.to_string())
            .map(|_| Apply::None),
        "tray-close-to-tray" => tray()
            .set_close_to_tray(value)
            .map_err(|e| e.to_string())
            .map(|_| Apply::None),
        // --- Integrations (phase 19) --------------------------------------
        "show-recommendations" => {
            crate::integrations_qt::set_show_recommendations(value).map(|_| Apply::None)
        }
        "musicbrainz" => {
            match crate::integrations_qt::set_musicbrainz_enabled(value) {
                Ok(()) => {
                    // Live apply (main.rs:9019-9029 seeds core from this pref).
                    runtime.core().musicbrainz_set_enabled(value).await;
                    Ok(Apply::None)
                }
                Err(e) => Err(e),
            }
        }
        "scrobble-enable" => {
            crate::integrations_qt::set_scrobble_enabled(value).map(|_| Apply::None)
        }
        "scrobble-collapse" => {
            crate::integrations_qt::set_scrobble_collapsed(value).map(|_| Apply::None)
        }
        "lastfm-enable" => {
            crate::integrations_qt::set_lastfm_enabled(value).map(|_| Apply::None)
        }
        "listenbrainz-enable" => {
            crate::integrations_qt::set_listenbrainz_enabled(value).map(|_| Apply::None)
        }
        "discord-rpc" => {
            crate::integrations_qt::set_discord_enabled(value).map(|_| Apply::None)
        }
        other => {
            log::warn!("[qbz-qt] unknown settings bool key: {other}");
            return;
        }
    };
    match outcome {
        Ok(apply) => {
            let apply = if cascaded { Apply::Reinit } else { apply };
            apply_audio(runtime, apply);
            publish_snapshot().await;
        }
        Err(e) => log::error!("[qbz-qt] settings persist failed ({key}): {e}"),
    }
}

pub async fn settings_select(runtime: &Arc<AppRuntime<LoggingAdapter>>, key: &str, index: usize) {
    match key {
        "streaming-quality" => {
            let Some(key) = STREAMING_QUALITY_KEYS.get(index) else {
                return;
            };
            save_streaming_quality(key);
            // Apply to the playback request tier + drop the tier-keyed cache
            // (settings.rs: bytes fetched at the old tier must not keep
            // serving).
            crate::playback_qt::set_streaming_quality(key);
            log::info!("[qbz-qt] streaming quality changed -> clearing audio cache");
            runtime.core().player().clear_audio_cache();
        }
        "backend" => {
            // Index 0 = "Auto" (resolve-and-set, #470): PipeWire when present,
            // else System. Indices >= 1 map to the concrete backends list.
            let backend = if index == 0 {
                let types = MAPS.lock().unwrap().0.clone();
                if types.iter().any(|t| *t == AudioBackendType::PipeWire) {
                    AudioBackendType::PipeWire
                } else {
                    AudioBackendType::SystemDefault
                }
            } else {
                let types = MAPS.lock().unwrap().0.clone();
                match types.get(index - 1) {
                    Some(t) => *t,
                    None => return,
                }
            };
            if let Err(e) = with_audio(|s| s.set_backend_type(Some(backend))) {
                log::error!("[qbz-qt] persist backend failed: {e}");
                return;
            }
            // Backend-switch cascade (settings.rs): routing-critical toggles
            // that don't translate across stacks reset.
            if backend != AudioBackendType::PipeWire {
                let _ = with_audio(|s| s.set_dac_passthrough(false));
                let _ = with_audio(|s| s.set_pw_force_bitperfect(false));
            }
            if backend != AudioBackendType::Alsa {
                let _ = with_audio(|s| s.set_exclusive_mode(false));
            }
            let _ = with_audio(|s| s.set_gapless_enabled(false));
            let _ = with_audio(|s| s.set_output_device(None));
            apply_audio(runtime, Apply::Reinit);
        }
        "device" => {
            let id = {
                let ids = MAPS.lock().unwrap().1.clone();
                ids.get(index).cloned()
            };
            let Some(id) = id else {
                return;
            };
            let device_opt = if id.is_empty() { None } else { Some(id.as_str()) };
            if let Err(e) = with_audio(|s| s.set_output_device(device_opt)) {
                log::error!("[qbz-qt] persist output device failed: {e}");
                return;
            }
            apply_audio(runtime, Apply::Reinit);
        }
        "dsd-mode" => {
            let Some(mode) = DSD_MODE_VALUES.get(index) else {
                return;
            };
            if let Err(e) = with_audio(|s| s.set_dsd_mode(mode)) {
                log::error!("[qbz-qt] persist dsd mode failed: {e}");
                return;
            }
            apply_audio(runtime, Apply::Reinit);
        }
        "alsa-plugin" => {
            let Some(plugin) = ALSA_PLUGIN_VALUES.get(index).copied() else {
                return;
            };
            if let Err(e) = with_audio(|s| s.set_alsa_plugin(Some(plugin))) {
                log::error!("[qbz-qt] persist alsa plugin failed: {e}");
                return;
            }
            apply_audio(runtime, Apply::Reinit);
        }
        "retry-behavior" => {
            let Some(behavior) = RETRY_BEHAVIOR_VALUES.get(index) else {
                return;
            };
            if let Err(e) = with_audio(|s| s.set_quality_fallback_behavior(behavior)) {
                log::error!("[qbz-qt] persist retry behavior failed: {e}");
                return;
            }
            apply_audio(runtime, Apply::Reload);
        }
        "qconnect-startup" => {
            let Some(mode) = QCONNECT_STARTUP_VALUES.get(index) else {
                return;
            };
            qconnect_persist_startup_mode(mode);
        }
        // --- Appearance (phase 19) ----------------------------------------
        "app-background" => {
            let Some(mode) = APP_BACKGROUND_VALUES.get(index) else {
                return;
            };
            save_pref("app_background", serde_json::json!(mode));
            // Live (pure QML layering; blurred renders AS ambient in the POC).
            let ambient = if *mode == "off" { 0 } else { 1 };
            crate::shell_bridge::ui(move |mut b| b.as_mut().set_ambient_mode(ambient));
        }
        "language" => {
            let Some(lang) = LANGUAGE_VALUES.get(index) else {
                return;
            };
            save_pref("language", serde_json::json!(lang));
            // Live switch (phase 20): trRev bump + doc republish.
            crate::apply_language(lang.to_string());
        }
        "ui-scale" => {
            let Some(scale) = UI_SCALE_VALUES.get(index) else {
                return;
            };
            save_pref("ui_scale", serde_json::json!(scale));
            log::info!("[qbz-qt] ui_scale -> {scale} (restart to apply)");
        }
        "immersive-search-action" => {
            let Some(v) = IMMERSIVE_SEARCH_VALUES.get(index) else {
                return;
            };
            save_pref("immersive_search_action", serde_json::json!(v));
        }
        "immersive-default-view" => {
            let Some(v) = IMMERSIVE_VIEW_VALUES.get(index) else {
                return;
            };
            save_pref("immersive_default_view", serde_json::json!(v));
        }
        "wc-position" => {
            let Some(v) = WC_POSITION_VALUES.get(index) else {
                return;
            };
            save_pref("wc_position", serde_json::json!(v));
        }
        "mini-default-view" => {
            let Some(v) = MINI_VIEW_VALUES.get(index) else {
                return;
            };
            save_pref("mini_default_view", serde_json::json!(v));
        }
        "startup-page" => {
            let Some(v) = STARTUP_PAGE_VALUES.get(index) else {
                return;
            };
            save_pref("startup_page", serde_json::json!(v));
        }
        "renderer" => {
            let Some(v) = RENDERER_VALUES.get(index) else {
                return;
            };
            save_pref("renderer", serde_json::json!(v));
            log::info!("[qbz-qt] renderer -> {v} (restart to apply)");
        }
        "gpu-power" => {
            // Only "Auto" is actionable in the POC (no adapter enumeration —
            // the persisted-name row is display-only).
            if index == 0 {
                save_pref("gpu_power", serde_json::json!("auto"));
                log::info!("[qbz-qt] gpu_power -> auto (restart to apply)");
            }
        }
        "tray-icon-theme" => {
            let Some(v) = TRAY_ICON_VALUES.get(index) else {
                return;
            };
            if let Err(e) = tray().set_tray_icon_theme(v) {
                log::error!("[qbz-qt] persist tray icon theme failed: {e}");
            }
        }
        "auto-theme-source" => {
            const SOURCES: &[&str] = &["system", "wallpaper", "image"];
            let Some(v) = SOURCES.get(index) else {
                return;
            };
            save_pref("auto_theme_source", serde_json::json!(v));
        }
        other => log::warn!("[qbz-qt] unknown settings select key: {other}"),
    }
    publish_snapshot().await;
}

pub async fn settings_slider(runtime: &Arc<AppRuntime<LoggingAdapter>>, key: &str, value: i32) {
    if key == "buffer-seconds" {
        let seconds = value.clamp(1, 10) as u8;
        match with_audio(|s| s.set_stream_buffer_seconds(seconds)) {
            Ok(()) => apply_audio(runtime, Apply::Reload),
            Err(e) => log::error!("[qbz-qt] persist buffer seconds failed: {e}"),
        }
    }
    publish_snapshot().await;
}

pub async fn settings_string(key: &str, value: String) {
    if key == "qconnect-device-name" {
        let trimmed = value.trim().to_string();
        let stored = (!trimmed.is_empty()).then_some(trimmed.as_str());
        qconnect_persist_device_name(stored);
    }
    if key == "myqbz-label" {
        save_myqbz_label(&value);
    }
    if key == "listenbrainz-token" {
        // Validates + persists + republishes itself (async flow).
        crate::integrations_qt::listenbrainz_set_token(&value).await;
        return;
    }
    publish_snapshot().await;
}

/// "Reset to defaults" — restores Audio + Playback defaults
/// (settings.rs handle_reset: store resets + apply + snapshot).
pub async fn settings_reset(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    if let Err(e) = with_audio(|s| s.reset_all().map(|_| ())) {
        log::error!("[qbz-qt] audio settings reset failed: {e}");
    }
    if let Err(e) = with_playback(|s| s.reset_all().map(|_| ())) {
        log::error!("[qbz-qt] playback preferences reset failed: {e}");
    }
    save_streaming_quality("hires_plus");
    crate::playback_qt::set_streaming_quality("hires_plus");
    apply_audio(runtime, Apply::Reinit);
    publish_snapshot().await;
}

/// The refresh/release button next to the output device (settings.rs:
/// frees a held ALSA-exclusive device and re-enumerates).
pub async fn refresh_devices(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    // Release whatever the player holds, then rebuild the snapshot.
    let player = runtime.core().player();
    if let Err(e) = player.release_device() {
        log::warn!("[qbz-qt] release device failed: {e}");
    }
    publish_snapshot().await;
}

// ---------------------------------------------------------------------------
// QConnect key/value DB (same file as the Slint app)
// ---------------------------------------------------------------------------

fn qconnect_conn() -> Option<rusqlite::Connection> {
    let db_path = qbz_app::qconnect_identity::qconnect_settings_db_path()?;
    let conn = rusqlite::Connection::open(&db_path).ok()?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .ok()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
    )
    .ok()?;
    Some(conn)
}

fn qconnect_load_startup_mode() -> String {
    qconnect_conn()
        .and_then(|conn| {
            conn.query_row(
                "SELECT value FROM settings WHERE key = 'startup_mode'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
        })
        .unwrap_or_else(|| "off".to_string())
}

fn qconnect_persist_startup_mode(mode: &str) {
    if let Some(conn) = qconnect_conn() {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('startup_mode', ?1)",
            rusqlite::params![mode],
        );
    }
}

fn qconnect_load_device_name() -> Option<String> {
    qconnect_conn().and_then(|conn| {
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'device_name'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .filter(|v: &String| !v.trim().is_empty())
    })
}

fn qconnect_persist_device_name(name: Option<&str>) {
    if let Some(conn) = qconnect_conn() {
        match name {
            Some(n) => {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES ('device_name', ?1)",
                    rusqlite::params![n],
                );
            }
            None => {
                let _ = conn.execute("DELETE FROM settings WHERE key = 'device_name'", []);
            }
        }
    }
}

fn qconnect_default_name() -> String {
    // qconnect_transport::resolve_qconnect_friendly_name(None): env var, else
    // "Qbz - {hostname}".
    std::env::var("QBZ_QCONNECT_DEVICE_NAME").unwrap_or_else(|_| {
        let host = std::env::var("HOSTNAME")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::fs::read_to_string("/etc/hostname")
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "device".to_string());
        format!("Qbz - {host}")
    })
}
