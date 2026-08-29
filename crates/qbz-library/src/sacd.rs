//! Persistent catalogue identity for tracks inside a SACD image.
//!
//! A SACD is not a directory and an `.iso` extension is not proof that an
//! image is a SACD. Discovery therefore stays explicit: the frontend first
//! parses the Scarlet Book TOC, then hands the complete, validated generation
//! to this module. Failed or partial reads never call this API and can never
//! prune the last known-good rows.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use crate::{AudioFormat, LibraryDatabase, LibraryError, LocalTrack};

/// One completely parsed SACD image generation.
#[derive(Debug, Clone)]
pub struct SacdImageImport {
    /// Stable geometry fingerprint supplied by the SACD parser.
    pub fingerprint: String,
    /// Native image path, without the `sacd:` virtual-track scheme.
    pub image_path: String,
    pub image_size_bytes: u64,
    pub image_modified_ns: i64,
    pub observed_at: i64,
    /// Complete stereo-area track set. Each path must be
    /// `sacd:<image_path>#<track_number>`.
    pub tracks: Vec<LocalTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SacdImportResult {
    /// Authoritative `local_tracks.id`s in disc order.
    pub track_ids: Vec<i64>,
    pub inserted: usize,
    pub updated: usize,
    pub removed: usize,
    /// A newer complete generation was already committed for this disc.
    pub stale: bool,
}

impl LibraryDatabase {
    /// Atomically adopt a fully parsed SACD image into the local library.
    ///
    /// The geometry fingerprint, not the file path, owns the row ids. Moving
    /// an image and opening it again therefore preserves playlists/history
    /// references. A successful shorter generation prunes obsolete virtual
    /// tracks; validation or SQL failure rolls the whole operation back.
    pub fn import_sacd_image(
        &self,
        import: &SacdImageImport,
    ) -> Result<SacdImportResult, LibraryError> {
        let ordered = validate_import(import)?;
        self.with_connection(|connection| {
            // Reserve the writer before reading the row-id high-water mark;
            // two simultaneous explicit imports must serialize rather than
            // allocate the same ids from a shared snapshot.
            let transaction =
                Transaction::new_unchecked(connection, TransactionBehavior::Immediate)
                    .map_err(database_error)?;
            let latest_observation = transaction
                .query_row(
                    "SELECT MAX(observed_at) FROM local_sacd_images
                      WHERE fingerprint=?1 OR image_path=?2",
                    params![import.fingerprint, import.image_path],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .map_err(database_error)?;
            if latest_observation.is_some_and(|latest| latest > import.observed_at) {
                let track_ids = {
                    let mut statement = transaction
                        .prepare(
                            "SELECT local_track_id FROM local_sacd_tracks
                              WHERE fingerprint=?1 ORDER BY track_number",
                        )
                        .map_err(database_error)?;
                    let rows = statement
                        .query_map(params![import.fingerprint], |row| row.get(0))
                        .map_err(database_error)?;
                    rows.collect::<Result<Vec<_>, _>>()
                        .map_err(database_error)?
                };
                transaction.rollback().map_err(database_error)?;
                return Ok(SacdImportResult {
                    track_ids,
                    inserted: 0,
                    updated: 0,
                    removed: 0,
                    stale: true,
                });
            }
            // Allocate above the pre-transaction high-water mark. SQLite may
            // otherwise reuse the just-deleted maximum rowid when an image is
            // replaced, silently making a playlist entry for the old disc
            // point at a track on the new one.
            let mut next_id: i64 = transaction
                .query_row(
                    "SELECT COALESCE(MAX(id),0)+1 FROM local_tracks",
                    [],
                    |row| row.get(0),
                )
                .map_err(database_error)?;

            // A successfully parsed new disc at an already-known path means
            // the file was replaced. An absent/NAS-down image never reaches
            // this method, so it cannot enter this prune arm.
            let replaced: Vec<String> = {
                let mut statement = transaction
                    .prepare(
                        "SELECT fingerprint FROM local_sacd_images
                          WHERE image_path=?1 AND fingerprint<>?2",
                    )
                    .map_err(database_error)?;
                let rows = statement
                    .query_map(params![import.image_path, import.fingerprint], |row| {
                        row.get(0)
                    })
                    .map_err(database_error)?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(database_error)?
            };

            let mut removed = 0usize;
            for fingerprint in replaced {
                removed += remove_generation(&transaction, &fingerprint)?;
            }

            let mapped: HashMap<u32, i64> = {
                let mut statement = transaction
                    .prepare(
                        "SELECT track_number,local_track_id FROM local_sacd_tracks
                          WHERE fingerprint=?1",
                    )
                    .map_err(database_error)?;
                let rows = statement
                    .query_map(params![import.fingerprint], |row| {
                        Ok((row.get::<_, u32>(0)?, row.get::<_, i64>(1)?))
                    })
                    .map_err(database_error)?;
                rows.collect::<Result<HashMap<_, _>, _>>()
                    .map_err(database_error)?
            };

            let incoming_numbers = ordered
                .iter()
                .map(|(number, _)| *number)
                .collect::<HashSet<_>>();
            for (number, id) in &mapped {
                if !incoming_numbers.contains(number) {
                    transaction
                        .execute(
                            "DELETE FROM local_sacd_tracks
                              WHERE fingerprint=?1 AND track_number=?2",
                            params![import.fingerprint, number],
                        )
                        .map_err(database_error)?;
                    removed += transaction
                        .execute("DELETE FROM local_tracks WHERE id=?1", params![id])
                        .map_err(database_error)?;
                }
            }

            let mut result = SacdImportResult {
                track_ids: Vec::with_capacity(ordered.len()),
                inserted: 0,
                updated: 0,
                removed,
                stale: false,
            };

            for (number, track) in ordered {
                let existing = match mapped.get(&number).copied() {
                    Some(id) => Some(id),
                    None => transaction
                        .query_row(
                            "SELECT id FROM local_tracks
                              WHERE file_path=?1 AND cue_file_path IS NULL",
                            params![track.file_path],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(database_error)?,
                };

                let id = match existing {
                    Some(id) => {
                        update_track(&transaction, id, track)?;
                        result.updated += 1;
                        id
                    }
                    None => {
                        let id = insert_track(&transaction, next_id, track)?;
                        next_id = next_id.checked_add(1).ok_or_else(|| {
                            LibraryError::Database("local track id space exhausted".to_string())
                        })?;
                        result.inserted += 1;
                        id
                    }
                };
                transaction
                    .execute(
                        "INSERT INTO local_sacd_tracks
                             (fingerprint,track_number,local_track_id)
                         VALUES (?1,?2,?3)
                         ON CONFLICT(fingerprint,track_number) DO UPDATE SET
                             local_track_id=excluded.local_track_id",
                        params![import.fingerprint, number, id],
                    )
                    .map_err(database_error)?;
                result.track_ids.push(id);
            }

            transaction
                .execute(
                    "INSERT INTO local_sacd_images
                         (fingerprint,image_path,image_size_bytes,image_modified_ns,observed_at)
                     VALUES (?1,?2,?3,?4,?5)
                     ON CONFLICT(fingerprint) DO UPDATE SET
                         image_path=excluded.image_path,
                         image_size_bytes=excluded.image_size_bytes,
                         image_modified_ns=excluded.image_modified_ns,
                         observed_at=excluded.observed_at",
                    params![
                        import.fingerprint,
                        import.image_path,
                        to_i64(import.image_size_bytes),
                        import.image_modified_ns,
                        import.observed_at,
                    ],
                )
                .map_err(database_error)?;

            transaction.commit().map_err(database_error)?;
            Ok(result)
        })
    }
}

fn validate_import(import: &SacdImageImport) -> Result<Vec<(u32, &LocalTrack)>, LibraryError> {
    if import.fingerprint.trim().is_empty() {
        return Err(LibraryError::Other("SACD fingerprint is empty".to_string()));
    }
    if import.image_path.is_empty() {
        return Err(LibraryError::InvalidPath(
            "SACD image path is empty".to_string(),
        ));
    }
    if import.tracks.is_empty() {
        return Err(LibraryError::Other(
            "SACD generation has no tracks".to_string(),
        ));
    }

    let mut seen = HashSet::with_capacity(import.tracks.len());
    let mut ordered = Vec::with_capacity(import.tracks.len());
    for track in &import.tracks {
        let number = track
            .track_number
            .filter(|number| (1..=255).contains(number))
            .ok_or_else(|| LibraryError::Other("SACD track number is invalid".to_string()))?;
        if !seen.insert(number) {
            return Err(LibraryError::Other(format!(
                "SACD generation repeats track {number}"
            )));
        }
        if track.format != AudioFormat::Dsd {
            return Err(LibraryError::Other(format!(
                "SACD track {number} is not DSD"
            )));
        }
        let (path, path_number) = parse_virtual_path(&track.file_path).ok_or_else(|| {
            LibraryError::InvalidPath(format!("invalid SACD virtual track {number}"))
        })?;
        if path != import.image_path || path_number != number {
            return Err(LibraryError::InvalidPath(format!(
                "SACD virtual track {number} does not match its image"
            )));
        }
        ordered.push((number, track));
    }
    ordered.sort_by_key(|(number, _)| *number);
    Ok(ordered)
}

fn parse_virtual_path(value: &str) -> Option<(&str, u32)> {
    let rest = value.strip_prefix("sacd:")?;
    let (path, number) = rest.rsplit_once('#')?;
    (!path.is_empty()).then_some((path, number.parse().ok()?))
}

fn remove_generation(
    transaction: &Transaction<'_>,
    fingerprint: &str,
) -> Result<usize, LibraryError> {
    let removed = transaction
        .execute(
            "DELETE FROM local_tracks WHERE id IN (
                 SELECT local_track_id FROM local_sacd_tracks WHERE fingerprint=?1
             )",
            params![fingerprint],
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "DELETE FROM local_sacd_tracks WHERE fingerprint=?1",
            params![fingerprint],
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "DELETE FROM local_sacd_images WHERE fingerprint=?1",
            params![fingerprint],
        )
        .map_err(database_error)?;
    Ok(removed)
}

fn update_track(
    transaction: &Transaction<'_>,
    id: i64,
    track: &LocalTrack,
) -> Result<(), LibraryError> {
    let format = track.format.to_string();
    let network = i64::from(track.is_network_mount);
    let changed = transaction
        .execute(
            r#"UPDATE local_tracks SET
                file_path=?1,title=?2,artist=?3,album=?4,album_artist=?5,
                track_number=?6,disc_number=?7,year=?8,genre=?9,catalog_number=?10,
                duration_secs=?11,format=?12,bit_depth=?13,sample_rate=?14,
                channels=?15,file_size_bytes=?16,cue_file_path=NULL,
                cue_start_secs=NULL,cue_end_secs=NULL,artwork_path=?17,
                last_modified=?18,indexed_at=?19,album_group_key=?20,
                album_group_title=?21,source='user',qobuz_track_id=NULL,
                is_network_mount=?22
               WHERE id=?23"#,
            params![
                track.file_path,
                track.title,
                track.artist,
                track.album,
                track.album_artist,
                track.track_number,
                track.disc_number,
                track.year,
                track.genre,
                track.catalog_number,
                track.duration_secs as i64,
                format,
                track.bit_depth,
                track.sample_rate,
                track.channels,
                track.file_size_bytes as i64,
                track.artwork_path,
                track.last_modified,
                track.indexed_at,
                track.album_group_key,
                track.album_group_title,
                network,
                id,
            ],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err(LibraryError::Database(format!(
            "SACD mapped track {id} is missing"
        )));
    }
    Ok(())
}

fn insert_track(
    transaction: &Transaction<'_>,
    id: i64,
    track: &LocalTrack,
) -> Result<i64, LibraryError> {
    let format = track.format.to_string();
    let network = i64::from(track.is_network_mount);
    transaction
        .execute(
            r#"INSERT INTO local_tracks
               (id,file_path,title,artist,album,album_artist,track_number,disc_number,
                year,genre,catalog_number,duration_secs,format,bit_depth,sample_rate,
                channels,file_size_bytes,cue_file_path,cue_start_secs,cue_end_secs,
                artwork_path,last_modified,indexed_at,album_group_key,album_group_title,
                source,qobuz_track_id,is_network_mount)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,
                       ?16,?17,NULL,NULL,NULL,?18,?19,?20,?21,?22,'user',NULL,?23)"#,
            params![
                id,
                track.file_path,
                track.title,
                track.artist,
                track.album,
                track.album_artist,
                track.track_number,
                track.disc_number,
                track.year,
                track.genre,
                track.catalog_number,
                track.duration_secs as i64,
                format,
                track.bit_depth,
                track.sample_rate,
                track.channels,
                track.file_size_bytes as i64,
                track.artwork_path,
                track.last_modified,
                track.indexed_at,
                track.album_group_key,
                track.album_group_title,
                network,
            ],
        )
        .map_err(database_error)?;
    Ok(id)
}

fn to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn database_error(error: rusqlite::Error) -> LibraryError {
    LibraryError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_db() -> (TempDir, LibraryDatabase) {
        let temp = TempDir::new().unwrap();
        let db = LibraryDatabase::open(&temp.path().join("library.db")).unwrap();
        (temp, db)
    }

    fn image(fingerprint: &str, path: &str, count: u32) -> SacdImageImport {
        SacdImageImport {
            fingerprint: fingerprint.to_string(),
            image_path: path.to_string(),
            image_size_bytes: 4_700_000_000,
            image_modified_ns: 123,
            observed_at: 456,
            tracks: (1..=count)
                .map(|number| LocalTrack {
                    file_path: format!("sacd:{path}#{number}"),
                    title: format!("Track {number}"),
                    artist: "Artist".to_string(),
                    album: "Album".to_string(),
                    album_artist: Some("Artist".to_string()),
                    album_group_key: "sacd|||Album".to_string(),
                    album_group_title: "Album".to_string(),
                    track_number: Some(number),
                    disc_number: Some(1),
                    duration_secs: 180,
                    format: AudioFormat::Dsd,
                    bit_depth: Some(1),
                    sample_rate: 2_822_400.0,
                    channels: 2,
                    indexed_at: 456,
                    ..Default::default()
                })
                .collect(),
        }
    }

    #[test]
    fn import_is_persistent_and_reimporting_a_moved_image_keeps_row_ids() {
        let (_temp, db) = fresh_db();
        let first = db
            .import_sacd_image(&image("sacd-a", "/music/old.iso", 3))
            .unwrap();
        assert_eq!((first.inserted, first.updated, first.removed), (3, 0, 0));
        db.add_local_track_to_playlist(7, first.track_ids[1], 0)
            .unwrap();

        let mut moved = image("sacd-a", "/nas/new.iso", 3);
        moved.tracks[1].title = "Corrected".to_string();
        let second = db.import_sacd_image(&moved).unwrap();
        assert_eq!((second.inserted, second.updated, second.removed), (0, 3, 0));
        assert_eq!(second.track_ids, first.track_ids);
        assert_eq!(
            db.get_track(first.track_ids[1]).unwrap().unwrap().title,
            "Corrected"
        );
        assert_eq!(
            db.get_track(first.track_ids[1]).unwrap().unwrap().file_path,
            "sacd:/nas/new.iso#2"
        );
        let playlist_title: String = db
            .with_connection(|connection| {
                connection.query_row(
                    "SELECT t.title FROM playlist_local_tracks p
                       JOIN local_tracks t ON t.id=p.local_track_id
                      WHERE p.qobuz_playlist_id=7",
                    [],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(playlist_title, "Corrected");
        let source: String = db
            .with_connection(|connection| {
                connection.query_row(
                    "SELECT source FROM local_tracks WHERE id=?1",
                    params![first.track_ids[0]],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(source, "user", "the existing local router must own the row");
    }

    #[test]
    fn only_a_complete_successful_generation_prunes_obsolete_tracks() {
        let (_temp, db) = fresh_db();
        let first = db
            .import_sacd_image(&image("sacd-a", "/music/disc.iso", 4))
            .unwrap();

        let mut invalid = image("sacd-a", "/music/disc.iso", 2);
        invalid.tracks[1].file_path = "sacd:/different.iso#2".to_string();
        assert!(db.import_sacd_image(&invalid).is_err());
        assert!(db.get_track(first.track_ids[3]).unwrap().is_some());

        let shorter = db
            .import_sacd_image(&image("sacd-a", "/music/disc.iso", 2))
            .unwrap();
        assert_eq!(shorter.removed, 2);
        assert_eq!(shorter.track_ids, first.track_ids[..2]);
        assert!(db.get_track(first.track_ids[2]).unwrap().is_none());
        assert!(db.get_track(first.track_ids[3]).unwrap().is_none());
    }

    #[test]
    fn an_older_completed_snapshot_cannot_overwrite_a_newer_correction() {
        let (_temp, db) = fresh_db();
        let mut newer = image("sacd-a", "/music/disc.iso", 2);
        newer.observed_at = 200;
        newer.tracks[0].title = "New naming".to_string();
        db.import_sacd_image(&newer).unwrap();

        let mut older = image("sacd-a", "/music/disc.iso", 2);
        older.observed_at = 100;
        older.tracks[0].title = "Late old naming".to_string();
        let outcome = db.import_sacd_image(&older).unwrap();

        assert!(outcome.stale);
        assert_eq!(
            (outcome.inserted, outcome.updated, outcome.removed),
            (0, 0, 0)
        );
        assert_eq!(
            db.get_track(outcome.track_ids[0]).unwrap().unwrap().title,
            "New naming"
        );
    }

    #[test]
    fn a_successfully_parsed_replacement_at_the_same_path_replaces_the_old_disc() {
        let (_temp, db) = fresh_db();
        let old = db
            .import_sacd_image(&image("sacd-old", "/music/disc.iso", 2))
            .unwrap();
        let new = db
            .import_sacd_image(&image("sacd-new", "/music/disc.iso", 3))
            .unwrap();

        assert_eq!(new.removed, 2);
        assert!(
            new.track_ids.iter().all(|id| *id > old.track_ids[1]),
            "a new disc must never inherit a removed disc's row ids"
        );
        for id in old.track_ids {
            assert!(db.get_track(id).unwrap().is_none());
        }
        let old_images: i64 = db
            .with_connection(|connection| {
                connection.query_row(
                    "SELECT COUNT(*) FROM local_sacd_images WHERE fingerprint='sacd-old'",
                    [],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(old_images, 0);
    }

    #[test]
    fn clear_library_removes_tracks_and_sacd_identity_together() {
        let (_temp, db) = fresh_db();
        db.import_sacd_image(&image("sacd-a", "/music/disc.iso", 2))
            .unwrap();
        db.clear_all_tracks().unwrap();

        let counts: (i64, i64, i64) = db
            .with_connection(|connection| {
                connection.query_row(
                    "SELECT
                         (SELECT COUNT(*) FROM local_tracks),
                         (SELECT COUNT(*) FROM local_sacd_images),
                         (SELECT COUNT(*) FROM local_sacd_tracks)",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            })
            .unwrap();
        assert_eq!(counts, (0, 0, 0));
    }

    #[test]
    fn the_full_sacd_track_number_domain_imports_without_a_hidden_cap() {
        let (_temp, db) = fresh_db();
        let started = std::time::Instant::now();
        let result = db
            .import_sacd_image(&image("sacd-max", "/music/max.iso", 255))
            .unwrap();
        let sqlite_bytes: i64 = db
            .with_connection(|connection| {
                connection.query_row(
                    "SELECT
                         (SELECT page_size FROM pragma_page_size) *
                         (SELECT page_count FROM pragma_page_count)",
                    [],
                    |row| row.get(0),
                )
            })
            .unwrap();
        eprintln!(
            "sacd_import_metric tracks=255 sqlite_bytes={sqlite_bytes} elapsed_us={}",
            started.elapsed().as_micros()
        );
        assert_eq!(result.inserted, 255);
        assert_eq!(result.track_ids.len(), 255);
        assert_eq!(
            db.get_track(result.track_ids[254])
                .unwrap()
                .unwrap()
                .track_number,
            Some(255)
        );
    }
}
