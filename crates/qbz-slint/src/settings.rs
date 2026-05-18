//! Settings controller — Audio and Playback preferences.
//!
//! Owns the two persistence stores (`AudioSettingsStore` from `qbz-audio`,
//! `PlaybackPreferencesStore` from `qbz-app`) and bridges them to the
//! `SettingsState` Slint global. Audio changes are persisted and then
//! applied to the live `Player`: routing-critical changes (backend,
//! device, sample rate, exclusive mode, DAC passthrough) trigger a device
//! re-init; the rest only reload the settings struct.
//!
//! Neither store is exposed by `AppRuntime`, so this module opens them
//! directly at the shared global path — the same path `AppRuntime::new`
//! reads to seed the `Player`, so the two stay consistent.

use std::sync::{Arc, Mutex};

use qbz_app::settings::playback::{
    AutoplayMode, PlaybackPreferencesState, PlaybackPreferencesStore,
};
use qbz_app::shell::AppRuntime;
use qbz_audio::backend::{AudioBackendType, BackendManager};
use qbz_audio::settings::{AudioSettingsState, AudioSettingsStore};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::adapter::SlintAdapter;
use crate::{AppWindow, SettingsState};

/// Sample-rate dropdown options. Index 0 is "Auto" (`None`).
const SAMPLE_RATES: &[(&str, Option<u32>)] = &[
    ("Auto", None),
    ("44.1 kHz", Some(44_100)),
    ("48 kHz", Some(48_000)),
    ("88.2 kHz", Some(88_200)),
    ("96 kHz", Some(96_000)),
    ("176.4 kHz", Some(176_400)),
    ("192 kHz", Some(192_000)),
];

/// Autoplay-mode dropdown options.
const AUTOPLAY_MODES: &[(&str, AutoplayMode)] = &[
    ("Continue within source", AutoplayMode::ContinueWithinSource),
    ("Play track only", AutoplayMode::PlayTrackOnly),
    ("Infinite radio", AutoplayMode::InfiniteRadio),
];

/// What a persisted audio change requires of the live `Player`.
enum Apply {
    /// Not an audio setting — nothing to apply.
    None,
    /// Settings struct refresh only (gapless, normalization, ...).
    Reload,
    /// Routing-critical — also re-init the output device.
    Reinit,
}

/// Index -> value maps the dropdown callbacks resolve against. The label
/// lists live in `SettingsState`; these are the parallel value lists.
#[derive(Default)]
struct SettingsMaps {
    backends: Vec<AudioBackendType>,
    /// Device ids, parallel to `SettingsState.devices` labels. An empty
    /// id is the "System default" entry (`output_device = None`).
    devices: Vec<String>,
}

/// Owns the settings stores and the dropdown index maps.
pub struct SettingsCtx {
    audio: AudioSettingsState,
    playback: PlaybackPreferencesState,
    maps: Mutex<SettingsMaps>,
}

impl SettingsCtx {
    /// Open both stores at the shared global path. A store that fails to
    /// open degrades to an empty (no-op) handle rather than aborting.
    pub fn open() -> Arc<Self> {
        let audio = AudioSettingsState::new().unwrap_or_else(|e| {
            log::warn!("[qbz-slint] audio settings store unavailable: {e}");
            AudioSettingsState::new_empty()
        });
        let playback = PlaybackPreferencesState::new().unwrap_or_else(|e| {
            log::warn!("[qbz-slint] playback preferences store unavailable: {e}");
            PlaybackPreferencesState::new_empty()
        });
        Arc::new(Self {
            audio,
            playback,
            maps: Mutex::new(SettingsMaps::default()),
        })
    }
}

/// Plain, `Send` settings data built off the UI thread.
pub struct SettingsSnapshot {
    backends: Vec<String>,
    backend_index: i32,
    devices: Vec<String>,
    device_index: i32,
    sample_rates: Vec<String>,
    sample_rate_index: i32,
    exclusive_mode: bool,
    dac_passthrough: bool,
    pw_force_bitperfect: bool,
    gapless: bool,
    normalization: bool,
    allow_quality_fallback: bool,
    autoplay_modes: Vec<String>,
    autoplay_index: i32,
    show_context_icon: bool,
    persist_session: bool,
    resume_position: bool,
}

