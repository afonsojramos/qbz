//! Tauri adapter for developer settings.
//!
//! The portable settings store lives in `qbz-app`; this module keeps the
//! existing Tauri command surface unchanged.

pub use qbz_app::settings::developer::{
    DeveloperSettings, DeveloperSettingsState, DeveloperSettingsStore,
};

#[tauri::command]
pub fn get_developer_settings(
    state: tauri::State<'_, DeveloperSettingsState>,
) -> Result<DeveloperSettings, String> {
    let guard = state
        .store
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let store = guard
        .as_ref()
        .ok_or("Developer settings store not initialized")?;
    store.get_settings()
}

#[tauri::command]
pub fn set_developer_force_dmabuf(
    state: tauri::State<'_, DeveloperSettingsState>,
    enabled: bool,
) -> Result<(), String> {
    log::info!("Command: set_developer_force_dmabuf {}", enabled);
    let guard = state
        .store
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let store = guard
        .as_ref()
        .ok_or("Developer settings store not initialized")?;
    store.set_force_dmabuf(enabled)
}
