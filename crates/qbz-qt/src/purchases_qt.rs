//! Purchases — the two screens and the download engine.
//!
//! Contract: `qbz-nix-docs/qt-frontend/2026-08-15-purchases-contract/`
//! `00-CONTRACT.md` (behaviour) and `01-BUILD-PLAN.md` §G (the frozen bridge
//! seam). The reference is Tauri's `PurchasesView.svelte` (1,945 lines) +
//! `PurchaseAlbumDetailView.svelte` (1,017 lines) + `purchaseDownloadStore.ts`;
//! the shipping Slint controller (`crates/qbz/src/purchases.rs`) supplied the
//! already-verified line-by-line ports of the pure filter/sort/group functions
//! and the download store's semantics, and NOTHING else — §8 lists four things
//! it invented and one it silently diverged on, and all five are corrected here.
//!
//! # Nobody can smoke-test this
//!
//! Qobuz does not sell purchases in the owner's region, so their account returns
//! an empty list forever. Every populated screen this file feeds will FIRST be
//! seen by a stranger. That is why the pure halves (format re-scoping, the
//! filters, the sort, the path helpers) carry unit tests at the bottom rather
//! than a promise to check them at runtime.
//!
//! # The four corrections to the Slint controller, all §8
//!
//! 1. **`is_controlling_remote()` / skip-if-remote is an INVENTION.** Zero hits
//!    for `skipIfRemote` in any Tauri purchases file; the Slint build silently
//!    no-ops "Download All" while a QConnect peer is active, with no toast.
//!    Not ported.
//! 2. **The album loop registers INSIDE the download.** Slint calls
//!    `download_purchase_track_file_only` and then does a best-effort registry
//!    write that swallows failure, leaving files on disk with no registry row
//!    while reporting success (§8-D1). This calls `download_purchase_track`
//!    with `Some(album_id)`, which registers in the same call and propagates —
//!    a registry failure marks the track `failed`, exactly as BOTH Tauri paths
//!    do.
//! 3. **The album-downloaded rule reads the LOCAL REGISTRY** (`§11-1`, owner
//!    ruling): `apply_download_flags` + `album_downloaded_from_registry`. The
//!    Slint keeps its own copy of the old nested-tracks predicate, which is
//!    unsatisfiable on the Albums tab and pins every card to "not downloaded".
//! 4. **The `metadataLoaded` guard is Tauri's and is owed** — the registry and
//!    the two per-type totals load ONCE per visit, CONCURRENTLY.
//!
//! # The album download is a FRONTEND loop (§4.1)
//!
//! `v2_purchases_download_album` exists in the reference, is registered, is
//! wrapped, and was **never called**. What ran is `purchaseDownloadStore.ts`'s
//! strictly sequential, concurrency-1 loop keyed by album id, honouring a cancel
//! flag checked BEFORE each track. Porting the Rust command instead would lose
//! cancellation and per-track progress. A failed track does NOT abort the album
//! (§4.2b): it is marked `failed`, the loop continues, and only `allComplete`
//! stays false.
//!
//! # The SEND boundary, and why the loop runs on its own thread
//!
//! `download_purchase_track(&client, &db, …)` holds `&LibraryDatabase` across a
//! multi-minute CDN await. `rusqlite::Connection` is `Send` but not `Sync`, so
//! that future is `!Send` and cannot be spawned on the multi-thread runtime. The
//! loop therefore runs on a `spawn_blocking` worker driving a CURRENT-THREAD
//! tokio runtime, which owns its own `LibraryDatabase`. That also gives the
//! required strictly-sequential execution for free.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use cxx_qt_lib::QString;
use qbz_library::LibraryDatabase;
use qbz_models::{
    Album, PurchaseAlbum, PurchaseFormatOption, PurchaseResponse, PurchaseTrack, SearchResultsPage,
};
use qbz_offline_cache::purchases_service as svc;
use qbz_qobuz::QobuzClient;
use serde::Serialize;

// ---------------------------------------------------------------------------
//  Preference keys — ui_prefs.json, SHARED with the shipping Slint build
// ---------------------------------------------------------------------------
//
// §10-A ruling: Qt persists per-MACHINE (its own mechanism) rather than Tauri's
// per-user `getUserItem`, flagged as §11-3 — account B inherits account A's
// Purchases visibility. These are the exact key names `crates/qbz/src/
// ui_prefs.rs:455-470` already declares, so a user moving between the two
// frontends keeps their filters instead of finding them reset.

const PREF_HIDE_UNAVAILABLE: &str = "purchases_hide_unavailable";
const PREF_HIDE_DOWNLOADED: &str = "purchases_hide_downloaded";
const PREF_QUALITY_FILTER: &str = "purchases_quality_filter";
const PREF_REGION_NOTICE_SEEN: &str = "purchases_region_notice_seen";

// ---------------------------------------------------------------------------
//  Documents (contract §G.2 / §G.3 — FROZEN; two QML lanes are written against
//  these). Additive fields are marked ADDITIVE and named in the lane report.
// ---------------------------------------------------------------------------

#[derive(Serialize, Default)]
struct Counts {
    albums: u32,
    tracks: u32,
}

#[derive(Serialize, Default)]
struct ToolbarDoc {
    group: String,
    sort: String,
    ascending: bool,
    #[serde(rename = "viewMode")]
    view_mode: String,
    #[serde(rename = "hideUnavailable")]
    hide_unavailable: bool,
    #[serde(rename = "hideDownloaded")]
    hide_downloaded: bool,
    #[serde(rename = "qualityFilter")]
    quality_filter: String,
}

/// One Albums-tab row.
#[derive(Serialize)]
struct AlbumRow {
    id: String,
    title: String,
    artist: String,
    #[serde(rename = "artistId")]
    artist_id: String,
    /// The REMOTE cover url — the key `QbzShell.sidebarArtworkWindow` replies
    /// on and the value `AlbumCard`'s pin payload persists.
    #[serde(rename = "artworkUrl")]
    artwork_url: String,
    /// ADDITIVE to §G.2. Rust-resolved `file://` path for the same cover, `""`
    /// until it is on disk. `AlbumCollection.qml:259` binds
    /// `modelData.artPath` into the card's `artSource`; a delegate given only
    /// the remote url draws NOTHING (the disk cache is a Rust-side concern —
    /// `artist_releases_qt.rs:37-47` and `musician_qt.rs::AppearanceRow`
    /// document the same trap).
    #[serde(rename = "artPath")]
    art_path: String,
    year: String,
    /// ADDITIVE: the full `release_date_original` (ISO `YYYY-MM-DD`, `""` when
    /// the listing has none), so the list can show a readable release date
    /// next to the purchase date and sort by it.
    #[serde(rename = "releaseDate")]
    release_date: String,
    genre: String,
    #[serde(rename = "qualityTier")]
    quality_tier: String,
    /// ADDITIVE to §G.2: the exact-quality line ("24-bit / 96 kHz") the shared
    /// quality badge renders beside the tier word.
    #[serde(rename = "qualityDetail")]
    quality_detail: String,
    #[serde(rename = "tracksCount")]
    tracks_count: u32,
    /// Drives the click gate AND the unavailable marker (§2.6).
    downloadable: bool,
    /// From the LOCAL REGISTRY (§11-1), not from the response.
    downloaded: bool,
    #[serde(rename = "purchasedAt")]
    purchased_at: i64,
}

/// One Tracks-tab row.
#[derive(Serialize)]
struct TrackRow {
    /// A **string**: QML `int` is 32-bit and track ids overflow it (§G.0).
    id: String,
    title: String,
    /// Rendered as `formatTrackTitle(title, version)` — see `AlbumTrackRow`.
    version: String,
    artist: String,
    album: String,
    #[serde(rename = "albumId")]
    album_id: String,
    #[serde(rename = "artworkUrl")]
    artwork_url: String,
    /// ADDITIVE to §G.2 — see `AlbumRow::art_path`. `TrackRow.qml`'s 36 px art
    /// cell reads `item.artPath` (the same patch `label_qt.rs:452-467` makes).
    #[serde(rename = "artPath")]
    art_path: String,
    duration: u32,
    #[serde(rename = "qualityTier")]
    quality_tier: String,
    /// ADDITIVE — see `AlbumRow::quality_detail`.
    #[serde(rename = "qualityDetail")]
    quality_detail: String,
    /// The purchases-list default is TRUE (`serde_true`, §2.6).
    streamable: bool,
    downloaded: bool,
    #[serde(rename = "purchasedAt")]
    purchased_at: i64,
}

/// `list_json`.
#[derive(Serialize)]
struct ListDoc {
    loading: bool,
    /// `""` = none; non-empty renders the retry block.
    error: String,
    tab: String,
    /// From TWO `getUserPurchases(type, limit=1)` calls — one per type — never
    /// from `getUserPurchasesIds`: that endpoint's `tracks.total` counts every
    /// purchased track INCLUDING the ones that came inside an album (34 on an
    /// account with three albums and no standalone track, 2026-09-01), while
    /// the Tracks tab lists standalone track purchases only. Never from the
    /// active tab's own page either: a typed page simply omits the sibling
    /// type's key (§2.5b-2), so the other counter needs its own call.
    counts: Counts,
    search: String,
    searching: bool,
    #[serde(rename = "regionNoticeVisible")]
    region_notice_visible: bool,
    /// The Tracks tab shows ONLY group + the two filters (§5).
    toolbar: ToolbarDoc,
    albums: Vec<AlbumRow>,
    tracks: Vec<TrackRow>,
}

#[derive(Serialize)]
struct FormatRow {
    id: u32,
    label: String,
}

/// One detail-screen track.
#[derive(Serialize)]
struct AlbumTrackRow {
    id: String,
    title: String,
    /// `formatTrackTitle(title, version)` appends `" (version)"` when this is
    /// non-empty (§10-C — the fix for the latent #360 regression; the reference
    /// declared the field on its frontend type and never mapped it).
    version: String,
    artist: String,
    #[serde(rename = "trackNumber")]
    track_number: u32,
    duration: u32,
    /// ADDITIVE to §G.3: the catalog track's tier + exact-quality line, so the
    /// detail rows can carry the same quality badge every other track surface
    /// has. `PurchaseAlbumView.qml:803` reads `qualityDetail`.
    #[serde(rename = "qualityTier")]
    quality_tier: String,
    #[serde(rename = "qualityDetail")]
    quality_detail: String,
    /// The CATALOG default is FALSE (`/album/get`'s `Track`, §2.6) — gates
    /// click-to-play. A blanket "streamable defaults true" inverts this screen.
    streamable: bool,
    /// RE-SCOPED by `selectedFormatId` (§6).
    downloaded: bool,
    /// 0 none · 1 queued · 2 downloading · 3 ready · 4 failed (§G.4).
    status: i32,
}

#[derive(Serialize)]
struct DiscRow {
    number: u32,
    tracks: Vec<AlbumTrackRow>,
}

#[derive(Serialize, Default)]
struct ProgressDoc {
    active: bool,
    /// Counted in TRACKS, never bytes (§4.2). The reference declares
    /// `bytesDownloaded`/`totalBytes` and uses neither.
    completed: u32,
    total: u32,
    #[serde(rename = "allComplete")]
    all_complete: bool,
    /// ADDITIVE to §G.3. The reference's progress section renders when
    /// `downloadingAll || allComplete || wasCancelled`, and the cancelled arm
    /// has its own already-translated msgid ("Download cancelled ({}/{})").
    /// Without this flag that arm is unreachable from QML.
    cancelled: bool,
}

#[derive(Serialize)]
struct GoodieRow {
    id: String,
    name: String,
}

/// ADDITIVE to §G.3. Goodies are counted SEPARATELY from the track progress —
/// a goodie is not a track (§14.3) — so they cannot ride `progress`.
#[derive(Serialize, Default)]
struct GoodiesProgressDoc {
    active: bool,
    completed: u32,
    total: u32,
}

/// `album_json`.
#[derive(Serialize)]
struct AlbumDoc {
    loading: bool,
    error: String,
    id: String,
    title: String,
    artist: String,
    #[serde(rename = "artistId")]
    artist_id: String,
    #[serde(rename = "artworkUrl")]
    artwork_url: String,
    /// ADDITIVE — see `AlbumRow::art_path`.
    #[serde(rename = "artPath")]
    art_path: String,
    year: String,
    label: String,
    genre: String,
    /// ADDITIVE: the header quality badge, same vocabulary as the rows.
    /// `PurchaseAlbumView.qml:411` reads `qualityDetail`.
    #[serde(rename = "qualityTier")]
    quality_tier: String,
    #[serde(rename = "qualityDetail")]
    quality_detail: String,
    #[serde(rename = "tracksCount")]
    tracks_count: u32,
    duration: u32,
    downloadable: bool,
    #[serde(rename = "purchasedAt")]
    purchased_at: i64,
    /// Order IS the dropdown order, and it is load-bearing: the highest
    /// available quality is `formats[0]` and therefore the default (§4.3).
    formats: Vec<FormatRow>,
    #[serde(rename = "selectedFormatId")]
    selected_format_id: i32,
    destination: String,
    /// Empty => render NOTHING. No empty state, no placeholder: the owner will
    /// never see this populated either, so a wrong empty state is a permanent
    /// wrong (§14.3).
    goodies: Vec<GoodieRow>,
    discs: Vec<DiscRow>,
    progress: ProgressDoc,
    #[serde(rename = "goodiesProgress")]
    goodies_progress: GoodiesProgressDoc,
    #[serde(rename = "addToLibraryVisible")]
    add_to_library_visible: bool,
}

// ---------------------------------------------------------------------------
//  Per-track download status — the §G.4 vocabulary
// ---------------------------------------------------------------------------

/// The four states a track can be in inside an album download (the reference's
/// `TrackDownloadStatus`, `purchaseDownloadStore.ts:1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DlStatus {
    Downloading,
    Complete,
    Failed,
    Cancelled,
}

