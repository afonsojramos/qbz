//! Per-user library.db access — the local-only playlist flags, ported from
//! `crates/qbz/src/library_db.rs`. Opens
//! `<data_dir>/qbz/users/<uid>/library.db` on demand via the `qbz-library`
//! backend crate (ADR-006 — no glue copied).
//!
//! Why a LOCAL db and not the Qobuz API: the playlist heart is a qbz-only
//! flag. `/favorite/create` and `/favorite/delete` accept exactly five id
//! params — `album_ids`, `artist_ids`, `track_ids`, `label_ids`, `award_ids`
//! (inferred OpenAPI v10.0.0.0-beta, §Favorites) — there is no
//! `playlist_ids`, which is why the reference routes `("playlist",
//! "favorite")` to `db.set_playlist_favorite` instead (main.rs:13652, its
//! comment: "Qobuz /favorite/create rejects playlist_ids").
//!
//! `LibraryDatabase` holds a non-Send `rusqlite::Connection`, so every call
//! opens it fresh; async callers must wrap these in `spawn_blocking`.

use std::path::{Path, PathBuf};

use qbz_app::user_data::UserDataPaths;
use qbz_library::{LibraryDatabase, LibraryError};

/// `<data_dir>/qbz/users/<uid>/library.db` — the same per-user path the
/// reference uses, so the local organization data is shared between builds.
fn db_path() -> Option<PathBuf> {
    let uid = UserDataPaths::load_last_user_id()?;
    Some(
        dirs::data_dir()?
            .join("qbz")
            .join("users")
            .join(uid.to_string())
            .join("library.db"),
    )
}

/// Run `f` against the per-user database. `create` mirrors the reference's
/// `open()`: writes must be able to bring the file into existence (a fresh
/// account has no library.db until the first local flag is set), reads must
/// NOT — an empty result is the right answer and creating the file as a side
/// effect of a read is a surprise.
fn with_db<F, R>(create: bool, f: F) -> Option<R>
where
    F: FnOnce(&LibraryDatabase) -> Result<R, LibraryError>,
{
    let path = db_path()?;
    if create {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    } else if !path.exists() {
        return None;
    }
    let db = match LibraryDatabase::open(&path) {
        Ok(db) => db,
        Err(e) => {
            log::warn!("[qbz-qt] library.db open failed: {e}");
            return None;
        }
    };
    match f(&db) {
        Ok(r) => Some(r),
        Err(e) => {
            log::error!("[qbz-qt] library.db op failed: {e}");
            None
        }
    }
}

/// Hearted playlist ids (favorites.rs `db.get_favorite_playlist_ids`).
/// Empty on any failure (missing user, missing db) — the Library sub-tab
/// then shows owned playlists only.
pub fn favorite_playlist_ids() -> Vec<u64> {
    with_db(false, |db| db.get_favorite_playlist_ids()).unwrap_or_default()
}

/// Same read, against an EXPLICIT per-user directory (`<…>/users/<uid>/`).
///
/// Session activation seeds `fav_cache_qt` from here, and at that moment
/// `UserDataPaths::load_last_user_id()` is not a safe way to name the user
/// being activated — `db_path()` above would resolve to whoever was persisted
/// last. The caller already holds the right directory, so it passes it.
///
/// Read-only, never creates the file: a fresh account with no local flags
/// yet is an empty set, not an error.
pub fn favorite_playlist_ids_at(base_dir: &Path) -> Vec<u64> {
    let path = base_dir.join("library.db");
    if !path.exists() {
        return Vec::new();
    }
    match LibraryDatabase::open(&path) {
        Ok(db) => db.get_favorite_playlist_ids().unwrap_or_else(|e| {
            log::warn!("[qbz-qt] library.db favorite playlist seed failed: {e}");
            Vec::new()
        }),
        Err(e) => {
            log::warn!("[qbz-qt] library.db open failed ({}): {e}", path.display());
            Vec::new()
        }
    }
}

/// Is this playlist hearted? Read straight from the db so the toggle picks
/// its direction from the authority, not from whatever a card happened to
/// render (main.rs `playlist_toggle_favorite_by_id`).
pub fn is_favorite_playlist(pid: u64) -> bool {
    with_db(false, |db| db.get_favorite_playlist_ids())
        .map(|ids| ids.contains(&pid))
        .unwrap_or(false)
}

/// Set the heart. Returns false when the write did not land (no user, open
/// or sqlite failure) so the caller can leave the UI alone.
pub fn set_favorite_playlist(pid: u64, favorite: bool) -> bool {
    with_db(true, |db| db.set_playlist_favorite(pid, favorite)).is_some()
}
