use tauri::State;

use crate::audio::{AlsaPlugin, AudioBackendType};
use crate::audio_device_watch::{check_selected_device_presence, DevicePresence};
use crate::config::audio_settings::{AudioSettings, AudioSettingsState};
use crate::core_bridge::CoreBridgeState;
use crate::runtime::RuntimeError;

use super::{convert_to_qbz_audio_settings, sync_audio_settings_to_player};

/// Frontend-facing shape: serializable snapshot of the presence check.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AudioDevicePresence {
    UsingDefault,
    Present { wanted: String },
    Missing { wanted: String, available: Vec<String> },
    Inconclusive,
}

/// Snapshot of which output device is currently in use and whether we're playing.
/// Mirrors the legacy `AudioOutputStatus` shape so the frontend type is unchanged.
#[derive(Debug, serde::Serialize)]
pub struct V2AudioOutputStatus {
    pub device_name: Option<String>,
    pub is_playing: bool,
}

/// Get current audio output status (V2 — reads from CoreBridge.player state).
///
/// Returns the device name actually in use by the audio thread plus whether
/// playback is currently active. Used by AudioOutputBadges to render the
/// active sink + DAC indicators in the now-playing bar.
#[tauri::command]
pub async fn v2_get_audio_output_status(
    bridge: State<'_, CoreBridgeState>,
) -> Result<V2AudioOutputStatus, RuntimeError> {
    let bridge_guard = bridge.get().await;
    let player = bridge_guard.player();
    Ok(V2AudioOutputStatus {
        device_name: player.state.current_device(),
        is_playing: player.state.is_playing(),
    })
}

/// List the available CPAL output sinks (V2).
///
/// Delegates enumeration to `qbz_audio::output_sinks::list_output_sinks`,
/// which returns the same `{name, description, volume, is_default}` shape
/// the legacy `get_pipewire_sinks` command returned. Used by the audio
/// settings UI and AudioOutputBadges to populate the device picker and
/// label the currently-routed sink.
#[tauri::command]
pub fn v2_get_pipewire_sinks() -> Result<Vec<qbz_audio::OutputSinkInfo>, RuntimeError> {
    qbz_audio::list_output_sinks().map_err(RuntimeError::Internal)
}

/// Snapshot the presence of the user's currently-selected output
/// device. Frontend calls this on demand (e.g. when a
/// `audio:device-missing` toast button fires Retry).
#[tauri::command]
pub fn v2_check_audio_device_presence(
    audio_settings: State<'_, AudioSettingsState>,
) -> Result<AudioDevicePresence, RuntimeError> {
    let presence = check_selected_device_presence(&audio_settings);
    Ok(match presence {
        DevicePresence::UsingDefault => AudioDevicePresence::UsingDefault,
        DevicePresence::Present => {
            // Re-read the wanted name so we can echo it back.
            let wanted = audio_settings
                .store
                .lock()
                .ok()
                .and_then(|g| g.as_ref().and_then(|s| s.get_settings().ok()))
                .and_then(|s| s.output_device)
                .unwrap_or_default();
            AudioDevicePresence::Present { wanted }
        }
        DevicePresence::Missing { wanted, available } => {
            AudioDevicePresence::Missing { wanted, available }
        }
        DevicePresence::Inconclusive => AudioDevicePresence::Inconclusive,
    })
}

// ==================== Audio Device Commands (V2) ====================

/// Reinitialize audio device (V2 - uses CoreBridge.player)
/// Call this when changing audio settings like exclusive mode or output device
#[tauri::command]
pub async fn v2_reinit_audio_device(
    device: Option<String>,
    bridge: State<'_, CoreBridgeState>,
    audio_settings: State<'_, AudioSettingsState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] Command: reinit_audio_device {:?}", device);

    let bridge_guard = bridge.get().await;
    let player = bridge_guard.player();

    // Reload settings from database to ensure Player has latest config
    if let Ok(guard) = audio_settings.store.lock() {
        if let Some(store) = guard.as_ref() {
            if let Ok(fresh_settings) = store.get_settings() {
                log::info!(
                    "[V2] Reloading audio settings before reinit (backend_type: {:?})",
                    fresh_settings.backend_type
                );
                let _ = player.reload_settings(convert_to_qbz_audio_settings(&fresh_settings));
            }
        }
    }

    player.reinit_device(device).map_err(RuntimeError::Internal)
}

