//! Recently-played store — Slint-free port of `crates/qbz/src/recently.rs`.
//!
//! Backs the two Discover rails the port was missing: "Recently Played
//! Albums" (album carousel) and "Recently Played Tracks" (slim carousel), on
//! Home AND For You.
//!
//! It is the SAME file the Slint and Tauri builds use —
//! `<data_dir>/qbz/recently_played.json`, an object
//! `{ "tracks": [...], "albums": [...] }` with the pre-#567 bare-array form
//! migrated on read by deriving the album list from the track window. Nothing
//! here is per-user: the path is app-wide, exactly as in the reference, so a
//! history recorded by the Slint build shows up here and vice versa.
//!
//! Read is free (one small JSON file, no network, no session). [`record`] is
//! the write side; its call site is the playback track-start edge, which lives
//! in `playback_qt.rs` — see the GLUE note on the function.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// How many recent tracks to keep (recently.rs `MAX_RECENT`).
const MAX_RECENT: usize = 24;
/// Independent album cap (#567) so a run of long albums cannot shrink the
/// distinct-album history.
const MAX_RECENT_ALBUMS: usize = 24;

/// One recently-played track. Every field defaults so a store written by an
/// older build (or a newer one with more fields) stays readable.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecentTrack {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub album_id: String,
    #[serde(default)]
    pub album_title: String,
    #[serde(default)]
    pub album_artist: String,
    #[serde(default)]
    pub album_artwork_url: String,
    #[serde(default)]
    pub quality_tier: String,
    #[serde(default)]
    pub quality_label: String,
    #[serde(default)]
    pub genre: String,
    /// Raw ISO album release date; localized at render time.
    #[serde(default)]
    pub release_date: String,
    #[serde(default)]
    pub artist_id: Option<u64>,
    /// "qobuz" | "plex" | "local"; empty = legacy entry, treated as Qobuz.
    #[serde(default)]
    pub source: String,
}

/// One recently-played album (its own history since #567).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecentAlbum {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub quality_tier: String,
    #[serde(default)]
    pub quality_label: String,
    #[serde(default)]
    pub genre: String,
    #[serde(default)]
    pub release_date: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentStore {
    #[serde(default)]
    tracks: Vec<RecentTrack>,
    #[serde(default)]
    albums: Vec<RecentAlbum>,
}

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("recently_played.json"))
}

/// Pre-#567 derive: first-occurrence de-dup over the track window. Kept ONLY
/// for the legacy migration path in [`read_store`].
fn derive_albums(tracks: &[RecentTrack]) -> Vec<RecentAlbum> {
    let mut albums: Vec<RecentAlbum> = Vec::new();
    for track in tracks {
        if track.album_id.is_empty() || albums.iter().any(|a| a.id == track.album_id) {
            continue;
        }
        albums.push(RecentAlbum {
            id: track.album_id.clone(),
            title: track.album_title.clone(),
            artist: track.album_artist.clone(),
            artwork_url: track.album_artwork_url.clone(),
            quality_tier: track.quality_tier.clone(),
            quality_label: track.quality_label.clone(),
            genre: track.genre.clone(),
            release_date: track.release_date.clone(),
            source: track.source.clone(),
        });
    }
    albums
}

/// True when a stored track id names an EPHEMERAL (in-memory, session-only)
/// track and therefore must never be persisted here.
///
/// Ephemeral ids are synthetic and `>= EPHEMERAL_ID_FLOOR` (2^48). Qobuz ids
/// and `local_tracks` row ids parse far below the floor; Plex rating_keys and
/// the empty id do not parse as `i64` at all — all of those keep recording.
fn is_ephemeral_track_id(id: &str) -> bool {
    id.parse::<i64>()
        .is_ok_and(crate::local_ephemeral::is_ephemeral_id)
}

/// Read the whole store. Missing / unreadable -> empty (the rails then render
/// their placeholders, never an error).
fn read_store() -> RecentStore {
    let Some(path) = store_path() else {
        return RecentStore::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return RecentStore::default();
    };
    if let Ok(store) = serde_json::from_slice::<RecentStore>(&bytes) {
        return store;
    }
    let tracks: Vec<RecentTrack> = serde_json::from_slice(&bytes).unwrap_or_default();
    let albums = derive_albums(&tracks);
    RecentStore { tracks, albums }
}

fn write_store(store: &RecentStore) {
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("[qbz-qt] recently-played store dir failed: {e}");
            return;
        }
    }
    match serde_json::to_vec_pretty(store) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("[qbz-qt] recently-played write failed: {e}");
            }
        }
        Err(e) => log::warn!("[qbz-qt] recently-played serialize failed: {e}"),
    }
}

