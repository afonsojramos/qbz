//! Shared favorite-track cache.
//!
//! A single process-wide set of the user's favorite track IDs, so every
//! track-list surface (album, artist, search, playlist, mix, favorites,
//! queue) can stamp `is-favorite` on each row without re-fetching, and the
//! row heart can toggle optimistically.
//!
//! Disk-first seeding: [`init_for_user`] binds the per-user persistent
//! store (`favorites_cache.db`, same file + schema as Tauri) on session
//! activation and loads the IDs from disk — so hearts are correct offline.
//! The online shell entry then refreshes the set from the network and
//! writes it back via [`set_all`]. Toggles keep memory and disk in sync
//! through [`set`].

use std::collections::HashSet;
use std::path::Path;
use std::sync::{LazyLock, Mutex, RwLock};

use qbz_app::settings::favorites_cache::FavoritesCacheStore;

static FAVORITES: LazyLock<RwLock<HashSet<u64>>> =
    LazyLock::new(|| RwLock::new(HashSet::new()));

/// Per-user persistent ID store. `None` until a session (online or offline)
/// is activated; pure in-memory behavior in that window.
static STORE: Mutex<Option<FavoritesCacheStore>> = Mutex::new(None);

/// Bind the per-user store and seed the in-memory set from disk (works
/// offline). Called on every session activation — login, restore, and
/// offline entry — next to `offline_mode::init_for_user`. Best-effort:
/// failures are logged and leave the set empty (hearts render unfavorited,
/// never block entry).
pub fn init_for_user(base_dir: &Path) {
    let store = match FavoritesCacheStore::new_at(base_dir) {
        Ok(store) => store,
        Err(e) => {
            log::error!("[qbz-slint] favorites cache store open failed: {e}");
            return;
        }
    };
    match store.get_favorite_track_ids() {
        Ok(ids) => {
            let set: HashSet<u64> = ids
                .into_iter()
                .filter_map(|id| u64::try_from(id).ok())
                .collect();
            log::info!(
                "[qbz-slint] favorites cache: {} track ids seeded from disk",
                set.len()
            );
            if let Ok(mut guard) = FAVORITES.write() {
                *guard = set;
            }
        }
        Err(e) => log::warn!("[qbz-slint] favorites cache disk seed failed: {e}"),
    }
    if let Ok(mut guard) = STORE.lock() {
        *guard = Some(store);
    }
}

/// Drop the per-user store and the in-memory set on logout.
pub fn teardown() {
    if let Ok(mut guard) = STORE.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = FAVORITES.write() {
        guard.clear();
    }
}

/// Replace the cache with a freshly-fetched set and mirror it to the
/// per-user store — full replace, the same semantics as Tauri's
/// `v2_sync_cached_favorite_tracks`. Blocking disk write; call off the
/// UI thread.
pub fn set_all(ids: HashSet<u64>) {
    if let Ok(mut guard) = FAVORITES.write() {
        *guard = ids.clone();
    }
    if let Ok(guard) = STORE.lock() {
        if let Some(store) = guard.as_ref() {
            let disk: Vec<i64> = ids.iter().map(|&id| id as i64).collect();
            if let Err(e) = store.sync_favorite_tracks(&disk) {
                log::warn!("[qbz-slint] favorites cache disk sync failed: {e}");
            }
        }
    }
}

/// True when the given track id (string form) is in the favorite set.
/// Non-numeric ids (local tracks) are never favorites.
pub fn is_favorite(track_id: &str) -> bool {
    let Ok(id) = track_id.parse::<u64>() else {
        return false;
    };
    contains(id)
}

/// True when the given numeric track id is in the favorite set.
pub fn contains(track_id: u64) -> bool {
    FAVORITES
        .read()
        .map(|g| g.contains(&track_id))
        .unwrap_or(false)
}

/// Insert / remove a single id, keeping the cache consistent with an
/// optimistic UI toggle, and mirror the change to the per-user store so
/// hearts survive a restart.
pub fn set(track_id: u64, favorite: bool) {
    if let Ok(mut guard) = FAVORITES.write() {
        if favorite {
            guard.insert(track_id);
        } else {
            guard.remove(&track_id);
        }
    }
    if let Ok(guard) = STORE.lock() {
        if let Some(store) = guard.as_ref() {
            let res = if favorite {
                store.add_favorite_track(track_id as i64)
            } else {
                store.remove_favorite_track(track_id as i64)
            };
            if let Err(e) = res {
                log::warn!("[qbz-slint] favorites cache disk update failed: {e}");
            }
        }
    }
}
