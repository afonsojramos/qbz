use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row, Statement};

use crate::model::{
    ProjectedTrack, QueryDescriptor, SourceKey, SourceKind, TrackCursor, TrackGroup, TrackPage,
    TrackRecord, TrackRef, TrackSort,
};
use crate::schema;
use crate::{CatalogError, Result, SCHEMA_VERSION};

const MAX_PAGE_SIZE: usize = 500;

pub struct Catalog {
    conn: Connection,
    generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogStats {
    pub schema_version: u32,
    pub generation: u64,
    pub track_count: u64,
    pub source_counts: Vec<(SourceKey, u64)>,
    pub page_size_bytes: u64,
    pub page_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityReport {
    pub sqlite_ok: bool,
    pub foreign_key_violations: u64,
    pub fts_ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryMetrics {
    pub sql_time: Duration,
    pub rows: usize,
}

struct RowWithCursor {
    record: TrackRecord,
    cursor: TrackCursor,
}

impl Catalog {
    pub fn open(path: &Path, generation: u64) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        schema::init(&mut conn, generation)?;
        Ok(Self { conn, generation })
    }

    pub fn open_in_memory(generation: u64) -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        schema::init(&mut conn, generation)?;
        Ok(Self { conn, generation })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Idempotent batch projection. Source-native identity owns the upsert;
    /// the internal catalog rowid is retained and never becomes product state.
    pub fn upsert_tracks(&mut self, tracks: &[ProjectedTrack]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        {
            let mut upsert = tx.prepare_cached(UPSERT_TRACK_SQL)?;
            let mut clear_credits =
                tx.prepare_cached("DELETE FROM artist_credits WHERE catalog_id = ?1")?;
            let mut insert_credit = tx.prepare_cached(
                "INSERT OR IGNORE INTO artist_credits(
                     catalog_id, artist_key, display_name, role, ordinal
                 ) VALUES (?1,?2,?3,?4,?5)",
            )?;
            for track in tracks {
                validate_track(track)?;
                upsert_track(&mut upsert, &mut clear_credits, &mut insert_credit, track)?;
            }
        }
        tx.commit()?;
        Ok(tracks.len())
    }

    pub fn remove_track(&mut self, track_ref: &TrackRef) -> Result<bool> {
        validate_ref(track_ref)?;
        Ok(self.conn.execute(
            "DELETE FROM tracks
              WHERE source_kind = ?1 AND source_instance = ?2 AND native_track_id = ?3",
            params![
                track_ref.source.as_str(),
                track_ref.source_instance,
                track_ref.native_id
            ],
        )? > 0)
    }

    pub fn resolve(&self, track_ref: &TrackRef) -> Result<Option<TrackRecord>> {
        validate_ref(track_ref)?;
        let sql = format!(
            "SELECT {TRACK_COLUMNS}
               FROM tracks t
              WHERE t.source_kind = ?1
                AND t.source_instance = ?2
                AND t.native_track_id = ?3"
        );
        self.conn
            .query_row(
                &sql,
                params![
                    track_ref.source.as_str(),
                    track_ref.source_instance,
                    track_ref.native_id
                ],
                map_row,
            )
            .optional()
            .map(|row| row.map(|value| value.record))
            .map_err(Into::into)
    }

    pub fn count_tracks(&self, descriptor: &QueryDescriptor) -> Result<u64> {
        validate_tracks_descriptor(descriptor)?;
        let (sql, values) = count_query(descriptor)?;
        let count: i64 = self
            .conn
            .query_row(&sql, params_from_iter(values), |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn query_tracks(
        &self,
        descriptor: &QueryDescriptor,
        cursor: Option<&TrackCursor>,
        page_size: usize,
    ) -> Result<TrackPage> {
        self.query_tracks_timed(descriptor, cursor, page_size)
            .map(|(page, _)| page)
    }

    pub fn query_tracks_timed(
        &self,
        descriptor: &QueryDescriptor,
        cursor: Option<&TrackCursor>,
        page_size: usize,
    ) -> Result<(TrackPage, QueryMetrics)> {
        validate_tracks_descriptor(descriptor)?;
        let limit = page_size.clamp(1, MAX_PAGE_SIZE);
        let (sql, values, sort) = page_query(descriptor, cursor, limit)?;
        let descriptor_key = descriptor_key(descriptor);
        let started = Instant::now();
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let mapped = stmt.query_map(params_from_iter(values), map_row)?;
        let mut values = mapped.collect::<rusqlite::Result<Vec<_>>>()?;
        for value in &mut values {
            value.cursor.sort = sort;
            value.cursor.descriptor_key = descriptor_key.clone();
        }
        let sql_time = started.elapsed();
        let has_more = values.len() > limit;
        values.truncate(limit);
        let next_cursor = has_more.then(|| values.last().expect("non-empty page").cursor.clone());
        let rows = values
            .into_iter()
            .map(|value| value.record)
            .collect::<Vec<_>>();
        let metrics = QueryMetrics {
            sql_time,
            rows: rows.len(),
        };
        Ok((
            TrackPage {
                rows,
                next_cursor,
                has_more,
            },
            metrics,
        ))
    }

    pub fn stats(&self) -> Result<CatalogStats> {
        let track_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))?;
        let mut stmt = self.conn.prepare(
            "SELECT source_kind, source_instance, COUNT(*)
               FROM tracks
              GROUP BY source_kind, source_instance
              ORDER BY source_kind, source_instance",
        )?;
        let rows = stmt.query_map([], |row| {
            let word: String = row.get(0)?;
            Ok((
                SourceKey {
                    source: SourceKind::from_str(&word).expect("schema source CHECK"),
                    source_instance: row.get(1)?,
                },
                row.get::<_, i64>(2)? as u64,
            ))
        })?;
        let source_counts = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let page_size_bytes: i64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))?;
        let page_count: i64 = self
            .conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))?;
        Ok(CatalogStats {
            schema_version: SCHEMA_VERSION,
            generation: self.generation,
            track_count: track_count as u64,
            source_counts,
            page_size_bytes: page_size_bytes as u64,
            page_count: page_count as u64,
        })
    }

    pub fn integrity_check(&self) -> Result<IntegrityReport> {
        let result: String = self
            .conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        let foreign_key_violations: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })?;
        let fts_ok = self
            .conn
            .execute(
                "INSERT INTO tracks_fts(tracks_fts) VALUES ('integrity-check')",
                [],
            )
            .is_ok();
        Ok(IntegrityReport {
            sqlite_ok: result == "ok",
            foreign_key_violations: foreign_key_violations as u64,
            fts_ok,
        })
    }

    #[cfg(test)]
    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }
}

