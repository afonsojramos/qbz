//! Frontend-agnostic session/liveness primitives for Qobuz Connect.
//!
//! These are pure functions and pure data enums relocated out of the Tauri
//! adapter so that both the shipping Tauri adapter and a future Slint adapter
//! can share the exact same session-arbitration / liveness logic. Behavior is
//! byte-for-byte identical to the prior Tauri-side definitions.

use qbz_models::Quality;

/// Playing-state wire value for PLAYING. Mirrors the Tauri adapter's
/// `PLAYING_STATE_PLAYING` (the renderer reports playing_state == 2 while
/// actively playing). Kept private to the moved liveness predicate.
const PLAYING_STATE_PLAYING: i32 = 2;

/// JoinSession `reason` wire values (proto tag 3): a first join from a fresh
/// runtime is a controller request, a join after a transport drop carries the
/// reconnection reason so the server treats it as session continuity rather
/// than a brand-new controller (P1-2).
pub const JOIN_SESSION_REASON_CONTROLLER_REQUEST: i32 = 1;
pub const JOIN_SESSION_REASON_RECONNECTION: i32 = 2;

/// Official "renderer LOST" silence budget. A *playing* active peer renderer
/// that sends no RENDERER_STATE_UPDATED for this long is considered
/// unreachable (webplayer arms setTimeout(...,12e3) on onPlayerStateUpdated
/// while playingState==PLAY). See 05-sync-status-queue.md §1.
pub const QCONNECT_RENDERER_LOST_TIMEOUT_MS: u64 = 12_000;

/// Pure arming predicate for the renderer-liveness watchdog: arm only while the
/// active renderer is a peer AND its reported playing_state is PLAYING.
pub fn should_arm_renderer_watchdog(playing_state: Option<i32>, is_active_peer: bool) -> bool {
    is_active_peer && playing_state == Some(PLAYING_STATE_PLAYING)
}

/// The server's view of who currently owns the active-renderer slot in a
/// SESSION_STATE frame, classified relative to us (P1-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerActiveState {
    /// No renderer is active in the session.
    None,
    /// We are the active renderer.
    Me,
    /// Another renderer is active and reports PLAYING.
    OtherPlaying,
    /// Another renderer is active and reports a non-playing state.
    OtherPaused,
}

/// Outcome of takeover arbitration on a SESSION_STATE frame (P1-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionDecision {
    /// We should consider ourselves the active renderer.
    pub should_be_active: bool,
    /// We should emit CtrlSrvrSetActiveRenderer to claim the slot.
    pub should_set_active_renderer: bool,
    /// We should push our queue + state to the session.
    pub should_set_queue: bool,
    /// We should ask the renderer for its current state.
    pub should_ask_queue: bool,
}

/// Pure takeover arbitration ported from the web controller's
/// `computeConnectionState`. This is a REDUCED matrix: it captures the
/// shape and named outputs faithfully from the appendix prose, but the
/// exact web truth-table cell values were not dumped verbatim. The
/// `OtherPaused` vs `OtherPlaying` divergence and repeat-mode
/// reconciliation are pending a decompiled-bundle cross-check; repeat-mode
/// reconciliation is intentionally OUT of P1-3 scope (it overlaps the
/// existing `session_loop_mode` handling in `event_sink.rs`).
pub fn compute_connection_state(
    was_active: bool,
    was_playing: bool,
    server: ServerActiveState,
    queue_equal: bool,
) -> ConnectionDecision {
    use ServerActiveState::*;
    match server {
        None => ConnectionDecision {
            should_be_active: was_active,
            should_set_active_renderer: was_active,
            should_set_queue: was_active,
            should_ask_queue: false,
        },
        Me => ConnectionDecision {
            should_be_active: true,
            should_set_active_renderer: false,
            should_set_queue: !queue_equal && (was_active || was_playing),
            should_ask_queue: queue_equal,
        },
        OtherPlaying | OtherPaused => ConnectionDecision {
            should_be_active: false,
            should_set_active_renderer: false,
            should_set_queue: false,
            should_ask_queue: true,
        },
    }
}

/// Pure selector for the JoinSession `reason`: a post-drop rejoin carries
/// RECONNECTION, the first join from a fresh runtime carries CONTROLLER_REQUEST
/// (P1-2).
pub fn deferred_join_reason(has_disconnected: bool) -> i32 {
    if has_disconnected {
        JOIN_SESSION_REASON_RECONNECTION
    } else {
        JOIN_SESSION_REASON_CONTROLLER_REQUEST
    }
}

/// Pure predicate (P1-8): keep re-asking for queue state after a Lagged
/// broadcast drop until the session_uuid is confirmed or the attempt budget is
/// spent. Stops immediately once the session_uuid is known.
pub fn should_reask_queue_state(
    session_uuid_known: bool,
    attempts: u32,
    max_attempts: u32,
) -> bool {
    !session_uuid_known && attempts < max_attempts
}

/// Wire renderer status from `MESSAGE_TYPE_SRVR_CTRL_RENDERER_STATE_UPDATED`
/// (`status` field, decoded at qconnect-protocol decoder.rs:801).
/// Wire enum: UNKNOWN=0, ACTIVE_CONNECTED=1, ACTIVE_DISCONNECTED=2, INACTIVE=3.
/// Per the official client's `Ya` collapse, UNKNOWN and any UNRECOGNIZED value
/// map to INACTIVE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererStatus {
    ActiveConnected,
    ActiveDisconnected,
    Inactive,
}

