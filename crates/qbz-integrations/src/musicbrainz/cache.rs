//! MusicBrainz cache for resolved entities and settings
//!
//! SQLite-based cache with TTL expiration for MusicBrainz lookups.
//! Also persists integration settings (enabled state).

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::identity::{
    ArtistCandidate, EvidenceKind, IsrcCredits, SceneIdentityRow, SourceIdentityRow,
    MATCHER_VERSION,
};
use super::models::{
    ArtistMetadata, ArtistRelationships, ArtistType, LocationDiscoveryResponse, MatchConfidence,
    ResolvedArtist, ResolvedTrack,
};

/// TTL for recording cache (30 days)
const RECORDING_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// TTL for artist cache (7 days)
const ARTIST_TTL_SECS: i64 = 7 * 24 * 60 * 60;
/// TTL for release cache (30 days)
const RELEASE_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// TTL for artist relationships cache (7 days)
const RELATIONS_TTL_SECS: i64 = 7 * 24 * 60 * 60;
/// TTL for artist metadata cache (30 days)
const METADATA_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// TTL for scene discovery cache (30 days)
const SCENE_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// TTL for Qobuz artist validation cache (30 days)
const QOBUZ_VALIDATION_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// TTL for the typed Qobuz artist-match cache (30 days, same as above)
const QOBUZ_ARTIST_MATCH_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// TTL of a cached exact-name search WINDOW (evidence, re-gated on every hit)
const ARTIST_QUERY_TTL_SECS: i64 = 7 * 24 * 60 * 60;
/// TTL of a cached ISRC → recording-credits set
const ISRC_CREDITS_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// TTL of an ISRC-verified identity row (source or scene)
const VERIFIED_IDENTITY_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// TTL of a provisional (unique-exact-name) scene identity row — a name alone
/// is not identity, so it may not outlive a week
const PROVISIONAL_IDENTITY_TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// The Qobuz half of a scene validation, and NOTHING else.
///
/// This exists because the old `mb_qobuz_validation` table stores a whole
/// `LocationCandidate` — MBID, affinity score and genres included — keyed on
/// the normalised artist NAME alone. Tauri only ever wrote it; reading it back
/// would transplant one scene's identity and ranking onto another scene's
/// candidate with the same name. Only these four fields are a pure function of
/// the name (they are literally what the Qobuz artist search returns), so only
/// these four are safe to replay. The caller rebuilds the candidate around them
/// from the CURRENT mbid, score and genres.
///
/// Negative results are deliberately NOT cached: Tauri disabled its negative
/// cache (`// Negative cache — TEMPORARILY DISABLED`) and a "not on Qobuz"
/// verdict that sticks for 30 days is how a newly-added artist stays invisible.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QobuzArtistMatch {
    pub qobuz_id: i64,
    pub name: String,
    pub image: Option<String>,
    pub albums_count: Option<u32>,
}

/// Cache statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub recordings: u64,
    pub artists: u64,
    pub releases: u64,
    pub relations: u64,
}

/// MusicBrainz cache
pub struct MusicBrainzCache {
    conn: Connection,
}

impl MusicBrainzCache {
    /// Create a new cache at the given path
    pub fn new(db_path: &Path) -> Result<Self, String> {
        let conn = Connection::open(db_path)
            .map_err(|e| format!("Failed to open MusicBrainz cache: {}", e))?;

        // Enable WAL mode for concurrent read/write (ADR-002)
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("Failed to enable WAL mode: {}", e))?;

        let cache = Self { conn };
        cache.init_schema()?;

