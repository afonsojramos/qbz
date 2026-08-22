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
//! NOT WIRED (deliberate, reported): `album_edit_tags` is a LOGGED SEAM — the
//! Qt port has no tag-editor modal, and a menu item must never write tags to
//! disk with no UI in front of it. `album_add_to_mixtape` is LIVE since the
//! MyQBZ domain landed, and `album_add_to_playlist` since QbzPlaylistPicker
//! did: it opens the picker in LOCAL MODE over the selected version's tracks.

use std::collections::HashMap;
use std::sync::Arc;

use cxx_qt_lib::QString;
use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_library::{LocalTrack, MetadataExtractor};
use qbz_models::QueueTrack;
use serde::Serialize;

use crate::local_bridge::ui;
use crate::local_playback::local_queue_track;
use crate::local_rows::{
    album_key, badge_source, badge_source_raw, map_track, tier_of, to_json, total_duration,
    AlbumRow, TrackRow,
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

/// One disc of a multi-disc album: what the divider needs and nothing else.
///
/// A SEPARATE ARRAY, not extra columns on `TrackRow`. The divider is one item
/// per disc — two to four of them — where the track list is the freeze surface
/// that can run to hundreds of rows; paying for a title and an art key on
/// every row to label three would be the wrong trade.
#[derive(Clone, Serialize)]
pub struct DiscRow {
    /// `TrackRow.disc` for the tracks this describes.
    pub disc: u32,
    /// The disc's OWN name ("Das Rheingold", "TV Series Soundtrack #01"), or
    /// empty when the box does not name its discs — QML then draws the bare
    /// "Disc N" it drew before.
    pub title: String,
    /// Art index key for this disc's own cover (the first track's), so the
    /// divider can show the disc's artwork rather than the box's.
    #[serde(rename = "artKey")]
    pub art_key: String,
    /// Absolute path to THIS disc's own cover, or empty.
    ///
    /// Resolved here rather than read off the track, because the scan-time
    /// `artwork_path` is deliberately biased to the ALBUM ROOT
    /// (`find_folder_artwork` gives the root a +5 bonus), so every disc of a
    /// box carries the SAME file and a per-disc thumbnail drawn from it would
    /// be N copies of one image.
    pub cover: String,
}

/// This disc's own folder cover as a `file://` URL, ignoring the album root's.
///
/// Encoded through `artwork_qt::file_url`, which is not optional: this box's
/// disc folders are named "Disc 07 - TV Series Soundtrack #05", and a raw
/// concatenation gives QML `file:///…Soundtrack #05/cover.jpg`, where the `#`
/// opens a URL FRAGMENT and the path silently truncates at "Soundtrack ".
/// Caught on the owner's own library, 2026-08-22 — every disc logged
/// "QML Image: Cannot open".
fn disc_cover(t: &LocalTrack) -> String {
    let Some(dir) = std::path::Path::new(&t.file_path).parent() else {
        return String::new();
    };
    MetadataExtractor::folder_artwork_in_dir(dir)
        .map(|p| crate::artwork_qt::file_url(&p))
        .unwrap_or_default()
}

/// `{album:{…}, tracks:[…], discs:[…]}` — the `localAlbumJson` document.
#[derive(Clone, Serialize)]
pub struct AlbumDetailDoc {
    pub album: AlbumHeaderDoc,
    pub tracks: Vec<TrackRow>,
    /// EMPTY for a single-disc album. Present only so a multi-disc box can
    /// label and illustrate its dividers — see `DiscRow`.
    pub discs: Vec<DiscRow>,
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
            let source = tracks.first().and_then(|t| t.source.as_deref());
            if let Some(t) = crate::local_rows::art_token(source, p) {
                art.entry(row.art_key.clone()).or_insert(t);
            }
        }
        tracks.iter().map(|t| map_track(t, art)).collect::<Vec<TrackRow>>()
    });
    let discs = disc_rows(&tracks, &row.title);
    Some(AlbumDetailDoc {
        album: AlbumHeaderDoc {
            row,
            versions: infos,
            version_index: index as i32,
        },
        tracks: rows,
        discs,
    })
}

