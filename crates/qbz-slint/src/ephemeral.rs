//! Process-global wrapper around the shared `qbz_library::ephemeral` state.
//!
//! Ephemeral folders let the user open a folder OUTSIDE their indexed library,
//! browse it, and play it without anything being written to `library.db`. The
//! scan/metadata/CUE logic is frontend-agnostic (`qbz-library`, ADR-006); this
//! module just holds the single in-process session and exposes thin helpers the
//! UI controller (`local_library.rs`) and the playback resolver (`playback.rs`)
//! call. The session is in-memory only — it vanishes on app exit; what survives
//! a restart is the folder PATH, persisted in `locallibrary_prefs` and
//! re-scanned on startup.

use std::path::Path;
use std::sync::LazyLock;

use qbz_library::ephemeral::{
    EphemeralError, EphemeralFolderResult, EphemeralLibraryState, EPHEMERAL_ID_FLOOR,
};
use qbz_library::LocalTrack;

// Every method on `EphemeralLibraryState` takes `&self` and guards its own
// inner `Mutex`, so a bare `LazyLock` (no outer lock) is enough.
static STATE: LazyLock<EphemeralLibraryState> = LazyLock::new(EphemeralLibraryState::new);

/// `true` if `id` is a synthetic ephemeral track id (>= 2^48). DB row ids never
/// reach this floor, so the check unambiguously routes playback to the in-memory
/// store instead of `library.db`.
pub fn is_ephemeral_id(id: i64) -> bool {
    id >= EPHEMERAL_ID_FLOOR
}

/// Scan a folder and replace the current ephemeral session. Blocking (fs I/O) —
/// call from `spawn_blocking`.
pub fn open(path: &Path) -> Result<EphemeralFolderResult, EphemeralError> {
    STATE.open_folder(path)
}

/// Drop the current ephemeral session.
pub fn clear() {
    STATE.clear();
}

/// Resolve a synthetic id to its cached `LocalTrack` (None if the session was
/// cleared or the id is stale).
pub fn get_track(id: i64) -> Option<LocalTrack> {
    STATE.get_track(id)
}

/// Every track in the current session, in scan order.
pub fn tracks_snapshot() -> Vec<LocalTrack> {
    STATE.tracks_snapshot()
}

/// The tracks of one album group (matched on `album_group_key`, with the same
/// `album|||album_artist` fallback the scanner uses), in scan order.
pub fn album_tracks(group_key: &str) -> Vec<LocalTrack> {
    STATE
        .tracks_snapshot()
        .into_iter()
        .filter(|t| ephemeral_album_key(t) == group_key)
        .collect()
}

/// The album grouping key for an ephemeral track — `album_group_key` when set,
/// else `album|||album_artist` (mirrors the scanner's fallback so the UI groups
/// and the play-album lookup agree).
pub fn ephemeral_album_key(t: &LocalTrack) -> String {
    if !t.album_group_key.is_empty() {
        t.album_group_key.clone()
    } else {
        format!(
            "{}|||{}",
            t.album,
            t.album_artist.as_deref().unwrap_or(&t.artist)
        )
    }
}
