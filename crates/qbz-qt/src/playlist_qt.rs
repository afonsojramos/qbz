//! Playlist detail controller — the QML port of the Slint PlaylistView
//! surface (crates/qbz/src/playlist.rs load + the PlaylistActions glue in
//! main.rs). Publishes ONE JSON document (`playlistJson`).
//!
//! Wired:
//! - load: `get_playlist` (full track list inline) + header mapping
//!   (the playlist's OWN `image_rectangle` graphic else the member-cover
//!   mosaic urls, owner/count/total duration, HTML-stripped description +
//!   word-boundary 160-char short).
//! - play all / shuffle (the playlist track list as the queue).
//! - favorite toggle (owned), follow/unfollow (foreign, subscribe API,
//!   optimistic flip + revert), copy-to-library (create + add all ids),
//!   pin (shared pinned store), rename (`update_playlist`), delete
//!   (`delete_playlist` + nav back).
//! - in-playlist search filter + sort (default/title/artist/album/
//!   duration/added/custom), "custom" = the drag order.
//! - per-row Remove from playlist (owner) via `remove_tracks_from_playlist`
//!   (the playlist's own track-row ids).
//! - drag reorder (issue #589): visible-index move + persistence of the
//!   custom order.
//!
//! POC-NOTEs:
//! - LOCAL playlists (library.db entities) are OUT OF SCOPE everywhere:
//!   the view is Qobuz-only (the sidebar's local rows keep their
//!   mark-active behavior from phase 7).
//! - The custom drag order persists to a per-user `playlist_orders.json`
//!   in the user dir (the Slint uses `playlist_orders.db` with
//!   `(u64, bool, i32)` rows — same behavior, simpler backend for the POC).
//! - Custom cover art (set/clear via library.db), multi-select + bulk bar,
//!   Suggested Songs, offline download of the playlist, share: not ported.
//! - is_copied persists only for the session (the Slint's mark_playlist_copied
//!   local-db write is skipped).

use std::sync::{Arc, Mutex};

use cxx_qt_lib::QString;
use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_models::{Playlist, QueueTrack, Track};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Doc + rows
// ---------------------------------------------------------------------------

#[derive(Clone, Default, Serialize)]
pub struct PlaylistTrackRow {
    pub id: String,
    /// The playlist membership row id (== catalog track id for Qobuz
    /// playlists — what remove_tracks_from_playlist takes).
    #[serde(rename = "playlistTrackId")]
    pub playlist_track_id: u64,
    pub title: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub album: String,
    #[serde(rename = "albumId")]
    pub album_id: String,
    pub duration: String,
    #[serde(rename = "durationSecs")]
    pub duration_secs: u64,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    /// Bit-depth / rate line for the row's quality badge — every other
    /// producer already emits it; without it the badge shows a bare tier.
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    #[serde(rename = "qualityLabel")]
    pub quality_label: String,
    pub explicit: bool,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    #[serde(rename = "artPath")]
    pub art_path: String,
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct PlaylistDoc {
    pub id: String,
    pub name: String,
    pub owner: String,
    #[serde(rename = "ownerId")]
    pub owner_id: u64,
    pub description: String,
    #[serde(rename = "descriptionShort")]
    pub description_short: String,
    /// The playlist's OWN Qobuz artwork (`image_rectangle`), and NOTHING
    /// else — EMPTY for every playlist without one (user playlists, and
    /// editorial ones whose graphic Qobuz omits). The `images*` lists are
    /// member-ALBUM covers; binding them here is what put an album sleeve
    /// where the playlist graphic belongs (same divergence the cards had).
    /// The header renders this CONTAIN — the graphics are landscape and
    /// cropping cuts the wordmark.
    #[serde(rename = "coverUrl")]
    pub cover_url: String,
    #[serde(rename = "coverPath")]
    pub cover_path: String,
    /// Mosaic source for a playlist with no artwork of its own: up to four
    /// de-duplicated MEMBER-ALBUM covers (`images300` > `images150` >
    /// `images`, else the first four distinct track covers — the Slint
    /// header's 2x2 collage reads `tracks[0..3].artwork`). The view feeds
    /// them to the shared `cards/PlaylistCollage.qml`, which resolves the
    /// urls itself and owns the empty (list-music) state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub covers: Vec<String>,
    pub tracks: Vec<PlaylistTrackRow>,
    #[serde(rename = "trackCount")]
    pub track_count: i32,
    #[serde(rename = "totalDuration")]
    pub total_duration: String,
    #[serde(rename = "isOwner")]
    pub is_owner: bool,
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
    #[serde(rename = "isFollowing")]
    pub is_following: bool,
    #[serde(rename = "isCopied")]
    pub is_copied: bool,
    pub pinned: bool,
    pub loading: bool,
    #[serde(rename = "sortField")]
    pub sort_field: String,
    #[serde(rename = "sortAsc")]
    pub sort_asc: bool,
    pub search: String,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Default)]
