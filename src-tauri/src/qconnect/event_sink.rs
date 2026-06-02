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
use super::service::emit_qconnect_diagnostic;
use super::session::{
    build_session_renderer_snapshot, cache_renderer_snapshot, is_peer_renderer_active,
};
use super::{QconnectRemoteSyncState, BUFFER_STATE_OK};

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
            QconnectAppEvent::RendererUnreachable { renderer_id } => {
                self.emit_renderer_freeze_channel("qconnect:renderer_unreachable", *renderer_id);
            }
            QconnectAppEvent::RendererDisconnected { renderer_id } => {
                self.emit_renderer_freeze_channel("qconnect:renderer_disconnected", *renderer_id);
            }
            QconnectAppEvent::ResyncComplete => {
                emit_qconnect_diagnostic(
                    &self.app_handle,
                    "qconnect:resync_complete",
                    "info",
                    serde_json::json!({}),
                );
            }
            QconnectAppEvent::LifecycleChanged { state } => {
                let serialized =
                    serde_json::to_value(state).unwrap_or_else(|_| serde_json::json!("unknown"));
                if let Err(err) = self.app_handle.emit(
                    "qconnect:status_changed",
                    serde_json::json!({ "state": serialized }),
                ) {
                    log::warn!("[QConnect] Failed to emit qconnect:status_changed: {err}");
                }
            }
            QconnectAppEvent::Diagnostic {
                channel,
                level,
                payload,
            } => {
                emit_qconnect_diagnostic(&self.app_handle, channel, level, payload.clone());
            }
            _ => {}
        }

        if let Err(err) = self.app_handle.emit("qconnect:event", &event) {
            log::warn!("[QConnect] Failed to emit tauri event: {err}");
        }
    }
}

impl TauriQconnectEventSink {
    /// Apply a server session-management event by delegating the locked critical
    /// section to qconnect-app, then running the post-lock CoreBridge work the
    /// returned `SessionApplyOutcome` asks for (renderer-engine seam stays
    /// adapter-side, slice 6). This device's identity is resolved here and
    /// injected so qconnect-app stays frontend-agnostic. The post-lock ordering
    /// (loop mode -> local-playback handoff -> projection -> freeze -> watchdog)
    /// is byte-for-byte the prior inline implementation; only the locked match,
    /// watchdog spawn, and freeze now live in qconnect-app.
    async fn apply_session_management_event(&self, message_type: &str, payload: &Value) {
        let Some(app) = self.app.get().and_then(Weak::upgrade) else {
            return;
        };
        let identity = super::session::resolve_local_identity();
        let outcome = app
            .apply_session_management_event(message_type, payload, &identity)
            .await;

        if let Some(loop_mode) = outcome.apply_loop_mode {
            if let Err(err) =
                apply_remote_loop_mode_to_corebridge(&self.core_bridge, loop_mode).await
            {
                log::warn!("[QConnect] Failed to apply remote loop mode to CoreBridge: {err}");
            }
        }

        if outcome.sync_local_playback {
            self.sync_local_playback_for_renderer_ownership().await;
        }

        if let Some(renderer_id) = outcome.remote_projection_renderer_id {
            self.sync_active_renderer_projection(renderer_id).await;
        }

        if let Some(renderer_id) = outcome.disconnected_renderer_id {
            app.freeze_active_renderer_projection(
                renderer_id,
                QconnectAppEvent::RendererDisconnected { renderer_id },
            )
            .await;
        }

        if let Some((renderer_id, generation)) = outcome.watchdog_arm {
            app.arm_renderer_watchdog(renderer_id, generation);
        }
    }

    /// Emit one renderer-freeze dedicated channel (`qconnect:renderer_unreachable`
    /// / `qconnect:renderer_disconnected`) with the legacy `{renderer_id}` payload.
    /// Called from the `on_event` mapper arms; kept as a small helper so both
    /// freeze channels share the exact same payload shape.
    fn emit_renderer_freeze_channel(&self, channel: &str, renderer_id: i32) {
        if let Err(err) = self
            .app_handle
            .emit(channel, serde_json::json!({ "renderer_id": renderer_id }))
        {
            log::warn!("[QConnect] Failed to emit {channel}: {err}");
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