/// Recently-played tracks, newest first.
pub(crate) fn load_tracks() -> Vec<RecentTrack> {
    read_store().tracks
}

/// Recently-played albums, newest first.
pub(crate) fn load_albums() -> Vec<RecentAlbum> {
    read_store().albums
}

/// Drop every entry whose `album_id` is in `album_ids`; returns how many
/// TRACK entries went.
///
/// Called when a Local Library folder is removed, so its albums stop
/// lingering in Recently Played as rows that open onto nothing. Without it
/// the history keeps pointing at tracks whose database rows are gone —
/// visible as dead cards, not as an error.
///
/// Port of `crates/qbz/src/recently.rs:236-250` with ONE structural
/// difference: this store derives its album list from the track window
/// (`derive_albums`), so pruning the tracks is enough and the reference's
/// second `store.albums.retain` has no counterpart here. The `albums` field
/// still exists for the legacy migration path, so it is pruned too rather
/// than left holding stale ids.
pub(crate) fn prune_albums(album_ids: &[String]) -> usize {
    if album_ids.is_empty() {
        return 0;
    }
    let mut store = read_store();
    let tracks_before = store.tracks.len();
    let albums_before = store.albums.len();
    store
        .tracks
        .retain(|t| !album_ids.iter().any(|k| k == &t.album_id));
    store.albums.retain(|a| !album_ids.iter().any(|k| k == &a.id));
    let removed = tracks_before - store.tracks.len();
    if removed > 0 || albums_before != store.albums.len() {
        write_store(&store);
    }
    removed
}

/// Record a played track at the front of the track history (dedup by track
/// id, capped at [`MAX_RECENT`]) and its album at the front of the album
/// history (dedup by album id, capped at [`MAX_RECENT_ALBUMS`]).
///
/// WIRED: the only caller is [`record_queue_track`], from the de-duped
/// playback track-start edge in `playback_qt.rs`. Ephemeral ids are rejected
/// here as well as there — see [`is_ephemeral_track_id`].
#[allow(dead_code)]
pub(crate) fn record(track: RecentTrack) {
    if track.id.is_empty() {
        return;
    }
    // EPHEMERAL — store invariant, not just a call-site courtesy. An ephemeral
    // id must never reach this JSON, whoever calls. Real ids parse well below
    // `EPHEMERAL_ID_FLOOR` (2^48) and Plex rating_keys do not parse as i64 at
    // all, so this rejects exactly the synthetic ids and nothing else. See the
    // long note on `record_queue_track` for the why.
    if is_ephemeral_track_id(&track.id) {
        return;
    }
    let mut store = read_store();
    if !track.album_id.is_empty() {
        store.albums.retain(|a| a.id != track.album_id);
        store.albums.insert(
            0,
            RecentAlbum {
                id: track.album_id.clone(),
                title: track.album_title.clone(),
                artist: track.album_artist.clone(),
                artwork_url: track.album_artwork_url.clone(),
                quality_tier: track.quality_tier.clone(),
                quality_label: track.quality_label.clone(),
                genre: track.genre.clone(),
                release_date: track.release_date.clone(),
                source: track.source.clone(),
            },
        );
        store.albums.truncate(MAX_RECENT_ALBUMS);
    }
    store.tracks.retain(|t| t.id != track.id);
    store.tracks.insert(0, track);
    store.tracks.truncate(MAX_RECENT);
    write_store(&store);
}

