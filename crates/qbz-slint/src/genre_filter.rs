//! Filter-by-genre controller.
//!
//! Loads the parent genres for the Discover popup's simple grid and
//! owns the shared genre selection (one set for all three Discover
//! tabs, matching Tauri's single "home" genre context). The selection
//! persists to `<data-dir>/qbz/genre_filter.json` when "Remember
//! selection" is on, and feeds `genre_ids` into the discover-index
//! fetch.

use std::path::PathBuf;
use std::sync::Mutex;

use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppWindow, GenreChip, GenreFilterState};

#[derive(Clone)]
struct GenreItem {
    id: u64,
    name: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Persisted {
    #[serde(default)]
    selected: Vec<u64>,
    #[serde(default = "default_true")]
    remember: bool,
}

fn default_true() -> bool {
    true
}

struct State {
    parents: Vec<GenreItem>,
    selected: Vec<u64>,
    remember: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    parents: Vec::new(),
    selected: Vec::new(),
    remember: true,
});

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("genre_filter.json"))
}

fn load_persisted() -> Persisted {
    let Some(path) = store_path() else {
        return Persisted::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Persisted::default(),
    }
}

fn save_persisted(selected: &[u64], remember: bool) {
    let Some(path) = store_path() else {
        return;
    };
    if !remember {
        // Remember off — drop any persisted selection.
        let _ = std::fs::remove_file(&path);
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let data = Persisted {
        selected: selected.to_vec(),
        remember,
    };
    if let Ok(json) = serde_json::to_vec_pretty(&data) {
        let _ = std::fs::write(&path, json);
    }
}

/// The current shared genre-id selection. Read by navigate_home to
/// pass into the discover-index fetch.
pub fn selected_ids() -> Vec<u64> {
    STATE.lock().map(|s| s.selected.clone()).unwrap_or_default()
}

/// Fetch the parent genres (if not already loaded) and seed the
/// persisted selection. Runs on a worker; call apply_state afterwards
/// on the UI thread.
pub async fn load_parents<A>(runtime: &AppRuntime<A>)
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    {
        let already = STATE.lock().map(|s| !s.parents.is_empty()).unwrap_or(false);
        if already {
            return;
        }
    }
    let persisted = load_persisted();
    let mut parents: Vec<GenreItem> = match runtime.core().get_genres(None).await {
        Ok(list) => list
            .into_iter()
            .map(|g| GenreItem {
                id: g.id,
                name: g.name,
            })
            .collect(),
        Err(e) => {
            log::warn!("[qbz-slint] genre filter: get_genres failed: {e}");
            Vec::new()
        }
    };
    parents.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // Drop any persisted ids that no longer exist.
    let valid: Vec<u64> = persisted
        .selected
        .into_iter()
        .filter(|id| parents.iter().any(|p| p.id == *id))
        .collect();

    if let Ok(mut s) = STATE.lock() {
        s.parents = parents;
        s.selected = valid;
        s.remember = persisted.remember;
    }
}

/// Push the current parents + selection into GenreFilterState. UI thread.
pub fn apply_state(window: &AppWindow) {
    let (chips, count, remember) = {
        let Ok(s) = STATE.lock() else {
            return;
        };
        let chips: Vec<GenreChip> = s
            .parents
            .iter()
            .map(|g| GenreChip {
                id: g.id.to_string().into(),
                name: g.name.clone().into(),
                selected: s.selected.contains(&g.id),
            })
            .collect();
        (chips, s.selected.len() as i32, s.remember)
    };
    let state = window.global::<GenreFilterState>();
    state.set_genres(ModelRc::new(VecModel::from(chips)));
    state.set_selected_count(count);
    state.set_remember(remember);
}

/// Toggle a genre id in the selection. Returns true if the selection
/// changed (so the caller can re-fetch).
pub fn toggle(id_str: &str) -> bool {
    let Ok(id) = id_str.parse::<u64>() else {
        return false;
    };
    let Ok(mut s) = STATE.lock() else {
        return false;
    };
    if let Some(pos) = s.selected.iter().position(|x| *x == id) {
        s.selected.remove(pos);
    } else {
        s.selected.push(id);
    }
    let (sel, rem) = (s.selected.clone(), s.remember);
    drop(s);
    save_persisted(&sel, rem);
    true
}

pub fn clear() {
    let Ok(mut s) = STATE.lock() else {
        return;
    };
    s.selected.clear();
    let rem = s.remember;
    drop(s);
    save_persisted(&[], rem);
}

pub fn set_remember(remember: bool) {
    let Ok(mut s) = STATE.lock() else {
        return;
    };
    s.remember = remember;
    let sel = s.selected.clone();
    drop(s);
    save_persisted(&sel, remember);
}
