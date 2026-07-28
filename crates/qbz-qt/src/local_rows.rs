//! Local Library transport rows + the `qbz_library` -> row mappers.
//!
//! Split out of `local_library_qt.rs` (phase-24 modularization). This file
//! owns ONLY the shape the QML view parses (one flat serde document per
//! surface) and the pure mapping helpers that fill it. No DB access, no
//! state, no Qt.
//!
//! The `source` field is the row's SOURCE BADGE value and has exactly three
//! values app-wide: `"local"` (a user file), `"offline"` (a Qobuz download —
//! the Slint's raw `qobuz_download`), `"plex"` (a Plex-cache row). QML keys
//! its badge glyph/tint off this string.

use std::collections::HashMap;

use qbz_library::{AudioFormat, LocalAlbum, LocalTrack};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Rows (the QML contract)
// ---------------------------------------------------------------------------

/// One album card (Albums tab + Folders tab flat mode). `id` is the group key
/// of the ACTIVE identity mode — folder or metadata — so the detail query can
/// round-trip it. Plex albums carry the content-hash key `plex:<hash>`.
#[derive(Clone, Default, Serialize)]
pub struct AlbumRow {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "allArtists")]
    pub all_artists: String,
    pub year: String,
    #[serde(rename = "trackCount")]
    pub track_count: u32,
    pub duration: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub format: String,
    #[serde(rename = "artKey")]
    pub art_key: String,
    /// "local" | "offline" | "plex" (the source badge).
    pub source: String,
    #[serde(rename = "directoryPath")]
    pub directory_path: String,
    /// Number of distinct contributing folders (metadata mode only; > 1 is
    /// the "album spans N folders" tooltip case).
    #[serde(rename = "folderCount")]
    pub folder_count: u32,
}

/// One track row (Tracks tab, album detail, folder detail).
#[derive(Clone, Default, Serialize)]
pub struct TrackRow {
    /// The local-library row id as a string. Plex rows carry the namespaced
    /// id from `local_plex::PLEX_TRACK_ID_FLOOR`, so it can never collide
    /// with a real `local_tracks.id`.
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(rename = "albumId")]
    pub album_id: String,
    /// Never a Qobuz artist id — local/Plex rows have none; kept so the
    /// shared TrackRow.qml "go to artist" arm stays hidden.
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub number: u32,
    pub disc: u32,
    pub duration: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub format: String,
    pub year: String,
    #[serde(rename = "artKey")]
    pub art_key: String,
    /// Filled by the artwork window (never on the bulk publish).
    #[serde(rename = "artPath")]
    pub art_path: String,
    /// "local" | "offline" | "plex" (the source badge).
    pub source: String,
    pub explicit: bool,
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct ArtistRow {
    pub name: String,
    #[serde(rename = "albumCount")]
    pub album_count: u32,
    #[serde(rename = "trackCount")]
    pub track_count: u32,
    #[serde(rename = "artKey")]
    pub art_key: String,
    /// "local" | "plex" | "mixed" — the rail's provenance hint.
    pub source: String,
}

/// One row of the FLATTENED folder tree (the rail renders a windowed list
/// over this array — never a recursive component).
#[derive(Clone, Default, Serialize)]
pub struct TreeNode {
    pub path: String,
    pub segment: String,
    pub depth: i32,
    #[serde(rename = "isFolder")]
    pub is_folder: bool,
    #[serde(rename = "canExpand")]
    pub can_expand: bool,
    pub expanded: bool,
    #[serde(rename = "trackCount")]
    pub track_count: u32,
    #[serde(rename = "artKey")]
    pub art_key: String,
}

#[derive(Clone, Default, Serialize)]
pub struct SubfolderRow {
    pub path: String,
    pub name: String,
    #[serde(rename = "trackCount")]
    pub track_count: u32,
    #[serde(rename = "artKey")]
    pub art_key: String,
}

#[derive(Clone, Default, Serialize)]
pub struct FolderDetail {
    pub path: String,
    pub name: String,
    #[serde(rename = "trackCount")]
    pub track_count: u32,
    pub subfolders: Vec<SubfolderRow>,
    pub tracks: Vec<TrackRow>,
}

