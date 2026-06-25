//! "Open Qobuz Link" (Ctrl+L) controller.
//!
//! Bridges the frontend-agnostic `qbz-music-link` crate (the cross-platform
//! resolver extracted out of the Tauri command) to the Slint frontend: it
//! supplies a `QobuzSearchBridge` over `QbzCore`, the instant platform
//! detection for the input icon, and the async resolve entry point. The actual
//! navigation on a `Resolved` result is wired in `main.rs` (where the
//! `navigate_*` helpers live).

use std::sync::Arc;

use async_trait::async_trait;
use qbz_app::shell::AppRuntime;
use qbz_models::{Album, SearchResultsPage, Track};
use qbz_music_link::{resolve_music_link, MusicLinkResult, QobuzSearchBridge, SongLinkClient};

use crate::adapter::SlintAdapter;

/// Instant, network-free platform detection for the modal's leading icon.
/// Mirrors the Tauri `detectPlatform`. Returns "" when unknown.
pub fn detect_platform(url: &str) -> &'static str {
    let lower = url.trim().to_ascii_lowercase();
    if lower.contains("qobuz.com/") || lower.starts_with("qobuzapp://") {
        "qobuz"
    } else if lower.contains("spotify.com/") || lower.starts_with("spotify:") {
        "spotify"
    } else if lower.contains("music.apple.com/") {
        "apple"
    } else if lower.contains("tidal.com/") {
        "tidal"
    } else if lower.contains("deezer.com/") {
        "deezer"
    } else if lower.contains("song.link/")
        || lower.contains("album.link/")
        || lower.contains("odesli.co/")
    {
        "songlink"
    } else {
        ""
    }
}

/// `QobuzSearchBridge` over the live `QbzCore` (the smart-search fallback the
/// cross-platform resolver uses when an Odesli match must be found on Qobuz).
struct CoreSearchBridge {
    runtime: Arc<AppRuntime<SlintAdapter>>,
}

#[async_trait]
impl QobuzSearchBridge for CoreSearchBridge {
    async fn search_tracks(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<SearchResultsPage<Track>, String> {
        let core = self.runtime.core();
        core.search_tracks(query, limit as u32, offset as u32, None)
            .await
            .map_err(|e| e.to_string())
    }

    async fn search_albums(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<SearchResultsPage<Album>, String> {
        let core = self.runtime.core();
        core.search_albums(query, limit as u32, offset as u32, None)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Resolve a pasted URL to a Qobuz entity (or a playlist / not-found verdict).
/// Native Qobuz links resolve offline; cross-platform links hit Odesli + a
/// smart Qobuz search.
pub async fn resolve(
    runtime: Arc<AppRuntime<SlintAdapter>>,
    url: String,
) -> Result<MusicLinkResult, String> {
    let bridge = CoreSearchBridge { runtime };
    let songlink = SongLinkClient::new();
    resolve_music_link(&url, &songlink, &bridge)
        .await
        .map_err(|e| e.to_string())
}
