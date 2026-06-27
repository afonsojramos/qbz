//! Resolve a recommendation candidate to a real, streamable Qobuz entity.
//!
//! Precedence for tracks (strongest key first):
//!   1. ISRC known            -> Qobuz text-search the ISRC, accept only on an
//!                               exact `Track.isrc` match (Qobuz indexes ISRC).
//!   2. recording_mbid known  -> MusicBrainz `inc=isrcs` bridge -> step 1.
//!   3. names only            -> fuzzy text search scored by [`crate::matching`],
//!                               gated at `MIN_SCORE`.
//! Artists resolve by Qobuz artist-search + normalized-name match.
//!
//! Every outcome (positive AND negative) is cached so rotation re-renders and
//! repeat builds never re-hit the Qobuz/MusicBrainz APIs for a known candidate.

use std::sync::Mutex;

use qbz_integrations::MusicBrainzClient;
use qbz_models::Track;

use crate::cache::{CacheLookup, RecoCache};
use crate::matching::{normalize, select_best_match, MatchInput, MIN_SCORE};
use crate::types::{ArtistCandidate, ArtistReco, TrackCandidate, TrackReco};
use crate::RecoCatalog;

type Cache<'a> = Option<&'a Mutex<RecoCache>>;

fn track_cache_key(c: &TrackCandidate) -> String {
    if let Some(isrc) = c.isrc.as_deref().filter(|s| !s.is_empty()) {
        format!("t:isrc:{}", isrc.to_uppercase())
    } else if let Some(mbid) = c.recording_mbid.as_deref().filter(|s| !s.is_empty()) {
        format!("t:mbid:{}", mbid)
    } else {
        format!("t:name:{}|{}", normalize(&c.artist), normalize(&c.title))
    }
}

fn build_track_reco(track: &Track, source: crate::types::RecoSource) -> TrackReco {
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

/// Find a Qobuz track whose ISRC matches `isrc` exactly (Qobuz has no ISRC
/// endpoint; it indexes ISRC in free-text search, so we verify the field).
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
    // 1. ISRC direct.
    if let Some(isrc) = cand.isrc.as_deref().filter(|s| !s.is_empty()) {
        if let Some(track) = find_by_isrc(catalog, isrc).await {
            return Some(build_track_reco(&track, cand.source));
        }
    }
    // 2. recording_mbid -> MusicBrainz ISRCs -> ISRC.
    if let Some(mbid) = cand.recording_mbid.as_deref().filter(|s| !s.is_empty()) {
        let isrcs = mb.get_recording_isrcs(mbid).await.unwrap_or_default();
        for isrc in isrcs {
            if let Some(track) = find_by_isrc(catalog, &isrc).await {
                return Some(build_track_reco(&track, cand.source));
            }
        }
    }
    // 3. Fuzzy text search.
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

/// Validate a track candidate, using the cache for both hits and misses.
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
                Some(r) => {
                    let json = serde_json::to_string(r).unwrap_or_default();
                    guard.put(&key, "track", Some(&json));
                }
                None => guard.put(&key, "track", None),
            }
        }
    }
    reco
}

/// Validate an artist candidate (name -> Qobuz artist), cached.
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
            source: cand.source,
        });

    if let Some(c) = cache {
        if let Ok(guard) = c.lock() {
            match &reco {
                Some(r) => {
                    let json = serde_json::to_string(r).unwrap_or_default();
                    guard.put(&key, "artist", Some(&json));
                }
                None => guard.put(&key, "artist", None),
            }
        }
    }
    reco
}
