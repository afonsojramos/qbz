//! Repair missing derived library rows using the ready download index only.
//! Never download, delete audio, or replace an existing library row.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::{OfflineCacheDb, OfflineCacheStatus};
use qbz_library::LibraryDatabase;

#[derive(Debug, Default)]
pub struct LibrarySyncReport {
    pub ready: usize,
    pub restored: usize,
    pub unavailable: usize,
    pub failed: usize,
}

pub fn restore_missing_library_rows(
    cache: &OfflineCacheDb,
    library: &LibraryDatabase,
    cache_root: &str,
    track_id: Option<u64>,
) -> Result<LibrarySyncReport, String> {
    let tracks = match track_id {
        Some(id) => cache.get_track(id)?.into_iter().collect(),
        None => cache.get_all_tracks()?,
    };
    let existing = library
        .get_qobuz_download_tracks()
        .map_err(|e| e.to_string())?;
    let ids: HashSet<u64> = existing
        .iter()
        .filter_map(|t| t.qobuz_track_id.map(|id| id as u64))
        .collect();
    // Reuse an album's established grouping when another cached occurrence
    // supplies its album ID. This preserves compilation album artists.
    let by_id: HashMap<u64, _> = existing
        .iter()
        .filter_map(|t| t.qobuz_track_id.map(|id| (id as u64, t)))
        .collect();
    let mut groups = HashMap::new();
    for row in cache.get_all_tracks()? {
        if let (Some(album_id), Some(local)) = (row.album_id, by_id.get(&row.track_id)) {
            groups.insert(
                album_id,
                (
                    local.album_group_key.clone(),
                    local.album_group_title.clone(),
                    local.album_artist.clone(),
                ),
            );
        }
    }
    let mut report = LibrarySyncReport::default();
    for row in tracks {
        if row.status != OfflineCacheStatus::Ready {
            continue;
        }
        report.ready += 1;
        if ids.contains(&row.track_id) {
            continue;
        }
        let bundle = cache
            .get_cmaf_bundle(row.track_id)?
            .filter(|b| b.cache_format == 2);
        let path = Path::new(&row.file_path);
        let playable = if let Some(bundle) = bundle.as_ref() {
            if !bundle
                .init_path
                .as_deref()
                .is_some_and(|p| Path::new(p).is_file())
                || !path.is_file()
            {
                report.unavailable += 1;
                continue;
            }
            path.parent().unwrap_or(path)
        } else {
            if !path.is_file() {
                report.unavailable += 1;
                continue;
            }
            path
        };
        let album = row.album.as_deref().unwrap_or("Unknown Album");
        let fallback = (format!("{}|{}", album, row.artist), album.to_owned(), None);
        let (group, title, album_artist) = row
            .album_id
            .as_ref()
            .and_then(|id| groups.get(id))
            .unwrap_or(&fallback);
        let artwork = row.resolve_cover_path(cache_root);
        // The Qobuz index stores sampling rates in kHz; local_tracks uses Hz.
        let rate = row
            .sample_rate
            .filter(|v| v.is_finite() && *v > 0.0)
            .map(|v| if v < 1000.0 { v * 1000.0 } else { v });
        let result = library.insert_qobuz_cached_track_with_grouping(
            row.track_id,
            &row.title,
            &row.artist,
            row.album.as_deref(),
            album_artist.as_deref(),
            None,
            None,
            None,
            row.duration_secs,
            &playable.to_string_lossy(),
            group,
            title,
            row.bit_depth,
            rate,
            artwork.as_deref(),
        );
        match result {
            Ok(()) => report.restored += 1,
            Err(error) => {
                report.failed += 1;
                log::warn!(
                    "[offline-library] track={} repair failed: {error}",
                    row.track_id
                );
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TrackCacheInfo;

    #[test]
    fn restores_ready_files_without_replacing_metadata_or_reviving_missing_downloads() {
        let dir = tempfile::tempdir().unwrap();
        let cache = OfflineCacheDb::new(&dir.path().join("index.db")).unwrap();
        let library = LibraryDatabase::open(&dir.path().join("library.db")).unwrap();
        for id in 1..=5 {
            let path_id = if id == 5 { 1 } else { id };
            let path = dir.path().join(format!("{path_id}.flac"));
            if id != 3 {
                std::fs::write(&path, b"fixture").unwrap();
            }
            cache
                .insert_track(
                    &TrackCacheInfo {
                        track_id: id,
                        title: format!("Track {id}"),
                        artist: "Artist".into(),
                        album: Some("Album".into()),
                        album_id: Some("album-id".into()),
                        duration_secs: 90,
                        quality: "HiRes".into(),
                        bit_depth: Some(24),
                        sample_rate: Some(96.0),
                    },
                    &path.to_string_lossy(),
                )
                .unwrap();
            if id != 4 {
                cache.mark_complete(id, 7).unwrap();
            }
        }
        library
            .insert_qobuz_cached_track_with_grouping(
                1,
                "Existing title",
                "Artist",
                Some("Album"),
                Some("Compilation"),
                Some(8),
                Some(2),
                Some(1990),
                91,
                &dir.path().join("1.flac").to_string_lossy(),
                "existing-group",
                "Album",
                Some(24),
                Some(96000.0),
                None,
            )
            .unwrap();
        let before = library.get_qobuz_download_tracks().unwrap()[0].clone();
        let report =
            restore_missing_library_rows(&cache, &library, &dir.path().to_string_lossy(), None)
                .unwrap();
        assert_eq!(
            (report.ready, report.restored, report.unavailable),
            (4, 1, 1)
        );
        assert_eq!(report.failed, 1);
        let rows = library.get_qobuz_download_tracks().unwrap();
        assert_eq!(rows.len(), 2);
        let kept = rows.iter().find(|t| t.qobuz_track_id == Some(1)).unwrap();
        assert_eq!(kept.id, before.id);
        assert_eq!(kept.title, "Existing title");
        assert_eq!(kept.track_number, Some(8));
        let added = rows.iter().find(|t| t.qobuz_track_id == Some(2)).unwrap();
        assert_eq!(added.album_group_key, "existing-group");
        assert_eq!(added.sample_rate, 96000.0);
        assert_eq!(
            restore_missing_library_rows(&cache, &library, "", None)
                .unwrap()
                .restored,
            0
        );
        cache.delete_track(2).unwrap();
        library.remove_qobuz_cached_track(2).unwrap();
        assert_eq!(
            restore_missing_library_rows(&cache, &library, "", Some(2))
                .unwrap()
                .restored,
            0
        );
    }

    #[test]
    fn cmaf_requires_init_and_segments_and_uses_bundle_directory() {
        let dir = tempfile::tempdir().unwrap();
        let cache = OfflineCacheDb::new(&dir.path().join("index.db")).unwrap();
        let library = LibraryDatabase::open(&dir.path().join("library.db")).unwrap();
        let segments = dir.path().join("segments.bin");
        let init = dir.path().join("init.mp4");
        std::fs::write(&segments, b"encrypted-fixture").unwrap();
        cache
            .insert_track(
                &TrackCacheInfo {
                    track_id: 8,
                    title: "CMAF".into(),
                    artist: "Artist".into(),
                    album: Some("Album".into()),
                    album_id: Some("a".into()),
                    duration_secs: 90,
                    quality: "HiRes".into(),
                    bit_depth: Some(24),
                    sample_rate: Some(192.0),
                },
                &segments.to_string_lossy(),
            )
            .unwrap();
        cache
            .set_cmaf_bundle(
                8,
                &segments.to_string_lossy(),
                &init.to_string_lossy(),
                b"wrapped",
                b"wrapped",
                27,
                1,
                17,
            )
            .unwrap();
        cache.mark_complete(8, 17).unwrap();
        assert_eq!(
            restore_missing_library_rows(&cache, &library, "", None)
                .unwrap()
                .unavailable,
            1
        );
        std::fs::write(&init, b"init-fixture").unwrap();
        assert_eq!(
            restore_missing_library_rows(&cache, &library, "", None)
                .unwrap()
                .restored,
            1
        );
        let rows = library.get_qobuz_download_tracks().unwrap();
        assert_eq!(rows[0].file_path, dir.path().to_string_lossy());
        assert_eq!(std::fs::read(segments).unwrap(), b"encrypted-fixture");
    }
}
