//! Library view data layer — Slint-free port of `crates/qbz/src/favorites.rs`
//! (per-tab loaders) + `crates/qbz/src/library_all.rs` (the mixed All feed).
//!
//! One fetch fans out to every source (Qobuz favorites paged per type,
//! playlists, purchases, the local-favorites store), normalizes each into a
//! `FeedItem`, and merges the All feed by the recency proxy
//! (`added_rank = index / len` per source, stable sort — library_all.rs).
//!
//! Transport: ONE JSON document on the bridge (`libraryJson`); the QML side
//! parses once and derives tabs/search/sort/source-filters in JS (the Slint
//! app derives in Rust because Slint JS is weak — QML JS is real; the
//! JSON.parse cost at owner scale is MEASURED and reported, per the phase
//! brief). Artwork is id-keyed via the `libraryArtworkReady` signal (see
//! bridge.rs) so a cover never lands on the wrong row and decoded-cover RAM
//! scales with the viewport.
//!
//! POC-NOTEs:
//! - Artist/album blacklist filtering: skipped (store not open).
//! - Genre filter ("library-all" context): skipped (genre_filter glue is
//!   Slint-side; the toolbar button is an inert stub).
//! - Local scope "all" (the whole local library in the feed): NOT wired —
//!   it needs the qbz-library scan DB, which the POC never opens. The
//!   show-local toggle and the "favorites" scope (hearted local items) work
//!   via the LocalFavoritesService (now initialized, per owner).
//! - Playlist follow/copy affordances: a feed row's heart is the qbz-LOCAL
//!   library.db flag (`toggle_playlist_favorite` — Qobuz has no
//!   `playlist_ids` favorite param); subscribe/unsubscribe of a FOREIGN
//!   playlist only exists on the playlist page (`playlist_qt::toggle_follow`),
//!   not on the feed rows.
//! - Purchase download states / Play purchases / multi-select / group
//!   modes / alpha jumps: out of scope.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use qbz_app::settings::local_favorites::{LocalFavoritesService, DB_FILE_NAME};
use qbz_app::shell::AppRuntime;
use qbz_app::user_data::UserDataPaths;
use qbz_core::LoggingAdapter;
use qbz_models::{Album, Artist, Playlist, Track};
use serde::Serialize;

use crate::home_qt;

/// Matches favorites.rs paging.
const PAGE_SIZE: u32 = 500;
const MAX_ITEMS: usize = 10_000;

/// One normalized row (superset of the per-tab cards + the All feed item).
#[derive(Clone, Default, Serialize)]
pub struct FeedItem {
    pub kind: String,   // track | album | artist | playlist | label
    pub group: String,  // favorites | following | purchases | local
    pub source: String, // qobuz | local | plex
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub album: String,
    #[serde(rename = "albumId")]
    pub album_id: String,
    #[serde(rename = "imageUrl")]
    pub image_url: String,
    /// Stable artwork key (`{kind}:{id}`) — library_all.rs `feed_key`.
    #[serde(rename = "artKey")]
    pub art_key: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
    pub genre: String,
    pub year: String,
    pub duration: String,
    pub explicit: bool,
    #[serde(rename = "playlistOwned")]
    pub playlist_owned: bool,
    #[serde(rename = "playlistFollowing")]
    pub playlist_following: bool,
    #[serde(rename = "isPinned", default)]
    pub is_pinned: bool,
    /// Playlist rows only: up to four de-duplicated member-track covers —
    /// the 2x2 mosaic the card renders when the playlist has no artwork of
    /// its OWN (Tauri `FavoritePlaylistCard` -> `PlaylistCollage`, Slint
    /// `PlaylistCollage.slint`). Skipped when empty so the ~10k-row feed
    /// JSON does not grow an `"covers":[]` per non-playlist row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub covers: Vec<String>,
    /// True when `image_url` is the playlist's OWN Qobuz artwork
    /// (`image_rectangle`) instead of a member-track cover. The card then
    /// renders it CONTAIN — those graphics are landscape and cropping
    /// butchers them (Tauri `QobuzPlaylistCard`: `object-fit: contain`).
    #[serde(
        rename = "playlistOwnImage",
        default,
        skip_serializing_if = "is_false"
    )]
    pub playlist_own_image: bool,
    /// Recency proxy in [0,1]; 0 = most-recently added (per source).
    pub added_rank: f32,
}

/// `skip_serializing_if` predicate for the default-false flags above.
fn is_false(value: &bool) -> bool {
    !*value
}

impl FeedItem {
    fn keyed(mut self) -> Self {
        self.art_key = feed_key(&self.kind, &self.id);
        self
    }
}

/// library_all.rs `feed_key` — Qobuz numeric ids overlap across entity
/// types, so the kind prefixes the key.
pub fn feed_key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

#[derive(Clone, Serialize)]
pub struct LibraryCounts {
    pub tracks: i64,
    pub albums: i64,
    pub artists: i64,
    pub playlists: i64,
    pub labels: i64,
    pub all: i64,
}

pub struct LibraryData {
    pub feed: Vec<FeedItem>,
    pub counts: LibraryCounts,
}

static LIBRARY: Mutex<Option<LibraryData>> = Mutex::new(None);

fn rank(i: usize, n: usize) -> f32 {
    if n <= 1 {
        0.0
    } else {
        i as f32 / n as f32
    }
}

