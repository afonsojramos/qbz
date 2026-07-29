//! Discover > Home data layer — a Slint-free port of the PURE mapping
//! logic of `crates/qbz/src/home.rs` (+ the personalized rails of
//! `crates/qbz/src/foryou.rs` that need only a live session).
//!
//! Produces plain data rows; serialization to the bridge happens as one
//! JSON document (`homeSectionsJson`) — see the POC-NOTE in
//! `publish_sections` for why not QVariantList-of-QVariantMap.
//!
//! POC-NOTEs (skipped vs the Slint controller):
//! - Artist/album blacklist filtering (T8): the blacklist store is not
//!   opened in this POC (phase-1 skip), so no rows are dropped.
//! - Reco-scored taste ordering of favorite albums (reco store skipped):
//!   favorites render in plain favorite order, and Rediscover falls back to
//!   the local "not in the recently-played window" heuristic (the same
//!   fallback the Slint build uses while its reco store is cold).
//! - Radio Stations (For You): the port has no album-radio invokable, so the
//!   rail is NOT built — a tile that starts nothing is worse than an absent
//!   rail. Artist Spotlight likewise has no ported panel.
//! - Qobuz Mixes renders as the four static navigation tiles (the mix DETAIL
//!   views are out of scope — tiles inert).
//! - The "View all" full-list pages are LIVE (see `browse_qt`): the generic
//!   album carousels open DiscoverBrowse for their endpoint, and the three
//!   local rails (Qobuz Playlists / Recently Played Albums / Most Played
//!   Albums) open their own pages. WHICH rails carry the link is decided in
//!   `HomeView.qml`'s `viewAllKind()` — per TAB, because the same candidate
//!   section is cloned into all three tab lists by `assemble()` and Slint's
//!   For You arm for `recentlyPlayedAlbums` has NO link while Home's does
//!   (ForYouView.slint:117 vs HomeView.slint:411). Stamping it on the
//!   candidate here would leak the link into For You.
//! - Editor's Picks / For You: RENDERING parity (same rails as the Slint
//!   descriptor arms, prefs-driven order); the Slint per-branch LAZY For You
//!   load is flattened into this one pass, so the rails that need a wave-1
//!   seed (similar albums, artists to follow) add one extra concurrent
//!   round trip to the home load.
//! - Recommendations tab: ported in `recommendations_qt` (this module only
//!   supplies the shared `HomeSection`/`HomeCard` transport).

use std::sync::{Arc, Mutex};

use cxx_qt_lib::QString;
use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use qbz_models::{
    Album, AlbumAward, Artist, DiscoverAlbum, DiscoverAudioInfo, DiscoverContainer,
    DiscoverPlaylist,
};
use serde::Serialize;

/// One card row in a section rail (superset of home.rs's CardData /
/// SlimData / PlaylistCardData / foryou ArtistSlim — unused fields stay
/// empty per section kind).
#[derive(Clone, Default, Serialize)]
pub struct HomeCard {
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Artist rails only: the muted second line ArtistCard renders under the
    /// name (the reco "Similar to X, Y" caption). Omitted from the wire when
    /// empty — every QML reader is `|| ""`-guarded.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subtitle: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub genre: String,
    pub year: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityLabel")]
    pub quality_label: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub ribbon: String,
    #[serde(rename = "ribbonKind")]
    pub ribbon_kind: String,
    #[serde(rename = "isPinned", default)]
    pub is_pinned: bool,
    /// Slim rows ("Popular albums"): the 1-based rank ("" = none).
    pub rank: String,
    /// Local play count. NON-ZERO ONLY on the Most Played Albums page —
    /// `discover/AlbumCard.slint:508` renders `@tr("{} plays")` when it is,
    /// which is the whole reason that page's card is 286px and every other
    /// surface's is 266px. Omitted from the wire when 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub plays: u32,
    /// Pinned rail only: the card's own kind ("album" | "artist" |
    /// "playlist") — the mixed PinnedCarousel slot dispatch.
    #[serde(rename = "itemKind", default)]
    pub item_kind: String,
    /// Playlist rows: the UPPERCASE first-tag category subtag.
    pub category: String,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    /// `file://<cached path>` when already on disk ("" = needs download).
    #[serde(rename = "artPath")]
    pub art_path: String,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Clone, Serialize)]
pub struct HomeSection {
    pub id: String,
    pub title: String,
    /// "album" | "slim" | "slimTracks" | "playlist" | "artists" | "pinned" |
    /// "mixes" | "recentPlaceholder". `slim` rows are ALBUMS (click opens the
    /// album); `slimTracks` rows are TRACKS (click plays) — same card, two
    /// activations, so the kind carries the difference.
    pub kind: String,
    /// Placeholder hint for recentPlaceholder sections.
    pub hint: String,
    /// Discover endpoint path for the "View all" header link ("" = no
    /// full-list page — home.rs SectionData.endpoint).
    #[serde(default)]
    pub endpoint: String,
    pub items: Vec<HomeCard>,
}

/// Push one album-carousel section from its discover container (drops
/// empty/missing containers, like home.rs `push_section`).
fn push_albums(
    out: &mut Vec<HomeSection>,
    id: &str,
    title: String,
    endpoint: &str,
    container: Option<DiscoverContainer<DiscoverAlbum>>,
) {
    let Some(container) = container else {
        return;
    };
    if container.data.items.is_empty() {
        return;
    }
    out.push(HomeSection {
        id: id.to_string(),
        title,
        kind: "album".to_string(),
        hint: String::new(),
        endpoint: endpoint.to_string(),
        items: container.data.items.into_iter().map(map_album).collect(),
    });
}

/// The three Discover tab section sets (phase 13) — Home / Editor's Picks /
/// For You, each ordered + gated by its OWN prefs list (the Slint
/// `DiscoverState.home-sections` / `editor-sections` / `foryou-sections`
/// descriptors, all driven by discover_prefs.db).
pub struct DiscoverSections {
    pub home: Vec<HomeSection>,
    pub editor: Vec<HomeSection>,
    pub for_you: Vec<HomeSection>,
}