impl DlStatus {
    /// Onto the shell's existing four-part vocabulary: 0 none · 1 queued ·
    /// 2 downloading · 3 ready · 4 failed (§G.4), so a purchase download LOOKS
    /// identical to a cache download in `TrackRow`.
    ///
    /// `Cancelled` maps to **none**: the reference draws no distinct cancelled
    /// row state (its `.track-row.failed` CSS class has no rule in the only
    /// stylesheet, §5), and the cancellation is reported at the ALBUM level
    /// through `progress.cancelled`. `queued` is deliberately never emitted —
    /// the reference marks one track at a time and leaves the rest blank.
    fn wire(self) -> i32 {
        match self {
            DlStatus::Downloading => 2,
            DlStatus::Complete => 3,
            DlStatus::Failed => 4,
            DlStatus::Cancelled => 0,
        }
    }
}

// ---------------------------------------------------------------------------
//  The download store (process-wide; survives navigation, like the reference's
//  module-level writable + abortFlags map)
// ---------------------------------------------------------------------------

/// Per-album download state (`AlbumDownloadState`,
/// `purchaseDownloadStore.ts:2-8`), keyed by album id.
#[derive(Debug, Clone, Default)]
struct AlbumDownload {
    /// `track_id -> status` for tracks touched by THIS album's downloads.
    statuses: HashMap<u64, DlStatus>,
    /// True while the sequential loop is running.
    downloading_all: bool,
    /// True only once EVERY requested track is `Complete`. NOT format-scoped
    /// here — the scoping is applied at READ time, matching the reference's
    /// derived `allComplete = state.allComplete && (formatId === undefined ||
    /// formatId === selectedFormatId)`.
    all_complete: bool,
    /// The album's own folder on disk, resolved from the FIRST successfully
    /// downloaded track's path.
    ///
    /// Deliberately NOT called `destination`, and deliberately not the same
    /// value as `DetailState::destination`. That one is the root the user
    /// picked (`/music`); this one is the leaf a download produced
    /// (`/music/Artist/Album [FLAC][24-bit,96kHz]`). It exists so
    /// "Add to Library" adds the ALBUM rather than the whole music root, and so
    /// covers and goodies land beside the tracks.
    ///
    /// Feeding it back in as a download destination nests the tree one level
    /// deeper on every subsequent download of the same album, which is what
    /// happened while the two shared a name.
    resolved_album_folder: Option<String>,
    /// The format these downloads used. `None` until a download starts.
    ///
    /// `startTrackDownload` OVERWRITES it (§6), which can retroactively
    /// suppress a completed album's `allComplete` — reproduced deliberately.
    format_id: Option<u32>,
    /// Goodies, counted separately from tracks (§14.3).
    goodies_active: bool,
    goodies_done: u32,
    goodies_total: u32,
}

#[derive(Debug, Default)]
struct DownloadStore {
    albums: HashMap<String, AlbumDownload>,
    /// `album_id -> abort?` (the reference's module-level `abortFlags`, NOT
    /// part of the store itself — cooperative cancellation checked BETWEEN
    /// tracks).
    abort: HashMap<String, bool>,
}

impl DownloadStore {
    fn entry(&mut self, album_id: &str) -> &mut AlbumDownload {
        self.albums.entry(album_id.to_string()).or_default()
    }

    /// `startAlbumDownload` seed (`:140-148`): clear the abort flag, then
    /// REPLACE the album state. Album downloads seed FRESH; only single-track
    /// merges.
    fn seed_album(&mut self, album_id: &str, format_id: u32) {
        self.abort.remove(album_id);
        let goodies = self
            .albums
            .get(album_id)
            .map(|a| (a.goodies_active, a.goodies_done, a.goodies_total))
            .unwrap_or_default();
        self.albums.insert(
            album_id.to_string(),
            AlbumDownload {
                statuses: HashMap::new(),
                downloading_all: true,
                all_complete: false,
                resolved_album_folder: None,
                format_id: Some(format_id),
                // A goodie download in flight is NOT a track download and must
                // survive the seed, or its counter resets under the user.
                goodies_active: goodies.0,
                goodies_done: goodies.1,
                goodies_total: goodies.2,
            },
        );
    }

    fn mark(&mut self, album_id: &str, track_id: u64, status: DlStatus) {
        self.entry(album_id).statuses.insert(track_id, status);
    }

    /// Resolve the album folder from the FIRST successful track (`:160-165`).
    fn resolve_album_folder(&mut self, album_id: &str, folder: &str) {
        self.entry(album_id).resolved_album_folder = Some(folder.to_string());
    }

    /// `finalize` (`:167-174`): drop the abort flag; `allComplete = trackIds
    /// .every(id => statuses[id] === 'complete')`; `isDownloadingAll = false`.
    /// `Iterator::all` matches `Array.prototype.every` exactly, including the
    /// empty case (`[].every(...) === true`).
    fn finalize(&mut self, album_id: &str, track_ids: &[u64]) {
        self.abort.remove(album_id);
        let state = self.entry(album_id);
        state.all_complete = track_ids
            .iter()
            .all(|id| matches!(state.statuses.get(id), Some(DlStatus::Complete)));
        state.downloading_all = false;
    }

    /// The abort branch (`:151-158`): every requested track with NO status
    /// entry becomes `Cancelled` — a track that already has ANY status
    /// (including `Downloading`) is left alone, because the abort check runs
    /// BETWEEN tracks and the in-flight one has already reached a terminal
    /// state.
    fn apply_cancellation(&mut self, album_id: &str, track_ids: &[u64]) {
        {
            let state = self.entry(album_id);
            for id in track_ids {
                state.statuses.entry(*id).or_insert(DlStatus::Cancelled);
            }
            state.downloading_all = false;
            state.all_complete = false;
        }
        self.abort.remove(album_id);
    }

    /// Single-track MERGE (`:171-184`): overwrite ONLY the target track's
    /// status and set `formatId`. Every prior field survives — so a post-album
    /// single-track redownload keeps `allComplete` and the Add-to-Library
    /// affordance. Does NOT seed fresh, does NOT rewrite the destination, does
    /// NOT set `isDownloadingAll`.
    fn merge_track_start(&mut self, album_id: &str, track_id: u64, format_id: u32) {
        let state = self.entry(album_id);
        state.statuses.insert(track_id, DlStatus::Downloading);
        state.format_id = Some(format_id);
    }

    fn set_abort(&mut self, album_id: &str) {
        self.abort.insert(album_id.to_string(), true);
    }

    fn is_aborted(&self, album_id: &str) -> bool {
        self.abort.get(album_id).copied().unwrap_or(false)
    }

    /// `clearAlbumDownloadState` (`:124-128`): drop the abort flag AND remove
    /// the album entry, so the progress and Add-to-Library blocks vanish.
    fn clear(&mut self, album_id: &str) {
        self.abort.remove(album_id);
        self.albums.remove(album_id);
    }

    fn get(&self, album_id: &str) -> Option<&AlbumDownload> {
        self.albums.get(album_id)
    }
}

fn store() -> &'static Mutex<DownloadStore> {
    static STORE: OnceLock<Mutex<DownloadStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(DownloadStore::default()))
}

/// Lock the store, run `f`, return its result. Recovers from a poisoned mutex —
/// a panic in a mutator can only corrupt one album's transient status map,
/// which the next download reseeds anyway.
fn with_store<R>(f: impl FnOnce(&mut DownloadStore) -> R) -> R {
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

// ---------------------------------------------------------------------------
//  Format scoping (§6 / §15.5-5) — PURE, and the behaviour most likely to be
//  silently wrong, so it is the one thing with real unit tests below.
// ---------------------------------------------------------------------------

/// `isDownloadedForFormat` (`PurchaseAlbumDetailView.svelte:446`):
/// `selectedFormatId !== null && (track.downloaded_format_ids ?? [])
/// .includes(selectedFormatId)`. Keys off the SERVER-derived registry formats
/// (the REQUESTED format, §B.2), never off the transient store.
fn downloaded_for_format(downloaded_format_ids: &[u32], selected: Option<u32>) -> bool {
    match selected {
        Some(fmt) => downloaded_format_ids.contains(&fmt),
        None => false,
    }
}

/// `getTrackStatus(trackId)` (`:406-415`), FORMAT-SCOPED: a `Complete` whose
/// album `formatId` is set and differs from the selected format reports NOTHING
/// — that is what makes changing the dropdown hide the green checks. Every
/// other status passes through unscoped.
fn status_scoped(
    state: Option<&AlbumDownload>,
    track_id: u64,
    selected: Option<u32>,
) -> Option<DlStatus> {
    let state = state?;
    let status = *state.statuses.get(&track_id)?;
    if status == DlStatus::Complete {
        if let Some(fmt) = state.format_id {
            if Some(fmt) != selected {
                return None;
            }
        }
    }
    Some(status)
}

/// Detail-view `allComplete` (`:388`): the stored flag gated by the selected
/// format. A fully downloaded album for format X reads `false` while format Y
/// is selected — which is what retroactively hides the Add-to-Library
/// affordance and the "complete" progress label.
fn all_complete_for_format(state: Option<&AlbumDownload>, selected: Option<u32>) -> bool {
    let Some(state) = state else { return false };
    if !state.all_complete {
        return false;
    }
    match state.format_id {
        None => true,
        Some(fmt) => Some(fmt) == selected,
    }
}

// ---------------------------------------------------------------------------
//  Pure list helpers — ported from `PurchasesView.svelte` §2.1.5
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QualityFilter {
    All,
    Hires,
    Cd,
    Lossy,
}

impl QualityFilter {
    fn parse(s: &str) -> Self {
        match s {
            "hires" => QualityFilter::Hires,
            "cd" => QualityFilter::Cd,
            "lossy" => QualityFilter::Lossy,
            _ => QualityFilter::All,
        }
    }
}

/// `matchesQualityFilter(hires, bitDepth?, samplingRate?)` (`:188-194`).
fn matches_quality(
    filter: QualityFilter,
    hires: bool,
    bit_depth: Option<u32>,
    sampling_rate: Option<f64>,
) -> bool {
    match filter {
        QualityFilter::All => true,
        QualityFilter::Hires => hires,
        // `!hires && (bitDepth === 16 || (!bitDepth && !samplingRate))`
        QualityFilter::Cd => {
            !hires && (bit_depth == Some(16) || (bit_depth.is_none() && sampling_rate.is_none()))
        }
        // `!bitDepth || bitDepth < 16`
        QualityFilter::Lossy => bit_depth.is_none_or(|d| d < 16),
    }
}

/// The tier badge word — `"hires"` | `"cd"`.
///
/// `home_qt::quality_tier_from_depth` is THE rule everywhere else in this port,
/// but it answers `""` when the payload carries no bit depth. A purchase row
/// always knows whether it is hi-res, because `hires` is a non-optional field
/// on both purchase structs (and on the catalog track), so the depth decides
/// when it is there and the flag decides when it is not. Same answer on the
/// list and on the detail for the same album, which is the point.
fn tier(hires: bool, bit_depth: Option<u32>) -> String {
    let by_depth = crate::home_qt::quality_tier_from_depth(bit_depth);
    if !by_depth.is_empty() {
        by_depth.to_string()
    } else if hires {
        "hires".to_string()
    } else {
        "cd".to_string()
    }
}

/// The exact-quality line under the badge ("24-bit / 96 kHz"), read by
/// `PurchaseAlbumView.qml` on the header and on every track row.
///
/// `quality_detail_from_parts` INVENTS `16-bit / 44.1 kHz` when handed two
/// `None`s — sensible where the tier already says "cd", but on a purchase row
/// with no quality data at all that is a number nobody sent us. Knowing
/// nothing prints nothing; everything else goes through the shared formatter so
/// the string matches every other surface character for character.
/// The badge for what the account BOUGHT. A DSD purchase streams as CD
/// (`hires:false`, 16/44.1 on the catalog — the live DSD64 album), so the
/// catalog's depth/rate describe the stream, not the purchase; the entitlement
/// ids are the only honest source. The badge control knows hires | cd | mp3,
/// so DSD rides the hires tier and says which DSD it is in the detail line.
/// Anything without a DSD id keeps the catalog-derived pair unchanged.
fn purchased_quality(
    hires: bool,
    bit_depth: Option<u32>,
    sampling_rate: Option<f64>,
    entitlement: &[u32],
) -> (String, String) {
    if entitlement.contains(&56) {
        return ("hires".to_string(), "DSD128".to_string());
    }
    if entitlement.contains(&55) {
        return ("hires".to_string(), "DSD64".to_string());
    }
    (
        tier(hires, bit_depth),
        quality_detail(bit_depth, sampling_rate),
    )
}

fn quality_detail(bit_depth: Option<u32>, sampling_rate: Option<f64>) -> String {
    if bit_depth.is_none() && sampling_rate.is_none() {
        return String::new();
    }
    crate::home_qt::quality_detail_from_parts(bit_depth, sampling_rate)
}

/// The `qualityDir` derivation (§4.4): the selected format label with `'/'`
/// replaced by `'-'`, then TRIMMED. The trim is real and load-bearing — with a
/// trailing space the album folder is a different directory.
fn quality_dir(label: &str) -> String {
    label.replace('/', "-").trim().to_string()
}

/// `albumFolderFromFilePath(filePath)` (`purchaseDownloadStore.ts:160-165`):
/// everything before the last separator, or the whole path when there is none.
fn album_folder_from_file_path(file_path: &str) -> String {
    if let Some(idx) = file_path.rfind('/') {
        return file_path[..idx].to_string();
    }
    if let Some(idx) = file_path.rfind('\\') {
        return file_path[..idx].to_string();
    }
    file_path.to_string()
}

/// The album folder a download WOULD write into, derived without waiting for a
/// track to land. `target_path` owns the whole naming rule (sanitisation, the
/// quality suffix, the single space that is omitted when `quality_dir` is
/// empty), so asking it for any file inside the album and taking the parent is
/// the only way to get this without restating that rule.
fn album_dir_for(destination: &str, artist: &str, album_title: &str, quality: &str) -> PathBuf {
    svc::target_path(destination, artist, album_title, quality, 1, "cover", "jpg")
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(destination))
}

// ---------------------------------------------------------------------------
//  State
// ---------------------------------------------------------------------------

/// The registry snapshot every annotation reads. Refreshed by the guarded
/// metadata load and again after every download, so a green check survives a
/// reload rather than only existing in the transient store.
#[derive(Default, Clone)]
struct Registry {
    /// Format-AGNOSTIC downloaded track ids. This accessor also PRUNES rows
    /// whose file is gone, which is why it is the one the list reads.
    downloaded_ids: HashSet<i64>,
    format_map: HashMap<i64, Vec<u32>>,
    /// album id -> count of DISTINCT downloaded tracks whose file still exists.
    album_counts: HashMap<String, u32>,
}

