//! `QconnectServiceState` — the public service facade exposed to the
//! Tauri layer. Owns the runtime (transport + event loop), connection
//! lifecycle, command dispatch, snapshot queries, and the controller-
//! handoff helpers used by the `v2_qconnect_*` invokes. Also hosts the
//! transport-event ingestion loop and the deferred renderer-join /
//! pending-action plumbing.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use qconnect_app::{
    QConnectQueueState, QConnectRendererState, QconnectApp, QconnectAppEvent, QconnectEventSink,
    QueueCommandType, RendererReport, RendererReportType,
};
use qconnect_transport_ws::{NativeWsTransport, WsTransportConfig};
use serde_json::{json, Value};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::core_bridge::{CoreBridge, CoreBridgeState};

use super::corebridge::align_corebridge_queue_cursor;
use super::event_sink::TauriQconnectEventSink;
use super::queue_resolution::{
    resolve_controller_queue_item_from_snapshots, resolve_queue_item_ids_from_queue_state,
    QconnectRemoteSkipDirection,
};
use super::session::{
    build_effective_renderer_snapshot, build_visible_queue_projection,
    ensure_session_renderer_state, is_local_renderer_active, is_peer_renderer_active,
    renderer_allows_remote_volume, QconnectFileAudioQualitySnapshot,
};
use super::transport::{
    default_qconnect_device_info, default_qconnect_device_info_with_name, load_persisted_device_name,
};
use super::{
    qconnect_now_ms, QconnectConnectionStatus, QconnectJoinSessionRequest,
    QconnectLifecycleState, QconnectMuteVolumeRequest, QconnectQueueVersionPayload,
    QconnectRemoteSyncState, QconnectSessionState, QconnectSetPlayerStateQueueItemPayload,
    QconnectSetPlayerStateRequest, QconnectSetVolumeRequest, QconnectVisibleQueueProjection,
    AUDIO_QUALITY_HIRES_LEVEL2, BUFFER_STATE_OK, PLAYING_STATE_PAUSED, PLAYING_STATE_PLAYING,
    PLAYING_STATE_STOPPED,
};

/// `deferred_join_reason` and `should_reask_queue_state` now live in the
/// frontend-agnostic `qconnect_app::session` module; the session loop that
/// consumed them now lives in `qconnect_app::QconnectApp::run_session_loop`
/// (slice 5), so they are no longer referenced on the Tauri side.

const QCONNECT_PLAY_TRACK_HANDOFF_WAIT_MS: u64 = 1_500;
const QCONNECT_PLAY_TRACK_HANDOFF_POLL_MS: u64 = 50;

struct QconnectRuntime {
    app: Arc<QconnectApp<NativeWsTransport, TauriQconnectEventSink>>,
    config: WsTransportConfig,
    event_loop: JoinHandle<()>,
    sync_state: Arc<Mutex<QconnectRemoteSyncState>>,
}

#[derive(Default)]
struct QconnectServiceInner {
    runtime: Option<QconnectRuntime>,
    last_error: Option<String>,
    /// Lifecycle state — driven by transport events from the spawned event
    /// loop. See `QconnectLifecycleState` (issue #358).
    lifecycle_state: QconnectLifecycleState,
}

async fn update_lifecycle_state_if_running(
    inner: &Arc<Mutex<QconnectServiceInner>>,
    sink: &TauriQconnectEventSink,
    next: QconnectLifecycleState,
) {
    let mut guard = inner.lock().await;
    if guard.runtime.is_none() {
        return;
    }
    if guard.lifecycle_state == next {
        return;
    }
    guard.lifecycle_state = next;
    drop(guard);
    // Slice 3: route through the event sink. The sink's `LifecycleChanged` arm
    // emits `qconnect:status_changed` with `{state}` — byte-identical to the
    // prior raw emit here.
    sink.on_event(QconnectAppEvent::LifecycleChanged { state: next })
        .await;
}

pub(super) fn emit_qconnect_diagnostic(
    app_handle: &AppHandle,
    channel: &str,
    level: &str,
    payload: Value,
) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    if let Err(err) = app_handle.emit(
        "qconnect:diagnostic",
        json!({
            "ts": ts,
            "channel": channel,
            "level": level,
            "payload": payload,
        }),
    ) {
        log::warn!("[QConnect] Failed to emit diagnostic {channel}: {err}");
    }
}
/// Tauri-side implementation of the shared session-loop seams (slice 5). Holds
/// the adapter handles the loop reaches back into: the service runtime
/// (lifecycle gating + reconnect-exhausted teardown), the sync accumulator +
/// app_handle (renderer join + its CoreBridge duration read), and the sink
/// (lifecycle emit). `QconnectApp::run_session_loop` owns the control flow.
struct TauriSessionLoopHost {
    app: Arc<QconnectApp<NativeWsTransport, TauriQconnectEventSink>>,
    sync_state: Arc<Mutex<QconnectRemoteSyncState>>,
    inner: Arc<Mutex<QconnectServiceInner>>,
    sink: Arc<TauriQconnectEventSink>,
    app_handle: AppHandle,
}

#[async_trait::async_trait]
impl qconnect_app::SessionLoopHost for TauriSessionLoopHost {
    async fn update_lifecycle(&self, state: QconnectLifecycleState) {
        update_lifecycle_state_if_running(&self.inner, &self.sink, state).await;
    }

    async fn bootstrap_after_reconnect(&self) {
        if let Err(err) = bootstrap_remote_presence(&self.app, None).await {
            log::error!("[QConnect] Re-bootstrap after reconnect failed: {err}");
        }
    }

    async fn deferred_renderer_join(&self, session_uuid: String, reason: i32) {
        deferred_renderer_join(
            &self.app,
            &self.sync_state,
            &self.app_handle,
            &session_uuid,
            reason,
        )
        .await;
    }

    async fn on_reconnect_exhausted(
        &self,
        attempts: u32,
        last_reason: String,
        idle_retry_active: bool,
    ) -> bool {
        // Surface the Exhausted lifecycle in both modes. The difference is what
        // happens to the runtime + event loop afterwards.
        {
            let mut guard = self.inner.lock().await;
            guard.lifecycle_state = QconnectLifecycleState::Exhausted;
            guard.last_error = Some(format!(
                "Reconnect attempts exhausted ({attempts}): {last_reason}"
            ));
            if !idle_retry_active {
                // Legacy terminate path: take the runtime out so a fresh
                // user-initiated connect() succeeds. Dropping it detaches this
                // task's own JoinHandle (fine — the loop breaks right after) (#358).
                guard.runtime = None;
            }
            // gap #7 (idle_retry_active): leave the runtime in place; the
            // transport is idling and will re-arm, and the loop keeps consuming.
        }
        let _ = self.app_handle.emit(
            "qconnect:status_changed",
            json!({
                "state": "exhausted",
                "reason": "max_reconnect_attempts_exceeded",
                "attempts": attempts,
                "last_reason": last_reason,
            }),
        );
        !idle_retry_active
    }

    async fn on_loop_error(&self, message: String) {
        let _ = self.app_handle.emit("qconnect:error", &message);
    }
}

