use qbz_app::session_store::{
    PersistedPlaybackSession, PersistedSessionSnapshot, PersistedShellViewState,
    SessionStore as AppSessionStore,
};
pub use qbz_app::session_store::PersistedQueueTrack;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Represents the full persisted session state as exposed through Tauri
/// commands. Keep this flat shape stable for the existing frontend payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSession {
    pub queue_tracks: Vec<PersistedQueueTrack>,
    pub current_index: Option<usize>,
    pub current_position_secs: u64,
    pub volume: f32,
    pub shuffle_enabled: bool,
    pub repeat_mode: String, // "off", "all", "one"
    pub was_playing: bool,
    pub saved_at: i64,
    #[serde(default = "default_last_view")]
    pub last_view: String,
    #[serde(default)]
    pub view_context_id: Option<String>,
    #[serde(default)]
    pub view_context_type: Option<String>,
}

fn default_last_view() -> String {
    "home".to_string()
}

impl Default for PersistedSession {
    fn default() -> Self {
        Self {
            queue_tracks: Vec::new(),
            current_index: None,
            current_position_secs: 0,
            volume: 0.75,
            shuffle_enabled: false,
            repeat_mode: "off".to_string(),
            was_playing: false,
            saved_at: 0,
            last_view: "home".to_string(),
            view_context_id: None,
            view_context_type: None,
        }
    }
}

impl PersistedSession {
    fn to_snapshot(&self) -> PersistedSessionSnapshot {
        PersistedSessionSnapshot {
            playback: PersistedPlaybackSession {
                queue_tracks: self.queue_tracks.clone(),
                current_index: self.current_index,
                current_position_secs: self.current_position_secs,
                volume: self.volume,
                shuffle_enabled: self.shuffle_enabled,
                repeat_mode: self.repeat_mode.clone(),
                was_playing: self.was_playing,
                saved_at: self.saved_at,
            },
            shell_view: PersistedShellViewState {
                last_view: self.last_view.clone(),
                view_context_id: self.view_context_id.clone(),
                view_context_type: self.view_context_type.clone(),
            },
        }
    }

    fn from_snapshot(snapshot: PersistedSessionSnapshot) -> Self {
        Self {
            queue_tracks: snapshot.playback.queue_tracks,
            current_index: snapshot.playback.current_index,
            current_position_secs: snapshot.playback.current_position_secs,
            volume: snapshot.playback.volume,
            shuffle_enabled: snapshot.playback.shuffle_enabled,
            repeat_mode: snapshot.playback.repeat_mode,
            was_playing: snapshot.playback.was_playing,
            saved_at: snapshot.playback.saved_at,
            last_view: snapshot.shell_view.last_view,
            view_context_id: snapshot.shell_view.view_context_id,
            view_context_type: snapshot.shell_view.view_context_type,
        }
    }
}

/// Thin Tauri-side adapter that preserves the existing flat command model while
/// delegating persistence to qbz-app.
pub struct SessionStore {
    inner: AppSessionStore,
}

impl SessionStore {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            inner: AppSessionStore::new()?,
        })
    }

    pub fn new_at(base_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            inner: AppSessionStore::new_at(base_dir)?,
        })
    }

    pub fn save_session(&self, session: &PersistedSession) -> Result<(), String> {
        self.inner.save_session(&session.to_snapshot())
    }

    pub fn load_session(&self) -> Result<PersistedSession, String> {
        self.inner.load_session().map(PersistedSession::from_snapshot)
    }

    pub fn save_position(&self, position_secs: u64) -> Result<(), String> {
        self.inner.save_position(position_secs)
    }

    pub fn save_volume(&self, volume: f32) -> Result<(), String> {
        self.inner.save_volume(volume)
    }

    pub fn save_playback_mode(&self, shuffle: bool, repeat_mode: &str) -> Result<(), String> {
        self.inner.save_playback_mode(shuffle, repeat_mode)
    }

    pub fn clear_session(&self) -> Result<(), String> {
        self.inner.clear_session()
    }
}

/// Thread-safe wrapper for SessionStore
pub struct SessionStoreState {
    pub store: Arc<Mutex<Option<SessionStore>>>,
}

impl SessionStoreState {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            store: Arc::new(Mutex::new(Some(SessionStore::new()?))),
        })
    }

    /// Create an empty state (no active session store)
    pub fn new_empty() -> Self {
        Self {
            store: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize the store at a specific directory
    pub fn init_at(&self, base_dir: &Path) -> Result<(), String> {
        let store = SessionStore::new_at(base_dir)?;
        *self
            .store
            .lock()
            .map_err(|e| format!("Lock error: {}", e))? = Some(store);
        Ok(())
    }

    /// Close the store (logout)
    pub fn teardown(&self) {
        if let Ok(mut guard) = self.store.lock() {
            *guard = None;
        }
    }
}

// Tauri commands
#[tauri::command]
pub fn save_session_state(
    state: tauri::State<'_, SessionStoreState>,
    queue_tracks: Vec<PersistedQueueTrack>,
    current_index: Option<usize>,
    current_position_secs: u64,
    volume: f32,
    shuffle_enabled: bool,
    repeat_mode: String,
    was_playing: bool,
) -> Result<(), String> {
    let session = PersistedSession {
        queue_tracks,
        current_index,
        current_position_secs,
        volume,
        shuffle_enabled,
        repeat_mode,
        was_playing,
        saved_at: 0, // Will be set in save_session
        last_view: "home".to_string(),
        view_context_id: None,
        view_context_type: None,
    };

    let guard = state
        .store
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.save_session(&session)
}

#[tauri::command]
pub fn load_session_state(
    state: tauri::State<'_, SessionStoreState>,
) -> Result<PersistedSession, String> {
    let guard = state
        .store
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.load_session()
}

#[tauri::command]
pub fn save_session_volume(
    state: tauri::State<'_, SessionStoreState>,
    volume: f32,
) -> Result<(), String> {
    let guard = state
        .store
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.save_volume(volume)
}

#[tauri::command]
pub fn save_session_position(
    state: tauri::State<'_, SessionStoreState>,
    position_secs: u64,
) -> Result<(), String> {
    let guard = state
        .store
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.save_position(position_secs)
}

#[tauri::command]
pub fn save_session_playback_mode(
    state: tauri::State<'_, SessionStoreState>,
    shuffle: bool,
    repeat_mode: String,
) -> Result<(), String> {
    let guard = state
        .store
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.save_playback_mode(shuffle, &repeat_mode)
}

#[tauri::command]
pub fn clear_session(state: tauri::State<'_, SessionStoreState>) -> Result<(), String> {
    let guard = state
        .store
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let store = guard.as_ref().ok_or("No active session - please log in")?;
    store.clear_session()
}