// ---------------------------------------------------------------------------
// Local favorites store (LocalFavoritesService, ADR-006 backend)
// ---------------------------------------------------------------------------

static LOCAL_FAVS: OnceLock<Mutex<LocalFavoritesService>> = OnceLock::new();

/// Open the per-user local-favorites DB. Called on every session
/// activation (login / restore / offline entry) next to
/// `offline_fwd::init_for_user`.
pub fn init_local_favorites(base_dir: &std::path::Path) {
    let path = base_dir.join(DB_FILE_NAME);
    match LocalFavoritesService::new(&path) {
        Ok(service) => {
            let _ = LOCAL_FAVS.set(Mutex::new(service));
            log::info!("[qbz-qt] local favorites store opened");
        }
        Err(e) => log::error!("[qbz-qt] local favorites store open failed: {e}"),
    }
}

fn local_favorites_list() -> Vec<qbz_app::settings::local_favorites::LocalFavItem> {
    LOCAL_FAVS
        .get()
        .and_then(|s| s.lock().unwrap().list().ok())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Fetch
// ---------------------------------------------------------------------------

/// Page one favorites type until exhausted (favorites.rs loop).
async fn fetch_favorites(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    key: &str,
) -> Result<(Vec<serde_json::Value>, usize), String> {
    let mut total: usize;
    let mut all_items: Vec<serde_json::Value> = Vec::new();
    let mut offset = 0u32;
    loop {
        let value = runtime
            .core()
            .get_favorites(key, PAGE_SIZE, offset)
            .await
            .map_err(|e| e.to_string())?;
        let branch = value.get(key);
        total = branch
            .and_then(|b| b.get("total"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as usize;
        let page: Vec<serde_json::Value> = branch
            .and_then(|b| b.get("items"))
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default();
        let page_len = page.len();
        all_items.extend(page);
        offset += page_len as u32;
        if page_len < PAGE_SIZE as usize
            || (total > 0 && offset as usize >= total)
            || all_items.len() >= MAX_ITEMS
        {
            break;
        }
    }
    Ok((all_items, total))
}

/// Refresh the favourite-id cache from the network — the online half of the
/// disk-first lifecycle (`fav_cache_qt::init_for_user` already seeded it at
/// session activation). Fired once per shell entry from `main::enter_shell`,
/// which is where the reference warms the same cache
/// (`crates/qbz/src/main.rs:418-500`, four `tokio::spawn`s next to the
/// sidebar load). Skipped offline: there the disk seed IS the truth.
///
/// Reuses `fetch_favorites` — the same paged reader the Library load uses, so
/// the paging/cap semantics cannot drift between the two. Each type is
/// independent: one failing (labels 400'd for months) must not cost the
/// other three.
pub async fn warm_favorites_cache(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let t = Instant::now();
    // Album ids are TEXT (Qobuz mixes numeric and alphanumeric catalog ids);
    // tracks / artists / labels are numeric.
    match fetch_favorites(runtime, "albums").await {
        Ok((items, _)) => {
            let ids: std::collections::HashSet<String> = items
                .iter()
                .filter_map(|it| it.get("id"))
                .filter_map(|v| match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect();
            let n = ids.len();
            let _ = tokio::task::spawn_blocking(move || crate::fav_cache_qt::set_all_albums(ids)).await;
            log::info!("[qbz-qt] favorites cache warmed: {n} albums");
        }
        Err(e) => log::warn!("[qbz-qt] favorites cache album warm failed: {e}"),
    }
    warm_numeric(runtime, "tracks", crate::fav_cache_qt::set_all_tracks).await;
    warm_numeric(runtime, "artists", crate::fav_cache_qt::set_all_artists).await;
    // PLURAL here, singular on `/favorite/create|delete` — see the
    // `toggle_favorite` note. This is the `type` param's spelling.
    warm_numeric(runtime, "labels", crate::fav_cache_qt::set_all_labels).await;
    log::info!("[qbz-qt][perf] favorites cache warm: {:?}", t.elapsed());
}

/// One numeric favourites type -> its cache setter. `apply` is a plain fn
/// pointer so the blocking disk write can move onto a blocking hop.
async fn warm_numeric(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    key: &str,
    apply: fn(std::collections::HashSet<u64>),
) {
    match fetch_favorites(runtime, key).await {
        Ok((items, _)) => {
            let ids: std::collections::HashSet<u64> = items
                .iter()
                .filter_map(|it| it.get("id").and_then(|v| v.as_u64()))
                .collect();
            let n = ids.len();
            let _ = tokio::task::spawn_blocking(move || apply(ids)).await;
            log::info!("[qbz-qt] favorites cache warmed: {n} {key}");
        }
        Err(e) => log::warn!("[qbz-qt] favorites cache {key} warm failed: {e}"),
    }
}

fn parse_items<T: serde::de::DeserializeOwned>(items: Vec<serde_json::Value>, what: &str) -> Vec<T> {
    items
        .into_iter()
        .filter_map(|v| match serde_json::from_value::<T>(v) {
            Ok(item) => Some(item),
            Err(e) => {
                log::warn!("[qbz-qt] dropping malformed {what}: {e}");
                None
            }
        })
        .collect()
}

fn mmss(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

// ---- mappers (favorites.rs / album_map.rs / search.rs ports) --------------

fn map_track(track: Track) -> FeedItem {
    let mut title = track.title;
    if let Some(version) = track.version.as_ref().filter(|v| !v.is_empty()) {
        title = format!("{title} ({version})");
    }
    let artwork_url = track
        .album
        .as_ref()
        .and_then(|a| a.image.best().cloned())
        .unwrap_or_default();
    let album = track
        .album
        .as_ref()
        .map(|a| a.title.clone())
        .unwrap_or_default();
    let album_id = track.album.as_ref().map(|a| a.id.clone()).unwrap_or_default();
    let genre = track
        .album
        .as_ref()
        .and_then(|a| a.genre.as_ref())
        .map(|g| g.name.clone())
        .unwrap_or_default();
    let (artist, artist_id) = track
        .performer
        .map(|p| (p.name, p.id.to_string()))
        .unwrap_or_default();
    FeedItem {
        kind: "track".into(),
        group: "favorites".into(),
        source: "qobuz".into(),
        id: track.id.to_string(),
        title,
        subtitle: artist.clone(),
        artist,
        artist_id,
        album,
        album_id,
        genre,
        duration: mmss(track.duration),
        quality_tier: home_qt::quality_tier_from_depth(track.maximum_bit_depth).to_string(),
        quality_detail: home_qt::quality_detail_from_parts(
            track.maximum_bit_depth,
            track.maximum_sampling_rate,
        ),
        explicit: track.parental_warning,
        image_url: artwork_url,
        is_favorite: true,
        ..Default::default()
    }
    .keyed()
}

fn map_album(album: Album) -> FeedItem {
    let bit_depth = album
        .audio_info
        .as_ref()
        .and_then(|a| a.maximum_bit_depth)
        .or(album.maximum_bit_depth);
    let sample_rate = album
        .audio_info
        .as_ref()
        .and_then(|a| a.maximum_sampling_rate)
        .or(album.maximum_sampling_rate);
    let date = album
        .dates
        .as_ref()
        .and_then(|d| d.original.clone().or(d.download.clone()).or(d.stream.clone()))
        .or(album.release_date_original.clone());
    let artist = if !album.artist.name.is_empty() {
        album.artist.name
    } else {
        album
            .artists
            .as_ref()
            .and_then(|c| c.first().map(|a| a.name.clone()))
            .unwrap_or_default()
    };
    let is_pinned = crate::sidebar_qt::is_pinned("album", &album.id);
    FeedItem {
        is_pinned,
        kind: "album".into(),
        group: "favorites".into(),
        source: "qobuz".into(),
        id: album.id,
        title: album.title,
        subtitle: artist.clone(),
        artist,
        artist_id: album.artist.id.to_string(),
        genre: album.genre.map(|g| g.name).unwrap_or_default(),
        year: qbz_text_utils::dates::release_label(date.as_deref()),
        quality_tier: home_qt::quality_tier_from_depth(bit_depth).to_string(),
        quality_detail: home_qt::quality_detail_from_parts(bit_depth, sample_rate),
        image_url: album.image.best().cloned().unwrap_or_default(),
        is_favorite: true,
        ..Default::default()
    }
    .keyed()
}

fn map_artist(artist: Artist) -> FeedItem {
    FeedItem {
        // Pin badge state from the per-user store. The album mapper above
        // has always seeded it; the artist and playlist rows did not, so
        // every artist/playlist card in the Library grid drew the hollow
        // glyph and the first click un-pinned what the user meant to pin.
        is_pinned: crate::sidebar_qt::is_pinned("artist", &artist.id.to_string()),
        kind: "artist".into(),
        group: "following".into(),
        source: "qobuz".into(),
        id: artist.id.to_string(),
        title: artist.name,
        image_url: artist
            .image
            .and_then(|img| img.best().cloned())
            .unwrap_or_default(),
        is_favorite: true,
        ..Default::default()
    }
    .keyed()
}

/// Up to four de-duplicated member-track covers (`images300` > `images150`
/// > `images`) — the collage source. Same picker as `sidebar_qt::load` and
/// `qbz::search::playlist_cover_urls`; these lists are the covers of the
/// playlist's MEMBER ALBUMS, never the playlist's own artwork.
fn playlist_cover_urls(playlist: &Playlist) -> Vec<String> {
    let source = [&playlist.images300, &playlist.images150, &playlist.images]
        .into_iter()
        .flatten()
        .find(|v| !v.is_empty());
    let mut out: Vec<String> = Vec::new();
    if let Some(list) = source {
        for url in list {
            if url.is_empty() || out.contains(url) {
                continue;
            }
            out.push(url.clone());
            if out.len() == 4 {
                break;
            }
        }
    }
    out
}

/// The playlist's OWN Qobuz artwork: the editorial `image_rectangle`
/// graphic (`image_rectangle_mini` as the lighter fallback). Only Qobuz
/// editorial playlists carry one — user-created playlists have none and
/// fall back to the member-cover collage, exactly the Tauri split between
/// `QobuzPlaylistCard` (`image.rectangle`, contain-fitted) and
/// `FavoritePlaylistCard` (`PlaylistCollage`).
fn playlist_own_image(playlist: &Playlist) -> String {
    [&playlist.image_rectangle, &playlist.image_rectangle_mini]
        .into_iter()
        .flatten()
        .find_map(|list| list.iter().find(|u| !u.is_empty()).cloned())
        .unwrap_or_default()
}

fn map_playlist_row(playlist: &Playlist, is_following: bool) -> FeedItem {
    // The card's single image is the playlist's own Qobuz artwork ONLY.
    // Falling back to `images300[0]` here is what put a member ALBUM cover
    // on every playlist card; the mosaic is fed separately via `covers`.
    let cover_url = playlist_own_image(playlist);
    let covers = playlist_cover_urls(playlist);
    let mut subtitle = playlist.owner.name.clone();
    if playlist.tracks_count > 0 {
        let count = playlist.tracks_count;
        let tracks_label = qbz_i18n::tf("{} track", "{} tracks", count as i64, &[&count.to_string()]);
        subtitle = if subtitle.is_empty() {
            tracks_label
        } else {
            format!("{}   •   {}", subtitle, tracks_label)
        };
    }
    let uid = UserDataPaths::load_last_user_id();
    let owned = uid.map(|uid| uid == playlist.owner.id).unwrap_or(false);
    FeedItem {
        // Pin badge state from the per-user store (see `map_artist`).
        is_pinned: crate::sidebar_qt::is_pinned("playlist", &playlist.id.to_string()),
        kind: "playlist".into(),
        group: if is_following { "following" } else { "favorites" }.into(),
        source: "qobuz".into(),
        id: playlist.id.to_string(),
        title: playlist.name.clone(),
        subtitle,
        playlist_own_image: !cover_url.is_empty(),
        image_url: cover_url,
        covers,
        // The library.db heart, NOT "this row landed in the favorites group".
        // `!is_following` answers a different question — which BUCKET the row
        // was fetched into — and the cards render this field as the heart while
        // `toggle_playlist_favorite` takes its direction from library.db. With
        // the two conflated, every OWNED playlist drew a filled heart reading
        // "Remove from Library" even when it had never been hearted, and the
        // click wrote `false` over a flag that was already false. The bucket is
        // still available to callers as `group` / `playlist_following`.
        is_favorite: crate::fav_cache_qt::is_favorite("playlist", &playlist.id.to_string()),
        playlist_owned: owned,
        playlist_following: is_following,
        ..Default::default()
    }
    .keyed()
}

/// The whole library load: fan out, normalize, merge (library_all.rs
/// ordering semantics), compute the tab counts.
pub async fn load_library(runtime: &Arc<AppRuntime<LoggingAdapter>>) -> Result<usize, String> {
    let t_fetch = Instant::now();

    // Favorites per type (paged) — sequential shares one client; measured
    // individually for the perf report.
    let t = Instant::now();
    let (raw_tracks, tracks_total) = fetch_favorites(runtime, "tracks").await?;
    log::info!("[qbz-qt][perf] favorites tracks fetch: {:?} ({} items)", t.elapsed(), raw_tracks.len());
    let t = Instant::now();
    let (raw_albums, albums_total) = fetch_favorites(runtime, "albums").await?;
    log::info!("[qbz-qt][perf] favorites albums fetch: {:?} ({} items)", t.elapsed(), raw_albums.len());
    let t = Instant::now();
    let (raw_artists, artists_total) = fetch_favorites(runtime, "artists").await?;
    log::info!("[qbz-qt][perf] favorites artists fetch: {:?} ({} items)", t.elapsed(), raw_artists.len());
    let t = Instant::now();
    let (raw_labels, labels_total) = fetch_favorites(runtime, "labels").await?;
    log::info!("[qbz-qt][perf] favorites labels fetch: {:?} ({} items)", t.elapsed(), raw_labels.len());

    // Playlists (owned + followed + locally hearted).
    let t = Instant::now();
    let all_playlists = runtime
        .core()
        .get_user_playlists()
        .await
        .map_err(|e| e.to_string())?;
    // Refresh the playlist ownership / follow snapshot from this same
    // response (see `playlist_qt::set_user_playlists`) — the Library load is
    // the second of the two places that fetch the user's playlist list, and
    // the fresher of the two.
    {
        let pairs: Vec<(u64, u64)> = all_playlists.iter().map(|p| (p.id, p.owner.id)).collect();
        crate::playlist_qt::set_user_playlists(&pairs);
    }
    let uid = UserDataPaths::load_last_user_id();
    let following: Vec<FeedItem> = match uid {
        Some(uid) => all_playlists
            .iter()
            .filter(|p| p.owner.id != uid)
            .map(|p| map_playlist_row(p, true))
            .collect(),
        None => Vec::new(),
    };
    let mut pl_favorites: Vec<FeedItem> = Vec::new();
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
    if let Some(uid) = uid {
        for p in all_playlists.iter().filter(|p| p.owner.id == uid) {
            seen.insert(p.id);
            pl_favorites.push(map_playlist_row(p, false));
        }
    }
    // Hearted playlist ids from the per-user library.db (favorites.rs). This
    // is the SAME set `fav_cache_qt` mirrors for the card producers, so
    // refresh the mirror while it is in hand — the session seed was taken at
    // activation and a heart set from another surface since then would
    // otherwise only reach it through that surface's own write-through.
    let fav_pl_ids = crate::library_db_qt::favorite_playlist_ids();
    crate::fav_cache_qt::set_all_playlists(fav_pl_ids.iter().copied().collect());
    for fid in fav_pl_ids {
        if !seen.insert(fid) {
            continue;
        }
        if let Some(p) = all_playlists.iter().find(|p| p.id == fid) {
            pl_favorites.push(map_playlist_row(p, p.owner.id != uid.unwrap_or(0)));
        } else if let Ok(p) = runtime.core().get_playlist(fid).await {
            pl_favorites.push(map_playlist_row(&p, false));
        }
    }
    let playlists_total = pl_favorites.len() as i64;
    log::info!("[qbz-qt][perf] playlists fetch: {:?} ({} fav / {} following)", t.elapsed(), pl_favorites.len(), following.len());

    // Purchases (group "purchases") — best-effort: a purchase-less account
    // returns empty; a failure must not sink the library.
    let t = Instant::now();
    let (purchase_albums, purchase_tracks) = fetch_purchases(runtime).await;
    log::info!("[qbz-qt][perf] purchases fetch: {:?} ({} albums / {} tracks)", t.elapsed(), purchase_albums.len(), purchase_tracks.len());

    log::info!("[qbz-qt][perf] total fetch wall: {:?}", t_fetch.elapsed());

    // ---- Map + merge ------------------------------------------------------
    let t_map = Instant::now();
    let mut feed: Vec<FeedItem> = Vec::new();

    let tracks: Vec<Track> = parse_items(raw_tracks, "track");
    let n = tracks.len();
    for (i, item) in tracks.into_iter().map(map_track).enumerate() {
        let mut item = item;
        item.added_rank = rank(i, n);
        feed.push(item);
    }
    let albums: Vec<Album> = parse_items(raw_albums, "album");
    let n = albums.len();
    for (i, item) in albums.into_iter().map(map_album).enumerate() {
        let mut item = item;
        item.added_rank = rank(i, n);
        feed.push(item);
    }
    let artists: Vec<Artist> = parse_items(raw_artists, "artist");
    let n = artists.len();
    for (i, item) in artists.into_iter().map(map_artist).enumerate() {
        let mut item = item;
        item.added_rank = rank(i, n);
        feed.push(item);
    }
    let n = pl_favorites.len();
    for (i, item) in pl_favorites.into_iter().enumerate() {
        let mut item = item;
        item.added_rank = rank(i, n);
        feed.push(item);
    }
    let n = following.len();
    for (i, item) in following.into_iter().enumerate() {
        let mut item = item;
        item.added_rank = rank(i, n);
        feed.push(item);
    }
    let n = purchase_albums.len();
    for (i, mut item) in purchase_albums.into_iter().enumerate() {
        item.added_rank = rank(i, n);
        feed.push(item);
    }
    let n = purchase_tracks.len();
    for (i, mut item) in purchase_tracks.into_iter().enumerate() {
        item.added_rank = rank(i, n);
        feed.push(item);
    }

    // Labels (no card artwork fields beyond the image + "{n} albums" line).
    /// favorites.rs-local richer label shape (image + count).
    #[derive(serde::Deserialize)]
    struct FavLabel {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        name: String,
        #[serde(default)]
        albums_count: Option<u32>,
        #[serde(default)]
        image: Option<serde_json::Value>,
    }
    let labels: Vec<FavLabel> = parse_items(raw_labels, "label");
    let n = labels.len();
    for (i, l) in labels.into_iter().enumerate() {
        feed.push(FeedItem {
            kind: "label".into(),
            group: "following".into(),
            source: "qobuz".into(),
            subtitle: match l.albums_count {
                Some(c) if c > 0 => format!("{c} albums"),
                _ => String::new(),
            },
            id: l.id.to_string(),
            title: l.name,
            image_url: extract_label_image(l.image.as_ref()),
            is_favorite: true,
            added_rank: rank(i, n),
            ..Default::default()
        }
        .keyed());
    }

    // Local favorites layer (show-local default ON; scope "favorites" —
    // see module docs for the "all" scope cut).
    let locals = local_favorites_list();
    let n = locals.len();
    for (i, lf) in locals.into_iter().enumerate() {
        feed.push(FeedItem {
            kind: lf.kind,
            group: "local".into(),
            source: lf.source,
            subtitle: lf.subtitle,
            artist: lf.artist.clone(),
            image_url: lf.artwork_url,
            is_favorite: true,
            added_rank: rank(i, n),
            id: lf.id,
            title: lf.title,
            ..Default::default()
        }
        .keyed());
    }

    // Merge by recency proxy (stable — equal ranks keep source order).
    feed.sort_by(|a, b| {
        a.added_rank
            .partial_cmp(&b.added_rank)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let all_total = feed.len() as i64;
    log::info!(
        "[qbz-qt][perf] map+merge: {:?} ({} feed items)",
        t_map.elapsed(),
        feed.len()
    );

    let counts = LibraryCounts {
        tracks: tracks_total as i64,
        albums: albums_total as i64,
        artists: artists_total as i64,
        playlists: playlists_total,
        labels: labels_total as i64,
        all: all_total,
    };
    *LIBRARY.lock().unwrap() = Some(LibraryData { feed, counts });
    Ok(all_total as usize)
}

/// Purchases via the offline-cache crate's pass-through (best-effort).
async fn fetch_purchases(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
) -> (Vec<FeedItem>, Vec<FeedItem>) {
    let client_lock = runtime.core().client();
    let client = {
        let guard = client_lock.read().await;
        guard.as_ref().cloned()
    };
    let Some(client) = client else {
        return (Vec::new(), Vec::new());
    };
    let Ok(response) = qbz_offline_cache::purchases_service::get_user_purchases_all(&client).await
    else {
        return (Vec::new(), Vec::new());
    };
    let filtered =
        qbz_offline_cache::purchases_service::filter_purchase_response(response, "");
    let albums = filtered
        .albums
        .items
        .into_iter()
        .map(|a| {
            let tier = if a.hires { "hires" } else { "cd" };
            FeedItem {
                // Purchase rows are the ONE group in this feed whose flags are
                // not implied by the group itself: a purchased album can also
                // be pinned and can also be a favourite, and both were left at
                // their `Default` false. The card then drew a hollow heart and
                // an unpinned badge over an album that IS both, and the first
                // click on either REMOVED it. Same two O(1) reads every other
                // producer does.
                is_pinned: crate::sidebar_qt::is_pinned("album", &a.id),
                is_favorite: crate::fav_cache_qt::is_album_favorite(&a.id),
                kind: "album".into(),
                group: "purchases".into(),
                source: "qobuz".into(),
                subtitle: a.artist.name.clone(),
                artist: a.artist.name,
                artist_id: a.artist.id.to_string(),
                image_url: a.image.best().cloned().unwrap_or_default(),
                quality_tier: tier.into(),
                genre: a.genre.as_ref().map(|g| g.name.clone()).unwrap_or_default(),
                id: a.id,
                title: a.title,
                ..Default::default()
            }
            .keyed()
        })
        .collect();
    let tracks = filtered
        .tracks
        .items
        .into_iter()
        .map(|t| {
            let (img, alb, aid) = t
                .album
                .as_ref()
                .map(|a| (a.image.best().cloned().unwrap_or_default(), a.title.clone(), a.id.clone()))
                .unwrap_or_default();
            let tier = if t.hires { "hires" } else { "cd" };
            FeedItem {
                // Tracks are not pinnable, but they ARE favouritable — see the
                // album arm above for why this cannot stay at Default::false.
                is_favorite: crate::fav_cache_qt::contains_track(t.id),
                kind: "track".into(),
                group: "purchases".into(),
                source: "qobuz".into(),
                subtitle: t.performer.name.clone(),
                artist: t.performer.name,
                artist_id: t.performer.id.to_string(),
                album: alb,
                album_id: aid,
                image_url: img,
                quality_tier: tier.into(),
                id: t.id.to_string(),
                title: t.title,
                ..Default::default()
            }
            .keyed()
        })
        .collect();
    (albums, tracks)
}

/// Label image — 1:1 with the reference `label.rs::extract_label_image`.
/// The value is flexible: the favorites wire ships a BARE STRING (Android
/// DTO), `/label/page` ships an object keyed mega/extralarge/large/... .
/// The string arm must come first — `Value::get(key)` on a string is always
/// `None`, so without it every favourite label resolved to "" and the card
/// fell back to the gradient placeholder. The `is_empty` guard is a port
/// addition (the reference lacks it) and cannot regress the key order.
fn extract_label_image(image: Option<&serde_json::Value>) -> String {
    let Some(image) = image else {
        return String::new();
    };
    if let Some(url) = image.as_str() {
        return url.to_string();
    }
    for key in ["mega", "extralarge", "large", "thumbnail", "small"] {
        if let Some(url) = image.get(key).and_then(|v| v.as_str()) {
            if !url.is_empty() {
                return url.to_string();
            }
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Accessors for the bridge / favorite toggles
// ---------------------------------------------------------------------------

pub fn with_library<R>(f: impl FnOnce(&LibraryData) -> R) -> Option<R> {
    LIBRARY.lock().unwrap().as_ref().map(f)
}

pub fn with_library_mut<R>(f: impl FnOnce(&mut LibraryData) -> R) -> Option<R> {
    LIBRARY.lock().unwrap().as_mut().map(f)
}

/// THE heart-state read for a Qobuz entity — the one every publisher and the
/// toggle direction must go through.
///
/// Two sources, in cost order:
///  1. `fav_cache_qt` — O(1), seeded from `favorites_cache.db` at session
///     activation and refreshed by `warm_favorites_cache` at shell entry, so
///     it is populated from first paint and while offline.
///  2. the Library feed — a linear scan, populated only after the user opens
///     the Library view, kept as a fast path in the sense the brief meant it:
///     it is the freshest thing there is once loaded, so a favourite that
///     landed after the warm still lights up.
///
/// The two never disagree for long because every mutation writes BOTH (see
/// `toggle_favorite` / `set_feed_favorite`), which is what makes the OR safe:
/// an un-favourite clears the cache entry AND the feed flag.
///
/// Playlists are not Qobuz favourites at all — their flag lives in
/// `library.db` (see `library_db_qt`) — but they ARE in the cache: it keeps a
/// mirror of that set precisely so this function and
/// `toggle_playlist_favorite` read one authority instead of two. Local / Plex
/// rows are not in the cache (their authority is the local-favorites store)
/// and fall through to the feed.
///
/// COST: the feed arm is a linear scan of up to 10k rows. That is correct for
/// a detail HEADER (one call per page open) and wrong for a card rail — row
/// producers call `fav_cache_qt::is_favorite` directly, which is the O(1) half
/// alone. See its docs.
pub fn is_favorite(kind: &str, id: &str) -> bool {
    if !is_local_feed_id(kind, id) && crate::fav_cache_qt::is_favorite(kind, id) {
        return true;
    }
    // Playlists STOP at the cache. For album/track/artist/label the feed flag
    // and the cache flag answer the same question, so OR-ing the feed in only
    // widens coverage. A playlist row's `is_favorite` used to mean "fetched
    // into the favorites bucket", and even now that it mirrors library.db the
    // feed is a stale snapshot of it — one that a toggle does not refresh. The
    // OR would therefore resurrect a heart the user just cleared, in the one
    // caller this rule sends here (the playlist header), which is precisely
    // the display-vs-direction split the cache exists to close.
    if kind == "playlist" {
        return false;
    }
    with_library(|d| {
        d.feed
            .iter()
            .any(|i| i.kind == kind && i.id == id && i.is_favorite)
    })
    .unwrap_or(false)
}

/// Flip a heart. Qobuz ids route to the favorites API; local/Plex ids to
/// the local-favorites store (library_all.rs `is_local_feed_id` routing);
/// playlists to the local library.db (see `toggle_playlist_favorite`).
///
/// Returns the state the UI should now show: the FLIPPED value on success,
/// the UNCHANGED one when the write failed (so the caller's optimistic flip
/// rolls back instead of leaving a heart lit over a 404), and None only when
/// the kind is unroutable or the store could not be reached at all.
///
/// THE TYPE STRING IS SINGULAR. `/favorite/create` and `/favorite/delete`
/// take `album_ids | artist_ids | track_ids | label_ids | award_ids`
/// (inferred OpenAPI v10.0.0.0-beta, §Favorites), and the client builds the
/// query key as `format!("{fav_type}_ids")` (qbz-qobuz client.rs
/// `add_favorite`/`remove_favorite`) — so the reference passes "track" /
/// "album" / "artist" / "label" everywhere (qbz/src/main.rs:2265, :12082,
/// :12942, :13054). Passing the PLURAL `type` spelling that
/// `/favorite/getUserFavorites` wants produced `artists_ids=…`: an unknown
/// param, so the delete matched nothing and came back 404 Not Found, while
/// the create silently no-opped with the heart left lit. Same word, two
/// endpoints, two conventions — do not unify them.
pub async fn toggle_favorite(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    kind: &str,
    id: &str,
) -> Option<bool> {
    if is_local_feed_id(kind, id) {
        return toggle_local(kind, id);
    }
    if kind == "playlist" {
        return toggle_playlist_favorite(id).await;
    }
    let fav_type = match kind {
        "track" | "album" | "artist" | "label" => kind,
        _ => {
            log::warn!("[qbz-qt] favorite toggle: unroutable kind '{kind}' ({id})");
            return None;
        }
    };
    // Direction from `is_favorite` — cache first, feed second. It used to come
    // from the feed ALONE, which `load_library_once()` fills only when the user
    // opens the Library view (main.rs `navigate_to("library")`): until then
    // every read answered false, so every click sent `favorite/create` — a
    // server no-op — while the UI flipped to filled. Un-favouriting was
    // impossible from every detail page, card and row. The publishers read the
    // same function now, so the drawn heart and the action cannot disagree.
    let current = is_favorite(kind, id);
    let result = if current {
        runtime.core().remove_favorite(fav_type, id).await
    } else {
        runtime.core().add_favorite(fav_type, id).await
    };
    match result {
        Ok(()) => {
            let new_state = !current;
            set_feed_favorite(kind, id, new_state);
            // Mirror to memory + `favorites_cache.db`, so the next page open
            // (and the next launch) agrees with what just happened.
            crate::fav_cache_qt::set(kind, id, new_state);
            Some(new_state)
        }
        Err(e) => {
            log::error!("[qbz-qt] favorite toggle ({kind}:{id}) failed: {e}");
            // Report the UNCHANGED state: the caller flipped optimistically.
            Some(current)
        }
    }
}

/// The playlist heart is a qbz-LOCAL flag, never a Qobuz favorite — the
/// favorites endpoints have no `playlist_ids` param (inferred OpenAPI
/// §Favorites lists five, none of them playlists), which is exactly why the
/// reference sends this to `db.set_playlist_favorite` instead
/// (qbz/src/main.rs:13652 + `playlist_toggle_favorite_by_id`:2196). Routing
/// it to `/favorite/create` produced `playlists_ids=…`, i.e. a create that
/// changed nothing and a delete that 404'd.
///
/// Direction comes from the db, not from the caller's rendered state: a card
/// cannot know it (main.rs:2198, verbatim reasoning). Follow/unfollow of a
/// FOREIGN playlist is a different action entirely (`playlist/subscribe`,
/// wired in `playlist_qt::toggle_follow`) and is not touched here.
async fn toggle_playlist_favorite(id: &str) -> Option<bool> {
    let Ok(pid) = id.parse::<u64>() else {
        log::warn!("[qbz-qt] playlist favorite: non-Qobuz id '{id}' — refusing");
        return None;
    };
    // rusqlite off the async path (library_db_qt holds a blocking Connection).
    // Read + write in ONE blocking hop so nothing can interleave between the
    // direction read and the write.
    let new_state = tokio::task::spawn_blocking(move || {
        let current = crate::library_db_qt::is_favorite_playlist(pid);
        let next = !current;
        if crate::library_db_qt::set_favorite_playlist(pid, next) {
            // Write-through to the in-memory mirror IN THE SAME HOP, so no
            // card can be built between the db write and the mirror update
            // and read the pre-toggle value.
            crate::fav_cache_qt::set_playlist(pid, next);
            next
        } else {
            log::error!("[qbz-qt] playlist {pid} favorite write failed");
            current
        }
    })
    .await
    .ok()?;
    set_feed_favorite("playlist", id, new_state);
    Some(new_state)
}

/// Local/Plex heart against the local-favorites store. Same contract as
/// `toggle_favorite`: the flipped state on success, the UNCHANGED state when
/// the store write failed (so the caller's optimistic flip rolls back — the
/// `.ok()?` arms used to swallow that as "nothing happened" and the heart
/// kept the flip), None only when there is no store to talk to at all.
fn toggle_local(kind: &str, id: &str) -> Option<bool> {
    let service = LOCAL_FAVS.get()?.lock().unwrap();
    let current = service.is_favorite(kind, id);
    let new_state = if current {
        if let Err(e) = service.unfavorite(kind, id) {
            log::error!("[qbz-qt] local unfavorite ({kind}:{id}) failed: {e}");
            return Some(current);
        }
        false
    } else {
        // Re-favorite: rebuild the snapshot from the feed row.
        let snap = with_library(|d| {
            d.feed
                .iter()
                .find(|i| i.kind == kind && i.id == id)
                .map(|item| qbz_app::settings::local_favorites::LocalFavItem {
                    kind: kind.to_string(),
                    id: id.to_string(),
                    title: item.title.clone(),
                    subtitle: item.subtitle.clone(),
                    artwork_url: item.image_url.clone(),
                    artist: item.artist.clone(),
                    source: item.source.clone(),
                    favorited_at: 0,
                })
        })
        .flatten();
        let Some(snap) = snap else {
            log::warn!("[qbz-qt] local favorite ({kind}:{id}): no feed row to snapshot");
            return Some(current);
        };
        if let Err(e) = service.favorite(&snap) {
            log::error!("[qbz-qt] local favorite ({kind}:{id}) failed: {e}");
            return Some(current);
        }
        true
    };
    set_feed_favorite(kind, id, new_state);
    Some(new_state)
}

/// library_all.rs `is_local_feed_id` — local tracks are file paths, local
/// albums group keys, local artists plain names; Qobuz ids are numeric.
pub(crate) fn is_local_feed_id(kind: &str, id: &str) -> bool {
    match kind {
        "track" | "artist" => id.parse::<u64>().is_err(),
        "album" => id.starts_with("plex:") || id.contains('|') || id.contains('/'),
        _ => false,
    }
}

/// main.rs `is_local_album_key` (qbz/src/main.rs:2630-2632) — the PLAY / NAV
/// guard, a SUPERSET of `is_local_feed_id("album", …)`: it also catches the
/// unknown-album bucket. Deliberately a separate helper: `is_local_feed_id` is
/// a byte-for-byte port of the Slint's `library_all.rs:874-880` twin and also
/// decides local-favorites routing, so widening it there would diverge from
/// its reference AND change which API an album literally titled
/// `__unknown_album__` hits.
pub(crate) fn is_local_album_key(id: &str) -> bool {
    is_local_feed_id("album", id) || id == "__unknown_album__"
}

/// Flip the favorite flag on the stored feed row (model-of-truth for the
/// `libraryFavoriteChanged` signal).
///
/// ALL matching rows, not the first: one entity can appear twice in the merged
/// feed under the same `(kind, id)` — a purchased album that is also a
/// favourite lands once in the `favorites` group and once in `purchases`.
/// `find` updated only one of them, and since `is_favorite` answers with
/// `any(...)`, an un-favourite that left the second row lit kept reading
/// "favourite" and the next click re-added it.
fn set_feed_favorite(kind: &str, id: &str, value: bool) {
    with_library_mut(|d| {
        for item in d.feed.iter_mut().filter(|i| i.kind == kind && i.id == id) {
            item.is_favorite = value;
        }
    });
}

/// A pin/unpin just landed: patch the cached feed rows so a later
/// serialization of this document does not resurrect the stale flag.
///
/// Twin of `home_qt::apply_pin_change` / `search_qt::apply_pin_change`, and
/// like them it publishes NOTHING: the rows on screen are corrected in place
/// by `QbzLibrary.pinChanged`, which `LibraryView.qml` listens to, and
/// re-serializing a 10k-row feed per pin click is exactly the delegate-model
/// teardown this split exists to avoid.
pub(crate) fn apply_pin_change(kind: &str, id: &str, pinned: bool) {
    with_library_mut(|d| {
        for item in d.feed.iter_mut().filter(|i| i.kind == kind && i.id == id) {
            item.is_pinned = pinned;
        }
    });
}