struct PageState {
    doc: PlaylistDoc,
}

static PAGE: Mutex<Option<PageState>> = Mutex::new(None);

/// The signed-in user's id, stashed at session activation (is_owner math).
static USER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn set_user_id(id: u64) {
    USER_ID.store(id, std::sync::atomic::Ordering::SeqCst);
}

fn publish(doc: &PlaylistDoc) {
    let json = serde_json::to_string(doc).unwrap_or_else(|_| "{}".into());
    crate::ui(move |mut b| {
        b.as_mut().set_playlist_json(QString::from(json.as_str()));
    });
}

fn with_doc<R>(f: impl FnOnce(&mut PlaylistDoc) -> R) -> Option<R> {
    let mut guard = PAGE.lock().unwrap();
    guard.as_mut().map(|page| f(&mut page.doc))
}

// ---------------------------------------------------------------------------
// Mapping (playlist.rs to_item + header)
// ---------------------------------------------------------------------------

fn mmss(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn tier(bit_depth: Option<u32>) -> &'static str {
    match bit_depth {
        Some(d) if d >= 24 => "hires",
        Some(_) => "cd",
        None => "",
    }
}

fn quality_label(bit_depth: Option<u32>, sample_rate: Option<f64>) -> String {
    match bit_depth {
        None => String::new(),
        Some(depth) => {
            let prefix = if depth >= 24 { "Hi-Res" } else { "CD" };
            let rate = sample_rate.unwrap_or(if depth >= 24 { 96.0 } else { 44.1 });
            let rate = if rate.fract().abs() < f64::EPSILON {
                format!("{}", rate as i64)
            } else {
                format!("{rate}")
            };
            format!("{prefix} {depth}-bit / {rate} kHz")
        }
    }
}

fn map_track(track: &Track) -> PlaylistTrackRow {
    let mut title = track.title.clone();
    if let Some(v) = track.version.as_ref().filter(|v| !v.is_empty()) {
        title = format!("{title} ({v})");
    }
    let album = track.album.as_ref();
    let (artist, artist_id) = track
        .performer
        .clone()
        .map(|p| (p.name, p.id.to_string()))
        .unwrap_or_default();
    PlaylistTrackRow {
        id: track.id.to_string(),
        playlist_track_id: track.id,
        title,
        artist,
        artist_id,
        album: album.map(|a| a.title.clone()).unwrap_or_default(),
        album_id: album.map(|a| a.id.clone()).unwrap_or_default(),
        duration: mmss(track.duration),
        duration_secs: track.duration as u64,
        quality_tier: tier(track.maximum_bit_depth).to_string(),
        quality_detail: crate::home_qt::quality_detail_from_parts(
            track.maximum_bit_depth,
            track.maximum_sampling_rate,
        ),
        quality_label: quality_label(track.maximum_bit_depth, track.maximum_sampling_rate),
        explicit: track.parental_warning,
        art_url: album
            .and_then(|a| a.image.best().cloned())
            .unwrap_or_default(),
        ..Default::default()
    }
}

