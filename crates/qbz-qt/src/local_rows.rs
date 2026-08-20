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
//!
//! `sourceRaw` rides ALONGSIDE it and is the exception that proves the fold is
//! lossy: a scanned file that matches a `downloaded_purchases` row is stamped
//! `qobuz_purchase` in the DB (`database.rs:1105-1120`), `source` folds that
//! into `"offline"` so it keeps filtering under the Offline chip, and
//! `sourceRaw` carries the word itself so the badge can draw the GOLD Qobuz
//! mark. Two fields because the filter and the badge want different answers
//! from the same column. It is emitted ONLY when it says something new, so on
//! a library with no purchases every published row is byte-identical to what
//! it was before.

use std::collections::HashMap;

use qbz_library::{AudioFormat, LocalAlbum, LocalTrack};
use qbz_source::{ArtRef, ArtSize};
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
    /// `"qobuz_purchase"`, or empty (omitted from the JSON). See the module
    /// header: the badge prefers this, every filter keeps reading `source`.
    #[serde(rename = "sourceRaw", skip_serializing_if = "String::is_empty")]
    pub source_raw: String,
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
    /// `"qobuz_purchase"`, or empty (omitted from the JSON). See the module
    /// header: the badge prefers this, every filter keeps reading `source`.
    #[serde(rename = "sourceRaw", skip_serializing_if = "String::is_empty")]
    pub source_raw: String,
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
///
/// `qobuz_purchase` folds into `"offline"` DELIBERATELY and must keep doing
/// so: this value is what the Local Library's source chips filter on
/// (`LocalLibraryView.qml`'s `applyFilter`), and a purchased album is a Qobuz
/// download — giving it a fourth bucket would make it vanish the moment the
/// user ticks any source chip. The distinction the fold loses is restored
/// beside it by `badge_source_raw`, not by widening this function.
pub fn badge_source(raw: Option<&str>) -> String {
    match raw {
        Some("plex") => "plex".into(),
        Some("qobuz_download") | Some("qobuz_purchase") => "offline".into(),
        _ => "local".into(),
    }
}

/// The raw source word the BADGE needs, or empty when the folded value above
/// already says everything.
///
/// Only `qobuz_purchase` qualifies today: it is the one raw value with a badge
/// of its own (`controls/SourceIcon.qml:75` draws the Qobuz mark in gold for
/// it) that `badge_source` destroys. `"user"` and `"qobuz_download"` are NOT
/// echoed here on purpose — they have no badge the folded value does not
/// already produce, and handing four QML consumers a second spelling of a
/// value they already handle is how the two drift apart.
///
/// Contract §9.4: this word is preserved on SCANNED LOCAL rows only. The
/// remote purchases feed (`library_qt::fetch_purchases`) keeps publishing
/// `"qobuz"` — it never consults the download registry, so stamping it there
/// would badge every purchase the user merely OWNS as locally downloaded.
pub fn badge_source_raw(raw: Option<&str>) -> String {
    match raw {
        Some("qobuz_purchase") => "qobuz_purchase".into(),
        _ => String::new(),
    }
}

/// The [`ArtRef`] for a row's raw artwork token, decided by the row's OWN
/// source (design 02 §9 stage 4).
///
/// The art index used to hold the token as a bare `String`, and
/// `artwork_qt::classify` sniffed it back apart later — by which point the row
/// was gone and all that was left was the shape of the characters. That is bug
/// 3, and its concrete failure is not hypothetical: `classify` knows a Plex
/// thumb only because `local_plex::is_thumb_path` was bolted onto it, so a
/// Jellyfin token (`/Items/<id>/Images/Primary`) matches its `starts_with('/')`
/// arm and the pipeline tries to open it as a file on disk — blank cover, no
/// error, nothing in the log.
///
/// Registering the answer HERE is the fix: this is the last moment the row's
/// provenance exists. `SourceId::from_word` is the one vocabulary table
/// (§3.1), so all six source words land on the right source, and an unknown
/// word gets `None` rather than being guessed into Qobuz.
///
/// Cheap by contract — `Source::artwork_token` may not touch the network, a DB
/// or the filesystem — so this stays a per-row call on a page load.
pub fn art_ref(source_word: Option<&str>, token: &str) -> ArtRef {
    if token.trim().is_empty() {
        return ArtRef::None;
    }
    // The grid's size class. `ArtSize::Card` is `artwork_qt::PLEX_THUMB_PX`
    // (256), which is deliberately ONE app-wide Plex transcode size so a cover
    // is downloaded once — do not introduce a second size here.
    match qbz_source::SourceId::from_word(source_word.unwrap_or("")) {
        Some(id) => qbz_source::registry().artwork_token(id, token, ArtSize::Card),
        // No word at all is the LOCAL case in practice (`local_tracks.source`
        // is empty for a plain scanned file, §3.1's vocabulary table), and it
        // is what `classify` did with those rows too.
        None => qbz_source::registry().artwork_token(
            qbz_source::SourceId::LOCAL,
            token,
            ArtSize::Card,
        ),
    }
}

// ---------------------------------------------------------------------------
// Mappers (they also register the row's artwork REFERENCE in the art index —
// see `art_ref`: the row's own source decides what its token means)
// ---------------------------------------------------------------------------

pub fn map_album(a: LocalAlbum, art: &mut HashMap<String, ArtRef>) -> AlbumRow {
    let key = album_key(&a.id);
    if let Some(p) = a.artwork_path.as_ref().filter(|p| !p.is_empty()) {
        art.insert(key.clone(), art_ref(Some(a.source.as_str()), p));
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
        source_raw: badge_source_raw(Some(a.source.as_str())),
        directory_path: a.directory_path,
        all_artists: a.all_artists,
        folder_count,
        id: a.id,
        title: a.title,
        artist: a.artist,
    }
}

pub fn map_track(t: &LocalTrack, art: &mut HashMap<String, ArtRef>) -> TrackRow {
    let key = track_key(t.id);
    if let Some(p) = t.artwork_path.as_ref().filter(|p| !p.is_empty()) {
        art.insert(key.clone(), art_ref(t.source.as_deref(), p));
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
        source_raw: badge_source_raw(t.source.as_deref()),
        explicit: false,
        is_favorite: false,
    }
}

/// The bridge publishes strings, never structs.
pub fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}