impl RendererStatus {
    pub fn from_wire(value: Option<i64>) -> Self {
        match value {
            Some(1) => Self::ActiveConnected,
            Some(2) => Self::ActiveDisconnected,
            // 0 (UNKNOWN), 3 (INACTIVE), any UNRECOGNIZED value, and a missing
            // field all collapse to INACTIVE.
            _ => Self::Inactive,
        }
    }
}

/// Map a QConnect `max_audio_quality` level to a qbz `Quality`.
/// QConnect levels: 0/1 ~ MP3, 2 ~ CD/Lossless, 3 ~ Hi-Res (<=96kHz),
/// 4 ~ Hi-Res (>96kHz), 5/None ~ uncapped. The qbz `Quality` enum only has
/// four variants (Mp3, Lossless, HiRes, UltraHiRes), so 4 and uncapped both
/// resolve to UltraHiRes.
pub fn quality_from_max_audio_quality(level: Option<i32>) -> Quality {
    match level {
        Some(l) if l <= 1 => Quality::Mp3,
        Some(2) => Quality::Lossless,
        Some(3) => Quality::HiRes,
        Some(4) => Quality::UltraHiRes,
        _ => Quality::UltraHiRes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_join_reason_is_reconnection_only_after_a_drop() {
        assert_eq!(
            deferred_join_reason(false),
            JOIN_SESSION_REASON_CONTROLLER_REQUEST
        );
        assert_eq!(deferred_join_reason(true), JOIN_SESSION_REASON_RECONNECTION);
    }

    #[test]
    fn reask_queue_state_stops_once_session_uuid_known_or_budget_spent() {
        assert!(should_reask_queue_state(false, 0, 5));
        assert!(should_reask_queue_state(false, 4, 5));
        assert!(!should_reask_queue_state(false, 5, 5));
        assert!(!should_reask_queue_state(true, 0, 5));
    }

    #[test]
    fn compute_connection_state_matrix() {
        use ServerActiveState::*;
        let d = compute_connection_state(true, true, None, false);
        assert!(
            d.should_be_active
                && d.should_set_active_renderer
                && d.should_set_queue
                && !d.should_ask_queue
        );
        let d = compute_connection_state(true, false, Me, false);
        assert!(d.should_be_active && !d.should_set_active_renderer && d.should_set_queue);
        let d = compute_connection_state(false, false, Me, true);
        assert!(d.should_ask_queue && !d.should_set_queue && !d.should_set_active_renderer);
        let d = compute_connection_state(false, false, OtherPlaying, false);
        assert!(!d.should_be_active && !d.should_set_active_renderer && d.should_ask_queue);
        let d = compute_connection_state(false, false, None, false);
        assert!(
            !d.should_be_active
                && !d.should_set_active_renderer
                && !d.should_set_queue
                && !d.should_ask_queue
        );
    }

    #[test]
    fn renderer_status_from_wire_maps_known_values() {
        assert_eq!(RendererStatus::from_wire(Some(0)), RendererStatus::Inactive); // UNKNOWN collapses
        assert_eq!(
            RendererStatus::from_wire(Some(1)),
            RendererStatus::ActiveConnected
        );
        assert_eq!(
            RendererStatus::from_wire(Some(2)),
            RendererStatus::ActiveDisconnected
        );
        assert_eq!(RendererStatus::from_wire(Some(3)), RendererStatus::Inactive);
    }

    #[test]
    fn renderer_status_from_wire_collapses_unknown_and_missing_to_inactive() {
        assert_eq!(RendererStatus::from_wire(Some(99)), RendererStatus::Inactive); // UNRECOGNIZED
        assert_eq!(RendererStatus::from_wire(None), RendererStatus::Inactive); // absent field
    }

    #[test]
    fn watchdog_arms_only_for_playing_active_peer() {
        const PLAYING_STATE_UNKNOWN: i32 = 0;
        const PLAYING_STATE_STOPPED: i32 = 1;
        const PLAYING_STATE_PAUSED: i32 = 3;
        // Arm: playing AND active peer.
        assert!(should_arm_renderer_watchdog(
            Some(PLAYING_STATE_PLAYING),
            true
        ));
        // Do not arm when paused/stopped/unknown even if active peer.
        assert!(!should_arm_renderer_watchdog(
            Some(PLAYING_STATE_PAUSED),
            true
        ));
        assert!(!should_arm_renderer_watchdog(
            Some(PLAYING_STATE_STOPPED),
            true
        ));
        assert!(!should_arm_renderer_watchdog(
            Some(PLAYING_STATE_UNKNOWN),
            true
        ));
        assert!(!should_arm_renderer_watchdog(None, true));
        // Do not arm when not an active peer (e.g. local renderer is active).
        assert!(!should_arm_renderer_watchdog(
            Some(PLAYING_STATE_PLAYING),
            false
        ));
    }

    #[test]
    fn quality_from_max_audio_quality_maps_levels() {
        // qbz Quality has four variants: Mp3, Lossless (CD), HiRes (<=96kHz),
        // UltraHiRes (>96kHz). QConnect levels collapse onto these.
        assert_eq!(quality_from_max_audio_quality(Some(0)), Quality::Mp3);
        assert_eq!(quality_from_max_audio_quality(Some(1)), Quality::Mp3);
        assert_eq!(quality_from_max_audio_quality(Some(2)), Quality::Lossless);
        assert_eq!(quality_from_max_audio_quality(Some(3)), Quality::HiRes);
        assert_eq!(quality_from_max_audio_quality(Some(4)), Quality::UltraHiRes);
        assert_eq!(quality_from_max_audio_quality(Some(5)), Quality::UltraHiRes);
        assert_eq!(quality_from_max_audio_quality(None), Quality::UltraHiRes);
        assert_eq!(quality_from_max_audio_quality(Some(99)), Quality::UltraHiRes);
    }
}
