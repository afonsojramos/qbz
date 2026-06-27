//! Resolve a recommendation candidate to a real Qobuz entity (track/artist/album).
//!
//! Tracks: ISRC -> MusicBrainz `inc=isrcs` -> Qobuz, else fuzzy text.
//! Artists: Qobuz artist-search + normalized-name match.
//! Albums: UPC match if known, else fuzzy text (title*0.6 + artist*0.4).
//! Every outcome (positive AND negative) is cached.

use std::sync::Mutex;

use qbz_integrations::MusicBrainzClient;
use qbz_models::{Album, Track};

use crate::cache::{CacheLookup, RecoCache};
use crate::matching::{normalize, select_best_match, similarity, MatchInput, MIN_SCORE};
use crate::types::{
    AlbumCandidate, AlbumReco, ArtistCandidate, ArtistReco, RecoSource, TrackCandidate, TrackReco,
};
use crate::RecoCatalog;

type Cache<'a> = Option<&'a Mutex<RecoCache>>;

const ALBUM_MIN_SCORE: f32 = 0.6;

// ── Tracks ─────────────────────────────────────────────────────────────────

fn track_cache_key(c: &TrackCandidate) -> String {
    if let Some(isrc) = c.isrc.as_deref().filter(|s| !s.is_empty()) {
        format!("t:isrc:{}", isrc.to_uppercase())
    } else if let Some(mbid) = c.recording_mbid.as_deref().filter(|s| !s.is_empty()) {
        format!("t:mbid:{}", mbid)
    } else {
        format!("t:name:{}|{}", normalize(&c.artist), normalize(&c.title))
    }
}

fn build_track_reco(track: &Track, source: RecoSource) -> TrackReco {
    TrackReco {
        qobuz_track_id: track.id,
        title: track.title.clone(),
        artist: track
            .performer
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_default(),
        artwork_url: track
            .album
            .as_ref()
            .and_then(|al| al.image.best().cloned())
            .unwrap_or_default(),
        source,
    }
}

async fn find_by_isrc(catalog: &dyn RecoCatalog, isrc: &str) -> Option<Track> {
    let results = catalog.search_tracks(isrc, 5).await;
    results.into_iter().find(|t| {
        t.streamable
            && t.isrc
                .as_deref()
                .map(|c| c.eq_ignore_ascii_case(isrc))
                .unwrap_or(false)
    })
}

async fn resolve_track_live(
    catalog: &dyn RecoCatalog,
    mb: &MusicBrainzClient,
    cand: &TrackCandidate,
) -> Option<TrackReco> {
    if let Some(isrc) = cand.isrc.as_deref().filter(|s| !s.is_empty()) {
        if let Some(track) = find_by_isrc(catalog, isrc).await {
            return Some(build_track_reco(&track, cand.source));
        }
    }
    if let Some(mbid) = cand.recording_mbid.as_deref().filter(|s| !s.is_empty()) {
        let isrcs = mb.get_recording_isrcs(mbid).await.unwrap_or_default();
        for isrc in isrcs {
            if let Some(track) = find_by_isrc(catalog, &isrc).await {
                return Some(build_track_reco(&track, cand.source));
            }
        }
    }
    let query = format!("{} {}", cand.artist, cand.title);
    let candidates = catalog.search_tracks(query.trim(), 20).await;
    let input = MatchInput {
        artist: &cand.artist,
        title: &cand.title,
        album: cand.album.as_deref(),
        duration_ms: cand.duration_ms,
        isrc: cand.isrc.as_deref(),
    };
    let (best, score) = select_best_match(&input, &candidates);
    match best {
        Some(track) if score >= MIN_SCORE => Some(build_track_reco(track, cand.source)),
        _ => None,
    }
}

pub async fn validate_track(
    catalog: &dyn RecoCatalog,
    mb: &MusicBrainzClient,
    cache: Cache<'_>,
    cand: &TrackCandidate,
) -> Option<TrackReco> {
    let key = track_cache_key(cand);
    if let Some(c) = cache {
        if let Ok(guard) = c.lock() {
            match guard.get(&key) {
                CacheLookup::Found(json) => {
                    if let Ok(mut reco) = serde_json::from_str::<TrackReco>(&json) {
                        reco.source = cand.source;
                        return Some(reco);
                    }
                }
                CacheLookup::Negative => return None,
                CacheLookup::Miss => {}
            }
        }
    }
    let reco = resolve_track_live(catalog, mb, cand).await;
    if let Some(c) = cache {
        if let Ok(guard) = c.lock() {
            match &reco {
                Some(r) => guard.put(&key, "track", Some(&serde_json::to_string(r).unwrap_or_default())),
                None => guard.put(&key, "track", None),
            }
        }
    }
    reco
}

