use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::*;

fn source_for(index: usize) -> SourceKind {
    match index % 100 {
        0..=59 => SourceKind::Local,
        60..=79 => SourceKind::Plex,
        80..=89 => SourceKind::Jellyfin,
        90..=97 => SourceKind::Subsonic,
        _ => SourceKind::Offline,
    }
}

fn source_instance(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Local => "local-main",
        SourceKind::Offline => "qobuz-user-7",
        SourceKind::Plex => "plex-server-a",
        SourceKind::Jellyfin => "jellyfin-server-b",
        SourceKind::Subsonic => "subsonic-server-c",
    }
}

fn projected(index: usize, source: SourceKind) -> ProjectedTrack {
    let artist_number = index % 50_000;
    let album_number = index % 80_000;
    let format = match index % 10 {
        0..=5 => "flac",
        6..=7 => "mp3",
        8 => "alac",
        _ => "dsf",
    };
    let artist = format!("Artist {artist_number:05}");
    ProjectedTrack {
        track_ref: TrackRef {
            source,
            source_instance: source_instance(source).to_string(),
            native_id: index.to_string(),
        },
        local_track_id: (source == SourceKind::Local).then_some(index as i64 + 1),
        local_path: (source == SourceKind::Local)
            .then(|| format!("/fixture/artist-{artist_number:05}/track-{index:07}.{format}")),
        source_copy_id: None,
        title: format!("Track {index:07} Signal {:03}", index % 997),
        artist: artist.clone(),
        album_artist: artist.clone(),
        album: format!("Album {album_number:05}"),
        duration_ms: 120_000 + (index % 480_000) as u64,
        year: (index % 23 != 0).then_some(1950 + (index % 77) as u32),
        disc_number: Some((index % 3 + 1) as u32),
        track_number: Some((index % 24 + 1) as u32),
        format: format.to_string(),
        bit_depth: (format != "mp3").then_some(if index % 3 == 0 { 24 } else { 16 }),
        sample_rate_hz: Some(if index % 5 == 0 { 96_000 } else { 44_100 }),
        artwork_token: Some(format!("art-{:05}", album_number)),
        isrc: (index % 11 == 0).then(|| format!("ISRC{index:08}")),
        musicbrainz_recording_id: (index % 101 == 0).then(|| format!("mbid-{index:08}")),
        added_at: 1_700_000_000 + index as i64,
        available: index % 97 != 0,
        observed_generation: 1,
        credits: vec![ArtistCredit {
            display_name: artist,
            role: CreditRole::TrackArtist,
            ordinal: 0,
        }],
    }
}

fn insert_fixture(catalog: &mut Catalog, count: usize) {
    for start in (0..count).step_by(2_000) {
        let end = (start + 2_000).min(count);
        let batch = (start..end)
            .map(|index| projected(index, source_for(index)))
            .collect::<Vec<_>>();
        catalog.upsert_tracks(&batch).unwrap();
    }
}

fn collect_all(catalog: &Catalog, descriptor: &QueryDescriptor) -> Vec<TrackRecord> {
    let mut cursor = None;
    let mut rows = Vec::new();
    loop {
        let page = catalog
            .query_tracks(descriptor, cursor.as_ref(), 137)
            .unwrap();
        let done = !page.has_more;
        cursor = page.next_cursor;
        rows.extend(page.rows);
        if done {
            break;
        }
    }
    rows
}