// ==================== Audio Settings Commands (V2) ====================

/// Get current audio settings (V2)
#[tauri::command]
pub fn v2_get_audio_settings(
    state: State<'_, AudioSettingsState>,
) -> Result<AudioSettings, RuntimeError> {
    log::info!("[V2] get_audio_settings");
    let guard = state
        .store
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    let store = guard
        .as_ref()
        .ok_or(RuntimeError::UserSessionNotActivated)?;
    store.get_settings().map_err(RuntimeError::Internal)
}

/// Set audio output device (V2)
#[tauri::command]
pub async fn v2_set_audio_output_device(
    device: Option<String>,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
    app_state: State<'_, crate::AppState>,
) -> Result<(), RuntimeError> {
    let normalized_device = device
        .as_ref()
        .map(|d| crate::audio::normalize_device_id_to_stable(d));
    log::info!(
        "[V2] set_audio_output_device {:?} -> {:?} (normalized)",
        device,
        normalized_device
    );
    let lifetime_b_enabled = {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_output_device(normalized_device.as_deref())
            .map_err(RuntimeError::Internal)?;
        store
            .get_settings()
            .map_err(RuntimeError::Internal)?
            .reserve_dac_while_running
    };
    sync_audio_settings_to_player(&state, &bridge).await;

    // Lifetime B: when the user changes DAC, drop the old reservation
    // and reacquire on the new card. Pass `None` if the new device isn't
    // card-specific (apply_dac_reservation will release without
    // reacquiring) or if the toggle is off.
    if lifetime_b_enabled {
        app_state
            .apply_dac_reservation(normalized_device.as_deref(), normalized_device.as_deref());
    }
    Ok(())
}

/// Set audio exclusive mode (V2)
#[tauri::command]
pub async fn v2_set_audio_exclusive_mode(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_exclusive_mode: {}", enabled);
    {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_exclusive_mode(enabled)
            .map_err(RuntimeError::Internal)?;
    }
    sync_audio_settings_to_player(&state, &bridge).await;
    Ok(())
}

/// Set DAC passthrough mode (V2)
#[tauri::command]
pub async fn v2_set_audio_dac_passthrough(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_dac_passthrough: {}", enabled);
    {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_dac_passthrough(enabled)
            .map_err(RuntimeError::Internal)?;
    }
    sync_audio_settings_to_player(&state, &bridge).await;
    Ok(())
}

/// Set PipeWire force bit-perfect mode (V2)
#[tauri::command]
pub async fn v2_set_audio_pw_force_bitperfect(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_pw_force_bitperfect: {}", enabled);
    {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_pw_force_bitperfect(enabled)
            .map_err(RuntimeError::Internal)?;
    }
    sync_audio_settings_to_player(&state, &bridge).await;
    Ok(())
}

/// Set sync audio settings on startup (V2)
#[tauri::command]
pub fn v2_set_sync_audio_on_startup(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_sync_audio_on_startup: {}", enabled);
    let guard = state
        .store
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    let store = guard
        .as_ref()
        .ok_or(RuntimeError::UserSessionNotActivated)?;
    store
        .set_sync_audio_on_startup(enabled)
        .map_err(RuntimeError::Internal)
}

/// Get quality fallback behavior (V2)
#[tauri::command]
pub fn v2_get_quality_fallback_behavior(
    audio_settings: State<'_, AudioSettingsState>,
) -> Result<String, RuntimeError> {
    let guard = audio_settings
        .store
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    let store = guard
        .as_ref()
        .ok_or(RuntimeError::UserSessionNotActivated)?;
    store
        .get_quality_fallback_behavior()
        .map_err(RuntimeError::Internal)
}

/// Set quality fallback behavior (V2)
#[tauri::command]
pub fn v2_set_quality_fallback_behavior(
    behavior: String,
    audio_settings: State<'_, AudioSettingsState>,
) -> Result<(), RuntimeError> {
    log::info!("Command: v2_set_quality_fallback_behavior {}", behavior);
    let guard = audio_settings
        .store
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    let store = guard
        .as_ref()
        .ok_or(RuntimeError::UserSessionNotActivated)?;
    store
        .set_quality_fallback_behavior(&behavior)
        .map_err(RuntimeError::Internal)
}

