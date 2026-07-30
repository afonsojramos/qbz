//! Local album ACTIONS — everything the routed local album page
//! (`qml/views/LocalAlbumView.qml` + `qml/views/local/LocalAlbumHeader.qml`)
//! drives that is not a plain query: the VERSION picker, the per-disc "Disc N"
//! menu, the artist-NAME route into the Local Library Artists tab, the
//! per-row-artwork appearance mirror, and the three album-level actions whose
//! surfaces the Qt port has not grown yet.
//!
//! Ported 1:1 from `album/LocalAlbumView.slint` (617 lines) plus its Rust glue
//! (`crates/qbz/src/local_library.rs`: `open_local_album`,
//! `apply_album_version`, `album_version_dir`, `current_album_version_tracks`,
//! `current_album_disc_tracks`; `crates/qbz/src/main.rs`: the
//! `LocalAlbumActions` handlers).
//!
//! VERSIONS: a "version" is a distinct PHYSICAL copy of the album — a distinct
//! source directory (`LocalTrack.album_group_key`). Metadata identity mode can
//! fold several directories into one album card, and merging their tracks
//! would render a duplicated track list; splitting by directory is what stops
//! that. The split is cached in `LocalState` so the picker switches with NO DB
//! round-trip (the Slint's `ALBUM_VERSIONS` static, 1:1).
//!
//! NOT WIRED (deliberate, reported): `album_edit_tags` and
//! `album_add_to_playlist` are LOGGED SEAMS. The Qt port has no tag-editor
//! modal and no playlist picker, and a menu item must never write tags to disk
//! with no UI in front of it. `album_add_to_mixtape` is LIVE since the MyQBZ
//! domain landed — it opens the Mixtape/Collection picker.

use std::collections::HashMap;
use std::sync::Arc;

use cxx_qt_lib::QString;
use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_library::LocalTrack;
use qbz_models::QueueTrack;
use serde::Serialize;

use crate::local_bridge::ui;
use crate::local_playback::{local_queue_track, play_local_file, play_plex_track};
use crate::local_rows::{
    album_key, badge_source, map_track, tier_of, to_json, total_duration, AlbumRow, TrackRow,
};
use crate::local_state::{state, with_art};

type Runtime = Arc<AppRuntime<LoggingAdapter>>;

// ---------------------------------------------------------------------------
// The routed page's document (the QML contract)
// ---------------------------------------------------------------------------

/// One selectable physical copy — the Slint's `LocalAlbumVersion`
/// (album/LocalAlbumView.slint:42), consumed by
/// `qml/views/local/VersionPicker.qml` as `{ label, source }`.
#[derive(Clone, Default, Serialize)]
pub struct AlbumVersion {
    /// "24-bit / 96 kHz · FLAC" (quality + container).
    pub label: String,
    /// RAW `local_tracks.source` ("" | "qobuz_download" | "qobuz_purchase" |
    /// "plex") — `SourceIcon.qml` keys its glyph/tint off exactly these.
    pub source: String,
}

/// The album-detail HEADER: the shared `AlbumRow` plus the two fields only the
/// routed page uses. Flattened so QML keeps reading `album.title`,
/// `album.artKey`, … unchanged.
#[derive(Clone, Serialize)]
pub struct AlbumHeaderDoc {
    #[serde(flatten)]
    pub row: AlbumRow,
    pub versions: Vec<AlbumVersion>,
    #[serde(rename = "versionIndex")]
    pub version_index: i32,
}

/// `{album:{…}, tracks:[…]}` — the `localAlbumJson` document.
#[derive(Clone, Serialize)]
pub struct AlbumDetailDoc {
    pub album: AlbumHeaderDoc,
    pub tracks: Vec<TrackRow>,
}

// ---------------------------------------------------------------------------
// Version splitting (local_library.rs `open_local_album` 1:1)
// ---------------------------------------------------------------------------

/// Quality rank for ordering versions (hi-res first).
fn version_rank(t: &LocalTrack) -> (u32, u64) {
    (t.bit_depth.unwrap_or(0), t.sample_rate as u64)
}

/// Group the album's tracks by SOURCE DIRECTORY, sort each copy by
/// (disc, track) and put the best-quality copy first, so the default
/// selection is the highest-res one.
pub fn split_versions(tracks: Vec<LocalTrack>) -> Vec<(String, Vec<LocalTrack>)> {
    let mut groups: HashMap<String, Vec<LocalTrack>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for t in tracks {
        let key = t.album_group_key.clone();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(t);
    }
    let mut versions: Vec<(String, Vec<LocalTrack>)> = order
        .into_iter()
        .filter_map(|k| {
            groups.remove(&k).map(|mut v| {
                v.sort_by_key(|t| (t.disc_number.unwrap_or(1), t.track_number.unwrap_or(0)));
                (k, v)
            })
        })
        .collect();
    versions.sort_by(|a, b| {
        let qa = a.1.iter().map(version_rank).max().unwrap_or((0, 0));
        let qb = b.1.iter().map(version_rank).max().unwrap_or((0, 0));
        qb.cmp(&qa)
    });
    versions
}