/// Record the current queue track into BOTH local play stores: the
/// recently-played history (this module) and the album play-count history
/// (`qbz_app::settings::album_play_history`, which ranks the "Most Played
/// Albums" rail). Keeps the playback glue a single line.
///
/// The queue model carries no album genre / release date (Slint stamps those
/// from a side `ALBUM_META` map filled by the album-fetch path), so a card
/// recorded here shows title / artist / artwork / quality and no genre-date
/// overlay line — never a wrong one.
///
/// WIRED: called from the DE-DUPED playback track-change edge in
/// `playback_qt::start_poll_loop`, next to
/// `integrations_qt::on_track_change_edge`. Both this rail and the "Most
/// Played Albums" rail therefore contain LOCAL and PLEX rows (the album play
/// history stores `source` verbatim and never filters on it), which is why
/// every album play/enqueue entry point has to route a group key to the local
/// path instead of `get_album` — see `playback_qt::is_local_album`.
pub(crate) fn record_queue_track(track: &qbz_models::QueueTrack) {
    // EPHEMERAL — owner rule: "ephemeral solo se reproduce y ya […] honramos el
    // nombre ephemeral: no deja rastros en las bibliotecas, ni now playing ni
    // nada." An ephemeral folder lives in memory for one session and may be
    // added to the QUEUE and to nothing else. Recording it here would write it
    // into TWO app-wide persistent stores that outlive the process —
    // `recently_played.json` (Recently Played Albums / Tracks) and
    // `album_play_history.db` (Most Played Albums) — naming a folder that is
    // not in the library, under synthetic ids that are meaningless after
    // restart. Neither store can express "temporary", so the only correct move
    // is to never write. Guarded HERE, above both writes, so any future caller
    // inherits the rule; the poll-loop call site skips it earlier too, purely
    // to save the queue-state fetch.
    //
    // NOTE: the Slint reference (`qbz/src/playback.rs::record_recent`) has no
    // such guard — this is the rule being implemented for the first time, not
    // a Qt-only regression being patched.
    if crate::local_ephemeral::is_ephemeral_id(track.id as i64) {
        log::debug!(
            "[qbz-qt] ephemeral track {} not recorded in play history (no library traces)",
            track.id
        );
        return;
    }
    let tier = crate::home_qt::quality_tier_from_depth(track.bit_depth);
    let detail = crate::home_qt::quality_detail_from_parts(track.bit_depth, track.sample_rate);
    let quality_label = if tier.is_empty() {
        String::new()
    } else {
        format!(
            "{}: {detail}",
            if tier == "hires" { "Hi-Res" } else { "CD" }
        )
    };
    let artwork = track.artwork_url.clone().unwrap_or_default();
    let album_id = track.album_id.clone().unwrap_or_default();
    let source = track.source.clone().unwrap_or_default();
    let artist_id = track.artist_id.map(|id| id.to_string()).unwrap_or_default();

    qbz_app::settings::album_play_history::record_album_play(
        qbz_app::settings::album_play_history::AlbumPlayMeta {
            album_id: &album_id,
            title: &track.album,
            artist: &track.artist,
            artist_id: &artist_id,
            artwork_url: &artwork,
            quality_tier: tier,
            quality_label: &quality_label,
            year: "",
            source: &source,
        },
    );

    record(RecentTrack {
        id: track.id.to_string(),
        title: track.title.clone(),
        subtitle: track.artist.clone(),
        artwork_url: artwork.clone(),
        album_id,
        album_title: track.album.clone(),
        album_artist: track.artist.clone(),
        album_artwork_url: artwork,
        quality_tier: tier.to_string(),
        quality_label,
        genre: String::new(),
        release_date: String::new(),
        artist_id: track.artist_id,
        source,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_bare_array_migrates_to_derived_albums() {
        let tracks = vec![
            RecentTrack {
                id: "1".into(),
                album_id: "a".into(),
                album_title: "A".into(),
                ..Default::default()
            },
            RecentTrack {
                id: "2".into(),
                album_id: "a".into(),
                ..Default::default()
            },
            RecentTrack {
                id: "3".into(),
                album_id: "b".into(),
                ..Default::default()
            },
        ];
        let derived = derive_albums(&tracks);
        assert_eq!(derived.len(), 2);
        assert_eq!(derived[0].id, "a");
        assert_eq!(derived[0].title, "A");
        assert_eq!(derived[1].id, "b");
    }

    /// The owner's ephemeral rule: a session-only folder leaves no trace in the
    /// play history. Pure predicate test — it never touches the real store.
    #[test]
    fn ephemeral_ids_are_rejected_by_the_history_store() {
        let floor = qbz_library::ephemeral::EPHEMERAL_ID_FLOOR;
        assert!(is_ephemeral_track_id(&floor.to_string()));
        assert!(is_ephemeral_track_id(&(floor + 42).to_string()));
    }

    #[test]
    fn real_ids_still_record() {
        // Qobuz track id, local_tracks row id, a Plex rating_key and the empty
        // id: none of them may be mistaken for ephemeral.
        assert!(!is_ephemeral_track_id("139578884"));
        assert!(!is_ephemeral_track_id("2954"));
        assert!(!is_ephemeral_track_id("plex-12345"));
        assert!(!is_ephemeral_track_id(""));
    }

    #[test]
    fn tracks_without_an_album_are_skipped_by_the_derive() {
        let tracks = vec![RecentTrack {
            id: "1".into(),
            ..Default::default()
        }];
        assert!(derive_albums(&tracks).is_empty());
    }
}
