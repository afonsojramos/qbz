//! The three implementations that exist, and the one obvious slot a fourth
//! (Jellyfin, Navidrome) would take: a new file here, a new `pub mod` /
//! `pub use` line, a `SourceId` constant, a badge value and one line in
//! `SourceRegistry::with_defaults`. Zero edits in any view.

pub mod local;
pub mod plex;
mod pool;
pub mod qobuz;

pub use local::{local_queue_track, EphemeralTracks, LocalSource};
pub use plex::{is_thumb_path, PlexCreds, PlexSource, PLEX_TRACK_ID_FLOOR};
pub use qobuz::{ClientFuture, ClientLens, QobuzSource};
