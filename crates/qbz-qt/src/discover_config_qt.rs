//! Discover section configurator — Slint-free port of the mutation half of
//! `crates/qbz/src/discover_prefs.rs` (`push_config_rows` /
//! `on_toggle` / `on_move` / `on_reset`).
//!
//! The gear in the Discover toolbar opens the per-tab "Customize" modal:
//! show/hide + reorder the active tab's rails. There is no Save button —
//! every toggle / move / reset persists live to the SHARED per-user
//! `discover_prefs.db` (`qbz_app::settings::discover_prefs`, the same store
//! `home_qt::load_prefs` already reads) and then re-renders the three tabs
//! from the cached candidate set, so the change lands on the next frame with
//! no network round trip.
//!
//! Transport: ONE JSON document on the bridge (`discoverConfigJson`) —
//! `{ tab, rows: [{ id, label, enabled }], enabled, total }` — parsed by
//! `qml/controls/DiscoverConfigModal.qml`.
//!
//! POC-NOTEs vs the Slint controller:
//! - Rows are filtered to the sections this port can actually RENDER
//!   (`home_qt::renderable_ids`). The Slint modal lists every pref entry
//!   because Slint implements every rail; here a reco-engine / radio /
//!   spotlight row would be a toggle that changes nothing, which is exactly
//!   the defect class this port removes.
//! - The Recommendations tab's arm of the Slint modal (explainer + cache
//!   window + "Refresh now") is NOT ported: it drives
//!   `ExternalRecoState`/`ExternalRecoActions`, and the external reco engine
//!   is out of scope. The toolbar gear is DISABLED on that tab instead of
//!   opening an empty modal (HomeView.qml).

use qbz_app::settings::discover_prefs::{
    DiscoverPrefs, DiscoverPrefsStore, DiscoverySectionId, DiscoveryTab,
};
use cxx_qt_lib::QString;
use serde::Serialize;

/// id -> label. Verbatim copy of `discover_prefs::label_for` in the Slint
/// binary crate (not reachable from here — it lives in `crates/qbz`, and the
/// headless prefs crate deliberately carries no frontend strings, ADR-006).
/// `mark` registers the msgid for the extractor; `t` does the lookup once at
/// the consumer.
fn label_for(id: DiscoverySectionId) -> &'static str {
    use DiscoverySectionId::*;
    match id {
        NewReleases => qbz_i18n::mark("New Releases"),
        PressAwards => qbz_i18n::mark("Press Accolades"),
        QobuzPlaylists => qbz_i18n::mark("Qobuz Playlists"),
        RecentlyPlayedAlbums => qbz_i18n::mark("Recently Played"),
        ContinueListening => qbz_i18n::mark("Continue Listening"),
        IdealDiscography => qbz_i18n::mark("Ideal Discography"),
        MostStreamed => qbz_i18n::mark("Most Streamed"),
        ReleaseWatch => qbz_i18n::mark("Release Watch"),
        EditorPicks => qbz_i18n::mark("Albums of the Week"),
        Qobuzissimes => qbz_i18n::mark("Qobuzissimes"),
        TopArtists => qbz_i18n::mark("Your Top Artists"),
        FavoriteAlbums => qbz_i18n::mark("Library Albums"),
        QobuzMixes => qbz_i18n::mark("Qobuz Mixes"),
        RadioStations => qbz_i18n::mark("Radio Stations"),
        SimilarAlbums => qbz_i18n::mark("More From Your Library"),
        RediscoverLibrary => qbz_i18n::mark("Rediscover Your Library"),
        EssentialsByGenre => qbz_i18n::mark("Essentials by Genre"),
        ArtistsToFollow => qbz_i18n::mark("Artists to Follow"),
        ArtistSpotlight => qbz_i18n::mark("Artist Spotlight"),
        Pinned => qbz_i18n::mark("Pinned"),
        MostPlayedAlbums => qbz_i18n::mark("Most Played Albums"),
    }
}

#[derive(Serialize)]
struct RowDoc {
    id: String,
    label: String,
    enabled: bool,
}

#[derive(Serialize)]
struct ConfigDoc {
    tab: String,
    rows: Vec<RowDoc>,
    enabled: i32,
    total: i32,
}

fn store() -> Option<DiscoverPrefsStore> {
    crate::sidebar_qt::user_dir()
        .and_then(|dir| DiscoverPrefsStore::new_at(&dir).ok())
}

/// The tab's ordered pref ids, narrowed to what this port renders.
fn visible_ids(prefs: &DiscoverPrefs, tab: DiscoveryTab) -> Vec<DiscoverySectionId> {
    let renderable = crate::home_qt::renderable_ids(tab);
    prefs
        .tab(tab)
        .iter()
        .map(|p| p.id)
        .filter(|id| renderable.is_empty() || renderable.iter().any(|r| r.as_str() == id.as_str()))
        .collect()
}

