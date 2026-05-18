//! Credential storage — Tauri-side wiring.
//!
//! The implementation now lives in the `qbz-credentials` crate so the
//! Slint shell shares the exact same keyring + AES-256-GCM file store.
//! This module re-exports it; the Tauri app and the Slint shell read and
//! write the same credentials and OAuth token.

pub use qbz_credentials::*;
