//! Session topology: renderer registry, active/local renderer tracking,
//! per-renderer cached state, and renderer-state snapshot construction
//! used by the rest of the QConnect module to reason about who owns
//! playback and what the cloud's view of each renderer looks like.



use qconnect_app::{QConnectQueueState, QConnectRendererState};
use qconnect_core::QueueItem;
use serde::Serialize;

use super::queue_resolution::{find_cursor_index_by_queue_item_id, ordered_queue_cursors};
use super::transport::default_qconnect_device_info;
use super::{QconnectQueueVersionPayload, QconnectVisibleQueueProjection};

/// Session topology types now live in the frontend-agnostic `qconnect_app::session`
/// module (slice 2+4). Re-exported here so existing `super::session::…` /
/// `super::…` references inside this module compile unchanged, and so the Tauri
/// command surface keeps the same serialized shape.
pub use qconnect_app::{QconnectRendererInfo, QconnectSessionState};
// `QconnectSessionRendererState` is only re-exported onward for the test module
// (mod.rs gates its re-export to `#[cfg(test)]`); since
// `build_effective_renderer_snapshot` moved to qconnect-app, no non-test code in
// this module references it, so gate it here too to avoid an unused-import warning.
#[cfg(test)]
pub use qconnect_app::QconnectSessionRendererState;

/// Pure session mutators + the file-audio-quality snapshot type also moved to
/// qconnect-app (slice 2+4). Re-exported so existing `super::session::…` /
/// `super::…` references compile unchanged.
pub(super) use qconnect_app::{
    ensure_session_renderer_state, is_local_renderer_active, is_peer_renderer_active,
    renderer_allows_remote_volume, QconnectFileAudioQualitySnapshot,
};

/// The session-projection helpers (`queue_item_snapshot_for_cursor`,
/// `build_session_renderer_snapshot`) and the renderer-snapshot cache mutator
/// (`cache_renderer_snapshot`) also moved to qconnect-app (slice 6, Slint port)
/// so the Tauri and Slint event sinks share one definition. Re-exported so the
/// existing `super::session::…` / `super::…` callers compile unchanged.
pub(super) use qconnect_app::{
    build_session_renderer_snapshot, cache_renderer_snapshot, queue_item_snapshot_for_cursor,
};
// find_unique_renderer_id's sole non-test caller (refresh_local_renderer_id)
// moved to qconnect-app, so it is referenced only by the test module now; gate
// the re-export to test builds.
#[cfg(test)]
pub(super) use qconnect_app::find_unique_renderer_id;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct QconnectRendererReportDebugEvent {
    pub(super) requested_current_queue_item_id: Option<i32>,
    pub(super) requested_next_queue_item_id: Option<i32>,
    pub(super) resolved_current_queue_item_id: Option<i32>,
    pub(super) resolved_next_queue_item_id: Option<i32>,
    pub(super) sent_current_queue_item_id: Option<i32>,
    pub(super) sent_next_queue_item_id: Option<i32>,
    pub(super) report_queue_item_ids: bool,
    pub(super) current_track_id: Option<i64>,
    pub(super) playing_state: i32,
    pub(super) current_position: Option<i32>,
    pub(super) duration: Option<i32>,
    pub(super) queue_version: QconnectQueueVersionPayload,
    pub(super) resolution_strategy: String,
}

/// Resolve THIS device's identity (uuid + device-info) for injection into the
/// frontend-agnostic session-apply logic in qconnect-app. The device-info build
/// is idempotent and side-effect-free (env reads + the cached uuid), so
/// resolving it here once per session event yields the exact renderer-id
/// resolution the prior in-place `refresh_local_renderer_id` produced.
pub(super) fn resolve_local_identity() -> qconnect_app::LocalIdentity {
    let info = default_qconnect_device_info();
    qconnect_app::LocalIdentity {
        device_uuid: info.device_uuid.unwrap_or_default(),
        friendly_name: info.friendly_name,
        brand: info.brand,
        model: info.model,
        device_type: info.device_type,
    }
}

// `build_effective_renderer_snapshot` is now defined in qconnect-app (ADR-006
// hoist) so the Tauri and Slint controller paths share one definition. Re-exported
// here so existing `super::session::…` / `super::…` callers compile unchanged.
pub(super) use qconnect_app::build_effective_renderer_snapshot;

pub(super) fn build_visible_queue_projection(
    queue: &QConnectQueueState,
    renderer: &QConnectRendererState,
) -> QconnectVisibleQueueProjection {
    use super::queue_resolution::find_cursor_index_by_track_id;

    let cursors = ordered_queue_cursors(queue);

    let current_index = find_cursor_index_by_queue_item_id(
        &cursors,
        queue,
        renderer
            .current_track
            .as_ref()
            .map(|item| item.queue_item_id),
    )
    .or_else(|| {
        find_cursor_index_by_track_id(
            &cursors,
            queue,
            renderer.current_track.as_ref().map(|item| item.track_id),
        )
    });

    let next_index = find_cursor_index_by_queue_item_id(
        &cursors,
        queue,
        renderer.next_track.as_ref().map(|item| item.queue_item_id),
    )
    .or_else(|| {
        find_cursor_index_by_track_id(
            &cursors,
            queue,
            renderer.next_track.as_ref().map(|item| item.track_id),
        )
    });

    let (current_track, start_index) = if let Some(index) = current_index {
        (
            queue_item_snapshot_for_cursor(queue, cursors[index]),
            index.saturating_add(1),
        )
    } else if let Some(index) = next_index {
        let inferred_current = index
            .checked_sub(1)
            .and_then(|current_index| cursors.get(current_index).copied())
            .and_then(|cursor| queue_item_snapshot_for_cursor(queue, cursor));
        (inferred_current, index)
    } else {
        (None, 0)
    };

    let upcoming_tracks: Vec<QueueItem> = cursors
        .into_iter()
        .skip(start_index)
        .filter_map(|cursor| queue_item_snapshot_for_cursor(queue, cursor))
        .collect();

    QconnectVisibleQueueProjection {
        current_track,
        upcoming_tracks,
    }
}

