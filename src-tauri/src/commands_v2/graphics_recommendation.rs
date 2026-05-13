//! V2 commands that expose the graphics auto-config recommendation to the
//! Settings UI. The detection + decision logic lives in
//! `autoconfig_graphics.rs` (shared with the `qbz --autoconfig-graphics` CLI
//! tool); this module is just the Tauri surface that the Graphics tab uses
//! to render the "Detected / Recommended" banner.

use crate::autoconfig_graphics::{
    compute_recommendation, detect_environment, write_recommendation, Environment, Recommendation,
};
use serde::Serialize;

/// Payload returned to the Settings UI. Splits environment from
/// recommendation so the frontend can show a human-readable detection
/// line and then compare the recommendation to the user's current
/// persisted settings to decide whether to surface the banner at all.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct GraphicsRecommendationPayload {
    pub environment: Environment,
    pub recommendation: Recommendation,
}

/// Compute the recommendation for the current host. Cheap — runs the
/// same detection that the CLI tool runs and synthesizes the matrix
/// decision. The Settings UI calls this on Graphics-tab mount.
#[tauri::command]
pub fn v2_get_graphics_recommendation() -> GraphicsRecommendationPayload {
    let environment = detect_environment();
    let recommendation = compute_recommendation(&environment);
    GraphicsRecommendationPayload {
        environment,
        recommendation,
    }
}

/// Write the current recommendation to the persistence layer.
/// `force_dmabuf` is derived from `disable_dmabuf` per the 1.2.13
/// opt-in semantics (see `write_recommendation` in autoconfig_graphics).
/// Returns the list of per-field errors so the UI can surface them.
/// Empty `Ok(Vec)` means "everything written, restart required".
#[tauri::command]
pub fn v2_apply_graphics_recommendation() -> Result<Vec<String>, Vec<String>> {
    let environment = detect_environment();
    let recommendation = compute_recommendation(&environment);
    match write_recommendation(&recommendation) {
        Ok(()) => Ok(Vec::new()),
        Err(errors) => Err(errors),
    }
}