/// A version's picker entry (`version_label` + `version_source` 1:1).
fn version_info(tracks: &[LocalTrack]) -> AlbumVersion {
    match tracks.first() {
        Some(t) => {
            let detail = crate::home_qt::quality_detail_from_parts(t.bit_depth, Some(t.sample_rate));
            let fmt = t.format.to_string();
            AlbumVersion {
                label: if detail.is_empty() {
                    fmt
                } else {
                    format!("{detail} · {fmt}")
                },
                source: t.source.clone().unwrap_or_default(),
            }
        }
        None => AlbumVersion::default(),
    }
}

/// Open `id`: cache its versions, select the best-quality one and return its
/// document. Called by `local_albums::load_album_detail_blocking` (BLOCKING
/// context — no Qt, no await).
pub fn open_versions(id: &str, tracks: Vec<LocalTrack>) -> Option<AlbumDetailDoc> {
    let versions = split_versions(tracks);
    if versions.is_empty() {
        return None;
    }
    // Keep the RAW rows of EVERY version so a context-menu enqueue on a Plex
    // detail row resolves without a DB id (`local_playback::find_track_blocking`).
    let all: Vec<LocalTrack> = versions.iter().flat_map(|(_, v)| v.iter().cloned()).collect();
    state(|s| {
        s.album_id = id.to_string();
        s.album_versions = versions;
        s.album_version_index = 0;
        s.detail_raw = all;
    });
    version_doc(id, 0)
}

/// Build the document for version `index` of the OPEN album. Reads the cached
/// split — no DB round-trip (the Slint's `apply_album_version`).
pub fn version_doc(id: &str, index: usize) -> Option<AlbumDetailDoc> {
    let (infos, tracks) = state(|s| {
        let infos: Vec<AlbumVersion> = s
            .album_versions
            .iter()
            .map(|(_, v)| version_info(v))
            .collect();
        let tracks = s
            .album_versions
            .get(index)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        (infos, tracks)
    });
    if tracks.is_empty() {
        return None;
    }
    let row = album_header(id, &tracks);
    // Register the album cover + the per-row sources in the art index (the
    // windowed artwork channel resolves them; nothing rides the document).
    let rows = with_art(|art| {
        if let Some(p) = tracks
            .iter()
            .find_map(|t| t.artwork_path.as_ref().filter(|p| !p.is_empty()))
        {
            art.entry(row.art_key.clone()).or_insert_with(|| p.clone());
        }
        tracks.iter().map(|t| map_track(t, art)).collect::<Vec<TrackRow>>()
    });
    Some(AlbumDetailDoc {
        album: AlbumHeaderDoc {
            row,
            versions: infos,
            version_index: index as i32,
        },
        tracks: rows,
    })
}

/// The header for ONE version — recomputed per version (the Slint recomputes
/// title / artist / info-line / quality in `apply_album_version`, because two
/// copies of the same album differ exactly in those). `directoryPath` and
/// `folderCount` come from the loaded card when it is in a mounted page; a
/// deep link leaves them empty, as the grid does.
fn album_header(id: &str, tracks: &[LocalTrack]) -> AlbumRow {
    let first = &tracks[0];
    let artist_of = |t: &LocalTrack| t.album_artist.clone().unwrap_or_else(|| t.artist.clone());
    let lead = artist_of(first);
    let artist = if tracks.iter().all(|t| artist_of(t) == lead) {
        lead
    } else {
        qbz_i18n::t("Various Artists")
    };
    // Distinct TRACK artists, first-appearance order — the "+N more artists"
    // expander (spec §B2). Raw `artist`, not `album_artist`: a compilation's
    // track artists are the list's whole point. QML splits this on ",".
    let mut all_artists: Vec<&str> = Vec::new();
    for t in tracks {
        let a = t.artist.trim();
        if !a.is_empty() && !all_artists.contains(&a) {
            all_artists.push(a);
        }
    }
    let best = tracks
        .iter()
        .max_by_key(|t| t.bit_depth.unwrap_or(0))
        .unwrap_or(first);
    let card = state(|s| {
        s.albums
            .iter()
            .chain(s.folders.iter())
            .find(|a| a.id == id)
            .cloned()
    });
    AlbumRow {
        id: id.to_string(),
        title: first.album_group_title.clone(),
        artist,
        all_artists: all_artists.join(", "),
        year: first.year.map(|y| y.to_string()).unwrap_or_default(),
        track_count: tracks.len() as u32,
        duration: total_duration(tracks.iter().map(|t| t.duration_secs).sum()),
        quality_tier: tier_of(&best.format, best.bit_depth, best.sample_rate).into(),
        quality_detail: crate::home_qt::quality_detail_from_parts(
            best.bit_depth,
            Some(best.sample_rate),
        ),
        format: best.format.to_string(),
        art_key: album_key(id),
        source: badge_source(first.source.as_deref()),
        directory_path: card.as_ref().map(|c| c.directory_path.clone()).unwrap_or_default(),
        folder_count: card.map(|c| c.folder_count).unwrap_or(0),
    }
}

