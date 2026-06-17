//! Headless artist blacklist service.
//!
//! Frontend-agnostic 1:1 port of the Tauri `BlacklistService`
//! (`src-tauri/src/artist_blacklist/service.rs` + `models.rs`). No
//! `tauri::State`, per ADR-006 and the V2 "move logic to a core crate, never
//! wrap legacy" rule. The DB filename, schema, and pragmas are kept IDENTICAL
//! to the Tauri store so existing users' `artist_blacklist.db` keeps working.
//!
//! Provides O(1) artist blacklist checks via an in-memory `HashSet` backed by
//! SQLite persistence, plus a global enable/disable feature flag.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

/// Database file name for the artist blacklist store.
///
/// Kept identical to the Tauri store so the later lifecycle layer opens the
/// same per-user database.
pub const DB_FILE_NAME: &str = "artist_blacklist.db";

/// A blacklisted artist entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistedArtist {
    pub artist_id: u64,
    pub artist_name: String,
    pub added_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Blacklist settings (enable/disable toggle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistSettings {
    pub enabled: bool,
}

impl Default for BlacklistSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Artist blacklist service with O(1) lookup performance.
pub struct BlacklistService {
    conn: Connection,
    /// In-memory set for O(1) lookups.
    blacklisted_ids: RwLock<HashSet<u64>>,
    /// Feature flag - when false, `is_blacklisted()` always returns false.
    enabled: AtomicBool,
}

impl BlacklistService {
    /// Create a new blacklist service, opening or creating the database.
    pub fn new(db_path: &Path) -> Result<Self, String> {
        log::info!("[Blacklist] Opening database at: {}", db_path.display());

        let conn = Connection::open(db_path)
            .map_err(|e| format!("Failed to open blacklist database: {}", e))?;

        // Enable WAL mode for better concurrent access (ADR-002).
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("Failed to set WAL mode: {}", e))?;

        let service = Self {
            conn,
            blacklisted_ids: RwLock::new(HashSet::new()),
            enabled: AtomicBool::new(true),
        };

        service.init_schema()?;
        service.load_from_db()?;
        service.load_settings()?;

        Ok(service)
    }

    /// Create an in-memory blacklist service (test/ephemeral helper).
    ///
    /// Opens a `:memory:` connection and runs schema init + loads, but does not
    /// set WAL mode (not needed for an in-memory database).
    pub fn new_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to open in-memory blacklist database: {}", e))?;

        let service = Self {
            conn,
            blacklisted_ids: RwLock::new(HashSet::new()),
            enabled: AtomicBool::new(true),
        };

        service.init_schema()?;
        service.load_from_db()?;
        service.load_settings()?;

        Ok(service)
    }

    /// Initialize database schema.
    fn init_schema(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                r#"
                -- Artist blacklist entries
                CREATE TABLE IF NOT EXISTS artist_blacklist (
                    artist_id INTEGER PRIMARY KEY,
                    artist_name TEXT NOT NULL,
                    added_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                    notes TEXT
                );

                -- Index for name search in UI
                CREATE INDEX IF NOT EXISTS idx_artist_blacklist_name
                    ON artist_blacklist(artist_name COLLATE NOCASE);

                -- Settings table (single row)
                CREATE TABLE IF NOT EXISTS blacklist_settings (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    enabled INTEGER NOT NULL DEFAULT 1
                );

                -- Insert default settings if not present
                INSERT OR IGNORE INTO blacklist_settings (id, enabled) VALUES (1, 1);
                "#,
            )
            .map_err(|e| format!("Failed to initialize blacklist schema: {}", e))?;

        Ok(())
    }

    /// Load all blacklisted IDs from database into memory.
    fn load_from_db(&self) -> Result<(), String> {
        let mut stmt = self
            .conn
            .prepare("SELECT artist_id FROM artist_blacklist")
            .map_err(|e| format!("Failed to prepare blacklist query: {}", e))?;

        let ids: Vec<u64> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("Failed to query blacklist: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        let count = ids.len();
        let mut set = self
            .blacklisted_ids
            .write()
            .map_err(|_| "Failed to acquire write lock")?;
        *set = ids.into_iter().collect();

        log::info!(
            "[Blacklist] Loaded {} blacklisted artists into memory",
            count
        );
        Ok(())
    }

    /// Load enabled setting from database.
    fn load_settings(&self) -> Result<(), String> {
        let enabled: bool = self
            .conn
            .query_row(
                "SELECT enabled FROM blacklist_settings WHERE id = 1",
                [],
                |row| {
                    let val: i32 = row.get(0)?;
                    Ok(val != 0)
                },
            )
            .map_err(|e| format!("Failed to load blacklist settings: {}", e))?;

        self.enabled.store(enabled, Ordering::Relaxed);
        log::info!("[Blacklist] Feature enabled: {}", enabled);
        Ok(())
    }

    /// Check if an artist is blacklisted - O(1) operation.
    ///
    /// Returns false if the feature is disabled.
    #[inline]
    pub fn is_blacklisted(&self, artist_id: u64) -> bool {
        // Fast path: if feature is disabled, always return false.
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }

        // O(1) HashSet lookup.
        self.blacklisted_ids
            .read()
            .map(|set| set.contains(&artist_id))
            .unwrap_or(false)
    }

    /// Add an artist to the blacklist.
    pub fn add(
        &self,
        artist_id: u64,
        artist_name: &str,
        notes: Option<&str>,
    ) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.conn
            .execute(
                "INSERT OR REPLACE INTO artist_blacklist (artist_id, artist_name, added_at, notes)
                 VALUES (?1, ?2, ?3, ?4)",
                params![artist_id as i64, artist_name, now, notes],
            )
            .map_err(|e| format!("Failed to add artist to blacklist: {}", e))?;

        // Update in-memory set.
        if let Ok(mut set) = self.blacklisted_ids.write() {
            set.insert(artist_id);
        }

        log::info!(
            "[Blacklist] Added artist: {} (id={})",
            artist_name,
            artist_id
        );
        Ok(())
    }

    /// Remove an artist from the blacklist.
    pub fn remove(&self, artist_id: u64) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM artist_blacklist WHERE artist_id = ?1",
                params![artist_id as i64],
            )
            .map_err(|e| format!("Failed to remove artist from blacklist: {}", e))?;

        // Update in-memory set.
        if let Ok(mut set) = self.blacklisted_ids.write() {
            set.remove(&artist_id);
        }

        log::info!("[Blacklist] Removed artist id={}", artist_id);
        Ok(())
    }

    /// Get all blacklisted artists.
    pub fn get_all(&self) -> Result<Vec<BlacklistedArtist>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT artist_id, artist_name, added_at, notes
                 FROM artist_blacklist
                 ORDER BY artist_name COLLATE NOCASE",
            )
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let artists = stmt
            .query_map([], |row| {
                Ok(BlacklistedArtist {
                    artist_id: row.get::<_, i64>(0)? as u64,
                    artist_name: row.get(1)?,
                    added_at: row.get(2)?,
                    notes: row.get(3)?,
                })
            })
            .map_err(|e| format!("Failed to query blacklist: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(artists)
    }

    /// Get count of blacklisted artists.
    ///
    /// Does not respect the enabled flag.
    pub fn count(&self) -> usize {
        self.blacklisted_ids
            .read()
            .map(|set| set.len())
            .unwrap_or(0)
    }

    /// Set the enabled state.
    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE blacklist_settings SET enabled = ?1 WHERE id = 1",
                params![if enabled { 1 } else { 0 }],
            )
            .map_err(|e| format!("Failed to update enabled setting: {}", e))?;

        self.enabled.store(enabled, Ordering::Relaxed);
        log::info!("[Blacklist] Feature enabled set to: {}", enabled);
        Ok(())
    }

    /// Check if the feature is enabled.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Get current settings.
    pub fn get_settings(&self) -> BlacklistSettings {
        BlacklistSettings {
            enabled: self.is_enabled(),
        }
    }

    /// Clear all blacklisted artists.
    ///
    /// Does not touch the settings row.
    pub fn clear_all(&self) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM artist_blacklist", [])
            .map_err(|e| format!("Failed to clear blacklist: {}", e))?;

        if let Ok(mut set) = self.blacklisted_ids.write() {
            set.clear();
        }

        log::info!("[Blacklist] Cleared all entries");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc() -> BlacklistService {
        BlacklistService::new_in_memory().expect("svc")
    }

    #[test]
    fn add_and_check() {
        let s = svc();
        s.add(123, "Test Artist", None).unwrap();
        assert!(s.is_blacklisted(123));
        assert!(!s.is_blacklisted(456));
    }

    #[test]
    fn remove_is_not_error_when_absent() {
        let s = svc();
        s.add(1, "A", None).unwrap();
        s.remove(1).unwrap();
        assert!(!s.is_blacklisted(1));
        s.remove(999).unwrap(); // absent -> Ok, not error
    }

    #[test]
    fn disabled_short_circuits_even_with_row() {
        let s = svc();
        s.add(1, "A", None).unwrap();
        s.set_enabled(false).unwrap();
        assert!(!s.is_blacklisted(1)); // disabled => false even though row exists
        assert_eq!(s.count(), 1); // count ignores the enabled flag
        s.set_enabled(true).unwrap();
        assert!(s.is_blacklisted(1)); // re-enable restores instantly
    }

    #[test]
    fn get_all_sorted_by_name_nocase_with_notes_roundtrip() {
        let s = svc();
        s.add(2, "zeta", Some("note-z".into())).unwrap();
        s.add(1, "Alpha", None).unwrap();
        let all = s.get_all().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].artist_name, "Alpha"); // case-insensitive asc
        assert_eq!(all[1].artist_name, "zeta");
        assert_eq!(all[1].notes.as_deref(), Some("note-z"));
        assert_eq!(all[0].notes, None);
    }

    #[test]
    fn upsert_replaces_name_and_notes() {
        let s = svc();
        s.add(5, "Old", Some("n".into())).unwrap();
        s.add(5, "New", None).unwrap(); // INSERT OR REPLACE
        let all = s.get_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].artist_name, "New");
        assert_eq!(all[0].notes, None);
    }

    #[test]
    fn clear_all_keeps_settings() {
        let s = svc();
        s.add(1, "A", None).unwrap();
        s.set_enabled(false).unwrap();
        s.clear_all().unwrap();
        assert_eq!(s.count(), 0);
        assert!(!s.is_enabled()); // clear_all does NOT touch enabled
    }
}
