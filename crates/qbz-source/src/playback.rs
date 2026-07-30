//! What the player needs to start a track — as DATA.
//!
//! This crate does not link `qbz-player` or `qbz-audio` and therefore *cannot*
//! touch sample rate, resampling or device selection. The ticket says what to
//! do; the frontend performs the entry into the PROTECTED audio path, at the
//! same `play_data` / `play_dsd_file` / `play_track_resolved` seams it uses
//! today.

use std::path::PathBuf;

/// Everything the frontend needs to enter the PROTECTED audio path.
///
/// **NOT `#[non_exhaustive]`, deliberately.** `qbz-source` and `qbz-qt` ship in
/// the same workspace and are versioned together; the attribute would force the
/// frontend's `match` to carry a `_ =>` arm, which silently swallows a variant
/// added later. That is the opposite of the "the compiler will not let you skip
/// a source" property the whole design leans on. When the documented
/// progressive-streaming follow-up (local_playback.rs:186-192) adds
/// `Stream { .. }`, the frontend match failing to compile is the feature.
#[derive(Clone, PartialEq)]
pub enum PlaybackTicket {
    /// Read this file and hand the bytes to `player().play_data(bytes, play_id)`.
    ///
    /// `seek_secs` is the CUE virtual-track offset (local_playback.rs:152-158,
    /// :177-180): every virtual track of a CUE album shares ONE audio file, so
    /// the frontend seeks after the load lands.
    File {
        path: PathBuf,
        play_id: u64,
        seek_secs: Option<f64>,
    },
    /// Stream from disk: `player().play_dsd_file(path, play_id)`
    /// (local_playback.rs:139-149). DSD stays on its additive path, untouched.
    DsdFile { path: PathBuf, play_id: u64 },
    /// The source already fetched the ORIGINAL bytes (Plex direct-play, no
    /// transcode requested): `player().play_data(bytes, play_id)`
    /// (local_playback.rs:199-206).
    Bytes { bytes: Vec<u8>, play_id: u64 },
    /// Let the core resolve + stream it: `core().play_track_resolved(track_id, …)`.
    Catalog { track_id: u64 },
    /// The current track is already loaded and this is a seek within the same
    /// container — the CUE fast path (local_playback.rs:150-158).
    ///
    /// NOTE (stage-1 status): **no source produces this yet.** The "is this
    /// container already loaded?" test reads `player().state.current_track_id()`
    /// and `has_loaded_audio()`, which this crate cannot link (§8 — the
    /// PROTECTED backend is not a dependency). `LocalSource::playback` emits
    /// `File { seek_secs: Some(_) }` instead and the frontend keeps making that
    /// decision where it makes it today. Kept because the design specifies it;
    /// if stage 3 confirms the frontend never wants a source to say it, this is
    /// the variant to cut.
    SeekLoaded { play_id: u64, secs: f64 },
}

/// Hand-written so `Bytes` prints its LENGTH, not a megabyte of FLAC. A
/// derived `Debug` would dump the whole buffer into any log line that formats
/// a ticket.
impl std::fmt::Debug for PlaybackTicket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaybackTicket::File {
                path,
                play_id,
                seek_secs,
            } => f
                .debug_struct("File")
                .field("path", path)
                .field("play_id", play_id)
                .field("seek_secs", seek_secs)
                .finish(),
            PlaybackTicket::DsdFile { path, play_id } => f
                .debug_struct("DsdFile")
                .field("path", path)
                .field("play_id", play_id)
                .finish(),
            PlaybackTicket::Bytes { bytes, play_id } => f
                .debug_struct("Bytes")
                .field("len", &bytes.len())
                .field("play_id", play_id)
                .finish(),
            PlaybackTicket::Catalog { track_id } => f
                .debug_struct("Catalog")
                .field("track_id", track_id)
                .finish(),
            PlaybackTicket::SeekLoaded { play_id, secs } => f
                .debug_struct("SeekLoaded")
                .field("play_id", play_id)
                .field("secs", secs)
                .finish(),
        }
    }
}

impl PlaybackTicket {
    /// The player-side id this ticket plays under, when it has one.
    pub fn play_id(&self) -> Option<u64> {
        match self {
            PlaybackTicket::File { play_id, .. }
            | PlaybackTicket::DsdFile { play_id, .. }
            | PlaybackTicket::Bytes { play_id, .. }
            | PlaybackTicket::SeekLoaded { play_id, .. } => Some(*play_id),
            PlaybackTicket::Catalog { .. } => None,
        }
    }
}
