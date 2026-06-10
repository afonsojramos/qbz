//! Qobuz playlist snapshot — Slint glue (offline-mode port, B7/B8).
//!
//! Storage lives in the shared per-user `library.db`
//! (`qbz_library::qobuz_playlist_snapshot`). Producers are opportunistic and
//! DETACHED: the sidebar / playlist-manager list loads persist names for all
//! listed playlists, and the online playlist detail full-replaces one
//! playlist's membership — all from data those loads already fetched, off
//! the calling task, never blocking a render.
//!
//! Offline consumers (sidebar / manager / mixed-playlist detail) read the
//! snapshot blocking-side to resolve real names and the cached-playable
//! membership subset. Rows are point-in-time — shown as-is (no staleness UI
//! in v1).

use std::collections::{HashMap, HashSet};

use qbz_library::qobuz_playlist_snapshot as repo;

pub use repo::SnapshotNameEntry;

/// NAMES producer: persist id+name(+owner, track_count) for every listed
/// playlist. Fire-and-forget — runs on a blocking thread, the caller (an
/// async list load) never waits on it.
pub fn record_names_detached(entries: Vec<SnapshotNameEntry>) {
    if entries.is_empty() {
        return;
    }
    let write = move || {
        let res = crate::library_db::with_db(|db| {
            Ok(db.with_connection(|conn| repo::upsert_names(conn, &entries)))
        });
        if let Some(Err(e)) = res {
            log::warn!("[qbz-slint] playlist snapshot names write failed: {e}");
        }
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::spawn_blocking(write);
    } else {
        std::thread::spawn(write);
    }
}

/// MEMBERSHIP producer: full-replace one playlist's snapshot track ids (the
/// online detail load already fetched them). Fire-and-forget. Writes nothing
/// for playlists the names producer never captured (not the user's list —
/// merely-viewed public playlists stay out of the snapshot).
pub fn record_detail_detached(
    playlist_id: u64,
    name: String,
    owner: String,
    track_ids: Vec<u64>,
) {
    let write = move || {
        let owner = Some(owner.as_str()).filter(|o| !o.is_empty());
        let res = crate::library_db::with_db(|db| {
            Ok(db.with_connection(|conn| {
                repo::replace_tracks(conn, playlist_id, &name, owner, &track_ids)
            }))
        });
        match res {
            Some(Ok(true)) => {}
            Some(Ok(false)) => log::debug!(
                "[qbz-slint] playlist snapshot: {playlist_id} not in the names set, membership skipped"
            ),
            Some(Err(e)) => {
                log::warn!("[qbz-slint] playlist snapshot membership write failed: {e}")
            }
            None => {}
        }
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::spawn_blocking(write);
    } else {
        std::thread::spawn(write);
    }
}

/// All snapshot headers: playlist id -> (name, total Qobuz track count at
/// snapshot time). Blocking — call from `spawn_blocking`.
pub fn headers_blocking() -> HashMap<u64, (String, Option<u32>)> {
    crate::library_db::with_db(|db| Ok(db.with_connection(repo::all_headers)))
        .and_then(|r| r.ok())
        .map(|headers| {
            headers
                .into_iter()
                .map(|h| (h.qobuz_playlist_id, (h.name, h.track_count)))
                .collect()
        })
        .unwrap_or_default()
}

/// Snapshot name of one playlist. Blocking.
pub fn name_blocking(playlist_id: u64) -> Option<String> {
    crate::library_db::with_db(|db| {
        Ok(db.with_connection(|conn| repo::get_header(conn, playlist_id)))
    })
    .and_then(|r| r.ok())
    .flatten()
    .map(|h| h.name)
}

/// B8 availability: the Qobuz playlists whose snapshot membership intersects
/// the offline-cache ready set — i.e. they have at least one track playable
/// offline right now. Empty past the subscription grace window (D4: the
/// cache may not serve full tracks, so nothing is playable). Blocking.
pub fn available_offline_blocking() -> HashSet<u64> {
    if !crate::offline_mode::offline_playback_allowed() {
        return HashSet::new();
    }
    let cached = crate::offline_cache::cached_ids_set();
    if cached.is_empty() {
        return HashSet::new();
    }
    crate::library_db::with_db(|db| Ok(db.with_connection(repo::all_track_ids)))
        .and_then(|r| r.ok())
        .map(|memberships| {
            memberships
                .into_iter()
                .filter(|(_, ids)| ids.iter().any(|id| cached.contains(id)))
                .map(|(id, _)| id)
                .collect()
        })
        .unwrap_or_default()
}

/// B8 detail: one playlist's snapshot track ids that are PLAYABLE offline
/// (cached + within the grace window), in snapshot position order. Blocking.
pub fn playable_track_ids_blocking(playlist_id: u64) -> Vec<u64> {
    if !crate::offline_mode::offline_playback_allowed() {
        return Vec::new();
    }
    let cached = crate::offline_cache::cached_ids_set();
    if cached.is_empty() {
        return Vec::new();
    }
    crate::library_db::with_db(|db| {
        Ok(db.with_connection(|conn| repo::track_ids(conn, playlist_id)))
    })
    .and_then(|r| r.ok())
    .map(|ids| ids.into_iter().filter(|id| cached.contains(id)).collect())
    .unwrap_or_default()
}
