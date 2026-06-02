//! Queue cursor / index resolution helpers — relocated to `qconnect-app` (slice
//! 6: pure protocol math, no engine/Tauri deps). Re-exported here so existing
//! `super::queue_resolution::…` call sites inside this module compile unchanged.
pub(super) use qconnect_app::queue_resolution::*;
