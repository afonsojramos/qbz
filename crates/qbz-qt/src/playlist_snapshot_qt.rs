//! Persisted Qobuz playlist names and membership for Qt offline navigation.
//!
//! The schema and queries live in `qbz-library`; this file only joins those
//! frontend-neutral records to Qt's per-user database and ready-cache set.
//! Producers are detached because they consume data the online list/detail
//! loads already fetched and must never delay a render.

use std::collections::{HashMap, HashSet};

use qbz_library::qobuz_playlist_snapshot as repo;

pub use repo::SnapshotNameEntry;

pub fn record_names_detached(entries: Vec<SnapshotNameEntry>) {
    if entries.is_empty() {
        return;
    }
    let write = move || {
        let result = crate::library_db_qt::with_db(true, |db| {
            Ok(db.with_connection(|conn| repo::upsert_names(conn, &entries)))
        });
        if let Some(Err(error)) = result {
            log::warn!("[qbz-qt] playlist snapshot names write failed: {error}");
        }
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::spawn_blocking(write);
    } else {
        std::thread::spawn(write);
    }
}

pub fn record_detail_detached(playlist_id: u64, name: String, owner: String, track_ids: Vec<u64>) {
    let write = move || {
        let owner = Some(owner.as_str()).filter(|value| !value.is_empty());
        let result = crate::library_db_qt::with_db(true, |db| {
            Ok(db.with_connection(|conn| {
                repo::replace_tracks(conn, playlist_id, &name, owner, &track_ids)
            }))
        });
        match result {
            Some(Ok(true)) => {}
            Some(Ok(false)) => log::debug!(
                "[qbz-qt] playlist snapshot: {playlist_id} is not in the user list; membership skipped"
            ),
            Some(Err(error)) => {
                log::warn!("[qbz-qt] playlist snapshot membership write failed: {error}")
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

/// playlist id -> (persisted name, point-in-time Qobuz track count).
pub fn headers_blocking() -> HashMap<u64, (String, Option<u32>)> {
    crate::library_db_qt::with_db(false, |db| Ok(db.with_connection(repo::all_headers)))
        .and_then(Result::ok)
        .map(|headers| {
            headers
                .into_iter()
                .map(|header| (header.qobuz_playlist_id, (header.name, header.track_count)))
                .collect()
        })
        .unwrap_or_default()
}

/// Playlists whose persisted membership contains at least one playable cache
/// entry. An expired subscription grace window makes the whole set empty.
pub fn available_offline_blocking() -> HashSet<u64> {
    if !crate::offline_fwd::offline_playback_allowed() {
        return HashSet::new();
    }
    let cached = crate::offline_qt::cached_ids_set();
    if cached.is_empty() {
        return HashSet::new();
    }
    crate::library_db_qt::with_db(false, |db| Ok(db.with_connection(repo::all_track_ids)))
        .and_then(Result::ok)
        .map(|memberships| {
            memberships
                .into_iter()
                .filter(|(_, ids)| ids.iter().any(|id| cached.contains(id)))
                .map(|(playlist_id, _)| playlist_id)
                .collect()
        })
        .unwrap_or_default()
}
