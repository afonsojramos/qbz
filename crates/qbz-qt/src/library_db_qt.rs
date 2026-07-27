//! Per-user library.db access — the one query the Playlists tab needs
//! (hearted playlist ids), ported from `crates/qbz/src/library_db.rs`.
//! Opens `<data_dir>/qbz/users/<uid>/library.db` on demand via the
//! `qbz-library` backend crate (ADR-006 — no glue copied).

use qbz_app::user_data::UserDataPaths;

/// Hearted playlist ids (favorites.rs `db.get_favorite_playlist_ids`).
/// Empty on any failure (missing user, missing db) — the Library sub-tab
/// then shows owned playlists only.
pub fn favorite_playlist_ids() -> Vec<u64> {
    let Some(uid) = UserDataPaths::load_last_user_id() else {
        return Vec::new();
    };
    let Some(path) = dirs::data_dir().map(|p| {
        p.join("qbz")
            .join("users")
            .join(uid.to_string())
            .join("library.db")
    }) else {
        return Vec::new();
    };
    if !path.exists() {
        return Vec::new();
    }
    match qbz_library::LibraryDatabase::open(&path) {
        Ok(db) => db.get_favorite_playlist_ids().unwrap_or_default(),
        Err(e) => {
            log::warn!("[qbz-qt] library.db open failed: {e}");
            Vec::new()
        }
    }
}