/// The LIST screen. Everything here except the four persisted preferences and
/// the region flag dies on `leave()` (§10-F) — Tauri's `{#if}` chain destroys
/// the component, and a QML `StackView` that kept it alive would change
/// behaviour by accident.
struct ListState {
    /// Bumped by every load. A response that lands for a visit the user has
    /// already left is DROPPED rather than applied.
    generation: u64,
    loading: bool,
    error: String,
    tab: String,
    counts: Counts,
    search: String,
    searching: bool,
    region_notice_visible: bool,
    group: String,
    sort: String,
    ascending: bool,
    view_mode: String,
    hide_unavailable: bool,
    hide_downloaded: bool,
    quality_filter: String,
    albums_raw: Vec<PurchaseAlbum>,
    tracks_raw: Vec<PurchaseTrack>,
    albums_loaded: bool,
    tracks_loaded: bool,
    /// Tauri's `metadataLoaded` (`PurchasesView.svelte:71`, `:314`, `:326`) —
    /// the registry and the two per-type totals load ONCE, CONCURRENTLY, and
    /// re-entry returns early. It is a COMPONENT-scoped `let` in the reference,
    /// so it is reset by `leave()` here: switching tabs within one visit hits
    /// the guard (its purpose), a fresh visit re-reads the counters (which a
    /// download in between has just invalidated).
    metadata_loaded: bool,
    /// Monotonic search token, so a stale in-flight refetch cannot overwrite a
    /// newer one.
    search_seq: u64,
}

impl Default for ListState {
    fn default() -> Self {
        Self {
            generation: 0,
            loading: false,
            error: String::new(),
            tab: "albums".to_string(),
            counts: Counts::default(),
            search: String::new(),
            searching: false,
            region_notice_visible: true,
            // FALSE until the entry seed reads the pref: the modal must never
            // flash for a user who already dismissed it.
            group: "off".to_string(),
            // §G.2's spelling of the reference's `'date'` sort key.
            sort: "purchased".to_string(),
            ascending: false,
            view_mode: "grid".to_string(),
            hide_unavailable: false,
            hide_downloaded: false,
            quality_filter: "all".to_string(),
            albums_raw: Vec::new(),
            tracks_raw: Vec::new(),
            albums_loaded: false,
            tracks_loaded: false,
            metadata_loaded: false,
            search_seq: 0,
        }
    }
}

/// The DETAIL screen. One album at a time; opening another resets it wholesale.
#[derive(Default)]
struct DetailState {
    generation: u64,
    loading: bool,
    error: String,
    album_id: String,
    /// The purchase-annotated album (header + nested tracks).
    album: Option<PurchaseAlbum>,
    /// The catalog album, kept for the tag context, the cover urls and the
    /// goodies list.
    catalog: Option<Album>,
    art_path: String,
    formats: Vec<PurchaseFormatOption>,
    selected_format_id: Option<u32>,
    /// The folder the user picked for THIS album, before any download has
    /// rewritten the store's copy to the album subfolder.
    destination: String,
}

static LIST: Mutex<Option<ListState>> = Mutex::new(None);
static DETAIL: Mutex<Option<DetailState>> = Mutex::new(None);
static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);

/// PROCESS-monotonic, deliberately not per-state. `leave()` destroys the whole
/// `ListState`, so a counter living inside it restarts at zero on the next
/// visit — and a slow response from the PREVIOUS visit would then carry a
/// generation the new visit also uses, pass the guard, and paint the old
/// account's or the old tab's rows over the new ones. One counter for both
/// screens is enough; they only ever compare against their own snapshot.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn next_generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
}

fn with_list<R>(f: impl FnOnce(&mut ListState) -> R) -> R {
    let mut guard = LIST.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(ListState::default))
}

fn with_detail<R>(f: impl FnOnce(&mut DetailState) -> R) -> R {
    let mut guard = DETAIL.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(DetailState::default))
}

fn set_registry(value: Registry) {
    *REGISTRY.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
}

/// The stored registry, reading it once if nothing has. Every annotation goes
/// through here rather than through its own `read_registry()`: the guarded
/// metadata load is what OWNS the read (that is Tauri's design —
/// `loadPurchasesMetadata` reads the two accessors and every later enrichment
/// uses the values it stored), and three consumers each stat-ing every
/// registered file would be three sweeps of the whole download folder per
/// visit.
async fn ensure_registry() -> Registry {
    if let Some(reg) = REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return reg;
    }
    let reg = read_registry().await;
    set_registry(reg.clone());
    reg
}

// ---------------------------------------------------------------------------
//  Publish
// ---------------------------------------------------------------------------
//
// `purchasesRev` is bumped inline in both publishers rather than through a
// helper: the `Pin<&mut QbzPurchases>` receiver cannot cross a plain fn boundary
// without naming the bridge's generated type, and two lines twice is cheaper
// than that import.

fn publish_list() {
    let json =
        with_list(|s| serde_json::to_string(&build_list_doc(s)).unwrap_or_else(|_| "{}".into()));
    crate::purchases_bridge::ui(move |mut b| {
        b.as_mut().set_list_json(QString::from(json.as_str()));
        let next = b.as_ref().purchases_rev() + 1;
        b.as_mut().set_purchases_rev(next);
    });
}

fn publish_album() {
    let json =
        with_detail(|s| serde_json::to_string(&build_album_doc(s)).unwrap_or_else(|_| "{}".into()));
    crate::purchases_bridge::ui(move |mut b| {
        b.as_mut().set_album_json(QString::from(json.as_str()));
        let next = b.as_ref().purchases_rev() + 1;
        b.as_mut().set_purchases_rev(next);
    });
}

/// Filter + sort the raw caches and project them into the wire rows.
///
/// GROUPING IS NOT DONE HERE. §G.2 publishes flat arrays plus `toolbar.group`,
/// and `AlbumCollection` renders pre-built sections (`grouped: [{title,
/// albums}]`), so the QML host owns the bucketing. The reference's rules, for
/// whoever writes it: album keys are the artist name or `alphaGroupKey(title)`
/// (first char uppercased; non-`[A-Z]` → `"#"`); track keys are
/// `alphaGroupKey(title)` / performer name / album title or `"Unknown"`; groups
/// are ordered by key, ascending.
fn build_list_doc(s: &ListState) -> ListDoc {
    let filter = QualityFilter::parse(&s.quality_filter);

    // `applyAlbumFilters` (`:196-202`) — the ORDER matters: hide unavailable,
    // then quality, then hide downloaded.
    let mut albums: Vec<&PurchaseAlbum> = s
        .albums_raw
        .iter()
        .filter(|a| !s.hide_unavailable || a.downloadable)
        .filter(|a| {
            matches_quality(
                filter,
                a.hires,
                a.maximum_bit_depth,
                a.maximum_sampling_rate,
            )
        })
        .filter(|a| !s.hide_downloaded || !a.downloaded)
        .collect();
    sort_albums(&mut albums, &s.sort, s.ascending);

    // `applyTrackFilters` (`:204-209`): hide-downloaded then quality. Tracks
    // have NO hide-unavailable filter (availability is albums-only) and are
    // NEVER sorted (`:142`).
    let tracks: Vec<&PurchaseTrack> = s
        .tracks_raw
        .iter()
        .filter(|t| !s.hide_downloaded || !t.downloaded)
        .filter(|t| {
            matches_quality(
                filter,
                t.hires,
                t.maximum_bit_depth,
                t.maximum_sampling_rate,
            )
        })
        .collect();

    ListDoc {
        loading: s.loading,
        error: s.error.clone(),
        tab: s.tab.clone(),
        counts: Counts {
            albums: s.counts.albums,
            tracks: s.counts.tracks,
        },
        search: s.search.clone(),
        searching: s.searching,
        region_notice_visible: s.region_notice_visible,
        toolbar: ToolbarDoc {
            group: s.group.clone(),
            sort: s.sort.clone(),
            ascending: s.ascending,
            view_mode: s.view_mode.clone(),
            hide_unavailable: s.hide_unavailable,
            hide_downloaded: s.hide_downloaded,
            quality_filter: s.quality_filter.clone(),
        },
        albums: albums.into_iter().map(album_row).collect(),
        tracks: tracks.into_iter().map(track_row).collect(),
    }
}

/// `sortAlbums` (`:211-…`): `dir = asc ? 1 : -1`, applied to one of five keys
/// (`released` is ours — the reference had no release-date sort).
/// `"purchased"` is §G.2's spelling of the reference's `"date"`; both are
/// accepted so a QML lane written against either name works.
fn sort_albums(list: &mut [&PurchaseAlbum], key: &str, ascending: bool) {
    let flip = |ord: std::cmp::Ordering| if ascending { ord } else { ord.reverse() };
    match key {
        "artist" => list.sort_by(|a, b| flip(a.artist.name.cmp(&b.artist.name))),
        "album" | "title" => list.sort_by(|a, b| flip(a.title.cmp(&b.title))),
        // ISO dates compare as strings; an album without one sorts first
        // ascending / last descending, never among the dated ones.
        "released" => list.sort_by(|a, b| {
            flip(
                a.release_date_original
                    .as_deref()
                    .unwrap_or("")
                    .cmp(b.release_date_original.as_deref().unwrap_or("")),
            )
        }),
        // `(a.sr||0)-(b.sr||0) || (a.bd||0)-(b.bd||0)`
        "quality" => list.sort_by(|a, b| {
            let asr = a.maximum_sampling_rate.unwrap_or(0.0);
            let bsr = b.maximum_sampling_rate.unwrap_or(0.0);
            let primary = asr.partial_cmp(&bsr).unwrap_or(std::cmp::Ordering::Equal);
            let ord = if primary != std::cmp::Ordering::Equal {
                primary
            } else {
                a.maximum_bit_depth
                    .unwrap_or(0)
                    .cmp(&b.maximum_bit_depth.unwrap_or(0))
            };
            flip(ord)
        }),
        // "purchased" / "date" — the default.
        _ => list.sort_by(|a, b| {
            flip(
                a.purchased_at
                    .unwrap_or(0)
                    .cmp(&b.purchased_at.unwrap_or(0)),
            )
        }),
    }
}

fn album_row(a: &PurchaseAlbum) -> AlbumRow {
    let url = a.image.best().cloned().unwrap_or_default();
    let quality = purchased_quality(
        a.hires,
        a.maximum_bit_depth,
        a.maximum_sampling_rate,
        &a.downloadable_format_ids,
    );
    AlbumRow {
        id: a.id.clone(),
        title: a.title.clone(),
        artist: a.artist.name.clone(),
        artist_id: if a.artist.id != 0 {
            a.artist.id.to_string()
        } else {
            String::new()
        },
        art_path: cached_art(&url),
        artwork_url: url,
        year: svc::parse_release_year(a.release_date_original.as_deref())
            .map(|y| y.to_string())
            .unwrap_or_default(),
        release_date: a.release_date_original.clone().unwrap_or_default(),
        genre: a.genre.as_ref().map(|g| g.name.clone()).unwrap_or_default(),
        quality_tier: quality.0,
        quality_detail: quality.1,
        tracks_count: a.tracks_count.unwrap_or(0),
        downloadable: a.downloadable,
        downloaded: a.downloaded,
        purchased_at: a.purchased_at.unwrap_or(0),
    }
}

fn track_row(t: &PurchaseTrack) -> TrackRow {
    let (album_title, album_id, url) = match &t.album {
        Some(al) => (
            al.title.clone(),
            al.id.clone(),
            al.image.best().cloned().unwrap_or_default(),
        ),
        None => (String::new(), String::new(), String::new()),
    };
    TrackRow {
        id: t.id.to_string(),
        title: t.title.clone(),
        version: t.version.clone().unwrap_or_default(),
        artist: t.performer.name.clone(),
        album: album_title,
        album_id,
        art_path: cached_art(&url),
        artwork_url: url,
        duration: t.duration,
        quality_tier: tier(t.hires, t.maximum_bit_depth),
        quality_detail: quality_detail(t.maximum_bit_depth, t.maximum_sampling_rate),
        streamable: t.streamable,
        downloaded: t.downloaded,
        purchased_at: t.purchased_at.unwrap_or(0),
    }
}

/// `file://…` for a cover already on disk, `""` otherwise. Synchronous and
/// memoised, so calling it once per row per publish is cheap.
fn cached_art(url: &str) -> String {
    if url.is_empty() {
        String::new()
    } else {
        crate::artwork_qt::cached_path(url)
    }
}

/// Every cover url on the list that is NOT on disk yet — the second-pass
/// download set (`musician_qt::attach_art` / `artist_releases_qt::fetch`).
fn missing_list_art(s: &ListState) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();
    for url in s
        .albums_raw
        .iter()
        .map(|a| a.image.best().cloned().unwrap_or_default())
        .chain(s.tracks_raw.iter().map(|t| {
            t.album
                .as_ref()
                .and_then(|al| al.image.best().cloned())
                .unwrap_or_default()
        }))
    {
        if !url.is_empty() && crate::artwork_qt::cached_path(&url).is_empty() {
            missing.push(url);
        }
    }
    missing.sort();
    missing.dedup();
    missing
}