#[test]
fn schema_is_versioned_frontend_agnostic_and_fts5_enabled() {
    let catalog = Catalog::open_in_memory(7).unwrap();
    let stats = catalog.stats().unwrap();
    assert_eq!(stats.schema_version, 1);
    assert_eq!(stats.generation, 7);
    assert_eq!(stats.track_count, 0);
    let fts5: i64 = catalog
        .connection()
        .query_row(
            "SELECT sqlite_compileoption_used('ENABLE_FTS5')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fts5, 1);
    let tables: HashSet<String> = catalog
        .connection()
        .prepare("SELECT name FROM sqlite_master WHERE type IN ('table','view')")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    for required in [
        "source_state",
        "logical_albums",
        "editions",
        "source_copies",
        "tracks",
        "artist_credits",
        "albums_materialized",
        "artists_materialized",
        "edition_artists",
        "tracks_fts",
    ] {
        assert!(tables.contains(required), "missing {required}");
    }
}

#[test]
fn opening_an_unrelated_sqlite_file_is_refused_without_mutating_it() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("authoritative.db");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute("CREATE TABLE authoritative_secret(value TEXT)", [])
        .unwrap();
    drop(connection);

    assert!(matches!(
        Catalog::open(&path, 1),
        Err(CatalogError::NotCatalog)
    ));
    let connection = rusqlite::Connection::open(&path).unwrap();
    let catalog_tables: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'catalog_meta'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(catalog_tables, 0);
}

#[test]
fn identical_native_ids_from_distinct_sources_never_collide() {
    let mut catalog = Catalog::open_in_memory(1).unwrap();
    let mut local = projected(42, SourceKind::Local);
    let mut plex = projected(43, SourceKind::Plex);
    local.track_ref.native_id = "same".to_string();
    plex.track_ref.native_id = "same".to_string();
    catalog
        .upsert_tracks(&[local.clone(), plex.clone()])
        .unwrap();

    assert_eq!(catalog.stats().unwrap().track_count, 2);
    assert_eq!(
        catalog
            .resolve(&local.track_ref)
            .unwrap()
            .unwrap()
            .track_ref,
        local.track_ref
    );
    assert_eq!(
        catalog.resolve(&plex.track_ref).unwrap().unwrap().track_ref,
        plex.track_ref
    );
}

#[test]
fn upsert_retains_identity_and_fts_tracks_updates_and_deletes() {
    let mut catalog = Catalog::open_in_memory(1).unwrap();
    let mut track = projected(1, SourceKind::Jellyfin);
    track.title = "Old Needle".to_string();
    catalog.upsert_tracks(&[track.clone()]).unwrap();
    assert_eq!(
        catalog
            .count_tracks(&QueryDescriptor::tracks().with_search("Old Needle"))
            .unwrap(),
        1
    );

    track.title = "New Signal".to_string();
    catalog.upsert_tracks(&[track.clone()]).unwrap();
    assert_eq!(catalog.stats().unwrap().track_count, 1);
    assert_eq!(
        catalog
            .count_tracks(&QueryDescriptor::tracks().with_search("Old Needle"))
            .unwrap(),
        0
    );
    assert_eq!(
        catalog
            .count_tracks(&QueryDescriptor::tracks().with_search("New Signal"))
            .unwrap(),
        1
    );
    assert!(catalog.remove_track(&track.track_ref).unwrap());
    assert_eq!(catalog.stats().unwrap().track_count, 0);
    let integrity = catalog.integrity_check().unwrap();
    assert!(integrity.sqlite_ok && integrity.fts_ok);
    assert_eq!(integrity.foreign_key_violations, 0);
}

#[test]
fn artist_keys_preserve_roles_fold_diacritics_and_keep_non_latin_names() {
    assert_eq!(normalize_artist_key("Beyoncé & Co."), "beyonce co");
    assert_eq!(normalize_artist_key("宇多田 ヒカル"), "宇多田 ヒカル");
    let mut catalog = Catalog::open_in_memory(1).unwrap();
    let mut track = projected(3, SourceKind::Subsonic);
    track.credits = vec![
        ArtistCredit {
            display_name: "Beyoncé".to_string(),
            role: CreditRole::TrackArtist,
            ordinal: 0,
        },
        ArtistCredit {
            display_name: "Guest".to_string(),
            role: CreditRole::Featured,
            ordinal: 1,
        },
    ];
    catalog.upsert_tracks(&[track]).unwrap();
    let credits: Vec<(String, String)> = catalog
        .connection()
        .prepare("SELECT artist_key, role FROM artist_credits ORDER BY ordinal")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        credits,
        vec![
            ("beyonce".to_string(), "track_artist".to_string()),
            ("guest".to_string(), "featured".to_string())
        ]
    );
}

