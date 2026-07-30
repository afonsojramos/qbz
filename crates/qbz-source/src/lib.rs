//! `qbz-source` — the source-agnostic seam.
//!
//! # The problem this exists to delete
//!
//! Views mix items from Qobuz, local folders and Plex in ONE list, and every
//! view re-derives the per-source handling by hand. Four concerns that should
//! be one seam each are decided in **7 + 5 + 4 + ~15** separate places
//! (`01-survey.md` §1), and splitting them is mechanically why "siempre falta
//! algo": a view that learned two of the three sniffs silently drops the third,
//! and nothing fails loudly — it renders a placeholder disc, returns an empty
//! track list, or 404s a resolve.
//!
//! | concern | Qobuz path today | local path today | Plex path today | who wires it |
//! |---|---|---|---|---|
//! | tracks | `qbz_mixtape::resolve_qobuz_album` | `resolve_local_album_tracks` | `resolve_plex_album_tracks` | every view |
//! | artwork | `artwork_qt::cached_path` | `local_artwork::local_thumbnail` | `local_plex::thumb_url` | every view |
//! | playback | `core().play_track_resolved` | `play_local_file` | `play_plex_track` | 2 duplicated routers |
//! | context/meta | `to_item` + resolve cache | `map_track` / `map_album` | same, plus merges | every view |
//!
//! This crate collapses that table into **one trait with four methods and one
//! registry**: [`Source`] and [`SourceRegistry`].
//!
//! # The three measured bugs, and where each dies
//!
//! 1. **`library.db` reopened per item** (23–34 opens for ONE MyQBZ
//!    collection). `LocalSource` owns a connection pool; `local_state::with_db`
//!    and `library_db_qt::with_db` keep their signatures and forward to
//!    [`sources::LocalSource::with`] / `with_creating`, so their 48 call sites
//!    are untouched and only the number of opens changes.
//! 2. **A `plex:<hash>` album key used as a rating key** → 404, the track never
//!    starts and the PREVIOUS track keeps playing under the new card. Killed by
//!    four independent properties: [`MediaRef`] has no public constructor;
//!    [`SourceRegistry::claim`] is the single normalisation point on EVERY path
//!    including playback; [`PlaybackTicket`] is opaque data with no field a
//!    source-specific id could be smuggled through; and there is exactly one
//!    router.
//! 3. **Missing artwork on the local rows of a hybrid list.** [`Source::artwork`]
//!    is on the trait, so a hybrid list makes ONE call per row regardless of
//!    source and cannot wire fewer pipelines than it has sources.
//!
//! # Shape rules
//!
//! - **Additive.** Nothing in `qbz/src` (Slint) or `qbz-mixtape` changes; this
//!   crate implements `qbz_mixtape::ItemResolver`, it does not absorb it.
//! - **The PROTECTED audio backend is not linked.** `qbz-audio` and
//!   `qbz-player` are not dependencies, so sample rate, resampling and device
//!   selection are unreachable from here. [`PlaybackTicket`] is data; the
//!   frontend performs the entry.
//! - **No executor.** This crate never spawns and does not depend on `tokio`.
//!   The blocking arms (SQLite, `stat`) stay blocking and the caller keeps
//!   wrapping them in `spawn_blocking` exactly where it does today.
//! - **Three impls, one registry, one obvious slot for a fourth.** No dynamic
//!   registry, no config loading, no trait hierarchy.
//!
//! # Adding a fourth source (Jellyfin, Navidrome)
//!
//! `sources/jellyfin.rs` (one impl), one `pub mod` + `pub use` in
//! [`sources`], one [`SourceId`] constant, one word in
//! [`SourceId::from_word`], one [`SourceBadge`] variant, and one line in
//! [`SourceRegistry::with_defaults`]. **Zero** changes in any view, any
//! playback path, any artwork path — because all four concerns hang off the one
//! trait.

pub mod art;
pub mod error;
pub mod id;
pub mod meta;
pub mod mixtape_adapter;
pub mod playback;
pub mod registry;
pub mod source;
pub mod sources;

pub use art::{ArtRef, ArtSize, PLEX_THUMB_PX};
pub use error::SourceError;
pub use id::{ItemKind, MediaRef, RawRef, SourceId};
pub use meta::{ItemMeta, QualityHint, SourceBadge};
pub use mixtape_adapter::RegistryResolver;
pub use playback::PlaybackTicket;
pub use registry::{registry, SourceRegistry};
pub use source::Source;
pub use sources::{
    local_queue_track, EphemeralTracks, LocalSource, PlexCreds, PlexSource, QobuzSource,
};
