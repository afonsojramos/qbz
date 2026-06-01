//! Tauri-side QconnectEventSink implementation: receives events from
//! the qconnect-app crate (renderer state changes, queue updates, session
//! management messages) and dispatches them into our local CoreBridge,
//! sync_state cache, and Tauri event emitter.

use std::sync::{Arc, OnceLock, Weak};

use async_trait::async_trait;
use qconnect_app::{
    QconnectApp, QconnectAppEvent, QconnectEventSink, RendererReport, RendererReportType,
};
use qconnect_transport_ws::NativeWsTransport;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::core_bridge::CoreBridge;

use super::corebridge::{
    align_corebridge_queue_cursor, apply_remote_loop_mode_to_corebridge,
    apply_renderer_command_to_corebridge, materialize_remote_queue_to_corebridge,
};
use super::queue_resolution::is_valid_ordered_queue_shuffle_order;
use super::session::{
    build_session_renderer_snapshot, cache_renderer_snapshot, ensure_session_renderer_state,
    is_peer_renderer_active, normalize_active_renderer_id, refresh_local_renderer_id,
    sync_session_renderer_active_flags,
};
use super::{
    qconnect_now_ms, should_arm_renderer_watchdog, QconnectRemoteSyncState, QconnectRendererInfo,
    RendererStatus, BUFFER_STATE_OK, PLAYING_STATE_PLAYING, PLAYING_STATE_UNKNOWN,
    QCONNECT_RENDERER_LOST_TIMEOUT_MS,
};

/// Concrete `QconnectApp` type used by the Tauri adapter.
type TauriQconnectApp = QconnectApp<NativeWsTransport, TauriQconnectEventSink>;

#[derive(Clone)]
pub(super) struct TauriQconnectEventSink {
    pub(super) app_handle: AppHandle,
    pub(super) core_bridge: Arc<RwLock<Option<CoreBridge>>>,
    pub(super) sync_state: Arc<Mutex<QconnectRemoteSyncState>>,
    /// Late-bound weak reference to the owning `QconnectApp`. The app is built
    /// FROM the sink, so it can only be wired after construction (via
    /// `set_app`). Used to emit renderer reports (e.g. is_active=true after a
    /// SetActive(true) command) from inside the sink without an ownership cycle.
    pub(super) app: Arc<OnceLock<Weak<TauriQconnectApp>>>,
}

impl TauriQconnectEventSink {
    /// Wire the owning app after it has been constructed. Idempotent: a second
    /// call is ignored (OnceLock).
    pub(super) fn set_app(&self, app: &Arc<TauriQconnectApp>) {
        let _ = self.app.set(Arc::downgrade(app));
    }

    /// Emit a StateUpdated report announcing that this renderer is now active.
    /// Sent after a SetActive(true) command is applied so the controller learns
    /// the renderer is ready.
    async fn report_active_renderer_ready(&self) {
        let Some(app) = self.app.get().and_then(Weak::upgrade) else {
            return;
        };
        let queue_version = app.queue_state_snapshot().await.version;
        let report = RendererReport::new(
            RendererReportType::RndrSrvrStateUpdated,
            Uuid::new_v4().to_string(),
            queue_version,
            serde_json::json!({
                "is_active": true,
                "buffer_state": BUFFER_STATE_OK,
                "queue_version": {
                    "major": queue_version.major,
                    "minor": queue_version.minor
                }
            }),
        );
        if let Err(err) = app.send_renderer_report_command(report).await {
            log::warn!("[QConnect] Failed to report active-renderer-ready: {err}");
        }
    }
}

