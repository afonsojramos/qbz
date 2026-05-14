//! Tauri commands for legal settings.

pub use qbz_app::settings::legal::{
    create_empty_legal_settings_state, create_legal_settings_state, LegalSettings,
    LegalSettingsState, LegalSettingsStore,
};

#[tauri::command]
pub fn get_legal_settings(
    state: tauri::State<'_, LegalSettingsState>,
) -> Result<LegalSettings, String> {
    let guard = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.get_settings()
}

#[tauri::command]
pub fn get_qobuz_tos_accepted(state: tauri::State<'_, LegalSettingsState>) -> Result<bool, String> {
    let guard = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    let settings = store.get_settings()?;
    Ok(settings.qobuz_tos_accepted)
}

#[tauri::command]
pub fn set_qobuz_tos_accepted(
    state: tauri::State<'_, LegalSettingsState>,
    accepted: bool,
) -> Result<(), String> {
    let guard = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.set_qobuz_tos_accepted(accepted)
}
