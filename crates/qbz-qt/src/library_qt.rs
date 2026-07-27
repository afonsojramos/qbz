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
//! - Playlist follow/copy affordances: hearts route to the favorites API
//!   only; subscribe/unsubscribe is not wired.
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
    /// Recency proxy in [0,1]; 0 = most-recently added (per source).
    pub added_rank: f32,
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
    FeedItem {
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

fn map_playlist_row(playlist: &Playlist, is_following: bool) -> FeedItem {
    let cover_url = [&playlist.images300, &playlist.images150, &playlist.images]
        .into_iter()
        .flatten()
        .find(|v| !v.is_empty())
        .and_then(|list| list.iter().find(|u| !u.is_empty()).cloned())
        .unwrap_or_default();
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
        kind: "playlist".into(),
        group: if is_following { "following" } else { "favorites" }.into(),
        source: "qobuz".into(),
        id: playlist.id.to_string(),
        title: playlist.name.clone(),
        subtitle,
        image_url: cover_url,
        is_favorite: !is_following,
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
    // Hearted playlist ids from the per-user library.db (favorites.rs).
    let fav_pl_ids = crate::library_db_qt::favorite_playlist_ids();
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

/// Label image — label.rs `extract_label_image`: the nested "square" url.
fn extract_label_image(image: Option<&serde_json::Value>) -> String {
    let Some(image) = image else {
        return String::new();
    };
    for key in ["square", "large", "medium", "small", "thumbnail"] {
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

/// Flip a heart. Qobuz ids route to the favorites API; local/Plex ids to
/// the local-favorites store (library_all.rs `is_local_feed_id` routing).
/// Returns the new state, or None when the toggle failed.
pub async fn toggle_favorite(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    kind: &str,
    id: &str,
) -> Option<bool> {
    if is_local_feed_id(kind, id) {
        return toggle_local(kind, id);
    }
    let fav_type = match kind {
        "track" => "tracks",
        "album" => "albums",
        "artist" => "artists",
        "label" => "labels",
        "playlist" => "playlists",
        _ => return None,
    };
    let current = with_library(|d| {
        d.feed
            .iter()
            .any(|i| i.kind == kind && i.id == id && i.is_favorite)
    })?;
    let result = if current {
        runtime.core().remove_favorite(fav_type, id).await
    } else {
        runtime.core().add_favorite(fav_type, id).await
    };
    match result {
        Ok(()) => {
            let new_state = !current;
            set_feed_favorite(kind, id, new_state);
            Some(new_state)
        }
        Err(e) => {
            log::error!("[qbz-qt] favorite toggle ({kind}:{id}) failed: {e}");
            None
        }
    }
}

fn toggle_local(kind: &str, id: &str) -> Option<bool> {
    let service = LOCAL_FAVS.get()?.lock().unwrap();
    let new_state = if service.is_favorite(kind, id) {
        service.unfavorite(kind, id).ok()?;
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
        })??;
        service.favorite(&snap).ok()?;
        true
    };
    set_feed_favorite(kind, id, new_state);
    Some(new_state)
}

/// library_all.rs `is_local_feed_id` — local tracks are file paths, local
/// albums group keys, local artists plain names; Qobuz ids are numeric.
fn is_local_feed_id(kind: &str, id: &str) -> bool {
    match kind {
        "track" | "artist" => id.parse::<u64>().is_err(),
        "album" => id.starts_with("plex:") || id.contains('|') || id.contains('/'),
        _ => false,
    }
}

/// Flip the favorite flag on the stored feed row (model-of-truth for the
/// `libraryFavoriteChanged` signal).
fn set_feed_favorite(kind: &str, id: &str, value: bool) {
    with_library_mut(|d| {
        if let Some(item) = d.feed.iter_mut().find(|i| i.kind == kind && i.id == id) {
            item.is_favorite = value;
        }
    });
}
