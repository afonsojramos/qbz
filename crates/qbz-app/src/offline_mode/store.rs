//! Per-user offline-mode settings store.
//!
//! Opens the SAME `offline_settings.db` Tauri's `src-tauri/src/offline/mod.rs`
//! uses (identical creation SQL + additive migrations), so the file stays
//! frontend-portable like `library.db`/`index.db`. This shared store exposes
//! only the subset the offline-MODE port consumes:
//!
//! - `manual_offline_mode` — the induced-offline flag (persisted; D1).
//! - `show_network_folders_in_manual_offline` — network-mount policy (D9).
//! - `pre_offline_stream_first_track` — the issue #279 snapshot of
//!   `audio_settings.stream_first_track` taken on entering induced offline.
//!
//! The legacy columns/tables (cast/scrobbling flags, `pending_playlist_sync`,
//! `scrobble_queue`, `cache_limit_bytes`) are still CREATED for byte-level
//! compatibility with the Tauri schema, but get no API here: the dead toggles
//! are not ported (spec §1) and offline playlist creation is replaced by
//! first-class local playlists (D7/D8).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The offline-mode settings the port consumes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineModeSettings {
    pub manual_offline_mode: bool,
    pub show_network_folders_in_manual_offline: bool,
}

pub struct OfflineModeStore {
    conn: Connection,
}

impl OfflineModeStore {
    /// Open (or create) `offline_settings.db` under `base_dir` — the per-user
    /// data directory, same location Tauri's `OfflineState::init_at` uses.
    pub fn new_at(base_dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(base_dir)
            .map_err(|e| format!("Failed to create data directory: {}", e))?;
        let db_path = base_dir.join("offline_settings.db");

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open offline settings database: {}", e))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("Failed to enable WAL for offline settings database: {}", e))?;

        // Base tables — kept IDENTICAL to the Tauri module so both frontends
        // can open the same per-user file.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS offline_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                manual_offline_mode INTEGER NOT NULL DEFAULT 0,
                show_partial_playlists INTEGER NOT NULL DEFAULT 1
            );
            INSERT OR IGNORE INTO offline_settings (id, manual_offline_mode, show_partial_playlists)
            VALUES (1, 0, 1);

            CREATE TABLE IF NOT EXISTS pending_playlist_sync (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                description TEXT,
                is_public INTEGER NOT NULL DEFAULT 0,
                track_ids TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                synced INTEGER NOT NULL DEFAULT 0,
                qobuz_playlist_id INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_pending_playlist_synced ON pending_playlist_sync(synced);

            CREATE TABLE IF NOT EXISTS scrobble_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                artist TEXT NOT NULL,
                track TEXT NOT NULL,
                album TEXT,
                timestamp INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                sent INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_scrobble_queue_sent ON scrobble_queue(sent);",
        )
        .map_err(|e| format!("Failed to create offline settings table: {}", e))?;

        // Additive migrations — same list as Tauri's; errors ignored because
        // the column may already exist.
        let migrations = [
            "ALTER TABLE offline_settings ADD COLUMN allow_cast_while_offline INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE offline_settings ADD COLUMN allow_immediate_scrobbling INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE offline_settings ADD COLUMN allow_accumulated_scrobbling INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE offline_settings ADD COLUMN show_network_folders_in_manual_offline INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE offline_settings ADD COLUMN pre_offline_stream_first_track INTEGER",
            "ALTER TABLE pending_playlist_sync ADD COLUMN local_track_ids TEXT",
            "ALTER TABLE pending_playlist_sync ADD COLUMN local_track_paths TEXT",
            "ALTER TABLE offline_settings ADD COLUMN cache_limit_bytes INTEGER",
        ];
        for migration in migrations {
            let _ = conn.execute(migration, []);
        }

