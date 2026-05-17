//! Download settings persistence.
//!
//! This module stores portable download preferences only. Host filesystem
//! validation, permission probes, and download/cache behavior stay outside
//! `qbz-app`.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSettings {
    pub download_root: String,
    pub show_in_library: bool,
}

impl Default for DownloadSettings {
    fn default() -> Self {
        let default_root = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("qbz")
            .join("audio")
            .to_string_lossy()
            .to_string();

        Self {
            download_root: default_root,
            // New installs surface offline-cached Qobuz tracks in the
            // Local Library by default. Existing users with a persisted value
            // are not affected because INSERT OR IGNORE only seeds first run.
            show_in_library: true,
        }
    }
}

pub struct DownloadSettingsStore {
    conn: Connection,
}

impl DownloadSettingsStore {
    fn open_at(dir: &Path, db_name: &str) -> Result<Self, String> {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create data directory: {}", e))?;

        let db_path = dir.join(db_name);
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open download settings database: {}", e))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("Failed to enable WAL for download settings database: {}", e))?;

        let default_settings = DownloadSettings::default();

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS download_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                download_root TEXT NOT NULL,
                show_in_library INTEGER NOT NULL DEFAULT 1
            );",
        )
        .map_err(|e| format!("Failed to create download settings table: {}", e))?;

        conn.execute(
            "INSERT OR IGNORE INTO download_settings (id, download_root, show_in_library)
             VALUES (1, ?1, ?2)",
            params![
                default_settings.download_root,
                default_settings.show_in_library as i64,
            ],
        )
        .map_err(|e| format!("Failed to initialize download settings: {}", e))?;

        Ok(Self { conn })
    }

    pub fn new() -> Result<Self, String> {
        let data_dir = dirs::data_dir()
            .ok_or("Could not determine data directory")?
            .join("qbz");
        Self::open_at(&data_dir, "download_settings.db")
    }

    pub fn new_at(base_dir: &Path) -> Result<Self, String> {
        Self::open_at(base_dir, "download_settings.db")
    }

    pub fn get_settings(&self) -> Result<DownloadSettings, String> {
        self.conn
            .query_row(
                "SELECT download_root, show_in_library FROM download_settings WHERE id = 1",
                [],
                |row| {
                    Ok(DownloadSettings {
                        download_root: row.get(0)?,
                        show_in_library: row.get::<_, i64>(1)? != 0,
                    })
                },
            )
            .map_err(|e| format!("Failed to get download settings: {}", e))
    }

    pub fn set_download_root(&self, path: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE download_settings SET download_root = ?1 WHERE id = 1",
                params![path],
            )
            .map_err(|e| format!("Failed to set download root: {}", e))?;
        Ok(())
    }

    pub fn set_show_in_library(&self, show: bool) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE download_settings SET show_in_library = ?1 WHERE id = 1",
                params![show as i64],
            )
            .map_err(|e| format!("Failed to set show_in_library: {}", e))?;
        Ok(())
    }
}

pub type DownloadSettingsState = Arc<Mutex<Option<DownloadSettingsStore>>>;

pub fn create_download_settings_state() -> Result<DownloadSettingsState, String> {
    let store = DownloadSettingsStore::new()?;
    Ok(Arc::new(Mutex::new(Some(store))))
}

pub fn create_empty_download_settings_state() -> DownloadSettingsState {
    Arc::new(Mutex::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("qbz-app-{name}-{}-{nonce}", std::process::id()))
    }

    fn fresh_store(name: &str) -> (std::path::PathBuf, DownloadSettingsStore) {
        let dir = unique_test_dir(name);
        let store = DownloadSettingsStore::new_at(&dir).expect("open store in temp dir");
        (dir, store)
    }

    #[test]
    fn download_settings_default_values_are_stable() {
        let settings = DownloadSettings::default();

        assert!(settings.download_root.ends_with("qbz/audio"));
        assert!(settings.show_in_library);
    }

    #[test]
    fn download_settings_store_returns_defaults() {
        let (dir, store) = fresh_store("download-default");

        let settings = store.get_settings().expect("get settings");

        assert!(settings.download_root.ends_with("qbz/audio"));
        assert!(settings.show_in_library);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn download_settings_persist_download_root() {
        let dir = unique_test_dir("download-root");
        {
            let store = DownloadSettingsStore::new_at(&dir).expect("open store");
            store
                .set_download_root("/tmp/qbz-downloads")
                .expect("set download root");
        }

        let reopened = DownloadSettingsStore::new_at(&dir).expect("reopen store");
        let settings = reopened.get_settings().expect("get settings");

        assert_eq!(settings.download_root, "/tmp/qbz-downloads");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn download_settings_persist_show_in_library() {
        let dir = unique_test_dir("download-library");
        {
            let store = DownloadSettingsStore::new_at(&dir).expect("open store");
            store
                .set_show_in_library(false)
                .expect("set show in library");
        }

        let reopened = DownloadSettingsStore::new_at(&dir).expect("reopen store");
        let settings = reopened.get_settings().expect("get settings");

        assert!(!settings.show_in_library);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn download_settings_reopen_does_not_overwrite_existing_row() {
        let dir = unique_test_dir("download-no-overwrite");
        {
            let store = DownloadSettingsStore::new_at(&dir).expect("open store");
            store
                .set_download_root("/tmp/custom-downloads")
                .expect("set download root");
            store
                .set_show_in_library(false)
                .expect("set show in library");
        }

        let reopened = DownloadSettingsStore::new_at(&dir).expect("reopen store");
        let settings = reopened.get_settings().expect("get settings");

        assert_eq!(settings.download_root, "/tmp/custom-downloads");
        assert!(!settings.show_in_library);
        let _ = std::fs::remove_dir_all(dir);
    }
}
