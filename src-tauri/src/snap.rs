//! Snap runtime detection and utilities
//!
//! Detects when QBZ is running inside a Snap sandbox
//! and provides sandbox-specific guidance.

use std::env;

/// Check if QBZ is running inside a Snap sandbox
pub fn is_snap() -> bool {
    env::var("SNAP").is_ok()
}

#[tauri::command]
pub fn is_running_in_snap() -> bool {
    is_snap()
}