/// Devices enumerated for one backend: parallel label / id lists.
struct DeviceList {
    labels: Vec<String>,
    ids: Vec<String>,
}

fn backend_label(t: AudioBackendType) -> String {
    match t {
        AudioBackendType::PipeWire => "PipeWire",
        AudioBackendType::Alsa => "ALSA",
        AudioBackendType::Pulse => "PulseAudio",
        AudioBackendType::SystemDefault => "System default",
    }
    .to_string()
}

/// Enumerate output devices for a backend. Always leads with a "System
/// default" entry (empty id). Blocking — call off the UI thread.
fn enumerate_devices(backend: AudioBackendType) -> DeviceList {
    let mut labels = vec!["System default".to_string()];
    let mut ids = vec![String::new()];
    match BackendManager::create_backend(backend).and_then(|b| b.enumerate_devices()) {
        Ok(devices) => {
            for d in devices {
                let label = match d.description.as_deref() {
                    Some(desc) if !desc.is_empty() => desc.to_string(),
                    _ => d.name.clone(),
                };
                labels.push(label);
                ids.push(d.id);
            }
        }
        Err(e) => log::warn!("[qbz-slint] device enumeration failed: {e}"),
    }
    DeviceList { labels, ids }
}

fn with_audio<T>(
    audio: &AudioSettingsState,
    f: impl FnOnce(&AudioSettingsStore) -> Result<T, String>,
) -> Result<T, String> {
    let guard = audio
        .store
        .lock()
        .map_err(|_| "audio settings lock poisoned".to_string())?;
    let store = guard
        .as_ref()
        .ok_or_else(|| "audio settings store not open".to_string())?;
    f(store)
}

fn with_playback<T>(
    playback: &PlaybackPreferencesState,
    f: impl FnOnce(&PlaybackPreferencesStore) -> Result<T, String>,
) -> Result<T, String> {
    let guard = playback
        .store
        .lock()
        .map_err(|_| "playback preferences lock poisoned".to_string())?;
    let store = guard
        .as_ref()
        .ok_or_else(|| "playback preferences store not open".to_string())?;
    f(store)
}

/// Read both stores and enumerate audio devices. Blocking (SQLite + device
/// enumeration) — run inside `spawn_blocking`. Also fills the index maps.
pub fn load_snapshot(ctx: &SettingsCtx) -> SettingsSnapshot {
    let audio = with_audio(&ctx.audio, |s| s.get_settings()).unwrap_or_default();
    let prefs = with_playback(&ctx.playback, |s| s.get_preferences()).unwrap_or_default();

    let backend_types = BackendManager::available_backends();
    let current_backend = audio.backend_type.unwrap_or_default();
    let backend_index = backend_types
        .iter()
        .position(|t| *t == current_backend)
        .unwrap_or(0);
    let active_backend = backend_types
        .get(backend_index)
        .copied()
        .unwrap_or_default();

    let device_list = enumerate_devices(active_backend);
    let device_index = match &audio.output_device {
        None => 0,
        Some(id) => device_list.ids.iter().position(|d| d == id).unwrap_or(0),
    };

    let sample_rate_index = SAMPLE_RATES
        .iter()
        .position(|(_, r)| *r == audio.preferred_sample_rate)
        .unwrap_or(0);
    let autoplay_index = AUTOPLAY_MODES
        .iter()
        .position(|(_, m)| *m == prefs.autoplay_mode)
        .unwrap_or(0);

    {
        let mut maps = ctx.maps.lock().unwrap_or_else(|e| e.into_inner());
        maps.backends = backend_types.clone();
        maps.devices = device_list.ids.clone();
    }

    SettingsSnapshot {
        backends: backend_types.iter().map(|t| backend_label(*t)).collect(),
        backend_index: backend_index as i32,
        devices: device_list.labels,
        device_index: device_index as i32,
        sample_rates: SAMPLE_RATES.iter().map(|(l, _)| l.to_string()).collect(),
        sample_rate_index: sample_rate_index as i32,
        exclusive_mode: audio.exclusive_mode,
        dac_passthrough: audio.dac_passthrough,
        pw_force_bitperfect: audio.pw_force_bitperfect,
        gapless: audio.gapless_enabled,
        normalization: audio.normalization_enabled,
        allow_quality_fallback: audio.allow_quality_fallback,
        autoplay_modes: AUTOPLAY_MODES.iter().map(|(l, _)| l.to_string()).collect(),
        autoplay_index: autoplay_index as i32,
        show_context_icon: prefs.show_context_icon,
        persist_session: prefs.persist_session,
        resume_position: prefs.resume_playback_position,
    }
}