pub struct QconnectServiceState {
    inner: Arc<Mutex<QconnectServiceInner>>,
    pub(super) custom_device_name: Arc<tokio::sync::RwLock<Option<String>>>,
}

impl QconnectServiceState {
    pub fn new() -> Self {
        // Load persisted device name from disk
        let saved_name = load_persisted_device_name();
        Self {
            inner: Arc::new(Mutex::new(QconnectServiceInner::default())),
            custom_device_name: Arc::new(tokio::sync::RwLock::new(saved_name)),
        }
    }

    pub async fn connect(
        &self,
        app_handle: AppHandle,
        core_bridge: Arc<RwLock<Option<CoreBridge>>>,
        config: WsTransportConfig,
    ) -> Result<QconnectConnectionStatus, String> {
        if config.endpoint_url.trim().is_empty() {
            return Err("QConnect endpoint_url is required".to_string());
        }

        let mut guard = self.inner.lock().await;
        // Idempotent connect: if a runtime is already alive (including stuck in
        // the reconnect loop), don't error — return the current status. The UI
        // toggle reads `running` so the badge stays "on" and clicking it routes
        // to disconnect, which DOES break the loop (issue #358).
        if guard.runtime.is_some() {
            log::info!(
                "[QConnect] connect() called while runtime is alive (state={:?}); returning current status",
                guard.lifecycle_state
            );
            drop(guard);
            return Ok(self.status().await);
        }
        guard.lifecycle_state = QconnectLifecycleState::Connecting;
        guard.last_error = None;

        let transport = Arc::new(NativeWsTransport::new());
        let sync_state = Arc::new(Mutex::new(QconnectRemoteSyncState::default()));
        let sink = Arc::new(TauriQconnectEventSink {
            app_handle: app_handle.clone(),
            core_bridge,
            sync_state: Arc::clone(&sync_state),
            app: Arc::new(std::sync::OnceLock::new()),
        });
        let app = Arc::new(QconnectApp::new(
            transport,
            Arc::clone(&sink),
            Arc::clone(&sync_state),
        ));
        // P1-6: wire the owning app into the sink so it can emit reports
        // (e.g. is_active=true after SetActive(true)).
        sink.set_app(&app);

        if let Err(err) = app.connect(config.clone()).await {
            // Don't leak `lifecycle_state = Connecting` with `runtime = None`
            // back to the frontend — `isQconnectToggleOn` treats `Connecting`
            // as on, so the toggle would stick "on" with no live runtime to
            // disconnect (issue #358).
            let msg = format!("qconnect transport connect failed: {err}");
            guard.lifecycle_state = QconnectLifecycleState::Off;
            guard.last_error = Some(msg.clone());
            return Err(msg);
        }

        // Subscribe to transport events SYNCHRONOUSLY here — after app.connect()
        // returns and BEFORE the spawn / any further await — so the receiver is
        // live before the WS handshake emits Connected / Subscribed /
        // SessionEstablished / SESSION_STATE. NativeWsTransport::connect spawns
        // the WS loop and returns without awaiting the handshake, and tokio
        // broadcast has no replay, so a receiver created later inside the spawned
        // loop would race those initial events and silently drop them (a lost
        // SessionEstablished leaves the UI stuck Connecting; a lost SESSION_STATE
        // skips the deferred renderer join).
        let transport_rx = app.subscribe_transport_events();
        let idle_retry_active = config.reconnect_idle_retry_ms > 0;
        // The session/liveness/diagnostic emits route THROUGH the event sink
        // (Slice 3); the adapter-coupled seams (lifecycle gating, reconnect-
        // exhausted teardown, controller bootstrap, renderer join, raw error)
        // go through the SessionLoopHost. The Svelte wire surface stays
        // byte-identical. The shared loop lives in qconnect-app (slice 5).
        let host: Arc<dyn qconnect_app::SessionLoopHost> = Arc::new(TauriSessionLoopHost {
            app: Arc::clone(&app),
            sync_state: Arc::clone(&sync_state),
            inner: Arc::clone(&self.inner),
            sink: Arc::clone(&sink),
            app_handle: app_handle.clone(),
        });
        let app_for_loop = Arc::clone(&app);
        let event_loop = tauri::async_runtime::spawn(async move {
            app_for_loop
                .run_session_loop(host, transport_rx, idle_retry_active)
                .await;
        });

        let runtime = QconnectRuntime {
            app,
            config,
            event_loop,
            sync_state,
        };
        let runtime_app = Arc::clone(&runtime.app);
        guard.last_error = None;
        guard.runtime = Some(runtime);

        drop(guard);
        let custom_name = self.custom_device_name.read().await.clone();
        if let Err(err) = bootstrap_remote_presence(&runtime_app, custom_name).await {
            let _ = self.disconnect().await;
            let mut guard = self.inner.lock().await;
            guard.last_error = Some(format!("qconnect bootstrap failed: {err}"));
            return Err(format!("qconnect bootstrap failed: {err}"));
        }

        Ok(self.status().await)
    }

    pub async fn disconnect(&self) -> Result<QconnectConnectionStatus, String> {
        let runtime = {
            let mut guard = self.inner.lock().await;
            // Always force lifecycle to Off — the user-facing requirement is
            // that "disable QConnect" must succeed regardless of whether the
            // backend is Connecting / Reconnecting / Connected / Exhausted
            // (issue #358). The transport's shutdown_tx watch + the runtime
            // event_loop.abort below will tear down any in-flight reconnect.
            guard.lifecycle_state = QconnectLifecycleState::Off;
            guard.runtime.take()
        };

        if let Some(runtime) = runtime {
            if let Err(err) = runtime.app.disconnect().await {
                let mut guard = self.inner.lock().await;
                guard.last_error = Some(format!("qconnect disconnect failed: {err}"));
            }
            runtime.event_loop.abort();
        }

        Ok(self.status().await)
    }

    pub async fn status(&self) -> QconnectConnectionStatus {
        let (app, endpoint_url, last_error, lifecycle_state) = {
            let guard = self.inner.lock().await;
            (
                guard
                    .runtime
                    .as_ref()
                    .map(|runtime| Arc::clone(&runtime.app)),
                guard
                    .runtime
                    .as_ref()
                    .map(|runtime| runtime.config.endpoint_url.clone()),
                guard.last_error.clone(),
                guard.lifecycle_state,
            )
        };

        let transport_connected = if let Some(app) = &app {
            app.state_handle().lock().await.transport_connected
        } else {
            false
        };

        QconnectConnectionStatus {
            running: app.is_some(),
            transport_connected,
            endpoint_url,
            last_error,
            state: lifecycle_state,
        }
    }

    pub async fn send_command(
        &self,
        command_type: QueueCommandType,
        payload: Value,
    ) -> Result<String, String> {
        let app = {
            let guard = self.inner.lock().await;
            guard
                .runtime
                .as_ref()
                .map(|runtime| Arc::clone(&runtime.app))
                .ok_or_else(|| "QConnect service is not running".to_string())?
        };

        if matches!(command_type, QueueCommandType::CtrlSrvrSetPlayerState) {
            let state_handle = app.state_handle();
            let mut state = state_handle.lock().await;
            let should_clear_transport_pending = state
                .pending
                .current()
                .map(|pending| pending.is_transport_control_action)
                .unwrap_or(false);
            if should_clear_transport_pending {
                log::info!(
                    "[QConnect] Clearing superseded pending transport control before sending next SET_PLAYER_STATE"
                );
                state.pending.clear();
            }
        }

        let command = app.build_queue_command(command_type, payload).await;
        app.send_queue_command(command)
            .await
            .map_err(|err| format!("qconnect send command failed: {err}"))
    }

