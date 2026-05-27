//! Share-link helpers — Qobuz track URL + Song.link (Odesli) resolution
//! + clipboard copy. Used by the track context menu's Share actions.

/// Canonical Qobuz track URL (the same form Tauri feeds to Song.link).
pub fn qobuz_track_url(track_id: &str) -> String {
    format!("https://www.qobuz.com/track/{track_id}")
}

/// Qobuz web-player playlist URL (matches Tauri's share-playlist link).
pub fn qobuz_playlist_url(playlist_id: &str) -> String {
    format!("https://play.qobuz.com/playlist/{playlist_id}")
}

/// Copy `text` to the system clipboard. Runs on a blocking thread —
/// clipboard backends (X11/Wayland) can block, and arboard keeps an
/// owner thread alive so the contents persist after this returns.
pub fn copy_to_clipboard(text: String) {
    tokio::task::spawn_blocking(move || match arboard::Clipboard::new() {
        Ok(mut clipboard) => {
            if let Err(e) = clipboard.set_text(text) {
                log::warn!("[qbz-slint] clipboard set failed: {e}");
            }
        }
        Err(e) => log::warn!("[qbz-slint] clipboard unavailable: {e}"),
    });
}

/// Resolve a source URL to its universal Song.link (Odesli) page URL.
/// One GET to the Odesli API; returns the `pageUrl` field.
pub async fn songlink_url(source_url: &str) -> Option<String> {
    let resp = reqwest::Client::new()
        .get("https://api.song.link/v1-alpha.1/links")
        .query(&[("url", source_url)])
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        log::warn!("[qbz-slint] song.link status {}", resp.status());
        return None;
    }
    let value: serde_json::Value = resp.json().await.ok()?;
    value
        .get("pageUrl")
        .and_then(|p| p.as_str())
        .map(|s| s.to_string())
}