#[async_trait]
impl QconnectEventSink for TauriQconnectEventSink {
    async fn on_event(&self, event: QconnectAppEvent) {
        match &event {
            QconnectAppEvent::SessionManagementEvent {
                message_type,
                payload,
            } => {
                log::info!(
                    "[QConnect] Session management: {} payload={}",
                    message_type,
                    serde_json::to_string(payload).unwrap_or_else(|_| "?".to_string())
                );
                self.apply_session_management_event(message_type, payload)
                    .await;
            }
            QconnectAppEvent::RendererUpdated(renderer_state) => {
                log::info!(
                    "[QConnect] Renderer updated: playing_state={:?} volume={:?} position={:?}",
                    renderer_state.playing_state,
                    renderer_state.volume,
                    renderer_state.current_position_ms,
                );
                let mut sync_state = self.sync_state.lock().await;
                cache_renderer_snapshot(&mut sync_state, renderer_state);
            }
            QconnectAppEvent::QueueUpdated(queue_state) => {
                log::debug!(
                    "[QConnect] QueueUpdated: items={} shuffle_mode={} shuffle_order={:?} version={}.{}",
                    queue_state.queue_items.len(),
                    queue_state.shuffle_mode,
                    queue_state.shuffle_order,
                    queue_state.version.major,
                    queue_state.version.minor,
                );
                if queue_state.shuffle_mode {
                    let valid = queue_state.shuffle_order.as_ref()
                        .map(|o| is_valid_ordered_queue_shuffle_order(o, queue_state.queue_items.len()))
                        .unwrap_or(false);
                    log::debug!(
                        "[QConnect] shuffle_order valid={} items_len={} order_len={:?}",
                        valid,
                        queue_state.queue_items.len(),
                        queue_state.shuffle_order.as_ref().map(|o| o.len()),
                    );
                }
                {
                    let mut sync_state = self.sync_state.lock().await;
                    sync_state.last_remote_queue_state = Some(queue_state.clone());
                }
                if let Err(err) = materialize_remote_queue_to_corebridge(
                    &self.core_bridge,
                    &self.sync_state,
                    queue_state,
                )
                .await
                {
                    log::warn!(
                        "[QConnect] Failed to materialize remote queue in CoreBridge: {err}"
                    );
                }
            }
            QconnectAppEvent::RendererCommandApplied { command, state } => {
                log::info!("[QConnect] Renderer command applied: {:?}", command);
                let became_active = matches!(
                    command,
                    qconnect_app::RendererCommand::SetActive { active: true }
                );
                if let Err(err) = apply_renderer_command_to_corebridge(
                    &self.core_bridge,
                    &self.sync_state,
                    command,
                    state,
                )
                .await
                {
                    log::warn!("[QConnect] Failed to apply renderer command to CoreBridge: {err}");
                } else if became_active {
                    // P1-6: SetActive(true) is now genuinely supported (current
                    // track preloaded above); announce readiness to the controller.
                    self.report_active_renderer_ready().await;
                }
            }
            _ => {}
        }

        if let Err(err) = self.app_handle.emit("qconnect:event", &event) {
            log::warn!("[QConnect] Failed to emit tauri event: {err}");
        }
    }
}

