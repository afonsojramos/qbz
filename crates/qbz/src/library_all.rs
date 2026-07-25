//! Library "All" — the mixed feed controller (webplayer /user-library/all).
//!
//! There is NO single Qobuz endpoint for the aggregated library; the webplayer
//! merges favorites + purchases + playlists client-side. We do the same: fan out
//! to the existing per-type loaders, normalize each into a `Feed` item, merge and
//! order by "date added" (approximated from each source's server order), then push
//! into `LibraryAllState`. Search / sort / source-switch filtering all run in Rust
//! (`derive`) — Slint renders the pre-computed `items-visible`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::adapter::SlintAdapter;
use crate::artwork::{ArtworkJob, ArtworkTarget, ImageCache};
use crate::favorites::{self, FavData, FavTab};
use crate::{AppWindow, LibraryAllState, LibraryFeedItem};

type Runtime = Arc<AppRuntime<SlintAdapter>>;

/// Plain, `Send` feed item produced on the worker thread.
#[derive(Clone, Default)]
pub struct Feed {
    pub kind: String,   // track | album | artist | playlist | label
    pub group: String,  // favorites | following | purchases
    pub source: String, // qobuz | local | plex
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub artist_id: String,
    pub album: String,
    pub album_id: String,
    pub image_url: String,
    pub quality_tier: String,
    pub quality_detail: String,
    pub is_favorite: bool,
    /// Genre name (albums + tracks carry one; artists/labels/playlists ""). Feeds
    /// the client-side genre filter — "" is excluded when a genre is selected.
    pub genre: String,
    /// Playlist ownership (only meaningful for kind == "playlist"): owned →
    /// favorite affordance; foreign Qobuz → follow + copy.
    pub playlist_owned: bool,
    pub playlist_following: bool,
    pub playlist_copied: bool,
    /// Recency proxy in [0.0, 1.0]; 0.0 = most-recently added. Each source list
    /// comes back date-desc, so `index / len` interleaves the sources by recency
    /// without needing exact per-item timestamps.
    pub added_rank: f32,
}

fn rank(i: usize, n: usize) -> f32 {
    if n <= 1 {
        0.0
    } else {
        i as f32 / n as f32
    }
}

