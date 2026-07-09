//! Tauri commands for favorites preferences.

pub use qbz_app::settings::favorites::{
    create_table, load_preferences, save_preferences, FavoritesPreferences,
    FavoritesPreferencesState, FavoritesPreferencesStore,
};

#[tauri::command]
pub fn get_favorites_preferences(
    state: tauri::State<FavoritesPreferencesState>,
) -> Result<FavoritesPreferences, String> {
    let guard = state
        .store
        .lock()
        .map_err(|_| "Failed to lock favorites preferences store".to_string())?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.get_preferences()
}

#[tauri::command]
pub fn save_favorites_preferences(
    prefs: FavoritesPreferences,
    state: tauri::State<FavoritesPreferencesState>,
) -> Result<FavoritesPreferences, String> {
    let guard = state
        .store
        .lock()
        .map_err(|_| "Failed to lock favorites preferences store".to_string())?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.save_preferences(prefs)
}