    pub async fn queue_snapshot(&self) -> Result<QConnectQueueState, String> {
        let app = {
            let guard = self.inner.lock().await;
            guard
                .runtime
                .as_ref()
                .map(|runtime| Arc::clone(&runtime.app))
                .ok_or_else(|| "QConnect service is not running".to_string())?
        };

        Ok(app.queue_state_snapshot().await)
    }

    pub async fn renderer_snapshot(&self) -> Result<QConnectRendererState, String> {
        if let Some((renderer_snapshot, _, _)) = self.effective_active_renderer_snapshot().await? {
            return Ok(renderer_snapshot);
        }

        let app = {
            let guard = self.inner.lock().await;
            guard
                .runtime
                .as_ref()
                .map(|runtime| Arc::clone(&runtime.app))
                .ok_or_else(|| "QConnect service is not running".to_string())?
        };

        Ok(app.renderer_state_snapshot().await)
    }

    pub async fn visible_queue_projection(&self) -> Result<QconnectVisibleQueueProjection, String> {
        if let Some((renderer, queue, _session)) = self.effective_active_renderer_snapshot().await?
        {
            return Ok(build_visible_queue_projection(&queue, &renderer));
        }

        let queue = self.queue_snapshot().await?;
        let renderer = self.renderer_snapshot().await?;
        Ok(build_visible_queue_projection(&queue, &renderer))
    }

    pub async fn session_snapshot(&self) -> Result<QconnectSessionState, String> {
        let sync_state = {
            let guard = self.inner.lock().await;
            guard
                .runtime
                .as_ref()
                .map(|runtime| Arc::clone(&runtime.sync_state))
                .ok_or_else(|| "QConnect service is not running".to_string())?
        };

        let state = sync_state.lock().await;
        Ok(state.session.clone())
    }

    pub async fn skip_next_if_remote(&self, app_handle: &AppHandle) -> Result<bool, String> {
        self.skip_remote_renderer_if_active(QconnectRemoteSkipDirection::Next, app_handle)
            .await
    }

    pub async fn skip_previous_if_remote(&self, app_handle: &AppHandle) -> Result<bool, String> {
        self.skip_remote_renderer_if_active(QconnectRemoteSkipDirection::Previous, app_handle)
            .await
    }

    pub(super) async fn effective_active_renderer_snapshot(
        &self,
    ) -> Result<
        Option<(
            QConnectRendererState,
            QConnectQueueState,
            QconnectSessionState,
        )>,
        String,
    > {
        let (app, sync_state) = {
            let guard = self.inner.lock().await;
            let Some(runtime) = guard.runtime.as_ref() else {
                return Ok(None);
            };
            (Arc::clone(&runtime.app), Arc::clone(&runtime.sync_state))
        };

        let queue = app.queue_state_snapshot().await;
        let base_renderer = app.renderer_state_snapshot().await;
        let state = sync_state.lock().await;
        let session = state.session.clone();
        let Some(active_renderer_id) = session.active_renderer_id else {
            return Ok(None);
        };

        let renderer_state = state
            .session_renderer_states
            .get(&active_renderer_id)
            .cloned();
        let renderer = build_effective_renderer_snapshot(
            &queue,
            &base_renderer,
            renderer_state.as_ref(),
            state.session_loop_mode,
        );

        Ok(Some((renderer, queue, session)))
    }

    pub(super) async fn effective_remote_renderer_snapshot(
        &self,
    ) -> Result<
        Option<(
            QConnectRendererState,
            QConnectQueueState,
            QconnectSessionState,
        )>,
        String,
    > {
        let Some((renderer, queue, session)) = self.effective_active_renderer_snapshot().await?
        else {
            return Ok(None);
        };

        if !is_peer_renderer_active(&session) {
            return Ok(None);
        }

        Ok(Some((renderer, queue, session)))
    }

    pub(super) async fn prime_remote_renderer_state(
        &self,
        queue_item_id: u64,
        playing_state: Option<i32>,
        current_position_ms: Option<u64>,
    ) {
        let guard = self.inner.lock().await;
        let Some(runtime) = guard.runtime.as_ref() else {
            return;
        };

        let mut sync_state = runtime.sync_state.lock().await;
        let Some(active_renderer_id) = sync_state.session.active_renderer_id else {
            return;
        };
        if sync_state.session.local_renderer_id == Some(active_renderer_id) {
            return;
        }

        let renderer_state = ensure_session_renderer_state(&mut sync_state, active_renderer_id);
        renderer_state.current_queue_item_id = Some(queue_item_id);
        if let Some(playing_state) = playing_state {
            renderer_state.playing_state = Some(playing_state);
        }
        if let Some(current_position_ms) = current_position_ms {
            renderer_state.current_position_ms = Some(current_position_ms);
        }
        renderer_state.updated_at_ms = qconnect_now_ms();
    }

    pub(super) async fn prime_remote_renderer_playing_state(&self, playing_state: i32) {
        let guard = self.inner.lock().await;
        let Some(runtime) = guard.runtime.as_ref() else {
            return;
        };

        let mut sync_state = runtime.sync_state.lock().await;
        let Some(active_renderer_id) = sync_state.session.active_renderer_id else {
            return;
        };
        if sync_state.session.local_renderer_id == Some(active_renderer_id) {
            return;
        }

        let renderer_state = ensure_session_renderer_state(&mut sync_state, active_renderer_id);
        renderer_state.playing_state = Some(playing_state);
        renderer_state.updated_at_ms = qconnect_now_ms();
    }

    pub async fn send_renderer_report(&self, report: RendererReport) -> Result<(), String> {
        let app = {
            let guard = self.inner.lock().await;
            guard
                .runtime
                .as_ref()
                .map(|runtime| Arc::clone(&runtime.app))
                .ok_or_else(|| "QConnect service is not running".to_string())?
        };

        app.send_renderer_report_command(report)
            .await
            .map_err(|err| format!("send renderer report failed: {err}"))
    }

    pub(super) async fn report_file_audio_quality_if_changed(
        &self,
        queue_version: qconnect_app::QueueVersion,
        audio_quality: QconnectFileAudioQualitySnapshot,
    ) -> Result<bool, String> {
        let (app, sync_state) = {
            let guard = self.inner.lock().await;
            let Some(runtime) = guard.runtime.as_ref() else {
                return Err("QConnect service is not running".to_string());
            };
            (Arc::clone(&runtime.app), Arc::clone(&runtime.sync_state))
        };

        {
            let state = sync_state.lock().await;
            if state.last_reported_file_audio_quality == Some(audio_quality) {
                return Ok(false);
            }
        }

        let report = RendererReport::new(
            RendererReportType::RndrSrvrFileAudioQualityChanged,
            Uuid::new_v4().to_string(),
            queue_version,
            serde_json::json!({
                "sampling_rate": audio_quality.sampling_rate,
                "bit_depth": audio_quality.bit_depth,
                "nb_channels": audio_quality.nb_channels,
                "audio_quality": audio_quality.audio_quality
            }),
        );

        app.send_renderer_report_command(report)
            .await
            .map_err(|err| format!("send file audio quality report failed: {err}"))?;

        let mut state = sync_state.lock().await;
        state.last_reported_file_audio_quality = Some(audio_quality);
        Ok(true)
    }

