//! Tauri-facing re-exports for remote control settings.
//!
//! Persistence lives in `qbz-app`. API server lifecycle, TLS, QR generation,
//! CORS application, and live restart behavior remain host-owned.

pub use qbz_app::settings::remote_control::{
    AllowedOrigin, AllowedOriginsState, AllowedOriginsStore, RemoteControlSettings,
    RemoteControlSettingsState, RemoteControlSettingsStore,
};