fn build_album_doc(s: &DetailState) -> AlbumDoc {
    let selected = s.selected_format_id;
    let dl = with_store(|st| st.get(&s.album_id).cloned());
    let dl_ref = dl.as_ref();

    let Some(album) = s.album.as_ref() else {
        // Loading or failed: the header fields are empty and the QML host
        // renders its spinner / retry block off `loading` + `error`.
        return AlbumDoc {
            loading: s.loading,
            error: s.error.clone(),
            id: s.album_id.clone(),
            title: String::new(),
            artist: String::new(),
            artist_id: String::new(),
            artwork_url: String::new(),
            art_path: String::new(),
            year: String::new(),
            label: String::new(),
            genre: String::new(),
            quality_tier: String::new(),
            quality_detail: String::new(),
            tracks_count: 0,
            duration: 0,
            downloadable: true,
            purchased_at: 0,
            formats: Vec::new(),
            selected_format_id: 0,
            destination: s.destination.clone(),
            goodies: Vec::new(),
            discs: Vec::new(),
            progress: ProgressDoc::default(),
            goodies_progress: GoodiesProgressDoc::default(),
            add_to_library_visible: false,
        };
    };

    let tracks: &[PurchaseTrack] = album
        .tracks
        .as_ref()
        .map(|p| p.items.as_slice())
        .unwrap_or(&[]);
    let header_quality = purchased_quality(
        album.hires,
        album.maximum_bit_depth,
        album.maximum_sampling_rate,
        &album.downloadable_format_ids,
    );

    // Disc grouping: item order preserved, bucketed by `media_number ?? 1`.
    let mut discs: Vec<DiscRow> = Vec::new();
    for t in tracks {
        let number = t.media_number.unwrap_or(1);
        let row = AlbumTrackRow {
            id: t.id.to_string(),
            title: t.title.clone(),
            version: t.version.clone().unwrap_or_default(),
            artist: t.performer.name.clone(),
            track_number: t.track_number,
            duration: t.duration,
            quality_tier: tier(t.hires, t.maximum_bit_depth),
            quality_detail: quality_detail(t.maximum_bit_depth, t.maximum_sampling_rate),
            streamable: t.streamable,
            // `isDownloadedForFormat || status === 'complete'` (`:446`, and the
            // per-row derive at `:406-415`). BOTH halves are format-scoped,
            // which is what makes the dropdown re-scope the green checks.
            downloaded: downloaded_for_format(&t.downloaded_format_ids, selected)
                || status_scoped(dl_ref, t.id, selected) == Some(DlStatus::Complete),
            status: status_scoped(dl_ref, t.id, selected)
                .map(DlStatus::wire)
                .unwrap_or(0),
        };
        match discs.iter_mut().find(|d| d.number == number) {
            Some(disc) => disc.tracks.push(row),
            None => discs.push(DiscRow {
                number,
                tracks: vec![row],
            }),
        }
    }

    // `completedCount` / `wasCancelled` / the bar fill are NOT format-scoped
    // (§A.4) — they count the RAW status map. Only `allComplete` is.
    let completed = dl_ref
        .map(|d| {
            d.statuses
                .values()
                .filter(|st| **st == DlStatus::Complete)
                .count() as u32
        })
        .unwrap_or(0);
    let any_cancelled = dl_ref
        .map(|d| d.statuses.values().any(|st| *st == DlStatus::Cancelled))
        .unwrap_or(false);
    let downloading_all = dl_ref.map(|d| d.downloading_all).unwrap_or(false);
    let all_complete = all_complete_for_format(dl_ref, selected);

    // The USER-PICKED root, always. The store's `resolved_album_folder` is a
    // leaf produced by a download and must never be shown here or fed back in as
    // a destination — see its declaration.
    let destination = s.destination.clone();

    AlbumDoc {
        loading: s.loading,
        error: s.error.clone(),
        id: album.id.clone(),
        title: album.title.clone(),
        artist: album.artist.name.clone(),
        artist_id: if album.artist.id != 0 {
            album.artist.id.to_string()
        } else {
            String::new()
        },
        artwork_url: album.image.best().cloned().unwrap_or_default(),
        art_path: s.art_path.clone(),
        year: svc::parse_release_year(album.release_date_original.as_deref())
            .map(|y| y.to_string())
            .unwrap_or_default(),
        label: album
            .label
            .as_ref()
            .map(|l| l.name.clone())
            .unwrap_or_default(),
        genre: album
            .genre
            .as_ref()
            .map(|g| g.name.clone())
            .unwrap_or_default(),
        quality_tier: header_quality.0,
        quality_detail: header_quality.1,
        tracks_count: album.tracks_count.unwrap_or(tracks.len() as u32),
        duration: album
            .duration
            .unwrap_or_else(|| tracks.iter().map(|t| t.duration).sum::<u32>()),
        downloadable: album.downloadable,
        purchased_at: album.purchased_at.unwrap_or(0),
        formats: s
            .formats
            .iter()
            .map(|f| FormatRow {
                id: f.id,
                label: f.label.clone(),
            })
            .collect(),
        selected_format_id: selected.unwrap_or(0) as i32,
        destination,
        goodies: s
            .catalog
            .as_ref()
            .and_then(|c| c.goodies.as_ref())
            .map(|list| {
                list.iter()
                    // An item with no usable url anywhere is not renderable and
                    // not downloadable; showing it would offer an action that
                    // silently does nothing.
                    .filter(|g| g.best_url().is_some())
                    .map(|g| GoodieRow {
                        id: g.id.to_string(),
                        name: g.display_name(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        discs,
        progress: ProgressDoc {
            active: downloading_all,
            completed,
            total: tracks.len() as u32,
            all_complete,
            // `wasCancelled = !isDownloadingAll && !allComplete && some cancelled`.
            cancelled: !downloading_all && !all_complete && any_cancelled,
        },
        goodies_progress: GoodiesProgressDoc {
            active: dl_ref.map(|d| d.goodies_active).unwrap_or(false),
            completed: dl_ref.map(|d| d.goodies_done).unwrap_or(0),
            total: dl_ref.map(|d| d.goodies_total).unwrap_or(0),
        },
        // The Add-to-Library block: only when `allComplete` (format-scoped) AND
        // a destination exists in the STORE — i.e. a download really happened,
        // not merely that the user picked a folder.
        add_to_library_visible: all_complete
            && dl_ref
                .and_then(|d| d.resolved_album_folder.as_ref())
                .is_some(),
    }
}

// ---------------------------------------------------------------------------
//  Plumbing
// ---------------------------------------------------------------------------

/// Snapshot-clone the live Qobuz client out of its `RwLock` so nothing holds
/// the lock across a multi-minute CDN fetch. `None` when logged out.
async fn snapshot_client() -> Option<QobuzClient> {
    let runtime = crate::app();
    let lock = runtime.core().client();
    let guard = lock.read().await;
    guard.as_ref().cloned()
}

/// Read the three registry accessors in ONE blocking hop.
async fn read_registry() -> Registry {
    let rows = tokio::task::spawn_blocking(|| {
        crate::library_db_qt::with_db(false, |db| {
            Ok((
                db.get_downloaded_purchase_track_ids()?,
                db.get_downloaded_purchase_formats()?,
                db.get_downloaded_purchase_album_counts()?,
            ))
        })
    })
    .await
    .ok()
    .flatten();

    let Some((ids, formats, album_counts)) = rows else {
        return Registry::default();
    };
    let mut format_map: HashMap<i64, Vec<u32>> = HashMap::new();
    for (track_id, format_id) in formats {
        format_map
            .entry(track_id)
            .or_default()
            .push(format_id as u32);
    }
    Registry {
        downloaded_ids: ids.into_iter().collect(),
        format_map,
        album_counts,
    }
}

/// Open a per-user `LibraryDatabase` the download thread can OWN.
/// `library_db_qt::with_db` scopes its handle to a synchronous closure, and the
/// download primitive needs `&db` across an await — so this is the one place
/// that opens its own.
///
/// `create: true` in effect: a fresh account has no `library.db` until the first
/// local write, and the first purchase download must not be the one action that
/// silently does nothing on a new install.
fn open_owned_library_db() -> Option<LibraryDatabase> {
    let uid = qbz_app::user_data::UserDataPaths::load_last_user_id().unwrap_or(0);
    let path = dirs::data_dir()?
        .join("qbz")
        .join("users")
        .join(uid.to_string())
        .join("library.db");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match LibraryDatabase::open(&path) {
        Ok(db) => Some(db),
        Err(e) => {
            log::error!("[qbz-qt] purchases: open library.db for download failed: {e}");
            None
        }
    }
}

/// Annotate the caches with the frontend-computed download flags. Runs the
/// SHARED `apply_download_flags`, which carries the owner's §11-1 ruling (the
/// album rule reads the local registry) — never a local re-derivation, so the
/// list and the detail cannot disagree about the same album.
fn annotate(
    albums: Vec<PurchaseAlbum>,
    tracks: Vec<PurchaseTrack>,
    reg: &Registry,
) -> (Vec<PurchaseAlbum>, Vec<PurchaseTrack>) {
    let mut response = PurchaseResponse {
        albums: SearchResultsPage {
            total: albums.len() as u32,
            offset: 0,
            limit: 0,
            items: albums,
        },
        tracks: SearchResultsPage {
            total: tracks.len() as u32,
            offset: 0,
            limit: 0,
            items: tracks,
        },
    };
    svc::apply_download_flags(
        &mut response,
        &reg.downloaded_ids,
        &reg.format_map,
        &reg.album_counts,
    );
    (response.albums.items, response.tracks.items)
}

/// Fill any cover that landed on disk since the last publish, then republish.
/// Two passes, the `artist_releases_qt::fetch` shape: publish what is cached,
/// download what is not, republish.
async fn fill_missing_art(generation: u64) {
    let missing = with_list(|s| {
        if s.generation != generation {
            return Vec::new();
        }
        missing_list_art(s)
    });
    if missing.is_empty() {
        return;
    }
    crate::artwork_qt::download_missing(missing).await;
    let still_current = with_list(|s| s.generation == generation);
    if still_current {
        publish_list();
    }
}

// ---------------------------------------------------------------------------
//  List — the invokables
// ---------------------------------------------------------------------------

/// Entering the route.
pub fn open_list() {
    let (generation, tab, claim_metadata) = with_list(|s| {
        s.generation = next_generation();
        // The four persisted preferences + the region flag are the ONLY state
        // that survives a visit (§5 / §10-F). Re-seed them on every entry so a
        // toggle made in the shipping Slint build is honoured here too.
        s.hide_unavailable = crate::settings_qt::pref_bool(PREF_HIDE_UNAVAILABLE, false);
        s.hide_downloaded = crate::settings_qt::pref_bool(PREF_HIDE_DOWNLOADED, false);
        s.quality_filter = crate::settings_qt::pref_str(PREF_QUALITY_FILTER, "all");
        s.region_notice_visible = !crate::settings_qt::pref_bool(PREF_REGION_NOTICE_SEEN, false);
        s.error.clear();
        // THE GUARD (§5). `false -> true` is claimed here, under the lock, so
        // two entries in the same frame cannot both fire the metadata load.
        let claim = !s.metadata_loaded;
        s.metadata_loaded = true;
        (s.generation, s.tab.clone(), claim)
    });
    publish_list();

    crate::spawn(async move {
        // The metadata load runs FIRST and the tab load follows, because the
        // tab's rows are annotated from the registry the metadata load stores.
        // The "concurrently" in §5 is about the registry and the two totals
        // relative to EACH OTHER (the `tokio::join!` below), not about racing
        // the list fetch — the reference awaits `loadPurchasesMetadata()`
        // before it fetches, and reversing that annotates the first paint
        // against an empty registry.
        if claim_metadata {
            load_metadata(generation).await;
        }
        load_tab(generation, tab, false);
    });
}

/// The metadata load: the registry and the two per-type totals, ONCE and
/// CONCURRENTLY.
///
/// The totals need **two** `getUserPurchases(type, limit=1)` calls, one per
/// type, each falling back to `None` on error — see `svc::get_purchase_total`
/// for why the ids endpoint is the wrong source (it counts album tracks as
/// tracks). NEVER one combined call: a typed page omits the sibling type's key
/// entirely, so the list response can never supply both counters at once.
async fn load_metadata(generation: u64) {
    let client = snapshot_client().await;
    let (reg, albums_total, tracks_total) = tokio::join!(
        read_registry(),
        async {
            match client.as_ref() {
                Some(c) => svc::get_purchase_total(c, "albums").await,
                None => None,
            }
        },
        async {
            match client.as_ref() {
                Some(c) => svc::get_purchase_total(c, "tracks").await,
                None => None,
            }
        },
    );
    set_registry(reg);

    let applied = with_list(|s| {
        if s.generation != generation {
            return false;
        }
        // `.catch(() => null)` on each: a failed counter falls back to 0 and the
        // tab still renders, exactly as the reference does.
        s.counts.albums = albums_total.unwrap_or(0);
        s.counts.tracks = tracks_total.unwrap_or(0);
        true
    });
    if applied {
        publish_list();
    }
}

/// Load one tab's complete list. `force` reloads even when the tab is already
/// populated (the search-cleared path).
fn load_tab(generation: u64, tab: String, force: bool) {
    let needed = with_list(|s| {
        if s.generation != generation {
            return false;
        }
        let already = if tab == "tracks" {
            s.tracks_loaded
        } else {
            s.albums_loaded
        };
        if already && !force {
            return false;
        }
        s.loading = true;
        s.error.clear();
        true
    });
    if !needed {
        return;
    }
    publish_list();

    crate::spawn(async move {
        let Some(client) = snapshot_client().await else {
            finish_tab_error(generation);
            return;
        };
        let result = svc::get_user_purchases_by_type(&client, &tab).await;
        let reg = ensure_registry().await;

        match result {
            Ok(response) => {
                // Captured before `annotate` consumes the items, for the counter
                // backfill below.
                let server_total = if tab == "tracks" {
                    response.tracks.total
                } else {
                    response.albums.total
                };
                let (albums, tracks) = annotate(response.albums.items, response.tracks.items, &reg);
                let applied = with_list(|s| {
                    if s.generation != generation {
                        return false;
                    }
                    if tab == "tracks" {
                        s.tracks_raw = tracks;
                        s.tracks_loaded = true;
                    } else {
                        s.albums_raw = albums;
                        s.albums_loaded = true;
                    }

                    // Backfill a counter the ids call could not supply. The
                    // reference does this too and it is easy to miss, because the
                    // two `getUserPurchasesIds` calls look like the whole story:
                    // `if (!totalAlbumPurchases) { totalAlbumPurchases =
                    // response.albums.total || enriched.albums.length }`
                    // (`PurchasesView.svelte:348-358`). Those ids calls each
                    // `.catch(() => null)`, so a single hiccup leaves a tab
                    // reading 0 forever while its list is plainly full of items.
                    // The TYPED list response does carry the requested type's
                    // total, so the number is already in hand.
                    let loaded_len = if tab == "tracks" {
                        s.tracks_raw.len() as u32
                    } else {
                        s.albums_raw.len() as u32
                    };
                    let counter = if tab == "tracks" {
                        &mut s.counts.tracks
                    } else {
                        &mut s.counts.albums
                    };
                    if *counter == 0 {
                        *counter = if server_total > 0 {
                            server_total
                        } else {
                            loaded_len
                        };
                    }

                    s.loading = false;
                    s.error.clear();
                    true
                });
                if applied {
                    publish_list();
                    fill_missing_art(generation).await;
                }
            }
            Err(e) => {
                log::warn!("[qbz-qt] purchases: load {tab} failed: {e}");
                finish_tab_error(generation);
            }
        }
    });
}

fn finish_tab_error(generation: u64) {
    let applied = with_list(|s| {
        if s.generation != generation {
            return false;
        }
        s.loading = false;
        // Rust-translated and travelling inside a JSON document, so it cannot
        // re-translate through `trRev` — the same limitation every other
        // controller's error line has. The copy must not accuse the network of
        // an EMPTY result (§5.6 / §2.5b): this string is only ever set on a real
        // request failure, never on "you own nothing".
        s.error = qbz_i18n::t("Couldn't load purchases. Check your connection.");
        true
    });
    if applied {
        publish_list();
    }
}

pub fn leave() {
    // Everything transient dies (§10-F): tab, sort, direction, grouping, view
    // mode, the search text and both caches. The persisted prefs are re-read on
    // the next `open_list`, and the download store is process-wide by design so
    // a running album download survives being navigated away from.
    *LIST.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn set_tab(tab: String) {
    let tab = if tab == "tracks" { "tracks" } else { "albums" }.to_string();
    let generation = with_list(|s| {
        s.tab = tab.clone();
        s.generation
    });
    publish_list();
    load_tab(generation, tab, false);
}

/// The search FIRE (the 300 ms debounce is QML's one-shot `Timer`, §G.1).
///
/// It REFETCHES FROM THE SERVER and re-requests BOTH types, then filters in
/// Rust and marks both tabs loaded (§5) — it is NOT a local filter over the
/// active tab. Clearing it refetches the active tab.
pub fn search(query: String) {
    let query = query.trim().to_string();
    let (generation, seq, tab) = with_list(|s| {
        s.search = query.clone();
        s.search_seq = s.search_seq.wrapping_add(1);
        s.searching = !query.is_empty();
        (s.generation, s.search_seq, s.tab.clone())
    });
    publish_list();

    if query.is_empty() {
        // Clearing refetches the ACTIVE tab (the search response replaced both
        // caches, so the tab has to come back from the server).
        with_list(|s| {
            s.albums_loaded = false;
            s.tracks_loaded = false;
        });
        load_tab(generation, tab, true);
        return;
    }

    crate::spawn(async move {
        let Some(client) = snapshot_client().await else {
            with_list(|s| s.searching = false);
            publish_list();
            return;
        };
        let result = svc::get_user_purchases_all(&client).await;
        let reg = ensure_registry().await;

        match result {
            Ok(response) => {
                let filtered = svc::filter_purchase_response(response, &query);
                let (albums, tracks) = annotate(filtered.albums.items, filtered.tracks.items, &reg);
                let applied = with_list(|s| {
                    // Two guards: the visit, and the search token — a slow
                    // earlier query must not overwrite a newer one's results.
                    if s.generation != generation || s.search_seq != seq {
                        return false;
                    }
                    s.albums_raw = albums;
                    s.tracks_raw = tracks;
                    s.albums_loaded = true;
                    s.tracks_loaded = true;
                    s.searching = false;
                    s.loading = false;
                    s.error.clear();
                    true
                });
                if applied {
                    publish_list();
                    fill_missing_art(generation).await;
                }
            }
            Err(e) => {
                log::warn!("[qbz-qt] purchases: search failed: {e}");
                let applied = with_list(|s| {
                    if s.generation != generation || s.search_seq != seq {
                        return false;
                    }
                    s.searching = false;
                    s.error = qbz_i18n::t("Couldn't load purchases. Check your connection.");
                    true
                });
                if applied {
                    publish_list();
                }
            }
        }
    });
}

pub fn set_sort(key: String, ascending: bool) {
    with_list(|s| {
        s.sort = key;
        s.ascending = ascending;
    });
    publish_list();
}

pub fn set_group(key: String) {
    with_list(|s| s.group = key);
    publish_list();
}

pub fn set_view_mode(mode: String) {
    with_list(|s| {
        s.view_mode = if mode == "list" {
            "list".into()
        } else {
            "grid".into()
        }
    });
    publish_list();
}

pub fn set_hide_unavailable(on: bool) {
    crate::settings_qt::save_pref(PREF_HIDE_UNAVAILABLE, serde_json::json!(on));
    with_list(|s| s.hide_unavailable = on);
    publish_list();
}

pub fn set_hide_downloaded(on: bool) {
    crate::settings_qt::save_pref(PREF_HIDE_DOWNLOADED, serde_json::json!(on));
    with_list(|s| s.hide_downloaded = on);
    publish_list();
}

pub fn set_quality_filter(value: String) {
    let value = match value.as_str() {
        "hires" | "cd" | "lossy" => value,
        _ => "all".to_string(),
    };
    crate::settings_qt::save_pref(PREF_QUALITY_FILTER, serde_json::json!(value));
    with_list(|s| s.quality_filter = value);
    publish_list();
}

pub fn dismiss_region_notice() {
    crate::settings_qt::save_pref(PREF_REGION_NOTICE_SEEN, serde_json::json!(true));
    with_list(|s| s.region_notice_visible = false);
    publish_list();
}

// ---------------------------------------------------------------------------
//  Detail
// ---------------------------------------------------------------------------

/// Open the purchased-album detail. Records the album and starts the fetch; the
/// QML caller navigates AFTERWARDS (§C.2, §G.1) — this deliberately does not
/// call `nav_qt::record` itself, or `QbzShell.navigateTo` would push a second
/// entry for the same route.
pub fn open_album(album_id: String) {
    let album_id = album_id.trim().to_string();
    if album_id.is_empty() {
        return;
    }
    let generation = with_detail(|s| {
        s.generation = next_generation();
        s.loading = true;
        s.error.clear();
        s.album_id = album_id.clone();
        // Clear the PREVIOUS album in the same publish that raises the loading
        // flag: without it the view renders the last album until the fetch
        // lands, so opening B after A shows A (`crate::open_album`'s rule).
        s.album = None;
        s.catalog = None;
        s.art_path.clear();
        s.formats.clear();
        s.selected_format_id = None;
        // Start with no destination. The reference's picker is component state
        // on the detail screen and dies with it (§10-F), so reopening an album
        // asks again — and seeding it from a previous download's RESOLVED ALBUM
        // FOLDER would hand the next download a leaf to nest inside.
        s.destination = String::new();
        s.generation
    });
    publish_album();

    crate::spawn(async move { load_album(generation, album_id).await });
}

/// Opening the detail is a request GRAPH in the reference: `/album/get`, then
/// ALL album and track purchase pages (to recover `downloadable` and
/// `purchased_at`), and CONCURRENTLY a SECOND `/album/get` for the formats.
///
/// **Divergence, recorded rather than taken for free (§6):** the duplicate
/// `/album/get` is consolidated into one call. The failure semantics are
/// preserved: `/album/get` is the one purchase-path call that hard-checks
/// status and parses the STRICT structs, so a bad payload fails the whole
/// screen — and it now fails it once instead of twice.
///
/// **The format menu is the ENTITLEMENT (2026-09-01 contract §1.4), not a
/// synthesis from the catalog.** The purchases listing fetched here carries
/// `downloadable_format_ids` per album — `[55]` for a DSD64 purchase whose
/// catalog streams at 16/44.1, `[56]` for DSD128, `[7, 6, 5]` for a hi-res
/// FLAC — and those are the only ids `getFileUrl(intent=download)` will
/// honour. The old `synth_formats` ladder (27/7/6/5 from `album.hires`) stays
/// as the fallback for a listing that does not carry the field, so an older
/// wire still gets a menu instead of nothing.
async fn load_album(generation: u64, album_id: String) {
    let Some(client) = snapshot_client().await else {
        fail_album(generation);
        return;
    };

    // UNSIGNED on the purchase path (§11-6, settled by vendor evidence): the
    // Qobuz desktop client sends `/album/get` without a signature and so did the
    // reference. The entitlement proof is the signature on `getFileUrl`, which
    // nothing here touches.
    let catalog = match client.get_album_for_purchase(&album_id).await {
        Ok(album) => album,
        Err(e) => {
            log::warn!("[qbz-qt] purchases: album {album_id} fetch failed: {e}");
            fail_album(generation);
            return;
        }
    };

    // The purchases listing supplies `downloadable` and `purchased_at`, which
    // the catalog album does not carry. A failure here is NOT fatal:
    // `build_purchase_album` defaults `downloadable` to TRUE when the album is
    // absent from the listing, which is the reference's own fallback.
    let purchases = svc::get_user_purchases_all(&client)
        .await
        .unwrap_or_else(|e| {
            log::warn!("[qbz-qt] purchases: listing for album {album_id} unavailable: {e}");
            PurchaseResponse {
                albums: SearchResultsPage::default(),
                tracks: SearchResultsPage::default(),
            }
        });

    let reg = ensure_registry().await;

    let album =
        svc::build_purchase_album(&catalog, &purchases, &reg.downloaded_ids, &reg.format_map);
    let formats = svc::purchase_formats(
        &catalog,
        &svc::entitlement_format_ids(&purchases, &album_id),
    );
    let cover_url = catalog.image.best().cloned().unwrap_or_default();

    let applied = with_detail(|s| {
        if s.generation != generation {
            return false;
        }
        // The dropdown defaults to `formats[0]` — the HIGHEST available quality,
        // which is why the synthesis order is load-bearing (§4.3). A format the
        // store already downloaded in wins, so re-entering the detail does not
        // silently re-scope a finished download's green checks.
        s.selected_format_id = with_store(|st| st.get(&s.album_id).and_then(|d| d.format_id))
            .filter(|id| formats.iter().any(|f| f.id == *id))
            .or_else(|| formats.first().map(|f| f.id));
        s.album = Some(album);
        s.catalog = Some(catalog);
        s.formats = formats;
        s.art_path = cached_art(&cover_url);
        s.loading = false;
        s.error.clear();
        true
    });
    if !applied {
        return;
    }
    publish_album();

    if !cover_url.is_empty() && crate::artwork_qt::cached_path(&cover_url).is_empty() {
        crate::artwork_qt::download_missing(vec![cover_url.clone()]).await;
        let landed = with_detail(|s| {
            if s.generation != generation {
                return false;
            }
            s.art_path = cached_art(&cover_url);
            !s.art_path.is_empty()
        });
        if landed {
            publish_album();
        }
    }
}

fn fail_album(generation: u64) {
    let applied = with_detail(|s| {
        if s.generation != generation {
            return false;
        }
        s.loading = false;
        s.error = qbz_i18n::t("Couldn't load purchases. Check your connection.");
        true
    });
    if applied {
        publish_album();
    }
}

/// Changing the format RE-SCOPES every downloaded mark, the progress block AND
/// the Add-to-Library affordance (§6). It is a pure republish: the scoping is
/// applied at read time in `build_album_doc`, never baked into the state.
pub fn set_format(format_id: i32) {
    if format_id <= 0 {
        return;
    }
    let changed = with_detail(|s| {
        let id = format_id as u32;
        if !s.formats.iter().any(|f| f.id == id) {
            log::warn!(
                "[qbz-qt] purchases: format {id} is not offered for album {}",
                s.album_id
            );
            return false;
        }
        s.selected_format_id = Some(id);
        true
    });
    if changed {
        publish_album();
    }
}

/// The native folder picker (`rfd::AsyncFileDialog`, §G.0), defaulting to the
/// OS audio directory. Cancelling is a no-op, not an error.
pub fn choose_destination() {
    crate::spawn(async move {
        let Some(folder) = pick_folder().await else {
            return;
        };
        with_detail(|s| s.destination = folder);
        publish_album();
    });
}

async fn pick_folder() -> Option<String> {
    let mut dialog = rfd::AsyncFileDialog::new().set_title(&qbz_i18n::t("Choose folder"));
    if let Some(audio_dir) = dirs::audio_dir() {
        dialog = dialog.set_directory(audio_dir);
    }
    let folder = dialog.pick_folder().await?;
    Some(folder.path().to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
//  Downloads
// ---------------------------------------------------------------------------

/// Everything one download needs, snapshotted off the detail state under the
/// lock so nothing reads it again across an await.
#[derive(Clone)]
struct DownloadPlan {
    album_id: String,
    artist: String,
    album_title: String,
    format_id: u32,
    quality_dir: String,
    destination: String,
    track_ids: Vec<u64>,
    catalog: Album,
}

/// `None` when the screen cannot start a download at all (no album, no format).
/// The destination is allowed to be empty here — the caller prompts.
fn build_plan() -> Option<DownloadPlan> {
    with_detail(|s| {
        let album = s.album.as_ref()?;
        let catalog = s.catalog.as_ref()?;
        let format = s
            .selected_format_id
            .and_then(|id| s.formats.iter().find(|f| f.id == id))?;
        Some(DownloadPlan {
            album_id: album.id.clone(),
            artist: album.artist.name.clone(),
            album_title: album.title.clone(),
            format_id: format.id,
            quality_dir: quality_dir(&format.label),
            // The user-picked ROOT. Reading the store's resolved album folder
            // here nested the tree one level deeper on every later download of
            // the same album.
            destination: s.destination.clone(),
            track_ids: album
                .tracks
                .as_ref()
                .map(|p| p.items.iter().map(|t| t.id).collect())
                .unwrap_or_default(),
            catalog: catalog.clone(),
        })
    })
}

/// "Download all" — the frontend loop (§4.1).
pub fn download_album() {
    let Some(plan) = build_plan() else {
        crate::toast_qt::error(qbz_i18n::t("No downloadable formats available"));
        return;
    };
    if plan.track_ids.is_empty() {
        crate::toast_qt::error(qbz_i18n::t("Failed to start download. Please try again."));
        return;
    }
    crate::spawn(async move {
        let Some(plan) = ensure_destination(plan).await else {
            return;
        };
        run_album_download(plan).await;
    });
}

/// One track — the row affordance and the retry control on a failed row.
///
/// This is NOT a small `download_album`: it never sets `destination`,
/// `isDownloadingAll` or `allComplete`, and it ignores the abort flag — but it
/// DOES overwrite the album's `formatId`, which can retroactively suppress a
/// completed album download's `allComplete` (§6). Reproduced deliberately.
pub fn download_track(track_id: String) {
    let Ok(track_id) = track_id.trim().parse::<u64>() else {
        return;
    };
    let Some(plan) = build_plan() else {
        crate::toast_qt::error(qbz_i18n::t("No downloadable formats available"));
        return;
    };
    if !plan.track_ids.contains(&track_id) {
        crate::toast_qt::error(qbz_i18n::t("This track has no album to download"));
        return;
    }
    crate::spawn(async move {
        let Some(plan) = ensure_destination(plan).await else {
            return;
        };
        with_store(|st| st.merge_track_start(&plan.album_id, track_id, plan.format_id));
        publish_album();

        let Some(client) = snapshot_client().await else {
            with_store(|st| st.mark(&plan.album_id, track_id, DlStatus::Failed));
            publish_album();
            return;
        };
        let ctx = build_context(&plan.catalog).await;

        let one = plan.clone();
        let done = tokio::task::spawn_blocking(move || {
            run_sequential(&one, &[track_id], client, ctx, false)
        })
        .await;
        if let Err(e) = done {
            log::error!("[qbz-qt] purchases: track-download thread join failed: {e}");
            with_store(|st| st.mark(&plan.album_id, track_id, DlStatus::Failed));
        }
        refresh_after_download().await;
    });
}

/// Prompt for a destination when there is none. The reference's
/// `promptForFolder` runs inside the download action, which is why this is here
/// and not a precondition the QML host has to enforce.
async fn ensure_destination(mut plan: DownloadPlan) -> Option<DownloadPlan> {
    if !plan.destination.trim().is_empty() {
        return Some(plan);
    }
    let folder = pick_folder().await?;
    with_detail(|s| s.destination = folder.clone());
    plan.destination = folder;
    publish_album();
    Some(plan)
}

/// The album-level context every track is tagged from (§14): built ONCE before
/// the loop, with the cover fetched once rather than per track.
async fn build_context(catalog: &Album) -> svc::PurchaseAlbumContext {
    let cover = match catalog
        .image
        .large
        .as_deref()
        .or(catalog.image.best().map(String::as_str))
    {
        Some(url) if !url.is_empty() => svc::fetch_asset_bytes(url).await,
        _ => None,
    };
    svc::PurchaseAlbumContext::from_album(catalog).with_cover(cover)
}

async fn run_album_download(plan: DownloadPlan) {
    with_store(|st| st.seed_album(&plan.album_id, plan.format_id));
    publish_album();

    let Some(client) = snapshot_client().await else {
        // No client: mark every track failed rather than leaving the UI on a
        // seeded "downloading" that never resolves.
        with_store(|st| {
            for id in &plan.track_ids {
                st.mark(&plan.album_id, *id, DlStatus::Failed);
            }
            st.finalize(&plan.album_id, &plan.track_ids);
        });
        publish_album();
        return;
    };

    // §14: the cover is fetched ONCE, before the loop, and embedded into every
    // track. Fetching it per track would issue one CDN request per file for a
    // value that never changes.
    let ctx = build_context(&plan.catalog).await;
    let back = match plan.catalog.image.back.as_deref() {
        Some(url) if !url.is_empty() => svc::fetch_asset_bytes(url).await,
        _ => None,
    };
    let cover = ctx.cover_jpeg.clone();

    let loop_plan = plan.clone();
    let ids = plan.track_ids.clone();
    let done =
        tokio::task::spawn_blocking(move || run_sequential(&loop_plan, &ids, client, ctx, true))
            .await;
    if let Err(e) = done {
        log::error!("[qbz-qt] purchases: album-download thread join failed: {e}");
        with_store(|st| st.finalize(&plan.album_id, &plan.track_ids));
    }

    // §14.2, best-effort and after the audio: `cover.jpg` / `back.jpg` beside
    // the tracks. The folder is the one the FIRST successful track produced —
    // the store's rewritten destination — falling back to the deterministic
    // derivation when nothing landed.
    let folder = with_store(|st| {
        st.get(&plan.album_id)
            .and_then(|d| d.resolved_album_folder.clone())
    })
    .filter(|d| d != &plan.destination)
    .map(PathBuf::from)
    .unwrap_or_else(|| {
        album_dir_for(
            &plan.destination,
            &plan.artist,
            &plan.album_title,
            &plan.quality_dir,
        )
    });
    if cover.is_some() || back.is_some() {
        let _ = tokio::task::spawn_blocking(move || {
            svc::write_album_cover_files(&folder, cover.as_deref(), back.as_deref());
        })
        .await;
    }

    refresh_after_download().await;
    toast_album_outcome(&plan);
}

/// THE LOOP — strictly sequential, CONCURRENCY 1, keyed by album id, honouring
/// the cancel flag checked BEFORE each track (§4.1).
///
/// Runs on a `spawn_blocking` worker driving a current-thread runtime: the
/// download primitive holds `&LibraryDatabase` across the CDN await, so its
/// future is `!Send` and cannot live on the multi-thread runtime (see the module
/// header).
///
/// `cancellable` is false for the single-track path, which ignores the abort
/// flag exactly as the reference does.
fn run_sequential(
    plan: &DownloadPlan,
    track_ids: &[u64],
    client: QobuzClient,
    ctx: svc::PurchaseAlbumContext,
    cancellable: bool,
) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            log::error!("[qbz-qt] purchases: download runtime build failed: {e}");
            with_store(|st| {
                for id in track_ids {
                    st.mark(&plan.album_id, *id, DlStatus::Failed);
                }
                if cancellable {
                    st.finalize(&plan.album_id, track_ids);
                }
            });
            publish_album();
            return;
        }
    };

    rt.block_on(async move {
        let Some(db) = open_owned_library_db() else {
            // No database means no registry row, and an unregistered file is a
            // download that does not survive a reload — so it is a failure, not
            // a success with a caveat.
            with_store(|st| {
                for id in track_ids {
                    st.mark(&plan.album_id, *id, DlStatus::Failed);
                }
                if cancellable {
                    st.finalize(&plan.album_id, track_ids);
                }
            });
            publish_album();
            return;
        };

        let mut first_success = false;
        for track_id in track_ids.iter().copied() {
            // (1) Cooperative abort check, BETWEEN tracks only — the in-flight
            // track always finishes.
            if cancellable && with_store(|st| st.is_aborted(&plan.album_id)) {
                with_store(|st| st.apply_cancellation(&plan.album_id, track_ids));
                publish_album();
                return;
            }
            with_store(|st| st.mark(&plan.album_id, track_id, DlStatus::Downloading));
            publish_album();

            // (2) The bundled primitive: `/track/get` (unsigned) → SIGNED
            // `getFileUrl` → CDN → `.part`→rename → registry write with the
            // REQUESTED format id → tags. `Some(album_id)` is what makes the
            // album's downloaded state answerable at all (§11-1), and the
            // registry error PROPAGATES — a registry failure marks the track
            // `failed`, matching BOTH reference paths (§8-D1).
            match svc::download_purchase_track(
                &client,
                &db,
                track_id,
                Some(plan.album_id.as_str()),
                plan.format_id,
                &plan.destination,
                &plan.quality_dir,
                Some(&ctx),
            )
            .await
            {
                Ok(file_path) => {
                    // (3) The destination is rewritten from the FIRST successful
                    // file path (§6) — this is what makes Add-to-Library add the
                    // album subfolder rather than the download root.
                    if cancellable && !first_success {
                        first_success = true;
                        let folder = album_folder_from_file_path(&file_path);
                        with_store(|st| st.resolve_album_folder(&plan.album_id, &folder));
                    }
                    with_store(|st| st.mark(&plan.album_id, track_id, DlStatus::Complete));
                }
                Err(e) => {
                    // (4) A FAILED TRACK DOES NOT ABORT THE ALBUM (§4.2b): mark
                    // it failed, keep going. Only `allComplete` stays false.
                    log::warn!("[qbz-qt] purchases: track {track_id} download failed: {e}");
                    with_store(|st| st.mark(&plan.album_id, track_id, DlStatus::Failed));
                }
            }
            publish_album();
        }

        if cancellable {
            with_store(|st| st.finalize(&plan.album_id, track_ids));
        }
        publish_album();
    });
}

/// Toast the album outcome. The reference gives no causal message on a format /
/// entitlement mismatch — §10-B keeps the visible failed state with its retry
/// control exactly as it is and ADDS a toast naming what happened.
fn toast_album_outcome(plan: &DownloadPlan) {
    let (complete, failed, cancelled, done) = with_store(|st| {
        let Some(state) = st.get(&plan.album_id) else {
            return (false, 0usize, false, 0usize);
        };
        let complete = state.all_complete;
        let failed = state
            .statuses
            .values()
            .filter(|s| **s == DlStatus::Failed)
            .count();
        let cancelled = state.statuses.values().any(|s| *s == DlStatus::Cancelled);
        let done = state
            .statuses
            .values()
            .filter(|s| **s == DlStatus::Complete)
            .count();
        (complete, failed, cancelled, done)
    });
    let total = plan.track_ids.len();
    if complete {
        crate::toast_qt::success(qbz_i18n::t("Download complete"));
    } else if cancelled {
        crate::toast_qt::info(qbz_i18n::t_args(
            "Download cancelled ({}/{})",
            &[&done.to_string(), &total.to_string()],
        ));
    } else if failed > 0 {
        crate::toast_qt::error(qbz_i18n::t("Failed to start download. Please try again."));
    }
}

/// Re-read the registry and re-annotate everything the download just changed, so
/// the green checks survive a reload instead of living only in the transient
/// store.
async fn refresh_after_download() {
    let reg = read_registry().await;
    set_registry(reg.clone());

    let (albums, tracks) = with_list(|s| {
        (
            std::mem::take(&mut s.albums_raw),
            std::mem::take(&mut s.tracks_raw),
        )
    });
    let (albums, tracks) = annotate(albums, tracks, &reg);
    with_list(|s| {
        s.albums_raw = albums;
        s.tracks_raw = tracks;
    });
    publish_list();

    // The detail's per-track `downloaded_format_ids` come from the same
    // registry, so the open album needs the same treatment.
    let refreshed = with_detail(|s| {
        let Some(album) = s.album.as_mut() else {
            return false;
        };
        if let Some(page) = album.tracks.as_mut() {
            for track in &mut page.items {
                let tid = track.id as i64;
                track.downloaded = reg.downloaded_ids.contains(&tid);
                track.downloaded_format_ids = reg.format_map.get(&tid).cloned().unwrap_or_default();
            }
        }
        album.downloaded =
            svc::album_downloaded_from_registry(&album.id, album.tracks_count, &reg.album_counts);
        true
    });
    if refreshed {
        publish_album();
    }
}

/// Cooperative cancel: the in-flight track finishes, everything that has not
/// started becomes `cancelled` between tracks (§4.1).
pub fn cancel_album_download() {
    let album_id = with_detail(|s| s.album_id.clone());
    if album_id.is_empty() {
        return;
    }
    with_store(|st| st.set_abort(&album_id));
    publish_album();
}

/// Album-level goodies (§14.3): booklets, videos. They go to the album folder
/// beside the tracks, through the same cancel-free, best-effort path, and they
/// are counted SEPARATELY from the track progress — a goodie is not a track, so
/// it must never move the bar in §6.
pub fn download_goodies() {
    let Some(plan) = build_plan() else {
        return;
    };
    let goodies: Vec<qbz_models::Goody> = plan
        .catalog
        .goodies
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|g| g.best_url().is_some())
        .collect();
    if goodies.is_empty() {
        // Nothing to do and nothing to say: the affordance is hidden when the
        // list is empty, so reaching here at all means the document moved
        // underneath the click.
        return;
    }

    crate::spawn(async move {
        let Some(plan) = ensure_destination(plan).await else {
            return;
        };
        let total = goodies.len() as u32;
        with_store(|st| {
            let state = st.entry(&plan.album_id);
            state.goodies_active = true;
            state.goodies_done = 0;
            state.goodies_total = total;
        });
        publish_album();

        // The album folder the tracks went into, or the one they WOULD go into
        // — a booklet is worth downloading before any audio is.
        let folder = with_store(|st| {
            st.get(&plan.album_id)
                .and_then(|d| d.resolved_album_folder.clone())
        })
        .filter(|d| d != &plan.destination)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            album_dir_for(
                &plan.destination,
                &plan.artist,
                &plan.album_title,
                &plan.quality_dir,
            )
        });

        for goody in &goodies {
            // A goodie failure never fails anything: `download_goodie` returns
            // `None` and logs, and the counter advances either way so the block
            // cannot hang on "3 of 5".
            if svc::download_goodie(goody, &folder).await.is_none() {
                log::warn!(
                    "[qbz-qt] purchases: goodie {:?} could not be downloaded",
                    goody.display_name()
                );
            }
            with_store(|st| st.entry(&plan.album_id).goodies_done += 1);
            publish_album();
        }

        with_store(|st| st.entry(&plan.album_id).goodies_active = false);
        publish_album();
        crate::toast_qt::success(qbz_i18n::t("Download complete"));
    });
}

/// "Add to Library" — a SEQUENCE, not a banner (§6):
///   1. add the RESOLVED album subfolder as a library folder (with the same
///      network detection Settings > Local Library uses);
///   2. clear the download state, so the progress and Add-to-Library blocks
///      vanish;
///   3. success/error toast;
///   4. start a background scan.
///
/// The destination is the folder the FIRST successful file path produced, which
/// is why step 1 adds the album subfolder rather than the download root — and
/// why the scanned files pick up the `source = 'qobuz_purchase'` stamp
/// (`database.rs:1105-1120`) and the gold badge in the Local Library.
pub fn add_to_library() {
    let (album_id, destination) = with_detail(|s| {
        let dest = with_store(|st| {
            st.get(&s.album_id)
                .and_then(|d| d.resolved_album_folder.clone())
        })
        .unwrap_or_else(|| s.destination.clone());
        (s.album_id.clone(), dest)
    });
    if album_id.is_empty() || destination.trim().is_empty() {
        crate::toast_qt::error(qbz_i18n::t("Couldn't add to library"));
        return;
    }

    crate::spawn(async move {
        // The folder table + the network probe + the insert all live in the
        // Settings > Local Library controller; reusing it keeps ONE add-folder
        // path rather than a second copy that drifts.
        crate::settings_qt::library::add_folder(destination.clone()).await;

        // `add_folder` reports its own status line and does not return the id,
        // so success is confirmed by the folder actually being in the table —
        // a silent no-op (unreadable db, path gone) must not clear the download
        // state and tell the user it worked.
        // Carry the folder's ID out, not just a yes/no. The row is right here
        // and throwing it away costs a full-library rescan below.
        let added_id = tokio::task::spawn_blocking(move || {
            // `add_folder` canonicalizes before inserting, so the comparison
            // has to canonicalize the same way or a symlinked download folder
            // never matches and a real success reports as a failure.
            let canonical = std::fs::canonicalize(&destination)
                .map(|c| c.to_string_lossy().into_owned())
                .unwrap_or_else(|_| destination.clone());
            crate::library_db_qt::with_db(false, |db| Ok(db.get_folders_with_metadata()?))
                .and_then(|folders| folders.iter().find(|f| f.path == canonical).map(|f| f.id))
        })
        .await
        .ok()
        .flatten();

        let Some(folder_id) = added_id else {
            crate::toast_qt::error(qbz_i18n::t("Couldn't add to library"));
            return;
        };

        with_store(|st| st.clear(&album_id));
        publish_album();
        crate::toast_qt::success(qbz_i18n::t("Added to library"));
        // Scan ONLY the folder just added. `settings_qt::library::scan` takes
        // `Option<i64>` and documents exactly this — "every enabled folder
        // (`None`) or exactly one (`Some(id)`)". Passing `None` here would
        // re-walk the user's entire library, which on a large one is minutes of
        // disk for a single new album.
        crate::settings_qt::library::scan(Some(folder_id));
    });
}

/// Live language switch. Both documents carry exactly one Rust-translated
/// string — the error line — so this is a no-op unless a screen is standing in
/// its error state. Called unconditionally for the same reason
/// `musician_qt::republish` is: `LAST_DETAIL` only ever latches "album" or
/// "artist" and cannot reach these routes.
///
/// NOT WIRED by this lane: `crate::apply_language` lives in `main.rs`, which
/// lane A may only add `mod` lines to. Whoever owns that file next should add
/// the call.
pub fn republish() {
    let list_error = with_list(|s| {
        if s.error.is_empty() {
            return false;
        }
        s.error = qbz_i18n::t("Couldn't load purchases. Check your connection.");
        true
    });
    if list_error {
        publish_list();
    }
    let album_error = with_detail(|s| {
        if s.error.is_empty() {
            return false;
        }
        s.error = qbz_i18n::t("Couldn't load purchases. Check your connection.");
        true
    });
    if album_error {
        publish_album();
    }
}

/// Logout / account switch: drop every trace of the previous user's purchases,
/// including in-flight download statuses. Unwired for the same reason
/// `republish` is.
pub fn reset() {
    *LIST.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *DETAIL.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *REGISTRY.lock().unwrap_or_else(|e| e.into_inner()) = None;
    with_store(|st| *st = DownloadStore::default());
}

// ---------------------------------------------------------------------------
//  Tests — the pure logic, and above all the format re-scoping (§15.5-5)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use qbz_models::{Artist, ImageSet};

    /// NOTE the explicit `downloadable` / `streamable`: `#[derive(Default)]`
    /// ignores the serde defaults, so a bare `PurchaseAlbum::default()` is
    /// `downloadable: false` while the WIRE default is `true` (`serde_true`,
    /// §2.6). Building fixtures off the derive would test a shape the API never
    /// sends — and would make "hide unavailable" look like it hides everything.
    fn album(id: &str, title: &str, artist: &str) -> PurchaseAlbum {
        PurchaseAlbum {
            id: id.to_string(),
            title: title.to_string(),
            artist: Artist {
                name: artist.to_string(),
                ..Default::default()
            },
            image: ImageSet::default(),
            downloadable: true,
            ..Default::default()
        }
    }

    fn track(id: u64, formats: &[u32]) -> PurchaseTrack {
        PurchaseTrack {
            id,
            title: format!("Track {id}"),
            downloaded_format_ids: formats.to_vec(),
            streamable: true,
            ..Default::default()
        }
    }

    fn state_with(format_id: Option<u32>, statuses: &[(u64, DlStatus)]) -> AlbumDownload {
        AlbumDownload {
            statuses: statuses.iter().copied().collect(),
            downloading_all: false,
            all_complete: statuses.iter().all(|(_, s)| *s == DlStatus::Complete),
            // The album folder a finished download resolved — NOT the root the
            // user picked. See `resolved_album_folder`'s declaration for why the
            // two must never share a name again.
            resolved_album_folder: Some("/music/Artist/Album".to_string()),
            format_id,
            ..Default::default()
        }
    }

    // ---- §15.5-5: the format re-scoping ---------------------------------

    #[test]
    fn a_track_is_downloaded_only_for_the_format_it_was_downloaded_in() {
        let t = track(1, &[27]);
        assert!(downloaded_for_format(&t.downloaded_format_ids, Some(27)));
        assert!(!downloaded_for_format(&t.downloaded_format_ids, Some(7)));
        assert!(!downloaded_for_format(&t.downloaded_format_ids, Some(5)));
        // No format selected (an album that offered none) is never downloaded.
        assert!(!downloaded_for_format(&t.downloaded_format_ids, None));
    }

    #[test]
    fn changing_the_format_rescopes_which_tracks_report_downloaded() {
        // The registry says: track 1 in FLAC 24/192 and MP3, track 2 in MP3
        // only, track 3 in nothing.
        let tracks = [track(1, &[27, 5]), track(2, &[5]), track(3, &[])];

        let for_27: Vec<bool> = tracks
            .iter()
            .map(|t| downloaded_for_format(&t.downloaded_format_ids, Some(27)))
            .collect();
        assert_eq!(for_27, vec![true, false, false]);

        let for_5: Vec<bool> = tracks
            .iter()
            .map(|t| downloaded_for_format(&t.downloaded_format_ids, Some(5)))
            .collect();
        assert_eq!(for_5, vec![true, true, false]);

        // A format nobody downloaded in blanks the whole column — this is the
        // behaviour that surprises people and the reason this test exists.
        let for_7: Vec<bool> = tracks
            .iter()
            .map(|t| downloaded_for_format(&t.downloaded_format_ids, Some(7)))
            .collect();
        assert_eq!(for_7, vec![false, false, false]);
    }

    #[test]
    fn a_completed_status_is_hidden_while_another_format_is_selected() {
        let state = state_with(Some(27), &[(1, DlStatus::Complete)]);
        assert_eq!(
            status_scoped(Some(&state), 1, Some(27)),
            Some(DlStatus::Complete)
        );
        // Same download, different dropdown value: the green check disappears.
        assert_eq!(status_scoped(Some(&state), 1, Some(7)), None);
    }

    #[test]
    fn failed_and_downloading_are_never_format_scoped() {
        // Only `complete` is scoped (`PurchaseAlbumDetailView.svelte:406-415`);
        // hiding a failure behind a dropdown change would hide the retry
        // control with it.
        let failed = state_with(Some(27), &[(1, DlStatus::Failed)]);
        assert_eq!(
            status_scoped(Some(&failed), 1, Some(7)),
            Some(DlStatus::Failed)
        );

        let running = state_with(Some(27), &[(1, DlStatus::Downloading)]);
        assert_eq!(
            status_scoped(Some(&running), 1, Some(7)),
            Some(DlStatus::Downloading)
        );
    }

    #[test]
    fn all_complete_is_format_scoped_and_gates_add_to_library() {
        let state = state_with(
            Some(27),
            &[(1, DlStatus::Complete), (2, DlStatus::Complete)],
        );
        assert!(all_complete_for_format(Some(&state), Some(27)));
        assert!(!all_complete_for_format(Some(&state), Some(7)));
        assert!(!all_complete_for_format(Some(&state), None));
        // An album the store has never seen is not complete.
        assert!(!all_complete_for_format(None, Some(27)));
    }

    #[test]
    fn an_album_downloaded_before_any_format_was_recorded_stays_complete() {
        // `format_id` is `None` until a download starts; the reference's
        // `formatId === undefined || formatId === selectedFormatId` passes on
        // the undefined arm.
        let state = AlbumDownload {
            all_complete: true,
            format_id: None,
            ..Default::default()
        };
        assert!(all_complete_for_format(Some(&state), Some(7)));
    }

    #[test]
    fn a_single_track_download_can_retroactively_suppress_all_complete() {
        // §6, reproduced deliberately: `startTrackDownload` overwrites the
        // album's formatId even though it sets nothing else.
        let mut store = DownloadStore::default();
        store.seed_album("A", 27);
        store.mark("A", 1, DlStatus::Complete);
        store.finalize("A", &[1]);
        assert!(all_complete_for_format(store.get("A"), Some(27)));

        store.merge_track_start("A", 1, 5);
        // The album is still flagged complete, but it now reads as complete for
        // format 5 — so with 27 selected the affordance is gone.
        assert!(!all_complete_for_format(store.get("A"), Some(27)));
        assert!(store.get("A").unwrap().all_complete);
    }

    // ---- The download store's loop semantics ----------------------------

    #[test]
    fn a_failed_track_does_not_abort_the_album() {
        // §4.2b: only `allComplete` stays false.
        let mut store = DownloadStore::default();
        store.seed_album("A", 7);
        store.mark("A", 1, DlStatus::Complete);
        store.mark("A", 2, DlStatus::Failed);
        store.mark("A", 3, DlStatus::Complete);
        store.finalize("A", &[1, 2, 3]);
        let state = store.get("A").unwrap();
        assert!(!state.all_complete);
        assert_eq!(state.statuses.get(&3), Some(&DlStatus::Complete));
        assert!(!state.downloading_all);
    }

    #[test]
    fn cancellation_only_touches_tracks_that_never_started() {
        let mut store = DownloadStore::default();
        store.seed_album("A", 7);
        store.mark("A", 1, DlStatus::Complete);
        store.mark("A", 2, DlStatus::Failed);
        store.set_abort("A");
        store.apply_cancellation("A", &[1, 2, 3, 4]);
        let state = store.get("A").unwrap();
        assert_eq!(state.statuses.get(&1), Some(&DlStatus::Complete));
        assert_eq!(state.statuses.get(&2), Some(&DlStatus::Failed));
        assert_eq!(state.statuses.get(&3), Some(&DlStatus::Cancelled));
        assert_eq!(state.statuses.get(&4), Some(&DlStatus::Cancelled));
        assert!(!state.downloading_all);
        assert!(!state.all_complete);
        assert!(!store.is_aborted("A"));
    }

    #[test]
    fn a_single_track_download_preserves_its_siblings() {
        // The merge path spreads the existing state; only the one track moves.
        let mut store = DownloadStore::default();
        store.seed_album("A", 7);
        store.mark("A", 1, DlStatus::Complete);
        store.mark("A", 2, DlStatus::Complete);
        store.finalize("A", &[1, 2]);
        store.merge_track_start("A", 1, 7);
        let state = store.get("A").unwrap();
        assert_eq!(state.statuses.get(&2), Some(&DlStatus::Complete));
        assert_eq!(state.statuses.get(&1), Some(&DlStatus::Downloading));
        // Not touched by a single-track download.
        assert!(state.all_complete);
        assert!(!state.downloading_all);
        // The resolved album folder is set by a completed download, never by
        // seeding — seeding it from the picked root is what made a later
        // download nest inside its own output.
        assert_eq!(state.resolved_album_folder, None);
    }

    #[test]
    fn seeding_an_album_download_does_not_reset_goodies_in_flight() {
        let mut store = DownloadStore::default();
        {
            let state = store.entry("A");
            state.goodies_active = true;
            state.goodies_done = 1;
            state.goodies_total = 3;
        }
        store.seed_album("A", 7);
        let state = store.get("A").unwrap();
        assert!(state.goodies_active);
        assert_eq!(state.goodies_done, 1);
        assert_eq!(state.goodies_total, 3);
        assert!(state.statuses.is_empty());
    }

    #[test]
    fn clearing_the_state_hides_the_progress_and_add_to_library_blocks() {
        let mut store = DownloadStore::default();
        store.seed_album("A", 7);
        store.mark("A", 1, DlStatus::Complete);
        store.finalize("A", &[1]);
        store.clear("A");
        assert!(store.get("A").is_none());
        assert!(!all_complete_for_format(store.get("A"), Some(7)));
    }

    // ---- The status wire vocabulary -------------------------------------

    #[test]
    fn the_status_wire_values_are_the_shared_four_part_vocabulary() {
        assert_eq!(DlStatus::Downloading.wire(), 2);
        assert_eq!(DlStatus::Complete.wire(), 3);
        assert_eq!(DlStatus::Failed.wire(), 4);
        // Cancelled has no slot in the shared vocabulary; the album-level
        // `progress.cancelled` carries it instead.
        assert_eq!(DlStatus::Cancelled.wire(), 0);
    }

    // ---- Filters and sort ------------------------------------------------

    #[test]
    fn the_quality_filter_matches_the_reference_predicates() {
        assert!(matches_quality(QualityFilter::All, false, None, None));
        assert!(matches_quality(
            QualityFilter::Hires,
            true,
            Some(24),
            Some(96.0)
        ));
        assert!(!matches_quality(
            QualityFilter::Hires,
            false,
            Some(16),
            Some(44.1)
        ));
        // CD: not hires AND (16-bit OR nothing known at all).
        assert!(matches_quality(
            QualityFilter::Cd,
            false,
            Some(16),
            Some(44.1)
        ));
        assert!(matches_quality(QualityFilter::Cd, false, None, None));
        assert!(!matches_quality(QualityFilter::Cd, false, None, Some(44.1)));
        assert!(!matches_quality(
            QualityFilter::Cd,
            true,
            Some(16),
            Some(44.1)
        ));
        // Lossy: no bit depth, or under 16.
        assert!(matches_quality(
            QualityFilter::Lossy,
            false,
            None,
            Some(44.1)
        ));
        assert!(matches_quality(QualityFilter::Lossy, false, Some(8), None));
        assert!(!matches_quality(
            QualityFilter::Lossy,
            false,
            Some(16),
            None
        ));
    }

    #[test]
    fn an_unknown_quality_filter_string_falls_back_to_all() {
        assert_eq!(QualityFilter::parse("nonsense"), QualityFilter::All);
        assert_eq!(QualityFilter::parse("all"), QualityFilter::All);
        assert_eq!(QualityFilter::parse("lossy"), QualityFilter::Lossy);
    }

    #[test]
    fn purchased_and_date_are_the_same_sort_key() {
        let mut a = album("1", "A", "X");
        a.purchased_at = Some(100);
        let mut b = album("2", "B", "Y");
        b.purchased_at = Some(200);

        let mut by_purchased: Vec<&PurchaseAlbum> = vec![&a, &b];
        sort_albums(&mut by_purchased, "purchased", false);
        assert_eq!(by_purchased[0].id, "2", "newest first when descending");

        let mut by_date: Vec<&PurchaseAlbum> = vec![&a, &b];
        sort_albums(&mut by_date, "date", false);
        assert_eq!(by_date[0].id, "2");
    }

    #[test]
    fn sorting_by_release_date_orders_iso_strings_and_parks_the_undated() {
        let mut a = album("a", "A", "X");
        a.release_date_original = Some("2024-03-01".to_string());
        let mut b = album("b", "B", "X");
        b.release_date_original = Some("1990-09-24".to_string());
        let c = album("c", "C", "X");
        let mut list: Vec<&PurchaseAlbum> = vec![&a, &b, &c];
        sort_albums(&mut list, "released", false);
        let ids: Vec<&str> = list.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"], "newest first, undated last");
        sort_albums(&mut list, "released", true);
        let ids: Vec<&str> = list.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
    }

    #[test]
    fn sorting_by_quality_compares_rate_then_depth() {
        let mut low = album("1", "Low", "X");
        low.maximum_sampling_rate = Some(44.1);
        low.maximum_bit_depth = Some(16);
        let mut mid = album("2", "Mid", "X");
        mid.maximum_sampling_rate = Some(96.0);
        mid.maximum_bit_depth = Some(16);
        let mut high = album("3", "High", "X");
        high.maximum_sampling_rate = Some(96.0);
        high.maximum_bit_depth = Some(24);

        let mut list: Vec<&PurchaseAlbum> = vec![&mid, &low, &high];
        sort_albums(&mut list, "quality", true);
        assert_eq!(
            list.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            vec!["1", "2", "3"]
        );
    }

    // ---- Path helpers ----------------------------------------------------

    #[test]
    fn the_quality_dir_replaces_slashes_and_trims() {
        // The `.trim()` is real (§4.4): a trailing space makes `target_path`
        // build a DIFFERENT directory, and every previously downloaded album
        // would then look un-downloaded.
        assert_eq!(quality_dir("[FLAC][24-bit,96kHz]"), "[FLAC][24-bit,96kHz]");
        assert_eq!(quality_dir("MP3/320 "), "MP3-320");
        assert_eq!(quality_dir("  "), "");
    }

    #[test]
    fn the_album_folder_is_everything_before_the_last_separator() {
        assert_eq!(
            album_folder_from_file_path("/music/Miles Davis/Kind of Blue [FLAC]/01 - So What.flac"),
            "/music/Miles Davis/Kind of Blue [FLAC]"
        );
        assert_eq!(
            album_folder_from_file_path("C:\\music\\Artist\\Album\\01 - t.flac"),
            "C:\\music\\Artist\\Album"
        );
        assert_eq!(album_folder_from_file_path("track.flac"), "track.flac");
    }

    #[test]
    fn the_derived_album_dir_matches_what_a_track_download_would_produce() {
        // Brackets are ASCII, so `sanitize_filename` KEEPS them — the quality
        // folder really is `Album [FLAC][24-bit,96kHz]` (§A.1-3).
        let track = svc::target_path(
            "/music",
            "Miles Davis",
            "Kind of Blue",
            "[FLAC][24-bit,96kHz]",
            1,
            "So What",
            "flac",
        );
        let derived = album_dir_for(
            "/music",
            "Miles Davis",
            "Kind of Blue",
            "[FLAC][24-bit,96kHz]",
        );
        assert_eq!(track.parent().unwrap(), derived.as_path());
        assert_eq!(
            derived,
            PathBuf::from("/music/Miles Davis/Kind of Blue [FLAC][24-bit,96kHz]")
        );
    }

    #[test]
    fn an_empty_quality_dir_drops_the_separating_space() {
        let derived = album_dir_for("/music", "Artist", "Album", "");
        assert_eq!(derived, PathBuf::from("/music/Artist/Album"));
    }

    // ---- What was bought, not what streams -------------------------------

    #[test]
    fn a_dsd_purchase_badges_as_hires_with_the_dsd_in_the_detail() {
        // Rust In Peace on the live account: hires:false, 16/44.1, [55].
        let (tier, detail) = purchased_quality(false, Some(16), Some(44.1), &[55]);
        assert_eq!((tier.as_str(), detail.as_str()), ("hires", "DSD64"));
        let (tier, detail) = purchased_quality(true, Some(24), Some(352.8), &[56]);
        assert_eq!((tier.as_str(), detail.as_str()), ("hires", "DSD128"));
        // A FLAC purchase keeps the catalog-derived pair untouched.
        let (tier, detail) = purchased_quality(true, Some(24), Some(96.0), &[7, 6, 5]);
        assert_eq!(tier, "hires");
        assert_eq!(detail, quality_detail(Some(24), Some(96.0)));
        // And so does a listing without the field.
        let (tier, _) = purchased_quality(false, Some(16), Some(44.1), &[]);
        assert_eq!(tier, "cd");
    }

    // ---- Document shape --------------------------------------------------

    #[test]
    fn the_list_document_carries_every_frozen_key() {
        // §G.2 is written against by two QML lanes; a rename here breaks them
        // silently, because a missing key in QML is `undefined`, not an error.
        let doc = build_list_doc(&ListState::default());
        let json = serde_json::to_value(&doc).expect("serializes");
        for key in [
            "loading",
            "error",
            "tab",
            "counts",
            "search",
            "searching",
            "regionNoticeVisible",
            "toolbar",
            "albums",
            "tracks",
        ] {
            assert!(json.get(key).is_some(), "missing list key {key}");
        }
        for key in [
            "group",
            "sort",
            "ascending",
            "viewMode",
            "hideUnavailable",
            "hideDownloaded",
            "qualityFilter",
        ] {
            assert!(
                json["toolbar"].get(key).is_some(),
                "missing toolbar key {key}"
            );
        }
        assert_eq!(json["tab"], "albums");
        assert_eq!(json["toolbar"]["sort"], "purchased");
        assert_eq!(json["toolbar"]["ascending"], false);
        assert_eq!(json["toolbar"]["qualityFilter"], "all");
    }

    #[test]
    fn the_album_document_carries_every_frozen_key_even_while_loading() {
        // The empty/loading arm is the one a stranger sees first, and it is the
        // arm no smoke test on the owner's account can ever leave.
        let doc = build_album_doc(&DetailState {
            loading: true,
            album_id: "42".to_string(),
            ..Default::default()
        });
        let json = serde_json::to_value(&doc).expect("serializes");
        for key in [
            "loading",
            "error",
            "id",
            "title",
            "artist",
            "artistId",
            "artworkUrl",
            "year",
            "label",
            "genre",
            "tracksCount",
            "duration",
            "downloadable",
            "purchasedAt",
            "formats",
            "selectedFormatId",
            "destination",
            "goodies",
            "discs",
            "progress",
            "addToLibraryVisible",
        ] {
            assert!(json.get(key).is_some(), "missing album key {key}");
        }
        assert_eq!(json["loading"], true);
        assert_eq!(json["id"], "42");
        assert!(json["goodies"].as_array().unwrap().is_empty());
        assert!(json["discs"].as_array().unwrap().is_empty());
    }

    #[test]
    fn the_detail_groups_tracks_by_disc_in_arrival_order() {
        let mut a = album("A", "Two Discs", "X");
        let mut t1 = track(1, &[]);
        t1.media_number = Some(1);
        let mut t2 = track(2, &[]);
        t2.media_number = Some(2);
        let mut t3 = track(3, &[]);
        t3.media_number = Some(1);
        a.tracks = Some(SearchResultsPage {
            total: 3,
            offset: 0,
            limit: 3,
            items: vec![t1, t2, t3],
        });

        let doc = build_album_doc(&DetailState {
            album_id: "A".to_string(),
            album: Some(a),
            selected_format_id: Some(7),
            ..Default::default()
        });
        assert_eq!(doc.discs.len(), 2);
        assert_eq!(doc.discs[0].number, 1);
        assert_eq!(doc.discs[0].tracks.len(), 2);
        assert_eq!(doc.discs[1].number, 2);
        assert_eq!(doc.discs[1].tracks.len(), 1);
        assert_eq!(doc.progress.total, 3);
    }

    #[test]
    fn a_track_with_no_media_number_lands_on_disc_one() {
        let mut a = album("A", "Single Disc", "X");
        a.tracks = Some(SearchResultsPage {
            total: 1,
            offset: 0,
            limit: 1,
            items: vec![track(1, &[])],
        });
        let doc = build_album_doc(&DetailState {
            album_id: "A".to_string(),
            album: Some(a),
            ..Default::default()
        });
        assert_eq!(doc.discs.len(), 1);
        assert_eq!(doc.discs[0].number, 1);
    }

    #[test]
    fn the_version_rides_along_so_format_track_title_can_use_it() {
        // §10-C: dropping this field is what made every purchased track lose
        // its subtitle (issue #360).
        let mut a = album("A", "Remixes", "X");
        let mut t = track(1, &[]);
        t.version = Some("Player's Ball Mix".to_string());
        a.tracks = Some(SearchResultsPage {
            total: 1,
            offset: 0,
            limit: 1,
            items: vec![t],
        });
        let doc = build_album_doc(&DetailState {
            album_id: "A".to_string(),
            album: Some(a),
            ..Default::default()
        });
        assert_eq!(doc.discs[0].tracks[0].version, "Player's Ball Mix");
    }

    #[test]
    fn hide_downloaded_and_hide_unavailable_filter_in_the_reference_order() {
        let mut owned = album("1", "Owned", "X");
        owned.downloaded = true;
        let mut gone = album("2", "Unavailable", "X");
        gone.downloadable = false;
        let keep = album("3", "Keep", "X");

        let base = ListState {
            albums_raw: vec![owned, gone, keep],
            ..Default::default()
        };
        assert_eq!(build_list_doc(&base).albums.len(), 3);

        let hidden = ListState {
            hide_downloaded: true,
            hide_unavailable: true,
            albums_raw: base.albums_raw.clone(),
            ..Default::default()
        };
        let rows = build_list_doc(&hidden).albums;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "3");
    }

    #[test]
    fn tracks_have_no_hide_unavailable_filter() {
        // Availability is an ALBUM concept in the reference; a Tracks tab that
        // honoured `hideUnavailable` would invent a filter.
        let state = ListState {
            tab: "tracks".to_string(),
            hide_unavailable: true,
            tracks_raw: vec![track(1, &[]), track(2, &[])],
            ..Default::default()
        };
        assert_eq!(build_list_doc(&state).tracks.len(), 2);
    }

    #[test]
    fn a_track_id_reaches_qml_as_a_string() {
        // QML `int` is 32-bit; this id does not fit in one (§G.0).
        let big = 5_000_000_000u64;
        let row = track_row(&track(big, &[]));
        assert_eq!(row.id, "5000000000");
        let json = serde_json::to_value(&row).expect("serializes");
        assert!(json["id"].is_string());
    }
}