fn validate_ref(track_ref: &TrackRef) -> Result<()> {
    if track_ref.source_instance.trim().is_empty() {
        return Err(CatalogError::InvalidInput(
            "source instance must not be empty".to_string(),
        ));
    }
    if track_ref.native_id.trim().is_empty() {
        return Err(CatalogError::InvalidInput(
            "native track id must not be empty".to_string(),
        ));
    }
    Ok(())
}

fn validate_track(track: &ProjectedTrack) -> Result<()> {
    validate_ref(&track.track_ref)?;
    if track.duration_ms > i64::MAX as u64 {
        return Err(CatalogError::InvalidInput(
            "duration does not fit SQLite INTEGER".to_string(),
        ));
    }
    Ok(())
}

fn validate_tracks_descriptor(descriptor: &QueryDescriptor) -> Result<()> {
    if descriptor.surface() != crate::QuerySurface::Tracks {
        return Err(CatalogError::InvalidInput(
            "a non-Tracks descriptor was passed to a Tracks query".to_string(),
        ));
    }
    Ok(())
}

fn upsert_track(
    upsert: &mut Statement<'_>,
    clear_credits: &mut Statement<'_>,
    insert_credit: &mut Statement<'_>,
    track: &ProjectedTrack,
) -> Result<i64> {
    let sort_title = normalize_sort_key(&track.title);
    let sort_artist = normalize_sort_key(if track.album_artist.trim().is_empty() {
        &track.artist
    } else {
        &track.album_artist
    });
    let sort_album = normalize_sort_key(&track.album);
    let year_missing = i64::from(track.year.is_none());
    let year_value = track.year.unwrap_or(0) as i64;
    let disc_sort = track.disc_number.unwrap_or(0) as i64;
    let track_sort = track.track_number.unwrap_or(0) as i64;
    let mut seen_credits = HashSet::new();
    let credits = track
        .credits
        .iter()
        .filter_map(|credit| {
            let name = credit.display_name.trim();
            (!name.is_empty() && seen_credits.insert(name.to_lowercase())).then(|| name.to_string())
        })
        .collect::<Vec<_>>()
        .join(" \u{1f} ");

    let catalog_id: i64 = upsert.query_row(
        params![
            track.track_ref.source.as_str(),
            track.track_ref.source_instance,
            track.track_ref.native_id,
            track.local_track_id,
            track.local_path,
            track.source_copy_id,
            track.title,
            sort_title,
            track.artist,
            sort_artist,
            track.album_artist,
            track.album,
            sort_album,
            credits,
            track.duration_ms as i64,
            track.year.map(i64::from),
            year_missing,
            year_value,
            track.disc_number.map(i64::from),
            disc_sort,
            track.track_number.map(i64::from),
            track_sort,
            track.format.trim().to_ascii_lowercase(),
            track.bit_depth.map(i64::from),
            track.sample_rate_hz.map(i64::from),
            track.artwork_token,
            track.isrc,
            track.musicbrainz_recording_id,
            track.added_at,
            i64::from(track.available),
            track.observed_generation,
        ],
        |row| row.get(0),
    )?;

    clear_credits.execute(params![catalog_id])?;
    for credit in &track.credits {
        let artist_key = normalize_artist_key(&credit.display_name);
        if artist_key.is_empty() {
            continue;
        }
        insert_credit.execute(params![
            catalog_id,
            artist_key,
            credit.display_name.trim(),
            credit.role.as_str(),
            i64::from(credit.ordinal),
        ])?;
    }
    Ok(catalog_id)
}