/// Fan out to every source, normalize + merge into one date-ordered feed.
/// Qobuz-only for now (favorites + following + purchases); local/Plex arrive
/// with the Phase 2 local-favorites layer behind the `show-local` switch.
pub async fn load_library_all(runtime: &Runtime) -> Result<Vec<Feed>, String> {
    let mut feed: Vec<Feed> = Vec::new();

    // --- Favorites: tracks + albums (group "favorites") -------------------
    if let Ok(FavData::Tracks { items, .. }) =
        favorites::load_favorites(runtime, FavTab::Tracks).await
    {
        let n = items.len();
        for (i, t) in items.into_iter().enumerate() {
            feed.push(Feed {
                kind: "track".into(),
                group: "favorites".into(),
                source: "qobuz".into(),
                subtitle: t.artist.clone(),
                artist: t.artist,
                artist_id: t.artist_id,
                album: t.album,
                album_id: t.album_id,
                image_url: t.artwork_url,
                quality_tier: t.quality_tier,
                quality_detail: t.quality_detail,
                is_favorite: true,
                genre: t.genre,
                added_rank: rank(i, n),
                id: t.id,
                title: t.title,
                ..Default::default()
            });
        }
    }
    if let Ok(FavData::Albums { items, .. }) =
        favorites::load_favorites(runtime, FavTab::Albums).await
    {
        let n = items.len();
        for (i, a) in items.into_iter().enumerate() {
            feed.push(Feed {
                kind: "album".into(),
                group: "favorites".into(),
                source: "qobuz".into(),
                subtitle: a.artist.clone(),
                artist: a.artist,
                artist_id: a.artist_id,
                album: String::new(),
                album_id: String::new(),
                image_url: a.artwork_url,
                quality_tier: a.quality_tier,
                quality_detail: a.quality_detail,
                is_favorite: true,
                genre: a.genre,
                added_rank: rank(i, n),
                id: a.id,
                title: a.title,
                ..Default::default()
            });
        }
    }

    // --- Following: artists + labels (group "following") ------------------
    if let Ok(FavData::Artists { items, .. }) =
        favorites::load_favorites(runtime, FavTab::Artists).await
    {
        let n = items.len();
        for (i, ar) in items.into_iter().enumerate() {
            feed.push(Feed {
                kind: "artist".into(),
                group: "following".into(),
                source: "qobuz".into(),
                subtitle: String::new(),
                artist: String::new(),
                artist_id: ar.id.clone(),
                album: String::new(),
                album_id: String::new(),
                image_url: ar.image_url,
                quality_tier: String::new(),
                quality_detail: String::new(),
                is_favorite: true,
                added_rank: rank(i, n),
                id: ar.id,
                title: ar.name,
                ..Default::default()
            });
        }
    }
    if let Ok(FavData::Labels { items, .. }) =
        favorites::load_favorites(runtime, FavTab::Labels).await
    {
        let n = items.len();
        for (i, l) in items.into_iter().enumerate() {
            feed.push(Feed {
                kind: "label".into(),
                group: "following".into(),
                source: "qobuz".into(),
                subtitle: l.albums_line,
                artist: String::new(),
                artist_id: String::new(),
                album: String::new(),
                album_id: String::new(),
                image_url: l.image_url,
                quality_tier: String::new(),
                quality_detail: String::new(),
                is_favorite: true,
                added_rank: rank(i, n),
                id: l.id,
                title: l.name,
                ..Default::default()
            });
        }
    }

    // --- Playlists: owned/hearted = favorites, followed = following -------
    if let Ok(FavData::Playlists {
        favorites: fav_pl,
        following: fol_pl,
    }) = favorites::load_favorites(runtime, FavTab::Playlists).await
    {
        let n = fav_pl.len();
        for (i, p) in fav_pl.into_iter().enumerate() {
            let image_url = p.cover_urls.iter().next().cloned().unwrap_or_default();
            feed.push(Feed {
                kind: "playlist".into(),
                group: "favorites".into(),
                source: "qobuz".into(),
                subtitle: p.subtitle,
                artist: String::new(),
                artist_id: String::new(),
                album: String::new(),
                album_id: String::new(),
                image_url,
                quality_tier: String::new(),
                quality_detail: String::new(),
                is_favorite: true,
                playlist_owned: p.is_owned,
                playlist_following: p.is_following,
                playlist_copied: p.is_copied,
                added_rank: rank(i, n),
                id: p.id,
                title: p.title,
                ..Default::default()
            });
        }
        let n = fol_pl.len();
        for (i, p) in fol_pl.into_iter().enumerate() {
            let image_url = p.cover_urls.iter().next().cloned().unwrap_or_default();
            feed.push(Feed {
                kind: "playlist".into(),
                group: "following".into(),
                source: "qobuz".into(),
                subtitle: p.subtitle,
                artist: String::new(),
                artist_id: String::new(),
                album: String::new(),
                album_id: String::new(),
                image_url,
                quality_tier: String::new(),
                quality_detail: String::new(),
                is_favorite: false,
                playlist_owned: p.is_owned,
                playlist_following: p.is_following,
                playlist_copied: p.is_copied,
                added_rank: rank(i, n),
                id: p.id,
                title: p.title,
                ..Default::default()
            });
        }
    }

    // --- Purchases: albums + tracks (group "purchases") -------------------
    // `search_purchases("")` returns the full owned set (both types).
    if let Ok((albums, tracks)) = crate::purchases::search_purchases(runtime, "").await {
        let n = albums.len();
        for (i, a) in albums.into_iter().enumerate() {
            let image_url = a.image.best().cloned().unwrap_or_default();
            let tier = if a.hires { "hires" } else { "cd" };
            let genre = a.genre.as_ref().map(|g| g.name.clone()).unwrap_or_default();
            feed.push(Feed {
                kind: "album".into(),
                group: "purchases".into(),
                source: "qobuz".into(),
                subtitle: a.artist.name.clone(),
                artist: a.artist.name,
                artist_id: a.artist.id.to_string(),
                album: String::new(),
                album_id: String::new(),
                image_url,
                quality_tier: tier.into(),
                quality_detail: String::new(),
                is_favorite: false,
                genre,
                added_rank: rank(i, n),
                id: a.id,
                title: a.title,
                ..Default::default()
            });
        }
        let n = tracks.len();
        for (i, t) in tracks.into_iter().enumerate() {
            let (artist, image_url, album, album_id) = {
                let artist = t.performer.name.clone();
                let (img, alb, aid) = t
                    .album
                    .as_ref()
                    .map(|a| {
                        (
                            a.image.best().cloned().unwrap_or_default(),
                            a.title.clone(),
                            a.id.clone(),
                        )
                    })
                    .unwrap_or_default();
                (artist, img, alb, aid)
            };
            let tier = if t.hires { "hires" } else { "cd" };
            feed.push(Feed {
                kind: "track".into(),
                group: "purchases".into(),
                source: "qobuz".into(),
                subtitle: artist.clone(),
                artist,
                artist_id: t.performer.id.to_string(),
                album,
                album_id,
                image_url,
                quality_tier: tier.into(),
                quality_detail: String::new(),
                is_favorite: false,
                added_rank: rank(i, n),
                id: t.id.to_string(),
                title: t.title,
                ..Default::default()
            });
        }
    }

    // --- Local + Plex (source "local"/"plex"; gated by show-local in
    // derive). group "local" — bypasses the Qobuz source switches. The
    // Settings scope picks the CONTENT: "favorites" (hearted items only,
    // webplayer parity) or "all" (the entire local library). ---
    if crate::favorites_prefs::local_scope() == "all" {
        match tokio::task::spawn_blocking(all_local_blocking).await {
            Ok(items) => feed.extend(items),
            Err(e) => log::error!("[qbz-slint] all-local feed load failed: {e}"),
        }
    } else {
        let locals = crate::local_favorites::list();
        let n = locals.len();
        for (i, lf) in locals.into_iter().enumerate() {
            feed.push(Feed {
                kind: lf.kind,
                group: "local".into(),
                source: lf.source,
                subtitle: lf.subtitle,
                artist: lf.artist.clone(),
                artist_id: String::new(),
                album: String::new(),
                album_id: String::new(),
                image_url: lf.artwork_url,
                quality_tier: String::new(),
                quality_detail: String::new(),
                is_favorite: true,
                added_rank: rank(i, n),
                id: lf.id,
                title: lf.title,
                ..Default::default()
            });
        }
    }

    // Merge by recency proxy (stable so equal ranks keep source order).
    feed.sort_by(|a, b| {
        a.added_rank
            .partial_cmp(&b.added_rank)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(feed)
}

/// The "all" local scope (Settings > Local Library): every local-library
/// album + artist + track, mirroring the Local Library tabs' own queries so
/// both surfaces agree (same Plex union, same network-folder gate, same
/// album-identity mode, offline-cache copies excluded — they must never
/// duplicate Qobuz rows in this feed). Runs on a blocking thread — rusqlite
/// is sync. Off-thread companion of the local arm of `load_library_all`.
fn all_local_blocking() -> Vec<Feed> {
    let mut out: Vec<Feed> = Vec::new();
    let exclude_network = crate::local_library::exclude_network_folders_now();
    let group_mode = qbz_library::album_grouping::AlbumGroupMode::from_pref(
        &crate::locallibrary_prefs::albums_id_mode(),
    );
    let plex_path = crate::local_library::plex_cache_db_path();
    let plex_enabled = crate::plex_settings::get().enabled;

    // Plex tracks ride TWO sections (artist names + the track list) — fetch
    // the bounded set once (mirrors the Tracks tab's page-1 merge).
    let plex_tracks: Vec<qbz_library::LocalTrack> = if plex_enabled {
        qbz_plex::plex_cache_search_tracks(String::new(), None)
            .map(|rows| {
                rows.into_iter()
                    .map(crate::local_library::map_plex_cached_to_local_track)
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // --- Albums — the Albums tab's own full-load page (Plex union folded in
    // by the query when enabled). ---
    let albums = crate::library_db::with_db(|db| {
        db.get_albums_metadata_page(
            0,
            crate::local_library::ALBUMS_FULL_LOAD_LIMIT,
            None,
            "artist",
            "asc",
            true,
            exclude_network,
            plex_path.as_deref(),
            group_mode,
        )
        .ok()
        .map(|p| p.albums)
    })
    .flatten()
    .unwrap_or_default();
    let n = albums.len();
    for (i, a) in albums.into_iter().enumerate() {
        let source = if a.id.starts_with("plex:") {
            "plex"
        } else {
            "local"
        };
        let (tier, detail, _) =
            crate::quality::badge(&a.format.to_string(), a.bit_depth, Some(a.sample_rate));
        out.push(Feed {
            kind: "album".into(),
            group: "local".into(),
            source: source.into(),
            subtitle: a.artist.clone(),
            artist: a.artist,
            artist_id: String::new(),
            album: String::new(),
            album_id: String::new(),
            image_url: a.artwork_path.unwrap_or_default(),
            quality_tier: tier.into(),
            quality_detail: detail.into(),
            is_favorite: crate::local_favorites::is_favorite("album", &a.id),
            genre: String::new(),
            added_rank: rank(i, n),
            id: a.id,
            title: a.title,
            ..Default::default()
        });
    }

    // --- Artists — local DB names + names that only exist on Plex tracks
    // (a local + Plex artist of the same name counts once). ---
    let local_artist_names: Vec<String> = crate::library_db::with_db(|db| db.get_artists().ok())
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|a| a.name)
        .filter(|n| !n.trim().is_empty())
        .collect();
    let local_keys: std::collections::HashSet<String> = local_artist_names
        .iter()
        .map(|n| n.trim().to_lowercase())
        .collect();
    let mut artist_names = local_artist_names;
    {
        let mut seen = local_keys.clone();
        for t in &plex_tracks {
            let name = t.artist.trim();
            if !name.is_empty() && seen.insert(name.to_lowercase()) {
                artist_names.push(name.to_string());
            }
        }
    }
    artist_names.sort_by_key(|n| n.to_lowercase());
    let n = artist_names.len();
    for (i, name) in artist_names.into_iter().enumerate() {
        let source = if local_keys.contains(&name.trim().to_lowercase()) {
            "local"
        } else {
            "plex"
        };
        out.push(Feed {
            kind: "artist".into(),
            group: "local".into(),
            source: source.into(),
            subtitle: String::new(),
            artist: String::new(),
            artist_id: String::new(),
            album: String::new(),
            album_id: String::new(),
            image_url: String::new(),
            quality_tier: String::new(),
            quality_detail: String::new(),
            is_favorite: crate::local_favorites::is_favorite("artist", &name),
            genre: String::new(),
            added_rank: rank(i, n),
            id: name.clone(),
            title: name,
            ..Default::default()
        });
    }

    // --- Tracks — local pages (offline-cache copies excluded in SQL) + the
    // Plex set fetched above. ---
    let mut tracks: Vec<qbz_library::LocalTrack> = Vec::new();
    const PAGE: u64 = 200;
    let mut offset = 0u64;
    loop {
        let rows = crate::library_db::with_db(|db| {
            db.search_with_filter_page("", offset, PAGE, false, exclude_network, "default")
                .ok()
        })
        .flatten();
        let Some(rows) = rows else { break };
        let full = rows.len() as u64 == PAGE;
        tracks.extend(rows);
        if !full || tracks.len() >= 200_000 {
            break;
        }
        offset += PAGE;
    }
    tracks.extend(plex_tracks);
    let n = tracks.len();
    for (i, t) in tracks.into_iter().enumerate() {
        let (id, source) = match t.source.as_deref() {
            Some("plex") => (format!("plex:{}", t.file_path), "plex"),
            Some("qobuz_download") => continue, // belt: the SQL already excludes
            _ => (t.file_path.clone(), "local"),
        };
        let (tier, detail, _) =
            crate::quality::badge(&t.format.to_string(), t.bit_depth, Some(t.sample_rate));
        out.push(Feed {
            kind: "track".into(),
            group: "local".into(),
            source: source.into(),
            subtitle: t.artist.clone(),
            artist: t.artist,
            artist_id: String::new(),
            album: t.album,
            album_id: t.album_group_key,
            image_url: t.artwork_path.unwrap_or_default(),
            quality_tier: tier.into(),
            quality_detail: detail.into(),
            is_favorite: crate::local_favorites::is_favorite("track", &id),
            genre: t.genre.unwrap_or_default(),
            added_rank: rank(i, n),
            id,
            title: t.title,
            ..Default::default()
        });
    }

    out
}

fn to_item(f: &Feed) -> LibraryFeedItem {
    LibraryFeedItem {
        kind: f.kind.clone().into(),
        group: f.group.clone().into(),
        source: f.source.clone().into(),
        id: f.id.clone().into(),
        title: f.title.clone().into(),
        subtitle: f.subtitle.clone().into(),
        artist: f.artist.clone().into(),
        artist_id: f.artist_id.clone().into(),
        album: f.album.clone().into(),
        album_id: f.album_id.clone().into(),
        image_url: f.image_url.clone().into(),
        image: slint::Image::default(),
        quality_tier: f.quality_tier.clone().into(),
        quality_detail: f.quality_detail.clone().into(),
        is_favorite: f.is_favorite,
        removing: false,
        sort_title: f.title.to_lowercase().into(),
        sort_artist: f.artist.to_lowercase().into(),
        genre: f.genre.to_lowercase().into(),
        playlist_owned: f.playlist_owned,
        playlist_following: f.playlist_following,
        playlist_copied: f.playlist_copied,
    }
}

/// Push the full merged feed into `LibraryAllState` and derive the first view.
pub fn apply_library_all(window: &AppWindow, feed: Vec<Feed>) {
    let items: Vec<LibraryFeedItem> = feed.iter().map(to_item).collect();
    let total = items.len() as i32;
    let st = window.global::<LibraryAllState>();
    st.set_items(ModelRc::new(VecModel::from(items)));
    st.set_total(total);
    st.set_loading(false);
    st.set_load_error("".into());
    derive(window);
}

/// PlaylistView-style sort toggle: re-selecting the active field flips its
/// direction; a new field resets to that field's natural default ("date"
/// newest-first, "title"/"artist" A→Z). Then re-derive.
pub fn set_sort(window: &AppWindow, field: &str) {
    let st = window.global::<LibraryAllState>();
    let cur_field = st.get_sort_by().to_string();
    let new_asc = if cur_field == field {
        !st.get_sort_asc()
    } else {
        // "date" starts descending (newest first); the others start ascending.
        field != "date"
    };
    st.set_sort_by(field.into());
    st.set_sort_asc(new_asc);
    derive(window);
}

/// Apply search + source-switch + genre + sort over the full model into
/// `items-visible`. Runs on the Slint event loop; Slint never sorts/filters.
pub fn derive(window: &AppWindow) {
    let st = window.global::<LibraryAllState>();
    let needle = st.get_search().to_lowercase();
    let show_purchases = st.get_show_purchases();
    let show_favorites = st.get_show_favorites();
    let show_following = st.get_show_following();
    let show_local = st.get_show_local();
    let sort_by = st.get_sort_by();
    let sort_asc = st.get_sort_asc();
    // Shared genre filter (its own "library-all" context). Empty = no filter;
    // otherwise an item shows only when its (lowercased) genre matches one of
    // the selected genre names — kinds with no genre (artist/label/playlist)
    // are excluded, so the feed narrows to the chosen genre's albums + tracks.
    let genre_names: Vec<String> = crate::genre_filter::selected_names("library-all")
        .into_iter()
        .map(|g| g.to_lowercase())
        .collect();

    let full = st.get_items();
    let mut out: Vec<LibraryFeedItem> = Vec::new();
    for i in 0..full.row_count() {
        let Some(item) = full.row_data(i) else {
            continue;
        };
        let src = item.source.as_str();
        let is_local = src == "local" || src == "plex";
        if is_local {
            // Local files + Plex are gated ONLY by the show-local switch; they
            // bypass the Qobuz purchases/favorites/following switches.
            if !show_local {
                continue;
            }
        } else {
            // Qobuz source switches: an item shows when its group's switch is on.
            // If ALL three are off, treat as "no filter" (show everything) to
            // avoid an empty grid from an accidental all-off state.
            let any_group = show_purchases || show_favorites || show_following;
            let group = item.group.as_str();
            let group_ok = !any_group
                || (group == "purchases" && show_purchases)
                || (group == "favorites" && show_favorites)
                || (group == "following" && show_following);
            if !group_ok {
                continue;
            }
        }
        if !needle.is_empty() {
            let hit = item.sort_title.as_str().contains(&needle)
                || item.sort_artist.as_str().contains(&needle);
            if !hit {
                continue;
            }
        }
        if !genre_names.is_empty() {
            let g = item.genre.as_str();
            if g.is_empty() || !genre_names.iter().any(|n| g.contains(n.as_str())) {
                continue;
            }
        }
        out.push(item);
    }

    // Canonical ascending order per field, then reverse for the other
    // direction. "date" has no key on the item (the model is stored
    // newest-first from load), so it uses the inherent order: asc(false) =
    // newest-first (default), asc(true) = oldest-first (reversed).
    match sort_by.as_str() {
        "title" => {
            out.sort_by(|a, b| a.sort_title.as_str().cmp(b.sort_title.as_str()));
            if !sort_asc {
                out.reverse();
            }
        }
        "artist" => {
            out.sort_by(|a, b| a.sort_artist.as_str().cmp(b.sort_artist.as_str()));
            if !sort_asc {
                out.reverse();
            }
        }
        // "date": model order is newest-first; reverse only for oldest-first.
        _ => {
            if sort_asc {
                out.reverse();
            }
        }
    }

    st.set_items_visible(ModelRc::new(VecModel::from(out)));

    // The grid/list only fire window-changed when the BAND changes, not when
    // the rows under it change — re-dispatch covers for the current band over
    // the fresh visible set (no-op where covers already ride the full model).
    dispatch_library_all_window(window);
}

// ---- Windowed artwork (mirrors favorites' albums grid, Phase 1 pattern) ---

/// Generation guard, bumped on every Library-All (re)load. A stale in-flight
/// cover fetch (an older load's job) is discarded on apply so it can't land
/// on the replacement set.
static LIB_ALL_GEN: AtomicU64 = AtomicU64::new(0);

/// True if `gen` is still the current Library-All generation. The artwork
/// pipeline calls this before applying a decoded cover so an in-flight job
/// from a superseded load doesn't paint the new model.
pub fn library_all_gen_current(gen: u64) -> bool {
    LIB_ALL_GEN.load(Ordering::SeqCst) == gen
}

/// Last row band reported by the windowed grid/list (item indices into
/// `items-visible`, prefetch margin already included by the view).
static LIB_ALL_WINDOW: std::sync::Mutex<(usize, usize)> = std::sync::Mutex::new((0, 59));

/// Cover keys currently in the artwork pipeline (dedupe during fast scroll).
/// Freed on apply; cleared on reloads.
fn lib_all_inflight() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static S: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// Image-cache handle captured at load time so the window dispatcher can
/// spawn artwork jobs outside the load path. Plex params are re-read at
/// dispatch time (`plex_settings::get`), so only the cache rides along here.
fn lib_all_dispatch_ctx() -> &'static std::sync::Mutex<Option<ImageCache>> {
    static S: std::sync::OnceLock<std::sync::Mutex<Option<ImageCache>>> =
        std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(None))
}

/// A windowed artwork job finished (applied or dropped) — free its slot so
/// the dispatcher can request the key again after an eviction.
pub fn artwork_job_done(key: &str) {
    lib_all_inflight().lock().unwrap().remove(key);
}

/// Reset the windowed artwork pipeline for a fresh Library-All load: bump the
/// generation (orphans every in-flight job — dropped on apply), free their
/// dedupe slots and stash the image-cache handle the dispatchers spawn jobs
/// with. Runs on the UI thread BEFORE `apply_library_all`.
pub fn begin_library_all_artwork(image_cache: ImageCache) {
    LIB_ALL_GEN.fetch_add(1, Ordering::SeqCst);
    lib_all_inflight().lock().unwrap().clear();
    *lib_all_dispatch_ctx().lock().unwrap() = Some(image_cache);
}

/// Dispatch throttle for the view's band reports (leading + trailing edge,
/// UI thread) — same rationale as the favorites albums grid: coalesce fling
/// crossings to one dispatch per interval.
const LIB_ALL_DISPATCH_THROTTLE_MS: u64 = 180;
thread_local! {
    static LIB_ALL_BAND: crate::viewport::BandDispatcher =
        crate::viewport::BandDispatcher::new(LIB_ALL_DISPATCH_THROTTLE_MS);
}

/// The windowed grid/list reported a new visible row band. The band is stored
/// immediately (model rebuilds re-read it); the artwork dispatch is throttled
/// through the BandDispatcher.
pub fn window_changed(window: &AppWindow, first: i32, last: i32) {
    let first = first.max(0) as usize;
    let last = last.max(first as i32) as usize;
    *LIB_ALL_WINDOW.lock().unwrap() = (first, last);
    let gen = LIB_ALL_GEN.load(Ordering::SeqCst);
    let weak = window.as_weak();
    LIB_ALL_BAND.with(|d| {
        d.report(Box::new(move || {
            if !library_all_gen_current(gen) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                dispatch_library_all_window(&w);
            }
        }));
    });
}

/// Dispatch covers for the current window (over `items-visible`) and evict
/// decoded covers far outside it back to the placeholder, so cover RAM scales
/// with the viewport instead of the library. Delivery is key-keyed
/// (`LibraryAllById`), so a derive re-sort between dispatch and apply cannot
/// land a cover on the wrong card; a reload bumps `LIB_ALL_GEN` and the apply
/// arm drops the stale image.
pub fn dispatch_library_all_window(window: &AppWindow) {
    let (first, last) = *LIB_ALL_WINDOW.lock().unwrap();
    let Some(image_cache) = lib_all_dispatch_ctx().lock().unwrap().clone() else {
        return;
    };
    let gen = LIB_ALL_GEN.load(Ordering::SeqCst);
    let state = window.global::<LibraryAllState>();
    let visible = state.get_items_visible();
    let len = visible.row_count();
    if len == 0 {
        return;
    }
    let last = last.min(len - 1);
    if first > last {
        return;
    }
    // Retention = the window plus one window-span on each side. Beyond it,
    // covers return to the placeholder; re-entry is cheap (byte-budgeted
    // decoded cache, else a bounded re-decode through the disk cache).
    let span = last - first + 1;
    let keep_lo = first.saturating_sub(span);
    let keep_hi = (last + span).min(len - 1);
    let mut jobs = Vec::new();
    {
        let mut inflight = lib_all_inflight().lock().unwrap();
        for vi in first..=last {
            let Some(item) = visible.row_data(vi) else { continue };
            if item.image.size().width > 0 || item.image_url.is_empty() {
                continue;
            }
            let key = feed_key(item.kind.as_str(), item.id.as_str());
            if inflight.insert(key.clone()) {
                jobs.push(ArtworkJob {
                    target: ArtworkTarget::LibraryAllById { key, gen },
                    url: item.image_url.to_string(),
                });
            }
        }
    }
    for vi in (0..keep_lo).chain(keep_hi + 1..len) {
        let Some(item) = visible.row_data(vi) else { continue };
        if item.image.size().width > 0 {
            set_library_all_artwork(
                window,
                &feed_key(item.kind.as_str(), item.id.as_str()),
                slint::Image::default(),
            );
        }
    }
    if !jobs.is_empty() {
        // Mixed payload (Qobuz http / local fs / Plex /library/) — route each
        // cover by scheme so local/Plex covers decode.
        let plex = crate::plex_settings::get();
        crate::artwork::spawn_search_loads(
            jobs,
            plex.base_url,
            plex.token,
            window.as_weak(),
            image_cache,
        );
    }
}

/// Stable artwork key for a feed row. Qobuz numeric ids overlap across entity
/// types (a track id can equal an artist id), so the kind prefixes the key.
fn feed_key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

/// Set a freshly-decoded (or evicted) cover BY KEY on the full `items` model
/// AND the rendered `items-visible`. Writing the full model too is what keeps
/// covers across derives: `derive` clones rows out of `items`, so a cover
/// that only ever landed on the visible copy was blanked by every search
/// keystroke (and re-fought by the whole dispatch pipeline each time).
pub fn set_library_all_artwork(window: &AppWindow, key: &str, image: slint::Image) {
    let st = window.global::<LibraryAllState>();
    for model in [st.get_items(), st.get_items_visible()] {
        for i in 0..model.row_count() {
            let Some(mut item) = model.row_data(i) else { continue };
            if feed_key(item.kind.as_str(), item.id.as_str()) == key {
                item.image = image.clone();
                model.set_row_data(i, item);
                break;
            }
        }
    }
}

/// Flip the favorite/follow flag BY KEY on the full `items` model AND the
/// rendered `items-visible` — the Library-All leg of the app-wide optimistic
/// favorite flips (`set_row_favorite` / `set_album_row_favorite` /
/// `search::mark_artist_followed` don't know this surface). Called for both
/// the optimistic flip and the failure rollback.
pub fn set_feed_favorite(window: &AppWindow, kind: &str, id: &str, value: bool) {
    let key = feed_key(kind, id);
    let st = window.global::<LibraryAllState>();
    for model in [st.get_items(), st.get_items_visible()] {
        for i in 0..model.row_count() {
            let Some(mut item) = model.row_data(i) else { continue };
            if feed_key(item.kind.as_str(), item.id.as_str()) == key {
                if item.is_favorite != value {
                    item.is_favorite = value;
                    model.set_row_data(i, item);
                }
                break;
            }
        }
    }
}

/// True when a feed id belongs to the LOCAL/Plex world, not Qobuz. Local
/// tracks are file paths (`/music/x.flac` or `plex:<path>` — Qobuz track ids
/// are numeric), local albums are group keys (`plex:…` or containing `|`/`/`),
/// local artists are plain names (Qobuz artist ids are numeric).
fn is_local_feed_id(kind: &str, id: &str) -> bool {
    match kind {
        "track" | "artist" => id.parse::<u64>().is_err(),
        "album" => id.starts_with("plex:") || id.contains('|') || id.contains('/'),
        _ => false,
    }
}

/// Toggle a LOCAL/Plex feed row's heart against the local-favorites store.
/// The All-feed media-action arms receive only `(kind, id, action)` and would
/// otherwise fire the Qobuz favorite API at a file path / group key / artist
/// name — always an error toast (owner report 2026-07-24). Returns
/// `Some(new_state)` when the id was local and the toggle was handled here
/// (the caller must NOT continue into its Qobuz path); `None` for Qobuz ids.
///
/// Every local row in the feed is favorited by construction (the store feeds
/// the feed), so the common click is an UN-favorite; the re-favorite path
/// rebuilds the store snapshot from the feed row itself (title / subtitle /
/// artwork / source all ride the model). Works offline — the store is local.
pub fn toggle_local_feed_favorite(window: &AppWindow, kind: &str, id: &str) -> Option<bool> {
    if !is_local_feed_id(kind, id) {
        return None;
    }
    let new_state = if crate::local_favorites::is_favorite(kind, id) {
        if let Err(e) = crate::local_favorites::unfavorite(kind, id) {
            log::error!("[qbz-slint] local unfavorite ({kind}:{id}) failed: {e}");
            return Some(true); // state unchanged — keep the heart filled
        }
        false
    } else {
        // Re-favorite: rebuild the snapshot from the feed row.
        let st = window.global::<LibraryAllState>();
        let items = st.get_items();
        let mut snap = None;
        for i in 0..items.row_count() {
            let Some(item) = items.row_data(i) else { continue };
            if feed_key(item.kind.as_str(), item.id.as_str()) == feed_key(kind, id) {
                snap = Some(crate::local_favorites::LocalFavItem {
                    kind: kind.to_string(),
                    id: id.to_string(),
                    title: item.title.to_string(),
                    subtitle: item.subtitle.to_string(),
                    artwork_url: item.image_url.to_string(),
                    artist: item.artist.to_string(),
                    source: item.source.to_string(),
                    favorited_at: 0, // the service stamps `now` itself
                });
                break;
            }
        }
        match snap {
            Some(s) if s.source == "local" || s.source == "plex" => {
                if let Err(e) = crate::local_favorites::favorite(&s) {
                    log::error!("[qbz-slint] local favorite ({kind}:{id}) failed: {e}");
                    return Some(false); // state unchanged — keep the heart hollow
                }
                true
            }
            // Not in the feed (or a malformed source) — nothing to snapshot;
            // treat as handled-but-unchanged rather than falling into Qobuz.
            _ => return Some(false),
        }
    };
    set_feed_favorite(window, kind, id, new_state);
    // Same toast wording as the Qobuz album/track favorite arms.
    crate::toast::success(
        window,
        if new_state {
            "Added to favorites"
        } else {
            "Removed from favorites"
        },
    );
    Some(new_state)
}