/// The personalized (non-discover-index) rails, resolved alongside the index.
/// Every one of them self-hides while empty, 1:1 with the Slint arms.
#[derive(Default)]
pub struct Personalized {
    /// "Library Albums" — the user's favorite albums.
    pub favorite_albums: Vec<HomeCard>,
    /// "Release Watch" — /release/watch.
    pub release_watch: Vec<HomeCard>,
    /// "Your Top Artists" — favorite artists.
    pub top_artists: Vec<HomeCard>,
    /// "Rediscover Your Library" — favorites absent from the recent window.
    pub rediscover: Vec<HomeCard>,
    /// "Artists to Follow" — similar artists off the favorite-artist seeds.
    pub to_follow: Vec<HomeCard>,
    /// "More From Your Library" / "Similar to {seed}" — /album/suggest.
    pub similar: Vec<HomeCard>,
    /// The localized title for `similar` (it names its seed album).
    pub similar_title: String,
}

/// All sections any Discover tab can render, in construction order (the
/// per-tab assembly clones from here). Ids are the DiscoverySectionId keys;
/// "mostStreamed#album" is the EDITOR-tab variant (album carousel — the
/// Home tab renders the same data as the "Popular albums" slim grid).
fn build_candidates(
    containers: qbz_models::DiscoverContainers,
    p: Personalized,
) -> Vec<HomeSection> {
    let Personalized {
        favorite_albums,
        release_watch,
        top_artists,
        rediscover,
        to_follow,
        similar,
        similar_title,
    } = p;
    let mut out: Vec<HomeSection> = Vec::new();

    push_albums(&mut out, "newReleases", qbz_i18n::t("New Releases"), "/discover/newReleases", containers.new_releases);
    push_albums(&mut out, "pressAwards", qbz_i18n::t("Press Accolades"), "/discover/pressAward", containers.press_awards);

    // Pinned rail (phase 11) — the user's mixed pinned albums/artists/
    // playlists from the shared per-user store. Self-hides while empty.
    let pinned: Vec<HomeCard> = crate::sidebar_qt::list_pinned()
        .into_iter()
        .map(|p| HomeCard {
            id: p.id,
            title: p.title,
            artist: p.subtitle,
            art_url: p.artwork_url,
            item_kind: p.kind,
            is_pinned: true,
            ..Default::default()
        })
        .collect();
    if !pinned.is_empty() {
        out.push(HomeSection {
            id: "pinned".to_string(),
            title: qbz_i18n::t("Pinned"),
            kind: "pinned".to_string(),
            hint: String::new(),
            endpoint: String::new(),
            items: pinned,
        });
    }

    // Qobuz Mixes (For You) — four STATIC navigation tiles (QobuzMixesRow:
    // no per-tile data, always rendered when the pref is on).
    out.push(HomeSection {
        id: "qobuzMixes".to_string(),
        title: qbz_i18n::t("Qobuz Mixes"),
        kind: "mixes".to_string(),
        hint: String::new(),
        endpoint: String::new(),
        items: Vec::new(),
    });

    // Qobuz Playlists row (single-cover cards, first-tag category subtag).
    let playlists: Vec<HomeCard> = containers
        .playlists
        .map(|c| c.data.items)
        .unwrap_or_default()
        .into_iter()
        .take(40)
        .map(map_playlist)
        .collect();
    if !playlists.is_empty() {
        out.push(HomeSection {
            id: "qobuzPlaylists".to_string(),
            title: qbz_i18n::t("Qobuz Playlists"),
            kind: "playlist".to_string(),
            hint: String::new(),
            // The Slint playlist "View all" opens the local browse page (an
            // action, not a discover endpoint) — no header link here.
            endpoint: String::new(),
            items: playlists,
        });
    }

    // Recently-played rails — the SHARED local history file
    // (`recently_qt`, the same `recently_played.json` the Slint build
    // writes). With data they are a real album carousel / slim TRACK
    // carousel; empty they keep the Slint placeholders, which the For You
    // assembly drops (its arms self-hide instead).
    let recent_albums: Vec<HomeCard> = crate::recently_qt::load_albums()
        .into_iter()
        .map(map_recent_album)
        .collect();
    if recent_albums.is_empty() {
        out.push(HomeSection {
            id: "recentlyPlayedAlbums".to_string(),
            title: qbz_i18n::t("Recently Played Albums"),
            kind: "recentPlaceholder".to_string(),
            hint: qbz_i18n::t("Albums you play will appear here."),
            endpoint: String::new(),
            items: Vec::new(),
        });
    } else {
        out.push(HomeSection {
            id: "recentlyPlayedAlbums".to_string(),
            title: qbz_i18n::t("Recently Played Albums"),
            kind: "album".to_string(),
            hint: String::new(),
            // Slint's "View all" here opens a LOCAL page (not a /discover
            // endpoint); that page is not ported, so no header link.
            endpoint: String::new(),
            items: recent_albums,
        });
    }
    let recent_tracks: Vec<HomeCard> = crate::recently_qt::load_tracks()
        .into_iter()
        .take(24)
        .map(map_recent_track)
        .collect();
    if recent_tracks.is_empty() {
        out.push(HomeSection {
            id: "continueListening".to_string(),
            title: qbz_i18n::t("Recently Played Tracks"),
            kind: "recentPlaceholder".to_string(),
            hint: qbz_i18n::t("Tracks you play will appear here."),
            endpoint: String::new(),
            items: Vec::new(),
        });
    } else {
        out.push(HomeSection {
            id: "continueListening".to_string(),
            title: qbz_i18n::t("Recently Played Tracks"),
            kind: "slimTracks".to_string(),
            hint: String::new(),
            endpoint: String::new(),
            items: recent_tracks,
        });
    }

    // Most Played Albums — ranked by LOCAL play count
    // (`album_play_history`, the shared per-app SQLite store). No network.
    let most_played: Vec<HomeCard> = qbz_app::settings::album_play_history::top_albums(20)
        .into_iter()
        .map(|r| HomeCard {
            is_pinned: crate::sidebar_qt::is_pinned("album", &r.album_id),
            id: r.album_id,
            title: r.title,
            artist: r.artist,
            artist_id: r.artist_id,
            year: r.year,
            quality_tier: r.quality_tier,
            quality_label: r.quality_label,
            art_url: r.artwork_url,
            ..HomeCard::default()
        })
        .collect();
    if !most_played.is_empty() {
        out.push(HomeSection {
            id: "mostPlayedAlbums".to_string(),
            title: qbz_i18n::t("Most Played Albums"),
            kind: "album".to_string(),
            hint: String::new(),
            endpoint: String::new(),
            items: most_played,
        });
    }

    push_albums(
        &mut out,
        "idealDiscography",
        qbz_i18n::t("Ideal Discography"),
        "/discover/idealDiscography",
        containers.ideal_discography,
    );

    // Most Streamed: Home renders the "Popular albums" slim grid (capped
    // 24, 1-based ranked); the Editor's Picks tab renders the SAME data as
    // a plain album carousel (HomeView.slint's generic carousel arm).
    let streamed: Vec<qbz_models::DiscoverAlbum> = containers
        .most_streamed
        .map(|container| container.data.items)
        .unwrap_or_default();
    let popular: Vec<HomeCard> = streamed
        .iter()
        .take(24)
        .cloned()
        .enumerate()
        .map(|(index, album)| map_slim(index, album))
        .collect();
    if !popular.is_empty() {
        out.push(HomeSection {
            id: "mostStreamed".to_string(),
            title: qbz_i18n::t("Popular albums"),
            kind: "slim".to_string(),
            hint: String::new(),
            endpoint: "/discover/mostStreamed".to_string(),
            items: popular,
        });
    }
    let streamed_albums: Vec<HomeCard> = streamed.iter().map(|a| map_album(a.clone())).collect();
    if !streamed_albums.is_empty() {
        out.push(HomeSection {
            id: "mostStreamed#album".to_string(),
            title: qbz_i18n::t("Most Streamed"),
            kind: "album".to_string(),
            hint: String::new(),
            endpoint: "/discover/mostStreamed".to_string(),
            items: streamed_albums,
        });
    }

    push_albums(
        &mut out,
        "editorPicks",
        qbz_i18n::t("Albums of the Week"),
        "/discover/albumOfTheWeek",
        containers.album_of_the_week,
    );
    push_albums(&mut out, "qobuzissimes", qbz_i18n::t("Qobuzissimes"), "/discover/qobuzissims", containers.qobuzissims);

    // Personalized rails (self-hide while empty, 1:1 Slint).
    if !favorite_albums.is_empty() {
        out.push(HomeSection {
            id: "favoriteAlbums".to_string(),
            title: qbz_i18n::t("Library Albums"),
            kind: "album".to_string(),
            hint: String::new(),
            endpoint: String::new(),
            items: favorite_albums,
        });
    }
    if !release_watch.is_empty() {
        out.push(HomeSection {
            id: "releaseWatch".to_string(),
            title: qbz_i18n::t("Release Watch"),
            kind: "album".to_string(),
            hint: String::new(),
            endpoint: String::new(),
            items: release_watch,
        });
    }
    if !top_artists.is_empty() {
        out.push(HomeSection {
            id: "topArtists".to_string(),
            title: qbz_i18n::t("Your Top Artists"),
            kind: "artists".to_string(),
            hint: String::new(),
            endpoint: String::new(),
            items: top_artists,
        });
    }
    // For You: "More From Your Library" (/album/suggest, seeded by the most
    // recent — else the first favorite — album; the title names its seed,
    // 1:1 with the Slint `apply_more_from_library`).
    if !similar.is_empty() {
        out.push(HomeSection {
            id: "similarAlbums".to_string(),
            title: similar_title,
            kind: "album".to_string(),
            hint: String::new(),
            endpoint: String::new(),
            items: similar,
        });
    }
    if !rediscover.is_empty() {
        out.push(HomeSection {
            id: "rediscoverLibrary".to_string(),
            title: qbz_i18n::t("Rediscover Your Library"),
            kind: "album".to_string(),
            hint: String::new(),
            endpoint: String::new(),
            items: rediscover,
        });
    }
    if !to_follow.is_empty() {
        out.push(HomeSection {
            id: "artistsToFollow".to_string(),
            title: qbz_i18n::t("Artists to Follow"),
            kind: "artists".to_string(),
            hint: String::new(),
            endpoint: String::new(),
            items: to_follow,
        });
    }

    out
}