    /// Emit a RndrSrvrDeviceAudioQualityChanged(27) report describing the actual
    /// DAC output format (sampling_rate / bit_depth / nb_channels), deduped against
    /// the last reported value. Returns Ok(true) when a report was sent.
    pub(super) async fn report_device_audio_quality_if_changed(
        &self,
        queue_version: qconnect_app::QueueVersion,
        sampling_rate: i32,
        bit_depth: i32,
        nb_channels: i32,
    ) -> Result<bool, String> {
        let (app, sync_state) = {
            let guard = self.inner.lock().await;
            let Some(runtime) = guard.runtime.as_ref() else {
                return Err("QConnect service is not running".to_string());
            };
            (Arc::clone(&runtime.app), Arc::clone(&runtime.sync_state))
        };

        let key = (sampling_rate, bit_depth, nb_channels);
        {
            let state = sync_state.lock().await;
            if state.last_reported_device_audio_quality == Some(key) {
                return Ok(false);
            }
        }

        let report = RendererReport::new(
            RendererReportType::RndrSrvrDeviceAudioQualityChanged,
            Uuid::new_v4().to_string(),
            queue_version,
            serde_json::json!({
                "sampling_rate": sampling_rate,
                "bit_depth": bit_depth,
                "nb_channels": nb_channels
            }),
        );

        app.send_renderer_report_command(report)
            .await
            .map_err(|err| format!("send device audio quality report failed: {err}"))?;

        let mut state = sync_state.lock().await;
        state.last_reported_device_audio_quality = Some(key);
        Ok(true)
    }

    /// Update the renderer's internal position from the frontend's actual playback position.
    /// This keeps the QConnect app's renderer state in sync with audio playback so that
    /// subsequent renderer reports (e.g. after pause/resume) include the correct position.
    pub async fn update_renderer_position(&self, position_ms: u64) {
        let guard = self.inner.lock().await;
        if let Some(runtime) = &guard.runtime {
            runtime.app.update_renderer_position(position_ms).await;
        }
    }

    pub async fn is_active(&self) -> bool {
        let guard = self.inner.lock().await;
        guard.runtime.is_some()
    }

    pub(super) async fn get_queue_version(&self) -> qconnect_app::QueueVersion {
        let guard = self.inner.lock().await;
        if let Some(runtime) = &guard.runtime {
            runtime.app.queue_state_snapshot().await.version
        } else {
            qconnect_app::QueueVersion::default()
        }
    }

    /// Get current and next queue_item_ids from the renderer state.
    /// Used to auto-fill state reports when frontend doesn't know queue_item_ids.
    pub(super) async fn get_renderer_queue_item_ids(&self) -> (Option<u64>, Option<u64>) {
        let guard = self.inner.lock().await;
        if let Some(runtime) = &guard.runtime {
            let sync_state = runtime.sync_state.lock().await;
            (
                sync_state.last_renderer_queue_item_id,
                sync_state.last_renderer_next_queue_item_id,
            )
        } else {
            (None, None)
        }
    }

    pub(super) async fn get_renderer_track_ids(&self) -> (Option<u64>, Option<u64>) {
        let guard = self.inner.lock().await;
        if let Some(runtime) = &guard.runtime {
            let sync_state = runtime.sync_state.lock().await;
            (
                sync_state.last_renderer_track_id,
                sync_state.last_renderer_next_track_id,
            )
        } else {
            (None, None)
        }
    }

    pub(super) async fn is_local_renderer_active(&self) -> bool {
        let guard = self.inner.lock().await;
        if let Some(runtime) = &guard.runtime {
            let sync_state = runtime.sync_state.lock().await;
            is_local_renderer_active(&sync_state.session)
        } else {
            false
        }
    }

    /// Resolve the current and next queue_item_ids from the QConnect queue state.
    /// Searches queue_items first, then autoplay_items, and caches the result in sync_state.
    pub(super) async fn resolve_queue_item_ids_by_track_id(
        &self,
        track_id: u64,
    ) -> (Option<u64>, Option<u64>) {
        let (app, sync_state) = {
            let guard = self.inner.lock().await;
            let Some(runtime) = guard.runtime.as_ref() else {
                return (None, None);
            };
            (Arc::clone(&runtime.app), Arc::clone(&runtime.sync_state))
        };

        let queue = app.queue_state_snapshot().await;
        let (current_qid, next_qid, next_track_id) =
            resolve_queue_item_ids_from_queue_state(&queue, track_id);

        if let Some(current_qid) = current_qid {
            let mut state = sync_state.lock().await;
            state.last_renderer_queue_item_id = Some(current_qid);
            state.last_renderer_next_queue_item_id = next_qid;
            state.last_renderer_track_id = Some(track_id);
            state.last_renderer_next_track_id = next_track_id;
            log::debug!(
                "[QConnect] Resolved queue_item_ids current={:?} next={:?} for track_id={} from queue state",
                current_qid,
                next_qid,
                track_id
            );
            (Some(current_qid), next_qid)
        } else {
            log::debug!(
                "[QConnect] Could not find track_id={} in queue state ({} queue_items, {} autoplay_items)",
                track_id,
                queue.queue_items.len(),
                queue.autoplay_items.len()
            );
            (None, None)
        }
    }