fn row_to_queue(row: &PlaylistTrackRow) -> QueueTrack {
    QueueTrack {
        id: row.id.parse().unwrap_or(0),
        title: row.title.clone(),
        version: None,
        artist: row.artist.clone(),
        album: row.album.clone(),
        album_version: None,
        duration_secs: row.duration_secs,
        artwork_url: if row.art_url.is_empty() {
            None
        } else {
            Some(row.art_url.clone())
        },
        hires: row.quality_tier == "hires",
        bit_depth: None,
        sample_rate: None,
        is_local: false,
        album_id: if row.album_id.is_empty() {
            None
        } else {
            Some(row.album_id.clone())
        },
        artist_id: row.artist_id.parse::<u64>().ok(),
        streamable: true,
        source: Some("qobuz".to_string()),
        parental_warning: row.explicit,
        source_item_id_hint: None,
        context_kind: Some("playlist".to_string()),
        context_id: None,
    }
}

/// The playlist's tracks as a playable queue (play-all / shuffle / enqueue).
pub fn current_queue() -> Vec<QueueTrack> {
    PAGE.lock()
        .unwrap()
        .as_ref()
        .map(|p| p.doc.tracks.iter().map(row_to_queue).collect())
        .unwrap_or_default()
}

/// Word-boundary truncation for the 2-line header description
/// (playlist.rs truncate_words).
fn truncate_words(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    let cut = truncated.rfind(' ').unwrap_or(truncated.len());
    format!("{}…", truncated[..cut].trim_end())
}

fn total_duration_label(tracks: &[PlaylistTrackRow]) -> String {
    let secs: u64 = tracks.iter().map(|t| t.duration_secs).sum();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    if hours > 0 {
        format!("{hours} h {minutes:02} min")
    } else {
        format!("{minutes} min")
    }
}

// ---------------------------------------------------------------------------
// Header artwork (the same split the cards use — library_qt.rs
// `playlist_own_image` / `playlist_cover_urls`; kept local because those are
// private to that module. Hoisting the pair into one shared helper is the
// obvious follow-up, see the report.)
// ---------------------------------------------------------------------------

/// The playlist's OWN Qobuz graphic (`image_rectangle`, `..._mini` as the
/// lighter fallback). Only editorial playlists carry one.
fn playlist_own_image(playlist: &Playlist) -> String {
    [&playlist.image_rectangle, &playlist.image_rectangle_mini]
        .into_iter()
        .flatten()
        .find_map(|list| list.iter().find(|u| !u.is_empty()).cloned())
        .unwrap_or_default()
}

