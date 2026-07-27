//! Frontend-agnostic media-control types — no winit / Slint / Tauri here.

use std::time::Duration;

/// Now-playing metadata pushed to the OS media controls.
#[derive(Debug, Clone, Default)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<Duration>,
    /// `mpris:artUrl` — the ALBUM ART for the track (http/https/file URL). This
    /// is NOT the application icon (that is resolved by GNOME from the MPRIS
    /// `DesktopEntry` property → the installed .desktop → `Icon=`).
    pub art_url: Option<String>,
    /// `xesam:url` (#658) — the track's PUBLIC web link so external scripts
    /// (playerctl widgets, handoff automation, scrobblers) can identify the
    /// exact song: `https://open.qobuz.com/track/<id>` for Qobuz catalog rows
    /// (offline `qobuz_download` keeps its real Qobuz id, so it qualifies too).
    /// None for local/Plex rows — QueueTrack carries no file path to map to a
    /// file:// URI.
    pub url: Option<String>,
}

/// Build the `xesam:url` for a queue track (#658): the public Qobuz web link
/// (`https://open.qobuz.com/track/<id>`) for catalog tracks — offline
/// `qobuz_download` rows qualify too, their id IS the real Qobuz id. None for
/// `local` / `plex` rows: QueueTrack carries no file path to map to a file://
/// URI, and a synthetic local id would produce a bogus Qobuz link.
pub fn xesam_url_for(source: Option<&str>, track_id: u64) -> Option<String> {
    match source.map(|s| s.to_ascii_lowercase()) {
        Some(ref s) if s == "local" || s == "plex" => None,
        _ if track_id > 0 => Some(format!("https://open.qobuz.com/track/{track_id}")),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// Inbound events from the OS media controls (media keys, GNOME/KDE widget,
/// macOS Now Playing, Windows SMTC). Delivered to the callback passed to
/// [`crate::spawn`]. All time values are MICROSECONDS.
#[derive(Debug, Clone)]
pub enum MediaEvent {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    /// Relative seek by the given micros (signed; MPRIS `Seek`).
    SeekBy(i64),
    /// Absolute seek to the given micros (MPRIS `SetPosition`).
    SetPosition(i64),
    /// Set volume 0.0..=1.0.
    SetVolume(f64),
    Raise,
    Quit,
}

/// A live handle to the OS media-controls integration. Cloneable callers hold
/// it for the app lifetime and push state through it; dropping the last one
/// tears the integration down.
pub trait MediaIntegration: Send + Sync {
    fn set_metadata(&self, meta: &TrackMeta);
    fn set_playback(&self, status: PlaybackStatus, position: Option<Duration>);
    fn set_volume(&self, vol: f64);
}
