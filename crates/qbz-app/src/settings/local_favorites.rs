//! Headless local-favorites service.
//!
//! Frontend-agnostic store for favoriting LOCAL library items (genuine local
//! files + Plex — never the Qobuz offline cache). Mirrors `pinned_items.rs`
//! (same pragmas, error style, in-memory `(kind, id)` set) per ADR-006; the
//! per-user lifecycle lives in the `qbz` crate wrapper (`crate::local_favorites`).
//!
//! Rows carry a display snapshot (title/subtitle/artwork) taken at favorite
//! time plus a denormalized `artist` (for per-artist counts) and `source`
//! (`local` | `plex`). The `CHECK` on `source` refuses `qobuz_download` at
//! write time, so the mixed-library feed built from this store is inherently
//! free of Qobuz-offline duplicates.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::RwLock;

/// Database file name, joined onto the per-user data dir by the lifecycle layer.
pub const DB_FILE_NAME: &str = "local_favorites.db";

/// A favorited local item with its display snapshot.
///
/// Ids are Strings: album = the local group key (`plex:…` / contains `|`/`/`),
/// artist = the artist NAME (local artists have no numeric id), track =
/// `file_path` (local) or `plex:<file_path>` (Plex) — the stable key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalFavItem {
    /// "album" | "artist" | "track".
    pub kind: String,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artwork_url: String,
    /// Denormalized artist name (for per-artist counts); empty for kind="artist".
    pub artist: String,
    /// "local" | "plex" — never "qobuz_download".
    pub source: String,
    /// Unix seconds; the ordering key (newest first).
    pub favorited_at: i64,
}

/// Local-favorites service with O(1) lookup performance.
pub struct LocalFavoritesService {
    conn: Connection,
    /// In-memory `(kind, id)` set for O(1) heart lookups.
    keys: RwLock<HashSet<(String, String)>>,
}

impl LocalFavoritesService {
    /// Create a new service, opening or creating the database.
    pub fn new(db_path: &Path) -> Result<Self, String> {
        log::info!("[LocalFav] Opening database at: {}", db_path.display());

        let conn = Connection::open(db_path)
            .map_err(|e| format!("Failed to open local favorites database: {}", e))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("Failed to set WAL mode: {}", e))?;

        let service = Self {
            conn,
            keys: RwLock::new(HashSet::new()),
        };

        service.init_schema()?;
        service.load_from_db()?;

