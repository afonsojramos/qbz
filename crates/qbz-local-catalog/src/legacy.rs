use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OpenFlags, Row};

use crate::{
    ActiveCatalog, ArtistCredit, BootstrapBatch, BootstrapLayout, BootstrapOutcome,
    BootstrapProgress, CatalogError, CreditRole, ProjectedTrack, Result, SourceKey, SourceKind,
    SourceProbe, TrackRef, BOOTSTRAP_BATCH_ROWS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacyTable {
    Local,
    Plex,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySourceSpec {
    pub source: SourceKey,
    pub database_path: PathBuf,
    table: LegacyTable,
}

pub fn discover_legacy_sources(data_dir: &Path) -> Result<Vec<LegacySourceSpec>> {
    let mut specs = Vec::new();
    discover_local(&data_dir.join("library.db"), &mut specs)?;
    discover_plex(&data_dir.join("plex_cache.db"), &mut specs)?;
    discover_remote(&data_dir.join("remote_cache.db"), &mut specs)?;
    specs.sort_by(|left, right| left.source.cmp(&right.source));
    specs.dedup_by(|left, right| left.source == right.source);
    Ok(specs)
}

pub fn bootstrap_legacy_caches(
    data_dir: &Path,
    cancelled: &AtomicBool,
) -> Result<BootstrapOutcome> {
    bootstrap_legacy_caches_with_progress(data_dir, cancelled, |_| {})
}

pub fn bootstrap_legacy_caches_with_progress(
    data_dir: &Path,
    cancelled: &AtomicBool,
    mut publish: impl FnMut(&BootstrapProgress),
) -> Result<BootstrapOutcome> {
    let layout = BootstrapLayout::new(data_dir);
    if let ActiveCatalog::Ready { catalog, manifest } = layout.open_active() {
        return Ok(BootstrapOutcome::Activated {
            generation: manifest.active_generation,
            track_count: catalog.stats()?.track_count,
            resumed_rows: 0,
        });
    }

    let specs = discover_legacy_sources(data_dir)?;
    let mut readers = specs
        .into_iter()
        .map(LegacyReader::open)
        .collect::<Result<Vec<_>>>()?;
    let probes = readers
        .iter()
        .map(|reader| reader.probe.clone())
        .collect::<Vec<_>>();
    let (mut session, _) = layout.prepare(&probes, None)?;
    let mut resumed_rows = 0_u64;
    let mut committed_rows = 0_u64;

    for reader in &mut readers {
        let mut checkpoint = session.checkpoint(&reader.spec.source)?;
        if let Some(saved) = &checkpoint {
            if saved.checkpoint_version != reader.probe.snapshot_version
                || saved.checkpoint_rows > reader.probe.row_count
            {
                session
                    .restart_changed_source(&reader.spec.source, &reader.probe.snapshot_version)?;
                checkpoint = None;
            } else {
                resumed_rows = resumed_rows.saturating_add(saved.checkpoint_rows);
            }
        }
        if checkpoint
            .as_ref()
            .is_some_and(|saved| saved.complete && saved.checkpoint_rows == reader.probe.row_count)
        {
            continue;
        }

        let mut cursor = checkpoint
            .map(|saved| saved.checkpoint_cursor)
            .unwrap_or_default();
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Ok(BootstrapOutcome::Paused {
                    generation: session.generation(),
                    source: Some(reader.spec.source.clone()),
                    committed_rows,
                });
            }
            let batch = reader.read_batch(&cursor)?;
            let saved = session.apply_batch(&batch)?;
            committed_rows = committed_rows.saturating_add(batch.tracks.len() as u64);
            cursor = saved.checkpoint_cursor;
            publish(&BootstrapProgress {
                generation: session.generation(),
                source: reader.spec.source.clone(),
                committed_rows: saved.checkpoint_rows,
                checkpoint_cursor: cursor.clone(),
                source_complete: saved.complete,
            });
            if saved.complete {
                break;
            }
            std::thread::yield_now();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    if cancelled.load(Ordering::Acquire) {
        return Ok(BootstrapOutcome::Paused {
            generation: session.generation(),
            source: None,
            committed_rows,
        });
    }
    let stats = session.stats()?;
    let manifest = session.activate(&probes)?;
    Ok(BootstrapOutcome::Activated {
        generation: manifest.active_generation,
        track_count: stats.track_count,
        resumed_rows,
    })
}

fn discover_local(path: &Path, specs: &mut Vec<LegacySourceSpec>) -> Result<()> {
    let Some(conn) = open_if_present(path)? else {
        return Ok(());
    };
    if table_exists(&conn, "local_tracks")? {
        specs.push(LegacySourceSpec {
            source: SourceKey {
                source: SourceKind::Local,
                source_instance: "library".to_string(),
            },
            database_path: path.to_path_buf(),
            table: LegacyTable::Local,
        });
    }
    Ok(())
}

fn discover_plex(path: &Path, specs: &mut Vec<LegacySourceSpec>) -> Result<()> {
    let Some(conn) = open_if_present(path)? else {
        return Ok(());
    };
    if !table_exists(&conn, "plex_cache_tracks")? {
        return Ok(());
    }
    let mut instances = HashSet::new();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT COALESCE(NULLIF(server_id,''),'default')
           FROM plex_cache_tracks",
    )?;
    for value in stmt.query_map([], |row| row.get::<_, String>(0))? {
        instances.insert(value?);
    }
    if table_exists(&conn, "plex_cache_sections")? {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT COALESCE(NULLIF(server_id,''),'default')
               FROM plex_cache_sections",
        )?;
        for value in stmt.query_map([], |row| row.get::<_, String>(0))? {
            instances.insert(value?);
        }
    }
    if instances.is_empty() {
        instances.insert("default".to_string());
    }
    for source_instance in instances {
        specs.push(LegacySourceSpec {
            source: SourceKey {
                source: SourceKind::Plex,
                source_instance,
            },
            database_path: path.to_path_buf(),
            table: LegacyTable::Plex,
        });
    }
    Ok(())
}

