//! Plex LAN-only integration — re-export shim.
//!
//! The implementation now lives in the frontend-agnostic `qbz-plex` core crate
//! (HTTP client, hand-rolled XML metadata parsing, and the SQLite `plex_cache.db`),
//! so it can be shared by every frontend (Tauri, Slint, TUI) per ADR-006.
//!
//! This module re-exports the crate's public surface so existing `crate::plex::*`
//! call sites keep compiling unchanged. The only Tauri-coupled piece (playing the
//! resolved bytes through `AppState.player`) lives in `commands_v2` as the thin
//! `v2_plex_play_track` command, which calls `qbz_plex::plex_resolve_track_media`.

pub use qbz_plex::*;
