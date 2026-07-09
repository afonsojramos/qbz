//! Tauri commands for download settings.
//!
//! Persistence lives in `qbz-app`. Host filesystem validation stays here.

pub use qbz_app::settings::download::{
    create_download_settings_state, create_empty_download_settings_state, DownloadSettings,
    DownloadSettingsState, DownloadSettingsStore,
};

#[tauri::command]
pub fn get_download_settings(
    state: tauri::State<'_, DownloadSettingsState>,
) -> Result<DownloadSettings, String> {
    log::info!("Command: get_download_settings");
    let guard = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.get_settings()
}

#[tauri::command]
pub fn set_download_root(
    path: String,
    state: tauri::State<'_, DownloadSettingsState>,
) -> Result<(), String> {
    log::info!("Command: set_download_root to: {}", path);

    let path_obj = std::path::Path::new(&path);
    if !path_obj.exists() {
        return Err("Path does not exist".to_string());
    }
    if !path_obj.is_dir() {
        return Err("Path is not a directory".to_string());
    }

    let guard = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.set_download_root(&path)
}

#[tauri::command]
pub fn set_show_downloads_in_library(
    show: bool,
    state: tauri::State<'_, DownloadSettingsState>,
) -> Result<(), String> {
    log::info!("Command: set_show_downloads_in_library to: {}", show);
    let guard = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.set_show_in_library(show)
}

#[tauri::command]
pub fn validate_download_root(path: String) -> Result<bool, String> {
    log::info!("Command: validate_download_root: {}", path);

    let path_obj = std::path::Path::new(&path);

    if !path_obj.exists() {
        return Ok(false);
    }
    if !path_obj.is_dir() {
        return Err("Path exists but is not a directory".to_string());
    }

    let test_file = path_obj.join(".qbz_write_test");
    match std::fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            Ok(true)
        }
        Err(e) => Err(format!("No write permission: {}", e)),
    }
}