/// Assemble one tab's render list from the candidates: the tab's ENABLED
/// pref ids in pref order (a disabled entry hides the section; a pref id
/// the POC does not implement is skipped). `most_streamed_variant` picks
/// the slim (Home) or album (Editor's Picks) candidate for the
/// "mostStreamed" pref id. When `include_tail` (Home only), candidates the
/// prefs know NOTHING about append at the end (phase-11 behavior);
/// tab-specific "#" variants never leak through the tail.
///
/// The tail is measured against ALL THREE tab lists, not just this tab's: a
/// section that another tab owns but this one deliberately omits (For You's
/// `similarAlbums` / `rediscoverLibrary` / `artistsToFollow`) is an
/// intentional absence, and appending it here would silently graft For You
/// rails onto Home. This is the port's equivalent of the Slint
/// `HOME_RENDERABLE` guard.
fn order_by_prefs(
    candidates: &[HomeSection],
    prefs: &qbz_app::settings::discover_prefs::DiscoverPrefs,
    tab: qbz_app::settings::discover_prefs::DiscoveryTab,
    most_streamed_variant: &str,
    include_tail: bool,
) -> Vec<HomeSection> {
    use qbz_app::settings::discover_prefs::DiscoveryTab;
    let tab_prefs = prefs.tab(tab);
    let mut gated: Vec<HomeSection> = Vec::new();
    for pref in tab_prefs {
        if !pref.enabled {
            continue;
        }
        let key = if pref.id.as_str() == "mostStreamed" {
            most_streamed_variant
        } else {
            pref.id.as_str()
        };
        if let Some(section) = candidates.iter().find(|s| s.id == key) {
            gated.push(section.clone());
        }
    }
    if include_tail {
        let known: std::collections::HashSet<&str> = [
            DiscoveryTab::Home,
            DiscoveryTab::EditorPicks,
            DiscoveryTab::ForYou,
        ]
        .into_iter()
        .flat_map(|t| prefs.tab(t).iter().map(|p| p.id.as_str()))
        .collect();
        for s in candidates {
            if !s.id.contains('#') && !known.contains(s.id.as_str()) && !gated.iter().any(|g| g.id == s.id) {
                gated.push(s.clone());
            }
        }
    }
    gated
}

