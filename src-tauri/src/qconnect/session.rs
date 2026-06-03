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
pub use qconnect_app::{QconnectRendererInfo, QconnectSessionRendererState, QconnectSessionState};

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

pub(super) fn build_effective_renderer_snapshot(
    queue: &QConnectQueueState,
    base_renderer_state: &QConnectRendererState,
    session_renderer_state: Option<&QconnectSessionRendererState>,
    session_loop_mode: Option<i32>,
) -> QConnectRendererState {
    let mut renderer_snapshot = base_renderer_state.clone();

    if let Some(session_renderer_state) = session_renderer_state {
        if let Some(active) = session_renderer_state.active {
            renderer_snapshot.active = Some(active);
        }
        if let Some(playing_state) = session_renderer_state.playing_state {
            renderer_snapshot.playing_state = Some(playing_state);
        }
        if let Some(current_position_ms) = session_renderer_state.current_position_ms {
            renderer_snapshot.current_position_ms = Some(current_position_ms);
        }
        if let Some(volume) = session_renderer_state.volume {
            renderer_snapshot.volume = Some(volume);
        }
        if let Some(muted) = session_renderer_state.muted {
            renderer_snapshot.muted = Some(muted);
        }
        if let Some(max_audio_quality) = session_renderer_state.max_audio_quality {
            renderer_snapshot.max_audio_quality = Some(max_audio_quality);
        }
        if let Some(loop_mode) = session_renderer_state.loop_mode.or(session_loop_mode) {
            renderer_snapshot.loop_mode = Some(loop_mode);
        }
        if let Some(shuffle_mode) = session_renderer_state.shuffle_mode {
            renderer_snapshot.shuffle_mode = Some(shuffle_mode);
        }
        if session_renderer_state.updated_at_ms > 0 {
            renderer_snapshot.updated_at_ms = session_renderer_state.updated_at_ms;
        }

        if session_renderer_state.current_queue_item_id.is_some() {
            let session_snapshot = build_session_renderer_snapshot(
                queue,
                Some(session_renderer_state),
                session_loop_mode,
            );
            if session_snapshot.current_track.is_some() {
                renderer_snapshot.current_track = session_snapshot.current_track;
                renderer_snapshot.next_track = session_snapshot.next_track;
            }
        }
    } else if let Some(loop_mode) = session_loop_mode {
        renderer_snapshot.loop_mode = Some(loop_mode);
    }

    renderer_snapshot
}

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

