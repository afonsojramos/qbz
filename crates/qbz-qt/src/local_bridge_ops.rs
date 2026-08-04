//! QbzLocal bridge OPS — the publish + load orchestration behind the
//! `local_bridge.rs` invokables.
//!
//! Split out of `local_bridge.rs` so the bridge file stays what it is meant
//! to be: ONE `#[cxx_qt::bridge]` declaration plus the thin invokable
//! forwards (cxx-qt requires the bridge module to live alone in a FLAT
//! `src/` file — QTBUG-93443). Everything here is a plain free function
//! that hops to the Qt thread through `local_bridge::ui`, so it carries no
//! cxx-qt constraints.
//!
//! Every loader publishes its own tab's document + the shared badges; the
//! Plex helpers publish the gates and drive the manual sync.

use cxx_qt_lib::QString;

use crate::local_bridge::ui;
use crate::local_library_qt as lib;
use crate::local_plex as plex;

/// Republish the tab badges + availability after any load.
pub(crate) fn publish_counts() {
    let counts = lib::to_json(&lib::counts());
    ui(move |mut b| {
        b.as_mut()
            .set_local_counts_json(QString::from(counts.as_str()));
    });
}

pub(crate) fn publish_tree(nodes_json: String) {
    ui(move |mut b| {
        b.as_mut()
            .set_local_tree_json(QString::from(nodes_json.as_str()));
        b.as_mut().set_local_tree_loading(false);
    });
}

// ---------------------------------------------------------------------------
// Plex helpers
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct PlexSectionRow {
    key: String,
    title: String,
    selected: bool,
}

/// Read the Plex gates + cached sections off the Qt thread and publish them.
pub(crate) fn publish_plex_state() {
    crate::spawn(async move {
        let snapshot = tokio::task::spawn_blocking(|| {
            let (sections, selected) = plex::cached_sections();
            let rows: Vec<PlexSectionRow> = sections
                .into_iter()
                .map(|s| PlexSectionRow {
                    selected: selected.iter().any(|k| *k == s.key),
                    key: s.key,
                    title: s.title,
                })
                .collect();
            (
                plex::is_enabled(),
                plex::is_configured(),
                serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string()),
            )
        })
        .await;
        let Ok((enabled, available, sections_json)) = snapshot else {
            return;
        };
        ui(move |mut b| {
            b.as_mut().set_plex_enabled(enabled);
            b.as_mut().set_plex_available(available);
            b.as_mut()
                .set_plex_sections_json(QString::from(sections_json.as_str()));
        });
    });
}

/// Reload every browse document in place (after a sync / connect / toggle).
pub(crate) fn reload_browse() {
    // The cortinilla's instant-paint cache embeds LOCAL rows and their artwork
    // paths, so anything that reaches here has just made those rows possibly
    // wrong. This is the single chokepoint for local-library mutations, which
    // is why the invalidation lives here rather than at each call site.
    crate::search_cache_qt::invalidate();
    load_albums();
    load_artists();
    load_tracks(true);
}

/// The sync body shared by the header button, `plex_connect` and
/// `set_plex_sections`.
pub(crate) fn run_sync() {
    ui(|mut b| {
        b.as_mut().set_plex_syncing(true);
        b.as_mut().set_plex_error(QString::from(""));
    });
    crate::spawn(async move {
        let result = plex::sync_now().await;
        match result {
            Ok(total) => {
                log::info!("[qbz-qt] plex sync finished: {total} tracks");
                ui(move |mut b| {
                    b.as_mut().set_plex_last_sync_tracks(total as i32);
                    b.as_mut().set_plex_syncing(false);
                });
            }
            Err(e) => {
                log::warn!("[qbz-qt] plex sync failed: {e}");
                ui(move |mut b| {
                    b.as_mut().set_plex_error(QString::from(e.as_str()));
                    b.as_mut().set_plex_syncing(false);
                });
            }
        }
        publish_plex_state();
        reload_browse();
    });
}

/// One resolved cover -> the id-keyed signal. A plain `fn` (not a closure) so
/// `local_artwork::stream_cold` can take it as a fn pointer and call it from
/// the blocking pool the moment each thumbnail lands.
pub(crate) fn emit_artwork_one(key: String, path: String) {
    ui(move |mut b| {
        b.as_mut()
            .local_artwork_ready(QString::from(key.as_str()), QString::from(path.as_str()));
    });
}

pub(crate) fn emit_artwork(pairs: Vec<(String, String)>) {
    for (key, path) in pairs {
        emit_artwork_one(key, path);
    }
}

// ---------------------------------------------------------------------------
// Loaders (each publishes its own tab's document + the shared badges)
// ---------------------------------------------------------------------------