/// Set preferred sample rate (V2)
#[tauri::command]
pub async fn v2_set_audio_sample_rate(
    rate: Option<u32>,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_sample_rate: {:?}", rate);
    {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store.set_sample_rate(rate).map_err(RuntimeError::Internal)?;
    }
    sync_audio_settings_to_player(&state, &bridge).await;
    Ok(())
}

/// Set audio backend type (V2)
#[tauri::command]
#[allow(non_snake_case)]
pub async fn v2_set_audio_backend_type(
    backendType: Option<AudioBackendType>,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_backend_type: {:?}", backendType);
    {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_backend_type(backendType)
            .map_err(RuntimeError::Internal)?;
    }
    sync_audio_settings_to_player(&state, &bridge).await;
    Ok(())
}

/// Set ALSA plugin (V2)
#[tauri::command]
pub async fn v2_set_audio_alsa_plugin(
    plugin: Option<AlsaPlugin>,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_alsa_plugin: {:?}", plugin);
    {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_alsa_plugin(plugin)
            .map_err(RuntimeError::Internal)?;
    }
    sync_audio_settings_to_player(&state, &bridge).await;
    Ok(())
}

/// Set gapless playback enabled (V2)
#[tauri::command]
pub async fn v2_set_audio_gapless_enabled(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_gapless_enabled: {}", enabled);
    let fresh_settings = {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_gapless_enabled(enabled)
            .map_err(RuntimeError::Internal)?;
        store.get_settings().ok()
    }; // guard dropped here before .await

    // Sync to player immediately so gapless takes effect without restart
    if let Some(fresh) = fresh_settings {
        if let Some(b) = bridge.try_get().await {
            let _ = b
                .player()
                .reload_settings(convert_to_qbz_audio_settings(&fresh));
        }
    }
    Ok(())
}

/// Set allow quality fallback (V2)
#[tauri::command]
pub async fn v2_set_audio_allow_quality_fallback(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_allow_quality_fallback: {}", enabled);
    let guard = state
        .store
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    let store = guard
        .as_ref()
        .ok_or(RuntimeError::UserSessionNotActivated)?;
    store
        .set_allow_quality_fallback(enabled)
        .map_err(RuntimeError::Internal)?;
    Ok(())
}

/// Set skip sink switch (V2) — preserves JACK/qjackctl routing
#[tauri::command]
pub async fn v2_set_audio_skip_sink_switch(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_skip_sink_switch: {}", enabled);
    let fresh_settings = {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;

        // Constraint: cannot enable when dac_passthrough is on
        if enabled {
            let current = store.get_settings().map_err(RuntimeError::Internal)?;
            if current.dac_passthrough {
                return Err(RuntimeError::Internal(
                    "Cannot enable skip sink switch while DAC passthrough is active".to_string(),
                ));
            }
        }

        store
            .set_skip_sink_switch(enabled)
            .map_err(RuntimeError::Internal)?;
        store.get_settings().ok()
    };

    if let Some(fresh) = fresh_settings {
        if let Some(b) = bridge.try_get().await {
            let _ = b
                .player()
                .reload_settings(convert_to_qbz_audio_settings(&fresh));
        }
    }
    Ok(())
}

/// Set normalization enabled (V2)
#[tauri::command]
pub async fn v2_set_audio_normalization_enabled(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_normalization_enabled: {}", enabled);
    let fresh_settings = {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_normalization_enabled(enabled)
            .map_err(RuntimeError::Internal)?;
        store.get_settings().ok()
    };

    if let Some(fresh) = fresh_settings {
        if let Some(b) = bridge.try_get().await {
            let _ = b
                .player()
                .reload_settings(convert_to_qbz_audio_settings(&fresh));
        }
    }
    Ok(())
}

