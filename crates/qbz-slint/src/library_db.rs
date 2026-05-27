//! Local library database access — playlist folders + playlist
//! settings (folder membership, custom artwork, hidden/favorite,
//! ordering). These are local-only, stored in the SAME per-user
//! SQLite file the Tauri app uses so the data is shared:
//!
//!   <data_dir>/qbz/users/<user_id>/library.db
//!
//! `LibraryDatabase` holds a non-Send `rusqlite::Connection`, so each
//! operation opens it fresh on a blocking thread (the qbz-radio
//! pattern). Helpers here are synchronous; async callers wrap them in
//! `tokio::task::spawn_blocking`.

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use qbz_library::{LibraryDatabase, LibraryError};

/// The active user id, set on shell entry. The per-user library.db
/// path depends on it.
static CURRENT_USER_ID: LazyLock<Mutex<Option<u64>>> = LazyLock::new(|| Mutex::new(None));

pub fn set_user(user_id: u64) {
    if let Ok(mut guard) = CURRENT_USER_ID.lock() {
        *guard = Some(user_id);
    }
}

fn user_id() -> Option<u64> {
    CURRENT_USER_ID.lock().ok().and_then(|g| *g)
}

/// The active user id (for the favorites Following filter `owner.id != uid`).
pub fn current_user_id() -> Option<u64> {
    user_id()
}

/// `<data_dir>/qbz/users/<user_id>/library.db` — matches the Tauri
/// per-user path so the local organization data is shared.
fn db_path() -> Option<PathBuf> {
    let uid = user_id()?;
    Some(
        dirs::data_dir()?
            .join("qbz")
            .join("users")
            .join(uid.to_string())
            .join("library.db"),
    )
}

/// Open the per-user library database, creating the directory if
/// needed. Returns None when there is no active user or the open
/// fails (logged).
fn open() -> Option<LibraryDatabase> {
    let path = db_path()?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match LibraryDatabase::open(&path) {
        Ok(db) => Some(db),
        Err(e) => {
            log::error!("[qbz-slint] open library.db failed: {e}");
            None
        }
    }
}

/// Run `f` against the per-user library database on the current
/// (blocking) thread. Returns None if the DB can't be opened; logs and
/// returns None on a database error.
pub fn with_db<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&LibraryDatabase) -> Result<R, LibraryError>,
{
    let db = open()?;
    match f(&db) {
        Ok(r) => Some(r),
        Err(e) => {
            log::error!("[qbz-slint] library.db op failed: {e}");
            None
        }
    }
}

/// Resolve the artwork-cache directory used for copied custom images,
/// matching Tauri (`<cache_dir>/qbz/artwork`).
pub fn artwork_cache_dir() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("qbz").join("artwork"))
}
