//! In-memory per-artist library index for the ArtistPage catalog/library toggle.
//!
//! Qobuz has NO server-side "library items by artist" endpoint, so we build a
//! process-wide index once per session (seeded at login, off the UI thread, like
//! `fav_cache`) from the user's favorite tracks + albums, grouped by artist id.
//! `navigate_artist` then does an O(1) lookup to decide whether the toggle shows
//! and to fill the "En la biblioteca" subset. Favorites-only for now (matches the
//! webplayer's "Pistas añadidas" / "Álbumes añadidos"); purchases + local fold in
//! as a follow-up.
//!
//! NOTE: the index is a session snapshot — favoriting/unfavoriting during the
//! session is not reflected until the next login (acceptable v1).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use slint::{ModelRc, VecModel};

use crate::album_map::{self, AlbumCard};
use crate::favorites::{self, FavData, FavTab, TrackCard};
use crate::{AlbumCardItem, TrackItem};

/// One artist's library items (favorites, this session).
#[derive(Default, Clone)]
pub struct ArtistLibrary {
    pub tracks: Vec<TrackCard>,
    pub albums: Vec<AlbumCard>,
}

impl ArtistLibrary {
    pub fn count(&self) -> usize {
        self.tracks.len() + self.albums.len()
    }
}

static INDEX: RwLock<Option<HashMap<String, ArtistLibrary>>> = RwLock::new(None);

/// Seed the index from favorite tracks + albums, grouped by artist id. Idempotent
/// (replaces any prior snapshot). Best-effort: a failed fetch leaves the artist
/// out (its toggle simply won't appear).
pub async fn seed<A>(runtime: &Arc<AppRuntime<A>>)
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let mut map: HashMap<String, ArtistLibrary> = HashMap::new();

    let mut fav_tracks: Vec<TrackCard> = Vec::new();
    let mut fav_albums: Vec<AlbumCard> = Vec::new();

    if let Ok(FavData::Tracks { items, .. }) =
        favorites::load_favorites(runtime, FavTab::Tracks).await
    {
        fav_tracks = items;
    }
    if let Ok(FavData::Albums { items, .. }) =
        favorites::load_favorites(runtime, FavTab::Albums).await
    {
        fav_albums = items;
    }

    // Feed the per-label index from the SAME two fetches (no doubled
    // pagination) — powers the LabelPage catalog/library toggle.
    crate::library_by_label::seed_from_parts(&fav_tracks, &fav_albums);

    for t in fav_tracks {
        if !t.artist_id.is_empty() {
            map.entry(t.artist_id.clone()).or_default().tracks.push(t);
        }
    }
    for a in fav_albums {
        if !a.artist_id.is_empty() {
            map.entry(a.artist_id.clone()).or_default().albums.push(a);
        }
    }

    if let Ok(mut guard) = INDEX.write() {
        *guard = Some(map);
    }
}

/// O(1) lookup of one artist's library items. `None` (or empty) when the artist
/// has nothing in the user's library / the index isn't seeded yet.
pub fn get(artist_id: &str) -> Option<ArtistLibrary> {
    INDEX
        .read()
        .ok()
        .and_then(|g| g.as_ref().and_then(|m| m.get(artist_id).cloned()))
}

/// Convert stored album cards to the Slint model (reuses `album_map::to_item`,
/// which stamps favorite/pin state).
pub fn album_items(albums: &[AlbumCard]) -> ModelRc<AlbumCardItem> {
    let rows: Vec<AlbumCardItem> = albums.iter().cloned().map(album_map::to_item).collect();
    ModelRc::new(VecModel::from(rows))
}

/// Convert stored track cards to the Slint model (mirrors the favorites
/// apply_favorites mapping; everything here is, by definition, a favorite).
pub fn track_items(tracks: &[TrackCard]) -> ModelRc<TrackItem> {
    let rows: Vec<TrackItem> = tracks
        .iter()
        .cloned()
        .map(|t| TrackItem {
            is_blacklisted: false,
            id: t.id.clone().into(),
            number: "".into(),
            title: t.title.into(),
            artist: t.artist.into(),
            album: t.album.into(),
            duration: t.duration.into(),
            quality_tier: t.quality_tier.into(),
            quality_detail: t.quality_detail.into(),
            explicit: t.explicit,
            selected: false,
            artwork_url: t.artwork_url.into(),
            artwork: slint::Image::default(),
            is_favorite: true,
            artist_id: t.artist_id.into(),
            album_id: t.album_id.into(),
            removing: false,
            cache_status: 0,
            cache_progress: 0.0,
            source: "qobuz".into(),
            unlocking: false,
            disc_header_number: 0,
            work_header: "".into(),
            work_composer_name: "".into(),
            work_composer_id: "".into(),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}
