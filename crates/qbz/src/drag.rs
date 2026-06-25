//! Track drag-and-drop state — the tracks currently being dragged onto a
//! sidebar playlist. The UI (DragState global) drives the ghost +
//! drop highlight; the actual track refs live here (the global only
//! carries a count + ghost text).
//!
//! The payload is SOURCE-TYPED: a row dragged from a Qobuz surface carries
//! its catalog id, a LocalLibrary file row its `local_tracks` row id, a
//! Plex row its rating key. The drop arm maps each variant to the right
//! playlist ref (`qobuz_track_id` / `local_path` / `plex_key`) — storing a
//! library row id as a Qobuz id is what made dropped local rows resolve
//! as unavailable (hidden, D11).

use std::sync::{LazyLock, Mutex};

/// One dragged track, typed by its source namespace.
#[derive(Debug, Clone)]
pub enum DragTrack {
    /// Qobuz catalog track id (also offline-cached rows — those ARE
    /// catalog ids with a local copy).
    Qobuz(u64),
    /// LocalLibrary `local_tracks` row id (resolved source-aware at
    /// insert: offline copies -> Qobuz ref, user files -> local_path).
    LocalRow(i64),
    /// Plex track — the rating key.
    Plex(String),
}

static DRAGGED: LazyLock<Mutex<Vec<DragTrack>>> = LazyLock::new(|| Mutex::new(Vec::new()));

pub fn set_dragged(tracks: Vec<DragTrack>) {
    if let Ok(mut d) = DRAGGED.lock() {
        *d = tracks;
    }
}

pub fn dragged() -> Vec<DragTrack> {
    DRAGGED.lock().map(|d| d.clone()).unwrap_or_default()
}

pub fn clear() {
    if let Ok(mut d) = DRAGGED.lock() {
        d.clear();
    }
}