        Ok(Self { conn })
    }

    pub fn get_settings(&self) -> Result<OfflineModeSettings, String> {
        self.conn
            .query_row(
                "SELECT manual_offline_mode,
                        COALESCE(show_network_folders_in_manual_offline, 0)
                 FROM offline_settings WHERE id = 1",
                [],
                |row| {
                    Ok(OfflineModeSettings {
                        manual_offline_mode: row.get::<_, i64>(0)? != 0,
                        show_network_folders_in_manual_offline: row.get::<_, i64>(1)? != 0,
                    })
                },
            )
            .map_err(|e| format!("Failed to get offline settings: {}", e))
    }

    pub fn set_manual_offline_mode(&self, enabled: bool) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE offline_settings SET manual_offline_mode = ?1 WHERE id = 1",
                params![enabled as i64],
            )
            .map_err(|e| format!("Failed to set manual offline mode: {}", e))?;
        Ok(())
    }

    pub fn set_show_network_folders_in_manual_offline(&self, enabled: bool) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE offline_settings SET show_network_folders_in_manual_offline = ?1 WHERE id = 1",
                params![enabled as i64],
            )
            .map_err(|e| format!("Failed to set show network folders in manual offline: {}", e))?;
        Ok(())
    }

    /// Issue #279 snapshot: the user's `stream_first_track` preference stashed
    /// when entering induced offline. `None` = no snapshot active.
    pub fn get_pre_offline_stream_first_track(&self) -> Result<Option<bool>, String> {
        self.conn
            .query_row(
                "SELECT pre_offline_stream_first_track FROM offline_settings WHERE id = 1",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map(|opt| opt.map(|v| v != 0))
            .map_err(|e| format!("Failed to read pre_offline_stream_first_track: {}", e))
    }

    /// Store (`Some`) on entering induced offline, clear (`None`) on exit.
    pub fn set_pre_offline_stream_first_track(&self, value: Option<bool>) -> Result<(), String> {
        let param: Option<i64> = value.map(|v| v as i64);
        self.conn
            .execute(
                "UPDATE offline_settings SET pre_offline_stream_first_track = ?1 WHERE id = 1",
                params![param],
            )
            .map_err(|e| format!("Failed to set pre_offline_stream_first_track: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        static NONCE: AtomicU64 = AtomicU64::new(0);
        let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("qbz-app-{name}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn defaults_are_online_and_no_network_folders() {
        let dir = unique_test_dir("offline-store-defaults");
        let store = OfflineModeStore::new_at(&dir).unwrap();

        let settings = store.get_settings().unwrap();
        assert!(!settings.manual_offline_mode);
        assert!(!settings.show_network_folders_in_manual_offline);
        assert_eq!(store.get_pre_offline_stream_first_track().unwrap(), None);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn manual_flag_round_trips() {
        let dir = unique_test_dir("offline-store-manual");
        let store = OfflineModeStore::new_at(&dir).unwrap();

        store.set_manual_offline_mode(true).unwrap();
        assert!(store.get_settings().unwrap().manual_offline_mode);
        store.set_manual_offline_mode(false).unwrap();
        assert!(!store.get_settings().unwrap().manual_offline_mode);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn network_folders_flag_round_trips() {
        let dir = unique_test_dir("offline-store-netfolders");
        let store = OfflineModeStore::new_at(&dir).unwrap();

        store.set_show_network_folders_in_manual_offline(true).unwrap();
        assert!(
            store
                .get_settings()
                .unwrap()
                .show_network_folders_in_manual_offline
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stream_first_snapshot_round_trips() {
        let dir = unique_test_dir("offline-store-snapshot");
        let store = OfflineModeStore::new_at(&dir).unwrap();

        store.set_pre_offline_stream_first_track(Some(true)).unwrap();
        assert_eq!(store.get_pre_offline_stream_first_track().unwrap(), Some(true));
        store.set_pre_offline_stream_first_track(None).unwrap();
        assert_eq!(store.get_pre_offline_stream_first_track().unwrap(), None);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reopens_tauri_era_database_without_data_loss() {
        // Simulate a DB created by the original Tauri schema (pre-migration
        // base tables only), then reopen with this store: migrations must be
        // additive and the existing flag must survive.
        let dir = unique_test_dir("offline-store-compat");
        std::fs::create_dir_all(&dir).unwrap();
        {
            let conn = Connection::open(dir.join("offline_settings.db")).unwrap();
            conn.execute_batch(
                "CREATE TABLE offline_settings (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    manual_offline_mode INTEGER NOT NULL DEFAULT 0,
                    show_partial_playlists INTEGER NOT NULL DEFAULT 1
                );
                INSERT INTO offline_settings (id, manual_offline_mode, show_partial_playlists)
                VALUES (1, 1, 1);",
            )
            .unwrap();
        }

        let store = OfflineModeStore::new_at(&dir).unwrap();
        let settings = store.get_settings().unwrap();
        assert!(settings.manual_offline_mode, "Tauri-era flag must survive");
        assert!(!settings.show_network_folders_in_manual_offline);

        let _ = std::fs::remove_dir_all(dir);
    }
}
