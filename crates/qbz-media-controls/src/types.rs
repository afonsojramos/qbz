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