#[derive(Clone, Default, Serialize)]
pub struct LocalCounts {
    pub albums: i64,
    pub artists: i64,
    pub folders: i64,
    pub tracks: i64,
    /// Cached Plex tracks folded into `tracks` (0 when Plex is off) — the
    /// header can show "N of them from Plex" without a second query.
    #[serde(rename = "plexTracks")]
    pub plex_tracks: i64,
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

pub fn mmss(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    if m >= 60 {
        format!("{}:{:02}:{:02}", m / 60, m % 60, s)
    } else {
        format!("{m}:{s:02}")
    }
}

/// Human total ("1 h 12 min" / "42 min").
pub fn total_duration(secs: u64) -> String {
    let mins = secs / 60;
    if mins >= 60 {
        format!("{} h {} min", mins / 60, mins % 60)
    } else {
        format!("{mins} min")
    }
}

/// Tier for the shared QualityBadge, mirroring its own derivation order
/// (MP3 first, then max / hires / cd) so a local card and a Qobuz card can
/// never disagree.
pub fn tier_of(format: &AudioFormat, bit_depth: Option<u32>, sample_rate_hz: f64) -> &'static str {
    if matches!(format, AudioFormat::Mp3) {
        return "mp3";
    }
    let khz = if sample_rate_hz >= 1000.0 {
        sample_rate_hz / 1000.0
    } else {
        sample_rate_hz
    };
    match bit_depth {
        Some(b) if b >= 24 && khz > 96.0 => "max",
        Some(b) if b >= 24 => "hires",
        Some(_) => "cd",
        None if khz >= 44.1 => "cd",
        None => "",
    }
}

pub fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

// ---------------------------------------------------------------------------
// Artwork keys (id-keyed windowing: a cover can never land on the wrong row)
// ---------------------------------------------------------------------------

pub fn album_key(id: &str) -> String {
    format!("album:{id}")
}
pub fn track_key(id: i64) -> String {
    format!("track:{id}")
}
pub fn folder_key(path: &str) -> String {
    format!("folder:{path}")
}
pub fn artist_key(name: &str) -> String {
    format!("artist:{name}")
}

/// The badge value for a raw `qbz_library` source column.
pub fn badge_source(raw: Option<&str>) -> String {
    match raw {
        Some("plex") => "plex".into(),
        Some("qobuz_download") | Some("qobuz_purchase") => "offline".into(),
        _ => "local".into(),
    }
}

// ---------------------------------------------------------------------------
// Mappers (they also register the row's artwork SOURCE in the art index —
// a local cover path, or a raw Plex `/library/...` thumb path)
// ---------------------------------------------------------------------------

pub fn map_album(a: LocalAlbum, art: &mut HashMap<String, String>) -> AlbumRow {
    let key = album_key(&a.id);
    if let Some(p) = a.artwork_path.as_ref().filter(|p| !p.is_empty()) {
        art.insert(key.clone(), p.clone());
    }
    let folder_count = a
        .source_folders
        .as_deref()
        .map(|s| s.split(',').filter(|x| !x.trim().is_empty()).count() as u32)
        .unwrap_or(0);
    AlbumRow {
        quality_tier: tier_of(&a.format, a.bit_depth, a.sample_rate).into(),
        quality_detail: crate::home_qt::quality_detail_from_parts(a.bit_depth, Some(a.sample_rate)),
        format: a.format.to_string(),
        year: a.year.map(|y| y.to_string()).unwrap_or_default(),
        duration: total_duration(a.total_duration_secs),
        track_count: a.track_count,
        art_key: key,
        source: badge_source(Some(a.source.as_str())),
        directory_path: a.directory_path,
        all_artists: a.all_artists,
        folder_count,
        id: a.id,
        title: a.title,
        artist: a.artist,
    }
}

pub fn map_track(t: &LocalTrack, art: &mut HashMap<String, String>) -> TrackRow {
    let key = track_key(t.id);
    if let Some(p) = t.artwork_path.as_ref().filter(|p| !p.is_empty()) {
        art.insert(key.clone(), p.clone());
    }
    TrackRow {
        id: t.id.to_string(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album_group_title.clone(),
        album_id: t.album_group_key.clone(),
        artist_id: String::new(),
        number: t.track_number.unwrap_or(0),
        disc: t.disc_number.unwrap_or(1),
        duration: mmss(t.duration_secs),
        quality_tier: tier_of(&t.format, t.bit_depth, t.sample_rate).into(),
        quality_detail: crate::home_qt::quality_detail_from_parts(t.bit_depth, Some(t.sample_rate)),
        format: t.format.to_string(),
        year: t.year.map(|y| y.to_string()).unwrap_or_default(),
        art_key: key,
        art_path: String::new(),
        source: badge_source(t.source.as_deref()),
        explicit: false,
        is_favorite: false,
    }
}

/// The bridge publishes strings, never structs.
pub fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}