fn discover_remote(path: &Path, specs: &mut Vec<LegacySourceSpec>) -> Result<()> {
    let Some(conn) = open_if_present(path)? else {
        return Ok(());
    };
    if !table_exists(&conn, "remote_cache_tracks")? {
        return Ok(());
    }
    let mut keys = HashSet::new();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT source, COALESCE(NULLIF(server_id,''),'default')
           FROM remote_cache_tracks",
    )?;
    for value in stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        keys.insert(value?);
    }
    if table_exists(&conn, "remote_cache_libraries")? {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT source, COALESCE(NULLIF(server_id,''),'default')
               FROM remote_cache_libraries",
        )?;
        for value in stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            keys.insert(value?);
        }
    }
    for (word, source_instance) in keys {
        let Some(source) = SourceKind::from_str(&word) else {
            continue;
        };
        if !matches!(source, SourceKind::Jellyfin | SourceKind::Subsonic) {
            continue;
        }
        specs.push(LegacySourceSpec {
            source: SourceKey {
                source,
                source_instance,
            },
            database_path: path.to_path_buf(),
            table: LegacyTable::Remote,
        });
    }
    Ok(())
}

struct LegacyReader {
    spec: LegacySourceSpec,
    conn: Connection,
    probe: SourceProbe,
}

impl LegacyReader {
    fn open(spec: LegacySourceSpec) -> Result<Self> {
        let conn = open_read_only(&spec.database_path)?;
        conn.execute_batch("PRAGMA query_only=ON; BEGIN;")?;
        let quick_check: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        let page_size: i64 = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        let page_count: i64 = conn.query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let schema_version: i64 = conn.query_row("PRAGMA schema_version", [], |row| row.get(0))?;
        let (row_count, maximum_id, maximum_updated) = source_summary(&conn, &spec)?;
        let snapshot_version =
            format!("v1:{schema_version}:{page_count}:{row_count}:{maximum_id}:{maximum_updated}");
        let probe = SourceProbe {
            source: spec.source.clone(),
            source_path: spec.database_path.clone(),
            snapshot_version,
            row_count,
            page_bytes: (page_size.max(0) as u64).saturating_mul(page_count.max(0) as u64),
            integrity_ok: quick_check == "ok",
        };
        Ok(Self { spec, conn, probe })
    }