/// The modal payload for one tab.
pub(crate) fn rows_json(tab_key: &str) -> String {
    let Some(tab) = DiscoveryTab::from_key(tab_key) else {
        // Recommendations (or an unknown key): no configurable sections.
        return serde_json::to_string(&ConfigDoc {
            tab: tab_key.to_string(),
            rows: Vec::new(),
            enabled: 0,
            total: 0,
        })
        .unwrap_or_else(|_| "{}".to_string());
    };
    let prefs = crate::home_qt::load_prefs();
    let visible = visible_ids(&prefs, tab);
    let rows: Vec<RowDoc> = prefs
        .tab(tab)
        .iter()
        .filter(|p| visible.contains(&p.id))
        .map(|p| RowDoc {
            id: p.id.as_str().to_string(),
            label: qbz_i18n::t(label_for(p.id)),
            enabled: p.enabled,
        })
        .collect();
    let total = rows.len() as i32;
    let enabled = rows.iter().filter(|r| r.enabled).count() as i32;
    serde_json::to_string(&ConfigDoc {
        tab: tab_key.to_string(),
        rows,
        enabled,
        total,
    })
    .unwrap_or_else(|_| "{}".to_string())
}

fn publish(tab_key: &str) {
    let json = rows_json(tab_key);
    crate::ui(move |mut b| {
        b.as_mut()
            .set_discover_config_json(QString::from(json.as_str()));
    });
}

/// Gear click: (re)publish the rows for the tab the modal is about to show.
pub(crate) fn open(tab_key: &str) {
    publish(tab_key);
}

/// Load -> mutate -> persist -> re-render the tabs -> re-publish the rows.
fn mutate(tab_key: &str, f: impl FnOnce(&mut DiscoverPrefs, DiscoveryTab)) {
    let Some(tab) = DiscoveryTab::from_key(tab_key) else {
        return;
    };
    let Some(store) = store() else {
        log::warn!("[qbz-qt] discover config: no per-user prefs store (not logged in?)");
        return;
    };
    let mut prefs = store.load();
    f(&mut prefs, tab);
    if let Err(e) = store.save(&prefs) {
        log::warn!("[qbz-qt] discover config: save failed: {e}");
        return;
    }
    crate::home_qt::republish_cached();
    publish(tab_key);
}

pub(crate) fn toggle_section(tab_key: &str, id: &str) {
    let Some(section) = DiscoverySectionId::from_str(id) else {
        return;
    };
    mutate(tab_key, |prefs, tab| prefs.toggle(tab, section));
}

/// Move one step through the VISIBLE order (`dir` = -1 up / +1 down).
/// `DiscoverPrefs::move_section` swaps with the adjacent PREF entry, which
/// may be a row this port filtered out — the click would then look dead. So
/// the neighbour is picked from the visible list and the two entries are
/// swapped in the full list (hidden entries keep their relative order).
pub(crate) fn move_section(tab_key: &str, id: &str, dir: i32) {
    let Some(section) = DiscoverySectionId::from_str(id) else {
        return;
    };
    if dir == 0 {
        return;
    }
    mutate(tab_key, |prefs, tab| {
        let visible = visible_ids(prefs, tab);
        let Some(vi) = visible.iter().position(|v| *v == section) else {
            return;
        };
        let target = if dir < 0 {
            if vi == 0 {
                return;
            }
            visible[vi - 1]
        } else {
            if vi + 1 >= visible.len() {
                return;
            }
            visible[vi + 1]
        };
        let list = prefs.tab_mut(tab);
        let (Some(a), Some(b)) = (
            list.iter().position(|p| p.id == section),
            list.iter().position(|p| p.id == target),
        ) else {
            return;
        };
        list.swap(a, b);
    });
}

pub(crate) fn reset_tab(tab_key: &str) {
    mutate(tab_key, |prefs, tab| prefs.reset_tab(tab));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_id_has_a_label() {
        // A missing arm would not compile; this pins the lookup itself.
        for id in DiscoverPrefs::available_ids(DiscoveryTab::Home) {
            assert!(!label_for(id).is_empty());
        }
        for id in DiscoverPrefs::available_ids(DiscoveryTab::ForYou) {
            assert!(!label_for(id).is_empty());
        }
    }

    #[test]
    fn rows_json_for_an_unknown_tab_is_empty_not_broken() {
        let doc = rows_json("recommendations");
        assert!(doc.contains("\"rows\":[]"));
        assert!(doc.contains("\"total\":0"));
    }
}