        Ok(cache)
    }

    /// An in-memory cache with the full schema — tests and dry runs.
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to open in-memory MusicBrainz cache: {}", e))?;
        let cache = Self { conn };
        cache.init_schema()?;
        Ok(cache)
    }

    fn init_schema(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                "
                -- Settings (enabled state, etc.)
                CREATE TABLE IF NOT EXISTS mb_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                -- Recordings indexed by ISRC
                CREATE TABLE IF NOT EXISTS mb_recordings (
                    isrc TEXT PRIMARY KEY,
                    data TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_mb_recordings_fetched ON mb_recordings(fetched_at);

                -- Artists indexed by normalized name
                CREATE TABLE IF NOT EXISTS mb_artists (
                    name_normalized TEXT PRIMARY KEY,
                    data TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_mb_artists_fetched ON mb_artists(fetched_at);

                -- Releases indexed by UPC/barcode
                CREATE TABLE IF NOT EXISTS mb_releases (
                    barcode TEXT PRIMARY KEY,
                    data TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_mb_releases_fetched ON mb_releases(fetched_at);

                -- Artist relationships indexed by MBID
                CREATE TABLE IF NOT EXISTS mb_artist_relations (
                    mbid TEXT PRIMARY KEY,
                    data TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_mb_relations_fetched ON mb_artist_relations(fetched_at);

                -- Artist metadata (location, genres, life span) indexed by MBID
                CREATE TABLE IF NOT EXISTS mb_artist_metadata (
                    mbid TEXT PRIMARY KEY,
                    data TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_mb_metadata_fetched ON mb_artist_metadata(fetched_at);

                -- Scene discovery results indexed by area + seed hash
                CREATE TABLE IF NOT EXISTS mb_scene_cache (
                    cache_key TEXT PRIMARY KEY,
                    data TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_mb_scene_fetched ON mb_scene_cache(fetched_at);

                -- Qobuz artist validation cache
                CREATE TABLE IF NOT EXISTS mb_qobuz_validation (
                    name_normalized TEXT PRIMARY KEY,
                    data TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_mb_qobuz_validation_fetched ON mb_qobuz_validation(fetched_at);

                -- Typed Qobuz artist match (id/name/image/album count only),
                -- keyed on the normalized artist name. Separate table from
                -- mb_qobuz_validation because that one holds Tauri-era whole
                -- LocationCandidate rows that must never be replayed.
                CREATE TABLE IF NOT EXISTS mb_qobuz_artist_match (
                    name_normalized TEXT PRIMARY KEY,
                    data TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_mb_qobuz_artist_match_fetched ON mb_qobuz_artist_match(fetched_at);

                -- V2 resolved tracks (simple cache)
                CREATE TABLE IF NOT EXISTS resolved_tracks (
                    isrc TEXT PRIMARY KEY,
                    recording_mbid TEXT NOT NULL,
                    title TEXT NOT NULL,
                    artist_mbids TEXT NOT NULL,
                    release_mbid TEXT,
                    isrcs TEXT NOT NULL,
                    confidence TEXT NOT NULL,
                    cached_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
                );

                -- V2 resolved artists (simple cache)
                CREATE TABLE IF NOT EXISTS resolved_artists (
                    name_lower TEXT PRIMARY KEY,
                    mbid TEXT NOT NULL,
                    name TEXT NOT NULL,
                    sort_name TEXT,
                    artist_type TEXT NOT NULL,
                    country TEXT,
                    disambiguation TEXT,
                    confidence TEXT NOT NULL,
                    cached_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
                );

                -- #768 identity matcher (identity.rs). Versioned, identifier-
                -- keyed, provenance-carrying; TTL is applied at READ time.
                -- Old name-keyed rows (resolved_artists, mb_qobuz_artist_match)
                -- stay on disk, inert: nothing on the corrected path reads them.
                CREATE TABLE IF NOT EXISTS mb_artist_query_v2 (
                    query_key TEXT PRIMARY KEY,
                    data TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS mb_isrc_credits_v2 (
                    isrc TEXT PRIMARY KEY,
                    data TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS mb_source_identity_v2 (
                    qobuz_artist_id INTEGER NOT NULL,
                    matcher_version INTEGER NOT NULL,
                    mbid TEXT NOT NULL,
                    evidence_kind TEXT NOT NULL,
                    evidence_count INTEGER NOT NULL,
                    source_name TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL,
                    PRIMARY KEY (qobuz_artist_id, matcher_version)
                );
                CREATE TABLE IF NOT EXISTS mb_scene_identity_v2 (
                    mbid TEXT NOT NULL,
                    scope TEXT NOT NULL,
                    matcher_version INTEGER NOT NULL,
                    qobuz_artist_id INTEGER NOT NULL,
                    evidence_kind TEXT NOT NULL,
                    evidence_count INTEGER NOT NULL,
                    qobuz_name TEXT NOT NULL,
                    image TEXT,
                    albums_count INTEGER,
                    fetched_at INTEGER NOT NULL,
                    PRIMARY KEY (mbid, scope, matcher_version)
                );

                CREATE TABLE IF NOT EXISTS cache_stats (
                    key TEXT PRIMARY KEY,
                    value INTEGER NOT NULL DEFAULT 0
                );
                INSERT OR IGNORE INTO cache_stats (key, value) VALUES ('hits', 0);
                INSERT OR IGNORE INTO cache_stats (key, value) VALUES ('misses', 0);
            ",
            )
            .map_err(|e| format!("Failed to init MusicBrainz schema: {}", e))
    }

    fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Normalize artist name for consistent cache keys
    pub fn normalize_name(name: &str) -> String {
        name.to_lowercase()
            .trim()
            .replace(['\'', '"', '.', ','], "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    // ============ Settings ============

    /// Check if MusicBrainz is enabled
    pub fn is_enabled(&self) -> Result<bool, String> {
        let result: rusqlite::Result<String> = self.conn.query_row(
            "SELECT value FROM mb_settings WHERE key = 'enabled'",
            [],
            |row| row.get(0),
        );
        match result {
            Ok(val) => Ok(val != "0"),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true), // Default enabled
            Err(e) => Err(format!("Failed to get enabled state: {}", e)),
        }
    }

    /// Set enabled state
    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let value = if enabled { "1" } else { "0" };
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_settings (key, value) VALUES ('enabled', ?)",
                [value],
            )
            .map_err(|e| format!("Failed to set enabled: {}", e))?;
        Ok(())
    }

    // ============ Recording Cache (JSON-serialized) ============

    /// Get cached recording by ISRC (legacy format)
    pub fn get_recording(&self, isrc: &str) -> Result<Option<serde_json::Value>, String> {
        let min_fetched_at = Self::current_timestamp() - RECORDING_TTL_SECS;
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM mb_recordings WHERE isrc = ? AND fetched_at > ?",
                params![isrc, min_fetched_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to query recording cache: {}", e))?;

        if let Some(data) = result {
            serde_json::from_str(&data)
                .map(Some)
                .map_err(|e| format!("Failed to parse cached recording: {}", e))
        } else {
            Ok(None)
        }
    }

    /// Cache a recording (JSON-serialized)
    pub fn set_recording<T: serde::Serialize>(&self, isrc: &str, data: &T) -> Result<(), String> {
        let fetched_at = Self::current_timestamp();
        let json = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize recording: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_recordings (isrc, data, fetched_at) VALUES (?, ?, ?)",
                params![isrc, json, fetched_at],
            )
            .map_err(|e| format!("Failed to cache recording: {}", e))?;
        Ok(())
    }

    // ============ Artist Cache (JSON-serialized) ============

    /// Get cached artist by name (JSON-serialized)
    pub fn get_artist_by_name<T: serde::de::DeserializeOwned>(
        &self,
        name: &str,
    ) -> Result<Option<T>, String> {
        let normalized = Self::normalize_name(name);
        let min_fetched_at = Self::current_timestamp() - ARTIST_TTL_SECS;
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM mb_artists WHERE name_normalized = ? AND fetched_at > ?",
                params![normalized, min_fetched_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to query artist cache: {}", e))?;

        if let Some(data) = result {
            serde_json::from_str(&data)
                .map(Some)
                .map_err(|e| format!("Failed to parse cached artist: {}", e))
        } else {
            Ok(None)
        }
    }

    /// Cache an artist (JSON-serialized)
    pub fn set_artist_by_name<T: serde::Serialize>(
        &self,
        name: &str,
        data: &T,
    ) -> Result<(), String> {
        let normalized = Self::normalize_name(name);
        let fetched_at = Self::current_timestamp();
        let json = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize artist: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_artists (name_normalized, data, fetched_at) VALUES (?, ?, ?)",
                params![normalized, json, fetched_at],
            )
            .map_err(|e| format!("Failed to cache artist: {}", e))?;
        Ok(())
    }

    // ============ Release Cache ============

    /// Get cached release by barcode
    pub fn get_release<T: serde::de::DeserializeOwned>(
        &self,
        barcode: &str,
    ) -> Result<Option<T>, String> {
        let min_fetched_at = Self::current_timestamp() - RELEASE_TTL_SECS;
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM mb_releases WHERE barcode = ? AND fetched_at > ?",
                params![barcode, min_fetched_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to query release cache: {}", e))?;

        if let Some(data) = result {
            serde_json::from_str(&data)
                .map(Some)
                .map_err(|e| format!("Failed to parse cached release: {}", e))
        } else {
            Ok(None)
        }
    }

    /// Cache a release
    pub fn set_release<T: serde::Serialize>(&self, barcode: &str, data: &T) -> Result<(), String> {
        let fetched_at = Self::current_timestamp();
        let json = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize release: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_releases (barcode, data, fetched_at) VALUES (?, ?, ?)",
                params![barcode, json, fetched_at],
            )
            .map_err(|e| format!("Failed to cache release: {}", e))?;
        Ok(())
    }

    // ============ Artist Relations Cache ============

    /// Get cached artist relationships by MBID
    pub fn get_artist_relations(&self, mbid: &str) -> Result<Option<ArtistRelationships>, String> {
        let min_fetched_at = Self::current_timestamp() - RELATIONS_TTL_SECS;
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM mb_artist_relations WHERE mbid = ? AND fetched_at > ?",
                params![mbid, min_fetched_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to query relations cache: {}", e))?;

        if let Some(data) = result {
            serde_json::from_str(&data)
                .map(Some)
                .map_err(|e| format!("Failed to parse cached relations: {}", e))
        } else {
            Ok(None)
        }
    }

    /// Cache artist relationships
    pub fn set_artist_relations(
        &self,
        mbid: &str,
        data: &ArtistRelationships,
    ) -> Result<(), String> {
        let fetched_at = Self::current_timestamp();
        let json = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize relations: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_artist_relations (mbid, data, fetched_at) VALUES (?, ?, ?)",
                params![mbid, json, fetched_at],
            )
            .map_err(|e| format!("Failed to cache relations: {}", e))?;
        Ok(())
    }

    // ============ Artist Metadata Cache ============

    /// Get cached artist metadata by MBID
    pub fn get_artist_metadata(&self, mbid: &str) -> Result<Option<ArtistMetadata>, String> {
        let min_fetched_at = Self::current_timestamp() - METADATA_TTL_SECS;
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM mb_artist_metadata WHERE mbid = ? AND fetched_at > ?",
                params![mbid, min_fetched_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to query metadata cache: {}", e))?;

        if let Some(data) = result {
            serde_json::from_str(&data)
                .map(Some)
                .map_err(|e| format!("Failed to parse cached metadata: {}", e))
        } else {
            Ok(None)
        }
    }

    /// Cache artist metadata
    pub fn set_artist_metadata(&self, mbid: &str, data: &ArtistMetadata) -> Result<(), String> {
        let fetched_at = Self::current_timestamp();
        let json = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_artist_metadata (mbid, data, fetched_at) VALUES (?, ?, ?)",
                params![mbid, json, fetched_at],
            )
            .map_err(|e| format!("Failed to cache metadata: {}", e))?;
        Ok(())
    }

    // ============ Scene Discovery Cache ============

    /// Get cached scene discovery results
    pub fn get_scene_cache(
        &self,
        cache_key: &str,
    ) -> Result<Option<LocationDiscoveryResponse>, String> {
        let min_fetched_at = Self::current_timestamp() - SCENE_TTL_SECS;
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM mb_scene_cache WHERE cache_key = ? AND fetched_at > ?",
                params![cache_key, min_fetched_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to query scene cache: {}", e))?;

        if let Some(data) = result {
            serde_json::from_str(&data)
                .map(Some)
                .map_err(|e| format!("Failed to parse cached scene: {}", e))
        } else {
            Ok(None)
        }
    }

    /// Cache scene discovery results
    pub fn set_scene_cache(
        &self,
        cache_key: &str,
        data: &LocationDiscoveryResponse,
    ) -> Result<(), String> {
        let fetched_at = Self::current_timestamp();
        let json =
            serde_json::to_string(data).map_err(|e| format!("Failed to serialize scene: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_scene_cache (cache_key, data, fetched_at) VALUES (?, ?, ?)",
                params![cache_key, json, fetched_at],
            )
            .map_err(|e| format!("Failed to cache scene: {}", e))?;
        Ok(())
    }

    /// Drop every cached scene, leaving the rest of the cache alone.
    ///
    /// Targeted invalidation for the cases a post-filter cannot fix: the
    /// blacklist is applied INSIDE the validation loop (a blocked artist makes
    /// the loop pick the next-best same-name Qobuz artist for that MBID), so
    /// UN-blacklisting cannot be undone by filtering on the way out. Also the
    /// right hammer on account switch, because the account territory decides
    /// which candidates validate and is not yet part of the key.
    ///
    /// Returns the number of rows removed.
    pub fn clear_scene_cache(&self) -> Result<usize, String> {
        let deleted = self
            .conn
            .execute("DELETE FROM mb_scene_cache", [])
            .map_err(|e| format!("Failed to clear scene cache: {}", e))?;
        if deleted > 0 {
            log::info!("MusicBrainz scene cache invalidated: {} entries", deleted);
        }
        Ok(deleted)
    }

    // ============ Qobuz Validation Cache ============

    /// Get the typed Qobuz match for a normalized artist name.
    ///
    /// A parse failure is a MISS, not an error — the row is from an older
    /// shape and re-validating costs one search.
    pub fn get_qobuz_artist_match(
        &self,
        name_normalized: &str,
    ) -> Result<Option<QobuzArtistMatch>, String> {
        let min_fetched_at = Self::current_timestamp() - QOBUZ_ARTIST_MATCH_TTL_SECS;
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM mb_qobuz_artist_match WHERE name_normalized = ? AND fetched_at > ?",
                params![name_normalized, min_fetched_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to query artist-match cache: {}", e))?;

        Ok(result.and_then(|data| serde_json::from_str(&data).ok()))
    }

    /// Cache the typed Qobuz match for a normalized artist name.
    pub fn set_qobuz_artist_match(
        &self,
        name_normalized: &str,
        data: &QobuzArtistMatch,
    ) -> Result<(), String> {
        let fetched_at = Self::current_timestamp();
        let json = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize artist match: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_qobuz_artist_match (name_normalized, data, fetched_at) VALUES (?, ?, ?)",
                params![name_normalized, json, fetched_at],
            )
            .map_err(|e| format!("Failed to cache artist match: {}", e))?;
        Ok(())
    }

    /// Get cached Qobuz validation result for an artist name
    ///
    /// DEAD in this tree and left that way on purpose: the stored value is a
    /// whole scene-specific `LocationCandidate` keyed on the name alone, so
    /// replaying it in another scene transplants the wrong MBID, score and
    /// genres. [`Self::get_qobuz_artist_match`] is the safe replacement.
    pub fn get_qobuz_validation(&self, name_normalized: &str) -> Result<Option<String>, String> {
        let min_fetched_at = Self::current_timestamp() - QOBUZ_VALIDATION_TTL_SECS;
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM mb_qobuz_validation WHERE name_normalized = ? AND fetched_at > ?",
                params![name_normalized, min_fetched_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to query validation cache: {}", e))?;
        Ok(result)
    }

    /// Cache Qobuz validation result
    pub fn set_qobuz_validation(&self, name_normalized: &str, data: &str) -> Result<(), String> {
        let fetched_at = Self::current_timestamp();
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_qobuz_validation (name_normalized, data, fetched_at) VALUES (?, ?, ?)",
                params![name_normalized, data, fetched_at],
            )
            .map_err(|e| format!("Failed to cache validation: {}", e))?;
        Ok(())
    }

    // ============ V2 Resolved Types (structured cache) ============

    /// Get cached track by ISRC (V2 structured format)
    pub fn get_track(&self, isrc: &str) -> Result<Option<ResolvedTrack>, String> {
        let result: rusqlite::Result<ResolvedTrack> = self.conn.query_row(
            "SELECT recording_mbid, title, artist_mbids, release_mbid, isrcs, confidence
             FROM resolved_tracks WHERE isrc = ?",
            [isrc],
            |row| {
                let artist_mbids_json: String = row.get(2)?;
                let isrcs_json: String = row.get(4)?;
                let confidence_str: String = row.get(5)?;
                Ok(ResolvedTrack {
                    recording_mbid: row.get(0)?,
                    title: row.get(1)?,
                    artist_mbids: serde_json::from_str(&artist_mbids_json).unwrap_or_default(),
                    release_mbid: row.get(3)?,
                    isrcs: serde_json::from_str(&isrcs_json).unwrap_or_default(),
                    confidence: match confidence_str.as_str() {
                        "exact" => MatchConfidence::Exact,
                        "high" => MatchConfidence::High,
                        "medium" => MatchConfidence::Medium,
                        "low" => MatchConfidence::Low,
                        _ => MatchConfidence::None,
                    },
                })
            },
        );
        match result {
            Ok(track) => {
                self.increment_stat("hits");
                Ok(Some(track))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                self.increment_stat("misses");
                Ok(None)
            }
            Err(e) => Err(format!("Failed to get track: {}", e)),
        }
    }

    /// Cache a resolved track (V2 structured format)
    pub fn put_track(&self, isrc: &str, track: &ResolvedTrack) -> Result<(), String> {
        let artist_mbids_json = serde_json::to_string(&track.artist_mbids).unwrap_or_default();
        let isrcs_json = serde_json::to_string(&track.isrcs).unwrap_or_default();
        let confidence = match track.confidence {
            MatchConfidence::Exact => "exact",
            MatchConfidence::High => "high",
            MatchConfidence::Medium => "medium",
            MatchConfidence::Low => "low",
            MatchConfidence::None => "none",
        };
        self.conn
            .execute(
                "INSERT OR REPLACE INTO resolved_tracks (isrc, recording_mbid, title, artist_mbids, release_mbid, isrcs, confidence)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![isrc, track.recording_mbid, track.title, artist_mbids_json, track.release_mbid, isrcs_json, confidence],
            )
            .map_err(|e| format!("Failed to cache track: {}", e))?;
        Ok(())
    }

    /// Get cached artist by name (V2 structured format)
    pub fn get_artist(&self, name: &str) -> Result<Option<ResolvedArtist>, String> {
        let name_lower = name.to_lowercase();
        let result: rusqlite::Result<ResolvedArtist> = self.conn.query_row(
            "SELECT mbid, name, sort_name, artist_type, country, disambiguation, confidence
             FROM resolved_artists WHERE name_lower = ?",
            [&name_lower],
            |row| {
                let artist_type_str: String = row.get(3)?;
                let confidence_str: String = row.get(6)?;
                Ok(ResolvedArtist {
                    mbid: row.get(0)?,
                    name: row.get(1)?,
                    sort_name: row.get(2)?,
                    artist_type: ArtistType::from(Some(artist_type_str.as_str())),
                    country: row.get(4)?,
                    disambiguation: row.get(5)?,
                    confidence: match confidence_str.as_str() {
                        "exact" => MatchConfidence::Exact,
                        "high" => MatchConfidence::High,
                        "medium" => MatchConfidence::Medium,
                        "low" => MatchConfidence::Low,
                        _ => MatchConfidence::None,
                    },
                })
            },
        );
        match result {
            Ok(artist) => {
                self.increment_stat("hits");
                Ok(Some(artist))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                self.increment_stat("misses");
                Ok(None)
            }
            Err(e) => Err(format!("Failed to get artist: {}", e)),
        }
    }

    /// Cache a resolved artist (V2 structured format)
    pub fn put_artist(&self, artist: &ResolvedArtist) -> Result<(), String> {
        let name_lower = artist.name.to_lowercase();
        let artist_type = match artist.artist_type {
            ArtistType::Person => "person",
            ArtistType::Group => "group",
            ArtistType::Orchestra => "orchestra",
            ArtistType::Choir => "choir",
            ArtistType::Character => "character",
            ArtistType::Other => "other",
        };
        let confidence = match artist.confidence {
            MatchConfidence::Exact => "exact",
            MatchConfidence::High => "high",
            MatchConfidence::Medium => "medium",
            MatchConfidence::Low => "low",
            MatchConfidence::None => "none",
        };
        self.conn
            .execute(
                "INSERT OR REPLACE INTO resolved_artists (name_lower, mbid, name, sort_name, artist_type, country, disambiguation, confidence)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![name_lower, artist.mbid, artist.name, artist.sort_name, artist_type, artist.country, artist.disambiguation, confidence],
            )
            .map_err(|e| format!("Failed to cache artist: {}", e))?;
        Ok(())
    }

    // ============ #768 identity matcher (identity.rs) ============
    //
    // Every getter enforces TTL and matcher version in the query itself and
    // treats a malformed row as a miss. The `*_at` variants exist so tests can
    // plant an old row without waiting for the clock.

    fn kind_ttl(kind: EvidenceKind) -> i64 {
        match kind {
            EvidenceKind::VerifiedIsrc => VERIFIED_IDENTITY_TTL_SECS,
            EvidenceKind::UniqueExactName => PROVISIONAL_IDENTITY_TTL_SECS,
        }
    }

    /// Cached exact-name search window for `query_key` (evidence only —
    /// the caller re-runs the acceptance rule on it).
    pub fn get_artist_query(&self, query_key: &str) -> Result<Option<Vec<ArtistCandidate>>, String> {
        let min_fetched_at = Self::current_timestamp() - ARTIST_QUERY_TTL_SECS;
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM mb_artist_query_v2 WHERE query_key = ? AND fetched_at > ?",
                params![query_key, min_fetched_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to query artist-query cache: {}", e))?;
        Ok(result.and_then(|data| serde_json::from_str(&data).ok()))
    }

    pub fn set_artist_query(&self, query_key: &str, window: &[ArtistCandidate]) -> Result<(), String> {
        self.set_artist_query_at(query_key, window, Self::current_timestamp())
    }

    pub fn set_artist_query_at(
        &self,
        query_key: &str,
        window: &[ArtistCandidate],
        fetched_at: i64,
    ) -> Result<(), String> {
        let json = serde_json::to_string(window)
            .map_err(|e| format!("Failed to serialize artist query: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_artist_query_v2 (query_key, data, fetched_at) VALUES (?, ?, ?)",
                params![query_key, json, fetched_at],
            )
            .map_err(|e| format!("Failed to cache artist query: {}", e))?;
        Ok(())
    }

    /// Cached complete recording/credit set for a normalised ISRC.
    pub fn get_isrc_credits(&self, isrc: &str) -> Result<Option<IsrcCredits>, String> {
        let min_fetched_at = Self::current_timestamp() - ISRC_CREDITS_TTL_SECS;
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM mb_isrc_credits_v2 WHERE isrc = ? AND fetched_at > ?",
                params![isrc, min_fetched_at],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to query isrc-credits cache: {}", e))?;
        Ok(result
            .and_then(|data| serde_json::from_str::<IsrcCredits>(&data).ok())
            .filter(|c| c.isrc == isrc))
    }

    pub fn set_isrc_credits(&self, credits: &IsrcCredits) -> Result<(), String> {
        self.set_isrc_credits_at(credits, Self::current_timestamp())
    }

    pub fn set_isrc_credits_at(&self, credits: &IsrcCredits, fetched_at: i64) -> Result<(), String> {
        let json = serde_json::to_string(credits)
            .map_err(|e| format!("Failed to serialize isrc credits: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_isrc_credits_v2 (isrc, data, fetched_at) VALUES (?, ?, ?)",
                params![credits.isrc, json, fetched_at],
            )
            .map_err(|e| format!("Failed to cache isrc credits: {}", e))?;
        Ok(())
    }

    /// The VERIFIED source identity for a Qobuz artist id under the current
    /// matcher version, if it has not expired. Provisional rows are never
    /// stored here (identity.rs only writes `VerifiedIsrc`), and a row whose
    /// kind or MBID is malformed is a miss.
    pub fn get_source_identity(&self, qobuz_artist_id: u64) -> Result<Option<SourceIdentityRow>, String> {
        let min_fetched_at = Self::current_timestamp() - VERIFIED_IDENTITY_TTL_SECS;
        let result: Option<(String, String, i64, String)> = self
            .conn
            .query_row(
                "SELECT mbid, evidence_kind, evidence_count, source_name
                 FROM mb_source_identity_v2
                 WHERE qobuz_artist_id = ? AND matcher_version = ? AND fetched_at > ?",
                params![qobuz_artist_id as i64, MATCHER_VERSION as i64, min_fetched_at],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|e| format!("Failed to query source identity: {}", e))?;
        Ok(result.and_then(|(mbid, kind, count, source_name)| {
            let evidence_kind = EvidenceKind::parse(&kind)?;
            if evidence_kind != EvidenceKind::VerifiedIsrc || mbid.trim().is_empty() {
                return None;
            }
            Some(SourceIdentityRow {
                qobuz_artist_id,
                mbid,
                evidence_kind,
                evidence_count: u32::try_from(count).unwrap_or(0),
                source_name,
            })
        }))
    }

    pub fn set_source_identity(&self, row: &SourceIdentityRow) -> Result<(), String> {
        self.set_source_identity_at(row, Self::current_timestamp())
    }

    pub fn set_source_identity_at(&self, row: &SourceIdentityRow, fetched_at: i64) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_source_identity_v2
                 (qobuz_artist_id, matcher_version, mbid, evidence_kind, evidence_count, source_name, fetched_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    row.qobuz_artist_id as i64,
                    MATCHER_VERSION as i64,
                    row.mbid,
                    row.evidence_kind.as_str(),
                    row.evidence_count as i64,
                    row.source_name,
                    fetched_at
                ],
            )
            .map_err(|e| format!("Failed to cache source identity: {}", e))?;
        Ok(())
    }

    /// The scene identity for (MBID, catalog scope) under the current matcher
    /// version, with the TTL of ITS OWN evidence kind (verified 30 d,
    /// provisional 7 d). Malformed rows are a miss.
    pub fn get_scene_identity(&self, mbid: &str, scope: &str) -> Result<Option<SceneIdentityRow>, String> {
        let now = Self::current_timestamp();
        let result: Option<(i64, String, i64, String, Option<String>, Option<i64>, i64)> = self
            .conn
            .query_row(
                "SELECT qobuz_artist_id, evidence_kind, evidence_count, qobuz_name, image, albums_count, fetched_at
                 FROM mb_scene_identity_v2
                 WHERE mbid = ? AND scope = ? AND matcher_version = ?",
                params![mbid, scope, MATCHER_VERSION as i64],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| format!("Failed to query scene identity: {}", e))?;
        Ok(result.and_then(
            |(qobuz_id, kind, count, qobuz_name, image, albums_count, fetched_at)| {
                let evidence_kind = EvidenceKind::parse(&kind)?;
                if fetched_at <= now - Self::kind_ttl(evidence_kind) {
                    return None;
                }
                let qobuz_id = u64::try_from(qobuz_id).ok().filter(|id| *id > 0)?;
                Some(SceneIdentityRow {
                    mbid: mbid.to_string(),
                    scope: scope.to_string(),
                    qobuz_id,
                    evidence_kind,
                    evidence_count: u32::try_from(count).unwrap_or(0),
                    qobuz_name,
                    image,
                    albums_count: albums_count.and_then(|n| u32::try_from(n).ok()),
                })
            },
        ))
    }

    pub fn set_scene_identity(&self, row: &SceneIdentityRow) -> Result<(), String> {
        self.set_scene_identity_at(row, Self::current_timestamp())
    }

    pub fn set_scene_identity_at(&self, row: &SceneIdentityRow, fetched_at: i64) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_scene_identity_v2
                 (mbid, scope, matcher_version, qobuz_artist_id, evidence_kind, evidence_count, qobuz_name, image, albums_count, fetched_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    row.mbid,
                    row.scope,
                    MATCHER_VERSION as i64,
                    row.qobuz_id as i64,
                    row.evidence_kind.as_str(),
                    row.evidence_count as i64,
                    row.qobuz_name,
                    row.image,
                    row.albums_count.map(|n| n as i64),
                    fetched_at
                ],
            )
            .map_err(|e| format!("Failed to cache scene identity: {}", e))?;
        Ok(())
    }

    /// Plant a row under an ARBITRARY matcher version (tests: an old matcher's
    /// row must not satisfy the current lookup).
    #[cfg(test)]
    fn set_source_identity_versioned(&self, row: &SourceIdentityRow, version: i64) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO mb_source_identity_v2
                 (qobuz_artist_id, matcher_version, mbid, evidence_kind, evidence_count, source_name, fetched_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    row.qobuz_artist_id as i64,
                    version,
                    row.mbid,
                    row.evidence_kind.as_str(),
                    row.evidence_count as i64,
                    row.source_name,
                    Self::current_timestamp()
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ============ Maintenance ============

    /// Clear expired entries from all tables
    pub fn cleanup_expired(&self) -> Result<usize, String> {
        let now = Self::current_timestamp();
        let mut total_deleted = 0;

        let tables_and_ttls = [
            ("mb_recordings", RECORDING_TTL_SECS),
            ("mb_artists", ARTIST_TTL_SECS),
            ("mb_releases", RELEASE_TTL_SECS),
            ("mb_artist_relations", RELATIONS_TTL_SECS),
            ("mb_artist_metadata", METADATA_TTL_SECS),
            ("mb_scene_cache", SCENE_TTL_SECS),
            ("mb_qobuz_validation", QOBUZ_VALIDATION_TTL_SECS),
            ("mb_qobuz_artist_match", QOBUZ_ARTIST_MATCH_TTL_SECS),
            ("mb_artist_query_v2", ARTIST_QUERY_TTL_SECS),
            ("mb_isrc_credits_v2", ISRC_CREDITS_TTL_SECS),
            ("mb_source_identity_v2", VERIFIED_IDENTITY_TTL_SECS),
            // The longer of the two kinds; the read path applies the
            // provisional one itself.
            ("mb_scene_identity_v2", VERIFIED_IDENTITY_TTL_SECS),
        ];

        for (table, ttl) in &tables_and_ttls {
            total_deleted += self
                .conn
                .execute(
                    &format!("DELETE FROM {} WHERE fetched_at <= ?", table),
                    params![now - ttl],
                )
                .map_err(|e| format!("Failed to cleanup {}: {}", table, e))?;
        }

        if total_deleted > 0 {
            log::info!(
                "MusicBrainz cache cleanup: removed {} expired entries",
                total_deleted
            );
        }
        Ok(total_deleted)
    }

    /// Clear all cached data (not settings)
    pub fn clear_all(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                "
                DELETE FROM mb_recordings;
                DELETE FROM mb_artists;
                DELETE FROM mb_releases;
                DELETE FROM mb_artist_relations;
                DELETE FROM mb_artist_metadata;
                DELETE FROM mb_scene_cache;
                DELETE FROM mb_qobuz_validation;
                DELETE FROM mb_qobuz_artist_match;
                DELETE FROM resolved_tracks;
                DELETE FROM resolved_artists;
                DELETE FROM mb_artist_query_v2;
                DELETE FROM mb_isrc_credits_v2;
                DELETE FROM mb_source_identity_v2;
                DELETE FROM mb_scene_identity_v2;
                UPDATE cache_stats SET value = 0;
                ",
            )
            .map_err(|e| format!("Failed to clear MusicBrainz cache: {}", e))?;
        log::info!("MusicBrainz cache cleared");
        Ok(())
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> Result<CacheStats, String> {
        let recordings: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM mb_recordings", [], |row| row.get(0))
            .unwrap_or(0);
        let artists: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM mb_artists", [], |row| row.get(0))
            .unwrap_or(0);
        let releases: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM mb_releases", [], |row| row.get(0))
            .unwrap_or(0);
        let relations: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM mb_artist_relations", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        Ok(CacheStats {
            recordings: recordings as u64,
            artists: artists as u64,
            releases: releases as u64,
            relations: relations as u64,
        })
    }

    /// TTL-based cleanup (V2 style)
    pub fn cleanup(&self, ttl_days: u32) -> Result<(u64, u64), String> {
        let cutoff = chrono::Utc::now().timestamp() - (ttl_days as i64 * 86400);
        let tracks_deleted =
            self.conn
                .execute("DELETE FROM resolved_tracks WHERE cached_at < ?", [cutoff])
                .map_err(|e| format!("Failed to cleanup tracks: {}", e))? as u64;
        let artists_deleted =
            self.conn
                .execute("DELETE FROM resolved_artists WHERE cached_at < ?", [cutoff])
                .map_err(|e| format!("Failed to cleanup artists: {}", e))? as u64;
        Ok((tracks_deleted, artists_deleted))
    }

    fn increment_stat(&self, key: &str) {
        let _ = self.conn.execute(
            "UPDATE cache_stats SET value = value + 1 WHERE key = ?",
            [key],
        );
    }
}

#[cfg(test)]
mod identity_cache_tests {
    use super::*;

    fn cand(mbid: &str, name: &str) -> ArtistCandidate {
        ArtistCandidate {
            mbid: mbid.into(),
            name: name.into(),
            score: Some(100),
            sort_name: None,
            artist_type: None,
            country: None,
            disambiguation: None,
        }
    }

    fn source_row(qobuz: u64, mbid: &str, kind: EvidenceKind) -> SourceIdentityRow {
        SourceIdentityRow {
            qobuz_artist_id: qobuz,
            mbid: mbid.into(),
            evidence_kind: kind,
            evidence_count: 1,
            source_name: "Eve".into(),
        }
    }

    fn scene_row(mbid: &str, scope: &str, qobuz: u64, kind: EvidenceKind) -> SceneIdentityRow {
        SceneIdentityRow {
            mbid: mbid.into(),
            scope: scope.into(),
            qobuz_id: qobuz,
            evidence_kind: kind,
            evidence_count: 1,
            qobuz_name: "Eve".into(),
            image: None,
            albums_count: Some(3),
        }
    }

    #[test]
    fn two_qobuz_ids_with_the_same_name_do_not_share_a_source_mapping() {
        let c = MusicBrainzCache::open_in_memory().unwrap();
        c.set_source_identity(&source_row(7, "mb-a", EvidenceKind::VerifiedIsrc)).unwrap();
        assert_eq!(c.get_source_identity(7).unwrap().unwrap().mbid, "mb-a");
        assert!(c.get_source_identity(8).unwrap().is_none());
    }

    #[test]
    fn two_mbids_with_the_same_name_do_not_share_a_scene_mapping() {
        let c = MusicBrainzCache::open_in_memory().unwrap();
        c.set_scene_identity(&scene_row("mb-a", "FR", 7, EvidenceKind::VerifiedIsrc)).unwrap();
        assert_eq!(c.get_scene_identity("mb-a", "FR").unwrap().unwrap().qobuz_id, 7);
        assert!(c.get_scene_identity("mb-b", "FR").unwrap().is_none());
        assert!(c.get_scene_identity("mb-a", "US").unwrap().is_none(), "scope is part of the key");
    }

    #[test]
    fn legacy_name_keyed_rows_cannot_satisfy_a_v2_lookup() {
        let c = MusicBrainzCache::open_in_memory().unwrap();
        // The pre-fix tables still accept writes; the v2 getters never read them.
        c.put_artist(&ResolvedArtist {
            mbid: "deep-purple".into(),
            name: "Deep Purple".into(),
            sort_name: None,
            artist_type: ArtistType::Group,
            country: None,
            disambiguation: None,
            confidence: MatchConfidence::Exact,
        })
        .unwrap();
        c.set_qobuz_artist_match(
            "swim deep",
            &QobuzArtistMatch {
                qobuz_id: 42,
                name: "Swim Deep".into(),
                image: None,
                albums_count: Some(3),
            },
        )
        .unwrap();
        assert!(c.get_source_identity(42).unwrap().is_none());
        assert!(c.get_scene_identity("85e67b0f-afd2-46c2-88b2-c3ba9b0883f2", "FR").unwrap().is_none());
        assert!(c.get_artist_query(&super::super::identity::artist_query_key("Swim Deep")).unwrap().is_none());
    }

    #[test]
    fn a_provisional_kind_never_reads_back_as_a_verified_source_identity() {
        let c = MusicBrainzCache::open_in_memory().unwrap();
        c.set_source_identity(&source_row(7, "mb-a", EvidenceKind::UniqueExactName)).unwrap();
        assert!(c.get_source_identity(7).unwrap().is_none());
    }

    #[test]
    fn expired_rows_miss_at_read_time_without_any_cleanup_call() {
        let c = MusicBrainzCache::open_in_memory().unwrap();
        let now = MusicBrainzCache::current_timestamp();
        c.set_source_identity_at(
            &source_row(7, "mb-a", EvidenceKind::VerifiedIsrc),
            now - VERIFIED_IDENTITY_TTL_SECS - 1,
        )
        .unwrap();
        assert!(c.get_source_identity(7).unwrap().is_none());

        // Scene: provisional expires after 7 days, verified after 30.
        c.set_scene_identity_at(
            &scene_row("mb-p", "FR", 1, EvidenceKind::UniqueExactName),
            now - PROVISIONAL_IDENTITY_TTL_SECS - 1,
        )
        .unwrap();
        c.set_scene_identity_at(
            &scene_row("mb-v", "FR", 2, EvidenceKind::VerifiedIsrc),
            now - PROVISIONAL_IDENTITY_TTL_SECS - 1,
        )
        .unwrap();
        assert!(c.get_scene_identity("mb-p", "FR").unwrap().is_none());
        assert_eq!(c.get_scene_identity("mb-v", "FR").unwrap().unwrap().qobuz_id, 2);

        c.set_artist_query_at("k", &[cand("a", "Eve")], now - ARTIST_QUERY_TTL_SECS - 1)
            .unwrap();
        assert!(c.get_artist_query("k").unwrap().is_none());
        c.set_isrc_credits_at(
            &IsrcCredits {
                isrc: "GBAAA0000001".into(),
                recordings: vec![],
            },
            now - ISRC_CREDITS_TTL_SECS - 1,
        )
        .unwrap();
        assert!(c.get_isrc_credits("GBAAA0000001").unwrap().is_none());
    }

    #[test]
    fn a_previous_matcher_version_row_is_a_miss() {
        let c = MusicBrainzCache::open_in_memory().unwrap();
        c.set_source_identity_versioned(
            &source_row(7, "mb-a", EvidenceKind::VerifiedIsrc),
            MATCHER_VERSION as i64 - 1,
        )
        .unwrap();
        assert!(c.get_source_identity(7).unwrap().is_none());
    }

    #[test]
    fn malformed_provenance_is_a_miss() {
        let c = MusicBrainzCache::open_in_memory().unwrap();
        c.conn
            .execute(
                "INSERT INTO mb_source_identity_v2 (qobuz_artist_id, matcher_version, mbid, evidence_kind, evidence_count, source_name, fetched_at) VALUES (7, ?, '', 'verified_isrc', 1, 'Eve', ?)",
                params![MATCHER_VERSION as i64, MusicBrainzCache::current_timestamp()],
            )
            .unwrap();
        assert!(c.get_source_identity(7).unwrap().is_none(), "empty mbid");
        c.conn
            .execute(
                "INSERT INTO mb_scene_identity_v2 (mbid, scope, matcher_version, qobuz_artist_id, evidence_kind, evidence_count, qobuz_name, image, albums_count, fetched_at) VALUES ('m', 'FR', ?, 9, 'guessed', 1, 'Eve', NULL, NULL, ?)",
                params![MATCHER_VERSION as i64, MusicBrainzCache::current_timestamp()],
            )
            .unwrap();
        assert!(c.get_scene_identity("m", "FR").unwrap().is_none(), "unknown kind");
    }

    #[test]
    fn search_window_and_isrc_credits_round_trip() {
        let c = MusicBrainzCache::open_in_memory().unwrap();
        let window = vec![cand("a", "Eve"), cand("b", "Eve")];
        c.set_artist_query("k", &window).unwrap();
        assert_eq!(c.get_artist_query("k").unwrap().unwrap(), window);
        let credits = IsrcCredits {
            isrc: "GBAAA0000001".into(),
            recordings: vec![super::super::identity::RecordingCredits {
                recording_mbid: "r".into(),
                credit_mbids: vec!["a".into(), "b".into()],
            }],
        };
        c.set_isrc_credits(&credits).unwrap();
        assert_eq!(c.get_isrc_credits("GBAAA0000001").unwrap().unwrap(), credits);
    }

    #[test]
    fn clear_all_and_cleanup_cover_the_v2_tables() {
        let c = MusicBrainzCache::open_in_memory().unwrap();
        c.set_source_identity(&source_row(7, "mb-a", EvidenceKind::VerifiedIsrc)).unwrap();
        c.clear_all().unwrap();
        assert!(c.get_source_identity(7).unwrap().is_none());
        let old = MusicBrainzCache::current_timestamp() - VERIFIED_IDENTITY_TTL_SECS - 1;
        c.set_source_identity_at(&source_row(7, "mb-a", EvidenceKind::VerifiedIsrc), old).unwrap();
        assert_eq!(c.cleanup_expired().unwrap(), 1);
    }
}