/// Up to four de-duplicated MEMBER-ALBUM covers for the header mosaic:
/// the server lists first (`images300` > `images150` > `images` — the same
/// picker as the sidebar tree and the cards, so one playlist shows the same
/// mosaic everywhere), else the first four distinct track covers (what the
/// Slint header's 2x2 collage reads).
fn collage_urls(playlist: &Playlist, tracks: &[Track]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let push = |url: String, out: &mut Vec<String>| {
        if !url.is_empty() && !out.contains(&url) {
            out.push(url);
        }
    };
    let listed = [&playlist.images300, &playlist.images150, &playlist.images]
        .into_iter()
        .flatten()
        .find(|v| !v.is_empty());
    if let Some(list) = listed {
        for url in list {
            push(url.clone(), &mut out);
            if out.len() == 4 {
                return out;
            }
        }
    }
    if out.is_empty() {
        for track in tracks {
            let url = track
                .album
                .as_ref()
                .and_then(|a| a.image.best().cloned())
                .unwrap_or_default();
            push(url, &mut out);
            if out.len() == 4 {
                break;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Load
// ---------------------------------------------------------------------------

pub async fn load(runtime: &Arc<AppRuntime<LoggingAdapter>>, playlist_id: u64) -> Result<(), String> {
    {
        let mut guard = PAGE.lock().unwrap();
        let doc = &mut guard.get_or_insert_with(PageState::default).doc;
        *doc = PlaylistDoc {
            id: playlist_id.to_string(),
            loading: true,
            sort_field: doc.sort_field.clone(),
            sort_asc: doc.sort_asc,
            ..Default::default()
        };
        publish(doc);
    }

    let mut pl = runtime
        .core()
        .get_playlist(playlist_id)
        .await
        .map_err(|e| format!("get_playlist {playlist_id} failed: {e}"))?;
    // `take` (not clone): the header helpers below still need `&pl`, and the
    // track list is the heavy half of the response.
    let tracks = pl.tracks.take().map(|c| c.items).unwrap_or_default();
    // Header artwork, two arms — 1:1 with the cards (PlaylistCard.qml):
    // the playlist's OWN graphic, else the member-cover mosaic. `images[0]`
    // (and the first track's sleeve) is a MEMBER-ALBUM cover: binding it
    // here is exactly what rendered an album sleeve as the playlist's
    // artwork, and it also starved the collage of its tiles.
    let cover_url = playlist_own_image(&pl);
    let covers = collage_urls(&pl, &tracks);
    let description = pl
        .description
        .map(|d| qbz_text_utils::strip_html::strip_html(&d))
        .unwrap_or_default();

    let mut rows: Vec<PlaylistTrackRow> = tracks.iter().map(map_track).collect();
    // Artwork (the reload_home pattern): disk hits inline, one background
    // download + republish.
    let cover_path = crate::artwork_qt::cached_path(&cover_url);
    let mut missing: Vec<String> = Vec::new();
    if !cover_url.is_empty() && cover_path.is_empty() {
        missing.push(cover_url.clone());
    }
    for row in rows.iter_mut() {
        if row.art_url.is_empty() {
            continue;
        }
        let hit = crate::artwork_qt::cached_path(&row.art_url);
        if hit.is_empty() {
            if !missing.contains(&row.art_url) {
                missing.push(row.art_url.clone());
            }
        } else {
            row.art_path = hit;
        }
    }

    // Header state: favorite/following from the Library feed when present;
    // owner from the session user; pinned from the shared store.
    let (is_favorite, is_following) = crate::library_qt::with_library(|d| {
        d.feed
            .iter()
            .find(|i| i.kind == "playlist" && i.id == playlist_id.to_string())
            .map(|i| (i.is_favorite, i.playlist_following))
            .unwrap_or((false, false))
    })
    .unwrap_or((false, false));
    let is_owner = pl.owner.id != 0 && pl.owner.id == USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    let pinned = crate::sidebar_qt::is_pinned("playlist", &playlist_id.to_string());

    // Custom drag order (applied when sort == custom).
    let sort_state = with_doc(|d| (d.sort_field.clone(), d.sort_asc)).unwrap_or(("default".into(), true));

    let track_count = rows.len() as i32;
    let total_duration = total_duration_label(&rows);
    let doc = {
        let mut guard = PAGE.lock().unwrap();
        let doc = &mut guard.get_or_insert_with(PageState::default).doc;
        doc.name = pl.name.clone();
        doc.owner = pl.owner.name.clone();
        doc.owner_id = pl.owner.id;
        doc.description_short = truncate_words(&description, 160);
        doc.description = description;
        doc.cover_url = cover_url.clone();
        doc.cover_path = cover_path;
        doc.covers = covers;
        doc.tracks = rows;
        doc.track_count = track_count;
        doc.total_duration = total_duration;
        doc.is_owner = is_owner;
        doc.is_favorite = is_favorite;
        doc.is_following = is_following;
        doc.pinned = pinned;
        doc.loading = false;
        let (field, asc) = sort_state;
        apply_sort(doc, &field, asc);
        doc.clone()
    };
    publish(&doc);

    if !missing.is_empty() {
        crate::spawn(async move {
            crate::artwork_qt::download_missing(missing).await;
            let doc = with_doc(|d| {
                if !d.cover_url.is_empty() {
                    d.cover_path = crate::artwork_qt::cached_path(&d.cover_url);
                }
                for row in d.tracks.iter_mut() {
                    if row.art_path.is_empty() {
                        row.art_path = crate::artwork_qt::cached_path(&row.art_url);
                    }
                }
                d.clone()
            });
            if let Some(doc) = doc {
                publish(&doc);
            }
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Sort + search (PlaylistActions.set-sort / filter_tracks)
// ---------------------------------------------------------------------------

fn apply_sort(doc: &mut PlaylistDoc, field: &str, asc: bool) {
    doc.sort_field = field.to_string();
    doc.sort_asc = asc;
    match field {
        "title" => doc.tracks.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        "artist" => doc.tracks.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase())),
        "album" => doc.tracks.sort_by(|a, b| a.album.to_lowercase().cmp(&b.album.to_lowercase())),
        "duration" => doc.tracks.sort_by_key(|t| t.duration_secs),
        "custom" => apply_custom_order(doc),
        // "default" / "added": the API insertion order is the natural
        // order; "added" starts newest-first (asc=false reverses).
        _ => {}
    }
    // Canonical ascending, then reverse for the other direction
    // (library_all.rs derive; default/added keep model order).
    if matches!(field, "title" | "artist" | "album" | "duration") && !asc {
        doc.tracks.reverse();
    }
    if field == "added" && asc {
        doc.tracks.reverse();
    }
}

/// The per-user custom-order sidecar (`<user dir>/playlist_orders.json`:
/// { playlist_id: [track_id, ...] } — the Slint uses playlist_orders.db
/// with (u64, bool, i32) rows; same behavior, simpler backend).
fn orders_path() -> Option<std::path::PathBuf> {
    crate::sidebar_qt::user_dir().map(|d| d.join("playlist_orders.json"))
}

fn load_custom_orders(playlist_id: u64) -> Vec<u64> {
    let Some(path) = orders_path() else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| {
            v.get(playlist_id.to_string()).and_then(|ids| {
                ids.as_array().map(|a| {
                    a.iter()
                        .filter_map(|id| id.as_str().and_then(|s| s.parse::<u64>().ok()).or(id.as_u64()))
                        .collect()
                })
            })
        })
        .unwrap_or_default()
}

fn save_custom_orders(playlist_id: u64, ids: &[u64]) {
    let Some(path) = orders_path() else {
        return;
    };
    let mut value: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            playlist_id.to_string(),
            serde_json::json!(ids.iter().map(|id| id.to_string()).collect::<Vec<_>>()),
        );
        if let Ok(text) = serde_json::to_string_pretty(&value) {
            let _ = std::fs::write(&path, text);
        }
    }
}

fn apply_custom_order(doc: &mut PlaylistDoc) {
    let Ok(pid) = doc.id.parse::<u64>() else {
        return;
    };
    let order = load_custom_orders(pid);
    if order.is_empty() {
        return;
    }
    let rank: std::collections::HashMap<u64, usize> =
        order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    // Custom-ordered first (sidecar rank), then any track the sidecar
    // doesn't know (appended in current order — playlist.rs parity).
    doc.tracks.sort_by_key(|t| {
        let id = t.id.parse::<u64>().unwrap_or(u64::MAX);
        (rank.get(&id).copied().unwrap_or(usize::MAX), 0usize)
    });
}

pub fn set_sort(field: &str) {
    let doc = with_doc(|d| {
        // Re-pick flips direction for the sortable fields; a new field
        // resets to its natural default (Library All parity).
        let asc = if d.sort_field == field {
            !d.sort_asc
        } else {
            !matches!(field, "added")
        };
        apply_sort(d, field, asc);
        d.clone()
    });
    if let Some(doc) = doc {
        publish(&doc);
    }
}

pub fn set_search(query: &str) {
    let doc = with_doc(|d| {
        d.search = query.to_string();
        d.clone()
    });
    if let Some(doc) = doc {
        publish(&doc);
    }
}

// ---------------------------------------------------------------------------
// Header actions
// ---------------------------------------------------------------------------

/// Favorite toggle (owned playlist heart; optimistic flip + signal).
pub fn toggle_favorite() {
    let doc = with_doc(|d| {
        d.is_favorite = !d.is_favorite;
        (d.id.clone(), d.is_favorite, d.clone())
    });
    if let Some((id, value, doc)) = doc {
        publish(&doc);
        crate::library_toggle_favorite("playlist".to_string(), id.clone());
        let _ = (id, value);
    }
}

/// Follow / unfollow a foreign playlist (subscribe API; optimistic flip,
/// revert on error — main.rs playlist_set_follow_by_id).
pub async fn toggle_follow(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let Some((pid, follow)) = with_doc(|d| {
        d.is_following = !d.is_following;
        let doc = d.clone();
        publish(&doc);
        (d.id.parse::<u64>().ok(), d.is_following)
    }) else {
        return;
    };
    let Some(pid) = pid else { return };
    let res = if follow {
        runtime.core().subscribe_playlist(pid).await
    } else {
        runtime.core().unsubscribe_playlist(pid).await
    };
    if let Err(e) = res {
        log::error!("[qbz-qt] playlist {pid} follow={follow} failed: {e}");
        with_doc(|d| {
            d.is_following = !follow;
            let doc = d.clone();
            publish(&doc);
        });
    }
}

/// Pin toggle (the shared pinned store; refreshes the Home rail + sidebar).
pub fn toggle_pin() {
    let doc = with_doc(|d| {
        d.pinned = !d.pinned;
        let doc = d.clone();
        publish(&doc);
        doc
    });
    if let Some(doc) = doc {
        // Pin payload artwork: the playlist's own graphic, else its first
        // member cover so the Pinned rail card still has art (PlaylistCard's
        // `artworkUrl` falls back the same way).
        let art = if doc.cover_url.is_empty() {
            doc.covers.first().cloned().unwrap_or_default()
        } else {
            doc.cover_url.clone()
        };
        crate::toggle_pin("playlist".to_string(), doc.id, doc.name, doc.owner, art);
    }
}

/// "Copy to your library" — clone a foreign playlist: create + add all
/// track ids (main.rs copy_playlist; the local mark_playlist_copied db
/// write is a POC-NOTE — is_copied is session-only).
pub async fn copy_playlist(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let Some((pid, name, description, track_ids)) = with_doc(|d| {
        (
            d.id.parse::<u64>().ok(),
            d.name.clone(),
            d.description.clone(),
            d.tracks
                .iter()
                .filter_map(|t| t.id.parse::<u64>().ok())
                .collect::<Vec<u64>>(),
        )
    }) else {
        return;
    };
    let Some(pid) = pid else { return };
    let new_description = if description.is_empty() {
        None
    } else {
        Some(description.as_str())
    };
    let new_playlist = match runtime
        .core()
        .create_playlist(&name, new_description, false)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            log::error!("[qbz-qt] copy playlist {pid}: create failed: {e}");
            return;
        }
    };
    if let Err(e) = runtime
        .core()
        .add_tracks_to_playlist(new_playlist.id, &track_ids)
        .await
    {
        log::error!("[qbz-qt] copy playlist {pid}: add tracks failed: {e}");
    }
    with_doc(|d| {
        d.is_copied = true;
        let doc = d.clone();
        publish(&doc);
    });
    log::info!("[qbz-qt] playlist {pid} copied to library as {}", new_playlist.id);
    crate::reload_sidebar();
}

/// Rename (update_playlist, name only — the edit modal's save).
pub async fn rename(runtime: &Arc<AppRuntime<LoggingAdapter>>, name: &str) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("empty playlist name".to_string());
    }
    let Some(pid) = with_doc(|d| d.id.parse::<u64>().ok()) else {
        return Err("no playlist open".to_string());
    };
    let Some(pid) = pid else {
        return Err("invalid playlist id".to_string());
    };
    runtime
        .core()
        .update_playlist(pid, Some(&name), None, None)
        .await
        .map_err(|e| format!("rename playlist {pid} failed: {e}"))?;
    with_doc(|d| {
        d.name = name;
        let doc = d.clone();
        publish(&doc);
    });
    crate::reload_sidebar();
    Ok(())
}

