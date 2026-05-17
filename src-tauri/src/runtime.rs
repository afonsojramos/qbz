//! Tauri runtime state wrapper.

pub use qbz_app::runtime::{
    CommandRequirement, DegradedReason, RuntimeError, RuntimeEvent, RuntimeManager, RuntimeState,
    RuntimeStatus,
};
use std::sync::Arc;

/// Tauri state wrapper
pub struct RuntimeManagerState(pub Arc<RuntimeManager>);

impl RuntimeManagerState {
    pub fn new() -> Self {
        Self(Arc::new(RuntimeManager::new()))
    }

    pub fn manager(&self) -> &RuntimeManager {
        &self.0
    }
}

impl Default for RuntimeManagerState {
    fn default() -> Self {
        Self::new()
    }
}
