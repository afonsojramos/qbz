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