    pub(super) async fn skip_remote_renderer_if_active(
        &self,
        direction: QconnectRemoteSkipDirection,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let remote_context = self.effective_remote_renderer_snapshot().await?;
        let Some((renderer, queue, session)) = remote_context else {
            let (active_renderer_id, local_renderer_id, renderer_count, reason) = {
                let guard = self.inner.lock().await;
                let Some(runtime) = guard.runtime.as_ref() else {
                    return Ok(false);
                };
                let session = runtime.sync_state.lock().await.session.clone();
                let reason = if session.active_renderer_id.is_none() {
                    "missing_active_renderer_id"
                } else if session.local_renderer_id.is_none() {
                    "missing_local_renderer_id"
                } else {
                    "active_renderer_is_local"
                };
                (
                    session.active_renderer_id,
                    session.local_renderer_id,
                    session.renderers.len(),
                    reason,
                )
            };

            emit_qconnect_diagnostic(
                app_handle,
                "qconnect:controller_skip_handoff",
                "info",
                json!({
                    "direction": match direction {
                        QconnectRemoteSkipDirection::Next => "next",
                        QconnectRemoteSkipDirection::Previous => "previous",
                    },
                    "reason": reason,
                    "active_renderer_id": active_renderer_id,
                    "local_renderer_id": local_renderer_id,
                    "renderer_count": renderer_count,
                }),
            );
            return Ok(false);
        };

        let active_renderer_id = session.active_renderer_id;
        let local_renderer_id = session.local_renderer_id;
        let resolution = resolve_controller_queue_item_from_snapshots(&queue, &renderer, direction);

        let diagnostic_payload = json!({
            "direction": match direction {
                QconnectRemoteSkipDirection::Next => "next",
                QconnectRemoteSkipDirection::Previous => "previous",
            },
            "active_renderer_id": active_renderer_id,
            "local_renderer_id": local_renderer_id,
            "queue_version": {
                "major": queue.version.major,
                "minor": queue.version.minor,
            },
            "current_position_ms": renderer.current_position_ms,
            "playing_state": renderer.playing_state,
            "target_queue_item_id": resolution.target_queue_item_id,
            "strategy": resolution.strategy,
            "queue_index": resolution.queue_index,
            "matched_track_id": resolution.matched_track_id,
            "matched_queue_item_id": resolution.matched_queue_item_id,
        });

        let Some(target_queue_item_id) = resolution.target_queue_item_id else {
            emit_qconnect_diagnostic(
                app_handle,
                "qconnect:controller_skip_handoff",
                "warn",
                diagnostic_payload,
            );
            return Err(format!(
                "remote renderer active but no {} target queue item could be resolved",
                match direction {
                    QconnectRemoteSkipDirection::Next => "next",
                    QconnectRemoteSkipDirection::Previous => "previous",
                }
            ));
        };

        let target_queue_item_id_i32 = i32::try_from(target_queue_item_id)
            .map_err(|_| format!("target queue item id out of range: {target_queue_item_id}"))?;
        let payload = serde_json::to_value(QconnectSetPlayerStateRequest {
            playing_state: renderer.playing_state,
            current_position: Some(0),
            current_queue_item: Some(QconnectSetPlayerStateQueueItemPayload {
                queue_version: Some(QconnectQueueVersionPayload {
                    major: queue.version.major,
                    minor: queue.version.minor,
                }),
                id: Some(target_queue_item_id_i32),
            }),
        })
        .map_err(|err| format!("serialize controller skip payload: {err}"))?;

        self.send_command(QueueCommandType::CtrlSrvrSetPlayerState, payload)
            .await?;
        self.prime_remote_renderer_state(target_queue_item_id, renderer.playing_state, Some(0))
            .await;
        if let Some(target_track_id) = resolution.matched_track_id {
            if let Some(bridge_state) = app_handle.try_state::<CoreBridgeState>() {
                if let Some(bridge) = bridge_state.try_get().await {
                    if let Err(err) = align_corebridge_queue_cursor(&bridge, target_track_id).await
                    {
                        log::warn!(
                            "[QConnect] Failed to align CoreBridge after remote skip handoff: {err}"
                        );
                    }
                }
            }
        }

        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:controller_skip_handoff",
            "info",
            diagnostic_payload,
        );