/// The persisted per-tab section prefs (order + visibility) — the store the
/// Discover configurator mutates (`discover_config_qt`).
pub(crate) fn load_prefs() -> qbz_app::settings::discover_prefs::DiscoverPrefs {
    crate::sidebar_qt::user_dir()
        .and_then(|dir| {
            qbz_app::settings::discover_prefs::DiscoverPrefsStore::new_at(&dir).ok()
        })
        .map(|store| store.load())
        .unwrap_or_else(qbz_app::settings::discover_prefs::default_prefs)
}

/// Last fetched candidate set, kept so a configurator mutation (show/hide,
/// reorder, reset) re-renders the three tabs INSTANTLY from cache instead of
/// re-hitting /discover/index — the Slint configurator re-renders from its
/// own section cache the same way (`home::rerender_active_tab`).
static CANDIDATES: Mutex<Vec<HomeSection>> = Mutex::new(Vec::new());

/// Assemble the three tab render lists from a candidate set + prefs.
fn assemble(
    candidates: &[HomeSection],
    prefs: &qbz_app::settings::discover_prefs::DiscoverPrefs,
) -> DiscoverSections {
    use qbz_app::settings::discover_prefs::DiscoveryTab;
    let home = order_by_prefs(candidates, prefs, DiscoveryTab::Home, "mostStreamed", true);
    let editor = order_by_prefs(candidates, prefs, DiscoveryTab::EditorPicks, "mostStreamed#album", false);
    // For You: the local-history arms (recentPlaceholder) self-hide on
    // empty data in Slint — drop them here (local store out of scope).
    let for_you: Vec<HomeSection> = order_by_prefs(candidates, prefs, DiscoveryTab::ForYou, "mostStreamed", false)
        .into_iter()
        .filter(|s| s.kind != "recentPlaceholder")
        .collect();
    DiscoverSections {
        home,
        editor,
        for_you,
    }
}

/// The SET of section ids the POC can actually RENDER for a tab — the
/// configurator's row filter (the row ORDER comes from the prefs, not from
/// here). A pref entry whose rail this port
/// never builds (radio / spotlight / reco-engine rows, see the module docs)
/// is dropped instead of listed as a toggle that changes nothing. Returns
/// EMPTY before the first fetch (nothing is known to render yet) — the
/// caller then falls back to the full pref list so the modal is never blank.
pub(crate) fn renderable_ids(tab: qbz_app::settings::discover_prefs::DiscoveryTab) -> Vec<String> {
    use qbz_app::settings::discover_prefs::DiscoveryTab;
    let Ok(candidates) = CANDIDATES.lock() else {
        return Vec::new();
    };
    if candidates.is_empty() {
        return Vec::new();
    }
    let variant = if matches!(tab, DiscoveryTab::EditorPicks) {
        "mostStreamed#album"
    } else {
        "mostStreamed"
    };
    let mut out: Vec<String> = Vec::new();
    for section in candidates.iter() {
        // The For You tab drops the local-history placeholders (see assemble).
        if matches!(tab, DiscoveryTab::ForYou) && section.kind == "recentPlaceholder" {
            continue;
        }
        if section.id == variant {
            out.push("mostStreamed".to_string());
        } else if !section.id.contains('#') {
            out.push(section.id.clone());
        }
    }
    // Both "mostStreamed" candidates map to the SAME pref id, so the Editor
    // pass can emit it twice. The caller only does membership tests, so the
    // set is sorted + deduped rather than kept in candidate order.
    out.sort();
    out.dedup();
    out
}

/// Fill `art_path` from the disk cache for a BARE card list and return the
/// urls still missing. `artwork_qt::attach_cached` only speaks
/// `[HomeSection]`, and every full-list page (`browse_qt`, `label_qt`)
/// carries plain `Vec<HomeCard>` — wrapping here keeps ONE artwork path for
/// the rails and the pages instead of a second copy of the cache lookup.
pub(crate) fn attach_card_art(cards: &mut Vec<HomeCard>) -> Vec<String> {
    let mut wrap = vec![HomeSection {
        id: String::new(),
        title: String::new(),
        kind: "album".to_string(),
        hint: String::new(),
        endpoint: String::new(),
        items: std::mem::take(cards),
    }];
    let missing = crate::artwork_qt::attach_cached(&mut wrap);
    *cards = wrap.pop().map(|s| s.items).unwrap_or_default();
    missing
}

/// Serialize + push the three tab documents onto the home bridge.
pub(crate) fn publish(sections: &DiscoverSections) {
    let home_json = serde_json::to_string(&sections.home).unwrap_or_else(|_| "[]".to_string());
    let editor_json = serde_json::to_string(&sections.editor).unwrap_or_else(|_| "[]".to_string());
    let for_you_json = serde_json::to_string(&sections.for_you).unwrap_or_else(|_| "[]".to_string());
    crate::home_bridge::ui(move |mut b| {
        b.as_mut()
            .set_home_sections_json(QString::from(home_json.as_str()));
        b.as_mut()
            .set_editor_sections_json(QString::from(editor_json.as_str()));
        b.as_mut()
            .set_for_you_sections_json(QString::from(for_you_json.as_str()));
    });
}

