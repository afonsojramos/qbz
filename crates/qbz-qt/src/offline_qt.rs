//! Offline-cache state for the Qt frontend — port of `crates/qbz/src/
//! offline.rs` plus the cached-id session set from `offline_cache.rs`.
//!
//! Same on-disk layout as the other frontends (`<cache>/qbz/users/<id>/audio/
//! index.db` + `<data>/qbz/users/<id>/library.db`), so the offline store is
//! SHARED with Slint/Tauri: a track downloaded there plays from the offline
//! tier here and vice versa. Activated on login/session-restore from
//! `auth_qt`, torn down on logout.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use tokio::sync::Mutex;

use qbz_offline_cache::OfflineCacheState;

static OFFLINE: OnceLock<Mutex<Option<Arc<OfflineCacheState>>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Arc<OfflineCacheState>>> {
    OFFLINE.get_or_init(|| Mutex::new(None))
}

fn user_cache_dir(user_id: u64) -> Option<PathBuf> {
    Some(
        dirs::cache_dir()?
            .join("qbz")
            .join("users")
            .join(user_id.to_string()),
    )
}

fn user_data_dir(user_id: u64) -> Option<PathBuf> {
    Some(
        dirs::data_dir()?
            .join("qbz")
            .join("users")
            .join(user_id.to_string()),
    )
}

/// The on-disk limit file (next to index.db, in the per-user audio dir).
fn limit_file(audio_dir: &str) -> PathBuf {
    std::path::Path::new(audio_dir).join("offline_limit")
}

fn read_persisted_limit(audio_dir: &str) -> Option<u64> {
    std::fs::read_to_string(limit_file(audio_dir))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
}

/// Construct + initialize the offline cache for `user_id`, replacing any
/// prior instance (Slint `offline::activate`).
pub async fn activate(user_id: u64) {
    let Some(cache_dir) = user_cache_dir(user_id) else {
        log::error!("[qbz-qt] offline: could not resolve cache dir");
        return;
    };
    let Some(data_dir) = user_data_dir(user_id) else {
        log::error!("[qbz-qt] offline: could not resolve data dir");
        return;
    };

    let state = OfflineCacheState::new_empty();
    if let Err(e) = state.init_at(&cache_dir).await {
        log::error!("[qbz-qt] offline: init_at failed: {e}");
        return;
    }
    // Restore the persisted cache size limit (written by the manager's
    // edit-limit). Falls back to the 5 GB default when absent.
    if let Some(bytes) = read_persisted_limit(&state.get_cache_path()) {
        state.apply_persisted_limit(Some(bytes)).await;
    }
    if let Err(e) = state.init_library_connection(&data_dir).await {
        // Non-fatal: cached playback still resolves from index.db; only the
        // library-row sync (show-in-library) needs this connection.
        log::warn!("[qbz-qt] offline: library connection failed: {e}");
    }

    *slot().lock().await = Some(Arc::new(state));
    log::info!("[qbz-qt] offline cache activated for user {user_id}");
    load_cached_ids().await;
}

/// Tear down the offline cache on logout.
pub async fn deactivate() {
    if let Some(state) = slot().lock().await.take() {
        state.teardown().await;
        log::info!("[qbz-qt] offline cache deactivated");
    }
    if let Ok(mut s) = cached_ids().lock() {
        s.clear();
    }
}

/// The active offline cache state, or `None` before login / after logout.
pub async fn get() -> Option<Arc<OfflineCacheState>> {
    slot().lock().await.clone()
}

// ---------------------------------------------------------------------------
// The session-wide ready set (Slint offline_cache.rs CACHED_IDS)
// ---------------------------------------------------------------------------

static CACHED_IDS: OnceLock<StdMutex<HashSet<u64>>> = OnceLock::new();

fn cached_ids() -> &'static StdMutex<HashSet<u64>> {
    CACHED_IDS.get_or_init(|| StdMutex::new(HashSet::new()))
}

/// True if `track_id` (string form, as the rows carry it) has a ready offline
/// copy. Used to seed each track row's cache status at document build time.
pub fn is_cached(track_id: &str) -> bool {
    let Ok(id) = track_id.parse::<u64>() else {
        return false;
    };
    cached_ids().lock().map(|s| s.contains(&id)).unwrap_or(false)
}

pub fn mark_cached(track_id: u64, cached: bool) {
    if let Ok(mut set) = cached_ids().lock() {
        if cached {
            set.insert(track_id);
        } else {
            set.remove(&track_id);
        }
    }
}

/// Seed the ready-set from the active cache's index.db (Slint
/// `load_cached_ids`). Called by `activate`.
async fn load_cached_ids() {
    let Some(off) = get().await else {
        return;
    };
    let ids: Vec<u64> = {
        let guard = off.db.lock().await;
        match guard.as_ref() {
            Some(db) => db
                .get_all_tracks()
                .map(|tracks| {
                    tracks
                        .into_iter()
                        .filter(|t| matches!(t.status, qbz_offline_cache::OfflineCacheStatus::Ready))
                        .map(|t| t.track_id)
                        .collect()
                })
                .unwrap_or_default(),
            None => Vec::new(),
        }
    };
    if let Ok(mut set) = cached_ids().lock() {
        *set = ids.into_iter().collect();
        log::info!("[qbz-qt] offline: seeded {} cached track ids", set.len());
    }
}
