//! Image cache — Tauri-side wiring.
//!
//! The cache implementation now lives in the `qbz-cache` crate so the
//! Slint shell shares the exact same code and on-disk cache. This module
//! re-exports it and keeps the Tauri managed-state wrapper.

use std::sync::{Arc, Mutex};

pub use qbz_cache::{ImageCacheService, ImageCacheStats};

pub struct ImageCacheState {
    pub service: Arc<Mutex<Option<ImageCacheService>>>,
}

impl ImageCacheState {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            service: Arc::new(Mutex::new(Some(ImageCacheService::new()?))),
        })
    }

    pub fn new_empty() -> Self {
        Self {
            service: Arc::new(Mutex::new(None)),
        }
    }
}