    fn read_batch(&self, cursor: &str) -> Result<BootstrapBatch> {
        let after = if cursor.is_empty() {
            0
        } else {
            cursor.parse::<i64>().map_err(|_| {
                CatalogError::InvalidInput(format!("invalid bootstrap cursor {cursor:?}"))
            })?
        };
        let mut tracks = match self.spec.table {
            LegacyTable::Local => read_local(&self.conn, &self.spec, after)?,
            LegacyTable::Plex => read_plex(&self.conn, &self.spec, after)?,
            LegacyTable::Remote => read_remote(&self.conn, &self.spec, after)?,
        };
        let complete = tracks.len() <= BOOTSTRAP_BATCH_ROWS;
        tracks.truncate(BOOTSTRAP_BATCH_ROWS);
        let next_cursor = tracks
            .last()
            .map(|track| legacy_cursor(track).to_string())
            .unwrap_or_else(|| cursor.to_string());
        Ok(BootstrapBatch {
            source: self.spec.source.clone(),
            snapshot_version: self.probe.snapshot_version.clone(),
            expected_cursor: cursor.to_string(),
            next_cursor,
            tracks: tracks.into_iter().map(|(_, track)| track).collect(),
            complete,
        })
    }
}

type LegacyRow = (i64, ProjectedTrack);

fn legacy_cursor(row: &LegacyRow) -> i64 {
    row.0
}

