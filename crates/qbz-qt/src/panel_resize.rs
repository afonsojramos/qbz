//! Resizable shell panels (#771): the pure drag rules for the playlist
//! sidebar and the queue/lyrics column. QML owns the pointer and paints the
//! handle; every decision about what a dragged x means lives here so it can
//! be unit-tested without Qt.
//!
//! Ranges (owner spec 2026-09-11): the minimum is the width each panel had
//! before, the maximum exactly double. Dragging the sidebar narrower than its
//! minimum snaps it to the mini rail, and narrower still closes it through the
//! same state the header button uses. Dragging the column narrower than its
//! minimum closes it, as its X would.

/// The three-state sidebar's open width, before #771.
pub const SIDEBAR_MIN_WIDTH: i32 = 240;
pub const SIDEBAR_MAX_WIDTH: i32 = SIDEBAR_MIN_WIDTH * 2;
/// The mini rail (theme `sidebarMiniWidth`).
pub const SIDEBAR_MINI_WIDTH: i32 = 64;
/// The queue/lyrics column, before #771.
pub const QUEUE_MIN_WIDTH: i32 = 300;
pub const QUEUE_MAX_WIDTH: i32 = QUEUE_MIN_WIDTH * 2;
/// How far past a boundary the pointer must go before a state snaps. Wide
/// enough that a sloppy drag to "exactly 240" does not collapse the panel.
pub const SNAP_THRESHOLD: i32 = 40;
/// Hysteresis between collapsing and reopening the sidebar: the open panel
/// collapses below `MIN - SNAP_THRESHOLD` (200), the mini rail reopens only
/// at or above `MIN - REOPEN_MARGIN` (220). The 20px band in between is
/// stable in BOTH states. Without it (the first cut reopened at 104) every
/// pointer move between 104 and 199 flipped the state — the open/collapsed
/// flapping in the owner's 2026-09-11 screencast.
pub const REOPEN_MARGIN: i32 = 20;
/// How far past the column's maximum the pointer must push to open the
/// Listen List (the full queue view) — only while the queue is in the column.
pub const PUSH_THROUGH: i32 = 60;

/// Sidebar states as `QbzShell.sidebarState` publishes them.
pub const SIDEBAR_OPEN: i32 = 0;
pub const SIDEBAR_MINI: i32 = 1;
pub const SIDEBAR_CLOSED: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarDrag {
    /// Stay open at this width (already clamped to the range).
    Open(i32),
    /// Snap to the mini rail.
    Mini,
    /// Close, exactly as the header button's third state.
    Closed,
}

/// What a pointer at `x` (window-left-relative px) means for the sidebar in
/// `state`. The closed state has no edge to grab, so it never drags.
pub fn sidebar_drag_target(state: i32, x: i32) -> SidebarDrag {
    match state {
        SIDEBAR_OPEN => {
            if x >= SIDEBAR_MIN_WIDTH - SNAP_THRESHOLD {
                SidebarDrag::Open(x.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH))
            } else if x >= SIDEBAR_MINI_WIDTH - SNAP_THRESHOLD {
                SidebarDrag::Mini
            } else {
                SidebarDrag::Closed
            }
        }
        SIDEBAR_MINI => {
            if x >= SIDEBAR_MIN_WIDTH - REOPEN_MARGIN {
                // Re-expanding lands on the minimum; the persisted wider width
                // is restored by the caller (it never dropped below the min).
                SidebarDrag::Open(x.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH))
            } else if x >= SIDEBAR_MINI_WIDTH - SNAP_THRESHOLD {
                SidebarDrag::Mini
            } else {
                SidebarDrag::Closed
            }
        }
        _ => SidebarDrag::Closed,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnDrag {
    Open(i32),
    Closed,
    /// Pushed past the maximum with the queue in the column: hand the queue
    /// to the Listen List view (queue-view). Lyrics, if open, stay in the
    /// column. A lyrics-only column never pushes through.
    PushThrough,
}

/// What a column width of `width` px (window-right edge minus pointer x)
/// means for the queue/lyrics column. `queue_in_column` is whether the
/// queue panel is currently part of the column.
pub fn queue_drag_target(width: i32, queue_in_column: bool) -> ColumnDrag {
    if queue_in_column && width >= QUEUE_MAX_WIDTH + PUSH_THROUGH {
        ColumnDrag::PushThrough
    } else if width >= QUEUE_MIN_WIDTH - SNAP_THRESHOLD {
        ColumnDrag::Open(width.clamp(QUEUE_MIN_WIDTH, QUEUE_MAX_WIDTH))
    } else {
        ColumnDrag::Closed
    }
}