/// Delete the open playlist (edit modal's danger action) and navigate back.
pub async fn delete_playlist(runtime: &Arc<AppRuntime<LoggingAdapter>>) -> Result<(), String> {
    let Some(pid) = with_doc(|d| d.id.parse::<u64>().ok()) else {
        return Err("no playlist open".to_string());
    };
    let Some(pid) = pid else {
        return Err("invalid playlist id".to_string());
    };
    runtime
        .core()
        .delete_playlist(pid)
        .await
        .map_err(|e| format!("delete playlist {pid} failed: {e}"))?;
    log::info!("[qbz-qt] playlist {pid} deleted");
    crate::reload_sidebar();
    crate::nav_qt::back();
    Ok(())
}

// ---------------------------------------------------------------------------
// Track actions
// ---------------------------------------------------------------------------

/// Per-row "Remove from playlist" (owner-gated, spec §1.6.1 — the Qobuz
/// API is owner-only). playlist_track_ids = the membership row ids.
pub async fn remove_track(runtime: &Arc<AppRuntime<LoggingAdapter>>, playlist_track_id: u64) {
    let Some((pid, playlist_track_id)) = with_doc(|d| {
        if !d.is_owner {
            return (None, 0);
        }
        (d.id.parse::<u64>().ok(), playlist_track_id)
    }) else {
        return;
    };
    let Some(pid) = pid else { return };
    // Optimistic removal; the API is the source of truth on failure.
    with_doc(|d| {
        d.tracks.retain(|t| t.playlist_track_id != playlist_track_id);
        d.track_count = d.tracks.len() as i32;
        d.total_duration = total_duration_label(&d.tracks);
        let doc = d.clone();
        publish(&doc);
    });
    if let Err(e) = runtime
        .core()
        .remove_tracks_from_playlist(pid, &[playlist_track_id])
        .await
    {
        log::error!("[qbz-qt] remove track {playlist_track_id} from playlist {pid} failed: {e}");
        // Reload to reconcile (bounded-retry equivalent — the Slint
        // reconciles the same way after failed playlist ops).
        let _ = load(runtime, pid).await;
    }
}