fn string_model(items: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
}

/// Push a snapshot onto the `SettingsState` global. Runs on the UI thread.
pub fn apply_snapshot(window: &AppWindow, snap: SettingsSnapshot) {
    let st = window.global::<SettingsState>();
    st.set_backends(string_model(snap.backends));
    st.set_backend_index(snap.backend_index);
    st.set_devices(string_model(snap.devices));
    st.set_device_index(snap.device_index);
    st.set_sample_rates(string_model(snap.sample_rates));
    st.set_sample_rate_index(snap.sample_rate_index);
    st.set_exclusive_mode(snap.exclusive_mode);
    st.set_dac_passthrough(snap.dac_passthrough);
    st.set_pw_force_bitperfect(snap.pw_force_bitperfect);
    st.set_gapless(snap.gapless);
    st.set_normalization(snap.normalization);
    st.set_allow_quality_fallback(snap.allow_quality_fallback);
    st.set_autoplay_modes(string_model(snap.autoplay_modes));
    st.set_autoplay_index(snap.autoplay_index);
    st.set_show_context_icon(snap.show_context_icon);
    st.set_persist_session(snap.persist_session);
    st.set_resume_position(snap.resume_position);
    st.set_loading(false);
}

/// Re-read the persisted audio settings and apply them to the live player.
fn apply_audio(ctx: &SettingsCtx, runtime: &AppRuntime<SlintAdapter>, apply: Apply) {
    let reinit = match apply {
        Apply::None => return,
        Apply::Reload => false,
        Apply::Reinit => true,
    };
    let fresh = match with_audio(&ctx.audio, |s| s.get_settings()) {
        Ok(s) => s,
        Err(e) => {
            log::error!("[qbz-slint] re-read audio settings failed: {e}");
            return;
        }
    };
    let player = runtime.core().player();
    if let Err(e) = player.reload_settings(fresh.clone()) {
        log::error!("[qbz-slint] player.reload_settings failed: {e}");
    }
    if reinit {
        if let Err(e) = player.reinit_device(fresh.output_device.clone()) {
            log::error!("[qbz-slint] player.reinit_device failed: {e}");
        }
    }
    log::info!("[qbz-slint] audio settings applied to player (reinit={reinit})");
}

/// Handle a toggle change: persist it, then apply audio ones to the player.
pub fn handle_bool(ctx: &SettingsCtx, runtime: &AppRuntime<SlintAdapter>, key: &str, value: bool) {
    let outcome: Result<Apply, String> = match key {
        "exclusive-mode" => {
            with_audio(&ctx.audio, |s| s.set_exclusive_mode(value)).map(|_| Apply::Reinit)
        }
        "dac-passthrough" => {
            with_audio(&ctx.audio, |s| s.set_dac_passthrough(value)).map(|_| Apply::Reinit)
        }
        "pw-force-bitperfect" => {
            with_audio(&ctx.audio, |s| s.set_pw_force_bitperfect(value)).map(|_| Apply::Reload)
        }
        "gapless" => {
            with_audio(&ctx.audio, |s| s.set_gapless_enabled(value)).map(|_| Apply::Reload)
        }
        "normalization" => {
            with_audio(&ctx.audio, |s| s.set_normalization_enabled(value)).map(|_| Apply::Reload)
        }
        "allow-quality-fallback" => {
            with_audio(&ctx.audio, |s| s.set_allow_quality_fallback(value)).map(|_| Apply::Reload)
        }
        "show-context-icon" => {
            with_playback(&ctx.playback, |s| s.set_show_context_icon(value)).map(|_| Apply::None)
        }
        "persist-session" => {
            with_playback(&ctx.playback, |s| s.set_persist_session(value)).map(|_| Apply::None)
        }
        "resume-position" => with_playback(&ctx.playback, |s| {
            s.set_resume_playback_position(value)
        })
        .map(|_| Apply::None),
        other => {
            log::warn!("[qbz-slint] unknown settings bool key: {other}");
            return;
        }
    };
    match outcome {
        Ok(apply) => apply_audio(ctx, runtime, apply),
        Err(e) => log::error!("[qbz-slint] failed to persist '{key}': {e}"),
    }
}