        Ok(true)
    }

    pub(super) async fn toggle_remote_renderer_playback_if_active(
        &self,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let remote_context = self.effective_remote_renderer_snapshot().await?;
        let Some((renderer, queue, session)) = remote_context else {
            let (active_renderer_id, local_renderer_id, renderer_count, reason) = {
                let guard = self.inner.lock().await;
                let Some(runtime) = guard.runtime.as_ref() else {
                    return Ok(false);
                };
                let session = runtime.sync_state.lock().await.session.clone();
                let reason = if session.active_renderer_id.is_none() {
                    "missing_active_renderer_id"
                } else if session.local_renderer_id.is_none() {
                    "missing_local_renderer_id"
                } else {
                    "active_renderer_is_local"
                };
                (
                    session.active_renderer_id,
                    session.local_renderer_id,
                    session.renderers.len(),
                    reason,
                )
            };

            emit_qconnect_diagnostic(
                app_handle,
                "qconnect:toggle_play_handoff",
                "info",
                json!({
                    "reason": reason,
                    "active_renderer_id": active_renderer_id,
                    "local_renderer_id": local_renderer_id,
                    "renderer_count": renderer_count,
                }),
            );
            return Ok(false);
        };

        let active_renderer_id = session.active_renderer_id;
        let local_renderer_id = session.local_renderer_id;
        let next_playing_state = match renderer.playing_state {
            Some(PLAYING_STATE_PLAYING) => PLAYING_STATE_PAUSED,
            _ => PLAYING_STATE_PLAYING,
        };
        let current_position = renderer
            .current_position_ms
            .and_then(|value| i32::try_from(value).ok());
        let current_queue_item = renderer.current_track.as_ref().and_then(|item| {
            i32::try_from(item.queue_item_id).ok().map(|queue_item_id| {
                QconnectSetPlayerStateQueueItemPayload {
                    queue_version: Some(QconnectQueueVersionPayload {
                        major: queue.version.major,
                        minor: queue.version.minor,
                    }),
                    id: Some(queue_item_id),
                }
            })
        });

        let payload = serde_json::to_value(QconnectSetPlayerStateRequest {
            playing_state: Some(next_playing_state),
            current_position,
            current_queue_item,
        })
        .map_err(|err| format!("serialize toggle_play request: {err}"))?;

        self.send_command(QueueCommandType::CtrlSrvrSetPlayerState, payload)
            .await?;
        self.prime_remote_renderer_playing_state(next_playing_state)
            .await;

        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:toggle_play_handoff",
            "info",
            json!({
                "active_renderer_id": active_renderer_id,
                "local_renderer_id": local_renderer_id,
                "current_playing_state": renderer.playing_state,
                "requested_playing_state": next_playing_state,
                "current_position": current_position,
                "current_queue_item_id": renderer.current_track.as_ref().map(|item| item.queue_item_id),
            }),
        );

        Ok(true)
    }

    pub(super) async fn play_remote_renderer_track_if_active(
        &self,
        track_id: u64,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let (app, session, sync_state) = {
            let guard = self.inner.lock().await;
            let Some(runtime) = guard.runtime.as_ref() else {
                return Ok(false);
            };
            let session = runtime.sync_state.lock().await.session.clone();
            (
                Arc::clone(&runtime.app),
                session,
                Arc::clone(&runtime.sync_state),
            )
        };

        let active_renderer_id = session.active_renderer_id;
        let local_renderer_id = session.local_renderer_id;
        let early_return_reason = if active_renderer_id.is_none() {
            Some("missing_active_renderer_id")
        } else if local_renderer_id.is_none() {
            Some("missing_local_renderer_id")
        } else if active_renderer_id == local_renderer_id {
            Some("active_renderer_is_local")
        } else {
            None
        };

        if let Some(reason) = early_return_reason {
            // Mark this track as a recent load attempt when the play is
            // about to happen locally — this prevents the cloud's echo
            // SetState (arriving ~1-2s later) from re-triggering a
            // redundant load while the V2 path is still buffering.
            if reason == "active_renderer_is_local" {
                let mut state = sync_state.lock().await;
                state.last_load_attempt = Some((track_id, std::time::Instant::now()));
            }
            emit_qconnect_diagnostic(
                app_handle,
                "qconnect:play_track_handoff",
                "info",
                json!({
                    "reason": reason,
                    "track_id": track_id,
                    "active_renderer_id": active_renderer_id,
                    "local_renderer_id": local_renderer_id,
                    "renderer_count": session.renderers.len(),
                }),
            );
            return Ok(false);
        }

        let deadline = tokio::time::Instant::now()
            + Duration::from_millis(QCONNECT_PLAY_TRACK_HANDOFF_WAIT_MS);
        let poll_interval = Duration::from_millis(QCONNECT_PLAY_TRACK_HANDOFF_POLL_MS);
        let mut attempts: u32 = 0;
        let (last_queue_version, last_queue_track_count) = loop {
            attempts += 1;
            let queue = app.queue_state_snapshot().await;
            let queue_version = (queue.version.major, queue.version.minor);
            let queue_track_count = queue.queue_items.len() + queue.autoplay_items.len();

            let (resolved_queue_item_id, _, _) =
                resolve_queue_item_ids_from_queue_state(&queue, track_id);

            if let Some(target_queue_item_id) = resolved_queue_item_id {
                let target_queue_item_id_i32 =
                    i32::try_from(target_queue_item_id).map_err(|_| {
                        format!("target queue item id out of range: {target_queue_item_id}")
                    })?;

                let payload = serde_json::to_value(QconnectSetPlayerStateRequest {
                    playing_state: Some(PLAYING_STATE_PLAYING),
                    current_position: Some(0),
                    current_queue_item: Some(QconnectSetPlayerStateQueueItemPayload {
                        queue_version: Some(QconnectQueueVersionPayload {
                            major: queue.version.major,
                            minor: queue.version.minor,
                        }),
                        id: Some(target_queue_item_id_i32),
                    }),
                })
                .map_err(|err| format!("serialize play_track handoff payload: {err}"))?;

                self.send_command(QueueCommandType::CtrlSrvrSetPlayerState, payload)
                    .await?;
                self.prime_remote_renderer_state(
                    target_queue_item_id,
                    Some(PLAYING_STATE_PLAYING),
                    Some(0),
                )
                .await;
                if let Some(bridge_state) = app_handle.try_state::<CoreBridgeState>() {
                    if let Some(bridge) = bridge_state.try_get().await {
                        if let Err(err) = align_corebridge_queue_cursor(&bridge, track_id).await {
                            log::warn!(
                                "[QConnect] Failed to align CoreBridge after remote play-track handoff: {err}"
                            );
                        }
                    }
                }

                emit_qconnect_diagnostic(
                    app_handle,
                    "qconnect:play_track_handoff",
                    "info",
                    json!({
                        "track_id": track_id,
                        "active_renderer_id": active_renderer_id,
                        "local_renderer_id": local_renderer_id,
                        "target_queue_item_id": target_queue_item_id,
                        "queue_version": {
                            "major": queue.version.major,
                            "minor": queue.version.minor,
                        },
                        "queue_track_count": queue_track_count,
                        "attempts": attempts,
                        "waited_ms": (attempts.saturating_sub(1) as u64) * QCONNECT_PLAY_TRACK_HANDOFF_POLL_MS,
                    }),
                );

                return Ok(true);
            }

            if tokio::time::Instant::now() >= deadline {
                break (Some(queue_version), queue_track_count);
            }

            tokio::time::sleep(poll_interval).await;
        };

        let renderer = self
            .effective_remote_renderer_snapshot()
            .await?
            .map(|(renderer, _, _)| renderer)
            .unwrap_or_else(QConnectRendererState::default);
        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:play_track_handoff",
            "warn",
            json!({
                "reason": "track_not_present_in_remote_queue",
                "track_id": track_id,
                "active_renderer_id": active_renderer_id,
                "local_renderer_id": local_renderer_id,
                "attempts": attempts,
                "wait_timeout_ms": QCONNECT_PLAY_TRACK_HANDOFF_WAIT_MS,
                "last_queue_version": last_queue_version.map(|(major, minor)| json!({
                    "major": major,
                    "minor": minor,
                })),
                "queue_track_count": last_queue_track_count,
                "renderer_current_track_id": renderer.current_track.as_ref().map(|item| item.track_id),
                "renderer_current_queue_item_id": renderer.current_track.as_ref().map(|item| item.queue_item_id),
                "renderer_next_track_id": renderer.next_track.as_ref().map(|item| item.track_id),
                "renderer_next_queue_item_id": renderer.next_track.as_ref().map(|item| item.queue_item_id),
            }),
        );

        Err(format!(
            "remote renderer active but track {track_id} was not present in qconnect queue after {}ms",
            QCONNECT_PLAY_TRACK_HANDOFF_WAIT_MS
        ))
    }

    pub(super) async fn toggle_shuffle_if_remote(
        &self,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let remote_context = self.effective_remote_renderer_snapshot().await?;
        let Some((renderer, _queue, session)) = remote_context else {
            return Ok(false);
        };

        let current_shuffle = renderer.shuffle_mode.unwrap_or(false);
        let next_shuffle = !current_shuffle;

        let payload = json!({ "shuffle_mode": next_shuffle });
        self.send_command(QueueCommandType::CtrlSrvrSetShuffleMode, payload)
            .await?;

        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:toggle_shuffle_handoff",
            "info",
            json!({
                "active_renderer_id": session.active_renderer_id,
                "local_renderer_id": session.local_renderer_id,
                "current_shuffle_mode": current_shuffle,
                "requested_shuffle_mode": next_shuffle,
            }),
        );

        Ok(true)
    }

    pub(super) async fn cycle_repeat_if_remote(
        &self,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let remote_context = self.effective_remote_renderer_snapshot().await?;
        let Some((renderer, _queue, session)) = remote_context else {
            return Ok(false);
        };

        // QConnect loop mode: 1 = off, 3 = repeat all, 2 = repeat one
        // Cycle: off(1) → all(3) → one(2) → off(1)
        let current_loop = renderer.loop_mode.unwrap_or(1);
        let next_loop = match current_loop {
            0 | 1 => 3, // off → all
            3 => 2,     // all → one
            _ => 1,     // one → off
        };

        let payload = json!({ "loop_mode": next_loop });
        self.send_command(QueueCommandType::CtrlSrvrSetLoopMode, payload)
            .await?;

        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:cycle_repeat_handoff",
            "info",
            json!({
                "active_renderer_id": session.active_renderer_id,
                "local_renderer_id": session.local_renderer_id,
                "current_loop_mode": current_loop,
                "requested_loop_mode": next_loop,
            }),
        );

        Ok(true)
    }

    pub(super) async fn set_volume_if_remote(
        &self,
        volume: i32,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let remote_context = self.effective_remote_renderer_snapshot().await?;
        let Some((_renderer, _queue, session)) = remote_context else {
            return Ok(false);
        };

        // P1-5: respect the active renderer's volume_remote_control capability.
        // Absent => allowed. Only an explicit non-ALLOWED value disables.
        if let Some(active_id) = session.active_renderer_id {
            if let Some(info) = session
                .renderers
                .iter()
                .find(|r| r.renderer_id == active_id)
            {
                if !renderer_allows_remote_volume(info) {
                    log::info!(
                        "[QConnect] set_volume_if_remote short-circuited: renderer {active_id} disallows remote volume"
                    );
                    // Handled (no-op): the frontend must NOT fall back to local.
                    return Ok(true);
                }
            }
        }

        let payload = serde_json::to_value(QconnectSetVolumeRequest {
            renderer_id: session.active_renderer_id,
            volume: Some(volume),
            volume_delta: None,
        })
        .map_err(|err| format!("serialize set_volume request: {err}"))?;

        self.send_command(QueueCommandType::CtrlSrvrSetVolume, payload)
            .await?;

        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:set_volume_handoff",
            "info",
            json!({
                "active_renderer_id": session.active_renderer_id,
                "local_renderer_id": session.local_renderer_id,
                "volume": volume,
            }),
        );

        Ok(true)
    }

    pub(super) async fn mute_if_remote(
        &self,
        value: bool,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let remote_context = self.effective_remote_renderer_snapshot().await?;
        let Some((_renderer, _queue, session)) = remote_context else {
            return Ok(false);
        };

        let payload = serde_json::to_value(QconnectMuteVolumeRequest {
            renderer_id: session.active_renderer_id,
            value,
        })
        .map_err(|err| format!("serialize mute_volume request: {err}"))?;

        self.send_command(QueueCommandType::CtrlSrvrMuteVolume, payload)
            .await?;

        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:mute_handoff",
            "info",
            json!({
                "active_renderer_id": session.active_renderer_id,
                "local_renderer_id": session.local_renderer_id,
                "mute": value,
            }),
        );

        Ok(true)
    }

    pub(super) async fn set_autoplay_mode_if_remote(
        &self,
        enabled: bool,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let remote_context = self.effective_remote_renderer_snapshot().await?;
        let Some((_renderer, _queue, session)) = remote_context else {
            return Ok(false);
        };

        let payload = json!({
            "autoplay_mode": enabled,
            "autoplay_reset": true,
            "autoplay_loading": false
        });
        self.send_command(QueueCommandType::CtrlSrvrSetAutoplayMode, payload)
            .await?;

        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:set_autoplay_mode_handoff",
            "info",
            json!({
                "active_renderer_id": session.active_renderer_id,
                "local_renderer_id": session.local_renderer_id,
                "autoplay_mode": enabled,
            }),
        );

        Ok(true)
    }

    pub(super) async fn autoplay_load_tracks_if_remote(
        &self,
        track_ids: Vec<u32>,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let remote_context = self.effective_remote_renderer_snapshot().await?;
        let Some((_renderer, _queue, session)) = remote_context else {
            return Ok(false);
        };

        if track_ids.is_empty() {
            return Ok(true); // nothing to load, but handled remotely
        }

        let payload = json!({
            "track_ids": track_ids,
            "context_uuid": uuid::Uuid::new_v4().to_string()
        });
        self.send_command(QueueCommandType::CtrlSrvrAutoplayLoadTracks, payload)
            .await?;

        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:autoplay_load_tracks_handoff",
            "info",
            json!({
                "active_renderer_id": session.active_renderer_id,
                "local_renderer_id": session.local_renderer_id,
                "track_count": track_ids.len(),
            }),
        );

        Ok(true)
    }

    pub(super) async fn stop_if_remote(
        &self,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let remote_context = self.effective_remote_renderer_snapshot().await?;
        let Some((renderer, queue, session)) = remote_context else {
            return Ok(false);
        };

        let current_position = renderer
            .current_position_ms
            .and_then(|value| i32::try_from(value).ok());
        let current_queue_item = renderer.current_track.as_ref().and_then(|item| {
            i32::try_from(item.queue_item_id).ok().map(|queue_item_id| {
                QconnectSetPlayerStateQueueItemPayload {
                    queue_version: Some(QconnectQueueVersionPayload {
                        major: queue.version.major,
                        minor: queue.version.minor,
                    }),
                    id: Some(queue_item_id),
                }
            })
        });

        let payload = serde_json::to_value(QconnectSetPlayerStateRequest {
            playing_state: Some(PLAYING_STATE_STOPPED),
            current_position,
            current_queue_item,
        })
        .map_err(|err| format!("serialize stop request: {err}"))?;

        self.send_command(QueueCommandType::CtrlSrvrSetPlayerState, payload)
            .await?;
        self.prime_remote_renderer_playing_state(PLAYING_STATE_STOPPED)
            .await;

        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:stop_handoff",
            "info",
            json!({
                "active_renderer_id": session.active_renderer_id,
                "local_renderer_id": session.local_renderer_id,
                "current_position": current_position,
            }),
        );

        Ok(true)
    }

    pub(super) async fn set_position_if_remote(
        &self,
        position_ms: i64,
        app_handle: &AppHandle,
    ) -> Result<bool, String> {
        let remote_context = self.effective_remote_renderer_snapshot().await?;
        let Some((renderer, queue, session)) = remote_context else {
            return Ok(false);
        };

        let current_queue_item_id = renderer
            .current_track
            .as_ref()
            .map(|item| item.queue_item_id);

        let request = super::commands::build_set_position_player_state_request(
            position_ms,
            current_queue_item_id,
            QconnectQueueVersionPayload {
                major: queue.version.major,
                minor: queue.version.minor,
            },
        );
        let payload = serde_json::to_value(request)
            .map_err(|err| format!("serialize set_position request: {err}"))?;

        self.send_command(QueueCommandType::CtrlSrvrSetPlayerState, payload)
            .await?;

        if position_ms >= 0 {
            self.update_renderer_position(position_ms as u64).await;
        }

        emit_qconnect_diagnostic(
            app_handle,
            "qconnect:set_position_handoff",
            "info",
            json!({
                "active_renderer_id": session.active_renderer_id,
                "local_renderer_id": session.local_renderer_id,
                "position_ms": position_ms,
            }),
        );

        Ok(true)
    }
}