/// Set normalization target LUFS (V2)
#[tauri::command]
pub async fn v2_set_audio_normalization_target(
    target: f32,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_normalization_target: {}", target);
    let fresh_settings = {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_normalization_target_lufs(target)
            .map_err(RuntimeError::Internal)?;
        store.get_settings().ok()
    };

    if let Some(fresh) = fresh_settings {
        if let Some(b) = bridge.try_get().await {
            let _ = b
                .player()
                .reload_settings(convert_to_qbz_audio_settings(&fresh));
        }
    }
    Ok(())
}

/// Set device max sample rate (V2)
#[tauri::command]
pub fn v2_set_audio_device_max_sample_rate(
    rate: Option<u32>,
    state: State<'_, AudioSettingsState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_device_max_sample_rate: {:?}", rate);
    let guard = state
        .store
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    let store = guard
        .as_ref()
        .ok_or(RuntimeError::UserSessionNotActivated)?;
    store
        .set_device_max_sample_rate(rate)
        .map_err(RuntimeError::Internal)
}

/// Set limit quality to device capability (V2)
#[tauri::command]
pub fn v2_set_audio_limit_quality_to_device(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_limit_quality_to_device: {}", enabled);
    let guard = state
        .store
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    let store = guard
        .as_ref()
        .ok_or(RuntimeError::UserSessionNotActivated)?;
    store
        .set_limit_quality_to_device(enabled)
        .map_err(RuntimeError::Internal)
}

/// Set streaming only mode (V2)
#[tauri::command]
pub fn v2_set_audio_streaming_only(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_streaming_only: {}", enabled);
    let guard = state
        .store
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    let store = guard
        .as_ref()
        .ok_or(RuntimeError::UserSessionNotActivated)?;
    store
        .set_streaming_only(enabled)
        .map_err(RuntimeError::Internal)
}

/// Reset audio settings to defaults (V2)
#[tauri::command]
pub async fn v2_reset_audio_settings(
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] reset_audio_settings");
    {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .reset_all()
            .map(|_| ())
            .map_err(RuntimeError::Internal)?;
    }
    sync_audio_settings_to_player(&state, &bridge).await;
    Ok(())
}

/// Set stream first track enabled (V2)
#[tauri::command]
pub fn v2_set_audio_stream_first_track(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_stream_first_track: {}", enabled);
    let guard = state
        .store
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    let store = guard
        .as_ref()
        .ok_or(RuntimeError::UserSessionNotActivated)?;
    store
        .set_stream_first_track(enabled)
        .map_err(RuntimeError::Internal)
}

/// Set stream buffer seconds (V2)
#[tauri::command]
pub fn v2_set_audio_stream_buffer_seconds(
    seconds: u8,
    state: State<'_, AudioSettingsState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_stream_buffer_seconds: {}", seconds);
    let guard = state
        .store
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    let store = guard
        .as_ref()
        .ok_or(RuntimeError::UserSessionNotActivated)?;
    store
        .set_stream_buffer_seconds(seconds)
        .map_err(RuntimeError::Internal)
}

/// Set ALSA hardware volume control (V2)
#[tauri::command]
pub async fn v2_set_audio_alsa_hardware_volume(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
    bridge: State<'_, CoreBridgeState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_audio_alsa_hardware_volume: {}", enabled);
    {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_alsa_hardware_volume(enabled)
            .map_err(RuntimeError::Internal)?;
    }
    sync_audio_settings_to_player(&state, &bridge).await;
    Ok(())
}

// ==================== DAC Reservation (Lifetime B) ====================
//
// See `qbz-nix-docs/specs/2026-05-07-alsa-exclusive-hardening-design.md`
// for the protocol-level design. Lifetime B = a long-lived
// `DeviceReservation` guard held in `AppState` for the QBZ process,
// gated by the `reserve_dac_while_running` audio setting.

