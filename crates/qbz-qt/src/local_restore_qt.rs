//! Per-profile Local Library navigation preferences. The in-memory history
//! still owns Back/Forward; this small document carries browser choices over
//! a process restart without persisting the rest of the navigation stack.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static WRITE: Mutex<()> = Mutex::new(());
const MAX_STATE_BYTES: usize = 64 * 1024;

fn path() -> Option<PathBuf> {
    use qbz_app::user_data::UserDataPaths;
    // Same identity as local_state::db_path, including the guest profile.
    let user = UserDataPaths::load_last_user_id().unwrap_or(0);
    Some(
        UserDataPaths::data_dir_for(user)
            .ok()?
            .join("local_navigation_qt.json"),
    )
}

fn browser_state(json: &str) -> Option<Value> {
    if json.len() > MAX_STATE_BYTES {
        return None;
    }
    let value: Value = serde_json::from_str(json).ok()?;
    let input = value.as_object()?;
    let mut state = Map::new();
    for key in [
        "activeTab",
        "genresSearch",
        "genreYearsSearch",
        "genreArtistsSearch",
        "genreAlbumsSearch",
        "genresView",
        "genresSort",
        "explorerColumns",
    ] {
        if let Some(Value::String(text)) = input.get(key) {
            state.insert(key.into(), Value::String(text.clone()));
        }
    }
    for key in [
        "selectedGenres",
        "selectedGenreYears",
        "selectedGenreArtists",
        "selectedGenreAlbums",
    ] {
        if let Some(Value::Object(selections)) = input.get(key) {
            state.insert(
                key.into(),
                Value::Object(
                    selections
                        .iter()
                        .filter(|(_, selected)| selected.as_bool() == Some(true))
                        .map(|(key, selected)| (key.clone(), selected.clone()))
                        .collect(),
                ),
            );
        }
    }
    if let Some(Value::Bool(collapsed)) = input.get("genresBrowserCollapsed") {
        state.insert("genresBrowserCollapsed".into(), Value::Bool(*collapsed));
    }
    if state
        .get("activeTab")
        .and_then(Value::as_str)
        .is_some_and(|tab| !["genres", "albums", "artists", "folders", "tracks"].contains(&tab))
    {
        state.remove("activeTab");
    }
    Some(Value::Object(state))
}

fn save_browser_at(path: &Path, json: &str) {
    let Some(state) = browser_state(json) else {
        return;
    };
    let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(mut doc) = crate::settings_qt::read_json_object(path) else {
        return;
    };
    if doc.get("browser") != Some(&state) {
        doc.insert("browser".into(), state);
        crate::settings_qt::write_json_object_atomic(path, &doc);
    }
}

pub fn save_browser(json: &str) {
    if let Some(path) = path() {
        save_browser_at(&path, json);
    }
}

fn browser_at(path: &Path) -> Option<String> {
    let doc = crate::settings_qt::read_json_object(path)?;
    browser_state(&doc.get("browser")?.to_string()).map(|state| state.to_string())
}

pub fn browser_json() -> String {
    // A saved UI state must not trap the next boot after a crash. The file
    // remains intact; ordinary navigation can overwrite it with fresh choices.
    if crate::nav_qt::crash_level() >= 2 {
        return String::new();
    }
    path()
        .and_then(|path| browser_at(&path))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn browser_choices_survive_reopen_and_stay_in_their_profile() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("users/1/local_navigation_qt.json");
        let second = temp.path().join("users/2/local_navigation_qt.json");
        let choices = json!({"activeTab":"genres", "genresSearch":"rock",
            "genreYearsSearch":"199", "genreArtistsSearch":"band", "genreAlbumsSearch":"live",
            "selectedGenres":{"rock":true}, "selectedGenreYears":{"1994":true},
            "selectedGenreArtists":{"band":true}, "selectedGenreAlbums":{"local:a":true},
            "explorerColumns":"both", "genresView":"grid"});
        save_browser_at(&first, &choices.to_string());
        assert_eq!(
            serde_json::from_str::<Value>(&browser_at(&first).unwrap()).unwrap(),
            choices
        );
        assert!(browser_at(&second).is_none());
        save_browser_at(&second, r#"{"activeTab":"albums","selectedGenres":{}}"#);
        assert_eq!(
            serde_json::from_str::<Value>(&browser_at(&first).unwrap()).unwrap(),
            choices
        );
        save_browser_at(
            &first,
            r#"{"activeTab":"genres","selectedGenres":{},"genresSearch":""}"#,
        );
        let reopened: Value = serde_json::from_str(&browser_at(&first).unwrap()).unwrap();
        assert_eq!(reopened["selectedGenres"], json!({}));
        assert_eq!(reopened["genresSearch"], "");
    }

    #[test]
    fn bad_state_cannot_replace_choices_or_other_document_fields() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("local_navigation_qt.json");
        std::fs::write(&file, r#"{"future":42,"browser":{"activeTab":"genres"}}"#).unwrap();
        for invalid in [
            "null".to_string(),
            "[1]".into(),
            "{".into(),
            "x".repeat(MAX_STATE_BYTES + 1),
        ] {
            save_browser_at(&file, &invalid);
        }
        assert_eq!(
            browser_at(&file).as_deref(),
            Some(r#"{"activeTab":"genres"}"#)
        );
        save_browser_at(
            &file,
            r#"{"activeTab":"bad-route","selectedGenres":{"ok":true,"bad":"true"},"unrelated":7}"#,
        );
        let doc: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(doc["future"], 42);
        assert_eq!(doc["browser"], json!({"selectedGenres":{"ok":true}}));
        std::fs::write(&file, "broken document").unwrap();
        assert!(browser_at(&file).is_none());
        save_browser_at(&file, r#"{"activeTab":"albums"}"#);
        assert_eq!(std::fs::read_to_string(file).unwrap(), "broken document");
    }
}
