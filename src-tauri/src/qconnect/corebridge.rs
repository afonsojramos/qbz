//! Thin Tauri glue between cloud-emitted QConnect events and the local
//! `CoreBridge` audio engine.
//!
//! The renderer orchestration (apply renderer commands, queue materialize,
//! shuffle projection, cursor align, loop mode, load dedup) was hoisted into the
//! frontend-agnostic `qconnect_app::renderer` module (slice 6, step 6) so the
//! Slint adapter reuses it. What stays here is ONLY the adapter-specific work the
//! `&impl QconnectRendererEngine` boundary cannot express: unwrapping the
//! `Arc<RwLock<Option<CoreBridge>>>` "not initialized yet" guard before dispatch.
//! `CoreBridge` implements the trait (see `track_loading.rs`), so each wrapper
//! unwraps the guard and forwards `&CoreBridge`.
//!
//! Note: in operation `core_bridge` is populated at startup (before any QConnect
//! session can open), so the `else { return Err }` arms are unreachable; the
//! guard stays as a defensive invariant. This keeps behavior byte-identical to
//! the prior inline implementation.

use std::sync::Arc;

use qconnect_app::{QConnectQueueState, QConnectRendererState, RendererCommand};
use tokio::sync::{Mutex, RwLock};

use crate::core_bridge::CoreBridge;

use super::QconnectRemoteSyncState;

/// Re-exported for `tests.rs` (`super::corebridge::queue_state_needs_materialization`);
/// the non-test orchestration that consumed it moved to `qconnect_app::renderer`.
#[cfg(test)]
pub(super) use qconnect_app::renderer::queue_state_needs_materialization;

pub(super) async fn apply_remote_loop_mode_to_corebridge(
    core_bridge: &Arc<RwLock<Option<CoreBridge>>>,
    loop_mode: i32,
) -> Result<(), String> {
    let bridge_guard = core_bridge.read().await;
    let Some(bridge) = bridge_guard.as_ref() else {
        return Err("core bridge is not initialized yet".to_string());
    };
    qconnect_app::renderer::apply_remote_loop_mode(bridge, loop_mode).await
}

pub(super) async fn apply_renderer_command_to_corebridge(
    core_bridge: &Arc<RwLock<Option<CoreBridge>>>,
    sync_state: &Arc<Mutex<QconnectRemoteSyncState>>,
    command: &RendererCommand,
    renderer_state: &QConnectRendererState,
) -> Result<(), String> {
    let bridge_guard = core_bridge.read().await;
    let Some(bridge) = bridge_guard.as_ref() else {
        return Err("core bridge is not initialized yet".to_string());
    };
    qconnect_app::renderer::apply_renderer_command(bridge, sync_state, command, renderer_state).await
}

pub(super) async fn materialize_remote_queue_to_corebridge(
    core_bridge: &Arc<RwLock<Option<CoreBridge>>>,
    sync_state: &Arc<Mutex<QconnectRemoteSyncState>>,
    queue_state: &QConnectQueueState,
) -> Result<(), String> {
    let bridge_guard = core_bridge.read().await;
    let Some(bridge) = bridge_guard.as_ref() else {
        return Err("core bridge is not initialized yet".to_string());
    };
    qconnect_app::renderer::materialize_remote_queue(bridge, sync_state, queue_state).await
}

pub(super) async fn align_corebridge_queue_cursor(
    bridge: &CoreBridge,
    track_id: u64,
) -> Result<(), String> {
    qconnect_app::renderer::align_queue_cursor(bridge, track_id).await
}