impl Default for QconnectServiceState {
    fn default() -> Self {
        Self::new()
    }
}
async fn bootstrap_remote_presence(
    app: &Arc<QconnectApp<NativeWsTransport, TauriQconnectEventSink>>,
    custom_device_name: Option<String>,
) -> Result<(), String> {
    let device_info = default_qconnect_device_info_with_name(custom_device_name.as_deref());

    // 1. Controller JoinSession first (works without session_uuid).
    //    The server will respond with session topology (AddRenderer, QueueState, etc.).
    let join_payload = serde_json::to_value(QconnectJoinSessionRequest {
        session_uuid: None,
        device_info: Some(device_info),
    })
    .map_err(|err| format!("serialize join_session bootstrap payload: {err}"))?;

    let join_command = app
        .build_queue_command(QueueCommandType::CtrlSrvrJoinSession, join_payload)
        .await;
    let join_action_uuid = app
        .send_queue_command(join_command)
        .await
        .map_err(|err| format!("send bootstrap ctrl_srvr_join_session failed: {err}"))?;

    // JoinSession typically responds with session/renderer controller events that are not part of
    // queue reducer correlation. Drop pending slot so queue operations are not blocked for 10s.
    app.clear_pending_if_matches(&join_action_uuid).await;

    // 2. Ask for current queue state from server
    let ask_queue_payload = serde_json::json!({});
    let ask_queue_command = app
        .build_queue_command(
            QueueCommandType::CtrlSrvrAskForQueueState,
            ask_queue_payload,
        )
        .await;
    let ask_action_uuid = app
        .send_queue_command(ask_queue_command)
        .await
        .map_err(|err| format!("send bootstrap ask_for_queue_state failed: {err}"))?;
    app.clear_pending_if_matches(&ask_action_uuid).await;

    // NOTE: Renderer JoinSession requires a session_uuid from the server (type 81 SESSION_STATE).
    // It is sent as a deferred step from the event loop when SESSION_STATE arrives.
    log::info!("[QConnect] Bootstrap complete: controller joined, queue state requested. Renderer join deferred until session_uuid received.");

    Ok(())
}