/// Handle a dropdown change: persist it, apply audio ones to the player,
/// and — for a backend switch — re-enumerate devices into `SettingsState`.
pub async fn handle_select(
    ctx: Arc<SettingsCtx>,
    runtime: Arc<AppRuntime<SlintAdapter>>,
    weak: slint::Weak<AppWindow>,
    key: String,
    index: usize,
) {
    match key.as_str() {
        "backend" => {
            let backend = ctx
                .maps
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .backends
                .get(index)
                .copied();
            let Some(backend) = backend else {
                return;
            };
            if let Err(e) = with_audio(&ctx.audio, |s| s.set_backend_type(Some(backend))) {
                log::error!("[qbz-slint] persist backend failed: {e}");
                return;
            }
            // A backend switch invalidates the device list; re-enumerate
            // and reset routing to the system default.
            let device_list =
                match tokio::task::spawn_blocking(move || enumerate_devices(backend)).await {
                    Ok(d) => d,
                    Err(e) => {
                        log::error!("[qbz-slint] device enumeration task failed: {e}");
                        return;
                    }
                };
            if let Err(e) = with_audio(&ctx.audio, |s| s.set_output_device(None)) {
                log::error!("[qbz-slint] reset output device failed: {e}");
            }
            {
                let mut maps = ctx.maps.lock().unwrap_or_else(|e| e.into_inner());
                maps.devices = device_list.ids.clone();
            }
            let labels = device_list.labels;
            let _ = weak.upgrade_in_event_loop(move |w| {
                let st = w.global::<SettingsState>();
                st.set_devices(string_model(labels));
                st.set_device_index(0);
            });
            apply_audio(&ctx, &runtime, Apply::Reinit);
        }
        "device" => {
            let id = ctx
                .maps
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .devices
                .get(index)
                .cloned();
            let Some(id) = id else {
                return;
            };
            let device_opt = if id.is_empty() { None } else { Some(id.as_str()) };
            if let Err(e) = with_audio(&ctx.audio, |s| s.set_output_device(device_opt)) {
                log::error!("[qbz-slint] persist device failed: {e}");
                return;
            }
            apply_audio(&ctx, &runtime, Apply::Reinit);
        }
        "sample-rate" => {
            let rate = SAMPLE_RATES.get(index).map(|(_, r)| *r).unwrap_or(None);
            if let Err(e) = with_audio(&ctx.audio, |s| s.set_sample_rate(rate)) {
                log::error!("[qbz-slint] persist sample rate failed: {e}");
                return;
            }
            apply_audio(&ctx, &runtime, Apply::Reinit);
        }
        "autoplay-mode" => {
            let mode = AUTOPLAY_MODES
                .get(index)
                .map(|(_, m)| *m)
                .unwrap_or_default();
            if let Err(e) = with_playback(&ctx.playback, |s| s.set_autoplay_mode(mode)) {
                log::error!("[qbz-slint] persist autoplay mode failed: {e}");
            }
        }
        other => log::warn!("[qbz-slint] unknown settings select key: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_rate_table_starts_with_auto() {
        assert_eq!(SAMPLE_RATES[0].1, None);
        assert_eq!(SAMPLE_RATES[4].1, Some(96_000));
    }

    #[test]
    fn autoplay_table_matches_default() {
        assert_eq!(AUTOPLAY_MODES[0].1, AutoplayMode::default());
    }

    #[test]
    fn backend_labels_are_distinct() {
        let labels: Vec<_> = [
            AudioBackendType::PipeWire,
            AudioBackendType::Alsa,
            AudioBackendType::Pulse,
            AudioBackendType::SystemDefault,
        ]
        .iter()
        .map(|t| backend_label(*t))
        .collect();
        let mut deduped = labels.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(labels.len(), deduped.len());
    }
}