#[test]
fn every_sort_uses_keysets_without_omissions_or_duplicates() {
    let mut catalog = Catalog::open_in_memory(1).unwrap();
    insert_fixture(&mut catalog, 1_701);
    let expected = catalog.count_tracks(&QueryDescriptor::tracks()).unwrap() as usize;
    for sort in [
        TrackSort::Default,
        TrackSort::TitleAsc,
        TrackSort::TitleDesc,
        TrackSort::ArtistAsc,
        TrackSort::ArtistDesc,
        TrackSort::YearAsc,
        TrackSort::YearDesc,
        TrackSort::AddedDesc,
    ] {
        let rows = collect_all(&catalog, &QueryDescriptor::tracks().with_sort(sort));
        let ids: HashSet<TrackRef> = rows.iter().map(|row| row.track_ref.clone()).collect();
        assert_eq!(rows.len(), expected, "sort {sort:?}");
        assert_eq!(ids.len(), rows.len(), "sort {sort:?}");
    }
}

#[test]
fn search_source_format_availability_and_group_are_descriptor_scoped() {
    let mut catalog = Catalog::open_in_memory(1).unwrap();
    insert_fixture(&mut catalog, 2_500);
    let search = QueryDescriptor::tracks().with_search("Signal 042");
    assert!(catalog.count_tracks(&search).unwrap() > 0);
    assert!(matches!(
        catalog.count_tracks(&QueryDescriptor::tracks().with_search("Si")),
        Err(CatalogError::SearchTooShort)
    ));

    let plex_flac = QueryDescriptor::tracks()
        .with_sources(vec![SourceKey {
            source: SourceKind::Plex,
            source_instance: source_instance(SourceKind::Plex).to_string(),
        }])
        .with_formats(vec!["FLAC".to_string()]);
    let rows = collect_all(&catalog, &plex_flac);
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|row| {
        row.track_ref.source == SourceKind::Plex && row.format == "flac" && row.available
    }));

    let available = catalog.count_tracks(&QueryDescriptor::tracks()).unwrap();
    let all = catalog
        .count_tracks(&QueryDescriptor::tracks().including_unavailable())
        .unwrap();
    assert!(all > available);

    let grouped = collect_all(
        &catalog,
        &QueryDescriptor::tracks()
            .with_sort(TrackSort::AddedDesc)
            .with_group(TrackGroup::Artist),
    );
    assert_eq!(grouped.len(), available as usize);

    let first = catalog
        .query_tracks(&QueryDescriptor::tracks(), None, 50)
        .unwrap();
    let cursor = first.next_cursor.expect("fixture has another page");
    assert!(matches!(
        catalog.query_tracks(
            &QueryDescriptor::tracks().with_formats(vec!["flac".to_string()]),
            Some(&cursor),
            50,
        ),
        Err(CatalogError::CursorDescriptorMismatch)
    ));
    assert!(matches!(
        catalog.count_tracks(&QueryDescriptor::albums()),
        Err(CatalogError::InvalidInput(_))
    ));
}