// ── Artists ────────────────────────────────────────────────────────────────

pub async fn validate_artist(
    catalog: &dyn RecoCatalog,
    cache: Cache<'_>,
    cand: &ArtistCandidate,
) -> Option<ArtistReco> {
    let key = format!("a:{}", normalize(&cand.name));
    if let Some(c) = cache {
        if let Ok(guard) = c.lock() {
            match guard.get(&key) {
                CacheLookup::Found(json) => {
                    if let Ok(mut reco) = serde_json::from_str::<ArtistReco>(&json) {
                        reco.source = cand.source;
                        reco.subtitle = cand.subtitle.clone();
                        return Some(reco);
                    }
                }
                CacheLookup::Negative => return None,
                CacheLookup::Miss => {}
            }
        }
    }

    let target = normalize(&cand.name);
    let artists = catalog.search_artists(&cand.name, 8).await;
    let reco = artists
        .iter()
        .find(|a| normalize(&a.name) == target)
        .or_else(|| artists.first())
        .filter(|a| a.id != 0)
        .map(|a| ArtistReco {
            qobuz_artist_id: a.id,
            name: a.name.clone(),
            image_url: a.image.as_ref().and_then(|i| i.best().cloned()).unwrap_or_default(),
            subtitle: cand.subtitle.clone(),
            source: cand.source,
        });

    if let Some(c) = cache {
        if let Ok(guard) = c.lock() {
            match &reco {
                Some(r) => guard.put(&key, "artist", Some(&serde_json::to_string(r).unwrap_or_default())),
                None => guard.put(&key, "artist", None),
            }
        }
    }
    reco
}

// ── Albums ─────────────────────────────────────────────────────────────────

fn album_cache_key(c: &AlbumCandidate) -> String {
    if let Some(upc) = c.upc.as_deref().filter(|s| !s.is_empty()) {
        format!("alb:upc:{}", upc)
    } else {
        format!("alb:{}|{}", normalize(&c.artist), normalize(&c.title))
    }
}

pub fn build_album_reco(album: &Album, subtitle: String, source: RecoSource) -> AlbumReco {
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
    AlbumReco {
        qobuz_album_id: album.id.clone(),
        title: album.title.clone(),
        artist: album.artist.name.clone(),
        artist_id: album.artist.id.to_string(),
        year,
        quality_tier,
        quality_label,
        artwork_url: album.image.best().cloned().unwrap_or_default(),
        subtitle,
        source,
    }
}

async fn resolve_album_live(catalog: &dyn RecoCatalog, cand: &AlbumCandidate) -> Option<AlbumReco> {
    if let Some(upc) = cand.upc.as_deref().filter(|s| !s.is_empty()) {
        let albums = catalog.search_albums(upc, 5).await;
        if let Some(a) = albums
            .iter()
            .find(|a| a.upc.as_deref().map(|u| u.eq_ignore_ascii_case(upc)).unwrap_or(false))
        {
            return Some(build_album_reco(a, cand.subtitle.clone(), cand.source));
        }
    }
    let query = format!("{} {}", cand.artist, cand.title);
    let albums = catalog.search_albums(query.trim(), 10).await;
    let mut best: Option<&Album> = None;
    let mut best_score = 0.0f32;
    for a in &albums {
        let title_s = similarity(&cand.title, &a.title);
        let artist_s = similarity(&cand.artist, &a.artist.name);
        let score = title_s * 0.6 + artist_s * 0.4;
        if score > best_score {
            best_score = score;
            best = Some(a);
        }
    }
    match best {
        Some(a) if best_score >= ALBUM_MIN_SCORE => {
            Some(build_album_reco(a, cand.subtitle.clone(), cand.source))
        }
        _ => None,
    }
}

pub async fn validate_album(
    catalog: &dyn RecoCatalog,
    cache: Cache<'_>,
    cand: &AlbumCandidate,
) -> Option<AlbumReco> {
    let key = album_cache_key(cand);
    if let Some(c) = cache {
        if let Ok(guard) = c.lock() {
            match guard.get(&key) {
                CacheLookup::Found(json) => {
                    if let Ok(mut reco) = serde_json::from_str::<AlbumReco>(&json) {
                        reco.source = cand.source;
                        reco.subtitle = cand.subtitle.clone();
                        return Some(reco);
                    }
                }
                CacheLookup::Negative => return None,
                CacheLookup::Miss => {}
            }
        }
    }
    let reco = resolve_album_live(catalog, cand).await;
    if let Some(c) = cache {
        if let Ok(guard) = c.lock() {
            match &reco {
                Some(r) => guard.put(&key, "album", Some(&serde_json::to_string(r).unwrap_or_default())),
                None => guard.put(&key, "album", None),
            }
        }
    }
    reco
}