/// Persisted widths are clamped on the way in so a hand-edited or stale
/// pref can never paint a panel outside its range.
pub fn clamp_sidebar_width(width: i32) -> i32 {
    width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
}

pub fn clamp_queue_width(width: i32) -> i32 {
    width.clamp(QUEUE_MIN_WIDTH, QUEUE_MAX_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_sidebar_clamps_between_min_and_double() {
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 240), SidebarDrag::Open(240));
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 333), SidebarDrag::Open(333));
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 480), SidebarDrag::Open(480));
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 900), SidebarDrag::Open(480));
    }

    #[test]
    fn a_sloppy_drag_just_under_the_minimum_stays_open_at_the_minimum() {
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 239), SidebarDrag::Open(240));
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 200), SidebarDrag::Open(240));
    }

    #[test]
    fn past_the_threshold_the_sidebar_snaps_to_mini_then_closed() {
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 199), SidebarDrag::Mini);
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 64), SidebarDrag::Mini);
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 24), SidebarDrag::Mini);
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 23), SidebarDrag::Closed);
        assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, 0), SidebarDrag::Closed);
    }

    #[test]
    fn mini_rail_reopens_past_its_threshold_and_closes_below_it() {
        assert_eq!(sidebar_drag_target(SIDEBAR_MINI, 64), SidebarDrag::Mini);
        assert_eq!(sidebar_drag_target(SIDEBAR_MINI, 199), SidebarDrag::Mini);
        assert_eq!(sidebar_drag_target(SIDEBAR_MINI, 219), SidebarDrag::Mini);
        assert_eq!(sidebar_drag_target(SIDEBAR_MINI, 220), SidebarDrag::Open(240));
        assert_eq!(sidebar_drag_target(SIDEBAR_MINI, 300), SidebarDrag::Open(300));
        assert_eq!(sidebar_drag_target(SIDEBAR_MINI, 23), SidebarDrag::Closed);
    }

    /// The screencast bug: no pointer x may make the open panel collapse AND
    /// the mini rail reopen — that pair flips the state on every move.
    #[test]
    fn open_and_mini_never_disagree_into_a_flip() {
        for x in 0..=SIDEBAR_MAX_WIDTH + 50 {
            let from_open = sidebar_drag_target(SIDEBAR_OPEN, x);
            let from_mini = sidebar_drag_target(SIDEBAR_MINI, x);
            let flips = from_open == SidebarDrag::Mini
                && matches!(from_mini, SidebarDrag::Open(_));
            assert!(!flips, "x={x} flips open<->mini");
        }
        // And the band between the two thresholds is stable both ways.
        for x in (SIDEBAR_MIN_WIDTH - SNAP_THRESHOLD)..(SIDEBAR_MIN_WIDTH - REOPEN_MARGIN) {
            assert_eq!(sidebar_drag_target(SIDEBAR_OPEN, x), SidebarDrag::Open(240));
            assert_eq!(sidebar_drag_target(SIDEBAR_MINI, x), SidebarDrag::Mini);
        }
    }

    #[test]
    fn closed_sidebar_never_drags() {
        assert_eq!(sidebar_drag_target(SIDEBAR_CLOSED, 300), SidebarDrag::Closed);
    }

    #[test]
    fn queue_column_clamps_and_closes_past_the_threshold() {
        assert_eq!(queue_drag_target(300, true), ColumnDrag::Open(300));
        assert_eq!(queue_drag_target(450, true), ColumnDrag::Open(450));
        assert_eq!(queue_drag_target(659, true), ColumnDrag::Open(600));
        assert_eq!(queue_drag_target(261, true), ColumnDrag::Open(300));
        assert_eq!(queue_drag_target(260, true), ColumnDrag::Open(300));
        assert_eq!(queue_drag_target(259, true), ColumnDrag::Closed);
    }

    #[test]
    fn pushing_past_the_maximum_opens_the_listen_list_only_with_the_queue_in_the_column() {
        assert_eq!(queue_drag_target(660, true), ColumnDrag::PushThrough);
        assert_eq!(queue_drag_target(900, true), ColumnDrag::PushThrough);
        // Lyrics-only column: clamps at the maximum, never pushes through.
        assert_eq!(queue_drag_target(660, false), ColumnDrag::Open(600));
        assert_eq!(queue_drag_target(900, false), ColumnDrag::Open(600));
    }

    #[test]
    fn persisted_widths_are_clamped_on_read() {
        assert_eq!(clamp_sidebar_width(0), 240);
        assert_eq!(clamp_sidebar_width(10_000), 480);
        assert_eq!(clamp_queue_width(-5), 300);
        assert_eq!(clamp_queue_width(601), 600);
    }
}