#[test]
fn album_hierarchy_keeps_weak_same_name_matches_reversible() {
    let catalog = Catalog::open_in_memory(1).unwrap();
    catalog
        .connection()
        .execute_batch(
            "INSERT INTO logical_albums
                 (stable_key,display_title,sort_title,display_artist,sort_artist,
                  association_strength,association_evidence)
             VALUES
                 ('plex:a','Album','album','Artist','artist','source_native','plex:a'),
                 ('jellyfin:b','Album','album','Artist','artist','source_native','jellyfin:b');
             INSERT INTO editions
                 (logical_album_id,edition_key,display_title,display_artist,evidence_kind,evidence_value)
             SELECT logical_album_id, stable_key || ':edition', display_title, display_artist,
                    'source_native', stable_key
               FROM logical_albums;
             INSERT INTO source_copies
                 (edition_id,source_kind,source_instance,native_album_id)
             SELECT edition_id,
                    CASE WHEN edition_key LIKE 'plex:%' THEN 'plex' ELSE 'jellyfin' END,
                    CASE WHEN edition_key LIKE 'plex:%' THEN 'plex-a' ELSE 'jellyfin-b' END,
                    edition_key
               FROM editions;",
        )
        .unwrap();
    let counts: (i64, i64, i64) = catalog
        .connection()
        .query_row(
            "SELECT
                 (SELECT COUNT(*) FROM logical_albums),
                 (SELECT COUNT(*) FROM editions),
                 (SELECT COUNT(*) FROM source_copies)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (2, 2, 2));
}

