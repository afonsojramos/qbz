//! Search controller — the QML port of the Slint search surfaces
//! (crates/qbz/src/search.rs pure mappings + the main.rs SearchActions
//! handlers):
//!
//! - **Cortinilla** (live as-you-type dropdown): `live()` loads the combined
//!   search, persists the page into the intelligent-search cache (SWR), picks
//!   the learned top result, and publishes ONE JSON document
//!   (`cortinillaJson`) with the top-result + capped sections (albums 5 /
//!   artists 2 / tracks 3 / playlists 3, spec §6.2.3 order) and controller-
//!   assigned flat indices. Keyboard selection (`move_selection`) rebuilds
//!   the navigable order from the snapshot exactly like the Slint handler.
//! - **Results page** (`submit` / `search_all_action`): `search_all` mapped
//!   to the page document (tabs, totals, most-popular hero, carousel-dedupe),
//!   per-category `load_more` (PAGE_SIZE 20) and the searchType filter radios
//!   (MainArtist/Performer/Composer/Label/ReleaseName) re-querying the three
//!   filterable categories.
//! - **Intelligent Search service** (the 2.0.0 opt-out module): the same
//!   headless `qbz_app::settings::search_service::SearchService` the Slint
//!   wraps — bound per session (`init`, next to the pinned store), kill
//!   switch from `ui_prefs.intelligent_search`, cache `store` on every live
//!   load, `record` on row activation, `rank_within` before truncation,
//!   `top_for_query` for the promoted row.
//!
//! POC-NOTEs:
//! - The LOCAL "on this device" cortinilla sections (library_db + Plex
//!   search, `load_cortinilla`/`append_local_sections`) are NOT ported —
//!   the POC's cortinilla/page are Qobuz-only. Offline, the Qobuz call
//!   fails and both surfaces show their empty states.
//! - Artist "following" flags are resolved from `fav_cache_qt` (the ported
//!   favourite-id cache) — they used to be hard-`false`, which made a search
//!   hit on a followed artist draw "Follow" and un-follow them on click.
//!
//! Blacklist filtering is live on every one of this module's four fetch paths
//! (spec 03 §9.2 F8): `live` and `submit` hand the snapshot pair INTO
//! `core.search_all` (`search.rs:1045-1059`, `:1095-1115`), and `load_more` /
//! `filter_changed` post-filter their pages because the paged
//! `search_albums` / `search_tracks` / `search_artists` pass-throughs take no
//! snapshot (`search.rs:1616-1666`).

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cxx_qt_lib::QString;
use qbz_app::settings::search_service::{InteractionAction, SearchService};
use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_models::{Album, Artist, MostPopularItem, Playlist, SearchAllResults, Track};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Intelligent Search service singleton (search_service.rs wrapper, 1:1)
// ---------------------------------------------------------------------------

static SERVICE: Mutex<Option<SearchService>> = Mutex::new(None);

/// Bind the per-user service (session activation, next to init_pinned).
/// `enabled` seeds from the persisted ui_prefs.intelligent_search pref.
pub fn init(base_dir: &Path, enabled: bool) {
    let service = SearchService::new(base_dir);
    service.set_enabled(enabled);
    if let Ok(mut guard) = SERVICE.lock() {
        *guard = Some(service);
    }
    log::info!("[qbz-qt] intelligent search service bound (enabled={enabled})");
}

fn with_service<T>(default: T, f: impl FnOnce(&SearchService) -> T) -> T {
    SERVICE
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(f))
        .unwrap_or(default)
}

fn with_service_mut(f: impl FnOnce(&mut SearchService)) {
    if let Ok(mut guard) = SERVICE.lock() {
        if let Some(service) = guard.as_mut() {
            f(service);
        }
    }
}

pub fn is_enabled() -> bool {
    with_service(false, |s| s.enabled())
}

pub fn set_enabled(on: bool) {
    with_service((), |s| s.set_enabled(on));
}

fn store(query: &str, results: &SearchAllResults) {
    with_service_mut(|s| s.store(query, results));
}

fn record(query: &str, kind: &str, id: &str, action: InteractionAction) {
    with_service_mut(|s| s.record_interaction(query, kind, id, action));
}

fn top_for_query(query: &str) -> Option<(String, String)> {
    with_service(None, |s| s.top_for_query(query))
}

fn rank_within<T>(query: &str, kind: &str, items: &mut Vec<T>, id_of: impl Fn(&T) -> String) {
    with_service((), |s| s.rank_within(query, kind, items, id_of));
}

// ---------------------------------------------------------------------------
// ui_prefs intelligent_search (the 2.0.0 opt-out).
//
// Both halves are settings_qt's now. This module used to keep its own path,
// its own `json!({})` fallback and its own truncating `std::fs::write` on
// ui_prefs.json — the file the SHIPPING Slint build has open — so one search
// toggle could hand Slint's `load()` an empty document and let its next save
// flatten the whole profile. See the write-discipline block in settings_qt.rs.
// ---------------------------------------------------------------------------

pub fn intelligent_search_pref() -> bool {
    crate::settings_qt::pref_bool("intelligent_search", true)
}

/// Flip + persist the pref and flip the live kill switch; returns the new value.
///
/// The read and the write share ONE document (`toggle_pref_bool`): reading the
/// old value through `intelligent_search_pref` first would answer a torn read
/// with the default `true` and then commit `false` over a user who already had
/// it off. When nothing could be written the live switch is left alone too —
/// the two must not diverge.
pub fn toggle_intelligent_search() -> bool {
    let Some(next) = crate::settings_qt::toggle_pref_bool("intelligent_search", true) else {
        let current = intelligent_search_pref();
        log::warn!(
            "[qbz-qt] intelligent_search toggle skipped (prefs unreadable) — staying {current}"
        );
        return current;
    };
    set_enabled(next);
    next
}

// ---------------------------------------------------------------------------
// Row types (search.rs AlbumRow / TrackRowData / ArtistRow / PlaylistRow,
// shaped so the QML cards (AlbumCard etc.) can consume them directly)
// ---------------------------------------------------------------------------

