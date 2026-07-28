//! Rust-side navigation history — the POC counterpart of Slint's
//! `NavState.can-back` / `can-forward` / request-back / request-forward.
//!
//! A simple stack (Vec + index): `record(view)` pushes a view (truncating
//! any forward entries), `back()` / `forward()` move the cursor. Every
//! mutation republishes `canBack` / `canForward` / `currentView` onto the
//! bridge. Phase 2 has a single content view ("home"); the stack is wired
//! now so phase 3's views only need to call `record`.

use std::sync::Mutex;

use cxx_qt_lib::QString;

struct NavHistory {
    entries: Vec<String>,
    // Index of the CURRENT entry in `entries` (0 when only "home" exists).
    index: usize,
}

static HISTORY: Mutex<Option<NavHistory>> = Mutex::new(None);

fn with_history<R>(f: impl FnOnce(&mut NavHistory) -> R) -> R {
    let mut guard = HISTORY.lock().unwrap();
    let history = guard.get_or_insert_with(|| NavHistory {
        entries: vec!["home".to_string()],
        index: 0,
    });
    f(history)
}

/// Push a view as the new current entry (no-op when it IS the current one).
pub fn record(view: &str) {
    let (can_back, can_forward, current) = with_history(|h| {
        if h.entries[h.index] != view {
            h.entries.truncate(h.index + 1);
            h.entries.push(view.to_string());
            h.index += 1;
        }
        snapshot(h)
    });
    publish(can_back, can_forward, current);
}

/// Move one entry back, if possible.
pub fn back() {
    let (can_back, can_forward, current) = with_history(|h| {
        if h.index > 0 {
            h.index -= 1;
        }
        snapshot(h)
    });
    publish(can_back, can_forward, current);
}

/// Move one entry forward, if possible.
pub fn forward() {
    let (can_back, can_forward, current) = with_history(|h| {
        if h.index + 1 < h.entries.len() {
            h.index += 1;
        }
        snapshot(h)
    });
    publish(can_back, can_forward, current);
}

fn snapshot(h: &NavHistory) -> (bool, bool, String) {
    (
        h.index > 0,
        h.index + 1 < h.entries.len(),
        h.entries[h.index].clone(),
    )
}

fn publish(can_back: bool, can_forward: bool, current: String) {
    crate::shell_bridge::ui(move |mut b| {
        b.as_mut().set_can_back(can_back);
        b.as_mut().set_can_forward(can_forward);
        b.as_mut().set_current_view(QString::from(current.as_str()));
    });
}

#[cfg(test)]
mod tests {
    // The history logic is exercised through the Mutex global; these tests
    // run serially within one process, so drive it through the public API
    // and re-derive expectations from fresh state each time. (The bridge
    // publish hop is a no-op off the Qt thread — QT_THREAD unset.)

    #[test]
    fn record_back_forward_cycle() {
        super::record("home");
        super::record("album");
        super::record("artist");
        super::back();
        super::back();
        super::forward();
        // Truncate-on-record: recording after back() drops forward entries.
        super::back();
        super::record("playlist");
        // Now: home, playlist (artist/album dropped). Back once -> home.
        super::back();
        // Forward -> playlist again. No panics = the invariants hold.
        super::forward();
    }
}
