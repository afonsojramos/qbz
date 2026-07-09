//! In-memory ephemeral library for ad-hoc folder playback.
//!
//! The implementation now lives in the frontend-agnostic `qbz-library` crate
//! (`qbz_library::ephemeral`, ADR-006) so both the Tauri and Slint frontends
//! consume identical logic. This module is a thin re-export shim that keeps the
//! existing `crate::ephemeral_library::*` call sites compiling unchanged.

pub use qbz_library::ephemeral::{
    EphemeralError, EphemeralFolderResult, EphemeralLibraryState, EPHEMERAL_ID_FLOOR,
};