#[test]
fn sort_indices_match_the_actual_order_by_without_temp_sort() {
    let mut catalog = Catalog::open_in_memory(1).unwrap();
    insert_fixture(&mut catalog, 100);
    for (index, order) in [
        (
            "idx_tracks_default",
            "sort_album,sort_artist,disc_sort,track_sort,sort_title,catalog_id",
        ),
        (
            "idx_tracks_title_desc",
            "sort_title DESC,sort_artist,catalog_id",
        ),
        (
            "idx_tracks_year_desc",
            "year_missing,year_value DESC,sort_album,disc_sort,track_sort,catalog_id",
        ),
    ] {
        let sql = format!(
            "EXPLAIN QUERY PLAN SELECT catalog_id FROM tracks
              WHERE available=1 ORDER BY {order} LIMIT 250"
        );
        let details = catalog
            .connection()
            .prepare(&sql)
            .unwrap()
            .query_map([], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
            .join(" | ");
        assert!(details.contains(index), "{details}");
        assert!(!details.contains("TEMP B-TREE"), "{details}");
    }

    let fts_plan = catalog
        .connection()
        .prepare(
            "EXPLAIN QUERY PLAN
             SELECT t.catalog_id
               FROM tracks_fts
               CROSS JOIN tracks t NOT INDEXED
              WHERE tracks_fts MATCH '\"Signal 042\"'
                AND t.catalog_id=tracks_fts.rowid
                AND t.available=1
              ORDER BY t.sort_album,t.sort_artist,t.disc_sort,t.track_sort,
                       t.sort_title,t.catalog_id
              LIMIT 250",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join(" | ");
    assert!(fts_plan.contains("VIRTUAL TABLE INDEX"), "{fts_plan}");
    assert!(fts_plan.contains("INTEGER PRIMARY KEY"), "{fts_plan}");
}

/// The contract's reproducible scale gate. It is ignored in ordinary unit
/// runs because it intentionally creates and removes a several-hundred-MiB
/// database; the implementation handoff records an explicit execution.
#[test]
#[ignore = "explicit 1M catalog fixture and query benchmark"]
fn fixture_one_million_tracks_meets_the_query_shape_gate() {
    const TRACKS: usize = 1_000_000;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("local_catalog-v1-g1.db.building");
    let mut catalog = Catalog::open(&path, 1).unwrap();
    let insert_started = Instant::now();
    for start in (0..TRACKS).step_by(5_000) {
        let end = (start + 5_000).min(TRACKS);
        let batch = (start..end)
            .map(|index| projected(index, source_for(index)))
            .collect::<Vec<_>>();
        catalog.upsert_tracks(&batch).unwrap();
        if end % 100_000 == 0 {
            println!("[catalog-1m] projected={end}");
        }
    }
    let insert_time = insert_started.elapsed();
    catalog
        .connection()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    let stats = catalog.stats().unwrap();
    assert_eq!(stats.track_count, TRACKS as u64);
    let integrity = catalog.integrity_check().unwrap();
    assert!(integrity.sqlite_ok && integrity.fts_ok);
    assert_eq!(integrity.foreign_key_violations, 0);

    let default = QueryDescriptor::tracks();
    let (_, first_warmup) = catalog.query_tracks_timed(&default, None, 250).unwrap();
    let (_, first_warm) = catalog.query_tracks_timed(&default, None, 250).unwrap();
    let broad_fts = QueryDescriptor::tracks().with_search("Signal 731");
    let (broad_page, broad_fts_cold) = catalog.query_tracks_timed(&broad_fts, None, 250).unwrap();
    assert_eq!(broad_page.rows.len(), 250);
    let (_, broad_fts_warm) = catalog.query_tracks_timed(&broad_fts, None, 250).unwrap();
    let selective_fts = QueryDescriptor::tracks().with_search("0731042");
    let (selective_page, selective_fts_cold) = catalog
        .query_tracks_timed(&selective_fts, None, 250)
        .unwrap();
    assert_eq!(selective_page.rows.len(), 1);
    let (_, selective_fts_warm) = catalog
        .query_tracks_timed(&selective_fts, None, 250)
        .unwrap();

    let deep_cursor = catalog
        .connection()
        .query_row(
            "SELECT sort_title,sort_artist,sort_album,year_missing,year_value,
                    disc_sort,track_sort,added_at,catalog_id
               FROM tracks WHERE available=1
              ORDER BY sort_title,sort_artist,catalog_id
              LIMIT 1 OFFSET 800000",
            [],
            |row| {
                Ok(TrackCursor {
                    sort: TrackSort::TitleAsc,
                    descriptor_key: crate::catalog::descriptor_key(
                        &QueryDescriptor::tracks().with_sort(TrackSort::TitleAsc),
                    ),
                    sort_title: row.get(0)?,
                    sort_artist: row.get(1)?,
                    sort_album: row.get(2)?,
                    year_missing: row.get(3)?,
                    year_value: row.get(4)?,
                    disc_sort: row.get(5)?,
                    track_sort: row.get(6)?,
                    added_at: row.get(7)?,
                    row_id: row.get(8)?,
                })
            },
        )
        .unwrap();
    let title = QueryDescriptor::tracks().with_sort(TrackSort::TitleAsc);
    let (_, deep_metrics) = catalog
        .query_tracks_timed(&title, Some(&deep_cursor), 250)
        .unwrap();

    drop(catalog);
    let reopened = Catalog::open(&path, 1).unwrap();
    let (_, connection_cold) = reopened.query_tracks_timed(&default, None, 250).unwrap();
    let file_bytes = std::fs::metadata(&path).unwrap().len();
    println!(
        "[catalog-1m] rows={TRACKS} db_bytes={file_bytes} insert_s={:.3} first_warmup_ms={:.3} first_warm_ms={:.3} broad_fts_cold_ms={:.3} broad_fts_warm_ms={:.3} selective_fts_cold_ms={:.3} selective_fts_warm_ms={:.3} deep_keyset_ms={:.3} connection_cold_ms={:.3}",
        insert_time.as_secs_f64(),
        first_warmup.sql_time.as_secs_f64() * 1000.0,
        first_warm.sql_time.as_secs_f64() * 1000.0,
        broad_fts_cold.sql_time.as_secs_f64() * 1000.0,
        broad_fts_warm.sql_time.as_secs_f64() * 1000.0,
        selective_fts_cold.sql_time.as_secs_f64() * 1000.0,
        selective_fts_warm.sql_time.as_secs_f64() * 1000.0,
        deep_metrics.sql_time.as_secs_f64() * 1000.0,
        connection_cold.sql_time.as_secs_f64() * 1000.0,
    );

    assert!(first_warm.sql_time <= Duration::from_millis(50));
    assert!(broad_fts_warm.sql_time <= Duration::from_millis(250));
    assert!(selective_fts_warm.sql_time <= Duration::from_millis(50));
    assert!(deep_metrics.sql_time <= Duration::from_millis(100));
    assert!(connection_cold.sql_time <= Duration::from_millis(250));
}