/// One row per disc of a MULTI-disc album, for the track list's divider.
///
/// WHY THIS IS NEEDED AT ALL. In FOLDER grouping a box set is deliberately ONE
/// album, so `TrackRow.album` is `album_group_title` — the group name with the
/// disc suffix STRIPPED (metadata.rs::strip_disc_suffix). "Box (Disc 1)" and
/// "Box (Disc 2)" both collapse to "Box", which is exactly what makes the box
/// hold together, and it is also why the divider had nothing to say: the only
/// thing distinguishing disc 1 from disc 2 in the published document was the
/// integer. Owner report 2026-08-22, a Saint Seiya box whose discs each have
/// their own name and cover: "el separador no hizo nada al respecto".
///
/// METADATA grouping shows them correctly for an unrelated reason — there each
/// disc is its OWN album row, keyed `album || artist`, so it keeps its raw tag
/// and its own artwork. Nothing below changes that path.
///
/// The title is resolved in this order, most trustworthy first:
///   1. the disc FOLDER's own titled tail ("Disc 1 - Rheingold" -> "Rheingold").
///      This is where a box that names its discs usually says so, and it is the
///      only source the tag cannot contradict.
///   2. the track's RAW album tag, when it names something the group title does
///      not already say. Compared AFTER stripping the disc suffix, so a tag of
///      "Box (Disc 2)" — which carries no name, only a number — is correctly
///      rejected instead of being echoed next to the "Disc 2" label.
///   3. nothing, and the divider stays the bare "Disc N" it has always been.
///
/// EMPTY for a single-disc album: there is no divider there to feed.
fn disc_rows(tracks: &[LocalTrack], group_title: &str) -> Vec<DiscRow> {
    use std::collections::BTreeMap;
    let mut first: BTreeMap<u32, &LocalTrack> = BTreeMap::new();
    for t in tracks {
        first.entry(t.disc_number.unwrap_or(1)).or_insert(t);
    }
    if first.len() < 2 {
        return Vec::new();
    }
    let group_stripped = MetadataExtractor::strip_disc_suffix_public(group_title);
    first
        .into_iter()
        .map(|(disc, t)| {
            let from_folder = std::path::Path::new(&t.file_path)
                .parent()
                .and_then(|d| d.file_name())
                .and_then(|n| n.to_str())
                .and_then(MetadataExtractor::disc_title_from_name);
            let title = from_folder.unwrap_or_else(|| {
                let tag = MetadataExtractor::strip_disc_suffix_public(&t.album);
                if tag.is_empty() || tag.eq_ignore_ascii_case(&group_stripped) {
                    String::new()
                } else {
                    tag
                }
            });
            DiscRow {
                disc,
                title,
                art_key: crate::local_rows::track_key(t.id),
                cover: disc_cover(t),
            }
        })
        .collect()
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
        quality_detail: crate::local_rows::detail_of(
            &best.format,
            best.bit_depth,
            best.sample_rate,
        ),
        format: best.format.to_string(),
        art_key: album_key(id),
        source: badge_source(first.source.as_deref()),
        source_raw: badge_source_raw(first.source.as_deref()),
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

/// Route the Local Library view to a TAB, optionally pre-filtered.
///
/// Used by the cortinilla's local "View more" links: those leave the search
/// surface entirely and land the user on the matching Local Library tab,
/// rather than on a Qobuz results page that has no local content at all.
///
/// The tab is QML-owned state (there is no `local_tab` property on this
/// bridge), so this is a pending route the view consumes, exactly like
/// `local_pending_artist` above. `query` is applied only by the tracks tab —
/// the albums and artists tabs have no search box of their own.
pub fn set_pending_route(tab: &str, query: &str) {
    let json = serde_json::json!({ "tab": tab, "query": query }).to_string();
    ui(move |mut b| {
        b.as_mut()
            .set_local_pending_route(QString::from(json.as_str()));
    });
}

/// The view applied the pending route — release it so the same route can fire
/// twice in a row.
pub fn clear_pending_route() {
    ui(|mut b| b.as_mut().set_local_pending_route(QString::from("")));
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

/// Album header ＋: the picker over the SELECTED version's tracks, in LOCAL
/// MODE (the Slint's `playlist_picker::open_for_ids(.., local = true)`).
///
/// The refs are built by `local_picker_ref_for_track`, never by hand: a Plex
/// row rides `plex:<rating key>` and everything else its library row id, and
/// the `Payload::LocalRefs` variant is what keeps either of them from reaching
/// a Qobuz endpoint, where the same number means a different track.
pub fn add_to_playlist(id: String) {
    let tracks = current_version_tracks();
    if tracks.is_empty() {
        log::warn!("[qbz-qt] local album add-to-playlist: no version open for album '{id}'");
        return;
    }
    let refs: Vec<String> = tracks
        .iter()
        .map(crate::local_playlist_qt::local_picker_ref_for_track)
        .collect();
    crate::playlist_picker_qt::open_for_local_refs(&crate::app(), refs);
}

/// Album header 📼: ONE `album` payload (source "local", the album cover, no
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
        // THE COVER, which this payload used to drop on the floor.
        //
        // `mixtape_collection_items.artwork_url` is a SNAPSHOT written once at
        // add time — the grid tile and the hero mosaic read that column and
        // stop, so a row stored without one is coverless for the life of the
        // row. Measured in the owner's live library.db on 2026-08-22: EIGHT
        // rows with an empty artwork_url, every one of them
        // `item_type='album' source='local'`, while every Qobuz album row
        // carried a url. This call site was the only place that could produce
        // them.
        //
        // A local absolute path is CORRECT here and is not the file:// cache
        // path the track sites are warned off: the store keeps local paths raw
        // and the mosaic passes them through unrewritten, which is what lets a
        // collection of local albums render with no network at all.
        artwork_url: tracks
            .iter()
            .find_map(|t| t.artwork_path.as_ref().filter(|p| !p.is_empty()))
            .cloned(),
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
    // THE shared audible step, not a second copy of it. The last hand-copy of
    // this function lived here and was kept in sync by comment; it drifted
    // anyway — it never learned that an OFFLINE row belongs to the offline
    // tier, so a per-disc play of a downloaded album went silent exactly like
    // the album funnel did.
    crate::local_playback::play_audible(runtime, &first).await;
    crate::playback_qt::refresh_now_playing(runtime).await;
}

/// "Play next" / "Play later" / "Add to queue" over a track SUBSET — the same
/// core helpers `local_playback::enqueue` uses, just without the id round-trip.
///
/// QConnect EXEMPT (contract §6.3): no routed arm / sync-on-add tail — a
/// local-only batch is never Qobuz-castable, so the predicate is always false
/// (Slint local_album_actions.rs:486-494 is likewise unhooked).
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