#[derive(Clone, Default, Serialize)]
pub struct CardRow {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub genre: String,
    pub year: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityLabel")]
    pub quality_label: String,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    #[serde(rename = "artPath")]
    pub art_path: String,
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
    #[serde(rename = "isPinned")]
    pub is_pinned: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct TrackRow {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    #[serde(rename = "albumId")]
    pub album_id: String,
    pub duration: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityLabel")]
    pub quality_label: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub explicit: bool,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    #[serde(rename = "artPath")]
    pub art_path: String,
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct ArtistRow {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    #[serde(rename = "artPath")]
    pub art_path: String,
    pub following: bool,
    /// Pin badge state at build time (`CardRow` carries the album twin).
    /// ArtistCard draws the same glyph as the album card, and a row that
    /// never carries the flag makes it lie — the first click on an
    /// already-pinned artist UN-pins it.
    #[serde(rename = "isPinned")]
    pub is_pinned: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct PlaylistRow {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    #[serde(rename = "artPath")]
    pub art_path: String,
    /// The ownership tri-state, under the names `cards/PlaylistCard.qml`
    /// actually reads (`item.playlistOwned` / `item.playlistFollowing`).
    ///
    /// They used to serialize as `isOwned` / `isFollowing`: names NOTHING in
    /// the tree consumed (verified by grep over `qml/` — the only
    /// `isFollowing` readers are LabelView's label doc, PlaylistView's
    /// playlist doc and ArtistView's artist doc, three different structs). So
    /// the fields were on the wire, and the card still saw `undefined` on both
    /// and fell into the "foreign playlist, offer Follow" arm. `library_qt::
    /// FeedItem` and `home_qt::HomeCard` already publish these two spellings;
    /// one card must read one contract on every surface.
    #[serde(rename = "playlistOwned")]
    pub is_owned: bool,
    #[serde(rename = "playlistFollowing")]
    pub is_following: bool,
    /// The library heart — only drawn on the OWNED arm of the tri-state.
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
    /// Pin badge state at build time — see `ArtistRow::is_pinned`.
    #[serde(rename = "isPinned")]
    pub is_pinned: bool,
}

// ---------------------------------------------------------------------------
// Pure helpers (search.rs: tier / quality_label / mmss / year_of /
// format_album_title)
// ---------------------------------------------------------------------------

fn tier(bit_depth: Option<u32>) -> &'static str {
    match bit_depth {
        Some(depth) if depth >= 24 => "hires",
        Some(_) => "cd",
        None => "",
    }
}

fn quality_label(bit_depth: Option<u32>, sample_rate: Option<f64>) -> String {
    match bit_depth {
        None => String::new(),
        Some(depth) => {
            let prefix = if depth >= 24 { "Hi-Res" } else { "CD" };
            let rate = sample_rate.unwrap_or(if depth >= 24 { 96.0 } else { 44.1 });
            let rate = if rate.fract().abs() < f64::EPSILON {
                format!("{}", rate as i64)
            } else {
                format!("{rate}")
            };
            format!("{prefix} {depth}-bit / {rate} kHz")
        }
    }
}

fn mmss(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn year_of(date: Option<&str>) -> String {
    date.and_then(|d| d.get(0..4)).unwrap_or("").to_string()
}

fn format_album_title(title: &str, version: Option<&str>) -> String {
    match version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => format!("{title} ({v})"),
        None => title.to_string(),
    }
}

fn map_album(album: &Album) -> CardRow {
    CardRow {
        // Pin badge state from the per-user store (search.rs stamps it the
        // same way). The field existed on the wire but nothing ever filled
        // it, so every search card drew the hollow glyph and the first
        // click on an already-pinned album UN-pinned it.
        is_pinned: crate::sidebar_qt::is_pinned("album", &album.id),
        // Heart at build time. `CardRow` has always DECLARED `is_favorite`
        // (line 158) and nothing ever filled it, so `SearchView.qml` mounts
        // AlbumCard with `isFavorite: false`: a favourited album drew hollow
        // and, now that the toggle takes its direction from the populated
        // cache, the first click REMOVED it from the library.
        is_favorite: crate::fav_cache_qt::is_album_favorite(&album.id),
        id: album.id.clone(),
        title: format_album_title(&album.title, album.version.as_deref()),
        artist: album.artist.name.clone(),
        artist_id: album.artist.id.to_string(),
        genre: album
            .genre
            .clone()
            .map(|g| g.name)
            .filter(|n| !n.is_empty())
            .unwrap_or_default(),
        year: year_of(album.release_date_original.as_deref()),
        quality_tier: tier(album.maximum_bit_depth).to_string(),
        quality_label: quality_label(album.maximum_bit_depth, album.maximum_sampling_rate),
        art_url: album.image.best().cloned().unwrap_or_default(),
        ..Default::default()
    }
}

fn map_track(track: &Track) -> TrackRow {
    let mut title = track.title.clone();
    if let Some(version) = track.version.as_ref().filter(|v| !v.is_empty()) {
        title = format!("{title} ({version})");
    }
    let artwork_url = track
        .album
        .as_ref()
        .and_then(|a| a.image.best().cloned())
        .unwrap_or_default();
    let album_id = track.album.as_ref().map(|a| a.id.clone()).unwrap_or_default();
    let (artist, artist_id) = track
        .performer
        .clone()
        .map(|p| (p.name, p.id.to_string()))
        .unwrap_or_default();
    TrackRow {
        // Same story as `map_album`: the field was declared (line 186) and
        // never stamped, so every search track row drew the empty heart and
        // the first click sent `favorite/delete` on a track the user had
        // favourited.
        is_favorite: crate::fav_cache_qt::contains_track(track.id),
        id: track.id.to_string(),
        title,
        artist,
        artist_id,
        album_id,
        duration: mmss(track.duration),
        quality_tier: tier(track.maximum_bit_depth).to_string(),
        quality_label: quality_label(track.maximum_bit_depth, track.maximum_sampling_rate),
        quality_detail: crate::home_qt::quality_detail_from_parts(
            track.maximum_bit_depth,
            track.maximum_sampling_rate,
        ),
        explicit: track.parental_warning,
        art_url: artwork_url,
        ..Default::default()
    }
}

/// `following` is DERIVED, not passed in. Every call site handed it `false`
/// (the module's POC-NOTE said "no favorites snapshot is consulted"), which
/// meant a search hit on an artist the user follows drew "Follow" and the
/// click un-followed them. The snapshot exists now — `fav_cache_qt` — and the
/// reference does exactly this (`search.rs::map_most_popular` resolves the
/// same flag from its `favorite_artists` set), so the parameter is gone rather
/// than left as a lie every caller has to remember not to tell.
fn map_artist(artist: &Artist) -> ArtistRow {
    let following = crate::fav_cache_qt::is_artist_favorite(artist.id);
    ArtistRow {
        is_pinned: crate::sidebar_qt::is_pinned("artist", &artist.id.to_string()),
        id: artist.id.to_string(),
        title: artist.name.clone(),
        subtitle: match artist.albums_count {
            Some(n) if n > 0 => qbz_i18n::tf("{} album", "{} albums", n as i64, &[&n.to_string()]),
            _ => String::new(),
        },
        art_url: artist
            .image
            .as_ref()
            .and_then(|i| i.best().cloned())
            .unwrap_or_default(),
        art_path: String::new(),
        following,
    }
}

fn map_playlist(playlist: &Playlist) -> PlaylistRow {
    // Highest-resolution non-empty cover list wins (playlist_cover_urls).
    let cover = playlist
        .images300
        .as_ref()
        .filter(|v| !v.is_empty())
        .or(playlist.images150.as_ref().filter(|v| !v.is_empty()))
        .or(playlist.images.as_ref().filter(|v| !v.is_empty()))
        .and_then(|v| v.first().cloned())
        .unwrap_or_default();
    let mut subtitle = playlist.owner.name.clone();
    if playlist.tracks_count > 0 {
        let count = playlist.tracks_count;
        let tracks_label = qbz_i18n::tf("{} track", "{} tracks", count as i64, &[&count.to_string()]);
        subtitle = if subtitle.is_empty() {
            tracks_label
        } else {
            format!("{subtitle}   •   {tracks_label}")
        };
    }
    PlaylistRow {
        is_pinned: crate::sidebar_qt::is_pinned("playlist", &playlist.id.to_string()),
        // The overlay tri-state. `is_owned` is authoritative from the owner id
        // — that is exactly what the reference does (search.rs:367,
        // `current_user_id() == playlist.owner.id`) and it is the half that
        // matters most: without it a search hit on the user's OWN playlist
        // offered "Follow on Qobuz" and the click subscribed them to
        // themselves. `is_following` is only knowable from the user's own
        // playlist list, so it comes from the ownership snapshot (the
        // reference leaves it `false` here; this is a strict improvement and
        // degrades to `false` before the first snapshot).
        is_owned: crate::playlist_qt::owns(playlist.owner.id),
        is_following: crate::playlist_qt::is_following(playlist.id),
        // The heart is the qbz-local library.db flag, mirrored in the cache.
        // PlaylistCard only draws it on the OWNED arm, which is precisely why
        // it has to be stamped together with `is_owned` and not before it.
        is_favorite: crate::fav_cache_qt::is_playlist_favorite(playlist.id),
        id: playlist.id.to_string(),
        title: playlist.name.clone(),
        subtitle,
        art_url: cover,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Blacklist snapshots (spec 03 §9.2 F8)
// ---------------------------------------------------------------------------

/// The `(artists, albums)` blacklist snapshot pair every fetch path in this
/// module needs, in the reference's exact shape: **one** shared `is_enabled()`
/// gate for both axes, empty sets when the feature is off so the `qbz_core`
/// predicates short-circuit on `bl.is_empty() && album_bl.is_empty()`
/// (`search.rs:1045-1055` / `:1618-1626`, `qbz-core/src/core.rs:128`).
///
/// Snapshotted per fetch, not cached: the store is mutated from the manager,
/// the artist page and the album page with no change-notify, so a stale
/// snapshot is exactly the leak this closes.
fn blacklist_snapshots() -> (HashSet<u64>, HashSet<String>) {
    if crate::artist_blacklist::is_enabled() {
        (
            crate::artist_blacklist::ids_snapshot(),
            crate::artist_blacklist::album_ids_snapshot(),
        )
    } else {
        Default::default()
    }
}

// ---------------------------------------------------------------------------
// Artwork (the reload_home pattern: disk hits inline, one background
// download + republish)
// ---------------------------------------------------------------------------

/// Resolve cover URLs into the row's art_path from the disk cache; returns
/// the urls still missing (deduped). The slot pair is (source url, art_path)
/// — the path starts empty and is filled on a cache hit (artwork_qt's
/// attach_cached pattern for Home).
fn attach_urls(pairs: Vec<(String, &mut String)>) -> Vec<String> {
    let mut missing = Vec::new();
    for (url, slot) in pairs {
        if url.is_empty() {
            continue;
        }
        let hit = crate::artwork_qt::cached_path(&url);
        if hit.is_empty() {
            if !missing.contains(&url) {
                missing.push(url);
            }
        } else {
            *slot = hit;
        }
    }
    missing
}

// ---------------------------------------------------------------------------
// Cortinilla payload (search.rs CortRow / CortSection / CortinillaData)
// ---------------------------------------------------------------------------

#[derive(Clone, Default, Serialize, PartialEq)]
pub struct CortRow {
    pub kind: String,
    pub id: String,
    pub source: String,
    pub title: String,
    pub subtitle: String,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    #[serde(rename = "artPath")]
    pub art_path: String,
    #[serde(rename = "flatIndex")]
    pub flat_index: i32,
}

#[derive(Clone, Default, Serialize, PartialEq)]
pub struct CortSection {
    pub title: String,
    pub kind: String,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
    pub rows: Vec<CortRow>,
}

#[derive(Clone, Default, Serialize, PartialEq)]
pub struct CortinillaData {
    pub query: String,
    pub top: Option<CortRow>,
    pub sections: Vec<CortSection>,
}

const CORTINILLA_CAP_ALBUMS: usize = 5;
const CORTINILLA_CAP_ARTISTS: usize = 2;
const CORTINILLA_CAP_TRACKS: usize = 3;
const CORTINILLA_CAP_PLAYLISTS: usize = 3;

/// search.rs `map_search_all_to_cortinilla`, 1:1 (ranking, caps, top-result
/// selection, flat-index assignment).
fn map_search_all_to_cortinilla(query: &str, results: &SearchAllResults) -> CortinillaData {
    let to_artist_row = |a: &Artist| CortRow {
        kind: "artist".into(),
        id: a.id.to_string(),
        source: "qobuz".into(),
        title: a.name.clone(),
        subtitle: map_artist(a).subtitle,
        art_url: a
            .image
            .as_ref()
            .and_then(|i| i.best().cloned())
            .unwrap_or_default(),
        ..Default::default()
    };
    let to_album_row = |al: &Album| {
        let m = map_album(al);
        CortRow {
            kind: "album".into(),
            id: m.id,
            source: "qobuz".into(),
            title: m.title,
            subtitle: m.artist,
            art_url: m.art_url,
            ..Default::default()
        }
    };
    let to_track_row = |t: &Track| {
        let m = map_track(t);
        CortRow {
            kind: "track".into(),
            id: m.id,
            source: "qobuz".into(),
            title: m.title,
            subtitle: m.artist,
            art_url: m.art_url,
            ..Default::default()
        }
    };
    let to_playlist_row = |p: &Playlist| {
        let m = map_playlist(p);
        CortRow {
            kind: "playlist".into(),
            id: m.id,
            source: "qobuz".into(),
            title: m.title,
            subtitle: m.subtitle,
            art_url: m.art_url,
            ..Default::default()
        }
    };

    let mut artist_rows: Vec<CortRow> = results.artists.items.iter().map(&to_artist_row).collect();
    let mut album_rows: Vec<CortRow> = results.albums.items.iter().map(&to_album_row).collect();
    let mut track_rows: Vec<CortRow> = results.tracks.items.iter().map(&to_track_row).collect();
    let mut playlist_rows: Vec<CortRow> =
        results.playlists.items.iter().map(&to_playlist_row).collect();

    rank_within(query, "artist", &mut artist_rows, |r| r.id.clone());
    rank_within(query, "album", &mut album_rows, |r| r.id.clone());
    rank_within(query, "track", &mut track_rows, |r| r.id.clone());
    rank_within(query, "playlist", &mut playlist_rows, |r| r.id.clone());
    artist_rows.truncate(CORTINILLA_CAP_ARTISTS);
    album_rows.truncate(CORTINILLA_CAP_ALBUMS);
    track_rows.truncate(CORTINILLA_CAP_TRACKS);
    playlist_rows.truncate(CORTINILLA_CAP_PLAYLISTS);

    // Top result: the learned (kind, id) for this query, else the
    // most_popular hero, else first artist, else first album.
    let top_kind_id = top_for_query(query);
    let find_in = |kind: &str, id: &str| -> Option<CortRow> {
        let sect = match kind {
            "artist" => &artist_rows,
            "album" => &album_rows,
            "track" => &track_rows,
            "playlist" => &playlist_rows,
            _ => return None,
        };
        sect.iter().find(|r| r.id == id).cloned()
    };
    let top: Option<CortRow> = top_kind_id
        .and_then(|(kind, id)| {
            find_in(&kind, &id).or_else(|| match kind.as_str() {
                "artist" => results
                    .artists
                    .items
                    .iter()
                    .find(|a| a.id.to_string() == id)
                    .map(&to_artist_row),
                "album" => results.albums.items.iter().find(|a| a.id == id).map(&to_album_row),
                "track" => results.tracks.items.iter().find(|t| t.id.to_string() == id).map(&to_track_row),
                "playlist" => results
                    .playlists
                    .items
                    .iter()
                    .find(|p| p.id.to_string() == id)
                    .map(&to_playlist_row),
                _ => None,
            })
        })
        .or_else(|| match &results.most_popular {
            Some(MostPopularItem::Artists(a)) => Some(to_artist_row(a)),
            Some(MostPopularItem::Albums(a)) => Some(to_album_row(a)),
            Some(MostPopularItem::Tracks(t)) => Some(to_track_row(t)),
            None => None,
        })
        .or_else(|| artist_rows.first().cloned())
        .or_else(|| album_rows.first().cloned());

    let mut sections: Vec<CortSection> = Vec::new();
    let mut push_section = |title: &str, kind: &str, rows: Vec<CortRow>, total: u32| {
        if !rows.is_empty() {
            sections.push(CortSection {
                title: title.to_string(),
                kind: kind.to_string(),
                has_more: total as usize > rows.len(),
                rows,
            });
        }
    };
    push_section(&qbz_i18n::t("Albums"), "album", album_rows, results.albums.total);
    push_section(&qbz_i18n::t("Artists"), "artist", artist_rows, results.artists.total);
    push_section(&qbz_i18n::t("Tracks"), "track", track_rows, results.tracks.total);
    push_section(&qbz_i18n::t("Playlists"), "playlist", playlist_rows, results.playlists.total);

    let mut data = CortinillaData {
        query: query.to_string(),
        top,
        sections,
    };
    assign_flat_indices(&mut data);
    data
}

fn assign_flat_indices(data: &mut CortinillaData) {
    let mut next = 0;
    if let Some(top) = &mut data.top {
        top.flat_index = next;
        next += 1;
    } else {
        next = 1; // no top result: section rows start at 1 (Slint convention)
    }
    for section in &mut data.sections {
        for row in &mut section.rows {
            row.flat_index = next;
            next += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Cortinilla controller state
// ---------------------------------------------------------------------------

static LAST_CORT: Mutex<Option<CortinillaData>> = Mutex::new(None);
static CORT_VERSION: AtomicU64 = AtomicU64::new(0);
/// The keyboard-selected flat index mirrored from the bridge property (so
/// `move_selection` needs no QML round-trip).
static CURRENT_SEL: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

fn next_cort_version() -> u64 {
    CORT_VERSION.fetch_add(1, Ordering::SeqCst) + 1
}
fn is_current_cort_version(v: u64) -> bool {
    CORT_VERSION.load(Ordering::SeqCst) == v
}

fn set_cortinilla_open(open: bool) {
    crate::ui(move |mut b| {
        b.as_mut().set_cortinilla_open(open);
        if !open {
            b.as_mut().set_selected_index(-1);
        }
    });
}

fn set_selected(index: i32, scroll_y: f32) {
    CURRENT_SEL.store(index, Ordering::SeqCst);
    crate::ui(move |mut b| {
        b.as_mut().set_selected_index(index);
        b.as_mut().set_cortinilla_scroll_y(scroll_y);
    });
}

/// The live header query changed (QML debounced at 220ms, >= 2 chars —
/// mirrors the Slint CORTINILLA_DEBOUNCE + the chars().count() gate).
/// Opens the dropdown, resets the keyboard selection, and loads.
pub async fn live(runtime: &Arc<AppRuntime<LoggingAdapter>>, query: &str) {
    let q = query.trim().to_string();
    if q.chars().count() < 2 {
        set_cortinilla_open(false);
        next_cort_version();
        return;
    }
    if !is_enabled() {
        // Module OFF: the Slint live-navigates to the results page instead
        // (300ms debounce). The POC keeps the cortinilla-only surface:
        // Enter still reaches the page via submit(). POC-NOTE.
        return;
    }
    set_selected(-1, 0.0);
    set_cortinilla_open(true);
    let version = next_cort_version();
    // Query echo + loading flag ride the cortinillaLoading property and a
    // minimal payload (skeleton rows are pure QML).
    crate::ui(move |mut b| {
        b.as_mut().set_cortinilla_loading(true);
    });

    let t = std::time::Instant::now();
    // Blacklist filtering happens INSIDE search_all (`search.rs:1112-1115`).
    let (bl, abl) = blacklist_snapshots();
    let results = runtime.core().search_all(&q, &bl, &abl).await;
    let results = match results {
        Ok(r) => r,
        Err(e) => {
            log::error!("[qbz-qt] cortinilla load failed: {e}");
            crate::ui(move |mut b| {
                if is_current_cort_version(version) {
                    b.as_mut().set_cortinilla_loading(false);
                }
            });
            return;
        }
    };
    log::info!(
        "[qbz-qt][perf] cortinilla query \"{q}\" -> results: {:?}",
        t.elapsed(),
    );
    store(&q, &results);
    let mut data = map_search_all_to_cortinilla(&q, &results);
    // Artwork: disk hits inline; misses download in the background and
    // republish the SAME payload (version-guarded).
    let mut urls: Vec<(String, &mut String)> = Vec::new();
    if let Some(top) = &mut data.top {
        urls.push((top.art_url.clone(), &mut top.art_path));
    }
    for section in &mut data.sections {
        for row in &mut section.rows {
            urls.push((row.art_url.clone(), &mut row.art_path));
        }
    }
    let missing = attach_urls(urls);
    *LAST_CORT.lock().unwrap() = Some(data.clone());
    crate::ui(move |mut b| {
        if is_current_cort_version(version) {
            let json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".into());
            b.as_mut().set_cortinilla_json(QString::from(json.as_str()));
            b.as_mut().set_cortinilla_loading(false);
        }
    });
    if !missing.is_empty() {
        crate::spawn(async move {
            crate::artwork_qt::download_missing(missing).await;
            if !is_current_cort_version(version) {
                return;
            }
            let mut data = LAST_CORT.lock().unwrap().clone();
            if let Some(data) = &mut data {
                let mut urls: Vec<(String, &mut String)> = Vec::new();
                if let Some(top) = &mut data.top {
                    urls.push((top.art_url.clone(), &mut top.art_path));
                }
                for section in &mut data.sections {
                    for row in &mut section.rows {
                        urls.push((row.art_url.clone(), &mut row.art_path));
                    }
                }
                let _ = attach_urls(urls);
                let data = data.clone();
                *LAST_CORT.lock().unwrap() = Some(data.clone());
                crate::ui(move |mut b| {
                    let json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".into());
                    b.as_mut().set_cortinilla_json(QString::from(json.as_str()));
                });
            }
        });
    }
}

/// Dismiss (Esc / click-outside / idle / page change).
pub fn dismiss() {
    set_cortinilla_open(false);
}

/// Arrow-key move (delta -1 up / +1 down; the Slint on_cortinilla_move_selection
/// semantics: Down from -1 -> first, Up from first -> -1, clamp both ends).
pub fn move_selection(delta: i32) {
    let current = CURRENT_SEL.load(Ordering::SeqCst);
    let snap = LAST_CORT.lock().unwrap().clone();
    let Some(data) = snap else { return };
    let mut order: Vec<i32> = Vec::new();
    if let Some(top) = &data.top {
        order.push(top.flat_index);
    }
    for section in &data.sections {
        for row in &section.rows {
            order.push(row.flat_index);
        }
    }
    if order.is_empty() {
        return;
    }
    let pos = order.iter().position(|&fi| fi == current);
    let new_index = if delta > 0 {
        match pos {
            None => order[0],
            Some(p) if p + 1 < order.len() => order[p + 1],
            Some(_) => order[order.len() - 1],
        }
    } else {
        match pos {
            None => -1,
            Some(0) => -1,
            Some(p) => order[p - 1],
        }
    };
    // Content-space top-y of the selected row (the QML scrolls it into
    // view). Layout mirrors the QML cortinilla: 6px top padding; the
    // top-result block is 4 + 22 + 56; each section is 4 + 24 header +
    // 56/row.
    let scroll_y = flat_index_content_y(&data, new_index);
    set_selected(new_index, scroll_y);
}

fn flat_index_content_y(data: &CortinillaData, flat_index: i32) -> f32 {
    let mut y = 6.0f32;
    if let Some(top) = &data.top {
        if top.flat_index == flat_index {
            return y + 4.0 + 22.0;
        }
        y += 4.0 + 22.0 + 56.0;
    }
    for section in &data.sections {
        y += 4.0 + 24.0;
        for row in &section.rows {
            if row.flat_index == flat_index {
                return y;
            }
            y += 56.0;
        }
    }
    0.0
}

/// Click / Enter on a row: record the interaction (Capa B) and dispatch by
/// kind (album -> album view, artist -> artist view, track -> play,
/// playlist -> inert POC-NOTE). The QML clears the input + closes itself.
pub fn row_clicked(flat_index: i32) {
    let snap = LAST_CORT.lock().unwrap().clone();
    let Some(data) = snap else { return };
    let row = data
        .top
        .iter()
        .chain(data.sections.iter().flat_map(|s| s.rows.iter()))
        .find(|r| r.flat_index == flat_index)
        .cloned();
    let Some(row) = row else { return };
    set_cortinilla_open(false);
    if row.source != "local" {
        let action = if row.kind == "track" {
            InteractionAction::Play
        } else {
            InteractionAction::Open
        };
        record(&data.query, &row.kind, &row.id, action);
    }
    match row.kind.as_str() {
        "album" => crate::open_album(row.id),
        "artist" => crate::open_artist(row.id),
        "track" => {
            if let Ok(id) = row.id.parse::<u64>() {
                crate::play_track(id);
            }
        }
        // playlist: no playlist view in the POC (POC-NOTE — inert).
        _ => {}
    }
}

/// "View more" on a section: full search page on the matching tab
/// (album -> 1, track -> 2, artist -> 3, playlist -> 4).
pub async fn view_more(runtime: &Arc<AppRuntime<LoggingAdapter>>, kind: &str) {
    let tab = match kind {
        "album" => 1,
        "track" => 2,
        "artist" => 3,
        "playlist" => 4,
        _ => 0,
    };
    let q = LAST_CORT.lock().unwrap().as_ref().map(|d| d.query.clone()).unwrap_or_default();
    set_cortinilla_open(false);
    submit(runtime, &q, Some(tab)).await;
}

/// The Enter affordance with no keyboard selection: full search, All tab.
pub async fn search_all_action(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let q = LAST_CORT.lock().unwrap().as_ref().map(|d| d.query.clone()).unwrap_or_default();
    set_cortinilla_open(false);
    submit(runtime, &q, Some(0)).await;
}

// ---------------------------------------------------------------------------
// Results page
// ---------------------------------------------------------------------------

#[derive(Clone, Default, Serialize)]
pub struct MostPopularDoc {
    pub kind: String,
    pub album: Option<CardRow>,
    pub artist: Option<ArtistRow>,
    pub track: Option<TrackRow>,
    #[serde(rename = "qualityLabel")]
    pub quality_label: String,
}

#[derive(Clone, Default, Serialize)]
pub struct SearchPageDoc {
    pub query: String,
    pub tab: i32,
    pub loading: bool,
    #[serde(rename = "filterIndex")]
    pub filter_index: i32,
    pub albums: Vec<CardRow>,
    pub tracks: Vec<TrackRow>,
    pub artists: Vec<ArtistRow>,
    #[serde(rename = "artistsCarousel")]
    pub artists_carousel: Vec<ArtistRow>,
    pub playlists: Vec<PlaylistRow>,
    #[serde(rename = "albumsTotal")]
    pub albums_total: i32,
    #[serde(rename = "tracksTotal")]
    pub tracks_total: i32,
    #[serde(rename = "artistsTotal")]
    pub artists_total: i32,
    #[serde(rename = "playlistsTotal")]
    pub playlists_total: i32,
    #[serde(rename = "mostPopular")]
    pub most_popular: MostPopularDoc,
}

#[derive(Default)]
struct PageState {
    doc: SearchPageDoc,
}

static PAGE: Mutex<Option<PageState>> = Mutex::new(None);
static PAGE_VERSION: AtomicU64 = AtomicU64::new(0);

fn next_page_version() -> u64 {
    PAGE_VERSION.fetch_add(1, Ordering::SeqCst) + 1
}
fn is_current_page_version(v: u64) -> bool {
    PAGE_VERSION.load(Ordering::SeqCst) == v
}

/// A pin/unpin just landed: patch the CACHED page rows carrying `(kind, id)`.
///
/// Third sibling of `home_qt::apply_pin_change` and
/// `recommendations_qt::apply_pin_change`, and it publishes nothing for the
/// same reason they do not: the cards on screen are corrected in place by
/// `QbzLibrary.pinChanged`, so no model is replaced and no delegate is torn
/// down.
///
/// Why search NEEDED it, specifically: `PAGE` is not a build-once cache — the
/// artwork pass in `submit` re-publishes the SAME document ~a second later
/// with only `art_path` mutated, and `tab_changed` / `load_more` /
/// `filter_changed` each publish the cache too. Every one of those swaps the
/// model out from under the card, so a pin made in the meantime was reverted
/// by the stale `isPinned` the cache still held — the optimistic flip
/// visibly undone by artwork landing.
///
/// The alternative (re-stamping `is_pinned` from the store inside
/// `publish_page`) was rejected: `sidebar_qt::is_pinned` is a sqlite query per
/// row, so that would put one query per album + artist + playlist row on EVERY
/// publish, including the two that a single search already triggers. Patching
/// the cache once per pin click is O(rows) in memory and matches the shape the
/// other two caches already use — the reference does the same thing with its
/// `set_*_row_pinned` model walks.
///
/// Tracks are not pinnable, so the track list is not walked. The cortinilla
/// snapshot (`LAST_CORT`) is not walked either: `CortRow` carries no pin flag
/// and the dropdown draws no badge.
pub(crate) fn apply_pin_change(kind: &str, id: &str, pinned: bool) {
    if !matches!(kind, "album" | "artist" | "playlist") {
        return;
    }
    let Ok(mut guard) = PAGE.lock() else {
        return;
    };
    let Some(page) = guard.as_mut() else {
        return;
    };
    let doc = &mut page.doc;
    match kind {
        "album" => {
            for row in doc.albums.iter_mut().filter(|r| r.id == id) {
                row.is_pinned = pinned;
            }
            if let Some(row) = doc.most_popular.album.as_mut().filter(|r| r.id == id) {
                row.is_pinned = pinned;
            }
        }
        "artist" => {
            // `artists_carousel` holds CLONES of `artists` rows (the All-tab
            // dedupe copies them), so both lists have to be walked or the
            // carousel keeps the stale badge.
            for row in doc.artists.iter_mut().filter(|r| r.id == id) {
                row.is_pinned = pinned;
            }
            for row in doc.artists_carousel.iter_mut().filter(|r| r.id == id) {
                row.is_pinned = pinned;
            }
            if let Some(row) = doc.most_popular.artist.as_mut().filter(|r| r.id == id) {
                row.is_pinned = pinned;
            }
        }
        _ => {
            for row in doc.playlists.iter_mut().filter(|r| r.id == id) {
                row.is_pinned = pinned;
            }
        }
    }
}

/// A favourite toggle just SETTLED: patch the cached page rows.
///
/// Same shape as [`apply_pin_change`], same reason it publishes nothing — and
/// search needs it MORE than the pin twin does, because its cached document is
/// re-published on a timer the user does not control (the artwork pass after
/// `submit`). Heart an album in the results grid, wait for a cover to land,
/// and the model swap put the hollow heart back over an album now IN the
/// library; the next click removed it.
///
/// Tracks are included here (unlike the pin twin) — track rows do draw a
/// heart. The cortinilla snapshot is not: `CortRow` carries no favourite flag.
pub(crate) fn apply_favorite_change(kind: &str, id: &str, favorite: bool) {
    if !matches!(kind, "album" | "track" | "artist" | "playlist") {
        return;
    }
    let Ok(mut guard) = PAGE.lock() else {
        return;
    };
    let Some(page) = guard.as_mut() else {
        return;
    };
    let doc = &mut page.doc;
    match kind {
        "album" => {
            for row in doc.albums.iter_mut().filter(|r| r.id == id) {
                row.is_favorite = favorite;
            }
            if let Some(row) = doc.most_popular.album.as_mut().filter(|r| r.id == id) {
                row.is_favorite = favorite;
            }
        }
        "track" => {
            for row in doc.tracks.iter_mut().filter(|r| r.id == id) {
                row.is_favorite = favorite;
            }
            if let Some(row) = doc.most_popular.track.as_mut().filter(|r| r.id == id) {
                row.is_favorite = favorite;
            }
        }
        "artist" => {
            // `following` is this row type's heart. Both lists again — the
            // carousel holds clones (see `apply_pin_change`).
            for row in doc.artists.iter_mut().filter(|r| r.id == id) {
                row.following = favorite;
            }
            for row in doc.artists_carousel.iter_mut().filter(|r| r.id == id) {
                row.following = favorite;
            }
            if let Some(row) = doc.most_popular.artist.as_mut().filter(|r| r.id == id) {
                row.following = favorite;
            }
        }
        _ => {
            for row in doc.playlists.iter_mut().filter(|r| r.id == id) {
                row.is_favorite = favorite;
            }
        }
    }
}

fn publish_page(doc: &SearchPageDoc) {
    let json = serde_json::to_string(doc).unwrap_or_else(|_| "{}".into());
    crate::ui(move |mut b| {
        b.as_mut().set_search_json(QString::from(json.as_str()));
    });
}

/// search.rs `search_type_for_filter`.
fn search_type_for_filter(index: i32) -> Option<String> {
    match index {
        1 => Some("MainArtist".into()),
        2 => Some("Performer".into()),
        3 => Some("Composer".into()),
        4 => Some("Label".into()),
        5 => Some("ReleaseName".into()),
        _ => None,
    }
}

/// Enter (or View-more / search-all): record the nav entry and run the full
/// combined search. `tab` is always All on plain submit; View-more picks
/// the section's tab (the Slint lands on the tab, the QML clears the header
/// input itself).
pub async fn submit(runtime: &Arc<AppRuntime<LoggingAdapter>>, query: &str, tab: Option<i32>) {
    let q = query.trim().to_string();
    if q.chars().count() < 2 {
        return;
    }
    let version = next_page_version();
    crate::navigate_to("search");
    {
        let mut guard = PAGE.lock().unwrap();
        let doc = &mut guard.get_or_insert_with(PageState::default).doc;
        doc.query = q.clone();
        doc.tab = tab.unwrap_or(0);
        doc.loading = true;
        publish_page(doc);
    }

    let t = std::time::Instant::now();
    // Blacklist filtering happens INSIDE search_all (`search.rs:1056-1059`).
    let (bl, abl) = blacklist_snapshots();
    let results = runtime.core().search_all(&q, &bl, &abl).await;
    if !is_current_page_version(version) {
        return;
    }
    let results = match results {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[qbz-qt] search failed: {e}");
            let mut guard = PAGE.lock().unwrap();
            let doc = &mut guard.get_or_insert_with(PageState::default).doc;
            doc.loading = false;
            doc.albums.clear();
            doc.tracks.clear();
            doc.artists.clear();
            doc.artists_carousel.clear();
            doc.playlists.clear();
            doc.albums_total = 0;
            doc.tracks_total = 0;
            doc.artists_total = 0;
            doc.playlists_total = 0;
            doc.most_popular = MostPopularDoc::default();
            publish_page(doc);
            return;
        }
    };
    log::info!(
        "[qbz-qt][perf] search page \"{q}\" -> results: {:?} ({}+{}+{}+{})",
        t.elapsed(),
        results.albums.items.len(),
        results.tracks.items.len(),
        results.artists.items.len(),
        results.playlists.items.len(),
    );

    let mut albums: Vec<CardRow> = results.albums.items.iter().map(map_album).collect();
    let mut tracks: Vec<TrackRow> = results.tracks.items.iter().map(map_track).collect();
    let mut artists: Vec<ArtistRow> = results.artists.items.iter().map(map_artist).collect();
    let mut playlists: Vec<PlaylistRow> = results.playlists.items.iter().map(map_playlist).collect();

    let (mp_kind, mut mp_album, mut mp_artist, mut mp_track, mp_quality) = match &results.most_popular {
        Some(MostPopularItem::Albums(a)) => ("album".to_string(), Some(map_album(a)), None, None, quality_label(a.maximum_bit_depth, a.maximum_sampling_rate)),
        Some(MostPopularItem::Artists(a)) => ("artist".to_string(), None, Some(map_artist(a)), None, String::new()),
        Some(MostPopularItem::Tracks(t)) => {
            ("track".to_string(), None, None, Some(map_track(t)), quality_label(t.maximum_bit_depth, t.maximum_sampling_rate))
        }
        None => (String::new(), None, None, None, String::new()),
    };
    let mut missing: Vec<String> = Vec::new();
    missing.extend(attach_urls(albums.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect()));
    missing.extend(attach_urls(tracks.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect()));
    missing.extend(attach_urls(artists.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect()));
    missing.extend(attach_urls(playlists.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect()));
    if let Some(a) = &mut mp_album {
        missing.extend(attach_urls(vec![(a.art_url.clone(), &mut a.art_path)]));
    }
    if let Some(a) = &mut mp_artist {
        missing.extend(attach_urls(vec![(a.art_url.clone(), &mut a.art_path)]));
    }
    if let Some(t) = &mut mp_track {
        missing.extend(attach_urls(vec![(t.art_url.clone(), &mut t.art_path)]));
    }
    missing.dedup();
    // The All-tab carousel skips the top-result artist when it is also the
    // first list entry (apply_search dedupe) — derived AFTER artwork attach
    // so its rows carry the disk paths (they are clones of `artists` rows).
    let artists_carousel: Vec<ArtistRow> = match (&mp_kind, artists.first()) {
        (kind, Some(first)) if kind == "artist" && mp_artist.as_ref().is_some_and(|m| m.id == first.id) => {
            artists[1..].to_vec()
        }
        _ => artists.clone(),
    };

    let doc = {
        let mut guard = PAGE.lock().unwrap();
        let doc = &mut guard.get_or_insert_with(PageState::default).doc;
        doc.loading = false;
        doc.albums = albums;
        doc.tracks = tracks;
        doc.artists_carousel = artists_carousel;
        doc.artists = artists;
        doc.playlists = playlists;
        doc.albums_total = results.albums.total as i32;
        doc.tracks_total = results.tracks.total as i32;
        doc.artists_total = results.artists.total as i32;
        doc.playlists_total = results.playlists.total as i32;
        doc.most_popular = MostPopularDoc {
            kind: mp_kind,
            album: mp_album,
            artist: mp_artist,
            track: mp_track,
            quality_label: mp_quality,
        };
        doc.clone()
    };
    publish_page(&doc);

    if !missing.is_empty() {
        crate::spawn(async move {
            crate::artwork_qt::download_missing(missing).await;
            if !is_current_page_version(version) {
                return;
            }
            let doc = {
                let mut guard = PAGE.lock().unwrap();
                let doc = &mut guard.get_or_insert_with(PageState::default).doc;
                let mut missing2 = Vec::new();
                missing2.extend(attach_urls(doc.albums.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect()));
                missing2.extend(attach_urls(doc.tracks.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect()));
                missing2.extend(attach_urls(doc.artists.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect()));
                missing2.extend(attach_urls(doc.playlists.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect()));
                // The carousel shares rows with `artists`; artPath lives on
                // the ArtistRow structs, so refresh the carousel from the
                // updated list.
                doc.artists_carousel = doc
                    .artists_carousel
                    .iter()
                    .map(|c| {
                        doc.artists
                            .iter()
                            .find(|a| a.id == c.id)
                            .cloned()
                            .unwrap_or_else(|| c.clone())
                    })
                    .collect();
                if let Some(a) = &mut doc.most_popular.album {
                    let _ = attach_urls(vec![(a.art_url.clone(), &mut a.art_path)]);
                }
                if let Some(a) = &mut doc.most_popular.artist {
                    let _ = attach_urls(vec![(a.art_url.clone(), &mut a.art_path)]);
                }
                if let Some(t) = &mut doc.most_popular.track {
                    let _ = attach_urls(vec![(t.art_url.clone(), &mut t.art_path)]);
                }
                let _ = missing2;
                doc.clone()
            };
            publish_page(&doc);
        });
    }
}

/// Tab strip: pure view state (search_all already loaded everything).
pub fn tab_changed(tab: i32) {
    let mut guard = PAGE.lock().unwrap();
    if let Some(page) = guard.as_mut() {
        page.doc.tab = tab;
        let doc = page.doc.clone();
        drop(guard);
        publish_page(&doc);
    }
}

/// search.rs PAGE_SIZE (matches the Tauri search page size).
const PAGE_SIZE: u32 = 20;

/// Load more rows for the active per-type tab (offset = rows already loaded).
pub async fn load_more(runtime: &Arc<AppRuntime<LoggingAdapter>>, tab: i32) {
    let (query, filter, offset) = {
        let guard = PAGE.lock().unwrap();
        let Some(page) = guard.as_ref() else { return };
        let offset = match tab {
            1 => page.doc.albums.len(),
            2 => page.doc.tracks.len(),
            3 => page.doc.artists.len(),
            4 => page.doc.playlists.len(),
            _ => return,
        };
        (page.doc.query.clone(), page.doc.filter_index, offset as u32)
    };
    let search_type = search_type_for_filter(filter);
    let version = next_page_version();
    // Page-2+ is NOT filtered by the API: `search_albums`/`search_tracks`/
    // `search_artists` take no snapshot, so the drop happens here, exactly as
    // the reference does it (`search.rs:1616-1666`). `offset` stays the VISIBLE
    // row count (`main.rs:9795-9800` reads `row_count()`), so a filtered page
    // shortens the batch rather than shifting the cursor — reference-identical.
    let (bl, abl) = blacklist_snapshots();

    match tab {
        1 => match runtime.core().search_albums(&query, PAGE_SIZE, offset, search_type.as_deref()).await {
            Ok(page) => {
                let mut rows: Vec<CardRow> = page
                    .items
                    .iter()
                    .filter(|a| !qbz_core::core::album_blacklisted(a, &bl, &abl))
                    .map(map_album)
                    .collect();
                let missing = attach_urls(rows.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect());
                let total = page.total as i32;
                let mut guard = PAGE.lock().unwrap();
                if let Some(p) = guard.as_mut() {
                    if is_current_page_version(version) {
                        p.doc.albums.extend(rows);
                        p.doc.albums_total = total;
                        let doc = p.doc.clone();
                        drop(guard);
                        publish_page(&doc);
                    }
                }
                let _ = missing;
            }
            Err(e) => log::error!("[qbz-qt] search load-more albums failed: {e}"),
        },
        2 => match runtime.core().search_tracks(&query, PAGE_SIZE, offset, search_type.as_deref()).await {
            Ok(page) => {
                let mut rows: Vec<TrackRow> = page
                    .items
                    .iter()
                    .filter(|t| !qbz_core::core::track_blacklisted(t, &bl, &abl))
                    .map(map_track)
                    .collect();
                let missing = attach_urls(rows.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect());
                let total = page.total as i32;
                let mut guard = PAGE.lock().unwrap();
                if let Some(p) = guard.as_mut() {
                    if is_current_page_version(version) {
                        p.doc.tracks.extend(rows);
                        p.doc.tracks_total = total;
                        let doc = p.doc.clone();
                        drop(guard);
                        publish_page(&doc);
                    }
                }
                let _ = missing;
            }
            Err(e) => log::error!("[qbz-qt] search load-more tracks failed: {e}"),
        },
        3 => match runtime.core().search_artists(&query, PAGE_SIZE, offset, search_type.as_deref()).await {
            Ok(page) => {
                // Artist axis ONLY — the reference filters this category on
                // `bl` alone (`search.rs:1655`); an artist has no album id.
                let mut rows: Vec<ArtistRow> = page
                    .items
                    .iter()
                    .filter(|a| !bl.contains(&a.id))
                    .map(map_artist)
                    .collect();
                let missing = attach_urls(rows.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect());
                let total = page.total as i32;
                let mut guard = PAGE.lock().unwrap();
                if let Some(p) = guard.as_mut() {
                    if is_current_page_version(version) {
                        p.doc.artists.extend(rows);
                        p.doc.artists_total = total;
                        let doc = p.doc.clone();
                        drop(guard);
                        publish_page(&doc);
                    }
                }
                let _ = missing;
            }
            Err(e) => log::error!("[qbz-qt] search load-more artists failed: {e}"),
        },
        4 => match runtime.core().search_playlists(&query, PAGE_SIZE, offset).await {
            Ok(page) => {
                let mut rows: Vec<PlaylistRow> = page.items.iter().map(map_playlist).collect();
                let missing = attach_urls(rows.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect());
                let total = page.total as i32;
                let mut guard = PAGE.lock().unwrap();
                if let Some(p) = guard.as_mut() {
                    if is_current_page_version(version) {
                        p.doc.playlists.extend(rows);
                        p.doc.playlists_total = total;
                        let doc = p.doc.clone();
                        drop(guard);
                        publish_page(&doc);
                    }
                }
                let _ = missing;
            }
            Err(e) => log::error!("[qbz-qt] search load-more playlists failed: {e}"),
        },
        _ => {}
    }
}

/// searchType filter radios: re-query the three filterable categories and
/// REPLACE their lists (the filter takes effect on every tab).
pub async fn filter_changed(runtime: &Arc<AppRuntime<LoggingAdapter>>, index: i32) {
    let query = {
        let mut guard = PAGE.lock().unwrap();
        let Some(page) = guard.as_mut() else { return };
        page.doc.filter_index = index;
        let doc = page.doc.clone();
        publish_page(&doc);
        doc.query
    };
    if query.trim().is_empty() {
        return;
    }
    let search_type = search_type_for_filter(index);
    let version = next_page_version();
    // The reference implements the filter radios by calling `load_more(.., 0)`
    // per category and `replace_category` (`main.rs:9843-9856`), so these three
    // re-queries carry the SAME post-filter as `load_more` above.
    let (bl, abl) = blacklist_snapshots();

    // Albums.
    if let Ok(page) = runtime.core().search_albums(&query, PAGE_SIZE, 0, search_type.as_deref()).await {
        let mut rows: Vec<CardRow> = page
            .items
            .iter()
            .filter(|a| !qbz_core::core::album_blacklisted(a, &bl, &abl))
            .map(map_album)
            .collect();
        let _ = attach_urls(rows.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect());
        let total = page.total as i32;
        let mut guard = PAGE.lock().unwrap();
        if let Some(p) = guard.as_mut() {
            if is_current_page_version(version) {
                p.doc.albums = rows;
                p.doc.albums_total = total;
                let doc = p.doc.clone();
                drop(guard);
                publish_page(&doc);
            }
        }
    }
    // Tracks.
    if let Ok(page) = runtime.core().search_tracks(&query, PAGE_SIZE, 0, search_type.as_deref()).await {
        let mut rows: Vec<TrackRow> = page
            .items
            .iter()
            .filter(|t| !qbz_core::core::track_blacklisted(t, &bl, &abl))
            .map(map_track)
            .collect();
        let _ = attach_urls(rows.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect());
        let total = page.total as i32;
        let mut guard = PAGE.lock().unwrap();
        if let Some(p) = guard.as_mut() {
            if is_current_page_version(version) {
                p.doc.tracks = rows;
                p.doc.tracks_total = total;
                let doc = p.doc.clone();
                drop(guard);
                publish_page(&doc);
            }
        }
    }
    // Artists (the API takes the search_type too, 1:1 the Slint filter).
    if let Ok(page) = runtime.core().search_artists(&query, PAGE_SIZE, 0, search_type.as_deref()).await {
        let mut rows: Vec<ArtistRow> = page
            .items
            .iter()
            .filter(|a| !bl.contains(&a.id))
            .map(map_artist)
            .collect();
        let _ = attach_urls(rows.iter_mut().map(|r| (r.art_url.clone(), &mut r.art_path)).collect());
        let total = page.total as i32;
        let mut guard = PAGE.lock().unwrap();
        if let Some(p) = guard.as_mut() {
            if is_current_page_version(version) {
                p.doc.artists = rows.clone();
                p.doc.artists_total = total;
                p.doc.artists_carousel = rows;
                let doc = p.doc.clone();
                drop(guard);
                publish_page(&doc);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_type_for_filter_maps_dropdown_index() {
        assert_eq!(search_type_for_filter(0), None);
        assert_eq!(search_type_for_filter(1), Some("MainArtist".to_string()));
        assert_eq!(search_type_for_filter(3), Some("Composer".to_string()));
        assert_eq!(search_type_for_filter(5), Some("ReleaseName".to_string()));
        assert_eq!(search_type_for_filter(99), None);
    }

    #[test]
    fn flat_indices_skip_zero_without_top_result() {
        let mut data = CortinillaData {
            query: "q".into(),
            top: None,
            sections: vec![CortSection {
                title: "Albums".into(),
                kind: "album".into(),
                has_more: false,
                rows: vec![
                    CortRow { kind: "album".into(), id: "1".into(), ..Default::default() },
                    CortRow { kind: "album".into(), id: "2".into(), ..Default::default() },
                ],
            }],
        };
        assign_flat_indices(&mut data);
        assert_eq!(data.sections[0].rows[0].flat_index, 1);
        assert_eq!(data.sections[0].rows[1].flat_index, 2);
    }

    #[test]
    fn flat_indices_top_is_zero() {
        let mut data = CortinillaData {
            query: "q".into(),
            top: Some(CortRow { kind: "artist".into(), id: "9".into(), ..Default::default() }),
            sections: vec![CortSection {
                title: "Albums".into(),
                kind: "album".into(),
                has_more: false,
                rows: vec![CortRow { kind: "album".into(), id: "1".into(), ..Default::default() }],
            }],
        };
        assign_flat_indices(&mut data);
        assert_eq!(data.top.as_ref().unwrap().flat_index, 0);
        assert_eq!(data.sections[0].rows[0].flat_index, 1);
    }

    #[test]
    fn quality_label_formats() {
        assert_eq!(quality_label(Some(24), Some(96.0)), "Hi-Res 24-bit / 96 kHz");
        assert_eq!(quality_label(Some(16), None), "CD 16-bit / 44.1 kHz");
        assert_eq!(quality_label(None, Some(192.0)), "");
        assert_eq!(mmss(5), "0:05");
        assert_eq!(mmss(225), "3:45");
        assert_eq!(tier(Some(24)), "hires");
        assert_eq!(tier(Some(16)), "cd");
        assert_eq!(tier(None), "");
    }
}