const UPSERT_TRACK_SQL: &str = "INSERT INTO tracks (
    source_kind, source_instance, native_track_id, local_track_id, local_path,
    source_copy_id, title, sort_title, artist, sort_artist, album_artist,
    album, sort_album, credits, duration_ms, year, year_missing, year_value,
    disc_number, disc_sort, track_number, track_sort, format, bit_depth,
    sample_rate_hz, artwork_token, isrc, musicbrainz_recording_id, added_at,
    available, last_observed_generation
) VALUES (
    ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,
    ?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31
)
ON CONFLICT(source_kind, source_instance, native_track_id) DO UPDATE SET
    local_track_id=excluded.local_track_id,
    local_path=excluded.local_path,
    source_copy_id=excluded.source_copy_id,
    title=excluded.title,
    sort_title=excluded.sort_title,
    artist=excluded.artist,
    sort_artist=excluded.sort_artist,
    album_artist=excluded.album_artist,
    album=excluded.album,
    sort_album=excluded.sort_album,
    credits=excluded.credits,
    duration_ms=excluded.duration_ms,
    year=excluded.year,
    year_missing=excluded.year_missing,
    year_value=excluded.year_value,
    disc_number=excluded.disc_number,
    disc_sort=excluded.disc_sort,
    track_number=excluded.track_number,
    track_sort=excluded.track_sort,
    format=excluded.format,
    bit_depth=excluded.bit_depth,
    sample_rate_hz=excluded.sample_rate_hz,
    artwork_token=excluded.artwork_token,
    isrc=excluded.isrc,
    musicbrainz_recording_id=excluded.musicbrainz_recording_id,
    added_at=excluded.added_at,
    available=excluded.available,
    last_observed_generation=excluded.last_observed_generation
RETURNING catalog_id";

struct QueryParts {
    where_sql: String,
    params: Vec<Value>,
    sort: TrackSort,
}

fn filter_parts(
    descriptor: &QueryDescriptor,
    cursor: Option<&TrackCursor>,
    alias: &str,
) -> Result<QueryParts> {
    let sort = effective_sort(descriptor);
    if let Some(cursor) = cursor {
        if cursor.sort != sort || cursor.descriptor_key != descriptor_key(descriptor) {
            return Err(CatalogError::CursorDescriptorMismatch);
        }
    }
    let mut predicates = Vec::new();
    let mut values = Vec::new();
    if descriptor.available_only() {
        predicates.push(format!("{alias}.available = ?"));
        values.push(Value::Integer(1));
    }
    if !descriptor.sources().is_empty() {
        let mut source_predicates = Vec::new();
        for source in descriptor.sources() {
            if source.source_instance.trim().is_empty() {
                return Err(CatalogError::InvalidInput(
                    "source filter instance must not be empty".to_string(),
                ));
            }
            source_predicates.push(format!(
                "({alias}.source_kind = ? AND {alias}.source_instance = ?)"
            ));
            values.push(Value::Text(source.source.as_str().to_string()));
            values.push(Value::Text(source.source_instance.clone()));
        }
        predicates.push(format!("({})", source_predicates.join(" OR ")));
    }
    if !descriptor.formats().is_empty() {
        predicates.push(format!(
            "{alias}.format IN ({})",
            std::iter::repeat("?")
                .take(descriptor.formats().len())
                .collect::<Vec<_>>()
                .join(",")
        ));
        values.extend(descriptor.formats().iter().cloned().map(Value::Text));
    }

    if let Some(cursor) = cursor {
        let (predicate, cursor_values) = cursor_predicate(cursor, alias);
        predicates.push(predicate);
        values.extend(cursor_values);
    }
    if predicates.is_empty() {
        predicates.push("1".to_string());
    }
    Ok(QueryParts {
        where_sql: predicates.join(" AND "),
        params: values,
        sort,
    })
}

