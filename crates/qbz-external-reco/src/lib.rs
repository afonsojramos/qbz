//! Frontend-agnostic external-recommendations engine for Discover (ADR-006).
//!
//! Blends up to three sources — the in-house artist-vector engine (`qbz-reco`,
//! deferred/placeholder), Last.fm, and ListenBrainz — into the Discover
//! "External Recommendations" carousels, validating every candidate against the
//! Qobuz catalog before it is shown (the app can only play Qobuz content).
//!
//! Owner design (2026-06-26/27):
//!   - C1 "Similar artists you haven't heard" + C2 "Similar tracks you haven't
//!     heard" = pure discovery, **blended** across all connected sources.
//!   - C3 "Listened but not recently" = re-discovery (Last.fm long-term tops).
//!   - C4 "From artists you know but not scrobbled" = deep cuts (Qobuz catalog
//!     of known artists, minus the external scrobble set).
//!   - Each carousel caps at 20 with daily rotation over a larger pool.
//!   - NEVER hide: with no connected external source it falls back to Qobuz
//!     editorial top albums + artists (already Qobuz-native, zero validation).
//!
//! The crate depends only on frontend-agnostic crates (`qbz-integrations`,
//! `qbz-models`). The Qobuz catalog is reached via the [`RecoCatalog`] trait,
//! which the frontend implements over its own core.

pub mod cache;
pub mod matching;
pub mod types;

mod carousels;
mod validate;

use std::sync::Mutex;

use qbz_integrations::{LastFmClient, ListenBrainzClient, MusicBrainzClient};
use qbz_models::{Album, Artist, Track};

pub use cache::RecoCache;
pub use types::{
    AlbumReco, ArtistReco, ExternalCarousels, LocalHistory, RecoSource, TrackReco,
};

/// The Qobuz catalog operations the engine needs. Implemented by the frontend
/// over its own `QbzCore`. Every method swallows errors to an empty result —
/// "no data" must never be an error to the engine.
#[async_trait::async_trait]
pub trait RecoCatalog: Send + Sync {
    /// Free-text Qobuz track search (also used for ISRC-as-query).
    async fn search_tracks(&self, query: &str, limit: usize) -> Vec<Track>;
    /// Qobuz artist search (for validating a recommended artist name).
    async fn search_artists(&self, query: &str, limit: usize) -> Vec<Artist>;
    /// An artist's top tracks (the deep-cut candidate source for C4).
    async fn artist_top_tracks(&self, artist_id: u64, limit: usize) -> Vec<Track>;
    /// Editorial featured albums by kind ("most-streamed" | "new-releases" | …).
    async fn featured_albums(&self, kind: &str, limit: usize) -> Vec<Album>;
    /// Full artist by id (cold-start top-artist portraits).
    async fn get_artist(&self, artist_id: u64) -> Option<Artist>;
}

/// A connected Last.fm account: the public username + a ready client.
pub struct LastFmHandle<'a> {
    pub username: String,
    pub client: &'a LastFmClient,
}

/// A connected ListenBrainz account: the public username + a ready client.
pub struct ListenBrainzHandle<'a> {
    pub username: String,
    pub client: &'a ListenBrainzClient,
}

/// All the inputs `build_external_carousels` needs. Borrowed so the frontend
/// owns the clients/cache and the engine stays allocation-light.
pub struct RecoInputs<'a> {
    pub lastfm: Option<LastFmHandle<'a>>,
    pub listenbrainz: Option<ListenBrainzHandle<'a>>,
    pub musicbrainz: &'a MusicBrainzClient,
    pub catalog: &'a dyn RecoCatalog,
    /// Optional resolution cache (per-user SQLite). `None` disables caching.
    pub cache: Option<&'a Mutex<RecoCache>>,
    /// Local listening signal from the `reco_events` store.
    pub local: LocalHistory,
    /// Daily rotation offset (e.g. days since the Unix epoch).
    pub rotation_seed: u64,
}

/// Build the Discover "External Recommendations" carousels. Cheap to call
/// repeatedly thanks to the resolution cache; the frontend should still throttle
/// rebuilds (e.g. once per session) to bound external-API load.
pub async fn build_external_carousels(inputs: RecoInputs<'_>) -> ExternalCarousels {
    carousels::build(inputs).await
}