fn read_local(conn: &Connection, spec: &LegacySourceSpec, after: i64) -> Result<Vec<LegacyRow>> {
    let columns = table_columns(conn, "local_tracks")?;
    let album_id = optional_column(&columns, "album_group_key", "NULL");
    let sql = format!(
        "SELECT id, file_path, title, artist, COALESCE(album_artist,''), album,
                      duration_secs, year, disc_number, track_number, format, bit_depth,
                      CAST(sample_rate AS INTEGER), artwork_path, indexed_at, {album_id}
                 FROM local_tracks
                WHERE id > ?1
                ORDER BY id
                LIMIT ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![after, BOOTSTRAP_BATCH_ROWS as i64 + 1], |row| {
        let id: i64 = row.get(0)?;
        let artist: String = row.get(3)?;
        let album_artist: String = row.get(4)?;
        let credits = credits(&artist, &album_artist);
        Ok((
            id,
            ProjectedTrack {
                track_ref: TrackRef {
                    source: SourceKind::Local,
                    source_instance: spec.source.source_instance.clone(),
                    native_id: id.to_string(),
                },
                local_track_id: Some(id),
                local_path: row.get(1)?,
                native_album_id: row.get(15)?,
                source_copy_id: None,
                title: row.get(2)?,
                artist,
                album_artist,
                album: row.get(5)?,
                duration_ms: nonnegative(row.get::<_, Option<i64>>(6)?.unwrap_or(0))
                    .saturating_mul(1_000),
                year: optional_u32(row, 7)?,
                disc_number: optional_u32(row, 8)?,
                track_number: optional_u32(row, 9)?,
                format: row.get::<_, String>(10)?.to_ascii_lowercase(),
                bit_depth: optional_u32(row, 11)?,
                sample_rate_hz: optional_u32(row, 12)?,
                artwork_token: row.get(13)?,
                isrc: None,
                musicbrainz_recording_id: None,
                added_at: row.get::<_, Option<i64>>(14)?.unwrap_or(0),
                available: true,
                observed_generation: 0,
                credits,
            },
        ))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn read_plex(conn: &Connection, spec: &LegacySourceSpec, after: i64) -> Result<Vec<LegacyRow>> {
    let columns = table_columns(conn, "plex_cache_tracks")?;
    require_columns(
        &columns,
        &["rating_key", "title", "artist", "album", "duration_ms"],
        "plex_cache_tracks",
    )?;
    let sql = format!(
        "SELECT rowid, rating_key, title, COALESCE(artist,''), COALESCE(album,''),
                COALESCE(duration_ms,0), {year}, {disc}, {track}, {format}, {depth},
                {rate}, {art}, {updated}, {album_id}
           FROM plex_cache_tracks
          WHERE rowid > ?1
            AND COALESCE(NULLIF(server_id,''),'default') = ?2
          ORDER BY rowid
          LIMIT ?3",
        year = optional_column(&columns, "year", "NULL"),
        disc = optional_column(&columns, "disc_number", "NULL"),
        track = optional_column(&columns, "track_number", "NULL"),
        format = format_expression(&columns),
        depth = optional_column(&columns, "bit_depth", "NULL"),
        rate = optional_column(&columns, "sampling_rate_hz", "NULL"),
        art = optional_column(&columns, "artwork_path", "NULL"),
        updated = optional_column(&columns, "updated_at", "0"),
        album_id = if columns.contains("parent_rating_key") {
            "parent_rating_key".to_string()
        } else {
            optional_column(&columns, "album_key", "NULL")
        },
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![
            after,
            spec.source.source_instance,
            BOOTSTRAP_BATCH_ROWS as i64 + 1
        ],
        |row| {
            let mut mapped = map_remote_like(row, spec, SourceKind::Plex)?;
            mapped.1.native_album_id = row.get(14)?;
            Ok(mapped)
        },
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn read_remote(conn: &Connection, spec: &LegacySourceSpec, after: i64) -> Result<Vec<LegacyRow>> {
    let columns = table_columns(conn, "remote_cache_tracks")?;
    require_columns(
        &columns,
        &[
            "id",
            "item_id",
            "title",
            "artist",
            "album_artist",
            "album",
            "duration_ms",
        ],
        "remote_cache_tracks",
    )?;
    let sql = format!(
        "SELECT id, item_id, title, artist, album, duration_ms, {year}, {disc}, {track},
                {format}, {depth}, {rate}, {art}, {updated}, album_artist, {album_id}
           FROM remote_cache_tracks
          WHERE id > ?1 AND source = ?2
            AND COALESCE(NULLIF(server_id,''),'default') = ?3
          ORDER BY id
          LIMIT ?4",
        year = optional_column(&columns, "year", "NULL"),
        disc = optional_column(&columns, "disc_number", "NULL"),
        track = optional_column(&columns, "track_number", "NULL"),
        format = format_expression(&columns),
        depth = optional_column(&columns, "bit_depth", "NULL"),
        rate = optional_column(&columns, "sample_rate_hz", "NULL"),
        art = optional_column(&columns, "artwork_token", "NULL"),
        updated = optional_column(&columns, "updated_at", "0"),
        album_id = optional_column(&columns, "album_id", "NULL"),
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![
            after,
            spec.source.source.as_str(),
            spec.source.source_instance,
            BOOTSTRAP_BATCH_ROWS as i64 + 1
        ],
        |row| {
            let mut mapped = map_remote_like(row, spec, spec.source.source)?;
            mapped.1.album_artist = row.get(14)?;
            mapped.1.native_album_id = row.get(15)?;
            mapped.1.credits = credits(&mapped.1.artist, &mapped.1.album_artist);
            Ok(mapped)
        },
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn map_remote_like(
    row: &Row<'_>,
    spec: &LegacySourceSpec,
    source: SourceKind,
) -> rusqlite::Result<LegacyRow> {
    let cursor: i64 = row.get(0)?;
    let artist: String = row.get(3)?;
    Ok((
        cursor,
        ProjectedTrack {
            track_ref: TrackRef {
                source,
                source_instance: spec.source.source_instance.clone(),
                native_id: value_as_string(row, 1)?,
            },
            local_track_id: None,
            local_path: None,
            native_album_id: None,
            source_copy_id: None,
            title: row.get(2)?,
            artist: artist.clone(),
            album_artist: artist.clone(),
            album: row.get(4)?,
            duration_ms: nonnegative(row.get::<_, Option<i64>>(5)?.unwrap_or(0)),
            year: optional_u32(row, 6)?,
            disc_number: optional_u32(row, 7)?,
            track_number: optional_u32(row, 8)?,
            format: row
                .get::<_, Option<String>>(9)?
                .unwrap_or_default()
                .to_ascii_lowercase(),
            bit_depth: optional_u32(row, 10)?,
            sample_rate_hz: optional_u32(row, 11)?,
            artwork_token: row.get(12)?,
            isrc: None,
            musicbrainz_recording_id: None,
            added_at: row.get::<_, Option<i64>>(13)?.unwrap_or(0),
            available: true,
            observed_generation: 0,
            credits: credits(&artist, &artist),
        },
    ))
}

fn source_summary(conn: &Connection, spec: &LegacySourceSpec) -> Result<(u64, i64, i64)> {
    let (sql, params) = match spec.table {
        LegacyTable::Local => (
            "SELECT COUNT(*), COALESCE(MAX(id),0), COALESCE(MAX(indexed_at),0)
               FROM local_tracks"
                .to_string(),
            Vec::new(),
        ),
        LegacyTable::Plex => (
            "SELECT COUNT(*), COALESCE(MAX(rowid),0), COALESCE(MAX(updated_at),0)
               FROM plex_cache_tracks
              WHERE COALESCE(NULLIF(server_id,''),'default') = ?1"
                .to_string(),
            vec![spec.source.source_instance.clone()],
        ),
        LegacyTable::Remote => (
            "SELECT COUNT(*), COALESCE(MAX(id),0), COALESCE(MAX(updated_at),0)
               FROM remote_cache_tracks
              WHERE source = ?1
                AND COALESCE(NULLIF(server_id,''),'default') = ?2"
                .to_string(),
            vec![
                spec.source.source.as_str().to_string(),
                spec.source.source_instance.clone(),
            ],
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let values = params.iter().map(String::as_str).collect::<Vec<_>>();
    let (count, maximum_id, maximum_updated): (i64, i64, i64) = stmt
        .query_row(rusqlite::params_from_iter(values), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    Ok((count.max(0) as u64, maximum_id, maximum_updated))
}

fn credits(artist: &str, album_artist: &str) -> Vec<ArtistCredit> {
    let mut values = Vec::new();
    if !artist.trim().is_empty() {
        values.push(ArtistCredit {
            display_name: artist.trim().to_string(),
            role: CreditRole::TrackArtist,
            ordinal: 0,
        });
    }
    if !album_artist.trim().is_empty() && !album_artist.eq_ignore_ascii_case(artist) {
        values.push(ArtistCredit {
            display_name: album_artist.trim().to_string(),
            role: CreditRole::AlbumArtist,
            ordinal: 0,
        });
    }
    values
}

fn nonnegative(value: i64) -> u64 {
    value.max(0) as u64
}

fn optional_u32(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<u32>> {
    Ok(row
        .get::<_, Option<i64>>(index)?
        .and_then(|value| u32::try_from(value).ok()))
}

fn value_as_string(row: &Row<'_>, index: usize) -> rusqlite::Result<String> {
    match row.get_ref(index)? {
        ValueRef::Text(value) => Ok(String::from_utf8_lossy(value).into_owned()),
        ValueRef::Integer(value) => Ok(value.to_string()),
        _ => row.get(index),
    }
}

fn open_if_present(path: &Path) -> Result<Option<Connection>> {
    if !path.is_file() {
        return Ok(None);
    }
    open_read_only(path).map(Some)
}

fn open_read_only(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(Duration::from_millis(2_500))?;
    conn.execute_batch("PRAGMA query_only=ON;")?;
    Ok(conn)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

fn table_columns(conn: &Connection, table: &str) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<rusqlite::Result<HashSet<_>>>()?)
}

fn require_columns(columns: &HashSet<String>, required: &[&str], table: &str) -> Result<()> {
    if let Some(column) = required.iter().find(|column| !columns.contains(**column)) {
        return Err(CatalogError::InvalidSource(format!(
            "{table} is missing required column {column}"
        )));
    }
    Ok(())
}

fn optional_column(columns: &HashSet<String>, column: &str, fallback: &str) -> String {
    if columns.contains(column) {
        column.to_string()
    } else {
        fallback.to_string()
    }
}

fn format_expression(columns: &HashSet<String>) -> String {
    match (columns.contains("codec"), columns.contains("container")) {
        (true, true) => "COALESCE(NULLIF(codec,''),NULLIF(container,''),'')".to_string(),
        (true, false) => "COALESCE(codec,'')".to_string(),
        (false, true) => "COALESCE(container,'')".to_string(),
        (false, false) => "''".to_string(),
    }
}
