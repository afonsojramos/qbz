//! Local Library data layer — FACADE.
//!
//! The Qt/QML port of the shipping Slint controller (`crates/qbz/src/
//! local_library.rs`). ADR-006: NOTHING here re-implements scanning, album
//! identity or the queries — every read goes through the frontend-agnostic
//! `qbz-library` crate (`LibraryDatabase`), and every Plex read goes through
//! `qbz-plex` + the shared `qbz_app::settings::plex` store.
//!
//! This file used to be the whole layer (1,237 lines). It is now a thin
//! re-export surface over five sibling modules, one per concern — the split
//! is by CONCERN, not by line count:
//!
//! | module              | owns                                              |
//! |---------------------|---------------------------------------------------|
//! | `local_rows`        | the transport rows + `qbz_library` -> row mappers |
//! | `local_state`       | `library.db` access, prefs, the document cache    |
//! | `local_plex`        | Plex settings/gates/cache reads + the manual sync |
//! | `local_albums`      | albums, artists, Tracks pages, badges, detail     |
//! | `local_tree`        | the lazy folder tree + folder detail              |
//! | `local_artwork`     | windowed, id-keyed artwork (local + Plex thumbs)  |
//! | `local_playback`    | queue mapping + the source-aware audible steps    |
//!
//! Everything the bridge (`local_bridge.rs`) and the shared playback
//! controller call is re-exported here, so call sites keep reading
//! `crate::local_library_qt::…`.
//!
//! PLEX (wired 2026-07-28, was the port's headline gap):
//! - Albums/badges: `get_albums_metadata_page(..., plex_cache_path, ...)`
//!   ATTACHes `<data_dir>/qbz/plex_cache.db` when the master toggle is ON, so
//!   the grid is the local+Plex UNION. Toggle OFF -> `None` -> local-only,
//!   byte-for-byte the pre-Plex behaviour.
//! - Tracks: the full Plex search set merges ONCE on page 1; later pages stay
//!   pure local so the LIMIT/OFFSET path and `has_more` are untouched.
//! - Artists: the aggregated Plex artists fold into the rail by name.
//! - Artwork: `/library/...` thumbs resolve through the shared image cache
//!   with a tokenized transcode URL; no token ever reaches QML.
//! - Playback: Plex rows carry their `rating_key` in `source_item_id_hint`
//!   and resolve their direct-play part at play time.
//!
//! Remaining POC-NOTEs (deliberate cuts, named for the effort report):
//! - Ephemeral folders, the tag editor, bulk multi-select bars, mixtapes and
//!   the A-Z alpha strips are out of scope.
//! - Network-folder exclusion is always `false` here (the Slint keys it on
//!   live connectivity); the index stays the source of truth.
//! - Plex playback downloads the whole part instead of Range-streaming it
//!   (the Slint's own documented fallback — the streaming feeder lives inside
//!   the Slint binary).

// The stable facade over the local_* modules: `local_bridge_ops` imports this
// module as `lib` and reaches everything through it, so the re-exports are
// load-bearing even where a plain grep for `local_library_qt::` misses them.
// (An earlier trim based on such a grep broke 47 call sites.)
#![allow(unused_imports)]

// --- rows ------------------------------------------------------------------
pub use crate::local_rows::{
    album_key, artist_key, badge_source, folder_key, tier_of, to_json, track_key,     AlbumRow, ArtistRow, FolderDetail, LocalCounts, SubfolderRow, TrackRow, TreeNode,
};

// --- state / prefs ---------------------------------------------------------
pub use crate::local_state::{
    album_mode, counts, has_library, set_album_mode, set_tracks_group, set_tracks_query,
    set_tracks_sort, state, tracks_group, tracks_has_more, tracks_sort, with_db,
};

// --- queries ---------------------------------------------------------------
pub use crate::local_albums::{
    fetch_album_tracks_blocking, load_album_detail_blocking, load_albums_blocking,
    load_artists_blocking, load_counts_blocking, load_folders_blocking, load_tracks_page_blocking,
};

// --- folder tree -----------------------------------------------------------
pub use crate::local_tree::{
    load_folder_detail_blocking, load_tree_roots_blocking, set_tree_search, tree_collapse,
    tree_collapse_all, tree_expand_blocking, tree_visible,
};

// --- artwork ---------------------------------------------------------------
pub use crate::local_artwork::{fetch_plex_misses, resolve_window_blocking, ArtworkWindow};

// --- playback --------------------------------------------------------------
pub use crate::local_playback::{
    enqueue, local_queue_track, play_album, play_current_if_local, play_folder, play_folder_track,
    play_local_file, play_plex_track, play_tracks_visible,
};

// --- plex ------------------------------------------------------------------
pub use crate::local_plex::{
    is_configured as plex_configured, is_enabled as plex_enabled, is_syncing as plex_syncing,
    sync_now as plex_sync_now,
};

/// Drop every cached document AND unbind the Plex store (logout / user
/// switch — the next read re-binds to the new user's `plex_settings.db`).
pub fn reset() {
    crate::local_tree::tree_clear_selection();
    crate::local_state::reset();
    crate::local_plex::reset();
}
