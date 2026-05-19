//! Settings controller — Audio and Playback preferences.
//!
//! Owns the two persistence stores (`AudioSettingsStore` from `qbz-audio`,
//! `PlaybackPreferencesStore` from `qbz-app`) plus the JSON `ui_prefs`
//! store (Streaming Quality), and bridges them to the `SettingsState`
//! Slint global.
//!
//! Audio changes are persisted and then applied to the live `Player`:
//! routing-critical changes (backend, output device, max sample rate,
//! exclusive mode, DAC passthrough, ALSA plugin) trigger a device
//! re-init; the rest only reload the settings struct. Playback-preference
//! changes (autoplay, show-context, persist, resume) just persist.
//!
//! Neither domain store is exposed by `AppRuntime`, so this module opens
//! them directly at the shared global path — the same path
//! `AppRuntime::new` reads to seed the `Player`, so the two stay
//! consistent.

use std::sync::{Arc, Mutex};

use qbz_app::settings::playback::{
    AutoplayMode, PlaybackPreferencesState, PlaybackPreferencesStore,
};
use qbz_app::shell::AppRuntime;
use qbz_audio::backend::{AlsaPlugin, AudioBackendType, BackendManager};
use qbz_audio::settings::{AudioSettingsState, AudioSettingsStore};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::adapter::SlintAdapter;
use crate::ui_prefs::{self, STREAMING_QUALITIES};
use crate::{AppWindow, SettingsState};

/// Maximum-sample-rate dropdown options. Index 0 is "No limit" (`None`).
/// Backs `device_max_sample_rate`.
const MAX_SAMPLE_RATES: &[(&str, Option<u32>)] = &[
    ("No limit", None),
    ("44.1 kHz", Some(44_100)),
    ("48 kHz", Some(48_000)),
    ("88.2 kHz", Some(88_200)),
    ("96 kHz", Some(96_000)),
    ("176.4 kHz", Some(176_400)),
    ("192 kHz", Some(192_000)),
    ("352.8 kHz", Some(352_800)),
    ("384 kHz", Some(384_000)),
];

/// ALSA-plugin dropdown options.
const ALSA_PLUGINS: &[(&str, AlsaPlugin)] = &[
    ("hw (Direct Hardware)", AlsaPlugin::Hw),
    ("plughw (Auto-convert)", AlsaPlugin::PlugHw),
    ("pcm (Most compatible)", AlsaPlugin::Pcm),
];

/// "When quality retries fail" dropdown options. The value is the
/// `quality_fallback_behavior` DB string.
const RETRY_BEHAVIORS: &[(&str, &str)] = &[
    ("Ask me", "ask"),
    ("Always try lowest quality", "always_fallback"),
    ("Always skip track", "always_skip"),
];

