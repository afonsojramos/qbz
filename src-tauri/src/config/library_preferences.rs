//! Tauri commands for library preferences.

pub use qbz_app::settings::library::{
    FoldersViewMode, LibraryPreferences, LibraryPreferencesState, LibraryPreferencesStore,
};

#[tauri::command]
pub fn get_library_preferences(
    state: tauri::State<LibraryPreferencesState>,
) -> Result<LibraryPreferences, String> {
    let guard = state
        .store
        .lock()
        .map_err(|_| "Failed to lock library preferences store".to_string())?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.get_preferences()
}

#[tauri::command]
pub fn save_library_preferences(
    prefs: LibraryPreferences,
    state: tauri::State<LibraryPreferencesState>,
) -> Result<LibraryPreferences, String> {
    let guard = state
        .store
        .lock()
        .map_err(|_| "Failed to lock library preferences store".to_string())?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.save_preferences(prefs)
}
