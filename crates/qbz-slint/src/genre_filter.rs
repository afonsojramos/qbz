//! Filter-by-genre controller.
//!
//! Loads the parent genres for the Discover popup's simple grid and
//! owns the shared genre selection (one set for all three Discover
//! tabs, matching Tauri's single "home" genre context). The selection
//! persists to `<data-dir>/qbz/genre_filter.json` when "Remember
//! selection" is on, and feeds `genre_ids` into the discover-index
//! fetch.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppWindow, GenreChip, GenreFilterState, GenreTreeRow};

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
    /// Lazily loaded children, keyed by parent id (levels 2 and 3).
    children: HashMap<u64, Vec<GenreItem>>,
    selected: Vec<u64>,
    expanded: HashSet<u64>,
    search: String,
    remember: bool,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| {
    Mutex::new(State {
        parents: Vec::new(),
        children: HashMap::new(),
        selected: Vec::new(),
        expanded: HashSet::new(),
        search: String::new(),
        remember: true,
    })
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

/// The explicitly-selected genre ids (what the user clicked).
pub fn selected_ids() -> Vec<u64> {
    STATE.lock().map(|s| s.selected.clone()).unwrap_or_default()
}

/// The ids to actually filter by: the selection plus every loaded
/// descendant of each selected genre. Selecting a parent therefore
/// covers its children/grandchildren — restoring the child-genre
/// filtering the discovery-v2 home redesign dropped (the old home
/// expanded to descendant names client-side; here we expand to
/// descendant ids for the server filter).
pub fn filter_ids() -> Vec<u64> {
    let Ok(s) = STATE.lock() else {
        return Vec::new();
    };
    let mut out: HashSet<u64> = HashSet::new();
    for id in &s.selected {
        out.insert(*id);
        collect_descendants(&s.children, *id, &mut out);
    }
    out.into_iter().collect()
}

fn collect_descendants(
    children: &HashMap<u64, Vec<GenreItem>>,
    id: u64,
    out: &mut HashSet<u64>,
) {
    if let Some(kids) = children.get(&id) {
        for kid in kids {
            if out.insert(kid.id) {
                collect_descendants(children, kid.id, out);
            }
        }
    }
}

pub fn children_loaded(id: u64) -> bool {
    STATE.lock().map(|s| s.children.contains_key(&id)).unwrap_or(false)
}

fn store_children(parent_id: u64, kids: Vec<GenreItem>) {
    if let Ok(mut s) = STATE.lock() {
        s.children.insert(parent_id, kids);
    }
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

    // Keep persisted selections as-is — they may reference child
    // genres not yet loaded (advanced view), so validating against
    // parents only would wrongly drop them.
    if let Ok(mut s) = STATE.lock() {
        s.parents = parents;
        s.selected = persisted.selected;
        s.remember = persisted.remember;
    }
}

/// Load one genre level (children of `parent_id`) and store it. No-op
/// if already loaded.
pub async fn load_children<A>(runtime: &AppRuntime<A>, parent_id: u64)
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    if children_loaded(parent_id) {
        return;
    }
    let kids: Vec<GenreItem> = match runtime.core().get_genres(Some(parent_id)).await {
        Ok(list) => list
            .into_iter()
            .map(|g| GenreItem { id: g.id, name: g.name })
            .collect(),
        Err(e) => {
            log::warn!("[qbz-slint] genre filter: get_genres({parent_id}) failed: {e}");
            Vec::new()
        }
    };
    store_children(parent_id, kids);
}

fn child_ids(parent_id: u64) -> Vec<u64> {
    STATE
        .lock()
        .ok()
        .and_then(|s| s.children.get(&parent_id).map(|k| k.iter().map(|c| c.id).collect()))
        .unwrap_or_default()
}

/// Eager-load every parent's children (level 2) so the advanced tree
/// can show child counts up front. Grandchildren stay lazy.
pub async fn load_all_parent_children<A>(runtime: &AppRuntime<A>)
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let parents: Vec<u64> = STATE
        .lock()
        .map(|s| s.parents.iter().map(|p| p.id).collect())
        .unwrap_or_default();
    for parent_id in parents {
        load_children(runtime, parent_id).await;
    }
}

/// Eager-load a genre's full descendant subtree (children +
/// grandchildren), so a selection expands correctly in filter_ids.
pub async fn load_descendants<A>(runtime: &AppRuntime<A>, id: u64)
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    load_children(runtime, id).await;
    for kid in child_ids(id) {
        load_children(runtime, kid).await;
    }
}

/// Toggle a tree node's expanded state. Returns true if it is now
/// expanded (so the caller can lazy-load its children).
pub fn toggle_expand(id_str: &str) -> bool {
    let Ok(id) = id_str.parse::<u64>() else {
        return false;
    };
    let Ok(mut s) = STATE.lock() else {
        return false;
    };
    if s.expanded.contains(&id) {
        s.expanded.remove(&id);
        false
    } else {
        s.expanded.insert(id);
        true
    }
}

pub fn set_search(query: &str) {
    if let Ok(mut s) = STATE.lock() {
        s.search = query.to_string();
    }
}

/// Push the current parents + selection + tree into GenreFilterState.
/// UI thread.
pub fn apply_state(window: &AppWindow) {
    let (chips, rows, count, remember) = {
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
        (chips, build_tree_rows(&s), s.selected.len() as i32, s.remember)
    };
    let state = window.global::<GenreFilterState>();
    state.set_genres(ModelRc::new(VecModel::from(chips)));
    state.set_tree(ModelRc::new(VecModel::from(rows)));
    state.set_selected_count(count);
    state.set_remember(remember);
}

fn tree_row(item: &GenreItem, level: i32, s: &State) -> GenreTreeRow {
    let loaded = s.children.get(&item.id);
    let count = loaded.map(|c| c.len()).unwrap_or(0);
    // Parents always have children; deeper levels show an expand
    // arrow optimistically until a load proves them empty.
    let has_children = if level == 0 {
        true
    } else if level == 1 {
        count > 0 || loaded.is_none()
    } else {
        false
    };
    GenreTreeRow {
        id: item.id.to_string().into(),
        name: item.name.clone().into(),
        level,
        selected: s.selected.contains(&item.id),
        expanded: s.expanded.contains(&item.id),
        has_children,
        count: count as i32,
    }
}

/// Flatten the genre tree into the currently-visible rows. With a
/// search query, returns a flat list of all loaded genres matching
/// the query (ignoring expansion); otherwise honors per-node
/// expansion down three levels.
fn build_tree_rows(s: &State) -> Vec<GenreTreeRow> {
    let query = s.search.trim().to_lowercase();
    let mut rows: Vec<GenreTreeRow> = Vec::new();

    if !query.is_empty() {
        let matches = |g: &GenreItem| g.name.to_lowercase().contains(&query);
        for p in &s.parents {
            if matches(p) {
                rows.push(tree_row(p, 0, s));
            }
        }
        for kids in s.children.values() {
            for k in kids {
                if matches(k) {
                    rows.push(tree_row(k, 0, s));
                }
            }
        }
        return rows;
    }

    for parent in &s.parents {
        rows.push(tree_row(parent, 0, s));
        if !s.expanded.contains(&parent.id) {
            continue;
        }
        let Some(children) = s.children.get(&parent.id) else {
            continue;
        };
        for child in children {
            rows.push(tree_row(child, 1, s));
            if !s.expanded.contains(&child.id) {
                continue;
            }
            if let Some(grandchildren) = s.children.get(&child.id) {
                for gc in grandchildren {
                    rows.push(tree_row(gc, 2, s));
                }
            }
        }
    }
    rows
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
