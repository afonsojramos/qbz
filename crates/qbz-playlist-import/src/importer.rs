//! Orchestrates playlist import

use std::sync::Arc;

use qbz_qobuz::QobuzClient;

use crate::errors::PlaylistImportError;
use crate::match_qobuz::match_tracks;
use crate::models::{ImportPlaylist, ImportProgress, ImportSummary};
use crate::providers::{detect_provider, fetch_playlist};
use crate::sink::{ImportEvent, ImportPhase, ImportProgressSink};

const ADD_CHUNK_SIZE: usize = 50;
const QOBUZ_PLAYLIST_TRACK_LIMIT: usize = 2000;

pub async fn preview_public_playlist(url: &str) -> Result<ImportPlaylist, PlaylistImportError> {
    let provider = detect_provider(url)?;
    fetch_playlist(provider).await
}

pub async fn import_public_playlist(
    url: &str,
    client: &QobuzClient,
    name_override: Option<&str>,
    is_public: bool,
    progress: Arc<dyn ImportProgressSink>,
) -> Result<ImportSummary, PlaylistImportError> {
    let playlist = preview_public_playlist(url).await?;

    // Phase: matching
    progress.emit(ImportEvent::Phase(ImportPhase::Matching));
    let matches = match_tracks(client, &playlist.tracks, Arc::clone(&progress)).await?;

    let mut matched_track_ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in &matches {
        if let Some(id) = entry.qobuz_track_id {
            if seen.insert(id) {
                matched_track_ids.push(id);
            }
        }
    }

    let matched_count = matched_track_ids.len() as u32;
    let total_tracks = playlist.tracks.len() as u32;
    let skipped_tracks = total_tracks.saturating_sub(matched_count);

    let mut qobuz_playlist_ids = Vec::new();

    if !matched_track_ids.is_empty() {
        let base_name = name_override.unwrap_or(&playlist.name);
        let description = playlist
            .description
            .clone()
            .or_else(|| Some(format!("Imported from {}", playlist.provider.as_str())));

        // Split into parts if more than QOBUZ_PLAYLIST_TRACK_LIMIT tracks
        let parts: Vec<&[u64]> = matched_track_ids
            .chunks(QOBUZ_PLAYLIST_TRACK_LIMIT)
            .collect();
        let total_parts = parts.len();

        for (part_idx, part_tracks) in parts.iter().enumerate() {
            // Phase: creating (per part)
            progress.emit(ImportEvent::Phase(ImportPhase::Creating));

            let playlist_name = if total_parts == 1 {
                base_name.to_string()
            } else {
                format!("{} (Part {})", base_name, part_idx + 1)
            };

            let part_desc = if total_parts == 1 {
                description.clone()
            } else {
                Some(format!(
                    "Part {} of {} — {}",
                    part_idx + 1,
                    total_parts,
                    description.as_deref().unwrap_or("")
                ))
            };

            let created = client
                .create_playlist(&playlist_name, part_desc.as_deref(), is_public)
                .await
                .map_err(|e| PlaylistImportError::Qobuz(e.to_string()))?;

            qobuz_playlist_ids.push(created.id);

            // Phase: adding
            progress.emit(ImportEvent::Phase(ImportPhase::Adding));

            let chunks: Vec<&[u64]> = part_tracks.chunks(ADD_CHUNK_SIZE).collect();
            let total_chunks = chunks.len() as u32;

            for (i, chunk) in chunks.iter().enumerate() {
                client
                    .add_tracks_to_playlist(created.id, chunk)
                    .await
                    .map_err(|e| PlaylistImportError::Qobuz(e.to_string()))?;

                progress.emit(ImportEvent::Progress(ImportProgress {
                    phase: "adding".to_string(),
                    current: (i as u32) + 1,
                    total: total_chunks,
                    matched_so_far: matched_count,
                    current_track: if total_parts > 1 {
                        Some(format!("Part {}/{}", part_idx + 1, total_parts))
                    } else {
                        None
                    },
                }));
            }
        }
    }

    let parts_created = qobuz_playlist_ids.len() as u32;

    Ok(ImportSummary {
        provider: playlist.provider,
        // Deliberate fix vs the Tauri original (owner decision): the summary
        // reports the name the playlist was actually created under — the
        // rename when one was given — not the original source name.
        playlist_name: match name_override {
            Some(name) => name.to_string(),
            None => playlist.name,
        },
        total_tracks,
        matched_tracks: matched_count,
        skipped_tracks,
        qobuz_playlist_ids,
        parts_created,
        matches,
    })
}
