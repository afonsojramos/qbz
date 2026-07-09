//! Public-playlist import for QBZ — provider scrapers, Qobuz matcher,
//! playlist creation (headless, frontend-agnostic).
//!
//! Extracted verbatim from `src-tauri/src/playlist_import/*` and re-typed
//! against the shared crates (`qbz-models`, `qbz-qobuz`), so it runs headless
//! with no Tauri state and no `#[tauri::command]` wrappers (ADR-005: no
//! legacy wrappers; ADR-006: frontend-agnostic core). Progress reaches the
//! frontend through the [`sink::ImportProgressSink`] trait instead of
//! `AppHandle::emit`.
//!
//! Known provider limitations (behavior-faithful copies of the Tauri code;
//! follow-up TODOs, not bugs introduced by the extraction):
//! - Spotify: embed scraping only (API access gone since 2026-03-06) — caps
//!   at ~50 tracks and provides no ISRC or album data.
//!   TODO: pagination/ISRC if a richer public source appears.
//! - Deezer: single public API call, no pagination — truncates around 400
//!   tracks. TODO: paginate `tracks.data`.
//! - Apple Music: scrapes `serialized-server-data` from the playlist page —
//!   the most fragile parser of the four.
//! - Tidal: fetches a fresh proxy token per playlist fetch (no caching/expiry
//!   handling). TODO: cache the token until expiry.
//! - Scrapers send no browser User-Agent (reqwest default) — TODO if any
//!   provider starts gating on UA.

pub mod errors;
pub mod importer;
pub mod match_qobuz;
pub mod models;
pub mod providers;
pub mod sink;

mod http;

pub use errors::PlaylistImportError;
pub use importer::{import_public_playlist, preview_public_playlist};
pub use models::{
    ImportPlaylist, ImportProgress, ImportProvider, ImportSummary, ImportTrack, TrackMatch,
};
pub use providers::{detect_music_resource, MusicProvider, MusicResource};
pub use sink::{ImportEvent, ImportPhase, ImportProgressSink};

/// Cloudflare Workers proxy that holds the third-party API credentials.
/// Hoisted from the Tidal provider so the future link-resolver port shares
/// the one constant instead of duplicating it.
pub const QBZ_PROXY_BASE: &str = "https://qbz-api-proxy.blitzkriegfc.workers.dev";

/// Provider key for the UI gate ("spotify" | "apple" | "tidal" | "deezer").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKey {
    Spotify,
    Apple,
    Tidal,
    Deezer,
}

impl ProviderKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKey::Spotify => "spotify",
            ProviderKey::Apple => "apple",
            ProviderKey::Tidal => "tidal",
            ProviderKey::Deezer => "deezer",
        }
    }
}

/// UI-gate provider detection — the exact substring rules of the Svelte
/// `detectProvider` (looser than [`providers::detect_provider`], which stays
/// the authoritative backend validation and may reject what this gate
/// passed). Kept here so the UI enable/disable logic and the backend share
/// one source of truth.
pub fn detect_provider_key(url: &str) -> Option<ProviderKey> {
    let url = url.trim();

    if url.starts_with("spotify:playlist:")
        || url.contains("open.spotify.com/playlist/")
        || url.contains("open.spotify.com/embed/playlist/")
    {
        return Some(ProviderKey::Spotify);
    }
    if url.contains("music.apple.com/") && url.contains("/playlist/") {
        return Some(ProviderKey::Apple);
    }
    if url.contains("tidal.com/") && url.contains("/playlist/") {
        return Some(ProviderKey::Tidal);
    }
    if url.contains("deezer.com/") && url.contains("/playlist/") {
        return Some(ProviderKey::Deezer);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_provider_key_table() {
        let cases: &[(&str, Option<ProviderKey>)] = &[
            // Spotify: URI, URL, embed
            (
                "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
                Some(ProviderKey::Spotify),
            ),
            (
                "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M",
                Some(ProviderKey::Spotify),
            ),
            (
                "https://open.spotify.com/embed/playlist/37i9dQZF1DXcBWIGoYBM5M",
                Some(ProviderKey::Spotify),
            ),
            // Apple: needs both music.apple.com/ AND /playlist/
            (
                "https://music.apple.com/us/playlist/top-100/pl.123",
                Some(ProviderKey::Apple),
            ),
            ("https://music.apple.com/us/album/x/123", None),
            // Tidal
            (
                "https://tidal.com/browse/playlist/abc-def",
                Some(ProviderKey::Tidal),
            ),
            ("https://tidal.com/browse/album/123", None),
            // Deezer
            (
                "https://www.deezer.com/en/playlist/1234567",
                Some(ProviderKey::Deezer),
            ),
            ("https://www.deezer.com/en/album/1234567", None),
            // Rejects
            ("https://open.spotify.com/track/abc", None),
            ("https://example.com/playlist/1", None),
            ("", None),
        ];

        for (url, expected) in cases {
            assert_eq!(detect_provider_key(url), *expected, "url: {}", url);
        }
    }

    #[test]
    fn detect_provider_key_trims_whitespace() {
        assert_eq!(
            detect_provider_key("  spotify:playlist:abc  "),
            Some(ProviderKey::Spotify)
        );
    }

    #[test]
    fn provider_key_as_str() {
        assert_eq!(ProviderKey::Spotify.as_str(), "spotify");
        assert_eq!(ProviderKey::Apple.as_str(), "apple");
        assert_eq!(ProviderKey::Tidal.as_str(), "tidal");
        assert_eq!(ProviderKey::Deezer.as_str(), "deezer");
    }
}