impl TauriQconnectEventSink {
    async fn apply_session_management_event(&self, message_type: &str, payload: &Value) {
        let mut remote_projection_renderer_id: Option<i32> = None;
        let mut sync_local_playback = false;
        let mut apply_loop_mode: Option<i32> = None;
        let mut disconnected_renderer_id: Option<i32> = None;
        let mut watchdog_arm: Option<(i32, u64)> = None;
        let mut state = self.sync_state.lock().await;
        match message_type {
            "MESSAGE_TYPE_SRVR_CTRL_SESSION_STATE" => {
                if let Some(uuid) = payload.get("session_uuid").and_then(Value::as_str) {
                    state.session.session_uuid = Some(uuid.to_string());
                }
                state.session.active_renderer_id = normalize_active_renderer_id(
                    payload.get("active_renderer_id").and_then(Value::as_i64),
                );
                if let Some(loop_mode) = payload
                    .get("loop_mode")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                {
                    state.session_loop_mode = Some(loop_mode);
                    apply_loop_mode = Some(loop_mode);
                }
                if let (Some(active_renderer_id), Some(loop_mode)) =
                    (state.session.active_renderer_id, state.session_loop_mode)
                {
                    let renderer_state =
                        ensure_session_renderer_state(&mut state, active_renderer_id);
                    renderer_state.loop_mode = Some(loop_mode);
                    renderer_state.updated_at_ms = qconnect_now_ms();
                }
                sync_session_renderer_active_flags(&mut state);
                sync_local_playback = true;
            }
            "MESSAGE_TYPE_SRVR_CTRL_ADD_RENDERER" => {
                if let Some(renderer_id) = payload.get("renderer_id").and_then(Value::as_i64) {
                    let renderer_id = renderer_id as i32;
                    // Don't add duplicates
                    if !state
                        .session
                        .renderers
                        .iter()
                        .any(|r| r.renderer_id == renderer_id)
                    {
                        let device_info = payload.get("device_info");
                        state.session.renderers.push(QconnectRendererInfo {
                            renderer_id,
                            device_uuid: device_info
                                .and_then(|d| d.get("device_uuid"))
                                .and_then(Value::as_str)
                                .map(String::from),
                            friendly_name: device_info
                                .and_then(|d| d.get("friendly_name"))
                                .and_then(Value::as_str)
                                .map(String::from),
                            brand: device_info
                                .and_then(|d| d.get("brand"))
                                .and_then(Value::as_str)
                                .map(String::from),
                            model: device_info
                                .and_then(|d| d.get("model"))
                                .and_then(Value::as_str)
                                .map(String::from),
                            device_type: device_info
                                .and_then(|d| d.get("device_type"))
                                .and_then(Value::as_i64)
                                .map(|v| v as i32),
                        });
                        refresh_local_renderer_id(&mut state.session);
                    }
                    let _ = ensure_session_renderer_state(&mut state, renderer_id);
                    sync_session_renderer_active_flags(&mut state);
                }
            }
            "MESSAGE_TYPE_SRVR_CTRL_UPDATE_RENDERER" => {
                if let Some(renderer_id) = payload.get("renderer_id").and_then(Value::as_i64) {
                    let renderer_id = renderer_id as i32;
                    if let Some(existing) = state
                        .session
                        .renderers
                        .iter_mut()
                        .find(|r| r.renderer_id == renderer_id)
                    {
                        let device_info = payload.get("device_info");
                        if let Some(device_uuid) = device_info
                            .and_then(|d| d.get("device_uuid"))
                            .and_then(Value::as_str)
                        {
                            existing.device_uuid = Some(device_uuid.to_string());
                        }
                        if let Some(name) = device_info
                            .and_then(|d| d.get("friendly_name"))
                            .and_then(Value::as_str)
                        {
                            existing.friendly_name = Some(name.to_string());
                        }
                        if let Some(brand) = device_info
                            .and_then(|d| d.get("brand"))
                            .and_then(Value::as_str)
                        {
                            existing.brand = Some(brand.to_string());
                        }
                        if let Some(model) = device_info
                            .and_then(|d| d.get("model"))
                            .and_then(Value::as_str)
                        {
                            existing.model = Some(model.to_string());
                        }
                        if let Some(device_type) = device_info
                            .and_then(|d| d.get("device_type"))
                            .and_then(Value::as_i64)
                        {
                            existing.device_type = Some(device_type as i32);
                        }
                        refresh_local_renderer_id(&mut state.session);
                    }
                    let _ = ensure_session_renderer_state(&mut state, renderer_id);
                    sync_session_renderer_active_flags(&mut state);
                }
            }
            "MESSAGE_TYPE_SRVR_CTRL_REMOVE_RENDERER" => {
                if let Some(renderer_id) = payload.get("renderer_id").and_then(Value::as_i64) {
                    let renderer_id = renderer_id as i32;
                    state
                        .session
                        .renderers
                        .retain(|r| r.renderer_id != renderer_id);
                    state.session_renderer_states.remove(&renderer_id);
                    refresh_local_renderer_id(&mut state.session);
                    sync_session_renderer_active_flags(&mut state);
                }
            }
            "MESSAGE_TYPE_SRVR_CTRL_ACTIVE_RENDERER_CHANGED" => {
                state.session.active_renderer_id = normalize_active_renderer_id(
                    payload.get("active_renderer_id").and_then(Value::as_i64),
                );
                if let (Some(active_renderer_id), Some(loop_mode)) =
                    (state.session.active_renderer_id, state.session_loop_mode)
                {
                    let renderer_state =
                        ensure_session_renderer_state(&mut state, active_renderer_id);
                    renderer_state.loop_mode = Some(loop_mode);
                    renderer_state.updated_at_ms = qconnect_now_ms();
                }
                apply_loop_mode = state.session_loop_mode;
                sync_session_renderer_active_flags(&mut state);
                remote_projection_renderer_id = state.session.active_renderer_id;
                sync_local_playback = true;
                // P0-1: active-renderer change disarms any pending liveness task.
                Self::bump_watchdog_generation(&mut state);
            }
            "MESSAGE_TYPE_SRVR_CTRL_RENDERER_STATE_UPDATED" => {
                let Some(renderer_id) = payload.get("renderer_id").and_then(Value::as_i64) else {
                    return;
                };
                let player_state = payload.get("player_state");
                let renderer_state = ensure_session_renderer_state(&mut state, renderer_id as i32);

                if let Some(playing_state) = player_state
                    .and_then(|value| value.get("playing_state"))
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                {
                    renderer_state.playing_state = Some(playing_state);
                }

                if let Some(current_position_ms) = player_state
                    .and_then(|value| value.get("current_position"))
                    .and_then(Value::as_i64)
                    .and_then(|value| u64::try_from(value).ok())
                {
                    renderer_state.current_position_ms = Some(current_position_ms);
                }

                if let Some(current_queue_item_id) = player_state
                    .and_then(|value| value.get("current_queue_item_id"))
                    .and_then(Value::as_i64)
                {
                    renderer_state.current_queue_item_id =
                        u64::try_from(current_queue_item_id).ok();
                }

                renderer_state.updated_at_ms = qconnect_now_ms();
                // Snapshot the just-written playing_state for the watchdog decision
                // (the &mut borrow ends here so we can re-read state.session below).
                let this_playing_state = renderer_state.playing_state;
                remote_projection_renderer_id = Some(renderer_id as i32);
                sync_local_playback = true;

                let is_active_peer = state.session.active_renderer_id
                    == Some(renderer_id as i32)
                    && is_peer_renderer_active(&state.session)
                    && renderer_id != -1;

                // P0-2: graceful disconnect. status==ACTIVE_DISCONNECTED(2) for
                // the active remote renderer (id != -1) means the renderer left
                // cleanly — freeze the projection so the UI stops lying.
                let status =
                    RendererStatus::from_wire(payload.get("status").and_then(Value::as_i64));
                if status == RendererStatus::ActiveDisconnected && is_active_peer {
                    disconnected_renderer_id = Some(renderer_id as i32);
                }

                // P0-1 liveness watchdog. Any RENDERER_STATE_UPDATED for the
                // active peer resets the timer (bump generation); arm a fresh 12s
                // task only while PLAYING. A non-playing update disarms (bump
                // only, no spawn).
                let new_generation = Self::bump_watchdog_generation(&mut state);
                if should_arm_renderer_watchdog(this_playing_state, is_active_peer) {
                    watchdog_arm = Some((renderer_id as i32, new_generation));
                }
            }
            "MESSAGE_TYPE_SRVR_CTRL_VOLUME_CHANGED" => {
                let Some(renderer_id) = payload.get("renderer_id").and_then(Value::as_i64) else {
                    return;
                };
                let Some(volume) = payload
                    .get("volume")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                else {
                    return;
                };

                let renderer_state = ensure_session_renderer_state(&mut state, renderer_id as i32);
                renderer_state.volume = Some(volume);
                renderer_state.updated_at_ms = qconnect_now_ms();
            }
            "MESSAGE_TYPE_SRVR_CTRL_VOLUME_MUTED" => {
                let Some(renderer_id) = payload.get("renderer_id").and_then(Value::as_i64) else {
                    return;
                };
                let Some(muted) = payload.get("value").and_then(Value::as_bool) else {
                    return;
                };

                let renderer_state = ensure_session_renderer_state(&mut state, renderer_id as i32);
                renderer_state.muted = Some(muted);
                renderer_state.updated_at_ms = qconnect_now_ms();
            }
            "MESSAGE_TYPE_SRVR_CTRL_MAX_AUDIO_QUALITY_CHANGED" => {
                let Some(renderer_id) = payload.get("renderer_id").and_then(Value::as_i64) else {
                    return;
                };
                let Some(max_audio_quality) = payload
                    .get("max_audio_quality")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                else {
                    return;
                };

                let renderer_state = ensure_session_renderer_state(&mut state, renderer_id as i32);
                renderer_state.max_audio_quality = Some(max_audio_quality);
                renderer_state.updated_at_ms = qconnect_now_ms();
            }
            "MESSAGE_TYPE_SRVR_CTRL_LOOP_MODE_SET" => {
                let Some(loop_mode) = payload
                    .get("loop_mode")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                else {
                    return;
                };
                state.session_loop_mode = Some(loop_mode);
                apply_loop_mode = Some(loop_mode);
                if let Some(active_renderer_id) = state.session.active_renderer_id {
                    let renderer_state =
                        ensure_session_renderer_state(&mut state, active_renderer_id);
                    renderer_state.loop_mode = Some(loop_mode);
                    renderer_state.updated_at_ms = qconnect_now_ms();
                }
            }
            _ => {}
        }
        drop(state);

        if let Some(loop_mode) = apply_loop_mode {
            if let Err(err) =
                apply_remote_loop_mode_to_corebridge(&self.core_bridge, loop_mode).await
            {
                log::warn!("[QConnect] Failed to apply remote loop mode to CoreBridge: {err}");
            }
        }

        if sync_local_playback {
            self.sync_local_playback_for_renderer_ownership().await;
        }

        if let Some(renderer_id) = remote_projection_renderer_id {
            self.sync_active_renderer_projection(renderer_id).await;
        }

        if let Some(renderer_id) = disconnected_renderer_id {
            self.freeze_active_renderer_projection_to_unknown(
                renderer_id,
                "qconnect:renderer_disconnected",
            )
            .await;
        }

        // P0-1: arm the generation-guarded 12s liveness watchdog. The spawned
        // task holds a clone of this sink (TauriQconnectEventSink: Clone). On
        // wake it re-locks and no-ops unless its captured epoch is still current
        // AND the renderer is still the active peer AND still nominally playing.
        if let Some((renderer_id, generation)) = watchdog_arm {
            let sink = self.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(
                    QCONNECT_RENDERER_LOST_TIMEOUT_MS,
                ))
                .await;
                let fire = {
                    let state = sink.sync_state.lock().await;
                    state.watchdog_generation == generation
                        && state.session.active_renderer_id == Some(renderer_id)
                        && is_peer_renderer_active(&state.session)
                        && state
                            .session_renderer_states
                            .get(&renderer_id)
                            .and_then(|r| r.playing_state)
                            == Some(PLAYING_STATE_PLAYING)
                };
                if fire {
                    log::warn!(
                        "[QConnect] Renderer {renderer_id} silent for {}ms — marking unreachable",
                        QCONNECT_RENDERER_LOST_TIMEOUT_MS
                    );
                    sink.freeze_active_renderer_projection_to_unknown(
                        renderer_id,
                        "qconnect:renderer_unreachable",
                    )
                    .await;
                }
            });
        }
    }

    /// Bump the watchdog epoch, invalidating any in-flight 12s liveness task.
    /// Returns the new generation (used when arming so the spawned task knows
    /// which epoch it belongs to).
    fn bump_watchdog_generation(state: &mut QconnectRemoteSyncState) -> u64 {
        state.watchdog_generation = state.watchdog_generation.wrapping_add(1);
        state.watchdog_generation
    }

    /// Force the active renderer's cached projection to UNKNOWN/stopped and
    /// emit a freeze event. Shared by ACTIVE_DISCONNECTED (P0-2) and the
    /// silence watchdog (P0-1). Caller passes the event name to emit.
    ///
    /// This is a pure-state mutation + emit; it does NOT route back through
    /// `apply_session_management_event`, so it cannot re-arm the watchdog.
    async fn freeze_active_renderer_projection_to_unknown(
        &self,
        renderer_id: i32,
        event_name: &str,
    ) {
        {
            let mut state = self.sync_state.lock().await;
            let renderer_state = ensure_session_renderer_state(&mut state, renderer_id);
            renderer_state.playing_state = Some(PLAYING_STATE_UNKNOWN);
            renderer_state.updated_at_ms = qconnect_now_ms();
        }
        if let Err(err) = self
            .app_handle
            .emit(event_name, serde_json::json!({ "renderer_id": renderer_id }))
        {
            log::warn!("[QConnect] Failed to emit {event_name}: {err}");
        }
    }

    async fn sync_local_playback_for_renderer_ownership(&self) {
        let peer_renderer_active = {
            let state = self.sync_state.lock().await;
            is_peer_renderer_active(&state.session)
        };
        if !peer_renderer_active {
            return;
        }

        let bridge_guard = self.core_bridge.read().await;
        let Some(bridge) = bridge_guard.as_ref() else {
            return;
        };

        let playback_state = bridge.get_playback_state();
        if playback_state.track_id == 0 {
            return;
        }

        log::info!(
            "[QConnect] Stopping local playback because active renderer is a peer (track_id={})",
            playback_state.track_id
        );
        if let Err(err) = bridge.stop() {
            log::warn!("[QConnect] Failed to stop local playback after renderer handoff: {err}");
        }
    }

    async fn sync_active_renderer_projection(&self, renderer_id: i32) {
        let (queue_state, renderer_state, session_loop_mode, should_align_corebridge) = {
            let state = self.sync_state.lock().await;
            let Some(active_renderer_id) = state.session.active_renderer_id else {
                return;
            };
            if active_renderer_id != renderer_id {
                return;
            }

            (
                state.last_remote_queue_state.clone(),
                state
                    .session_renderer_states
                    .get(&active_renderer_id)
                    .cloned(),
                state.session_loop_mode,
                state.session.local_renderer_id != Some(active_renderer_id),
            )
        };

        let (Some(queue_state), Some(renderer_state)) = (queue_state, renderer_state) else {
            return;
        };

        let renderer_snapshot =
            build_session_renderer_snapshot(&queue_state, Some(&renderer_state), session_loop_mode);
        {
            let mut state = self.sync_state.lock().await;
            cache_renderer_snapshot(&mut state, &renderer_snapshot);
        }

        if !should_align_corebridge {
            return;
        }

        let bridge_guard = self.core_bridge.read().await;
        let Some(bridge) = bridge_guard.as_ref() else {
            return;
        };

        let Some(current_track) = renderer_snapshot.current_track.as_ref() else {
            return;
        };

        if let Err(err) = align_corebridge_queue_cursor(bridge, current_track.track_id).await {
            log::warn!("[QConnect] Failed to sync peer renderer cursor into CoreBridge: {err}");
        }
    }
}