/// Drag reorder (issue #589): move the row at visible index `from` to
/// insertion slot `slot` (0..N), persist the custom order, and switch the
/// sort to "custom" (the Slint's reorder rides the custom sort).
pub fn reorder_track(from: usize, slot: usize) {
    let doc = with_doc(|d| {
        if !d.is_owner || from >= d.tracks.len() {
            return None;
        }
        let slot = slot.min(d.tracks.len());
        if slot == from || slot == from + 1 {
            return None;
        }
        let row = d.tracks.remove(from);
        let insert_at = if slot > from { slot - 1 } else { slot };
        d.tracks.insert(insert_at.min(d.tracks.len()), row);
        // Persist the full id order in the new arrangement and flip the
        // sort to custom so the order survives reloads.
        let ids: Vec<u64> = d
            .tracks
            .iter()
            .filter_map(|t| t.id.parse::<u64>().ok())
            .collect();
        if let Ok(pid) = d.id.parse::<u64>() {
            save_custom_orders(pid, &ids);
        }
        d.sort_field = "custom".to_string();
        d.sort_asc = true;
        Some(d.clone())
    });
    if let Some(doc) = doc.flatten() {
        publish(&doc);
    }
}

/// Fetch a playlist and build its queue (the card-level actions: the
/// LibPlaylistCard overlay Play and its menu's queueing — no open view
/// needed).
async fn queue_for(runtime: &Arc<AppRuntime<LoggingAdapter>>, playlist_id: u64) -> Result<Vec<QueueTrack>, String> {
    let pl = runtime
        .core()
        .get_playlist(playlist_id)
        .await
        .map_err(|e| format!("get_playlist {playlist_id} failed: {e}"))?;
    let tracks = pl.tracks.map(|c| c.items).unwrap_or_default();
    if tracks.is_empty() {
        return Err(format!("playlist {playlist_id} has no playable tracks"));
    }
    Ok(tracks.iter().map(|t| row_to_queue(&map_track(t))).collect())
}