/// Re-render + republish the three tabs from the CACHED candidates and the
/// freshly persisted prefs. The configurator's post-mutation hook: no
/// network, so a toggle / reorder lands on the next frame. A section that
/// was disabled at fetch time may still have uncached covers — those
/// download in the background and trigger one more publish, exactly like the
/// initial load.
pub(crate) fn republish_from_prefs() {
    let candidates = match CANDIDATES.lock() {
        Ok(c) if !c.is_empty() => c.clone(),
        // Nothing fetched yet — the next load_home publishes with the new prefs.
        _ => return,
    };
    let mut sections = assemble(&candidates, &load_prefs());
    let mut missing = crate::artwork_qt::attach_cached(&mut sections.home);
    missing.extend(crate::artwork_qt::attach_cached(&mut sections.editor));
    missing.extend(crate::artwork_qt::attach_cached(&mut sections.for_you));
    missing.dedup();
    publish(&sections);
    if !missing.is_empty() {
        crate::spawn(async move {
            crate::artwork_qt::download_missing(missing).await;
            let mut sections = sections;
            let _ = crate::artwork_qt::attach_cached(&mut sections.home);
            let _ = crate::artwork_qt::attach_cached(&mut sections.editor);
            let _ = crate::artwork_qt::attach_cached(&mut sections.for_you);
            publish(&sections);
        });
    }
}

/// Fetch the discover index — honoring the shared "discover" genre selection
/// (`genre_filter_qt`), 1:1 with the Slint `current_genre_filter()` — and the
/// personalized rails concurrently (mirrors home.rs's `join!`), then map
/// everything into plain rows.
pub async fn load_home<A>(runtime: &Arc<AppRuntime<A>>) -> Result<DiscoverSections, String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    // The RAW genre selection (parent OR sub-genre ids, exactly as toggled)
    // goes straight to /discover/index — Qobuz facets sub-genre ids
    // server-side; mapping them up to an ancestor silently widens the filter.
    let genre_ids = crate::genre_filter_qt::discover_genre_ids();
    // First point in this port that runs with a live session: load the genre
    // list in the background so the popup opens instantly and a REMEMBERED
    // Library > All selection resolves to names (see warm_up's docs).
    crate::genre_filter_qt::warm_up();
    // Wave 1 — the index plus everything that needs no seed. The favorites
    // are fetched ONCE each and feed several rails (Library Albums +
    // Rediscover; Top Artists + Artists to Follow).
    let (response, fav_albums, release_watch, fav_artists) = tokio::join!(
        runtime.core().get_discover_index(genre_ids),
        fetch_fav_albums(runtime),
        fetch_release_watch(runtime),
        fetch_fav_artists(runtime),
    );
    let response = response.map_err(|e| e.to_string())?;

    // Wave 2 — the two rails that need a wave-1 (or local-history) seed.
    // Both are silent when their seed is missing: no favorites and no
    // history means no request at all.
    let recent_albums = crate::recently_qt::load_albums();
    // `/album/suggest` only resolves QOBUZ album ids, so a locally-played or
    // Plex album is not eligible as the seed (the same source guard the Slint
    // Radio rail applies; empty source = legacy entry, treated as Qobuz).
    let seed: Option<(String, String)> = recent_albums
        .iter()
        .find(|a| a.source.is_empty() || a.source.eq_ignore_ascii_case("qobuz"))
        .map(|a| (a.id.clone(), a.title.clone()))
        .or_else(|| fav_albums.first().map(|a| (a.id.clone(), a.title.clone())))
        .filter(|(id, _)| !id.is_empty());
    let (similar, to_follow) = tokio::join!(
        async {
            match seed.as_ref() {
                Some((id, _)) => fetch_suggest(runtime, id).await,
                None => Vec::new(),
            }
        },
        fetch_to_follow(runtime, &fav_artists),
    );

    // Rediscover: favorites the user has NOT played recently. The Slint build
    // prefers its reco store's "forgotten favorites" when warm and falls back
    // to exactly this heuristic when cold; this port only has the fallback.
    let recent_ids: std::collections::HashSet<String> =
        recent_albums.iter().map(|a| a.id.clone()).collect();
    let rediscover: Vec<HomeCard> = fav_albums
        .iter()
        .filter(|a| !recent_ids.contains(&a.id))
        .take(18)
        .cloned()
        .map(map_flat_album)
        .collect();

    let personalized = Personalized {
        favorite_albums: fav_albums.iter().take(18).cloned().map(map_flat_album).collect(),
        release_watch,
        top_artists: fav_artists.iter().take(18).cloned().map(map_fav_artist).collect(),
        rediscover,
        to_follow,
        similar_title: match seed.as_ref() {
            Some((_, title)) if !title.is_empty() => {
                qbz_i18n::t_args("Similar to {}", &[title.as_str()])
            }
            _ => qbz_i18n::t("More From Your Library"),
        },
        similar,
    };
    log::info!(
        "[qbz-qt] discover index fetched; building home sections (fav={}, rw={}, artists={}, \
         rediscover={}, toFollow={}, similar={})",
        personalized.favorite_albums.len(),
        personalized.release_watch.len(),
        personalized.top_artists.len(),
        personalized.rediscover.len(),
        personalized.to_follow.len(),
        personalized.similar.len(),
    );
    let candidates = build_candidates(response.containers, personalized);
    let sections = assemble(&candidates, &load_prefs());
    if let Ok(mut cache) = CANDIDATES.lock() {
        *cache = candidates;
    }
    Ok(sections)
}

// ---------------------------------------------------------------------------
// Pure mapping — ported from home.rs (Discover* inputs).
// ---------------------------------------------------------------------------

