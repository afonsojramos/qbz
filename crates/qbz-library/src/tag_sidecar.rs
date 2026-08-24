//! LocalLibrary album tag sidecar support.
//!
//! Sidecar files live next to album folders (default `.qbz.json`) and store
//! album-level + per-track metadata overrides.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{LibraryError, LocalTrack};

const SIDECAR_FILE_NAME: &str = ".qbz.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AlbumMetadataOverride {
    pub album_title: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub catalog_number: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetadataOverride {
    pub file_path: String,
    pub cue_start_secs: Option<f64>,
    pub title: Option<String>,
    pub disc_number: Option<u32>,
    pub track_number: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumTagSidecar {
    pub version: u32,
    pub updated_at: i64,
    pub album: AlbumMetadataOverride,
    pub tracks: Vec<TrackMetadataOverride>,
}

impl AlbumTagSidecar {
    pub fn new(album: AlbumMetadataOverride, tracks: Vec<TrackMetadataOverride>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        Self {
            version: 1,
            updated_at: now,
            album,
            tracks,
        }
    }
}

pub fn sidecar_path(album_dir: &Path) -> PathBuf {
    album_dir.join(SIDECAR_FILE_NAME)
}

pub fn read_album_sidecar(album_dir: &Path) -> Result<Option<AlbumTagSidecar>, LibraryError> {
    let path = sidecar_path(album_dir);
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(&path).map_err(LibraryError::Io)?;
    let sidecar: AlbumTagSidecar =
        serde_json::from_slice(&bytes).map_err(|e| LibraryError::Metadata(e.to_string()))?;
    Ok(Some(sidecar))
}

pub fn write_album_sidecar(
    album_dir: &Path,
    sidecar: &AlbumTagSidecar,
) -> Result<(), LibraryError> {
    fs::create_dir_all(album_dir).map_err(LibraryError::Io)?;

    let target = sidecar_path(album_dir);
    let tmp = album_dir.join(format!("{}.tmp", SIDECAR_FILE_NAME));
    let content =
        serde_json::to_vec_pretty(sidecar).map_err(|e| LibraryError::Metadata(e.to_string()))?;

    fs::write(&tmp, content).map_err(LibraryError::Io)?;
    fs::rename(&tmp, &target).map_err(LibraryError::Io)?;
    Ok(())
}

pub fn delete_album_sidecar(album_dir: &Path) -> Result<(), LibraryError> {
    let path = sidecar_path(album_dir);
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).map_err(LibraryError::Io)?;
    Ok(())
}

pub fn apply_sidecar_to_track(track: &mut LocalTrack, sidecar: &AlbumTagSidecar) {
    if let Some(title) = sidecar
        .album
        .album_title
        .as_ref()
        .and_then(|s| normalize(s))
    {
        track.album = title.clone();
        track.album_group_title = title.clone();
    }

    if let Some(album_artist) = sidecar.album.album_artist.as_ref() {
        track.album_artist = normalize(album_artist);
    }

    if let Some(year) = sidecar.album.year {
        track.year = (year != 0).then_some(year);
    }

    if let Some(genre) = sidecar.album.genre.as_ref() {
        track.genre = normalize(genre);
    }

    if let Some(catalog_number) = sidecar.album.catalog_number.as_ref() {
        track.catalog_number = normalize(catalog_number);
    }

    if let Some(entry) = sidecar.tracks.iter().find(|t| {
        t.file_path == track.file_path
            && match (t.cue_start_secs, track.cue_start_secs) {
                (Some(a), Some(b)) => (a - b).abs() < 0.001,
                (None, None) => true,
                _ => false,
            }
    }) {
        if let Some(title) = entry.title.as_ref().and_then(|s| normalize(s)) {
            track.title = title.clone();
        }
        if let Some(disc) = entry.disc_number {
            track.disc_number = (disc != 0).then_some(disc);
        }
        if let Some(no) = entry.track_number {
            track.track_number = (no != 0).then_some(no);
        }
    }
}

fn normalize(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> LocalTrack {
        LocalTrack {
            id: 42,
            file_path: "/music/album/01.flac".to_string(),
            title: "Old title".to_string(),
            album: "Old album".to_string(),
            album_group_title: "Old album".to_string(),
            album_artist: Some("Old artist".to_string()),
            year: Some(1999),
            genre: Some("Old genre".to_string()),
            catalog_number: Some("OLD-1".to_string()),
            track_number: Some(1),
            disc_number: Some(2),
            ..LocalTrack::default()
        }
    }

    #[test]
    fn explicit_empty_and_zero_sentinels_clear_metadata() {
        let mut track = track();
        let sidecar = AlbumTagSidecar::new(
            AlbumMetadataOverride {
                album_title: Some("New album".to_string()),
                album_artist: Some("  ".to_string()),
                year: Some(0),
                genre: Some(String::new()),
                catalog_number: Some(String::new()),
            },
            vec![TrackMetadataOverride {
                file_path: track.file_path.clone(),
                cue_start_secs: None,
                title: Some("New title".to_string()),
                disc_number: Some(0),
                track_number: Some(0),
            }],
        );

        apply_sidecar_to_track(&mut track, &sidecar);

        assert_eq!(track.album, "New album");
        assert_eq!(track.title, "New title");
        assert_eq!(track.album_artist, None);
        assert_eq!(track.year, None);
        assert_eq!(track.genre, None);
        assert_eq!(track.catalog_number, None);
        assert_eq!(track.disc_number, None);
        assert_eq!(track.track_number, None);
    }

    #[test]
    fn absent_fields_keep_scanned_metadata_for_v1_compatibility() {
        let mut track = track();
        let sidecar = AlbumTagSidecar::new(
            AlbumMetadataOverride::default(),
            vec![TrackMetadataOverride {
                file_path: track.file_path.clone(),
                cue_start_secs: None,
                title: None,
                disc_number: None,
                track_number: None,
            }],
        );

        apply_sidecar_to_track(&mut track, &sidecar);

        assert_eq!(track.album_artist.as_deref(), Some("Old artist"));
        assert_eq!(track.year, Some(1999));
        assert_eq!(track.genre.as_deref(), Some("Old genre"));
        assert_eq!(track.catalog_number.as_deref(), Some("OLD-1"));
        assert_eq!(track.disc_number, Some(2));
        assert_eq!(track.track_number, Some(1));
    }
}