        Ok(service)
    }

    /// Create an in-memory service (test/ephemeral helper).
    pub fn new_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to open in-memory local favorites database: {}", e))?;

        let service = Self {
            conn,
            keys: RwLock::new(HashSet::new()),
        };

        service.init_schema()?;
        service.load_from_db()?;

        Ok(service)
    }

    fn init_schema(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS local_favorites (
                    kind TEXT NOT NULL CHECK (kind IN ('album','artist','track')),
                    id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    subtitle TEXT,
                    artwork_url TEXT,
                    artist TEXT,
                    source TEXT NOT NULL CHECK (source IN ('local','plex')),
                    favorited_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                    PRIMARY KEY (kind, id)
                );
                CREATE INDEX IF NOT EXISTS idx_local_favorites_at
                    ON local_favorites(favorited_at);
                CREATE INDEX IF NOT EXISTS idx_local_favorites_artist
                    ON local_favorites(kind, artist);
                "#,
            )
            .map_err(|e| format!("Failed to initialize local favorites schema: {}", e))?;

        Ok(())
    }

    fn load_from_db(&self) -> Result<(), String> {
        let mut stmt = self
            .conn
            .prepare("SELECT kind, id FROM local_favorites")
            .map_err(|e| format!("Failed to prepare local favorites query: {}", e))?;

        let keys: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Failed to query local favorites: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        let count = keys.len();
        let mut set = self
            .keys
            .write()
            .map_err(|_| "Failed to acquire write lock")?;
        *set = keys.into_iter().collect();

        log::info!("[LocalFav] Loaded {} local favorites into memory", count);
        Ok(())
    }

    /// Check if a local item is favorited — O(1).
    #[inline]
    pub fn is_favorite(&self, kind: &str, id: &str) -> bool {
        self.keys
            .read()
            .map(|set| set.contains(&(kind.to_string(), id.to_string())))
            .unwrap_or(false)
    }

    /// Favorite an item (upsert). `favorited_at` is stamped now.
    pub fn favorite(&self, item: &LocalFavItem) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.conn
            .execute(
                "INSERT OR REPLACE INTO local_favorites
                 (kind, id, title, subtitle, artwork_url, artist, source, favorited_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    item.kind,
                    item.id,
                    item.title,
                    item.subtitle,
                    item.artwork_url,
                    item.artist,
                    item.source,
                    now
                ],
            )
            .map_err(|e| format!("Failed to favorite item: {}", e))?;

        if let Ok(mut set) = self.keys.write() {
            set.insert((item.kind.clone(), item.id.clone()));
        }
        Ok(())
    }

    /// Unfavorite an item. Absent rows are Ok, not an error.
    pub fn unfavorite(&self, kind: &str, id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM local_favorites WHERE kind = ?1 AND id = ?2",
                params![kind, id],
            )
            .map_err(|e| format!("Failed to unfavorite item: {}", e))?;

        if let Ok(mut set) = self.keys.write() {
            set.remove(&(kind.to_string(), id.to_string()));
        }
        Ok(())
    }

    /// All favorites, newest first.
    pub fn list(&self) -> Result<Vec<LocalFavItem>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT kind, id, title, subtitle, artwork_url, artist, source, favorited_at
                 FROM local_favorites
                 ORDER BY favorited_at DESC",
            )
            .map_err(|e| format!("Failed to prepare local favorites query: {}", e))?;

        let items = stmt
            .query_map([], |row| {
                Ok(LocalFavItem {
                    kind: row.get(0)?,
                    id: row.get(1)?,
                    title: row.get(2)?,
                    subtitle: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    artwork_url: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    artist: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                    source: row.get(6)?,
                    favorited_at: row.get(7)?,
                })
            })
            .map_err(|e| format!("Failed to query local favorites: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    /// Per-artist favorite counts (album + track kinds carry an artist).
    pub fn count_by_artist(&self) -> Result<Vec<(String, i64)>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT artist, COUNT(*) FROM local_favorites
                 WHERE artist IS NOT NULL AND artist != ''
                 GROUP BY artist ORDER BY COUNT(*) DESC",
            )
            .map_err(|e| format!("Failed to prepare count-by-artist query: {}", e))?;

        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Failed to query count-by-artist: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(rows)
    }

    /// Count of favorites.
    pub fn count(&self) -> usize {
        self.keys.read().map(|set| set.len()).unwrap_or(0)
    }

    /// Snapshot of the in-memory `(kind, id)` set, for bulk card stamping.
    pub fn keys_snapshot(&self) -> HashSet<(String, String)> {
        self.keys.read().map(|set| set.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, id: &str, title: &str, artist: &str, source: &str) -> LocalFavItem {
        LocalFavItem {
            kind: kind.to_string(),
            id: id.to_string(),
            title: title.to_string(),
            subtitle: String::new(),
            artwork_url: String::new(),
            artist: artist.to_string(),
            source: source.to_string(),
            favorited_at: 0,
        }
    }

    #[test]
    fn lifecycle() {
        let s = LocalFavoritesService::new_in_memory().expect("svc");
        assert!(!s.is_favorite("album", "plex:abc"));
        assert_eq!(s.count(), 0);

        s.favorite(&item("album", "plex:abc", "A", "Artist X", "plex"))
            .unwrap();
        assert!(s.is_favorite("album", "plex:abc"));
        assert!(!s.is_favorite("track", "plex:abc"));

        s.favorite(&item("track", "/music/x.flac", "T", "Artist X", "local"))
            .unwrap();
        assert_eq!(s.count(), 2);
        let by_artist = s.count_by_artist().unwrap();
        assert_eq!(by_artist[0], ("Artist X".to_string(), 2));

        let all = s.list().unwrap();
        assert_eq!(all.len(), 2);

        s.unfavorite("album", "plex:abc").unwrap();
        assert!(!s.is_favorite("album", "plex:abc"));
        assert_eq!(s.count(), 1);
        s.unfavorite("album", "nope").unwrap();
    }

    #[test]
    fn source_check_rejects_offline() {
        let s = LocalFavoritesService::new_in_memory().expect("svc");
        assert!(
            s.favorite(&item("album", "x", "X", "A", "qobuz_download"))
                .is_err(),
            "the source CHECK refuses qobuz-offline rows"
        );
    }
}
