//! Persistence for the favorites toolbar choices (albums view / sort /
//! group + tracks group) across restarts. A small json under
//! `<data-dir>/qbz/favorites_ui.json`, mirroring the genre_filter store.
//! Search queries are transient and deliberately not persisted.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use slint::ComponentHandle;

use crate::{AppWindow, FavoritesState, LibraryAllState};

#[derive(Serialize, Deserialize)]
struct Prefs {
    #[serde(default = "d_grid")]
    albums_view: String,
    #[serde(default = "d_default")]
    albums_sort: String,
    #[serde(default = "d_off")]
    albums_group: String,
    #[serde(default = "d_off")]
    tracks_group: String,
    #[serde(default = "d_grid")]
    playlists_view: String,
    #[serde(default)]
    artists_group: bool,
    #[serde(default = "d_grid")]
    artists_view: String,
    // Library "All": include local files + Plex in the feed (the toolbar's
    // hard-drive filter). Default ON — users expect their local items there
    // (issuers' report).
    #[serde(default = "d_true")]
    all_show_local: bool,
    // Library "All": what "local" means in the feed — "favorites" (only
    // hearted local items, webplayer parity) or "all" (the entire local
    // library: every folder + Plex album/artist/track). Settings > Local
    // Library selector (owner 2026-07-24).
    #[serde(default = "d_favorites")]
    all_local_scope: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            albums_view: d_grid(),
            albums_sort: d_default(),
            albums_group: d_off(),
            tracks_group: d_off(),
            playlists_view: d_grid(),
            artists_group: false,
            artists_view: d_grid(),
            all_show_local: true,
            all_local_scope: d_favorites(),
        }
    }
}

fn d_grid() -> String {
    "grid".to_string()
}
fn d_default() -> String {
    "default".to_string()
}
fn d_off() -> String {
    "off".to_string()
}
fn d_true() -> bool {
    true
}
fn d_favorites() -> String {
    "favorites".to_string()
}

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("favorites_ui.json"))
}

fn read() -> Prefs {
    let Some(path) = store_path() else {
        return Prefs::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Prefs::default(),
    }
}

/// Apply the persisted toolbar choices to `FavoritesState`. UI thread.
pub fn load(window: &AppWindow) {
    let p = read();
    let st = window.global::<FavoritesState>();
    st.set_albums_view_mode(p.albums_view.into());
    st.set_albums_sort_by(p.albums_sort.into());
    st.set_albums_group_mode(p.albums_group.into());
    st.set_tracks_group_mode(p.tracks_group.into());
    st.set_playlists_view_mode(p.playlists_view.into());
    st.set_artists_group_enabled(p.artists_group);
    st.set_artists_view_mode(p.artists_view.into());
    window
        .global::<LibraryAllState>()
        .set_show_local(p.all_show_local);
    window
        .global::<LibraryAllState>()
        .set_local_scope(p.all_local_scope.clone().into());
}

/// Persist the current toolbar choices read from `FavoritesState`. Preserves
/// the Library-All local SCOPE (read-modify-write — it lives only here, not
/// in any Slint global).
pub fn save(window: &AppWindow) {
    let Some(path) = store_path() else {
        return;
    };
    let st = window.global::<FavoritesState>();
    let p = Prefs {
        albums_view: st.get_albums_view_mode().into(),
        albums_sort: st.get_albums_sort_by().into(),
        albums_group: st.get_albums_group_mode().into(),
        tracks_group: st.get_tracks_group_mode().into(),
        playlists_view: st.get_playlists_view_mode().into(),
        artists_group: st.get_artists_group_enabled(),
        artists_view: st.get_artists_view_mode().into(),
        all_show_local: window.global::<LibraryAllState>().get_show_local(),
        all_local_scope: read().all_local_scope,
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec_pretty(&p) {
        let _ = std::fs::write(&path, json);
    }
}

/// The persisted Library-All local scope: "favorites" (default) | "all".
pub fn local_scope() -> String {
    read().all_local_scope
}

/// Persist a new Library-All local scope (read-modify-write).
pub fn set_local_scope(scope: &str) {
    let mut p = read();
    p.all_local_scope = scope.to_string();
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec_pretty(&p) {
        let _ = std::fs::write(&path, json);
    }
}