pub(crate) fn map_album(album: DiscoverAlbum) -> HomeCard {
    let artist = album
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let artist_id = album
        .artists
        .first()
        .map(|a| a.id.to_string())
        .unwrap_or_default();
    let genre = album.genre.map(|g| g.name).unwrap_or_default();
    let year = qbz_text_utils::dates::release_label(
        album
            .dates
            .as_ref()
            .and_then(|d| d.original.as_ref().or(d.download.as_ref()).or(d.stream.as_ref()))
            .map(|s| s.as_str()),
    );
    let (ribbon, ribbon_kind) = pick_ribbon(album.awards.as_deref());
    let artwork_url = album
        .image
        .large
        .or(album.image.thumbnail)
        .or(album.image.small)
        .unwrap_or_default();
    let is_pinned = crate::sidebar_qt::is_pinned("album", &album.id);
    HomeCard {
        is_pinned,
        id: album.id,
        title: album.title,
        artist,
        artist_id,
        genre,
        year,
        quality_tier: quality_tier(album.audio_info.as_ref()).to_string(),
        quality_label: quality_label(album.audio_info.as_ref()),
        quality_detail: quality_detail(album.audio_info.as_ref()),
        ribbon,
        ribbon_kind,
        art_url: artwork_url,
        ..HomeCard::default()
    }
}

/// Map a Discover playlist into a single-cover card (1:1 home.rs
/// `map_playlist`): landscape `rectangle` preferred, first square cover
/// fallback; first tag -> UPPERCASE category subtag.
pub(crate) fn map_playlist(p: DiscoverPlaylist) -> HomeCard {
    let art_url = p
        .image
        .rectangle
        .or_else(|| p.image.covers.and_then(|c| c.into_iter().next()))
        .unwrap_or_default();
    let category = p
        .tags
        .as_ref()
        .and_then(|t| t.first())
        .map(|t| t.name.to_uppercase())
        .unwrap_or_default();
    HomeCard {
        id: p.id.to_string(),
        title: p.name,
        category,
        art_url,
        ..HomeCard::default()
    }
}

/// A compact ranked slim item (home.rs `map_slim`).
fn map_slim(index: usize, album: DiscoverAlbum) -> HomeCard {
    let subtitle = album
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let art_url = album
        .image
        .thumbnail
        .or(album.image.small)
        .or(album.image.large)
        .unwrap_or_default();
    HomeCard {
        id: album.id,
        title: album.title,
        artist: subtitle,
        rank: (index + 1).to_string(),
        art_url,
        ..HomeCard::default()
    }
}

/// Pick the single award ribbon, mirroring `pickAlbumRibbon`: award id 151
/// = Album of the Week, 88 = Qobuzissime, otherwise the last award becomes
/// a generic "press" ribbon.
pub(crate) fn pick_ribbon(awards: Option<&[AlbumAward]>) -> (String, String) {
    let Some(awards) = awards else {
        return (String::new(), String::new());
    };
    if awards.is_empty() {
        return (String::new(), String::new());
    }
    if let Some(a) = awards.iter().find(|a| a.id.as_deref() == Some("151")) {
        return (a.name.clone(), "albumOfTheWeek".to_string());
    }
    if let Some(a) = awards.iter().find(|a| a.id.as_deref() == Some("88")) {
        return (a.name.clone(), "qobuzissime".to_string());
    }
    let last = awards.last().expect("non-empty checked above");
    (last.name.clone(), "press".to_string())
}

/// Quality tier for the icon-only badge: 24-bit and up is Hi-Res, anything
/// else with audio info is CD-quality, no audio info hides the badge.
pub(crate) fn quality_tier(audio: Option<&DiscoverAudioInfo>) -> &'static str {
    let Some(audio) = audio else {
        return "";
    };
    match audio.maximum_bit_depth {
        Some(depth) if depth >= 24 => "hires",
        _ => "cd",
    }
}

/// Exact-quality label for the badge hover tooltip
/// (`{tier}: {depth}-bit / {rate} kHz`).
pub(crate) fn quality_label(audio: Option<&DiscoverAudioInfo>) -> String {
    let Some(audio) = audio else {
        return String::new();
    };
    let hi_res = matches!(audio.maximum_bit_depth, Some(depth) if depth >= 24);
    let tier = if hi_res { "Hi-Res" } else { "CD" };
    let depth = audio
        .maximum_bit_depth
        .unwrap_or(if hi_res { 24 } else { 16 });
    let rate = audio
        .maximum_sampling_rate
        .unwrap_or(if hi_res { 96.0 } else { 44.1 });
    format!("{tier}: {depth}-bit / {} kHz", format_rate(rate))
}

/// Bare exact-quality detail ("24-bit / 96 kHz", no tier prefix).
pub(crate) fn quality_detail(audio: Option<&DiscoverAudioInfo>) -> String {
    let Some(audio) = audio else {
        return String::new();
    };
    let hi_res = matches!(audio.maximum_bit_depth, Some(depth) if depth >= 24);
    let depth = audio
        .maximum_bit_depth
        .unwrap_or(if hi_res { 24 } else { 16 });
    let rate = audio
        .maximum_sampling_rate
        .unwrap_or(if hi_res { 96.0 } else { 44.1 });
    format!("{depth}-bit / {} kHz", format_rate(rate))
}

/// Format a kHz sample rate without a trailing `.0` (96.0 -> "96").
pub(crate) fn format_rate(rate: f64) -> String {
    if (rate.fract()).abs() < f64::EPSILON {
        format!("{}", rate as i64)
    } else {
        format!("{rate}")
    }
}

/// Tier from a bare bit depth (album_map.rs `tier`): >16 is Hi-Res, a
/// known depth <= 16 is CD, unknown hides the badge.
pub(crate) fn quality_tier_from_depth(bit_depth: Option<u32>) -> &'static str {
    match bit_depth {
        Some(b) if b > 16 => "hires",
        Some(_) => "cd",
        None => "",
    }
}