fn count_query(descriptor: &QueryDescriptor) -> Result<(String, Vec<Value>)> {
    if descriptor.search().is_empty() {
        let parts = filter_parts(descriptor, None, "t")?;
        return Ok((
            format!("SELECT COUNT(*) FROM tracks t WHERE {}", parts.where_sql),
            parts.params,
        ));
    }
    let match_value = fts_match_value(descriptor.search())?;
    let parts = filter_parts(descriptor, None, "t")?;
    let sql = format!(
        "SELECT COUNT(*)
           FROM tracks_fts
           CROSS JOIN tracks t NOT INDEXED
          WHERE tracks_fts MATCH ?
            AND t.catalog_id = tracks_fts.rowid
            AND {}",
        parts.where_sql
    );
    let mut values = vec![match_value];
    values.extend(parts.params);
    Ok((sql, values))
}

fn page_query(
    descriptor: &QueryDescriptor,
    cursor: Option<&TrackCursor>,
    limit: usize,
) -> Result<(String, Vec<Value>, TrackSort)> {
    if descriptor.search().is_empty() {
        let parts = filter_parts(descriptor, cursor, "t")?;
        let sql = format!(
            "SELECT {TRACK_COLUMNS}
               FROM tracks t
              WHERE {}
              ORDER BY {}
              LIMIT {}",
            parts.where_sql,
            order_clause(parts.sort, "t"),
            limit + 1
        );
        return Ok((sql, parts.params, parts.sort));
    }

    let match_value = fts_match_value(descriptor.search())?;
    let parts = filter_parts(descriptor, cursor, "t")?;
    let sql = format!(
        "SELECT {TRACK_COLUMNS}
           FROM tracks_fts
           CROSS JOIN tracks t NOT INDEXED
          WHERE tracks_fts MATCH ?
            AND t.catalog_id = tracks_fts.rowid
            AND {}
          ORDER BY {}
          LIMIT {}",
        parts.where_sql,
        order_clause(parts.sort, "t"),
        limit + 1,
    );
    let mut values = vec![match_value];
    values.extend(parts.params);
    Ok((sql, values, parts.sort))
}

fn fts_match_value(search: &str) -> Result<Value> {
    if search.chars().count() < 3 {
        return Err(CatalogError::SearchTooShort);
    }
    let escaped = search.replace('"', "\"\"");
    Ok(Value::Text(format!("\"{escaped}\"")))
}

fn effective_sort(descriptor: &QueryDescriptor) -> TrackSort {
    match descriptor.group() {
        TrackGroup::Album => TrackSort::Default,
        TrackGroup::Artist => TrackSort::ArtistAsc,
        TrackGroup::Name => TrackSort::TitleAsc,
        TrackGroup::Off => descriptor.sort(),
    }
}

