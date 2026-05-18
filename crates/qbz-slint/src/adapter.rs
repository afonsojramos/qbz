//! Slint frontend adapter.
//!
//! `SlintAdapter` implements `FrontendAdapter` so `QbzCore` can emit events
//! toward the Slint UI. It holds a weak handle to the window to post updates
//! onto the Slint event loop. The auth milestone only logs events; later
//! milestones route them into UI models.

use async_trait::async_trait;
use qbz_core::{CoreEvent, FrontendAdapter};

use crate::AppWindow;

pub struct SlintAdapter {
    #[allow(dead_code)] // used by later milestones to push UI model updates
    window: slint::Weak<AppWindow>,
}

impl SlintAdapter {
    pub fn new(window: slint::Weak<AppWindow>) -> Self {
        Self { window }
    }
}

#[async_trait]
impl FrontendAdapter for SlintAdapter {
    async fn on_event(&self, event: CoreEvent) {
        log::debug!("[qbz-slint] core event: {:?}", event);
    }

    async fn on_ready(&self) {
        log::info!("[qbz-slint] core ready");
    }
}