/// Bare exact-quality detail from parts, Hz- or kHz-tolerant (quality.rs
/// `detail`): "24-bit / 96 kHz".
pub(crate) fn quality_detail_from_parts(bit_depth: Option<u32>, sample_rate: Option<f64>) -> String {
    let hi_res = matches!(bit_depth, Some(depth) if depth >= 24);
    let depth = bit_depth.unwrap_or(if hi_res { 24 } else { 16 });
    let rate = sample_rate.unwrap_or(if hi_res { 96.0 } else { 44.1 });
    let rate = if rate >= 1000.0 { rate / 1000.0 } else { rate };
    format!("{depth}-bit / {} kHz", format_rate(rate))
}

// ---------------------------------------------------------------------------
// Personalized rails — ported from foryou.rs (live-session only; the reco /
// blacklist stores are skipped, see module docs).
// ---------------------------------------------------------------------------

/// Flat-Album card mapping (foryou.rs `map_album`).
pub(crate) fn map_flat_album(album: Album) -> HomeCard {
    let year = album
        .release_date_original
        .as_deref()
        .and_then(|s| s.get(..4).map(|y| y.to_string()))
        .unwrap_or_default();
    let quality_tier = match album.maximum_bit_depth {
        Some(d) if d >= 24 => "hires",
        Some(_) => "cd",
        None => "",
    }
    .to_string();
    let quality_label = match (album.maximum_bit_depth, album.maximum_sampling_rate) {
        (Some(bd), Some(sr)) => format!("{}-bit / {} kHz", bd, sr),
        _ => String::new(),
    };
    HomeCard {
        id: album.id,
        title: album.title,
        artist: album.artist.name,
        artist_id: album.artist.id.to_string(),
        year,
        quality_tier,
        quality_label,
        // The list arm's QualityBadgeFull renders the bare exact-quality
        // line; without it a list row shows the tier label over a blank.
        quality_detail: quality_detail_from_parts(
            album.maximum_bit_depth,
            album.maximum_sampling_rate,
        ),
        art_url: album.image.best().cloned().unwrap_or_default(),
        ..HomeCard::default()
    }
}

/// Map one recently-played album (local history) onto a card. The stored ISO
/// release date is localized here exactly as the discover cards do.
pub(crate) fn map_recent_album(a: crate::recently_qt::RecentAlbum) -> HomeCard {
    HomeCard {
        is_pinned: crate::sidebar_qt::is_pinned("album", &a.id),
        id: a.id,
        title: a.title,
        artist: a.artist,
        genre: a.genre,
        year: if a.release_date.is_empty() {
            String::new()
        } else {
            qbz_text_utils::dates::release_label(Some(a.release_date.as_str()))
        },
        quality_tier: a.quality_tier,
        quality_label: a.quality_label,
        art_url: a.artwork_url,
        ..HomeCard::default()
    }
}

/// Map one recently-played track onto a slim row (`slimTracks` — click plays).
fn map_recent_track(t: crate::recently_qt::RecentTrack) -> HomeCard {
    HomeCard {
        id: t.id,
        title: t.title,
        artist: t.subtitle,
        art_url: if t.artwork_url.is_empty() {
            t.album_artwork_url
        } else {
            t.artwork_url
        },
        ..HomeCard::default()
    }
}

/// The user's favorite albums, in favorite order. ONE fetch feeding both the
/// "Library Albums" rail and Rediscover (reco taste-ordering skipped — the
/// port has no reco store, so favorites keep their Qobuz order).
async fn fetch_fav_albums<A>(runtime: &Arc<AppRuntime<A>>) -> Vec<Album>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    match runtime.core().get_favorites("albums", 100, 0).await {
        Ok(value) => {
            qbz_models::lenient::parse_items_array::<Album>(&value, "albums", "home fav album")
        }
        Err(e) => {
            log::warn!("[qbz-qt] favorite albums fetch failed: {e}");
            Vec::new()
        }
    }
}

/// "Release Watch" — `/release/watch` artists, capped 18 (blacklist filter
/// skipped — the store is not open in this POC).
async fn fetch_release_watch<A>(runtime: &Arc<AppRuntime<A>>) -> Vec<HomeCard>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    match runtime.core().get_release_watch("artists", 18, 0).await {
        Ok(page) => page.items.into_iter().map(map_flat_album).collect(),
        Err(e) => {
            log::warn!("[qbz-qt] release watch fetch failed: {e}");
            Vec::new()
        }
    }
}

/// The user's favorite artists. ONE fetch feeding both "Your Top Artists"
/// and the "Artists to Follow" similar-artist seeds.
async fn fetch_fav_artists<A>(runtime: &Arc<AppRuntime<A>>) -> Vec<Artist>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    match runtime.core().get_favorites("artists", 50, 0).await {
        Ok(value) => {
            qbz_models::lenient::parse_items_array::<Artist>(&value, "artists", "home artist")
        }
        Err(e) => {
            log::warn!("[qbz-qt] favorite artists fetch failed: {e}");
            Vec::new()
        }
    }
}

fn map_fav_artist(a: Artist) -> HomeCard {
    HomeCard {
        is_pinned: crate::sidebar_qt::is_pinned("artist", &a.id.to_string()),
        id: a.id.to_string(),
        title: a.name,
        item_kind: "artist".to_string(),
        art_url: a
            .image
            .and_then(|img| img.best().cloned())
            .unwrap_or_default(),
        ..HomeCard::default()
    }
}

/// Seeds for "Artists to Follow" (foryou.rs `ARTIST_SEEDS`).
const ARTIST_SEEDS: usize = 4;
/// Similar artists requested per seed (foryou.rs `SIMILAR_PER_SEED`).
const SIMILAR_PER_SEED: u32 = 10;
/// Display cap for the row (foryou.rs `FOLLOW_MAX`).
const FOLLOW_MAX: usize = 18;

