//! Library (Favorites) toolbar prefs — the Qt half of
//! `crates/qbz/src/favorites_prefs.rs`.
//!
//! SAME document, same keys: `<data-dir>/qbz/favorites_ui.json`. That is
//! deliberate — a user who moves between the shipping Slint build and this one
//! keeps their view mode, sort direction and source toggles instead of finding
//! them reset. `locallibrary_ui.json` is co-owned exactly the same way
//! (`local_state.rs`), and this file follows ITS writer, not the Slint's:
//!
//!  - the Slint `save()` serializes the whole struct through `fs::write`
//!    (O_TRUNC — a reader that lands mid-write sees an empty file, answers
//!    `Prefs::default()` and writes those defaults back);
//!  - here every write is a read-modify-write of the parsed OBJECT, published
//!    through `settings_qt::write_json_object_atomic` (temp file + `rename(2)`),
//!    so a concurrent reader sees the whole old document or the whole new one,
//!    and any key the Slint gains later survives our writes untouched.
//!
//! `all_local_scope` is one such key: it is PARITY-DEBT #6 (the Settings row
//! and the `all` branch of the feed are not ported), so nothing here ever sets
//! it — but because the writer merges instead of replacing, a scope the user
//! picked in the Slint build is preserved rather than reset to "favorites" on
//! the first toolbar click here.
//!
//! Transport to QML is ONE json document on `QbzLibrary.libraryPrefsJson`
//! (camelCase keys, the port's QML convention), seeded at `boot()`; QML writes
//! back one key at a time through `QbzLibrary.setLibraryPref`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The on-disk shape. Field names are the SLINT's — do not rename them.
#[derive(Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default = "d_grid")]
    pub albums_view: String,
    #[serde(default = "d_default")]
    pub albums_sort: String,
    #[serde(default = "d_off")]
    pub albums_group: String,
    #[serde(default = "d_off")]
    pub tracks_group: String,
    #[serde(default = "d_grid")]
    pub playlists_view: String,
    #[serde(default)]
    pub artists_group: bool,
    #[serde(default = "d_grid")]
    pub artists_view: String,
    /// Library "All": include local files + Plex in the feed. Default ON.
    #[serde(default = "d_true")]
    pub all_show_local: bool,
    /// Library "All": what "local" means — "favorites" | "all". Read and
    /// PRESERVED here, never written (PARITY-DEBT #6).
    #[serde(default = "d_favorites")]
    pub all_local_scope: String,
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

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("favorites_ui.json"))
}

pub fn read() -> Prefs {
    let Some(path) = store_path() else {
        return Prefs::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Prefs::default(),
    }
}

/// The document QML seeds its toolbar state from (camelCase, the port's QML
/// convention — the disk keys stay snake_case for the Slint).
///
/// `allLocalScope` rides along read-only so a later round can wire the feed's
/// `all` branch without a second transport.
pub fn to_json() -> String {
    let p = read();
    serde_json::json!({
        "albumsView": p.albums_view,
        "albumsSort": p.albums_sort,
        "albumsGroup": p.albums_group,
        "tracksGroup": p.tracks_group,
        "playlistsView": p.playlists_view,
        "artistsGroup": if p.artists_group { "alpha" } else { "off" },
        "artistsView": p.artists_view,
        "allShowLocal": p.all_show_local,
        "allLocalScope": p.all_local_scope,
    })
    .to_string()
}

/// Read-modify-write ONE key. `edit` sees the prefs as they are on disk RIGHT
/// NOW (`local_state::update_prefs` shape) — the `read()` -> mutate ->
/// `write()` alternative has the torn-read hole this file exists to close.
fn update(edit: impl FnOnce(&mut Prefs)) {
    let Some(path) = store_path() else {
        return;
    };
    let Some(mut doc) = crate::settings_qt::read_json_object(&path) else {
        return;
    };
    let mut prefs: Prefs = serde_json::from_value(serde_json::Value::Object(doc.clone()))
        .unwrap_or_else(|e| {
            log::warn!("[qbz-qt] favorites_ui.json keys unreadable ({e}) — using defaults");
            Prefs::default()
        });
    edit(&mut prefs);
    if let Ok(serde_json::Value::Object(map)) = serde_json::to_value(&prefs) {
        for (key, value) in map {
            doc.insert(key, value);
        }
    }
    crate::settings_qt::write_json_object_atomic(&path, &doc);
}

/// QML setter. `key` is the camelCase name from `to_json`; `value` is always a
/// string (`"true"` / `"false"` for the two booleans) so ONE invokable covers
/// the whole toolbar.
///
/// An unknown key is logged and dropped rather than silently ignored: this is
/// the seam a typo in QML would otherwise vanish into (QML resolves lazily —
/// nothing else would ever report it).
pub fn set(key: &str, value: &str) {
    let v = value.to_string();
    match key {
        "albumsView" => update(|p| p.albums_view = v),
        "albumsSort" => update(|p| p.albums_sort = v),
        "albumsGroup" => update(|p| p.albums_group = v),
        "tracksGroup" => update(|p| p.tracks_group = v),
        "playlistsView" => update(|p| p.playlists_view = v),
        "artistsGroup" => update(|p| p.artists_group = v == "alpha" || v == "true"),
        "artistsView" => update(|p| p.artists_view = v),
        "allShowLocal" => update(|p| p.all_show_local = v == "true"),
        other => log::warn!("[qbz-qt] library pref: unknown key '{other}' (value '{value}')"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_slint_document() {
        let p = Prefs::default();
        assert_eq!(p.albums_view, "grid");
        assert_eq!(p.albums_sort, "default");
        assert_eq!(p.albums_group, "off");
        assert_eq!(p.tracks_group, "off");
        assert_eq!(p.playlists_view, "grid");
        assert!(!p.artists_group);
        assert_eq!(p.artists_view, "grid");
        assert!(p.all_show_local);
        assert_eq!(p.all_local_scope, "favorites");
    }

    #[test]
    fn a_partial_document_keeps_the_defaults_for_the_missing_keys() {
        // The Slint writes every key; a hand-edited or older file may not.
        let p: Prefs = serde_json::from_str(r#"{"albums_view":"list"}"#).expect("parses");
        assert_eq!(p.albums_view, "list");
        assert_eq!(p.albums_sort, "default");
        assert!(p.all_show_local);
    }

    #[test]
    fn the_disk_keys_stay_snake_case_for_the_slint() {
        // The Slint's favorites_prefs.rs reads these EXACT names; renaming one
        // silently splits the two frontends' stored choices.
        let json = serde_json::to_value(Prefs::default()).expect("serializes");
        for key in [
            "albums_view",
            "albums_sort",
            "albums_group",
            "tracks_group",
            "playlists_view",
            "artists_group",
            "artists_view",
            "all_show_local",
            "all_local_scope",
        ] {
            assert!(json.get(key).is_some(), "missing disk key {key}");
        }
    }
}