fn order_clause(sort: TrackSort, alias: &str) -> String {
    let columns = match sort {
        TrackSort::Default => {
            "sort_album, sort_artist, disc_sort, track_sort, sort_title, catalog_id"
        }
        TrackSort::TitleAsc => "sort_title, sort_artist, catalog_id",
        TrackSort::TitleDesc => "sort_title DESC, sort_artist, catalog_id",
        TrackSort::ArtistAsc => "sort_artist, sort_album, disc_sort, track_sort, catalog_id",
        TrackSort::ArtistDesc => "sort_artist DESC, sort_album, disc_sort, track_sort, catalog_id",
        TrackSort::YearAsc => {
            "year_missing, year_value, sort_album, disc_sort, track_sort, catalog_id"
        }
        TrackSort::YearDesc => {
            "year_missing, year_value DESC, sort_album, disc_sort, track_sort, catalog_id"
        }
        TrackSort::AddedDesc => "added_at DESC, sort_album, disc_sort, track_sort, catalog_id",
    };
    columns
        .split(", ")
        .map(|column| {
            let (name, suffix) = column.split_once(' ').unwrap_or((column, ""));
            if suffix.is_empty() {
                format!("{alias}.{name}")
            } else {
                format!("{alias}.{name} {suffix}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn cursor_predicate(cursor: &TrackCursor, alias: &str) -> (String, Vec<Value>) {
    let text = |value: &str| Value::Text(value.to_string());
    let integer = Value::Integer;
    let (predicate, values) = match cursor.sort {
        TrackSort::Default => (
            "(t.sort_album,t.sort_artist,t.disc_sort,t.track_sort,t.sort_title,t.catalog_id) > (?,?,?,?,?,?)",
            vec![
                text(&cursor.sort_album),
                text(&cursor.sort_artist),
                integer(cursor.disc_sort),
                integer(cursor.track_sort),
                text(&cursor.sort_title),
                integer(cursor.row_id),
            ],
        ),
        TrackSort::TitleAsc => (
            "(t.sort_title,t.sort_artist,t.catalog_id) > (?,?,?)",
            vec![
                text(&cursor.sort_title),
                text(&cursor.sort_artist),
                integer(cursor.row_id),
            ],
        ),
        TrackSort::TitleDesc => (
            "(t.sort_title < ? OR (t.sort_title = ? AND (t.sort_artist,t.catalog_id) > (?,?)))",
            vec![
                text(&cursor.sort_title),
                text(&cursor.sort_title),
                text(&cursor.sort_artist),
                integer(cursor.row_id),
            ],
        ),
        TrackSort::ArtistAsc => (
            "(t.sort_artist,t.sort_album,t.disc_sort,t.track_sort,t.catalog_id) > (?,?,?,?,?)",
            vec![
                text(&cursor.sort_artist),
                text(&cursor.sort_album),
                integer(cursor.disc_sort),
                integer(cursor.track_sort),
                integer(cursor.row_id),
            ],
        ),
        TrackSort::ArtistDesc => (
            "(t.sort_artist < ? OR (t.sort_artist = ? AND (t.sort_album,t.disc_sort,t.track_sort,t.catalog_id) > (?,?,?,?)))",
            vec![
                text(&cursor.sort_artist),
                text(&cursor.sort_artist),
                text(&cursor.sort_album),
                integer(cursor.disc_sort),
                integer(cursor.track_sort),
                integer(cursor.row_id),
            ],
        ),
        TrackSort::YearAsc => (
            "(t.year_missing,t.year_value,t.sort_album,t.disc_sort,t.track_sort,t.catalog_id) > (?,?,?,?,?,?)",
            vec![
                integer(cursor.year_missing),
                integer(cursor.year_value),
                text(&cursor.sort_album),
                integer(cursor.disc_sort),
                integer(cursor.track_sort),
                integer(cursor.row_id),
            ],
        ),
        TrackSort::YearDesc => (
            "(t.year_missing > ? OR (t.year_missing = ? AND (t.year_value < ? OR (t.year_value = ? AND (t.sort_album,t.disc_sort,t.track_sort,t.catalog_id) > (?,?,?,?)))))",
            vec![
                integer(cursor.year_missing),
                integer(cursor.year_missing),
                integer(cursor.year_value),
                integer(cursor.year_value),
                text(&cursor.sort_album),
                integer(cursor.disc_sort),
                integer(cursor.track_sort),
                integer(cursor.row_id),
            ],
        ),
        TrackSort::AddedDesc => (
            "(t.added_at < ? OR (t.added_at = ? AND (t.sort_album,t.disc_sort,t.track_sort,t.catalog_id) > (?,?,?,?)))",
            vec![
                integer(cursor.added_at),
                integer(cursor.added_at),
                text(&cursor.sort_album),
                integer(cursor.disc_sort),
                integer(cursor.track_sort),
                integer(cursor.row_id),
            ],
        ),
    };
    (predicate.replace("t.", &format!("{alias}.")), values)
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<RowWithCursor> {
    let source_word: String = row.get(0)?;
    let source = SourceKind::from_str(&source_word).expect("schema source CHECK");
    let sort_title = row.get(18)?;
    let sort_artist = row.get(19)?;
    let sort_album = row.get(20)?;
    let year_missing = row.get(21)?;
    let year_value = row.get(22)?;
    let disc_sort = row.get(23)?;
    let track_sort = row.get(24)?;
    let added_at = row.get(25)?;
    let row_id = row.get(26)?;
    Ok(RowWithCursor {
        record: TrackRecord {
            track_ref: TrackRef {
                source,
                source_instance: row.get(1)?,
                native_id: row.get(2)?,
            },
            local_track_id: row.get(3)?,
            local_path: row.get(4)?,
            title: row.get(5)?,
            artist: row.get(6)?,
            album_artist: row.get(7)?,
            album: row.get(8)?,
            duration_ms: row.get::<_, i64>(9)?.max(0) as u64,
            year: row.get::<_, Option<i64>>(10)?.map(|value| value as u32),
            disc_number: row.get::<_, Option<i64>>(11)?.map(|value| value as u32),
            track_number: row.get::<_, Option<i64>>(12)?.map(|value| value as u32),
            format: row.get(13)?,
            bit_depth: row.get::<_, Option<i64>>(14)?.map(|value| value as u32),
            sample_rate_hz: row.get::<_, Option<i64>>(15)?.map(|value| value as u32),
            artwork_token: row.get(16)?,
            available: row.get::<_, i64>(17)? != 0,
        },
        cursor: TrackCursor {
            // Filled by query_tracks_timed after the effective sort is known.
            sort: TrackSort::Default,
            descriptor_key: String::new(),
            sort_title,
            sort_artist,
            sort_album,
            year_missing,
            year_value,
            disc_sort,
            track_sort,
            added_at,
            row_id,
        },
    })
}

const TRACK_COLUMNS: &str = "
    t.source_kind, t.source_instance, t.native_track_id, t.local_track_id, t.local_path,
    t.title, t.artist, t.album_artist, t.album, t.duration_ms, t.year, t.disc_number,
    t.track_number, t.format, t.bit_depth, t.sample_rate_hz, t.artwork_token, t.available,
    t.sort_title, t.sort_artist, t.sort_album, t.year_missing, t.year_value, t.disc_sort,
    t.track_sort, t.added_at, t.catalog_id";

/// Unicode-aware display sort key shared by ingest and query indices.
pub fn normalize_sort_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Stable artist key: lowercased, common Latin diacritics folded, and runs of
/// punctuation collapsed while preserving non-Latin alphanumerics.
pub fn normalize_artist_key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.to_lowercase().chars() {
        let folded = fold_diacritic(character);
        if folded.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            out.push(folded);
            pending_space = false;
        } else {
            pending_space = true;
        }
    }
    out
}

fn fold_diacritic(character: char) -> char {
    match character {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ė' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'ī' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ō' | 'ø' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'ū' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        'ý' | 'ÿ' => 'y',
        'ß' => 's',
        _ => character,
    }
}

pub(crate) fn descriptor_key(descriptor: &QueryDescriptor) -> String {
    let mut key = String::new();
    push_key_part(
        &mut key,
        match descriptor.surface() {
            crate::QuerySurface::Tracks => "tracks",
            crate::QuerySurface::Albums => "albums",
            crate::QuerySurface::Artists => "artists",
        },
    );
    push_key_part(&mut key, descriptor.search());
    push_key_part(
        &mut key,
        if descriptor.available_only() {
            "1"
        } else {
            "0"
        },
    );
    push_key_part(&mut key, sort_word(effective_sort(descriptor)));
    push_key_part(
        &mut key,
        match descriptor.group() {
            TrackGroup::Off => "off",
            TrackGroup::Album => "album",
            TrackGroup::Artist => "artist",
            TrackGroup::Name => "name",
        },
    );
    for source in descriptor.sources() {
        push_key_part(&mut key, source.source.as_str());
        push_key_part(&mut key, &source.source_instance);
    }
    for format in descriptor.formats() {
        push_key_part(&mut key, format);
    }
    key
}

fn push_key_part(key: &mut String, value: &str) {
    key.push_str(&value.len().to_string());
    key.push(':');
    key.push_str(value);
    key.push('|');
}

fn sort_word(sort: TrackSort) -> &'static str {
    match sort {
        TrackSort::Default => "default",
        TrackSort::TitleAsc => "title-asc",
        TrackSort::TitleDesc => "title-desc",
        TrackSort::ArtistAsc => "artist-asc",
        TrackSort::ArtistDesc => "artist-desc",
        TrackSort::YearAsc => "year-asc",
        TrackSort::YearDesc => "year-desc",
        TrackSort::AddedDesc => "added-desc",
    }
}