/// Card overlay Play: fetch + play the whole playlist from the top.
pub async fn play_playlist_by_id(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    playlist_id: u64,
) -> Result<(), String> {
    let tracks = queue_for(runtime, playlist_id).await?;
    play_queue_at(runtime, tracks, 0).await
}

/// Card menu queueing: the whole playlist after the current track ("next"
/// — reversed so the first lands first, "later" — block tail, "queue" —
/// append).
pub async fn enqueue_playlist_by_id(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    playlist_id: u64,
    mode: &str,
) -> Result<(), String> {
    let tracks = queue_for(runtime, playlist_id).await?;
    match mode {
        "next" => {
            for track in tracks.into_iter().rev() {
                runtime.core().add_track_next(track).await;
            }
        }
        "later" => {
            for track in tracks {
                runtime.core().add_track_later(track).await;
            }
        }
        _ => runtime.core().add_tracks(tracks).await,
    }
    crate::queue_qt::publish(runtime).await;
    Ok(())
}

/// Follow / unfollow a playlist by id (card overlay follow — the header
/// toggle handles the open view's optimistic flip).
pub async fn set_follow_by_id(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    playlist_id: u64,
    follow: bool,
) {
    let res = if follow {
        runtime.core().subscribe_playlist(playlist_id).await
    } else {
        runtime.core().unsubscribe_playlist(playlist_id).await
    };
    if let Err(e) = res {
        log::error!("[qbz-qt] playlist {playlist_id} follow={follow} failed: {e}");
    } else {
        crate::reload_sidebar();
    }
}