pub(crate) fn load_tab_impl(tab: String) {
    match tab.as_str() {
        "albums" => load_albums(),
        "artists" => load_artists(),
        "folders" => load_folders(),
        "tracks" => load_tracks(true),
        _ => log::warn!("[qbz-qt] local: unknown tab {tab}"),
    }
}

pub(crate) fn load_albums() {
    ui(|mut b| {
        b.as_mut().set_local_albums_loading(true);
        b.as_mut().set_local_albums_error(QString::from(""));
    });
    crate::spawn(async move {
        let result = tokio::task::spawn_blocking(lib::load_albums_blocking)
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        let _ = tokio::task::spawn_blocking(lib::load_counts_blocking).await;
        match result {
            Ok(rows) => {
                let json = lib::to_json(&rows);
                log::info!("[qbz-qt] local albums published: {} ({} bytes)", rows.len(), json.len());
                ui(move |mut b| {
                    b.as_mut()
                        .set_local_albums_json(QString::from(json.as_str()));
                    b.as_mut().set_local_albums_loading(false);
                });
            }
            Err(e) => {
                log::warn!("[qbz-qt] local albums load failed: {e}");
                ui(move |mut b| {
                    b.as_mut()
                        .set_local_albums_error(QString::from(e.as_str()));
                    b.as_mut().set_local_albums_loading(false);
                });
            }
        }
        publish_counts();
    });
}

/// Album identity changed: the Artists tab's album cache is keyed by the group
/// key, so it is stale the moment the mode flips (PARITY-DEBT #9 — a
/// folder-mode compilation cross-lists under every artist). Drop the cached
/// document, then re-run the load so the tab is correct even when the user is
/// standing on it. Port of `local_library.rs:727-738 invalidate_artists` (the
/// reference can stop at the drop: its `ensure_artists_loaded` guard re-fetches
/// on the next visit, a guard this port does not have).
pub(crate) fn invalidate_artists() {
    lib::state(|s| {
        s.artists.clear();
    });
    ui(|mut b| b.as_mut().set_local_artists_json(QString::from("[]")));
    load_artists();
}

pub(crate) fn load_artists() {
    ui(|mut b| b.as_mut().set_local_artists_loading(true));
    crate::spawn(async move {
        let rows = tokio::task::spawn_blocking(lib::load_artists_blocking)
            .await
            .unwrap_or_else(|e| Err(e.to_string()))
            .unwrap_or_default();
        let json = lib::to_json(&rows);
        ui(move |mut b| {
            b.as_mut()
                .set_local_artists_json(QString::from(json.as_str()));
            b.as_mut().set_local_artists_loading(false);
        });
        publish_counts();
    });
}

pub(crate) fn load_folders() {
    ui(|mut b| {
        b.as_mut().set_local_folders_loading(true);
        b.as_mut().set_local_tree_loading(true);
    });
    crate::spawn(async move {
        let rows = tokio::task::spawn_blocking(lib::load_folders_blocking)
            .await
            .unwrap_or_else(|e| Err(e.to_string()))
            .unwrap_or_default();
        let json = lib::to_json(&rows);
        ui(move |mut b| {
            b.as_mut()
                .set_local_folders_json(QString::from(json.as_str()));
            b.as_mut().set_local_folders_loading(false);
        });
        // The tree rail seeds from the registered library folders; the
        // levels below it are fetched on expand.
        let _ = tokio::task::spawn_blocking(lib::load_tree_roots_blocking).await;
        publish_tree(lib::to_json(&lib::tree_visible()));
        publish_counts();
    });
}

pub(crate) fn load_tracks(reset: bool) {
    ui(move |mut b| {
        if reset {
            b.as_mut().set_local_tracks_loading(true);
        } else {
            b.as_mut().set_local_tracks_loading_more(true);
        }
    });
    crate::spawn(async move {
        let result = tokio::task::spawn_blocking(move || lib::load_tracks_page_blocking(reset))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        let _ = tokio::task::spawn_blocking(lib::load_counts_blocking).await;
        match result {
            Ok((rows, has_more)) => {
                let json = lib::to_json(&rows);
                ui(move |mut b| {
                    b.as_mut()
                        .set_local_tracks_json(QString::from(json.as_str()));
                    b.as_mut().set_local_tracks_has_more(has_more);
                    b.as_mut().set_local_tracks_loading(false);
                    b.as_mut().set_local_tracks_loading_more(false);
                });
            }
            Err(e) => {
                log::warn!("[qbz-qt] local tracks load failed: {e}");
                ui(|mut b| {
                    b.as_mut().set_local_tracks_loading(false);
                    b.as_mut().set_local_tracks_loading_more(false);
                });
            }
        }
        publish_counts();
    });
}