/// What a persisted audio change requires of the live `Player`.
enum Apply {
    /// Not a player-applied setting — nothing to apply.
    None,
    /// Settings struct refresh only (gapless, fallback, stream-*, ...).
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
    /// Open both domain stores at the shared global path. A store that
    /// fails to open degrades to an empty (no-op) handle rather than
    /// aborting.
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
    // Audio — dropdowns.
    streaming_qualities: Vec<String>,
    streaming_quality_index: i32,
    sample_rates: Vec<String>,
    sample_rate_index: i32,
    backends: Vec<String>,
    backend_index: i32,
    devices: Vec<String>,
    device_index: i32,
    alsa_plugins: Vec<String>,
    alsa_plugin_index: i32,
    // Audio — toggles.
    limit_quality_to_device: bool,
    alsa_hardware_volume: bool,
    exclusive_mode: bool,
    reserve_dac: bool,
    dac_passthrough: bool,
    pw_force_bitperfect: bool,
    allow_quality_fallback: bool,
    sync_audio_on_startup: bool,
    skip_sink_switch: bool,
    // Audio — conditional flags.
    backend_is_alsa: bool,
    backend_is_pipewire: bool,
    alsa_plugin_is_hw: bool,
    // Playback.
    continue_playback: bool,
    show_context_icon: bool,
    persist_session: bool,
    resume_position: bool,
    gapless: bool,
    stream_uncached: bool,
    streaming_only: bool,
    buffer_seconds: i32,
    retry_behaviors: Vec<String>,
    retry_behavior_index: i32,
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

/// Build a snapshot from already-read settings. Splitting this out lets
/// `load_snapshot` and a post-reset rebuild share the device-enumeration
/// and index-mapping logic.
fn build_snapshot(
    ctx: &SettingsCtx,
    audio: qbz_audio::settings::AudioSettings,
    prefs: qbz_app::settings::playback::PlaybackPreferences,
    streaming_quality_key: &str,
) -> SettingsSnapshot {
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

    let sample_rate_index = MAX_SAMPLE_RATES
        .iter()
        .position(|(_, r)| *r == audio.device_max_sample_rate)
        .unwrap_or(0);
    let alsa_plugin = audio.alsa_plugin.unwrap_or(AlsaPlugin::Hw);
    let alsa_plugin_index = ALSA_PLUGINS
        .iter()
        .position(|(_, p)| *p == alsa_plugin)
        .unwrap_or(0);
    let retry_behavior_index = RETRY_BEHAVIORS
        .iter()
        .position(|(_, v)| *v == audio.quality_fallback_behavior)
        .unwrap_or(0);

    let backend_is_alsa = active_backend == AudioBackendType::Alsa;
    let backend_is_pipewire = active_backend == AudioBackendType::PipeWire;
    let alsa_plugin_is_hw = alsa_plugin == AlsaPlugin::Hw;
    let continue_playback = prefs.autoplay_mode == AutoplayMode::ContinueWithinSource;

    {
        let mut maps = ctx.maps.lock().unwrap_or_else(|e| e.into_inner());
        maps.backends = backend_types.clone();
        maps.devices = device_list.ids.clone();
    }

    SettingsSnapshot {
        streaming_qualities: STREAMING_QUALITIES
            .iter()
            .map(|q| q.label.to_string())
            .collect(),
        streaming_quality_index: ui_prefs::streaming_quality_index(streaming_quality_key) as i32,
        sample_rates: MAX_SAMPLE_RATES.iter().map(|(l, _)| l.to_string()).collect(),
        sample_rate_index: sample_rate_index as i32,
        backends: backend_types.iter().map(|t| backend_label(*t)).collect(),
        backend_index: backend_index as i32,
        devices: device_list.labels,
        device_index: device_index as i32,
        alsa_plugins: ALSA_PLUGINS.iter().map(|(l, _)| l.to_string()).collect(),
        alsa_plugin_index: alsa_plugin_index as i32,
        limit_quality_to_device: audio.limit_quality_to_device,
        alsa_hardware_volume: audio.alsa_hardware_volume,
        exclusive_mode: audio.exclusive_mode,
        reserve_dac: audio.reserve_dac_while_running,
        dac_passthrough: audio.dac_passthrough,
        pw_force_bitperfect: audio.pw_force_bitperfect,
        allow_quality_fallback: audio.allow_quality_fallback,
        sync_audio_on_startup: audio.sync_audio_on_startup,
        skip_sink_switch: audio.skip_sink_switch,
        backend_is_alsa,
        backend_is_pipewire,
        alsa_plugin_is_hw,
        continue_playback,
        show_context_icon: prefs.show_context_icon,
        persist_session: prefs.persist_session,
        resume_position: prefs.resume_playback_position,
        gapless: audio.gapless_enabled,
        stream_uncached: audio.stream_first_track,
        streaming_only: audio.streaming_only,
        buffer_seconds: audio.stream_buffer_seconds as i32,
        retry_behaviors: RETRY_BEHAVIORS.iter().map(|(l, _)| l.to_string()).collect(),
        retry_behavior_index: retry_behavior_index as i32,
    }
}

/// Read both domain stores, the JSON UI prefs, and enumerate audio
/// devices. Blocking (SQLite + device enumeration) — run inside
/// `spawn_blocking`. Also fills the index maps.
pub fn load_snapshot(ctx: &SettingsCtx) -> SettingsSnapshot {
    let audio = with_audio(&ctx.audio, |s| s.get_settings()).unwrap_or_default();
    let prefs = with_playback(&ctx.playback, |s| s.get_preferences()).unwrap_or_default();
    let ui = ui_prefs::load();
    build_snapshot(ctx, audio, prefs, &ui.streaming_quality)
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
    // Audio — dropdowns.
    st.set_streaming_qualities(string_model(snap.streaming_qualities));
    st.set_streaming_quality_index(snap.streaming_quality_index);
    st.set_sample_rates(string_model(snap.sample_rates));
    st.set_sample_rate_index(snap.sample_rate_index);
    st.set_backends(string_model(snap.backends));
    st.set_backend_index(snap.backend_index);
    st.set_devices(string_model(snap.devices));
    st.set_device_index(snap.device_index);
    st.set_alsa_plugins(string_model(snap.alsa_plugins));
    st.set_alsa_plugin_index(snap.alsa_plugin_index);
    // Audio — toggles.
    st.set_limit_quality_to_device(snap.limit_quality_to_device);
    st.set_alsa_hardware_volume(snap.alsa_hardware_volume);
    st.set_exclusive_mode(snap.exclusive_mode);
    st.set_reserve_dac(snap.reserve_dac);
    st.set_dac_passthrough(snap.dac_passthrough);
    st.set_pw_force_bitperfect(snap.pw_force_bitperfect);
    st.set_allow_quality_fallback(snap.allow_quality_fallback);
    st.set_sync_audio_on_startup(snap.sync_audio_on_startup);
    st.set_skip_sink_switch(snap.skip_sink_switch);
    // Audio — conditional flags.
    st.set_backend_is_alsa(snap.backend_is_alsa);
    st.set_backend_is_pipewire(snap.backend_is_pipewire);
    st.set_alsa_plugin_is_hw(snap.alsa_plugin_is_hw);
    // Playback.
    st.set_continue_playback(snap.continue_playback);
    st.set_show_context_icon(snap.show_context_icon);
    st.set_persist_session(snap.persist_session);
    st.set_resume_position(snap.resume_position);
    st.set_gapless(snap.gapless);
    st.set_stream_uncached(snap.stream_uncached);
    st.set_streaming_only(snap.streaming_only);
    st.set_buffer_seconds(snap.buffer_seconds);
    st.set_retry_behaviors(string_model(snap.retry_behaviors));
    st.set_retry_behavior_index(snap.retry_behavior_index);
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

/// Recompute the backend/ALSA conditional flags from the current audio
/// settings and push them onto `SettingsState`. Called after a backend or
/// ALSA-plugin change so the `.slint` panels re-gate the conditional rows.
fn push_conditional_flags(ctx: &SettingsCtx, weak: &slint::Weak<AppWindow>) {
    let audio = match with_audio(&ctx.audio, |s| s.get_settings()) {
        Ok(s) => s,
        Err(e) => {
            log::error!("[qbz-slint] re-read audio settings for flags failed: {e}");
            return;
        }
    };
    let backend = audio.backend_type.unwrap_or_default();
    let plugin = audio.alsa_plugin.unwrap_or(AlsaPlugin::Hw);
    let is_alsa = backend == AudioBackendType::Alsa;
    let is_pipewire = backend == AudioBackendType::PipeWire;
    let plugin_is_hw = plugin == AlsaPlugin::Hw;
    let plugin_index = ALSA_PLUGINS
        .iter()
        .position(|(_, p)| *p == plugin)
        .unwrap_or(0) as i32;
    let _ = weak.upgrade_in_event_loop(move |w| {
        let st = w.global::<SettingsState>();
        st.set_backend_is_alsa(is_alsa);
        st.set_backend_is_pipewire(is_pipewire);
        st.set_alsa_plugin_is_hw(plugin_is_hw);
        st.set_alsa_plugin_index(plugin_index);
    });
}

/// Handle a toggle change: persist it, then apply audio ones to the
/// player.
///
/// No audio toggle moves the backend or ALSA plugin, so no conditional
/// flag recompute is needed here — the rows gated on toggle state
/// (`dac-passthrough`, `limit-quality-to-device`, `stream-uncached`,
/// `persist-session`) read `SettingsState` booleans directly in Slint.
/// Backend / ALSA-plugin flag recomputes happen in `handle_select`.
pub fn handle_bool(
    ctx: &SettingsCtx,
    runtime: &AppRuntime<SlintAdapter>,
    key: &str,
    value: bool,
) {
    let outcome: Result<Apply, String> = match key {
        // --- Audio toggles -------------------------------------------------
        "limit-quality-to-device" => {
            with_audio(&ctx.audio, |s| s.set_limit_quality_to_device(value))
                .map(|_| Apply::Reload)
        }
        "alsa-hardware-volume" => {
            with_audio(&ctx.audio, |s| s.set_alsa_hardware_volume(value)).map(|_| Apply::Reinit)
        }
        "exclusive-mode" => {
            with_audio(&ctx.audio, |s| s.set_exclusive_mode(value)).map(|_| Apply::Reinit)
        }
        "reserve-dac" => {
            with_audio(&ctx.audio, |s| s.set_reserve_dac_while_running(value))
                .map(|_| Apply::Reload)
        }
        "dac-passthrough" => {
            with_audio(&ctx.audio, |s| s.set_dac_passthrough(value)).map(|_| Apply::Reinit)
        }
        "pw-force-bitperfect" => {
            with_audio(&ctx.audio, |s| s.set_pw_force_bitperfect(value)).map(|_| Apply::Reload)
        }
        "allow-quality-fallback" => {
            with_audio(&ctx.audio, |s| s.set_allow_quality_fallback(value))
                .map(|_| Apply::Reload)
        }
        "sync-audio-on-startup" => {
            with_audio(&ctx.audio, |s| s.set_sync_audio_on_startup(value)).map(|_| Apply::Reload)
        }
        "skip-sink-switch" => {
            with_audio(&ctx.audio, |s| s.set_skip_sink_switch(value)).map(|_| Apply::Reinit)
        }
        // --- Playback toggles backed by AudioSettings ----------------------
        "gapless" => {
            with_audio(&ctx.audio, |s| s.set_gapless_enabled(value)).map(|_| Apply::Reload)
        }
        "stream-uncached" => {
            with_audio(&ctx.audio, |s| s.set_stream_first_track(value)).map(|_| Apply::Reload)
        }
        "streaming-only" => {
            with_audio(&ctx.audio, |s| s.set_streaming_only(value)).map(|_| Apply::Reload)
        }
        // --- Playback toggles backed by PlaybackPreferences ----------------
        "continue-playback" => {
            // On = ContinueWithinSource, off = PlayTrackOnly.
            let mode = if value {
                AutoplayMode::ContinueWithinSource
            } else {
                AutoplayMode::PlayTrackOnly
            };
            with_playback(&ctx.playback, |s| s.set_autoplay_mode(mode)).map(|_| Apply::None)
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

/// Handle a slider change: persist it and reload the player settings.
/// Currently only the Initial Buffer Size slider exists.
pub fn handle_slider(
    ctx: &SettingsCtx,
    runtime: &AppRuntime<SlintAdapter>,
    key: &str,
    value: i32,
) {
    match key {
        "buffer-seconds" => {
            let seconds = value.clamp(1, 10) as u8;
            match with_audio(&ctx.audio, |s| s.set_stream_buffer_seconds(seconds)) {
                Ok(()) => apply_audio(ctx, runtime, Apply::Reload),
                Err(e) => log::error!("[qbz-slint] persist buffer seconds failed: {e}"),
            }
        }
        other => log::warn!("[qbz-slint] unknown settings slider key: {other}"),
    }
}

/// Handle a dropdown change: persist it, apply audio ones to the player,
/// and — for a backend switch — re-enumerate devices and recompute the
/// conditional flags into `SettingsState`.
pub async fn handle_select(
    ctx: Arc<SettingsCtx>,
    runtime: Arc<AppRuntime<SlintAdapter>>,
    weak: slint::Weak<AppWindow>,
    key: String,
    index: usize,
) {
    match key.as_str() {
        "streaming-quality" => {
            // UI-only preference, persisted to ui_prefs.json.
            let Some(quality) = STREAMING_QUALITIES.get(index) else {
                return;
            };
            let mut prefs = ui_prefs::load();
            prefs.streaming_quality = quality.key.to_string();
            ui_prefs::save(&prefs);
        }
        "sample-rate" => {
            let rate = MAX_SAMPLE_RATES.get(index).map(|(_, r)| *r).unwrap_or(None);
            if let Err(e) = with_audio(&ctx.audio, |s| s.set_device_max_sample_rate(rate)) {
                log::error!("[qbz-slint] persist max sample rate failed: {e}");
                return;
            }
            apply_audio(&ctx, &runtime, Apply::Reinit);
        }
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
            // Recompute conditional flags (backend-is-alsa / -pipewire).
            push_conditional_flags(&ctx, &weak);
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
        "alsa-plugin" => {
            let plugin = ALSA_PLUGINS.get(index).map(|(_, p)| *p);
            let Some(plugin) = plugin else {
                return;
            };
            if let Err(e) = with_audio(&ctx.audio, |s| s.set_alsa_plugin(Some(plugin))) {
                log::error!("[qbz-slint] persist ALSA plugin failed: {e}");
                return;
            }
            // ALSA plugin gates the Hardware Volume Control row.
            push_conditional_flags(&ctx, &weak);
            apply_audio(&ctx, &runtime, Apply::Reinit);
        }
        "retry-behavior" => {
            let behavior = RETRY_BEHAVIORS.get(index).map(|(_, v)| *v).unwrap_or("ask");
            if let Err(e) = with_audio(&ctx.audio, |s| s.set_quality_fallback_behavior(behavior)) {
                log::error!("[qbz-slint] persist retry behavior failed: {e}");
                return;
            }
            apply_audio(&ctx, &runtime, Apply::Reload);
        }
        other => log::warn!("[qbz-slint] unknown settings select key: {other}"),
    }
}

/// Reset all Audio + Playback settings to defaults, rebuild the snapshot,
/// push it onto `SettingsState`, and re-apply the audio settings to the
/// player. Streaming Quality (a UI-only pref) is intentionally left
/// untouched — it is not part of either domain store.
pub async fn handle_reset(
    ctx: Arc<SettingsCtx>,
    runtime: Arc<AppRuntime<SlintAdapter>>,
    weak: slint::Weak<AppWindow>,
) {
    if let Err(e) = with_audio(&ctx.audio, |s| s.reset_all()) {
        log::error!("[qbz-slint] audio reset_all failed: {e}");
    }
    if let Err(e) = with_playback(&ctx.playback, |s| s.reset_all()) {
        log::error!("[qbz-slint] playback reset_all failed: {e}");
    }
    // Rebuild the snapshot off the UI thread (device enumeration blocks).
    let snap = {
        let ctx = ctx.clone();
        match tokio::task::spawn_blocking(move || load_snapshot(&ctx)).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("[qbz-slint] settings reset rebuild task failed: {e}");
                return;
            }
        }
    };
    let _ = weak.upgrade_in_event_loop(move |w| {
        apply_snapshot(&w, snap);
    });
    // Routing-critical defaults changed — re-init the device.
    apply_audio(&ctx, &runtime, Apply::Reinit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_sample_rate_table_starts_with_no_limit() {
        assert_eq!(MAX_SAMPLE_RATES[0].1, None);
        assert_eq!(MAX_SAMPLE_RATES.last().unwrap().1, Some(384_000));
        assert_eq!(MAX_SAMPLE_RATES.len(), 9);
    }

    #[test]
    fn alsa_plugin_table_first_is_hw() {
        assert_eq!(ALSA_PLUGINS[0].1, AlsaPlugin::Hw);
        assert_eq!(ALSA_PLUGINS.len(), 3);
    }

    #[test]
    fn retry_behavior_table_first_is_ask() {
        assert_eq!(RETRY_BEHAVIORS[0].1, "ask");
        assert_eq!(RETRY_BEHAVIORS[1].1, "always_fallback");
        assert_eq!(RETRY_BEHAVIORS[2].1, "always_skip");
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
