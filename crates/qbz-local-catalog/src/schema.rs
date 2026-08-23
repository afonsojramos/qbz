use rusqlite::{params, Connection};

use crate::{CatalogError, Result};

pub const SCHEMA_VERSION: u32 = 1;
pub const APPLICATION_ID: i64 = 0x5142_5A43; // "QBZC"

pub(crate) fn configure(conn: &Connection) -> Result<()> {
    conn.busy_timeout(std::time::Duration::from_millis(2_500))?;
    conn.execute_batch(&format!(
        "PRAGMA foreign_keys=ON;
         PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA temp_store=MEMORY;
         PRAGMA cache_size=-32768;
         PRAGMA mmap_size=268435456;
         PRAGMA application_id={APPLICATION_ID};"
    ))?;
    Ok(())
}

pub(crate) fn configure_read_only(conn: &Connection) -> Result<()> {
    conn.busy_timeout(std::time::Duration::from_millis(2_500))?;
    conn.execute_batch(
        "PRAGMA foreign_keys=ON;
         PRAGMA query_only=ON;
         PRAGMA temp_store=MEMORY;
         PRAGMA cache_size=-32768;
         PRAGMA mmap_size=268435456;",
    )?;
    Ok(())
}

pub(crate) fn verify(conn: &Connection, generation: u64) -> Result<()> {
    let application_id: i64 = conn.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    if application_id != APPLICATION_ID {
        return Err(CatalogError::NotCatalog);
    }
    let found: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if found != SCHEMA_VERSION {
        return Err(CatalogError::UnsupportedSchema {
            found,
            expected: SCHEMA_VERSION,
        });
    }
    let stored_generation: String = conn.query_row(
        "SELECT value FROM catalog_meta WHERE key = 'generation'",
        [],
        |row| row.get(0),
    )?;
    if stored_generation != generation.to_string() {
        return Err(CatalogError::InvalidInput(format!(
            "requested generation {generation}, database contains generation {stored_generation}"
        )));
    }
    Ok(())
}

pub(crate) fn init(conn: &mut Connection, generation: u64) -> Result<()> {
    let application_id: i64 = conn.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    if application_id != 0 && application_id != APPLICATION_ID {
        return Err(CatalogError::NotCatalog);
    }
    if application_id == 0 {
        let existing_tables: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
              WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        if existing_tables > 0 {
            return Err(CatalogError::NotCatalog);
        }
    }
    configure(conn)?;
    let found: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if found != 0 && found != SCHEMA_VERSION {
        return Err(CatalogError::UnsupportedSchema {
            found,
            expected: SCHEMA_VERSION,
        });
    }

    let tx = conn.transaction()?;
    tx.execute_batch(SCHEMA_SQL)?;
    tx.execute(
        "INSERT INTO catalog_meta(key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO NOTHING",
        params![SCHEMA_VERSION.to_string()],
    )?;
    tx.execute(
        "INSERT INTO catalog_meta(key, value) VALUES ('generation', ?1)
         ON CONFLICT(key) DO NOTHING",
        params![generation.to_string()],
    )?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;

    let stored_generation: String = conn.query_row(
        "SELECT value FROM catalog_meta WHERE key = 'generation'",
        [],
        |row| row.get(0),
    )?;
    if stored_generation != generation.to_string() {
        return Err(CatalogError::InvalidInput(format!(
            "requested generation {generation}, database contains generation {stored_generation}"
        )));
    }
    Ok(())
}

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS catalog_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS source_state (
    source_kind          TEXT NOT NULL CHECK (
        source_kind IN ('local','offline','plex','jellyfin','subsonic')
    ),
    source_instance      TEXT NOT NULL,
    available            INTEGER NOT NULL DEFAULT 1 CHECK (available IN (0,1)),
    last_observed_at     INTEGER NOT NULL DEFAULT 0,
    watermark            TEXT NOT NULL DEFAULT '',
    complete_generation  INTEGER NOT NULL DEFAULT 0,
    checkpoint_cursor    TEXT NOT NULL DEFAULT '',
    checkpoint_rows      INTEGER NOT NULL DEFAULT 0,
    checkpoint_version   TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (source_kind, source_instance)
) STRICT;

CREATE TABLE IF NOT EXISTS logical_albums (
    logical_album_id     INTEGER PRIMARY KEY,
    stable_key           TEXT NOT NULL UNIQUE,
    display_title        TEXT NOT NULL,
    sort_title           TEXT NOT NULL,
    display_artist       TEXT NOT NULL,
    sort_artist          TEXT NOT NULL,
    association_strength TEXT NOT NULL DEFAULT 'source_native'
        CHECK (association_strength IN (
            'source_native','text_fallback','isrc','musicbrainz','manual'
        )),
    association_evidence TEXT NOT NULL DEFAULT ''
) STRICT;

