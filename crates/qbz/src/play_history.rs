//! Local play history — the source of truth for the discovery
//! filter "skip artists I already know".
//!
//! Mirrors the minimum-needed shape of `src-tauri/src/reco_store` for
//! the artist-network sidebar: a `play_events` table that grows one
//! row per track-start, and a side `artist_names` table that maps
//! id -> name (updated on each play). The discovery pipeline reads
//! both at once and turns them into the (qobuz_ids, normalized_names)
//! pair that filters MB candidates and validated Qobuz matches.
//!
//! SQLite is opened lazily once, and every read/write swallows errors
//! into a `log::warn!`. A fresh user (no DB yet) yields empty sets,
//! which simply means no exclusion is applied — same default Tauri
//! lands on a first-run profile.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use qbz_core::normalize_artist_name;

static DB: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();

fn db_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("play_history.db"))
}

fn open_db() -> Option<Connection> {
    let path = db_path()?;
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("[qbz-slint] play_history dir create failed: {e}");
            return None;
        }
    }
    let conn = match Connection::open(&path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[qbz-slint] play_history open failed: {e}");
            return None;
        }
    };
    // ADR-002: WAL mode for any SQLite store touched off the UI thread.
    if let Err(e) =
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
    {
        log::warn!("[qbz-slint] play_history pragma failed: {e}");
    }
    if let Err(e) = conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS play_events (
            artist_id INTEGER NOT NULL,
            occurred_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS play_events_artist
            ON play_events(artist_id);

        CREATE TABLE IF NOT EXISTS artist_names (
            artist_id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        "#,
    ) {
        log::warn!("[qbz-slint] play_history schema failed: {e}");
        return None;
    }
    Some(conn)
}

fn with_db<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&Connection) -> Option<T>,
{
    let cell = DB.get_or_init(|| Mutex::new(open_db()));
    let guard = cell.lock().ok()?;
    let conn = guard.as_ref()?;
    f(conn)
}

/// Record a play. Called when a track starts audible playback so the
/// per-artist count converges on the user's listening reality.
#[allow(dead_code)] // wired by playback::record_recent
pub fn record_play(artist_id: u64, artist_name: &str) {
    if artist_id == 0 || artist_name.is_empty() {
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    with_db(|conn| {
        if let Err(e) = conn.execute(
            "INSERT INTO play_events (artist_id, occurred_at) VALUES (?, ?)",
            params![artist_id as i64, now],
        ) {
            log::warn!("[qbz-slint] play_history insert event failed: {e}");
        }
        if let Err(e) = conn.execute(
            r#"
            INSERT INTO artist_names (artist_id, name, updated_at)
            VALUES (?, ?, ?)
            ON CONFLICT(artist_id) DO UPDATE SET
                name = excluded.name,
                updated_at = excluded.updated_at
            "#,
            params![artist_id as i64, artist_name, now],
        ) {
            log::warn!("[qbz-slint] play_history upsert name failed: {e}");
        }
        Some(())
    });
}

/// Known artists with strictly more than `threshold` plays. Returns
/// the Qobuz id set (for filtering validated Qobuz results) and the
/// normalized-name set (for filtering MB candidates). Same two-axis
/// filter the Tauri discovery pipeline applies.
#[allow(dead_code)] // wired by artist::load_mb_discovery
pub fn known_artists(threshold: u32) -> (HashSet<u64>, HashSet<String>) {
    let pair = with_db(|conn| {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT a.artist_id, a.name
                FROM artist_names a
                JOIN (
                    SELECT artist_id, COUNT(*) AS play_count
                    FROM play_events
                    GROUP BY artist_id
                    HAVING play_count > ?
                ) p ON p.artist_id = a.artist_id
                "#,
            )
            .ok()?;
        let rows = stmt
            .query_map(params![threshold], |row| -> rusqlite::Result<(u64, String)> {
                let id: i64 = row.get(0)?;
                let name: String = row.get(1)?;
                Ok((id as u64, name))
            })
            .ok()?;
        let mut ids: HashSet<u64> = HashSet::new();
        let mut names: HashSet<String> = HashSet::new();
        for row in rows.flatten() {
            ids.insert(row.0);
            names.insert(normalize_artist_name(&row.1));
        }
        Some((ids, names))
    });
    pair.unwrap_or_default()
}
