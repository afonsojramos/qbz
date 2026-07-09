//! Offline-cache state for the Slint frontend.
//!
//! Constructs and owns the per-user `OfflineCacheState` (the same on-disk
//! `index.db` + `library.db` paths Tauri uses, so the offline store is
//! SHARED across frontends) and exposes it to the play path. Activated on
//! login/session-restore, torn down on logout.
//!
//! Per-user paths (mirror `src-tauri` `UserDataPaths`):
//!   - offline index.db: `<cache_dir>/qbz/users/<user_id>/audio/index.db`
//!   - library.db:       `<data_dir>/qbz/users/<user_id>/library.db`

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

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

/// Construct + initialize the offline cache for `user_id`, replacing any
/// prior instance. Opens `index.db` under the per-user cache dir and a
/// dedicated `library.db` connection under the per-user data dir.
pub async fn activate(user_id: u64) {
    let Some(cache_dir) = user_cache_dir(user_id) else {
        log::error!("[qbz-slint] offline: could not resolve cache dir");
        return;
    };
    let Some(data_dir) = user_data_dir(user_id) else {
        log::error!("[qbz-slint] offline: could not resolve data dir");
        return;
    };

    let state = OfflineCacheState::new_empty();
    if let Err(e) = state.init_at(&cache_dir).await {
        log::error!("[qbz-slint] offline: init_at failed: {e}");
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
        log::warn!("[qbz-slint] offline: library connection failed: {e}");
    }

    *slot().lock().await = Some(Arc::new(state));
    log::info!("[qbz-slint] offline cache activated for user {user_id}");
}

/// The on-disk limit file (next to index.db, in the per-user audio dir).
fn limit_file(audio_dir: &str) -> std::path::PathBuf {
    std::path::Path::new(audio_dir).join("offline_limit")
}

fn read_persisted_limit(audio_dir: &str) -> Option<u64> {
    std::fs::read_to_string(limit_file(audio_dir))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
}

/// Persist the cache size limit (bytes) to disk for the active cache, so the
/// manager's edit-limit survives restarts.
pub async fn persist_limit(bytes: u64) {
    if let Some(off) = get().await {
        let path = limit_file(&off.get_cache_path());
        if let Err(e) = std::fs::write(&path, bytes.to_string()) {
            log::warn!("[qbz-slint] offline: persist limit failed: {e}");
        }
    }
}

/// Tear down the offline cache on logout.
pub async fn deactivate() {
    if let Some(state) = slot().lock().await.take() {
        state.teardown().await;
        log::info!("[qbz-slint] offline cache deactivated");
    }
}

/// The active offline cache state, or `None` before login / after logout.
pub async fn get() -> Option<Arc<OfflineCacheState>> {
    slot().lock().await.clone()
}