/// Deferred renderer join: called from the event loop when we receive SESSION_STATE with a session_uuid.
async fn deferred_renderer_join(
    app: &Arc<QconnectApp<NativeWsTransport, TauriQconnectEventSink>>,
    sync_state: &Arc<Mutex<QconnectRemoteSyncState>>,
    app_handle: &AppHandle,
    session_uuid: &str,
    join_reason: i32,
) {
    // P1-8: make the deferred join idempotent. If we already joined this exact
    // session_uuid, skip re-sending the renderer-join reports (which would
    // re-announce us) but still re-AskForRendererState so a Lagged-dropped
    // renderer state is recovered.
    let already_joined = {
        let st = sync_state.lock().await;
        st.last_joined_session_uuid.as_deref() == Some(session_uuid)
    };
    if already_joined {
        log::info!(
            "[QConnect] Deferred join skipped (already joined session_uuid={}); re-asking renderer state",
            session_uuid
        );
        if let Err(err) = app.ask_for_active_renderer_state().await {
            log::warn!("[QConnect] Idempotent-join AskForRendererState failed: {err}");
        }
        return;
    }

    let device_info = default_qconnect_device_info();
    let queue_version_ref = app.queue_state_snapshot().await.version;

    log::info!(
        "[QConnect] Deferred renderer join with session_uuid={}",
        session_uuid
    );

    // 1. Renderer JoinSession with session_uuid
    let renderer_join_payload = serde_json::json!({
        "session_uuid": session_uuid,
        "device_info": serde_json::to_value(&device_info).unwrap_or_default(),
        "is_active": true,
        "reason": join_reason,
        "initial_state": {
            "playing_state": PLAYING_STATE_STOPPED,
            "buffer_state": BUFFER_STATE_OK,
            "current_position": 0,
            "duration": 0,
            "queue_version": {
                "major": queue_version_ref.major,
                "minor": queue_version_ref.minor
            }
        }
    });
    let renderer_join_report = RendererReport::new(
        RendererReportType::RndrSrvrJoinSession,
        Uuid::new_v4().to_string(),
        queue_version_ref,
        renderer_join_payload,
    );
    if let Err(err) = app.send_renderer_report_command(renderer_join_report).await {
        log::error!("[QConnect] Deferred renderer join failed: {err}");
        return;
    }

    // 2. Send initial StateUpdated report. At join time (e.g. reconnect
    // mid-playback) we may already have a current track, so resolve the real
    // duration + current/next queue_item_ids instead of hardcoding nulls.
    let renderer = app.renderer_state_snapshot().await;
    let queue = app.queue_state_snapshot().await;
    let current_track_id = renderer.current_track.as_ref().map(|item| item.track_id);
    let (current_qid, next_qid, _) = current_track_id
        .map(|tid| resolve_queue_item_ids_from_queue_state(&queue, tid))
        .unwrap_or((None, None, None));
    let duration_secs = match current_track_id {
        Some(track_id) => {
            let mut resolved: u64 = 0;
            if let Some(bridge_state) = app_handle.try_state::<CoreBridgeState>() {
                if let Some(bridge) = bridge_state.try_get().await {
                    resolved = bridge
                        .get_track(track_id)
                        .await
                        .map(|track| u64::from(track.duration))
                        .unwrap_or(0);
                }
            }
            resolved
        }
        None => 0,
    };
    let mut state_report_payload = serde_json::json!({
        "playing_state": PLAYING_STATE_STOPPED,
        "buffer_state": BUFFER_STATE_OK,
        "current_position": 0,
        "duration": duration_secs,
        "queue_version": {
            "major": queue_version_ref.major,
            "minor": queue_version_ref.minor
        }
    });
    if let Some(qid) = current_qid {
        state_report_payload["current_queue_item_id"] = serde_json::json!(qid);
    }
    if let Some(qid) = next_qid {
        state_report_payload["next_queue_item_id"] = serde_json::json!(qid);
    }
    let state_report = RendererReport::new(
        RendererReportType::RndrSrvrStateUpdated,
        Uuid::new_v4().to_string(),
        queue_version_ref,
        state_report_payload,
    );
    if let Err(err) = app.send_renderer_report_command(state_report).await {
        log::error!("[QConnect] Deferred renderer state report failed: {err}");
    }

    // 3. Report volume and max audio quality
    let volume_report = RendererReport::new(
        RendererReportType::RndrSrvrVolumeChanged,
        Uuid::new_v4().to_string(),
        queue_version_ref,
        serde_json::json!({ "volume": 100 }),
    );
    if let Err(err) = app.send_renderer_report_command(volume_report).await {
        log::error!("[QConnect] Deferred renderer volume report failed: {err}");
    }

    let max_quality_report = RendererReport::new(
        RendererReportType::RndrSrvrMaxAudioQualityChanged,
        Uuid::new_v4().to_string(),
        queue_version_ref,
        serde_json::json!({ "max_audio_quality": AUDIO_QUALITY_HIRES_LEVEL2 }),
    );
    if let Err(err) = app.send_renderer_report_command(max_quality_report).await {
        log::error!("[QConnect] Deferred renderer max quality report failed: {err}");
    }

    log::info!("[QConnect] Deferred renderer join complete");

    // Re-request session state so the server sends an updated renderer list
    // (including ourselves). Without this, the frontend may not see QBZ as a
    // renderer until the next reconnect cycle.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let refresh_payload = serde_json::json!({});
    let refresh_command = app
        .build_queue_command(
            QueueCommandType::CtrlSrvrAskForQueueState,
            refresh_payload,
        )
        .await;
    if let Ok(action_uuid) = app.send_queue_command(refresh_command).await {
        app.clear_pending_if_matches(&action_uuid).await;
        log::info!("[QConnect] Re-requested session state after renderer join");
    }

    // P1-8: also resync the active renderer's full state (not just the queue)
    // right after the join, so a reconnect rejoin restores renderer state too.
    if let Err(err) = app.ask_for_active_renderer_state().await {
        log::warn!("[QConnect] Post-join AskForRendererState failed: {err}");
    }

    // Record this session_uuid so a subsequent SESSION_STATE with the same uuid
    // takes the idempotent fast-path above instead of re-announcing us.
    {
        let mut st = sync_state.lock().await;
        st.last_joined_session_uuid = Some(session_uuid.to_string());
    }
}
