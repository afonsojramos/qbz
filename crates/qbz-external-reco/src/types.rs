//! Data types for the external-recommendations engine.

use serde::{Deserialize, Serialize};

/// Which source produced a recommendation row item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoSource {
    /// In-house artist-vector engine (`qbz-reco`) — deferred, placeholder.
    Internal,
    LastFm,
    ListenBrainz,
    /// Qobuz editorial (cold-start fallback).
    Editorial,
}

/// A raw artist candidate before Qobuz validation.
#[derive(Debug, Clone)]
pub struct ArtistCandidate {
    pub name: String,
    pub source: RecoSource,
    /// Source-normalized similarity score in 0..1.
    pub score: f32,
}

/// A raw track candidate before Qobuz validation.
#[derive(Debug, Clone)]
pub struct TrackCandidate {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    pub isrc: Option<String>,
    pub recording_mbid: Option<String>,
    pub source: RecoSource,
    /// Source-normalized similarity score in 0..1.
    pub score: f32,
}

/// A resolved artist row (validated to a Qobuz artist).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistReco {
    pub qobuz_artist_id: u64,
    pub name: String,
    pub image_url: String,
    pub source: RecoSource,
}

/// A resolved track row (validated to / sourced from a Qobuz track).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackReco {
    pub qobuz_track_id: u64,
    pub title: String,
    pub artist: String,
    pub artwork_url: String,
    pub source: RecoSource,
}

/// One album row for the cold-start editorial fallback.
#[derive(Debug, Clone)]
pub struct AlbumReco {
    pub qobuz_album_id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: String,
    pub year: String,
    pub quality_tier: String,
    pub quality_label: String,
    pub artwork_url: String,
}

/// The full external-recommendations result for the Discover section.
///
/// At runtime exactly one regime is populated: `editorial_fallback == true`
/// fills `top_albums`/`top_artists` (cold start, no connected external source),
/// otherwise the four personalized carousels are filled. Empty vecs self-hide
/// their row in the view, so partial population is always safe.
#[derive(Debug, Clone, Default)]
pub struct ExternalCarousels {
    /// No external integration connected -> editorial fallback regime.
    pub editorial_fallback: bool,
    /// C1 — "Similar artists you haven't heard".
    pub similar_artists: Vec<ArtistReco>,
    /// C2 — "Similar tracks you haven't heard".
    pub similar_tracks: Vec<TrackReco>,
    /// C3 — "Listened but not recently".
    pub rediscover_tracks: Vec<TrackReco>,
    /// C4 — "From artists you know but not scrobbled" (deep cuts).
    pub deep_cut_tracks: Vec<TrackReco>,
    /// Cold-start fallback: Qobuz editorial top albums.
    pub top_albums: Vec<AlbumReco>,
    /// Cold-start fallback: Qobuz editorial top artists.
    pub top_artists: Vec<ArtistReco>,
}

/// Local listening signal derived from the per-user `reco_events` store (all ids
/// are Qobuz ids; the play log is Qobuz-source-gated).
#[derive(Debug, Clone, Default)]
pub struct LocalHistory {
    /// Artists the user already knows (played > threshold or favorited).
    pub known_artist_ids: std::collections::HashSet<u64>,
    /// Tracks played in-app (the local "already heard" set).
    pub played_track_ids: std::collections::HashSet<u64>,
}