CREATE TABLE IF NOT EXISTS editions (
    edition_id             INTEGER PRIMARY KEY,
    logical_album_id       INTEGER NOT NULL REFERENCES logical_albums(logical_album_id),
    edition_key            TEXT NOT NULL UNIQUE,
    display_title          TEXT NOT NULL,
    display_artist         TEXT NOT NULL,
    release_year           INTEGER,
    musicbrainz_release_id TEXT,
    provider_release_id    TEXT,
    evidence_kind          TEXT NOT NULL DEFAULT 'source_native',
    evidence_value         TEXT NOT NULL DEFAULT ''
) STRICT;

CREATE INDEX IF NOT EXISTS idx_editions_logical
    ON editions(logical_album_id, release_year, edition_id);

CREATE TABLE IF NOT EXISTS source_copies (
    source_copy_id          INTEGER PRIMARY KEY,
    edition_id              INTEGER NOT NULL REFERENCES editions(edition_id),
    source_kind             TEXT NOT NULL CHECK (
        source_kind IN ('local','offline','plex','jellyfin','subsonic')
    ),
    source_instance         TEXT NOT NULL,
    native_album_id         TEXT NOT NULL,
    local_directory         TEXT,
    available               INTEGER NOT NULL DEFAULT 1 CHECK (available IN (0,1)),
    last_observed_generation INTEGER NOT NULL DEFAULT 0,
    UNIQUE (source_kind, source_instance, native_album_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_source_copies_edition
    ON source_copies(edition_id, source_kind, source_instance);

CREATE TABLE IF NOT EXISTS tracks (
    catalog_id                 INTEGER PRIMARY KEY,
    source_kind                TEXT NOT NULL CHECK (
        source_kind IN ('local','offline','plex','jellyfin','subsonic')
    ),
    source_instance            TEXT NOT NULL,
    native_track_id            TEXT NOT NULL,
    local_track_id             INTEGER,
    local_path                 TEXT,
    source_copy_id             INTEGER REFERENCES source_copies(source_copy_id),
    title                      TEXT NOT NULL,
    sort_title                 TEXT NOT NULL,
    artist                     TEXT NOT NULL,
    sort_artist                TEXT NOT NULL,
    album_artist               TEXT NOT NULL,
    album                      TEXT NOT NULL,
    sort_album                 TEXT NOT NULL,
    credits                    TEXT NOT NULL DEFAULT '',
    duration_ms                INTEGER NOT NULL DEFAULT 0,
    year                       INTEGER,
    year_missing               INTEGER NOT NULL CHECK (year_missing IN (0,1)),
    year_value                 INTEGER NOT NULL,
    disc_number                INTEGER,
    disc_sort                  INTEGER NOT NULL,
    track_number               INTEGER,
    track_sort                 INTEGER NOT NULL,
    format                     TEXT NOT NULL,
    bit_depth                  INTEGER,
    sample_rate_hz             INTEGER,
    artwork_token              TEXT,
    isrc                       TEXT,
    musicbrainz_recording_id   TEXT,
    added_at                   INTEGER NOT NULL DEFAULT 0,
    available                  INTEGER NOT NULL DEFAULT 1 CHECK (available IN (0,1)),
    last_observed_generation   INTEGER NOT NULL DEFAULT 0,
    UNIQUE (source_kind, source_instance, native_track_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_tracks_source_copy
    ON tracks(source_copy_id, disc_sort, track_sort, catalog_id);
CREATE INDEX IF NOT EXISTS idx_tracks_local_id
    ON tracks(local_track_id) WHERE local_track_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tracks_isrc
    ON tracks(isrc) WHERE isrc IS NOT NULL AND isrc != '';
CREATE INDEX IF NOT EXISTS idx_tracks_mbid
    ON tracks(musicbrainz_recording_id)
    WHERE musicbrainz_recording_id IS NOT NULL AND musicbrainz_recording_id != '';

-- Every index begins with the default availability predicate and ends in the
-- same catalog_id tie-breaker used by keyset cursors. Query SQL must not drift
-- from these expressions/collations.
CREATE INDEX IF NOT EXISTS idx_tracks_default
    ON tracks(available, sort_album, sort_artist, disc_sort, track_sort, sort_title, catalog_id);
CREATE INDEX IF NOT EXISTS idx_tracks_title_asc
    ON tracks(available, sort_title, sort_artist, catalog_id);
CREATE INDEX IF NOT EXISTS idx_tracks_title_desc
    ON tracks(available, sort_title DESC, sort_artist, catalog_id);
CREATE INDEX IF NOT EXISTS idx_tracks_artist_asc
    ON tracks(available, sort_artist, sort_album, disc_sort, track_sort, catalog_id);
CREATE INDEX IF NOT EXISTS idx_tracks_artist_desc
    ON tracks(available, sort_artist DESC, sort_album, disc_sort, track_sort, catalog_id);
CREATE INDEX IF NOT EXISTS idx_tracks_year_asc
    ON tracks(available, year_missing, year_value, sort_album, disc_sort, track_sort, catalog_id);
CREATE INDEX IF NOT EXISTS idx_tracks_year_desc
    ON tracks(available, year_missing, year_value DESC, sort_album, disc_sort, track_sort, catalog_id);
CREATE INDEX IF NOT EXISTS idx_tracks_added_desc
    ON tracks(available, added_at DESC, sort_album, disc_sort, track_sort, catalog_id);

CREATE TABLE IF NOT EXISTS artist_credits (
    catalog_id    INTEGER NOT NULL REFERENCES tracks(catalog_id) ON DELETE CASCADE,
    artist_key    TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    role          TEXT NOT NULL CHECK (
        role IN ('track_artist','album_artist','composer','performer','featured')
    ),
    ordinal       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (catalog_id, role, ordinal, artist_key)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artist_credits_artist
    ON artist_credits(artist_key, role, catalog_id);

CREATE TABLE IF NOT EXISTS albums_materialized (
    edition_id         INTEGER PRIMARY KEY REFERENCES editions(edition_id) ON DELETE CASCADE,
    logical_album_id   INTEGER NOT NULL REFERENCES logical_albums(logical_album_id),
    title              TEXT NOT NULL,
    sort_title         TEXT NOT NULL,
    artist             TEXT NOT NULL,
    sort_artist        TEXT NOT NULL,
    year               INTEGER,
    track_count        INTEGER NOT NULL DEFAULT 0,
    total_duration_ms  INTEGER NOT NULL DEFAULT 0,
    source_count       INTEGER NOT NULL DEFAULT 0,
    available          INTEGER NOT NULL DEFAULT 1 CHECK (available IN (0,1)),
    artwork_source     TEXT NOT NULL DEFAULT '',
    artwork_token      TEXT NOT NULL DEFAULT ''
) STRICT;

CREATE INDEX IF NOT EXISTS idx_albums_materialized_artist
    ON albums_materialized(available, sort_artist, year, sort_title, edition_id);
CREATE INDEX IF NOT EXISTS idx_albums_materialized_title
    ON albums_materialized(available, sort_title, sort_artist, edition_id);

CREATE TABLE IF NOT EXISTS artists_materialized (
    artist_key        TEXT PRIMARY KEY,
    display_name      TEXT NOT NULL,
    sort_name         TEXT NOT NULL,
    album_count       INTEGER NOT NULL DEFAULT 0,
    track_count       INTEGER NOT NULL DEFAULT 0,
    available         INTEGER NOT NULL DEFAULT 1 CHECK (available IN (0,1)),
    artwork_source    TEXT NOT NULL DEFAULT '',
    artwork_token     TEXT NOT NULL DEFAULT ''
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artists_materialized_name
    ON artists_materialized(available, sort_name, artist_key);

CREATE TABLE IF NOT EXISTS edition_artists (
    edition_id  INTEGER NOT NULL REFERENCES editions(edition_id) ON DELETE CASCADE,
    artist_key  TEXT NOT NULL,
    role        TEXT NOT NULL,
    PRIMARY KEY (edition_id, artist_key, role)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_edition_artists_artist
    ON edition_artists(artist_key, role, edition_id);

CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
    title,
    album,
    artist,
    credits,
    content='tracks',
    content_rowid='catalog_id',
    tokenize='trigram case_sensitive 0'
);

CREATE TRIGGER IF NOT EXISTS tracks_fts_insert AFTER INSERT ON tracks BEGIN
    INSERT INTO tracks_fts(rowid, title, album, artist, credits)
    VALUES (new.catalog_id, new.title, new.album, new.artist, new.credits);
END;

CREATE TRIGGER IF NOT EXISTS tracks_fts_delete AFTER DELETE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, album, artist, credits)
    VALUES ('delete', old.catalog_id, old.title, old.album, old.artist, old.credits);
END;

CREATE TRIGGER IF NOT EXISTS tracks_fts_update AFTER UPDATE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, album, artist, credits)
    VALUES ('delete', old.catalog_id, old.title, old.album, old.artist, old.credits);
    INSERT INTO tracks_fts(rowid, title, album, artist, credits)
    VALUES (new.catalog_id, new.title, new.album, new.artist, new.credits);
END;
"#;