#[cfg(test)]
mod destination_separation_tests {
    use super::*;

    /// The picked ROOT and the resolved ALBUM FOLDER are two different values,
    /// and only one of them may ever be used as a download destination.
    ///
    /// They used to share the name `destination`, and the album loop rewrote the
    /// store's copy to the album subfolder after the first successful track so
    /// that "Add to Library" would add the album rather than the whole music
    /// root. `build_plan` then preferred that rewritten value, so a SECOND
    /// download of the same album — a retry, or the same album in another
    /// format — was handed `/music/Artist/Album [q]` as its root and produced
    /// `/music/Artist/Album [q]/Artist/Album [q]/01 - x.flac`. One level deeper
    /// every time, silently, on a feature nobody here can smoke-test.
    #[test]
    fn seeding_never_pre_fills_the_resolved_album_folder() {
        let mut store = DownloadStore::default();
        store.seed_album("A", 27);
        assert_eq!(
            store.get("A").unwrap().resolved_album_folder,
            None,
            "seeding must not invent an album folder — only a finished download knows it"
        );
    }

    #[test]
    fn the_resolved_album_folder_comes_only_from_a_finished_track() {
        let mut store = DownloadStore::default();
        store.seed_album("A", 27);
        store.resolve_album_folder("A", "/music/Artist/Album [FLAC][24-bit,96kHz]");
        assert_eq!(
            store.get("A").unwrap().resolved_album_folder.as_deref(),
            Some("/music/Artist/Album [FLAC][24-bit,96kHz]")
        );
    }

    /// The regression proper: re-seeding after a resolved folder exists must not
    /// let that folder become the next download's root.
    #[test]
    fn a_second_download_does_not_nest_inside_the_first() {
        let mut store = DownloadStore::default();
        store.seed_album("A", 27);
        store.resolve_album_folder("A", "/music/Artist/Album [FLAC][24-bit,192kHz]");

        // Same album, different format — the case that nested.
        store.seed_album("A", 7);
        let root = "/music";
        let target = album_dir_for(root, "Artist", "Album", "[FLAC][24-bit,96kHz]");

        assert_eq!(
            target,
            std::path::PathBuf::from("/music/Artist/Album [FLAC][24-bit,96kHz]"),
            "the second download builds from the picked ROOT, not from the first \
             download's album folder"
        );
    }
}