/// Similar artists for ONE seed (absent seed -> no request, empty result).
async fn seed_similar<A>(runtime: &Arc<AppRuntime<A>>, seed: Option<u64>) -> Vec<Artist>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let Some(id) = seed else {
        return Vec::new();
    };
    match runtime.core().get_similar_artists(id, SIMILAR_PER_SEED, 0).await {
        Ok(page) => page.items,
        Err(e) => {
            log::warn!("[qbz-qt] similar artists fetch failed (seed {id}): {e}");
            Vec::new()
        }
    }
}

/// "Artists to Follow" — similar artists off up to [`ARTIST_SEEDS`]
/// favorites, excluding the ones already followed. The seed calls run
/// CONCURRENTLY, but the dedup + [`FOLLOW_MAX`] cap are then applied
/// sequentially IN SEED ORDER, so the membership matches the reference's
/// sequential loop exactly. No favorites -> no seeds -> no request.
async fn fetch_to_follow<A>(runtime: &Arc<AppRuntime<A>>, fav_artists: &[Artist]) -> Vec<HomeCard>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let seeds: Vec<u64> = fav_artists.iter().take(ARTIST_SEEDS).map(|a| a.id).collect();
    if seeds.is_empty() {
        return Vec::new();
    }
    // Fixed 4-wide fan-out (ARTIST_SEEDS); missing seeds resolve instantly.
    let (s0, s1, s2, s3) = tokio::join!(
        seed_similar(runtime, seeds.first().copied()),
        seed_similar(runtime, seeds.get(1).copied()),
        seed_similar(runtime, seeds.get(2).copied()),
        seed_similar(runtime, seeds.get(3).copied()),
    );

    let mut seen: std::collections::HashSet<u64> = fav_artists.iter().map(|a| a.id).collect();
    let mut out: Vec<HomeCard> = Vec::new();
    'outer: for group in [s0, s1, s2, s3] {
        for artist in group {
            if out.len() >= FOLLOW_MAX {
                break 'outer;
            }
            if seen.insert(artist.id) {
                out.push(map_fav_artist(artist));
            }
        }
    }
    out
}

/// "More From Your Library" — `/album/suggest` for a seed album, capped 18.
async fn fetch_suggest<A>(runtime: &Arc<AppRuntime<A>>, album_id: &str) -> Vec<HomeCard>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    if album_id.is_empty() {
        return Vec::new();
    }
    match runtime.core().get_album_suggest(album_id).await {
        Ok(resp) => resp
            .albums
            .map(|p| p.items)
            .unwrap_or_default()
            .into_iter()
            .take(18)
            .map(map_flat_album)
            .collect(),
        Err(e) => {
            log::warn!("[qbz-qt] album suggest fetch failed: {e}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(depth: Option<u32>, rate: Option<f64>) -> DiscoverAudioInfo {
        DiscoverAudioInfo {
            maximum_sampling_rate: rate,
            maximum_bit_depth: depth,
            maximum_channel_count: None,
        }
    }

    fn award(id: Option<&str>, name: &str) -> AlbumAward {
        AlbumAward {
            id: id.map(|s| s.to_string()),
            name: name.to_string(),
            awarded_at: None,
        }
    }

    #[test]
    fn quality_tier_mapping() {
        assert_eq!(quality_tier(None), "");
        assert_eq!(quality_tier(Some(&audio(Some(24), Some(96.0)))), "hires");
        assert_eq!(quality_tier(Some(&audio(Some(16), Some(44.1)))), "cd");
        // Bit depth present but < 24 is CD even without a rate.
        assert_eq!(quality_tier(Some(&audio(Some(16), None))), "cd");
        // No depth at all still maps to CD (audio info exists).
        assert_eq!(quality_tier(Some(&audio(None, None))), "cd");
    }

    #[test]
    fn quality_label_and_detail() {
        assert_eq!(quality_label(None), "");
        assert_eq!(quality_detail(None), "");
        assert_eq!(
            quality_label(Some(&audio(Some(24), Some(96.0)))),
            "Hi-Res: 24-bit / 96 kHz"
        );
        assert_eq!(
            quality_label(Some(&audio(Some(16), Some(44.1)))),
            "CD: 16-bit / 44.1 kHz"
        );
        assert_eq!(
            quality_detail(Some(&audio(Some(24), Some(192.0)))),
            "24-bit / 192 kHz"
        );
        // Missing fields fall back per tier (hi-res: 24/96, cd: 16/44.1).
        assert_eq!(quality_label(Some(&audio(Some(24), None))), "Hi-Res: 24-bit / 96 kHz");
        assert_eq!(quality_detail(Some(&audio(None, Some(44.1)))), "16-bit / 44.1 kHz");
    }

    #[test]
    fn format_rate_trailing_zero() {
        assert_eq!(format_rate(96.0), "96");
        assert_eq!(format_rate(44.1), "44.1");
    }

    #[test]
    fn pick_ribbon_precedence() {
        assert_eq!(pick_ribbon(None), (String::new(), String::new()));
        assert_eq!(pick_ribbon(Some(&[])), (String::new(), String::new()));
        // 151 wins over everything.
        assert_eq!(
            pick_ribbon(Some(&[award(Some("88"), "Qobuzissime"), award(Some("151"), "AOTW")])),
            ("AOTW".to_string(), "albumOfTheWeek".to_string())
        );
        // 88 wins over a generic press award.
        assert_eq!(
            pick_ribbon(Some(&[award(Some("1"), "Press X"), award(Some("88"), "Qobuzissime")])),
            ("Qobuzissime".to_string(), "qobuzissime".to_string())
        );
        // Otherwise the LAST award is the press ribbon.
        assert_eq!(
            pick_ribbon(Some(&[award(Some("1"), "First"), award(Some("2"), "Last")])),
            ("Last".to_string(), "press".to_string())
        );
        // Awards without ids are skipped by the id lookups.
        assert_eq!(
            pick_ribbon(Some(&[award(None, "NoId")])),
            ("NoId".to_string(), "press".to_string())
        );
    }
}
