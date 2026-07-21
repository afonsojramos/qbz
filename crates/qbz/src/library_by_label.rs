//! In-memory per-label library index for the LabelPage catalog/library toggle.
//!
//! Mirrors `library_by_artist.rs`: Qobuz has NO server-side "library items by
//! label" endpoint, so we keep a process-wide snapshot (seeded at login from
//! the same favorites fetch the artist index already performs — see
//! `seed_from_parts`) grouping favorite TRACKS (by their nested album's label
//! id) and favorite ALBUMS (by label id) per label. `navigate_label` then
//! does an O(1) lookup to decide whether the toggle shows and to fill the
//! "In library" tab. Favorites-only (matches the webplayer's "Pistas
//! añadidas" / "Álbumes añadidos"); purchases + local fold in as a follow-up,
//! same caveat as the artist index.
//!
//! NOTE: same session-snapshot caveat as the artist index — favoriting an
//! album/track mid-session is reflected at the next login, not live.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::album_map::AlbumCard;
use crate::favorites::TrackCard;

/// One label's library items (favorites, this session).
#[derive(Default, Clone)]
pub struct LabelLibrary {
    pub tracks: Vec<TrackCard>,
    pub albums: Vec<AlbumCard>,
}

impl LabelLibrary {
    pub fn count(&self) -> usize {
        self.tracks.len() + self.albums.len()
    }
}

static INDEX: RwLock<Option<HashMap<String, LabelLibrary>>> = RwLock::new(None);

/// Feed the index from already-fetched favorites lists. Called by
/// `library_by_artist::seed` so both indexes ride the SAME fetch (no doubled
/// pagination). Idempotent (replaces any prior snapshot).
pub fn seed_from_parts(tracks: &[TrackCard], albums: &[AlbumCard]) {
    let mut map: HashMap<String, LabelLibrary> = HashMap::new();
    for t in tracks {
        if !t.label_id.is_empty() {
            map.entry(t.label_id.clone()).or_default().tracks.push(t.clone());
        }
    }
    for a in albums {
        if !a.label_id.is_empty() {
            map.entry(a.label_id.clone()).or_default().albums.push(a.clone());
        }
    }
    if let Ok(mut guard) = INDEX.write() {
        *guard = Some(map);
    }
}

/// O(1) lookup of one label's library items. `None` (or empty) when the
/// label has nothing in the user's library / the index isn't seeded yet.
pub fn get(label_id: &str) -> Option<LabelLibrary> {
    INDEX
        .read()
        .ok()
        .and_then(|g| g.as_ref().and_then(|m| m.get(label_id).cloned()))
}