// ---------------------------------------------------------------------------
// The OPEN album's track cache (the Slint's `current_album_version_tracks` /
// `current_album_disc_tracks` / `album_version_dir`)
// ---------------------------------------------------------------------------

/// The selected version's tracks (play / enqueue / the unwired actions).
pub fn current_version_tracks() -> Vec<LocalTrack> {
    state(|s| {
        s.album_versions
            .get(s.album_version_index)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    })
}

/// The selected version's SOURCE DIRECTORY key — what a tag editor would edit.
pub fn current_version_dir() -> String {
    state(|s| {
        s.album_versions
            .get(s.album_version_index)
            .map(|(dir, _)| dir.clone())
            .unwrap_or_default()
    })
}

/// One disc of the selected version, in the upstream (disc, track) order.
/// `disc_number` defaults to 1 — exactly how the header number is stamped.
fn current_disc_tracks(disc: i32) -> Vec<LocalTrack> {
    current_version_tracks()
        .into_iter()
        .filter(|t| t.disc_number.unwrap_or(1) as i32 == disc)
        .collect()
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Version picker: switch the shown copy in place (no DB round-trip).
pub fn select_version(index: i32) {
    if index < 0 {
        return;
    }
    let index = index as usize;
    let id = state(|s| {
        if index >= s.album_versions.len() || index == s.album_version_index {
            return None;
        }
        s.album_version_index = index;
        Some(s.album_id.clone())
    });
    let Some(id) = id.filter(|id| !id.is_empty()) else {
        return;
    };
    publish_doc(version_doc(&id, index));
}

/// Per-disc "Disc N" header menu. `action` is the QML CardMenu's own vocabulary
/// ("play" | "next" | "later" | "queue") over THIS disc's tracks only, reusing
/// the same queue ops as the header play button and the per-row menu.
pub fn disc_action(disc: i32, action: String) {
    let tracks = current_disc_tracks(disc);
    if tracks.is_empty() {
        log::debug!("[qbz-qt] local album: disc {disc} has no tracks for '{action}'");
        return;
    }
    let runtime = crate::app();
    crate::spawn(async move {
        match action.as_str() {
            "play" => play_rows(&runtime, tracks).await,
            "next" | "later" | "queue" => enqueue_rows(&runtime, tracks, &action).await,
            other => log::debug!("[qbz-qt] local album: unhandled disc action '{other}'"),
        }
    });
}

/// "Go to artist" on a local/Plex album: those artists have no catalog id, so
/// the route is a NAME the Local Library view consumes on its Artists tab
/// (`consumePendingArtist`). Cleared by `clear_pending_artist` once applied.
pub fn open_artist_by_name(name: String) {
    if name.trim().is_empty() {
        return;
    }
    ui(move |mut b| {
        b.as_mut()
            .set_local_pending_artist(QString::from(name.as_str()));
    });
}

/// The view consumed the pending name — release it so the same artist can be
/// routed to twice in a row (the property change IS the trigger).
pub fn clear_pending_artist() {
    ui(|mut b| b.as_mut().set_local_pending_artist(QString::from("")));
}

/// Mirror `AppearanceState.local-library-track-artwork` (ui_prefs.json,
/// default OFF — per-row covers on a 16K-track list are the freeze surface)
/// onto the bridge so the Local Library view can gate its track artwork.
pub fn publish_track_artwork() {
    let on = crate::settings_qt::pref_bool("local_library_track_artwork", false);
    ui(move |mut b| b.as_mut().set_local_track_artwork(on));
}

// --- Not wired yet (LOGGED SEAMS — see the module header) ------------------

/// Album header ✏️: the Slint opens `LocalLibraryTagEditorModal` on the
/// selected version's directory (`tag_writer.rs` / `tag_sidecar.rs` do the
/// write). The Qt port has NO tag-editor modal, so this stays a seam: it
/// resolves and logs the directory a future modal would receive and writes
/// NOTHING. Writing tags from a menu item with no UI in front of it is not a
/// port, it is data loss.
pub fn edit_tags(id: String) {
    let dir = current_version_dir();
    log::info!(
        "[qbz-qt] local album edit-tags: no tag-editor modal in the Qt port yet \
         (album '{id}', version dir '{dir}') — seam only, no tags were written"
    );
}

/// Album header ＋: the Slint opens the playlist picker over the version's
/// tracks. The Qt port has no picker modal and `playlist_qt::add_tracks` only
/// speaks Qobuz track ids, so this is a logged seam.
pub fn add_to_playlist(id: String) {
    let n = current_version_tracks().len();
    log::info!(
        "[qbz-qt] local album add-to-playlist: no playlist picker in the Qt port yet \
         (album '{id}', {n} tracks) — seam only"
    );
}

/// Album header 📼: ONE `album` payload (source "local", no artwork_url, no
/// year) plus the SELECTED version's track count, then the Mixtape/Collection
/// picker — 1:1 with `LocalAlbumActions::on_add_to_mixtape`
/// (`qbz/src/main.rs:18714-18737`).
///
/// Title and artist come from `album_header`, the same function that produced
/// the header the user is looking at, so a version switch is reflected (the
/// Slint reads `LocalAlbumState`, which `apply_album_version` recomputes for
/// exactly that reason).
///
/// Deviation, deliberate: the reference does not check the track list and lets
/// `track_count` fall to `None`; here an empty list means no version is open at
/// all, and `album_header` indexes `tracks[0]`, so it is refused with a log
/// instead of panicking.
pub fn add_to_mixtape(id: String) {
    if id.is_empty() {
        return;
    }
    let tracks = current_version_tracks();
    if tracks.is_empty() {
        log::warn!("[qbz-qt] local album add-to-mixtape: no open version for '{id}', ignored");
        return;
    }
    let row = album_header(&id, &tracks);
    let item = crate::myqbz_add_qt::AddItem {
        item_type: "album".into(),
        source: "local".into(),
        source_item_id: id,
        title: row.title,
        subtitle: (!row.artist.is_empty()).then_some(row.artist),
        artwork_url: None,
        year: None,
        track_count: Some(tracks.len() as i32),
    };
    crate::myqbz_add_qt::open_items(vec![item]);
}

// ---------------------------------------------------------------------------
// Publish + queue helpers
// ---------------------------------------------------------------------------

fn publish_doc(doc: Option<AlbumDetailDoc>) {
    let json = doc.map(|d| to_json(&d)).unwrap_or_default();
    ui(move |mut b| {
        b.as_mut()
            .set_local_album_json(QString::from(json.as_str()));
        b.as_mut().set_local_album_loading(false);
    });
}

/// Route ONE queue track to its audible step. Mirrors
/// `local_playback::play_audible` (private there) over the SAME public seams —
/// the PROTECTED audio path is entered, never modified.
async fn play_audible(runtime: &Runtime, track: &QueueTrack) -> bool {
    match track.source.as_deref() {
        Some("plex") => match track.source_item_id_hint.clone() {
            Some(rating_key) => play_plex_track(runtime, rating_key, track.id).await,
            None => {
                log::error!(
                    "[qbz-qt] plex play: queue track {} has no rating key",
                    track.id
                );
                false
            }
        },
        _ => play_local_file(runtime, track.id).await,
    }
}

/// A track SUBSET becomes the queue and starts at its first row
/// (`local_playback::play_rows` over an explicit list — that one is private and
/// only reachable by album/folder id, which a per-disc menu cannot express).
async fn play_rows(runtime: &Runtime, tracks: Vec<LocalTrack>) {
    let queue: Vec<QueueTrack> = tracks.iter().map(local_queue_track).collect();
    let Some(first) = queue.first().cloned() else {
        return;
    };
    runtime.core().set_queue(queue, Some(0)).await;
    crate::playback_qt::publish_queue(runtime).await;
    play_audible(runtime, &first).await;
    crate::playback_qt::refresh_now_playing(runtime).await;
}

/// "Play next" / "Play later" / "Add to queue" over a track SUBSET — the same
/// core helpers `local_playback::enqueue` uses, just without the id round-trip.
async fn enqueue_rows(runtime: &Runtime, tracks: Vec<LocalTrack>, mode: &str) {
    let queue: Vec<QueueTrack> = tracks.iter().map(local_queue_track).collect();
    if queue.is_empty() {
        return;
    }
    match mode {
        // Reversed so a multi-track insert at the cursor keeps its order.
        "next" => {
            for t in queue.into_iter().rev() {
                runtime.core().add_track_next(t).await;
            }
        }
        "later" => {
            for t in queue {
                runtime.core().add_track_later(t).await;
            }
        }
        _ => runtime.core().add_tracks(queue).await,
    }
    crate::playback_qt::publish_queue(runtime).await;
}
