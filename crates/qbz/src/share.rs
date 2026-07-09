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

/// Qobuz web-player album URL (matches Tauri's `shareAlbumQobuzLink`,
/// `https://play.qobuz.com/album/{id}`). Also the source URL fed to
/// Song.link for the album-level "Album.link".
pub fn qobuz_album_url(album_id: &str) -> String {
    format!("https://play.qobuz.com/album/{album_id}")
}

/// Qobuz web-player artist URL (header Share action).
pub fn qobuz_artist_url(artist_id: &str) -> String {
    format!("https://play.qobuz.com/artist/{artist_id}")
}

/// Long-lived clipboard instance. arboard ties the offer's lifetime to the
/// LAST live `Clipboard` object: dropping it destroys the X11 selection
/// window (contents survive only when a clipboard MANAGER accepts the
/// handoff — KDE ships one, stock GNOME/XFCE/Cinnamon do not) and ends the
/// Wayland offer with the same rule. The old create-per-copy pattern
/// therefore worked on KDE and silently lost the text everywhere else
/// (HiFi-wizard copy report, #514). One instance kept alive for the whole
/// process serves the offer like any normal app.
static CLIPBOARD: std::sync::OnceLock<std::sync::Mutex<Option<arboard::Clipboard>>> =
    std::sync::OnceLock::new();

/// Copy `text` to the system clipboard. Runs on a blocking thread —
/// clipboard backends (X11/Wayland) can block.
pub fn copy_to_clipboard(text: String) {
    tokio::task::spawn_blocking(move || {
        let cell = CLIPBOARD.get_or_init(|| std::sync::Mutex::new(None));
        let mut guard = match cell.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if guard.is_none() {
            match arboard::Clipboard::new() {
                Ok(c) => *guard = Some(c),
                Err(e) => {
                    log::warn!("[qbz-slint] clipboard unavailable: {e}");
                    return;
                }
            }
        }
        if let Some(clipboard) = guard.as_mut() {
            if let Err(e) = clipboard.set_text(text) {
                log::warn!("[qbz-slint] clipboard set failed: {e}");
                // Drop the instance so the next copy reconnects — the
                // display connection may have gone away.
                *guard = None;
            }
        }
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