/// Add tracks to a playlist by id (the DnD drop path): Qobuz catalog ids
/// via the membership API, then refresh the sidebar + the open detail when
/// it matches.
pub async fn add_tracks(runtime: &Arc<AppRuntime<LoggingAdapter>>, playlist_id: u64, track_ids: &[u64]) {
    if track_ids.is_empty() {
        return;
    }
    match runtime
        .core()
        .add_tracks_to_playlist(playlist_id, track_ids)
        .await
    {
        Ok(()) => {
            log::info!("[qbz-qt] dropped {} track(s) onto playlist {playlist_id}", track_ids.len());
        }
        Err(e) => {
            log::error!("[qbz-qt] drop add to playlist {playlist_id} failed: {e}");
            return;
        }
    }
    crate::reload_sidebar();
    // E12: re-merge the open detail after a membership write to it.
    let open_id = with_doc(|d| d.id.parse::<u64>().ok()).flatten();
    if open_id == Some(playlist_id) {
        let _ = load(runtime, playlist_id).await;
    }
}

// ---------------------------------------------------------------------------
// Playback (play-all / shuffle / row play / row enqueue)
// ---------------------------------------------------------------------------

async fn play_queue_at(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    tracks: Vec<QueueTrack>,
    start: usize,
) -> Result<(), String> {
    if tracks.is_empty() {
        return Err("playlist has no playable tracks".to_string());
    }
    let first_id = tracks[start].id;
    runtime.core().set_queue(tracks, Some(start)).await;
    crate::queue_qt::publish(runtime).await;
    runtime
        .core()
        .play_track_resolved(first_id, crate::playback_qt::current_quality(), None, None, 0)
        .await
        .map_err(|e| format!("play_track {first_id} failed: {e}"))?;
    crate::playback_qt::refresh_now_playing(runtime).await;
    Ok(())
}

/// Header Play: the playlist's tracks (current sort) from the top.
pub async fn play_all(runtime: &Arc<AppRuntime<LoggingAdapter>>) -> Result<(), String> {
    let tracks = current_queue();
    play_queue_at(runtime, tracks, 0).await
}

/// Header Shuffle: shuffle on, then play (the core queue owns the order —
/// play_album_shuffled parity).
pub async fn play_shuffled(runtime: &Arc<AppRuntime<LoggingAdapter>>) -> Result<(), String> {
    runtime.core().set_shuffle(true).await;
    crate::now_playing::set_shuffle(true);
    play_all(runtime).await
}

/// Row play: the playlist as the queue, starting at this track.
pub async fn play_track(runtime: &Arc<AppRuntime<LoggingAdapter>>, track_id: &str) -> Result<(), String> {
    let start = with_doc(|d| d.tracks.iter().position(|t| t.id == track_id)).flatten();
    let tracks = current_queue();
    play_queue_at(runtime, tracks, start.unwrap_or(0)).await
}

/// Row ⋯ queueing: one playlist track into the EXISTING queue
/// ("next" -> add_track_next, "later" -> add_track_later, "queue" -> append).
pub async fn enqueue_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track_id: &str,
    mode: &str,
) -> Result<(), String> {
    let row = with_doc(|d| d.tracks.iter().find(|t| t.id == track_id).cloned())
        .flatten()
        .ok_or_else(|| format!("track {track_id} not in the open playlist"))?;
    let qt = row_to_queue(&row);
    match mode {
        "next" => runtime.core().add_track_next(qt).await,
        "later" => runtime.core().add_track_later(qt).await,
        _ => runtime.core().add_track(qt).await,
    }
    crate::queue_qt::publish(runtime).await;
    Ok(())
}
