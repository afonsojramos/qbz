//! Per-user local-favorites lifecycle + access wrapper.
//!
//! Process-global singleton over the headless
//! `qbz_app::settings::local_favorites::LocalFavoritesService` (ADR-006). Lets
//! the user favorite LOCAL library items (genuine local files + Plex — never the
//! Qobuz offline cache) so they appear in the mixed-library "All" feed behind the
//! `show-local` switch. Mirrors `crate::pinned`: a `Mutex<Option<Service>>` bound
//! per session, in-memory `(kind, id)` set for O(1) heart stamping, fail-open
//! when no session is bound.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use qbz_app::settings::local_favorites::{LocalFavoritesService, DB_FILE_NAME};

pub use qbz_app::settings::local_favorites::LocalFavItem;

static SERVICE: Mutex<Option<LocalFavoritesService>> = Mutex::new(None);

const NO_SESSION_ERR: &str = "No active session - please log in";

/// Bind the per-user store from `<dir>/local_favorites.db`. Called on every
/// session activation (login, restore, offline entry), next to
/// `crate::pinned::init_for_user`. Fail-open on error (nothing favorited).
pub fn init_for_user(base_dir: &Path) {
    let db_path = base_dir.join(DB_FILE_NAME);
    match LocalFavoritesService::new(&db_path) {
        Ok(service) => {
            if let Ok(mut guard) = SERVICE.lock() {
                *guard = Some(service);
            }
        }
        Err(e) => log::error!("[qbz-slint] local favorites store open failed: {e}"),
    }
}

/// Drop the per-user store on logout.
pub fn teardown() {
    if let Ok(mut guard) = SERVICE.lock() {
        *guard = None;
    }
}

fn with_service<T>(default: T, f: impl FnOnce(&LocalFavoritesService) -> T) -> T {
    SERVICE
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(f))
        .unwrap_or(default)
}

/// True when the local `(kind, id)` item is favorited. Fail-open `false`.
pub fn is_favorite(kind: &str, id: &str) -> bool {
    with_service(false, |s| s.is_favorite(kind, id))
}

/// All local favorites, newest first (for the mixed feed loader).
pub fn list() -> Vec<LocalFavItem> {
    with_service(Vec::new(), |s| s.list().unwrap_or_default())
}

/// Per-artist favorite counts, for the ArtistPage library toggle.
pub fn count_by_artist() -> Vec<(String, i64)> {
    with_service(Vec::new(), |s| s.count_by_artist().unwrap_or_default())
}

/// Snapshot of the full `(kind, id)` key set, for bulk card stamping.
#[allow(dead_code)]
pub fn keys_snapshot() -> HashSet<(String, String)> {
    with_service(HashSet::new(), |s| s.keys_snapshot())
}

fn mutate(f: impl FnOnce(&LocalFavoritesService) -> Result<(), String>) -> Result<(), String> {
    match SERVICE.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(service) => f(service),
            None => Err(NO_SESSION_ERR.into()),
        },
        Err(_) => Err(NO_SESSION_ERR.into()),
    }
}

/// Favorite a local item (upsert).
pub fn favorite(item: &LocalFavItem) -> Result<(), String> {
    mutate(|s| s.favorite(item))
}

/// Unfavorite a local item. Absent rows are Ok.
pub fn unfavorite(kind: &str, id: &str) -> Result<(), String> {
    mutate(|s| s.unfavorite(kind, id))
}

/// Toggle: returns the NEW favorite state (`true` = now favorited).
pub fn toggle(item: &LocalFavItem) -> Result<bool, String> {
    if is_favorite(&item.kind, &item.id) {
        unfavorite(&item.kind, &item.id)?;
        Ok(false)
    } else {
        favorite(item)?;
        Ok(true)
    }
}