/// Status payload returned by `v2_get_dac_reservation_status`.
///
/// `Inactive` covers the toggle-off and non-card-device cases. `Active`
/// means we currently hold the bus name. `Contested` means the toggle is
/// on but acquisition failed because another app holds the device at
/// equal-or-higher priority. `Unavailable` means the runtime environment
/// (no D-Bus session, sandbox without bus access) prevents any reservation
/// from working — surfaces as a degraded guard.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DacReservationStatus {
    Inactive,
    Active,
    /// Reservation could not be acquired and the guard is currently None.
    ///
    /// **Known limitation (2026-05-07):** this variant is reported whenever
    /// `apply_dac_reservation` failed to set the guard, regardless of why.
    /// In practice it most commonly means another app holds the DAC at
    /// higher priority (the spec's intended `Contested` semantic). It also
    /// fires for transient D-Bus protocol errors (`DbusError`) and ALSA
    /// enumeration errors (`AlsaError`) — those should ideally surface as
    /// `Unavailable`, but distinguishing them at this layer requires
    /// stashing the last `ReservationError` alongside the guard. Deferred
    /// until Tasks 7-8 confirm the UI needs the distinction.
    ///
    /// `holder` and `holder_priority` are currently always `"unknown"` and
    /// `0`. The real values come from `ReservationError::HigherPriorityHolder`
    /// and require the same stash refactor to surface here.
    Contested {
        holder: String,
        holder_priority: i32,
    },
    Unavailable {
        reason: String,
    },
}

/// Toggle the per-process DAC reservation (Lifetime B).
///
/// Persists the flag to `audio_settings` and applies it immediately:
/// `enabled=true` acquires a reservation for the current output device
/// (when it targets a single ALSA card); `enabled=false` releases the
/// guard. Idempotent — re-calling with the same value drops and
/// reacquires safely.
#[tauri::command]
pub async fn v2_set_reserve_dac_while_running(
    enabled: bool,
    state: State<'_, AudioSettingsState>,
    app_state: State<'_, crate::AppState>,
) -> Result<(), RuntimeError> {
    log::info!("[V2] set_reserve_dac_while_running: {}", enabled);

    // 1. Persist the flag in the audio_settings DB.
    let output_device = {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store
            .set_reserve_dac_while_running(enabled)
            .map_err(RuntimeError::Internal)?;
        // Re-read the current device while we hold the lock — avoids a
        // TOCTOU window where the user could change DAC between this
        // setter and our apply call.
        store
            .get_settings()
            .map_err(RuntimeError::Internal)?
            .output_device
    };

    // 2. Apply: take or release the guard. None on disable; current
    //    output device on enable.
    let device_for_apply = if enabled { output_device } else { None };
    app_state.apply_dac_reservation(device_for_apply.as_deref(), device_for_apply.as_deref());
    Ok(())
}

/// Snapshot the current Lifetime-B reservation state for the UI.
///
/// Reads both the persisted flag and the live guard so the UI can
/// distinguish between "user hasn't enabled it" (`Inactive`), "we own
/// it" (`Active`), "user enabled it but a higher-priority app holds the
/// DAC" (`Contested`), and "D-Bus is unavailable in this environment"
/// (`Unavailable`).
#[tauri::command]
pub fn v2_get_dac_reservation_status(
    state: State<'_, AudioSettingsState>,
    app_state: State<'_, crate::AppState>,
) -> Result<DacReservationStatus, RuntimeError> {
    let snapshot = {
        let guard = state
            .store
            .lock()
            .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
        let store = guard
            .as_ref()
            .ok_or(RuntimeError::UserSessionNotActivated)?;
        store.get_settings().map_err(RuntimeError::Internal)?
    };

    if !snapshot.reserve_dac_while_running {
        return Ok(DacReservationStatus::Inactive);
    }
    match snapshot.output_device.as_deref() {
        None => return Ok(DacReservationStatus::Inactive),
        Some(d) if !crate::is_card_specific_device(d) => {
            return Ok(DacReservationStatus::Inactive);
        }
        _ => {}
    }

    let guard = app_state
        .dac_reservation
        .lock()
        .map_err(|e| RuntimeError::Internal(format!("Lock error: {}", e)))?;
    Ok(match guard.as_ref() {
        Some(r) if r.is_active() => DacReservationStatus::Active,
        Some(_) => DacReservationStatus::Unavailable {
            reason: "D-Bus session bus unreachable or device not card-specific".to_string(),
        },
        None => {
            // See DacReservationStatus::Contested for the known limitation
            // about D-Bus / ALSA errors being misclassified as Contested.
            DacReservationStatus::Contested {
                holder: "unknown".to_string(),
                holder_priority: 0,
            }
        }
    })
}
