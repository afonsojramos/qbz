//! Qt local-album metadata editor controller.
//!
//! The selected physical version is snapshotted on open. QML edits a bounded
//! DTO and returns row ids only; paths are always resolved against this Rust
//! snapshot before sidecar or embedded-tag writes.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use cxx_qt_lib::QString;
use qbz_library::{
    AlbumMetadataOverride, AlbumTagInspection, AlbumTagSidecar, AlbumTagWrite,
    AlbumTrackUpdate, DirectTagWriteOptions, Id3v2WriteVersion, LocalTrack,
    TrackMetadataOverride, TrackTagWrite,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct EditorSession {
    generation: u64,
    album_id: String,
    group_key: String,
    directory: String,
    tracks: Vec<LocalTrack>,
}

static SESSION: OnceLock<Mutex<Option<EditorSession>>> = OnceLock::new();
static OPEN_GEN: AtomicU64 = AtomicU64::new(0);
static SAVE_GEN: AtomicU64 = AtomicU64::new(0);
static REMOTE_GEN: AtomicU64 = AtomicU64::new(0);
static REMOTE_SEQ: AtomicI32 = AtomicI32::new(0);
static SAVE_ACTIVE: AtomicBool = AtomicBool::new(false);

fn session() -> &'static Mutex<Option<EditorSession>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TagLayerDoc {
    name: String,
    file_count: usize,
    canonical_file_count: usize,
    writable_file_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectionDoc {
    file_count: usize,
    canonical_layers: Vec<TagLayerDoc>,
    present_layers: Vec<TagLayerDoc>,
    conflicting_files: usize,
    writable_files: usize,
    direct_write_supported: bool,
    error: String,
}

impl InspectionDoc {
    fn from_result(result: Result<AlbumTagInspection, qbz_library::LibraryError>) -> Self {
        match result {
            Ok(inspection) => {
                let direct_write_supported = inspection.direct_write_supported();
                Self {
                    file_count: inspection.file_count,
                    canonical_layers: inspection
                        .canonical_layers
                        .into_iter()
                        .map(|layer| TagLayerDoc {
                            name: layer.name,
                            file_count: layer.file_count,
                            canonical_file_count: layer.canonical_file_count,
                            writable_file_count: layer.writable_file_count,
                        })
                        .collect(),
                    present_layers: inspection
                        .present_layers
                        .into_iter()
                        .map(|layer| TagLayerDoc {
                            name: layer.name,
                            file_count: layer.file_count,
                            canonical_file_count: layer.canonical_file_count,
                            writable_file_count: layer.writable_file_count,
                        })
                        .collect(),
                    conflicting_files: inspection.conflicting_files,
                    writable_files: inspection.writable_files,
                    direct_write_supported,
                    error: String::new(),
                }
            }
            Err(error) => Self {
                file_count: 0,
                canonical_layers: Vec::new(),
                present_layers: Vec::new(),
                conflicting_files: 0,
                writable_files: 0,
                direct_write_supported: false,
                error: error.to_string(),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackSeed {
    id: String,
    file_name: String,
    title: String,
    track_number: String,
    disc_number: String,
    cue_based: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EditorSeed {
    album_id: String,
    group_key: String,
    directory: String,
    album_title: String,
    album_artist: String,
    year: String,
    genre: String,
    catalog_number: String,
    total_discs: u32,
    sidecar_exists: bool,
    can_direct_write: bool,
    direct_write_reason: String,
    inspection: InspectionDoc,
    tracks: Vec<TrackSeed>,
}

fn build_seed(open: &EditorSession) -> EditorSeed {
    let tracks = &open.tracks;
    let cue_based = tracks
        .iter()
        .any(|track| track.cue_file_path.is_some() || track.cue_start_secs.is_some());
    let paths = tracks
        .iter()
        .map(|track| track.file_path.clone())
        .collect::<Vec<_>>();
    let inspection = InspectionDoc::from_result(qbz_library::inspect_album_tag_layers(&paths));
    let local_files = tracks
        .iter()
        .all(|track| Path::new(&track.file_path).is_file());
    let can_direct_write = !cue_based && local_files && inspection.direct_write_supported;
    let direct_write_reason = if cue_based {
        qbz_i18n::t("CUE-based albums use sidecar metadata.")
    } else if !local_files {
        qbz_i18n::t("One or more audio files are unavailable.")
    } else if !inspection.error.is_empty() {
        inspection.error.clone()
    } else if !inspection.direct_write_supported {
        qbz_i18n::t("The canonical tag is not writable for every file.")
    } else {
        String::new()
    };

    let first = tracks.first();
    let album_title = first
        .map(|track| {
            if track.album_group_title.trim().is_empty() {
                track.album.trim().to_string()
            } else {
                track.album_group_title.trim().to_string()
            }
        })
        .unwrap_or_default();
    let album_artist = qbz_library::compute_track_artist_match(tracks).unwrap_or_default();
    let year = tracks
        .iter()
        .find_map(|track| track.year)
        .map(|year| year.to_string())
        .unwrap_or_default();
    let genre = tracks
        .iter()
        .find_map(|track| track.genre.as_ref().filter(|value| !value.trim().is_empty()))
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let catalog_number = tracks
        .iter()
        .find_map(|track| {
            track
                .catalog_number
                .as_ref()
                .filter(|value| !value.trim().is_empty())
        })
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let total_discs = tracks
        .iter()
        .filter_map(|track| track.disc_number)
        .max()
        .unwrap_or(1)
        .max(1);

    EditorSeed {
        album_id: open.album_id.clone(),
        group_key: open.group_key.clone(),
        directory: open.directory.clone(),
        album_title,
        album_artist,
        year,
        genre,
        catalog_number,
        total_discs,
        sidecar_exists: qbz_library::sidecar_path(Path::new(&open.directory)).exists(),
        can_direct_write,
        direct_write_reason,
        inspection,
        tracks: tracks
            .iter()
            .map(|track| TrackSeed {
                id: track.id.to_string(),
                file_name: Path::new(&track.file_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&track.file_path)
                    .to_string(),
                title: track.title.clone(),
                track_number: track
                    .track_number
                    .map(|number| number.to_string())
                    .unwrap_or_default(),
                disc_number: track
                    .disc_number
                    .map(|number| number.to_string())
                    .unwrap_or_default(),
                cue_based: track.cue_file_path.is_some() || track.cue_start_secs.is_some(),
            })
            .collect(),
    }
}

pub fn open(album_id: String) {
    if SAVE_ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let tracks = crate::local_album_actions::current_version_tracks();
    let group_key = crate::local_album_actions::current_version_dir();
    if tracks.is_empty() || group_key.trim().is_empty() {
        crate::toast_qt::error(qbz_i18n::t("No local album version is selected."));
        return;
    }
    if !Path::new(&group_key).is_dir() {
        crate::toast_qt::info(qbz_i18n::t(
            "Metadata editing is available for local file versions only.",
        ));
        return;
    }

    let generation = OPEN_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let open = EditorSession {
        generation,
        album_id,
        group_key: group_key.clone(),
        directory: group_key,
        tracks,
    };
    *session().lock().expect("tag editor session lock") = Some(open.clone());
    crate::tag_editor_bridge::ui(|mut bridge| {
        bridge.as_mut().set_editor_open(true);
        bridge.as_mut().set_editor_loading(true);
        bridge.as_mut().set_editor_saving(false);
        bridge.as_mut().set_editor_progress_current(0);
        bridge.as_mut().set_editor_progress_total(0);
        bridge.as_mut().set_editor_json(QString::from("{}"));
        bridge.as_mut().set_remote_json(QString::from("{}"));
        bridge.as_mut().set_remote_searching(false);
        bridge.as_mut().set_remote_loading(false);
    });

    crate::spawn(async move {
        let seed = tokio::task::spawn_blocking(move || build_seed(&open)).await;
        if OPEN_GEN.load(Ordering::SeqCst) != generation {
            return;
        }
        match seed {
            Ok(seed) => {
                let json = serde_json::to_string(&seed).unwrap_or_else(|_| "{}".to_string());
                crate::tag_editor_bridge::ui(move |mut bridge| {
                    bridge
                        .as_mut()
                        .set_editor_json(QString::from(json.as_str()));
                    bridge.as_mut().set_editor_loading(false);
                });
            }
            Err(error) => {
                log::error!("[qbz-qt] tag editor preflight task failed: {error}");
                crate::tag_editor_bridge::ui(|mut bridge| {
                    bridge.as_mut().set_editor_loading(false);
                    bridge.as_mut().set_editor_open(false);
                });
                crate::toast_qt::error(qbz_i18n::t("Couldn't inspect album metadata."));
            }
        }
    });
}

pub fn close() {
    if SAVE_ACTIVE.load(Ordering::Acquire) {
        return;
    }
    OPEN_GEN.fetch_add(1, Ordering::SeqCst);
    REMOTE_GEN.fetch_add(1, Ordering::SeqCst);
    *session().lock().expect("tag editor session lock") = None;
    crate::tag_editor_bridge::ui(|mut bridge| {
        bridge.as_mut().set_editor_open(false);
        bridge.as_mut().set_editor_loading(false);
        bridge.as_mut().set_remote_searching(false);
        bridge.as_mut().set_remote_loading(false);
    });
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TrackDraft {
    id: String,
    title: String,
    track_number: String,
    disc_number: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SaveDraft {
    album_title: String,
    album_artist: String,
    year: String,
    genre: String,
    catalog_number: String,
    persistence: String,
    id3v2_version: String,
    synchronize_secondary_tags: bool,
    tracks: Vec<TrackDraft>,
}

fn parse_optional_number(value: &str, field: &str) -> Result<Option<u32>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    match value.parse::<u32>() {
        Ok(number) if (1..=9999).contains(&number) => Ok(Some(number)),
        _ => Err(qbz_i18n::t_args("{} must be a positive number.", &[field])),
    }
}

fn parse_year(value: &str) -> Result<Option<u32>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    match value.parse::<u32>() {
        Ok(year) if (1..=3000).contains(&year) => Ok(Some(year)),
        _ => Err(qbz_i18n::t("Year must be a valid number.")),
    }
}

struct SavePayload {
    session: EditorSession,
    album: AlbumTagWrite,
    direct_tracks: Vec<TrackTagWrite>,
    db_updates: Vec<AlbumTrackUpdate>,
    sidecar: AlbumTagSidecar,
    direct: bool,
    options: DirectTagWriteOptions,
    prior_artist: Option<String>,
}

fn validate_draft(draft: SaveDraft, open: EditorSession) -> Result<SavePayload, String> {
    let album_title = draft.album_title.trim().to_string();
    if album_title.is_empty() {
        return Err(qbz_i18n::t("Album title is required."));
    }
    let year = parse_year(&draft.year)?;
    let direct = draft.persistence == "direct";
    if !matches!(draft.persistence.as_str(), "sidecar" | "direct") {
        return Err(qbz_i18n::t("Unknown metadata persistence mode."));
    }
    if direct
        && open
            .tracks
            .iter()
            .any(|track| track.cue_file_path.is_some() || track.cue_start_secs.is_some())
    {
        return Err(qbz_i18n::t(
            "CUE-based albums can only be edited with a sidecar.",
        ));
    }

    let mut incoming = HashMap::<i64, TrackDraft>::new();
    for row in draft.tracks {
        let id = row
            .id
            .parse::<i64>()
            .map_err(|_| qbz_i18n::t("The track list changed; reopen the editor."))?;
        if incoming.insert(id, row).is_some() {
            return Err(qbz_i18n::t("The track list contains a duplicate row."));
        }
    }
    let expected_ids = open.tracks.iter().map(|track| track.id).collect::<HashSet<_>>();
    if incoming.len() != expected_ids.len()
        || incoming.keys().any(|id| !expected_ids.contains(id))
    {
        return Err(qbz_i18n::t("The track list changed; reopen the editor."));
    }

    let album_artist = draft.album_artist.trim().to_string();
    let genre = draft.genre.trim().to_string();
    let catalog = draft.catalog_number.trim().to_string();
    let mut direct_tracks = Vec::with_capacity(open.tracks.len());
    let mut db_updates = Vec::with_capacity(open.tracks.len());
    let mut sidecar_tracks = Vec::with_capacity(open.tracks.len());
    for track in &open.tracks {
        let row = incoming
            .remove(&track.id)
            .expect("validated tag editor row set");
        let title = row.title.trim().to_string();
        if title.is_empty() {
            return Err(qbz_i18n::t("Every track needs a title."));
        }
        let track_number = parse_optional_number(&row.track_number, &qbz_i18n::t("Track number"))?;
        let disc_number = parse_optional_number(&row.disc_number, &qbz_i18n::t("Disc number"))?;
        direct_tracks.push(TrackTagWrite {
            file_path: track.file_path.clone(),
            title: title.clone(),
            track_number,
            disc_number,
        });
        db_updates.push(AlbumTrackUpdate {
            id: track.id,
            title: title.clone(),
            track_number,
            disc_number,
        });
        // Sidecar v1 uses explicit sentinels for clears. Empty/None used to
        // mean "no override", which made a cleared field reappear on rescan.
        sidecar_tracks.push(TrackMetadataOverride {
            file_path: track.file_path.clone(),
            cue_start_secs: track.cue_start_secs,
            title: Some(title),
            track_number: Some(track_number.unwrap_or(0)),
            disc_number: Some(disc_number.unwrap_or(0)),
        });
    }

    let sidecar = AlbumTagSidecar::new(
        AlbumMetadataOverride {
            album_title: Some(album_title.clone()),
            album_artist: Some(album_artist.clone()),
            year: Some(year.unwrap_or(0)),
            genre: Some(genre.clone()),
            catalog_number: Some(catalog.clone()),
        },
        sidecar_tracks,
    );
    let album = AlbumTagWrite {
        album_title,
        album_artist,
        year,
        genre: (!genre.is_empty()).then_some(genre),
        catalog_number: (!catalog.is_empty()).then_some(catalog),
    };
    let options = DirectTagWriteOptions {
        id3v2_version: if draft.id3v2_version == "2.3" {
            Id3v2WriteVersion::V23
        } else {
            Id3v2WriteVersion::V24
        },
        synchronize_secondary_tags: draft.synchronize_secondary_tags,
    };
    let prior_artist = qbz_library::compute_track_artist_match(&open.tracks);
    Ok(SavePayload {
        session: open,
        album,
        direct_tracks,
        db_updates,
        sidecar,
        direct,
        options,
        prior_artist,
    })
}

fn apply_database_update(payload: &SavePayload) -> Result<Vec<LocalTrack>, qbz_library::LibraryError> {
    let path = crate::local_state::db_path()
        .ok_or_else(|| qbz_library::LibraryError::Database("library path unavailable".to_string()))?;
    let mut db = qbz_library::LibraryDatabase::open(&path)?;
    db.update_album_group_metadata(
        &payload.session.group_key,
        &payload.album.album_title,
        &payload.album.album_artist,
        payload.album.year,
        payload.album.genre.as_deref(),
        payload.album.catalog_number.as_deref(),
        payload.prior_artist.as_deref(),
        &payload.db_updates,
    )?;
    db.get_album_tracks(&payload.session.group_key)
}

pub fn save(draft_json: &str) {
    if SAVE_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let parsed = serde_json::from_str::<SaveDraft>(draft_json)
        .map_err(|_| qbz_i18n::t("The metadata form could not be read."));
    let open = session().lock().expect("tag editor session lock").clone();
    let payload = match parsed.and_then(|draft| {
        open.ok_or_else(|| qbz_i18n::t("The metadata editor is no longer open."))
            .and_then(|open| validate_draft(draft, open))
    }) {
        Ok(payload) => payload,
        Err(error) => {
            SAVE_ACTIVE.store(false, Ordering::Release);
            crate::toast_qt::error(error);
            return;
        }
    };
    let save_generation = SAVE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let open_generation = payload.session.generation;
    crate::tag_editor_bridge::ui(|mut bridge| {
        bridge.as_mut().set_editor_saving(true);
        bridge.as_mut().set_editor_progress_current(0);
        bridge
            .as_mut()
            .set_editor_progress_total(0);
    });

    crate::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            if payload.direct {
                let total = payload.direct_tracks.len() as i32;
                crate::tag_editor_bridge::ui(move |mut bridge| {
                    bridge.as_mut().set_editor_progress_total(total);
                });
                qbz_library::write_album_tags_to_files_with_options(
                    &payload.album,
                    &payload.direct_tracks,
                    payload.options,
                    |current, total| {
                        crate::tag_editor_bridge::ui(move |mut bridge| {
                            bridge
                                .as_mut()
                                .set_editor_progress_current(current as i32);
                            bridge.as_mut().set_editor_progress_total(total as i32);
                        });
                    },
                )?;
                // A sidecar is removed only after every file was verified.
                qbz_library::delete_album_sidecar(Path::new(&payload.session.directory))?;
            } else {
                qbz_library::write_album_sidecar(
                    Path::new(&payload.session.directory),
                    &payload.sidecar,
                )?;
            }
            let tracks = apply_database_update(&payload)?;
            Ok::<_, qbz_library::LibraryError>((payload.session.album_id, tracks))
        })
        .await
        .unwrap_or_else(|error| {
            Err(qbz_library::LibraryError::Other(format!(
                "metadata save task failed: {error}"
            )))
        });

        SAVE_ACTIVE.store(false, Ordering::Release);
        if SAVE_GEN.load(Ordering::SeqCst) != save_generation {
            return;
        }
        crate::tag_editor_bridge::ui(|mut bridge| {
            bridge.as_mut().set_editor_saving(false);
            bridge.as_mut().set_editor_progress_current(0);
            bridge.as_mut().set_editor_progress_total(0);
        });
        match result {
            Ok((album_id, tracks)) => {
                // Publish from the authoritative DB rows directly. In metadata
                // grouping the logical id may change as part of this edit, so
                // re-querying by the old id would close a successful save.
                if let Some(doc) = crate::local_album_actions::open_versions(&album_id, tracks) {
                    let json = crate::local_rows::to_json(&doc);
                    crate::local_bridge::ui(move |mut bridge| {
                        bridge
                            .as_mut()
                            .set_local_album_json(QString::from(json.as_str()));
                        bridge.as_mut().set_local_album_loading(false);
                    });
                }
                crate::local_bridge_ops::reload_browse();
                if OPEN_GEN.load(Ordering::SeqCst) == open_generation {
                    close();
                }
                crate::toast_qt::success(qbz_i18n::t("Album metadata saved."));
            }
            Err(error) => {
                log::error!("[qbz-qt] metadata save failed: {error}");
                crate::toast_qt::error(qbz_i18n::t_args(
                    "Couldn't save metadata: {}",
                    &[&error.to_string()],
                ));
            }
        }
    });
}

fn publish_remote(kind: &str, value: serde_json::Value) {
    let sequence = REMOTE_SEQ.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let json = serde_json::json!({ "kind": kind, "value": value }).to_string();
    crate::tag_editor_bridge::ui(move |mut bridge| {
        bridge
            .as_mut()
            .set_remote_json(QString::from(json.as_str()));
        bridge.as_mut().set_remote_seq(sequence);
    });
}

pub fn search_remote(provider: &str, title: &str, artist: &str) {
    let provider = provider.to_string();
    let title = title.trim().to_string();
    let artist = artist.trim().to_string();
    if title.is_empty() && artist.is_empty() {
        crate::toast_qt::error(qbz_i18n::t("Enter an album title or artist to search."));
        return;
    }
    let generation = REMOTE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    crate::tag_editor_bridge::ui(|mut bridge| {
        bridge.as_mut().set_remote_searching(true);
        bridge.as_mut().set_remote_loading(false);
    });
    crate::spawn(async move {
        let result: Result<Vec<qbz_integrations::RemoteAlbumSearchResult>, String> =
            if provider == "discogs" {
                let client = qbz_integrations::DiscogsClient::new();
                client.search_releases(&artist, &title, None, 12).await.map(|rows| {
                    rows.iter()
                        .map(qbz_integrations::discogs_extended_to_search_result)
                        .collect()
                })
            } else {
                let client = qbz_integrations::MusicBrainzClient::new();
                client
                    .search_releases_extended(&title, &artist, None, 12)
                    .await
                    .map(|response| {
                        response
                            .releases
                            .iter()
                            .map(qbz_integrations::musicbrainz_release_to_search_result)
                            .collect()
                    })
                    .map_err(|error| error.to_string())
            };
        if REMOTE_GEN.load(Ordering::SeqCst) != generation {
            return;
        }
        crate::tag_editor_bridge::ui(|mut bridge| {
            bridge.as_mut().set_remote_searching(false);
        });
        match result {
            Ok(rows) => publish_remote(
                "results",
                serde_json::to_value(rows).unwrap_or_else(|_| serde_json::json!([])),
            ),
            Err(error) => {
                log::warn!("[qbz-qt] metadata search failed: {error}");
                publish_remote("results", serde_json::json!([]));
                crate::toast_qt::error(qbz_i18n::t("Metadata search failed."));
            }
        }
    });
}

pub fn load_remote(provider: &str, provider_id: &str) {
    let provider = provider.to_string();
    let provider_id = provider_id.to_string();
    if provider_id.trim().is_empty() {
        return;
    }
    let generation = REMOTE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    crate::tag_editor_bridge::ui(|mut bridge| {
        bridge.as_mut().set_remote_loading(true);
    });
    crate::spawn(async move {
        let result: Result<qbz_integrations::RemoteAlbumMetadata, String> = if provider == "discogs" {
            match provider_id.parse::<u64>() {
                Ok(id) => {
                    let client = qbz_integrations::DiscogsClient::new();
                    client
                        .get_release_metadata(id)
                        .await
                        .map(|metadata| qbz_integrations::discogs_full_to_metadata(&metadata))
                }
                Err(_) => Err("invalid Discogs release id".to_string()),
            }
        } else {
            let client = qbz_integrations::MusicBrainzClient::new();
            client
                .get_release_with_tracks(&provider_id)
                .await
                .map(|release| qbz_integrations::musicbrainz_full_to_metadata(&release))
                .map_err(|error| error.to_string())
        };
        if REMOTE_GEN.load(Ordering::SeqCst) != generation {
            return;
        }
        crate::tag_editor_bridge::ui(|mut bridge| {
            bridge.as_mut().set_remote_loading(false);
        });
        match result {
            Ok(metadata) => publish_remote(
                "metadata",
                serde_json::to_value(metadata).unwrap_or_else(|_| serde_json::json!({})),
            ),
            Err(error) => {
                log::warn!("[qbz-qt] metadata result load failed: {error}");
                crate::toast_qt::error(qbz_i18n::t("Couldn't load that metadata result."));
            }
        }
    });
}

pub fn open_remote(provider: &str, provider_id: &str) {
    let url = if provider == "discogs" {
        format!("https://www.discogs.com/release/{provider_id}")
    } else {
        format!("https://musicbrainz.org/release/{provider_id}")
    };
    if open::that(url).is_err() {
        crate::toast_qt::error(qbz_i18n::t("Couldn't open the metadata source."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_fixture() -> EditorSession {
        EditorSession {
            generation: 7,
            album_id: "album:test".to_string(),
            group_key: "/library/album".to_string(),
            directory: "/library/album".to_string(),
            tracks: vec![
                LocalTrack {
                    id: 10,
                    file_path: "/library/album/01.flac".to_string(),
                    title: "One".to_string(),
                    artist: "Artist".to_string(),
                    album_artist: Some("Artist".to_string()),
                    ..LocalTrack::default()
                },
                LocalTrack {
                    id: 20,
                    file_path: "/library/album/02.flac".to_string(),
                    title: "Two".to_string(),
                    artist: "Artist".to_string(),
                    album_artist: Some("Artist".to_string()),
                    ..LocalTrack::default()
                },
            ],
        }
    }

    fn draft_json(rows: &str) -> String {
        format!(
            r#"{{
                "albumTitle":"Album",
                "albumArtist":"Artist",
                "year":"2026",
                "genre":"Rock",
                "catalogNumber":"CAT-1",
                "persistence":"sidecar",
                "id3v2Version":"2.4",
                "synchronizeSecondaryTags":false,
                "tracks":{rows}
            }}"#
        )
    }

    #[test]
    fn optional_numbers_are_strict_and_blank_clears() {
        assert_eq!(parse_optional_number("", "Track").unwrap(), None);
        assert_eq!(parse_optional_number(" 12 ", "Track").unwrap(), Some(12));
        assert!(parse_optional_number("0", "Track").is_err());
        assert!(parse_optional_number("nope", "Track").is_err());
    }

    #[test]
    fn year_validation_does_not_accept_sentinels_from_qml() {
        assert_eq!(parse_year("").unwrap(), None);
        assert_eq!(parse_year("2026").unwrap(), Some(2026));
        assert!(parse_year("0").is_err());
        assert!(parse_year("3001").is_err());
    }

    #[test]
    fn draft_ids_resolve_only_to_the_snapshotted_paths() {
        let draft: SaveDraft = serde_json::from_str(&draft_json(
            r#"[
                {"id":"20","title":"Second","trackNumber":"2","discNumber":"1"},
                {"id":"10","title":"First","trackNumber":"1","discNumber":"1"}
            ]"#,
        ))
        .unwrap();

        let payload = validate_draft(draft, session_fixture()).unwrap();
        assert_eq!(payload.direct_tracks[0].file_path, "/library/album/01.flac");
        assert_eq!(payload.direct_tracks[0].title, "First");
        assert_eq!(payload.direct_tracks[1].file_path, "/library/album/02.flac");
        assert_eq!(payload.direct_tracks[1].title, "Second");
    }

    #[test]
    fn injected_rows_or_paths_are_rejected() {
        let injected_id: SaveDraft = serde_json::from_str(&draft_json(
            r#"[
                {"id":"10","title":"First","trackNumber":"1","discNumber":"1"},
                {"id":"999","title":"Injected","trackNumber":"2","discNumber":"1"}
            ]"#,
        ))
        .unwrap();
        assert!(validate_draft(injected_id, session_fixture()).is_err());

        let injected_path = draft_json(
            r#"[
                {"id":"10","filePath":"/tmp/other.flac","title":"First","trackNumber":"1","discNumber":"1"},
                {"id":"20","title":"Second","trackNumber":"2","discNumber":"1"}
            ]"#,
        );
        assert!(serde_json::from_str::<SaveDraft>(&injected_path).is_err());
    }
}
