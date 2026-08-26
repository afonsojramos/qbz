//! Purchases orchestration service (Slice 3 of the Purchases port).
//!
//! Frontend-agnostic glue between the `qbz-qobuz` purchase HTTP methods
//! (Slice 2) and the `qbz-library` `downloaded_purchases` registry. This crate
//! is the only one that already depends on BOTH `qbz-qobuz` and `qbz-library`,
//! so the orchestration lives here (ADR-006); the `qbz-slint` controller calls
//! these fns directly, never wrapping a `src-tauri` command.
//!
//! Slice 3 scope: the pagination-glue helpers around the client's
//! `get_user_purchases_*` methods, and the pure `filter_purchase_response`
//! search filter (`v2_filter_purchase_response`, ported from
//! `src-tauri/src/commands_v2/legacy_compat.rs:2627`).
//!
//! Slice 4 scope (pure, no I/O): `synth_formats` (the §4.9 client-side
//! format-synthesis table, ported from `v2_purchases_get_formats`
//! `legacy_compat.rs:2953`) and `apply_download_flags` (the §3.4 download-flag
//! annotation, ported from `v2_apply_purchase_download_flags`
//! `legacy_compat.rs:2594`).
//!
//! Slice 5 scope: the single-track download primitive
//! `download_purchase_track` (the canonical getFileUrl → CDN → `.part`→rename →
//! registry pipeline, ported from `v2_download_purchase_track_impl`
//! `legacy_compat.rs:2651` PLUS the registry write that `v2_purchases_download_track`
//! `legacy_compat.rs:3013` performs after it), and the pure path/extension
//! helpers `target_path` (§7.3 `v2_purchase_target_path`) and
//! `purchase_extension` (§7.1.5 `v2_purchase_extension`). The album loop, cancel,
//! and per-track progress live in the `qbz-slint` controller (Slice 7), not here
//! — this crate only exposes the single-track primitive.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use qbz_library::{write_purchase_tags, LibraryDatabase, PurchaseTagWrite};
use qbz_models::{
    Album, PurchaseAlbum, PurchaseFormatOption, PurchaseResponse, PurchaseTrack, SearchResultsPage,
    Track,
};
use qbz_qobuz::QobuzClient;
use qbz_qobuz::Result as QobuzResult;

use crate::metadata::sanitize_filename;

/// Fetch ONE purchases page, typed by purchase kind (`"albums"` / `"tracks"`,
/// or `None` for both). Thin pass-through to the client's
/// `get_user_purchases_page_typed` so the controller has a single service entry
/// point and never reaches into `qbz-qobuz` directly. Mirrors command #1's
/// single-page branch (both `limit` + `offset` present).
pub async fn get_user_purchases_page(
    client: &QobuzClient,
    limit: u32,
    offset: u32,
    kind: Option<&str>,
) -> QobuzResult<PurchaseResponse> {
    client
        .get_user_purchases_page_typed(kind, limit, offset)
        .await
}

/// Fetch ALL purchases (both types) by paginating through the Qobuz API.
/// Pass-through to the client's `get_user_purchases_all` (command #1's
/// paginate-all branch, used by search). The two-call per-type totals quirk is
/// preserved inside the client; this glue does not collapse it.
pub async fn get_user_purchases_all(client: &QobuzClient) -> QobuzResult<PurchaseResponse> {
    client.get_user_purchases_all().await
}

/// Fetch ALL purchases for a SINGLE type by paginating (`"albums"` /
/// `"tracks"`). Pass-through to `get_user_purchases_all_typed` — the primary
/// per-tab list-load path (command #3). The OTHER type's `total` is forced to 0
/// in the returned envelope (the root of the totals gotcha); the controller
/// recovers both totals via the two separate `get_ids(1,0,type)` calls in
/// `load_purchases_metadata`.
pub async fn get_user_purchases_by_type(
    client: &QobuzClient,
    purchase_type: &str,
) -> QobuzResult<PurchaseResponse> {
    client.get_user_purchases_all_typed(purchase_type).await
}

/// Read the per-type purchase TOTAL via a single `getUserPurchasesIds`
/// page (`limit=1, offset=0, type`). The items are opaque; only `.total` for
/// the matching type is read. Returns `None` on any error (the controller falls
/// back to 0 / the response length — `loadPurchasesMetadata`'s `.catch(()=>null)`).
///
/// GOTCHA (per-type totals): this MUST be called once per type. A single
/// unfiltered `limit=1` ids call carries only the FIRST type's total, so the
/// controller fires two of these — `get_purchase_total(client, "albums")` and
/// `get_purchase_total(client, "tracks")` — never one combined call.
pub async fn get_purchase_total(client: &QobuzClient, purchase_type: &str) -> Option<u32> {
    match client
        .get_user_purchases_ids_page_typed(Some(purchase_type), 1, 0)
        .await
    {
        Ok(resp) => match purchase_type {
            "albums" => Some(resp.albums.total),
            "tracks" => Some(resp.tracks.total),
            _ => None,
        },
        Err(e) => {
            log::warn!("[Purchases] get_purchase_total({purchase_type}) failed: {e}");
            None
        }
    }
}

/// Filter a `PurchaseResponse` in-memory by a search query. Pure — no I/O.
///
/// Ported byte-for-byte from `v2_filter_purchase_response`
/// (`src-tauri/src/commands_v2/legacy_compat.rs:2627`):
///   * the query is lowercased once;
///   * an album is RETAINED when its lowercased `title` OR `artist.name`
///     contains the query (case-insensitive substring);
///   * a track is RETAINED when its lowercased `title` OR `performer.name` OR
///     (if present) `album.title` contains the query;
///   * each surviving page's `total` is reset to its filtered `items.len()` and
///     `offset` is reset to 0 (`limit` is left untouched, matching the source).
///
/// No fuzzy matching, no ranking. An empty/whitespace query is handled by the
/// caller (it skips the filter entirely), so this fn always applies the
/// substring test as written.
pub fn filter_purchase_response(response: PurchaseResponse, query: &str) -> PurchaseResponse {
    let needle = query.to_lowercase();

    let albums: Vec<PurchaseAlbum> = response
        .albums
        .items
        .into_iter()
        .filter(|album| {
            album.title.to_lowercase().contains(&needle)
                || album.artist.name.to_lowercase().contains(&needle)
        })
        .collect();

    let tracks: Vec<PurchaseTrack> = response
        .tracks
        .items
        .into_iter()
        .filter(|track| {
            track.title.to_lowercase().contains(&needle)
                || track.performer.name.to_lowercase().contains(&needle)
                || track
                    .album
                    .as_ref()
                    .map(|a| a.title.to_lowercase().contains(&needle))
                    .unwrap_or(false)
        })
        .collect();

    PurchaseResponse {
        albums: SearchResultsPage {
            total: albums.len() as u32,
            offset: 0,
            limit: response.albums.limit,
            items: albums,
        },
        tracks: SearchResultsPage {
            total: tracks.len() as u32,
            offset: 0,
            limit: response.tracks.limit,
            items: tracks,
        },
    }
}

/// Synthesize the downloadable format options for a purchased album,
/// client-side from `/album/get` (command #6 `v2_purchases_get_formats`,
/// `legacy_compat.rs:2953-3001`). There is NO Qobuz formats endpoint — the
/// options are derived purely from `album.hires` + `album.maximum_sampling_rate`.
///
/// Order is load-bearing (it IS the dropdown order; the frontend default-selects
/// `formats[0]`, so the highest available quality is the default):
///   * id **27** `[FLAC][24-bit,192kHz]` — only if `hires && max_sr > 96.0`.
///   * id **7**  `[FLAC][24-bit,96kHz]`  — only if `hires`.
///   * id **6**  `[FLAC][16-bit,44.1kHz]` — always.
///   * id **5**  `[MP3][320kbps]`         — always.
///
/// The ids feed `getFileUrl`'s `format_id`; the `label` (with `/`→`-`) becomes
/// the `qualityDir` subfolder, so both ids AND label strings are reproduced
/// EXACTLY (port idéntico — these are not cosmetic).
pub fn synth_formats(album: &Album) -> Vec<PurchaseFormatOption> {
    let mut formats = Vec::new();

    if album.hires && album.maximum_sampling_rate.unwrap_or(0.0) > 96.0 {
        formats.push(PurchaseFormatOption {
            id: 27,
            label: "[FLAC][24-bit,192kHz]".to_string(),
            bit_depth: Some(24),
            sampling_rate: Some(192.0),
        });
    }

    if album.hires {
        formats.push(PurchaseFormatOption {
            id: 7,
            label: "[FLAC][24-bit,96kHz]".to_string(),
            bit_depth: Some(24),
            sampling_rate: Some(96.0),
        });
    }

    formats.push(PurchaseFormatOption {
        id: 6,
        label: "[FLAC][16-bit,44.1kHz]".to_string(),
        bit_depth: Some(16),
        sampling_rate: Some(44.1),
    });

    formats.push(PurchaseFormatOption {
        id: 5,
        label: "[MP3][320kbps]".to_string(),
        bit_depth: None,
        sampling_rate: None,
    });

    formats
}

/// Annotate a `PurchaseResponse` in-place with frontend-computed download flags
/// (§3.4 `v2_apply_purchase_download_flags`, `legacy_compat.rs:2594-2625`, used
/// by commands #1 / #4). Pure — no I/O. The frontend OVERRIDES any backend
/// `downloaded` value here.
///
/// Per track:
///   * `downloaded = downloaded_ids.contains(track.id)`;
///   * `downloaded_format_ids = format_map.get(track.id).cloned().unwrap_or_default()`.
///
/// Per album: **DELIBERATE DIVERGENCE from the reference, contract §11-1, ruled
/// by the owner on 2026-08-16.**
///
/// The reference collected the ids of `response.tracks.items` whose
/// `track.album.id == album.id` and set
/// `album.downloaded = !ids.is_empty() && all ids ∈ downloaded_ids`.
/// That predicate is UNSATISFIABLE on the screen that uses it: the Albums tab
/// loads `getUserPurchases?type=albums`, and that response omits the `tracks`
/// key entirely (measured against a live account, §2.5b). With no sibling
/// tracks page the id set is always empty, so the empty-set rule pins
/// `downloaded` to `false` — forever, and silently, because the field is
/// optional behind a lenient deserializer. The visible consequence in the
/// reference is that no album card can ever show as downloaded and the
/// "Hide downloaded" filter can never hide anything. The guide-dog user would
/// have had no way to notice a filter that simply does nothing.
///
/// So the album rule reads the LOCAL REGISTRY instead, which is the only party
/// that actually knows: `album_counts` maps a purchase album id to its count of
/// DISTINCT downloaded tracks (`get_downloaded_purchase_album_counts`), and an
/// album is downloaded when that count covers its `tracks_count`.
///
/// Two properties worth keeping in mind:
///   * it is format-AGNOSTIC, matching the reference's list-level semantics (the
///     detail screen is the format-scoped one);
///   * an absent or zero `tracks_count` yields `false`. `PurchaseAlbum.tracks_count`
///     is an `Option`, and without it there is no denominator — "every track is
///     downloaded" is unprovable, so the answer is no. That is the safe direction
///     (it under-claims rather than showing a green mark for an album that may be
///     half on disk) and it preserves the reference's empty-set rule.
///
/// **Status, stated so this comment does not describe behaviour that does not
/// exist yet:** as of 2026-08-16 this function has NO production caller. The Qt
/// controller that will call it on every list path — so that browsing and
/// searching cannot disagree about the same album — is not written. The shipping
/// Slint controller does NOT use this function; it computes album state itself
/// with its own copy of the old nested-tracks predicate
/// (`crates/qbz/src/purchases.rs`, `enrich_albums`), and therefore still shows
/// the unsatisfiable behaviour described above. Until the Qt side lands the two
/// frontends will disagree, and that is expected rather than a bug to chase.
///
/// `downloaded_ids`/`format_map` are keyed by `i64` (registry track ids); track
/// ids are `u64` and compared via `track.id as i64`, exactly as the source.
pub fn apply_download_flags(
    response: &mut PurchaseResponse,
    downloaded_ids: &HashSet<i64>,
    format_map: &HashMap<i64, Vec<u32>>,
    album_counts: &HashMap<String, u32>,
) {
    for track in &mut response.tracks.items {
        let tid = track.id as i64;
        track.downloaded = downloaded_ids.contains(&tid);
        track.downloaded_format_ids = format_map.get(&tid).cloned().unwrap_or_default();
    }

    for album in &mut response.albums.items {
        album.downloaded = album_downloaded_from_registry(&album.id, album.tracks_count, album_counts);
    }
}

/// The shared "is this purchased album fully downloaded" predicate (§11-1).
/// Pure, so both the list annotation and the detail builder can agree, and so it
/// is unit-testable without a database.
pub fn album_downloaded_from_registry(
    album_id: &str,
    tracks_count: Option<u32>,
    album_counts: &HashMap<String, u32>,
) -> bool {
    match tracks_count {
        Some(expected) if expected > 0 => {
            album_counts.get(album_id).copied().unwrap_or(0) >= expected
        }
        _ => false,
    }
}

/// Build the detail-view `PurchaseAlbum` from a full catalog `Album` plus the
/// purchases listing, then annotate it with the local download flags (§3.3
/// command #5 `v2_purchases_get_album`, `legacy_compat.rs:2846-2951`). Pure — no
/// I/O; the caller fetches the `Album` + `PurchaseResponse` and reads the
/// registry, then hands the derived `downloaded_ids` + `format_map` here, mirroring
/// the controller-thin pattern of `apply_download_flags`.
///
/// Mapping rules (verbatim from command #5):
///   * the nested track list comes from `album.tracks.items` mapped to
///     `PurchaseTrack`; `performer` defaults when the catalog track has none;
///     per-track `purchased_at` is the album-level meta value;
///   * the `version`/subtitle IS carried across. This is a deliberate divergence
///     from the reference: Tauri's frontend type declares `version` and its detail
///     view renders `formatTrackTitle(title, version)`, but the mapping here never
///     populated it, so every purchased track lost its subtitle (issue #360). The
///     catalog track does carry it (`qbz_models::Track::version`), so the fix is a
///     field copy. Contract §10-C rules this in.
///   * `downloadable = purchase_meta.map(|m| m.downloadable).unwrap_or(true)`
///     (defaults TRUE when the album is not found in the purchases listing);
///   * `purchased_at = purchase_meta.and_then(|m| m.purchased_at)`;
///   * the synthesized tracks page is `offset=0, limit=len, total=len`;
///   * after the registry annotation: per-track
///     `downloaded = downloaded_ids.contains(track.id)`,
///     `downloaded_format_ids = format_map.get(track.id).cloned().unwrap_or_default()`,
///     and album `downloaded = !tracks.is_empty() && all track ids ∈ downloaded_ids`.
///
/// `purchase_meta` is found by `purchases.albums.items[i].id == album.id`.
pub fn build_purchase_album(
    album: &Album,
    purchases: &PurchaseResponse,
    downloaded_ids: &HashSet<i64>,
    format_map: &HashMap<i64, Vec<u32>>,
) -> PurchaseAlbum {
    let purchase_meta = purchases
        .albums
        .items
        .iter()
        .find(|item| item.id == album.id);

    let mut tracks_items: Vec<PurchaseTrack> = album
        .tracks
        .as_ref()
        .map(|tracks| {
            tracks
                .items
                .iter()
                .map(|track| PurchaseTrack {
                    id: track.id,
                    title: track.title.clone(),
                    version: track.version.clone(),
                    track_number: track.track_number,
                    media_number: track.media_number,
                    duration: track.duration,
                    performer: track.performer.clone().unwrap_or_default(),
                    album: track.album.clone(),
                    hires: track.hires,
                    maximum_sampling_rate: track.maximum_sampling_rate,
                    maximum_bit_depth: track.maximum_bit_depth,
                    // `is_streamable()`, and the choice is load-bearing here:
                    // `PurchaseTrack.streamable` defaults TRUE on purpose (the
                    // §2.6 split in `qbz-models`) so a terse purchases payload
                    // never makes a purchased row unclickable — and this bridge
                    // was handing it the catalog `bool`, whose absence meant
                    // `false`, re-introducing exactly that inversion through
                    // the back door. Nobody would have caught it by clicking:
                    // Qobuz Purchases is not sold in the owner's region.
                    streamable: track.is_streamable(),
                    // Server-derived; set below from the registry.
                    downloaded: false,
                    downloaded_format_ids: Vec::new(),
                    purchased_at: purchase_meta.and_then(|item| item.purchased_at),
                })
                .collect()
        })
        .unwrap_or_default();

    // Per-track registry annotation (frontend OVERRIDES backend, §3.4).
    for track in &mut tracks_items {
        let tid = track.id as i64;
        track.downloaded = downloaded_ids.contains(&tid);
        track.downloaded_format_ids = format_map.get(&tid).cloned().unwrap_or_default();
    }
    // Album downloaded = non-empty AND every nested track id is in the registry
    // (the all-tracks-present rule; empty set → false).
    let album_downloaded = !tracks_items.is_empty()
        && tracks_items
            .iter()
            .all(|track| downloaded_ids.contains(&(track.id as i64)));

    let track_len = tracks_items.len() as u32;
    PurchaseAlbum {
        id: album.id.clone(),
        title: album.title.clone(),
        artist: album.artist.clone(),
        image: album.image.clone(),
        release_date_original: album.release_date_original.clone(),
        label: album.label.clone(),
        genre: album.genre.clone(),
        tracks_count: album.tracks_count,
        duration: album.duration,
        hires: album.hires,
        maximum_sampling_rate: album.maximum_sampling_rate,
        maximum_bit_depth: album.maximum_bit_depth,
        downloadable: purchase_meta.map(|item| item.downloadable).unwrap_or(true),
        downloaded: album_downloaded,
        purchased_at: purchase_meta.and_then(|item| item.purchased_at),
        tracks: Some(SearchResultsPage {
            items: tracks_items,
            total: track_len,
            offset: 0,
            limit: track_len,
        }),
    }
}

/// Pick the on-disk file extension for a purchased track from the RESPONSE
/// stream's `format_id` / `mime_type` (§7.1.5, ported byte-for-byte from
/// `v2_purchase_extension` `legacy_compat.rs:2553-2559`):
///   * `"mp3"` if the served `format_id == 5` OR the served `mime_type`
///     contains `"mpeg"`;
///   * `"flac"` otherwise.
///
/// IMPORTANT (Addendum B.2): the extension keys off the RESPONSE's served
/// format, NOT the requested one. If Qobuz downgrades (e.g. you asked for 27 but
/// it serves an MP3), the file gets the served extension while the registry
/// records the REQUESTED `format_id` (see `download_purchase_track`). Do NOT
/// reconcile the two — the Tauri app does not.
pub fn purchase_extension(format_id: u32, mime_type: &str) -> &'static str {
    if format_id == 5 || mime_type.contains("mpeg") {
        "mp3"
    } else {
        "flac"
    }
}

/// Build the deterministic on-disk target path for a purchased track
/// (§7.3, ported byte-for-byte from `v2_purchase_target_path`
/// `legacy_compat.rs:2561-2592`):
///   `{destination}/{artist_dir}/{album_dir}/{file_name}`
/// where
///   * `artist_dir = sanitize_filename(artist_name)`;
///   * `album_dir = sanitize_filename(album_title)` and, if `quality_dir` is
///     non-empty, `format!("{album_clean} {sanitize(quality_dir)}")` (a single
///     space joins them — this is the `"Album [FLAC][24-bit,96kHz]"` folder);
///   * `file_name = "{NN} - {title_clean}.{ext}"` (`NN` = zero-padded `{:02}`)
///     when `track_number > 0`, else `"{title_clean}.{ext}"`.
///
/// All three segments are run through the SHARED `sanitize_filename` (§7.4) so
/// the path matches what the library scan + Add-to-Library expect (the `[`/`]`
/// in the quality label become `-`, brackets collapse). The caller passes the
/// already-`'/'→'-'`-transformed `quality_dir` (§7.5 `qualityDir` derivation);
/// the re-sanitize here is idempotent for `/` and additionally strips brackets.
pub fn target_path(
    destination: &str,
    artist_name: &str,
    album_title: &str,
    quality_dir: &str,
    track_number: u32,
    track_title: &str,
    ext: &str,
) -> PathBuf {
    let artist_dir = sanitize_filename(artist_name);
    let album_clean = sanitize_filename(album_title);
    let title_clean = sanitize_filename(track_title);

    let file_name = if track_number > 0 {
        format!("{:02} - {}.{}", track_number, title_clean, ext)
    } else {
        format!("{}.{}", title_clean, ext)
    };

    // Embed quality in album folder name: "Album [FLAC][24-bit,96kHz]".
    let album_dir = if !quality_dir.is_empty() {
        let quality_clean = sanitize_filename(quality_dir);
        format!("{} {}", album_clean, quality_clean)
    } else {
        album_clean
    };

    PathBuf::from(destination)
        .join(artist_dir)
        .join(album_dir)
        .join(file_name)
}

/// I/O tail of the single-track download: given the already-fetched audio
/// `data` and the resolved track/stream metadata, derive the extension from the
/// RESPONSE format, build the target path, `create_dir_all`, write the `.part`
/// file, `fs::rename` to final, then write the registry row with the REQUESTED
/// `format_id`. Returns the final on-disk path string.
///
/// Split out from `download_purchase_track` purely so the filesystem +
/// registry ordering (Addendum B.1/B.2/B.3) is unit-testable without a live
/// HTTP client. The behavior is the EXACT concatenation of
/// `v2_download_purchase_track_impl`'s write tail (`legacy_compat.rs:2681-2701`)
/// and `v2_purchases_download_track`'s registry write (`:3019`).
///
/// Ordering & failure semantics (Addendum B.1 — replicated verbatim):
///   1. write `target.with_extension("{ext}.part")` → `2. fs::rename` to final
///      → `3. mark_purchase_downloaded`.
///   If the file write/rename SUCCEEDS but the registry write FAILS, this
///   returns `Err` with the file LEFT ON DISK (orphaned, no registry row). Do
///   NOT roll back the file or treat the registry failure as success.
///
/// No collision preflight (Addendum B.3): `.part`→`fs::rename` overwrites any
/// pre-existing final file or stale `.part` silently.
/// Filesystem-only tail: derive the extension from the RESPONSE format, build
/// the target path, `create_dir_all`, write the `.part`, `fs::rename` to final,
/// and return the final on-disk path string. Does **NOT** touch the registry.
///
/// This is the shared write core. The album loop uses it directly (so a registry
/// failure does NOT mark the track `Failed`; the album loop instead does a
/// SEPARATE best-effort registry write that swallows the error — Svelte
/// `markTrackDownloaded(...).catch(()=>{})`, §B.1 album-path semantics). The
/// single-track bundled path (`write_and_register_track`) wraps this and then
/// propagates a registry error so the track shows `Failed` (§B.1 single-track
/// semantics).
///
/// Addendum B.2: extension/MIME from the RESPONSE; Addendum B.3: silent overwrite.
#[allow(clippy::too_many_arguments)]
fn write_track_file(
    data: &[u8],
    artist_name: &str,
    album_title: &str,
    quality_dir: &str,
    track_number: u32,
    track_title: &str,
    response_format_id: u32,
    response_mime_type: &str,
    destination: &str,
) -> Result<String, String> {
    // Addendum B.2: extension derives from the RESPONSE's served format.
    let extension = purchase_extension(response_format_id, response_mime_type);
    let target = target_path(
        destination,
        artist_name,
        album_title,
        quality_dir,
        track_number,
        track_title,
        extension,
    );

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create destination folder: {}", e))?;
    }

    // Addendum B.3: write `.part`, then rename — no overwrite preflight.
    let temp_path = target.with_extension(format!("{}.part", extension));
    std::fs::write(&temp_path, data).map_err(|e| format!("Failed to write temporary file: {}", e))?;
    std::fs::rename(&temp_path, &target).map_err(|e| format!("Failed to finalize file: {}", e))?;

    Ok(target.to_string_lossy().to_string())
}

/// PUBLIC so the download harness (`tests/purchase_download_harness.rs`) can
/// drive the write + registry tail with real bytes without standing up a
/// `QobuzClient`. That harness is the only thing that executes this code before
/// a user does — Purchases cannot be smoke-tested here — so reachability from a
/// test is worth the wider surface.
#[allow(clippy::too_many_arguments)]
pub fn write_and_register_track(
    db: &LibraryDatabase,
    track_id: u64,
    album_id: Option<&str>,
    requested_format_id: u32,
    data: &[u8],
    artist_name: &str,
    album_title: &str,
    quality_dir: &str,
    track_number: u32,
    track_title: &str,
    response_format_id: u32,
    response_mime_type: &str,
    destination: &str,
) -> Result<String, String> {
    let file_path = write_track_file(
        data,
        artist_name,
        album_title,
        quality_dir,
        track_number,
        track_title,
        response_format_id,
        response_mime_type,
        destination,
    )?;

    // Addendum B.1/B.2: registry write AFTER the file is on disk, with the
    // REQUESTED format_id. A registry failure here returns Err while the file
    // stays on disk (orphaned) — the reference does the same and does not roll back.
    //
    // `album_id` is folded into THIS write rather than left to a follow-up call.
    // The reference wrote `None` here and then backfilled the album id from the
    // frontend (`markTrackDownloaded(...).catch(() => {})`, both in the album loop
    // and in the single-track path). Collapsing the two writes reaches the same end
    // state with one statement and no window in which the column is null — which
    // matters now that the column has a reader (the album-downloaded rule below).
    db.mark_purchase_downloaded(
        track_id as i64,
        album_id,
        &file_path,
        requested_format_id as i64,
    )
    .map_err(|e| e.to_string())?;

    Ok(file_path)
}

// ─── Scope expansion §14: tags, covers, goodies ──────────────────────────────
//
// Everything in this block is ADDITIVE over the reference, which wrote no tags,
// no cover and no goodies. Nothing here changes a request or the entitlement
// boundary, and every part degrades to reference behaviour when it fails: the
// audio file is the deliverable, and it is already on disk and registered before
// any of this runs.

/// Album-level facts a track download needs in order to tag itself, gathered
/// once by the caller before the album loop rather than re-derived per track.
///
/// `cover_jpeg` is carried as bytes because it is embedded into EVERY track of
/// the album; fetching it per track would issue one CDN request per file for a
/// value that never changes.
#[derive(Debug, Clone, Default)]
pub struct PurchaseAlbumContext {
    pub album_artist: String,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub label: Option<String>,
    pub cover_jpeg: Option<Vec<u8>>,
}

impl PurchaseAlbumContext {
    /// Build the context from the catalog album the detail screen already holds.
    /// The cover is fetched separately (it is I/O) and attached with
    /// [`Self::with_cover`].
    pub fn from_album(album: &Album) -> Self {
        // The V2 nested `dates.original` wins over the flat
        // `release_date_original` when present — the model says so on the field
        // itself, and on a V2-shaped album the flat field is absent, which would
        // silently drop the DATE tag from every track of that release.
        let date = album
            .dates
            .as_ref()
            .and_then(|d| d.original.as_deref())
            .or(album.release_date_original.as_deref());

        Self {
            album_artist: album.artist.name.clone(),
            year: parse_release_year(date),
            genre: album.genre.as_ref().map(|g| g.name.clone()),
            label: album.label.as_ref().map(|l| l.name.clone()),
            cover_jpeg: None,
        }
    }

    pub fn with_cover(mut self, cover_jpeg: Option<Vec<u8>>) -> Self {
        self.cover_jpeg = cover_jpeg;
        self
    }
}

/// Pull the year out of a Qobuz `release_date_original` (`"2019-05-31"`).
/// Anything that does not start with four digits yields `None` rather than a
/// wrong year — a wrong DATE tag is worse than an absent one.
pub fn parse_release_year(date: Option<&str>) -> Option<u32> {
    let date = date?;
    let head: String = date.chars().take(4).collect();
    if head.len() == 4 && head.chars().all(|c| c.is_ascii_digit()) {
        head.parse().ok()
    } else {
        None
    }
}

/// The largest asset this will pull into memory. Covers are ~100 KB and booklets
/// a few MB; the ceiling exists so a hostile or wrong `Content-Length` cannot
/// decide our allocation.
const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;

/// How long a single asset may take, end to end.
const ASSET_TIMEOUT_SECS: u64 = 60;

/// Fetch an image or extra asset over HTTP, returning `None` on any failure.
///
/// Deliberately infallible-by-return: every caller treats a missing cover or
/// goodie as "skip it", never as a download failure.
///
/// **This does NOT reuse `QobuzClient::download_audio`, and the difference is
/// deliberate.** That function is tuned for one job — a multi-minute hi-res
/// track from the Qobuz CDN — and three of its properties are wrong here:
///   * it sizes its buffer with `Vec::with_capacity(content_length)`, i.e. from
///     a number the remote chose. For a track that is a trusted CDN; for a
///     goodie it is an arbitrary URL out of an album payload whose item shape has
///     never been observed. A bogus length would attempt the allocation
///     immediately, and an allocation failure ABORTS the process — on a machine
///     this project's own build rules describe as hard-freezing under memory
///     pressure;
///   * it sets no total timeout, because a large track legitimately exceeds any
///     fixed budget. A cover that never finishes would hang the album loop with
///     nothing to cancel it;
///   * it logs every fetch as "Downloading audio", which is simply wrong for a
///     PDF and makes the logs lie during exactly the investigation that needs
///     them.
///
/// So assets get their own client: a hard size ceiling, a total timeout, and a
/// capacity hint that is `min(content_length, ceiling)`.
pub async fn fetch_asset_bytes(url: &str) -> Option<Vec<u8>> {
    use std::time::Duration;

    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(ASSET_TIMEOUT_SECS))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            log::warn!("[Purchases] could not build the asset client: {e}");
            return None;
        }
    };

    let response = match client.get(url).header("User-Agent", "Mozilla/5.0").send().await {
        Ok(response) => response,
        Err(e) => {
            log::warn!("[Purchases] asset fetch failed ({url}): {e}");
            return None;
        }
    };

    if !response.status().is_success() {
        log::warn!("[Purchases] asset fetch got HTTP {} ({url})", response.status());
        return None;
    }

    // Refuse oversized assets on the ANNOUNCED length before reading a byte, and
    // then again on the actual bytes, because the announcement is not binding.
    if let Some(len) = response.content_length() {
        if len as usize > MAX_ASSET_BYTES {
            log::warn!("[Purchases] asset is {len} bytes, over the {MAX_ASSET_BYTES} cap ({url})");
            return None;
        }
    }

    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            log::warn!("[Purchases] asset body failed ({url}): {e}");
            return None;
        }
    };

    if bytes.is_empty() {
        log::warn!("[Purchases] asset fetch returned 0 bytes: {url}");
        return None;
    }
    if bytes.len() > MAX_ASSET_BYTES {
        log::warn!(
            "[Purchases] asset body was {} bytes despite its headers, over the cap ({url})",
            bytes.len()
        );
        return None;
    }

    Some(bytes.to_vec())
}

/// Write `cover.jpg` / `back.jpg` beside the album's tracks (§14.2).
///
/// `album/get`'s `image` object carries exactly `small`, `thumbnail`, `large`
/// and `back` — measured 2026-08-15; there is no `mega`, matching the earlier
/// finding that Qobuz album art tops out around 600 px in practice. So `large`
/// is the best available and `back` is a bonus nothing else in QBZ uses.
///
/// Best-effort by contract: a write failure is logged and swallowed.
pub fn write_album_cover_files(
    album_dir: &std::path::Path,
    cover_jpeg: Option<&[u8]>,
    back_jpeg: Option<&[u8]>,
) {
    for (bytes, name) in [(cover_jpeg, "cover.jpg"), (back_jpeg, "back.jpg")] {
        let Some(bytes) = bytes.filter(|b| !b.is_empty()) else {
            continue;
        };
        let path = album_dir.join(name);
        if let Err(e) = std::fs::write(&path, bytes) {
            log::warn!("[Purchases] failed to write {}: {e}", path.display());
        }
    }
}

/// Tag a file that has just been downloaded (§14.1). Never fails the download —
/// it returns nothing and logs on error, so a caller cannot accidentally
/// propagate it.
pub fn tag_downloaded_file(file_path: &str, track: &Track, ctx: &PurchaseAlbumContext) {
    let meta = PurchaseTagWrite {
        title: track.title.clone(),
        version: track.version.clone(),
        // The track's OWN performer, straight from the API. This is the whole
        // reason purchases do not reuse the editor's writer, whose artist rule
        // reads the file — and a just-downloaded file has nothing to read.
        artist: track
            .performer
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| ctx.album_artist.clone()),
        // `"Singles"` matches what the PATH builder uses for a track with no
        // album (`download_purchase_track`). Defaulting to an empty string here
        // instead would file the track in a folder called `Singles` while
        // stamping `ALBUM=` on it — the folder and the tag disagreeing about the
        // same track.
        album_title: track
            .album
            .as_ref()
            .map(|a| a.title.clone())
            .unwrap_or_else(|| "Singles".to_string()),
        album_artist: ctx.album_artist.clone(),
        track_number: Some(track.track_number),
        disc_number: track.media_number,
        year: ctx.year,
        genre: ctx.genre.clone(),
        label: ctx.label.clone(),
        isrc: track.isrc.clone(),
        composer: track.composer.as_ref().map(|c| c.name.clone()),
        copyright: track.copyright.clone(),
    };

    if let Err(e) = write_purchase_tags(file_path, &meta, ctx.cover_jpeg.as_deref()) {
        log::warn!("[Purchases] failed to tag {file_path}: {e}");
    }
}

/// Download one album goodie (booklet PDF, video, …) into the album folder
/// (§14.3).
///
/// The item shape is UNVERIFIED — goodies come back as an empty array on albums
/// nobody owns, and we cannot capture a populated one without owning a purchase.
/// So this reads the URL and name through the defensive accessors and gives up
/// quietly on anything it cannot make sense of. Goodies are counted separately
/// from track progress (a goodie is not a track) and a goodie failure never
/// fails the album.
pub async fn download_goodie(
    goody: &qbz_models::Goody,
    album_dir: &std::path::Path,
) -> Option<String> {
    let url = goody.best_url()?;

    // Create the folder BEFORE spending the download. Doing it afterwards throws
    // away however many megabytes were just fetched if the directory cannot be
    // made.
    if let Err(e) = std::fs::create_dir_all(album_dir) {
        log::warn!("[Purchases] failed to create the goodie folder: {e}");
        return None;
    }

    let path = goodie_target_path(album_dir, &goody.display_name(), url);
    let bytes = fetch_asset_bytes(url).await?;

    match std::fs::write(&path, &bytes) {
        Ok(()) => Some(path.to_string_lossy().to_string()),
        Err(e) => {
            log::warn!("[Purchases] failed to write goodie {}: {e}", path.display());
            None
        }
    }
}

/// Pick a safe, non-colliding on-disk name for a goodie. Pure, so the naming
/// rules are testable without a download.
///
/// Three hazards, all handled here:
///   * **the extension must be read from the PATH, not the whole URL.** Query and
///     fragment are stripped FIRST. Doing it the other way round — splitting on
///     the last `.` and then removing the query — turns a signed
///     `…/booklet.pdf?sig=a.b3f` into a file called `Booklet.b3f`;
///   * **a leading dot would hide the file.** `sanitize_filename` keeps ASCII
///     `.`, so a goodie named `.notes` would land as a dotfile the user never
///     sees. (It also maps `/` and `\` to `-`, which is what makes escaping the
///     album folder impossible — that part needs no help here.)
///   * **two goodies can sanitize to the same name.** Rather than let the second
///     silently overwrite the first, later ones get a ` (2)`, ` (3)` suffix.
fn goodie_target_path(album_dir: &std::path::Path, display_name: &str, url: &str) -> PathBuf {
    let path_part = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path_part
        .rsplit('/')
        .next()
        .and_then(|segment| segment.rsplit_once('.'))
        .map(|(_, ext)| ext)
        // Booklets are the only goodie kind ever observed, and a wrong extension
        // is recoverable while a missing file is not.
        .filter(|ext| !ext.is_empty() && ext.len() <= 5 && ext.chars().all(char::is_alphanumeric))
        .unwrap_or("pdf");

    let mut stem = sanitize_filename(display_name);
    if stem.starts_with('.') {
        stem.insert(0, '_');
    }

    let mut candidate = album_dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while candidate.exists() && n < 100 {
        candidate = album_dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    candidate
}

/// The single canonical single-track download primitive (Slice 5).
///
/// Ported from `v2_download_purchase_track_impl` (`legacy_compat.rs:2651-2702`)
/// combined with the registry write of `v2_purchases_download_track`
/// (`:3013-3022`). Sequence:
///   1. `client.get_track(track_id)` → metadata. Error → `"Failed to fetch track
///      {id}: {e}"`.
///   2. `client.get_track_file_url_by_format(track_id, format_id)` → SIGNED
///      `StreamUrl` (intent=stream). Error → `"Failed to get download URL for
///      track {id}: {e}"`. (In the reference the client lock is dropped here; in
///      this crate the caller holds the `QobuzClient` by `&`, so there is no lock
///      to drop — the read guard is released by the controller before the
///      multi-minute CDN fetch. No behavioral divergence in the bytes path.)
///   3. `QobuzClient::download_audio(&stream.url)` → `Vec<u8>` (HTTP/1.1-only, no
///      total timeout — see `qbz-qobuz`).
///   4. Resolve names: artist = `track.performer.name` else `"Unknown Artist"`;
///      album = `track.album.title` else `"Singles"`.
///   5. Extension from RESPONSE `stream.format_id`/`mime_type` (B.2); path via
///      `target_path`; `.part`→rename; registry write with REQUESTED `format_id`.
///
/// `quality_dir` is the UI-selected format label with `'/'→'-'` already applied
/// (§7.5); it becomes the album-folder quality suffix AND the registry's quality
/// dimension is the REQUESTED format (B.2). Returns the final on-disk path (the
/// controller uses the FIRST track's returned path to rewrite the album-download
/// destination to the album folder — Slice 7).
///
/// Addendum B.5: only `stream.url` / `stream.format_id` / `stream.mime_type` are
/// consumed; `stream.restrictions` is IGNORED (no restriction-based blocking).
pub async fn download_purchase_track(
    client: &QobuzClient,
    db: &LibraryDatabase,
    track_id: u64,
    album_id: Option<&str>,
    format_id: u32,
    destination: &str,
    quality_dir: &str,
    ctx: Option<&PurchaseAlbumContext>,
) -> Result<String, String> {
    // UNSIGNED on the purchase path (contract §11-6): the vendor's own desktop
    // client sends `/track/get` without a signature, and so did the reference.
    // The entitlement proof is the signature on `getFileUrl` below, which is
    // untouched.
    let track = client
        .get_track_for_purchase(track_id)
        .await
        .map_err(|e| format!("Failed to fetch track {}: {}", track_id, e))?;
    let stream = client
        .get_track_file_url_by_format(track_id, format_id)
        .await
        .map_err(|e| format!("Failed to get download URL for track {}: {}", track_id, e))?;

    let data = QobuzClient::download_audio(&stream.url).await?;

    let artist_name = track
        .performer
        .as_ref()
        .map(|artist| artist.name.clone())
        .unwrap_or_else(|| "Unknown Artist".to_string());
    let album_title = track
        .album
        .as_ref()
        .map(|album| album.title.clone())
        .unwrap_or_else(|| "Singles".to_string());

    // Fall back to the album the TRACK itself reports. `album_id` is what makes
    // an album's downloaded state answerable at all (§11-1), and the reference
    // only ever populated it from the album loop — so a user who downloaded a
    // release one track at a time produced rows the rule can never attribute,
    // and their album stayed un-downloaded forever with no way to notice. The
    // `/track/get` payload fetched two lines above already carries the id, so
    // there is no reason for any path to write NULL.
    let resolved_album_id = album_id.or_else(|| track.album.as_ref().map(|a| a.id.as_str()));

    let file_path = write_and_register_track(
        db,
        track_id,
        resolved_album_id,
        format_id,
        &data,
        &artist_name,
        &album_title,
        quality_dir,
        track.track_number,
        &track.title,
        stream.format_id,
        &stream.mime_type,
        destination,
    )?;

    // §14.1, additive: tag AFTER the file is on disk and registered, so the
    // deliverable is already durable and a tagging failure can only cost tags.
    // `None` reproduces reference behaviour exactly (no tags written at all),
    // which is what the legacy Slint call site passes.
    if let Some(ctx) = ctx {
        tag_downloaded_file(&file_path, &track, ctx);
    }

    Ok(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qbz_models::{Artist, AlbumSummary, PurchaseAlbum, PurchaseTrack, SearchResultsPage};

    fn album(title: &str, artist: &str) -> PurchaseAlbum {
        PurchaseAlbum {
            title: title.to_string(),
            artist: Artist {
                name: artist.to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn track(title: &str, performer: &str, album_title: Option<&str>) -> PurchaseTrack {
        PurchaseTrack {
            title: title.to_string(),
            performer: Artist {
                name: performer.to_string(),
                ..Default::default()
            },
            album: album_title.map(|t| AlbumSummary {
                id: String::new(),
                title: t.to_string(),
                image: Default::default(),
                label: None,
                genre: None,
            }),
            ..Default::default()
        }
    }

    fn response(albums: Vec<PurchaseAlbum>, tracks: Vec<PurchaseTrack>) -> PurchaseResponse {
        PurchaseResponse {
            albums: SearchResultsPage {
                total: albums.len() as u32,
                offset: 7,
                limit: 500,
                items: albums,
            },
            tracks: SearchResultsPage {
                total: tracks.len() as u32,
                offset: 9,
                limit: 500,
                items: tracks,
            },
        }
    }

    #[test]
    fn filter_matches_album_title_case_insensitive() {
        let resp = response(
            vec![album("Kind of Blue", "Miles Davis"), album("Thriller", "Michael Jackson")],
            vec![],
        );
        let out = filter_purchase_response(resp, "BLUE");
        assert_eq!(out.albums.items.len(), 1);
        assert_eq!(out.albums.items[0].title, "Kind of Blue");
        // total reset to filtered length, offset reset to 0, limit preserved.
        assert_eq!(out.albums.total, 1);
        assert_eq!(out.albums.offset, 0);
        assert_eq!(out.albums.limit, 500);
    }

    #[test]
    fn filter_matches_album_artist_name() {
        let resp = response(
            vec![album("Thriller", "Michael Jackson"), album("Blue", "Davis")],
            vec![],
        );
        let out = filter_purchase_response(resp, "jackson");
        assert_eq!(out.albums.items.len(), 1);
        assert_eq!(out.albums.items[0].title, "Thriller");
    }

    #[test]
    fn filter_matches_track_title_performer_and_album() {
        let resp = response(
            vec![],
            vec![
                track("So What", "Miles Davis", Some("Kind of Blue")),
                track("Beat It", "Michael Jackson", Some("Thriller")),
                track("Random", "Nobody", None),
            ],
        );
        // performer match
        let by_performer = filter_purchase_response(resp.clone(), "jackson");
        assert_eq!(by_performer.tracks.items.len(), 1);
        assert_eq!(by_performer.tracks.items[0].title, "Beat It");

        // album-title match
        let by_album = filter_purchase_response(resp.clone(), "kind of blue");
        assert_eq!(by_album.tracks.items.len(), 1);
        assert_eq!(by_album.tracks.items[0].title, "So What");

        // title match
        let by_title = filter_purchase_response(resp, "random");
        assert_eq!(by_title.tracks.items.len(), 1);
        assert_eq!(by_title.tracks.items[0].title, "Random");
    }

    #[test]
    fn filter_track_with_no_album_does_not_panic() {
        let resp = response(vec![], vec![track("Solo", "Artist", None)]);
        let out = filter_purchase_response(resp, "solo");
        assert_eq!(out.tracks.items.len(), 1);
        // total/offset reset, limit preserved on the tracks page too.
        assert_eq!(out.tracks.total, 1);
        assert_eq!(out.tracks.offset, 0);
        assert_eq!(out.tracks.limit, 500);
    }

    #[test]
    fn filter_no_match_yields_empty_pages() {
        let resp = response(
            vec![album("Thriller", "Michael Jackson")],
            vec![track("Beat It", "Michael Jackson", Some("Thriller"))],
        );
        let out = filter_purchase_response(resp, "zzz-no-match");
        assert!(out.albums.items.is_empty());
        assert!(out.tracks.items.is_empty());
        assert_eq!(out.albums.total, 0);
        assert_eq!(out.tracks.total, 0);
    }

    // ── Slice 4: synth_formats ───────────────────────────────────────────

    // `Album` has no `Default`; build the minimal shape from JSON (relying on
    // the serde defaults / Option fields) so we never reach into qbz-models.
    fn album_with(hires: bool, max_sr: Option<f64>) -> Album {
        let json = match max_sr {
            Some(sr) => format!(r#"{{"hires":{hires},"maximum_sampling_rate":{sr}}}"#),
            None => format!(r#"{{"hires":{hires}}}"#),
        };
        serde_json::from_str(&json).expect("minimal Album JSON deserializes")
    }

    #[test]
    fn synth_formats_24_192_yields_four_options_in_order() {
        // hires + max_sr > 96 → all four, highest first, index 0 = the 192k default.
        let fmts = synth_formats(&album_with(true, Some(192.0)));
        assert_eq!(fmts.len(), 4);
        let ids: Vec<u32> = fmts.iter().map(|f| f.id).collect();
        assert_eq!(ids, vec![27, 7, 6, 5]);
        // Exact labels are load-bearing (feed qualityDir + the dropdown).
        assert_eq!(fmts[0].label, "[FLAC][24-bit,192kHz]");
        assert_eq!(fmts[1].label, "[FLAC][24-bit,96kHz]");
        assert_eq!(fmts[2].label, "[FLAC][16-bit,44.1kHz]");
        assert_eq!(fmts[3].label, "[MP3][320kbps]");
        // bit_depth / sampling_rate carried verbatim.
        assert_eq!((fmts[0].bit_depth, fmts[0].sampling_rate), (Some(24), Some(192.0)));
        assert_eq!((fmts[1].bit_depth, fmts[1].sampling_rate), (Some(24), Some(96.0)));
        assert_eq!((fmts[2].bit_depth, fmts[2].sampling_rate), (Some(16), Some(44.1)));
        assert_eq!((fmts[3].bit_depth, fmts[3].sampling_rate), (None, None));
        // default-select is index 0 (highest available).
        assert_eq!(fmts[0].id, 27);
    }

    #[test]
    fn synth_formats_24_96_drops_192_option() {
        // hires but max_sr exactly 96 (not > 96) → no id 27.
        let fmts = synth_formats(&album_with(true, Some(96.0)));
        let ids: Vec<u32> = fmts.iter().map(|f| f.id).collect();
        assert_eq!(ids, vec![7, 6, 5]);
        assert_eq!(fmts[0].id, 7);
    }

    #[test]
    fn synth_formats_hires_with_no_sampling_rate_drops_192() {
        // max_sr None → unwrap_or(0.0) → not > 96 → no id 27, but hires keeps id 7.
        let fmts = synth_formats(&album_with(true, None));
        let ids: Vec<u32> = fmts.iter().map(|f| f.id).collect();
        assert_eq!(ids, vec![7, 6, 5]);
    }

    #[test]
    fn synth_formats_non_hires_yields_only_cd_and_mp3() {
        // Not hires → only the always-present 6 + 5; max_sr is irrelevant.
        let fmts = synth_formats(&album_with(false, Some(192.0)));
        let ids: Vec<u32> = fmts.iter().map(|f| f.id).collect();
        assert_eq!(ids, vec![6, 5]);
        assert_eq!(fmts[0].id, 6);
    }

    // ── Slice 4: apply_download_flags ────────────────────────────────────

    fn album_id(id: &str) -> PurchaseAlbum {
        PurchaseAlbum {
            id: id.to_string(),
            ..Default::default()
        }
    }

    /// An album that declares how many tracks it has — the denominator the
    /// registry-backed downloaded rule needs (§11-1).
    fn album_with_tracks(id: &str, tracks_count: u32) -> PurchaseAlbum {
        PurchaseAlbum {
            id: id.to_string(),
            tracks_count: Some(tracks_count),
            ..Default::default()
        }
    }

    /// Registry counts: purchase album id → DISTINCT downloaded track count.
    fn reg(pairs: &[(&str, u32)]) -> HashMap<String, u32> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn track_for_album(id: u64, album_id: &str) -> PurchaseTrack {
        PurchaseTrack {
            id,
            album: Some(AlbumSummary {
                id: album_id.to_string(),
                title: String::new(),
                image: Default::default(),
                label: None,
                genre: None,
            }),
            ..Default::default()
        }
    }

    fn dl_ids(ids: &[i64]) -> HashSet<i64> {
        ids.iter().copied().collect()
    }

    #[test]
    fn apply_flags_marks_track_downloaded_and_records_format_ids() {
        let mut resp = response(vec![], vec![track_for_album(10, "a1"), track_for_album(20, "a1")]);
        let downloaded = dl_ids(&[10]);
        let mut format_map: HashMap<i64, Vec<u32>> = HashMap::new();
        format_map.insert(10, vec![27, 6]);

        apply_download_flags(&mut resp, &downloaded, &format_map, &HashMap::new());

        assert!(resp.tracks.items[0].downloaded);
        assert_eq!(resp.tracks.items[0].downloaded_format_ids, vec![27, 6]);
        // track not in dlIds → not downloaded, empty format ids.
        assert!(!resp.tracks.items[1].downloaded);
        assert!(resp.tracks.items[1].downloaded_format_ids.is_empty());
    }

    // The three album tests below were rewritten on 2026-08-16. They used to
    // assert the reference's nested-tracks predicate (every track whose
    // `album.id` matches must be in `downloaded_ids`). That predicate was
    // replaced under contract §11-1 because the screen that uses it never
    // receives a tracks page, so it could only ever answer `false`. The INTENT
    // of each test is preserved — complete, partial, unknown — but the source of
    // truth is now the local registry.

    #[test]
    fn apply_flags_album_downloaded_when_the_registry_covers_it() {
        let mut resp = response(
            vec![album_with_tracks("a1", 2)],
            vec![track_for_album(10, "a1"), track_for_album(20, "a1")],
        );
        apply_download_flags(
            &mut resp,
            &dl_ids(&[10, 20]),
            &HashMap::new(),
            &reg(&[("a1", 2)]),
        );
        assert!(resp.albums.items[0].downloaded);
    }

    #[test]
    fn apply_flags_album_not_downloaded_when_partially_owned() {
        let mut resp = response(
            vec![album_with_tracks("a1", 2)],
            vec![track_for_album(10, "a1"), track_for_album(20, "a1")],
        );
        apply_download_flags(
            &mut resp,
            &dl_ids(&[10]),
            &HashMap::new(),
            &reg(&[("a1", 1)]),
        );
        assert!(!resp.albums.items[0].downloaded);
    }

    #[test]
    fn apply_flags_album_absent_from_the_registry_is_not_downloaded() {
        // Nothing of this album has been downloaded → false, never panic.
        let mut resp = response(
            vec![album_with_tracks("a1", 2)],
            vec![track_for_album(10, "other")],
        );
        apply_download_flags(
            &mut resp,
            &dl_ids(&[10]),
            &HashMap::new(),
            &reg(&[("other", 1)]),
        );
        assert!(!resp.albums.items[0].downloaded);
    }

    #[test]
    fn apply_flags_frontend_overrides_stale_backend_downloaded() {
        // Backend wrongly set downloaded=true; the annotation recomputes to false.
        let mut track = track_for_album(10, "a1");
        track.downloaded = true;
        track.downloaded_format_ids = vec![99];
        let mut backend_true_album = album_with_tracks("a1", 1);
        backend_true_album.downloaded = true;

        let mut resp = response(vec![backend_true_album], vec![track]);
        // Nothing downloaded → both must be overridden to false / cleared.
        apply_download_flags(&mut resp, &dl_ids(&[]), &HashMap::new(), &HashMap::new());
        assert!(!resp.tracks.items[0].downloaded);
        assert!(resp.tracks.items[0].downloaded_format_ids.is_empty());
        assert!(!resp.albums.items[0].downloaded);
    }

    // ── Slice 9: build_purchase_album (detail mapping) ───────────────────

    // A catalog `Album` with N nested tracks + the meta fields the mapping
    // reads. JSON-built (Album has no Default), tracks carry id/title/number so
    // the per-track mapping + registry annotation can be asserted.
    fn catalog_album(id: &str, downloadable: bool, track_ids: &[u64]) -> Album {
        let items: Vec<String> = track_ids
            .iter()
            .enumerate()
            .map(|(i, tid)| {
                format!(
                    r#"{{"id":{tid},"title":"T{tid}","track_number":{n},"streamable":true}}"#,
                    n = i + 1
                )
            })
            .collect();
        let json = format!(
            r#"{{"id":"{id}","title":"Detail","artist":{{"id":3,"name":"Some Artist"}},
                 "downloadable":{downloadable},
                 "tracks":{{"items":[{tracks}],"total":{total}}}}}"#,
            tracks = items.join(","),
            total = track_ids.len()
        );
        serde_json::from_str(&json).expect("catalog Album JSON deserializes")
    }

    #[test]
    fn build_detail_maps_tracks_and_synthesizes_page_counts() {
        let album = catalog_album("alb1", true, &[10, 20, 30]);
        // purchases listing carries the album meta (downloadable + purchased_at).
        let mut meta = album_id("alb1");
        meta.downloadable = true;
        meta.purchased_at = Some(1_700_000_000);
        let purchases = response(vec![meta], vec![]);

        let detail =
            build_purchase_album(&album, &purchases, &dl_ids(&[]), &HashMap::new());

        assert_eq!(detail.id, "alb1");
        assert!(detail.downloadable);
        assert_eq!(detail.purchased_at, Some(1_700_000_000));
        let tracks = detail.tracks.expect("nested tracks present");
        assert_eq!(tracks.items.len(), 3);
        // synthesized page: offset 0, limit = total = len.
        assert_eq!(tracks.offset, 0);
        assert_eq!(tracks.total, 3);
        assert_eq!(tracks.limit, 3);
        // per-track purchased_at copies the album-level meta.
        assert!(tracks.items.iter().all(|t| t.purchased_at == Some(1_700_000_000)));
    }

    #[test]
    fn build_detail_downloadable_defaults_true_when_album_not_in_listing() {
        // No matching purchase meta → downloadable defaults TRUE, purchased_at None.
        let album = catalog_album("missing", false, &[1]);
        let purchases = response(vec![album_id("other")], vec![]);
        let detail =
            build_purchase_album(&album, &purchases, &dl_ids(&[]), &HashMap::new());
        // The catalog `downloadable:false` is IGNORED — the value comes from the
        // purchase meta (absent → unwrap_or(true)).
        assert!(detail.downloadable);
        assert_eq!(detail.purchased_at, None);
    }

    #[test]
    fn build_detail_annotates_per_track_and_album_downloaded() {
        let album = catalog_album("alb1", true, &[10, 20]);
        let purchases = response(vec![album_id("alb1")], vec![]);
        let mut format_map: HashMap<i64, Vec<u32>> = HashMap::new();
        format_map.insert(10, vec![7]);

        // Both nested track ids owned → album downloaded; per-track flags set.
        let detail =
            build_purchase_album(&album, &purchases, &dl_ids(&[10, 20]), &format_map);
        assert!(detail.downloaded);
        let tracks = detail.tracks.unwrap();
        assert!(tracks.items[0].downloaded);
        assert_eq!(tracks.items[0].downloaded_format_ids, vec![7]);
        assert!(tracks.items[1].downloaded);
        assert!(tracks.items[1].downloaded_format_ids.is_empty());
    }

    #[test]
    fn build_detail_album_not_downloaded_when_partially_owned() {
        let album = catalog_album("alb1", true, &[10, 20]);
        let purchases = response(vec![album_id("alb1")], vec![]);
        // Only one of two nested tracks owned → album NOT downloaded (all-rule).
        let detail =
            build_purchase_album(&album, &purchases, &dl_ids(&[10]), &HashMap::new());
        assert!(!detail.downloaded);
        let tracks = detail.tracks.unwrap();
        assert!(tracks.items[0].downloaded);
        assert!(!tracks.items[1].downloaded);
    }

    #[test]
    fn build_detail_empty_track_album_is_not_downloaded() {
        // No nested tracks → empty-set rule → album downloaded = false.
        let album = catalog_album("alb1", true, &[]);
        let purchases = response(vec![album_id("alb1")], vec![]);
        let detail =
            build_purchase_album(&album, &purchases, &dl_ids(&[]), &HashMap::new());
        assert!(!detail.downloaded);
        assert_eq!(detail.tracks.unwrap().items.len(), 0);
    }

    // ── Slice 5: purchase_extension ──────────────────────────────────────

    #[test]
    fn purchase_extension_mp3_when_format_id_5() {
        // RESPONSE format_id 5 → mp3 regardless of mime.
        assert_eq!(purchase_extension(5, "audio/flac"), "mp3");
    }

    #[test]
    fn purchase_extension_mp3_when_mime_contains_mpeg() {
        // mime contains "mpeg" → mp3 even if the served id is a FLAC id.
        assert_eq!(purchase_extension(27, "audio/mpeg"), "mp3");
    }

    #[test]
    fn purchase_extension_flac_otherwise() {
        assert_eq!(purchase_extension(27, "audio/flac"), "flac");
        assert_eq!(purchase_extension(7, ""), "flac");
        assert_eq!(purchase_extension(6, "application/octet-stream"), "flac");
    }

    // ── Slice 5: target_path ─────────────────────────────────────────────

    #[test]
    fn target_path_full_template_with_quality_and_track_number() {
        // {dest}/{artist}/{album [quality]}/{NN - title.ext}; quality joined by a
        // single space; sanitize strips the `[`/`]` brackets to `-` and collapses.
        let p = target_path(
            "/music",
            "Miles Davis",
            "Kind of Blue",
            "[FLAC][24-bit,96kHz]",
            3,
            "So What",
            "flac",
        );
        // sanitize: "[FLAC][24-bit,96kHz]" → brackets→`-`, collapsed/trimmed.
        let expected = PathBuf::from("/music")
            .join("Miles Davis")
            .join(format!("Kind of Blue {}", sanitize_filename("[FLAC][24-bit,96kHz]")))
            .join("03 - So What.flac");
        assert_eq!(p, expected);
        // zero-padding is two digits.
        assert!(p.to_string_lossy().contains("/03 - So What.flac"));
    }

    #[test]
    fn target_path_no_quality_dir_uses_bare_album_folder() {
        let p = target_path("/d", "Artist", "Album", "", 1, "Title", "flac");
        assert_eq!(p, PathBuf::from("/d").join("Artist").join("Album").join("01 - Title.flac"));
    }

    #[test]
    fn target_path_zero_track_number_drops_number_prefix() {
        let p = target_path("/d", "Artist", "Album", "", 0, "Title", "mp3");
        assert_eq!(p, PathBuf::from("/d").join("Artist").join("Album").join("Title.mp3"));
    }

    #[test]
    fn target_path_unknown_artist_and_singles_fallbacks_sanitize() {
        // The "Unknown Artist"/"Singles" fallbacks are applied by the caller;
        // here verify they round-trip through sanitize unchanged (ASCII alnum +
        // spaces survive).
        let p = target_path("/d", "Unknown Artist", "Singles", "", 0, "Loose Track", "flac");
        assert_eq!(
            p,
            PathBuf::from("/d").join("Unknown Artist").join("Singles").join("Loose Track.flac")
        );
    }

    // ── Slice 5: write_and_register_track (filesystem + registry I/O) ─────

    fn open_temp_db(dir: &std::path::Path) -> LibraryDatabase {
        LibraryDatabase::open(&dir.join("library.db")).expect("open temp library db")
    }

    #[test]
    fn write_and_register_writes_part_then_renames_and_records_requested_format() {
        let tmp = tempfile::tempdir().unwrap();
        let db = open_temp_db(tmp.path());
        let dest = tmp.path().join("downloads");
        let data = b"FLACfakebytes".to_vec();

        // Requested format 27 (192k FLAC); RESPONSE downgraded to id 6 FLAC.
        let path = write_and_register_track(
            &db,
            /*track_id*/ 4242,
            /*album_id*/ Some("alb-4242"),
            /*requested_format_id*/ 27,
            &data,
            "Miles Davis",
            "Kind of Blue",
            /*quality_dir*/ "[FLAC][24-bit,192kHz]",
            /*track_number*/ 5,
            "So What",
            /*response_format_id*/ 6,
            /*response_mime_type*/ "audio/flac",
            dest.to_str().unwrap(),
        )
        .expect("write+register succeeds");

        // Final file exists with the RESPONSE-derived extension (flac), the `.part`
        // is gone, and the path embeds the REQUESTED-quality folder.
        let final_path = PathBuf::from(&path);
        assert!(final_path.exists(), "final file must exist");
        assert!(final_path.to_string_lossy().ends_with("05 - So What.flac"));
        assert!(!final_path.with_extension("flac.part").exists(), "`.part` removed after rename");
        assert_eq!(std::fs::read(&final_path).unwrap(), data);

        // Registry recorded the REQUESTED format (27), NOT the served 6 (B.2).
        let formats = db.get_downloaded_purchase_formats().unwrap();
        assert!(formats.contains(&(4242, 27)), "registry keys off REQUESTED format: {formats:?}");
        assert!(!formats.iter().any(|&(tid, fid)| tid == 4242 && fid == 6));
    }

    #[test]
    fn write_and_register_response_mp3_uses_mp3_extension() {
        // B.2: extension follows the RESPONSE (served id 5 → mp3) even though the
        // requested format was a FLAC id; registry still records the requested id.
        let tmp = tempfile::tempdir().unwrap();
        let db = open_temp_db(tmp.path());
        let dest = tmp.path().join("dl");

        let path = write_and_register_track(
            &db,
            1,
            /*album_id*/ None,
            /*requested*/ 7,
            b"x",
            "A",
            "B",
            "",
            1,
            "T",
            /*response*/ 5,
            "audio/mpeg",
            dest.to_str().unwrap(),
        )
        .unwrap();
        assert!(path.ends_with("01 - T.mp3"), "served mp3 → .mp3 extension: {path}");
        let formats = db.get_downloaded_purchase_formats().unwrap();
        assert!(formats.contains(&(1, 7)), "requested format 7 recorded: {formats:?}");
    }

    #[test]
    fn write_and_register_silently_overwrites_existing_final_file() {
        // B.3: no collision preflight — a pre-existing final file is replaced
        // without prompt or `(1)` disambiguation.
        let tmp = tempfile::tempdir().unwrap();
        let db = open_temp_db(tmp.path());
        let dest = tmp.path().join("dl");

        let first = write_and_register_track(
            &db, 9, None, 6, b"old", "A", "Alb", "", 2, "Song", 6, "audio/flac",
            dest.to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(std::fs::read(&first).unwrap(), b"old");

        // Second write to the SAME deterministic path with new bytes overwrites.
        let second = write_and_register_track(
            &db, 9, None, 6, b"new", "A", "Alb", "", 2, "Song", 6, "audio/flac",
            dest.to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(first, second, "same deterministic target path");
        assert_eq!(std::fs::read(&second).unwrap(), b"new", "silent overwrite");
    }

    #[test]
    fn write_and_register_registry_failure_leaves_file_orphaned() {
        // B.1: write file → rename → registry; if the registry write FAILS after a
        // successful file write, return Err with the file LEFT ON DISK (orphaned).
        //
        // Inject a real registry failure WITHOUT touching `qbz-library`'s API: open
        // a normal DB, then DROP the `downloaded_purchases` table via a SECOND
        // connection to the same file. The held `LibraryDatabase` connection then
        // sees "no such table" on its INSERT → registry write fails, while the
        // filesystem write (to a separate destination dir) still succeeds.
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("library.db");
        let db = LibraryDatabase::open(&db_path).unwrap();

        // Drop the registry table out from under the open connection.
        {
            let saboteur = rusqlite::Connection::open(&db_path).unwrap();
            saboteur
                .execute_batch("PRAGMA journal_mode=WAL; DROP TABLE downloaded_purchases;")
                .unwrap();
            // Checkpoint so the held connection observes the drop.
            let _ = saboteur.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }

        let dest = tmp.path().join("downloads");
        let res = write_and_register_track(
            &db, 77, None, 6, b"bytes", "A", "Alb", "", 1, "Song", 6, "audio/flac",
            dest.to_str().unwrap(),
        );

        // Registry INSERT failed (no such table) → Err.
        assert!(res.is_err(), "registry write failure must surface as Err: {res:?}");
        // ...but the file write happened BEFORE the registry write → orphaned file.
        let orphan = dest.join("A").join("Alb").join("01 - Song.flac");
        assert!(orphan.exists(), "file left on disk after registry failure (orphaned, B.1)");
        assert!(!orphan.with_extension("flac.part").exists(), "`.part` already renamed away");
    }
}

// ─── §12-3: the path builder and sanitizer, against the inputs that break them ──
#[cfg(test)]
mod path_contract_tests {
    use super::*;
    use crate::metadata::sanitize_filename;

    /// §4.5. `char::is_alphanumeric` is Unicode-aware, so accented Latin, Greek,
    /// Cyrillic and CJK all SURVIVE sanitization — only non-ASCII
    /// NON-alphanumerics (em dash, curly quotes, ™, ♭) become `-`.
    ///
    /// This matters far beyond aesthetics: the library-to-registry join is exact
    /// `file_path` string equality, so an implementer who "knows" that non-ASCII
    /// becomes dashes writes different paths for most non-English releases and
    /// the gold purchase badge silently stops working for them.
    #[test]
    fn sanitize_keeps_unicode_alphanumerics() {
        assert_eq!(sanitize_filename("Björk"), "Björk");
        assert_eq!(sanitize_filename("Café Tacvba"), "Café Tacvba");
        assert_eq!(sanitize_filename("Ελλάδα"), "Ελλάδα");
        assert_eq!(sanitize_filename("Кино"), "Кино");
        assert_eq!(sanitize_filename("日本語"), "日本語");
    }

    /// The other half of the same rule: non-ASCII punctuation does NOT survive.
    #[test]
    fn sanitize_replaces_non_ascii_punctuation() {
        // Em dash and curly apostrophe are non-ASCII and non-alphanumeric.
        assert_eq!(sanitize_filename("A—B"), "A-B");
        assert_eq!(sanitize_filename("Don’t"), "Don-t");
        // Consecutive dashes collapse, ends are trimmed.
        assert_eq!(sanitize_filename("™™Hits™™"), "Hits");
    }

    /// Filesystem-invalid ASCII still becomes `-`; ASCII brackets do NOT.
    ///
    /// The bracket half is load-bearing and was mis-documented: the quality
    /// folder suffix is literally `[FLAC][24-bit,96kHz]`, and `[` is ASCII, so
    /// it is kept. "Fixing" the brackets would relocate every downloaded album.
    #[test]
    fn sanitize_keeps_ascii_brackets_but_replaces_path_separators() {
        assert_eq!(sanitize_filename("[FLAC][24-bit,96kHz]"), "[FLAC][24-bit,96kHz]");
        assert_eq!(sanitize_filename("AC/DC"), "AC-DC");
        assert_eq!(sanitize_filename("a:b*c?d\"e<f>g|h"), "a-b-c-d-e-f-g-h");
    }

    /// §4.5 / §10-E: the 200 cap is in BYTES and `String::truncate` panics on a
    /// non-char-boundary index — reachable exactly BECAUSE multibyte characters
    /// survive the mapping above.
    ///
    /// Which characters actually crash is worth stating precisely, because the
    /// obvious guess is wrong. 200 is even, so a run of TWO-byte characters
    /// (`é`, `Ω`, Cyrillic) lands on a boundary and truncates fine. The crash
    /// belongs to the THREE-byte class — CJK and most of the BMP beyond Latin
    /// and Cyrillic — where 200 % 3 == 2 always lands mid-character. Measured,
    /// not reasoned: `'本'` and `'日'` panic under the old code, the rest do not.
    /// The two-byte cases are kept here anyway as regression cover for the
    /// boundary walk itself.
    #[test]
    fn sanitize_does_not_panic_truncating_multibyte_titles() {
        // '本' and '日' are the ones that panicked before the fix.
        for ch in ['é', 'Ω', '本', 'ы', '日'] {
            let long: String = std::iter::repeat(ch).take(400).collect();
            let out = sanitize_filename(&long);
            assert!(out.len() <= 200, "cap is bytes: {} bytes", out.len());
            assert!(!out.is_empty());
            // Still valid UTF-8 with no replacement damage: every char is the input char.
            assert!(out.chars().all(|c| c == ch));
        }
    }

    /// A byte cap must not silently become a char cap: pure ASCII is unchanged,
    /// so already-downloaded libraries keep their paths.
    #[test]
    fn sanitize_ascii_cap_is_exactly_200() {
        let long = "a".repeat(400);
        assert_eq!(sanitize_filename(&long).len(), 200);
    }

    #[test]
    fn sanitize_empty_result_falls_back() {
        assert_eq!(sanitize_filename("—"), "track");
        assert_eq!(sanitize_filename(""), "track");
    }

    /// §4.4: an EMPTY quality dir drops the album folder's trailing space too.
    /// Copying the formula literally yields `"Album "` and a different directory.
    #[test]
    fn target_path_empty_quality_dir_has_no_trailing_space() {
        let with_quality = target_path("/d", "A", "Album", "[FLAC][16-bit,44.1kHz]", 1, "T", "flac");
        let without = target_path("/d", "A", "Album", "", 1, "T", "flac");

        assert_eq!(
            without,
            std::path::PathBuf::from("/d/A/Album/01 - T.flac"),
            "no trailing space when the quality dir is empty"
        );
        assert_eq!(
            with_quality,
            std::path::PathBuf::from("/d/A/Album [FLAC][16-bit,44.1kHz]/01 - T.flac")
        );
    }

    /// §4.4, recorded as a KNOWN reference behaviour rather than a bug to fix:
    /// `media_number` never reaches the path, so two discs of one release collide
    /// on track number and `fs::rename` overwrites silently. Disc 2 track 1
    /// lands on top of disc 1 track 1.
    ///
    /// This test exists to make the collision explicit and to fail loudly if
    /// someone changes the scheme without deciding to — the scheme is what the
    /// reference shipped and what existing users' folders look like.
    #[test]
    fn target_path_multi_disc_collides_on_track_number() {
        let disc1 = target_path("/d", "A", "Album", "", 1, "Opening", "flac");
        let disc2 = target_path("/d", "A", "Album", "", 1, "Opening", "flac");
        assert_eq!(
            disc1, disc2,
            "documented reference behaviour: no media_number in the path"
        );

        // Different titles still collide on the NUMBER prefix only, not the name,
        // so the practical collision needs the same title as well.
        let other_title = target_path("/d", "A", "Album", "", 1, "Reprise", "flac");
        assert_ne!(disc1, other_title);
    }

    /// Non-ASCII flows through the whole builder, not just the sanitizer.
    #[test]
    fn target_path_preserves_unicode_end_to_end() {
        let p = target_path(
            "/music",
            "Café Tacvba",
            "Ré",
            "[FLAC][24-bit,96kHz]",
            7,
            "El Ciclón",
            "flac",
        );
        assert_eq!(
            p,
            std::path::PathBuf::from("/music/Café Tacvba/Ré [FLAC][24-bit,96kHz]/07 - El Ciclón.flac")
        );
    }

    /// §4.4: the extension comes from the SERVED format, the folder suffix from
    /// the REQUESTED label — they are allowed to disagree, and do when Qobuz
    /// downgrades a request.
    #[test]
    fn extension_follows_the_served_format_not_the_request() {
        assert_eq!(purchase_extension(5, "audio/flac"), "mp3");
        assert_eq!(purchase_extension(27, "audio/mpeg"), "mp3");
        assert_eq!(purchase_extension(27, "audio/flac"), "flac");
        assert_eq!(purchase_extension(6, ""), "flac");
    }
}

// ─── §11-1: the album-downloaded rule, which no UI test could reach ────────────
#[cfg(test)]
mod album_downloaded_tests {
    use super::*;

    fn counts(pairs: &[(&str, u32)]) -> HashMap<String, u32> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn album_is_downloaded_when_the_registry_covers_every_track() {
        let c = counts(&[("alb1", 12)]);
        assert!(album_downloaded_from_registry("alb1", Some(12), &c));
        // Over-count (same track in two formats is already de-duplicated by the
        // query, but a stale row must not un-mark the album).
        assert!(album_downloaded_from_registry("alb1", Some(10), &c));
    }

    #[test]
    fn partial_downloads_do_not_mark_the_album() {
        let c = counts(&[("alb1", 11)]);
        assert!(!album_downloaded_from_registry("alb1", Some(12), &c));
    }

    #[test]
    fn unknown_album_is_not_downloaded() {
        let c = counts(&[("alb1", 12)]);
        assert!(!album_downloaded_from_registry("other", Some(12), &c));
    }

    /// No denominator means the answer is NO. Showing a green mark for an album
    /// that might be half on disk is the worse failure.
    #[test]
    fn absent_or_zero_track_count_is_never_downloaded() {
        let c = counts(&[("alb1", 99)]);
        assert!(!album_downloaded_from_registry("alb1", None, &c));
        assert!(!album_downloaded_from_registry("alb1", Some(0), &c));
    }

    /// The regression this rule exists to prevent: with the reference's
    /// predicate, an albums-only response (which carries NO tracks page) pinned
    /// every album to `false`. Here the registry answers instead.
    #[test]
    fn albums_only_response_can_still_report_downloaded() {
        let mut response = PurchaseResponse {
            albums: SearchResultsPage {
                items: vec![PurchaseAlbum {
                    id: "alb1".to_string(),
                    tracks_count: Some(2),
                    ..Default::default()
                }],
                total: 1,
                offset: 0,
                limit: 500,
            },
            // `?type=albums` omits the tracks key entirely — this is that shape.
            tracks: SearchResultsPage {
                items: vec![],
                total: 0,
                offset: 0,
                limit: 0,
            },
        };

        apply_download_flags(
            &mut response,
            &HashSet::new(),
            &HashMap::new(),
            &counts(&[("alb1", 2)]),
        );

        assert!(
            response.albums.items[0].downloaded,
            "the registry knows this album is complete even with no tracks page"
        );
    }

    #[test]
    fn track_flags_are_still_format_scoped_and_registry_driven() {
        let mut response = PurchaseResponse {
            albums: SearchResultsPage { items: vec![], total: 0, offset: 0, limit: 0 },
            tracks: SearchResultsPage {
                items: vec![
                    PurchaseTrack { id: 10, ..Default::default() },
                    PurchaseTrack { id: 11, ..Default::default() },
                ],
                total: 2,
                offset: 0,
                limit: 500,
            },
        };

        let downloaded: HashSet<i64> = [10i64].into_iter().collect();
        let mut formats: HashMap<i64, Vec<u32>> = HashMap::new();
        formats.insert(10, vec![6, 27]);

        apply_download_flags(&mut response, &downloaded, &formats, &HashMap::new());

        assert!(response.tracks.items[0].downloaded);
        assert_eq!(response.tracks.items[0].downloaded_format_ids, vec![6, 27]);
        assert!(!response.tracks.items[1].downloaded);
        assert!(response.tracks.items[1].downloaded_format_ids.is_empty());
    }
}

// ─── §14: the scope-expansion helpers ──────────────────────────────────────────
#[cfg(test)]
mod scope_expansion_tests {
    use super::*;

    #[test]
    fn release_year_parses_only_a_real_leading_year() {
        assert_eq!(parse_release_year(Some("2019-05-31")), Some(2019));
        assert_eq!(parse_release_year(Some("1971")), Some(1971));
        // A wrong year is worse than no year.
        assert_eq!(parse_release_year(Some("n/a")), None);
        assert_eq!(parse_release_year(Some("")), None);
        assert_eq!(parse_release_year(Some("19-05-31")), None);
        assert_eq!(parse_release_year(None), None);
    }

    #[test]
    fn goody_url_falls_back_through_every_known_spelling() {
        use qbz_models::Goody;

        let original = Goody {
            original_url: "https://o/a.pdf".into(),
            url: "https://u/a.pdf".into(),
            ..goody_blank()
        };
        assert_eq!(original.best_url(), Some("https://o/a.pdf"));

        let only_url = Goody { url: "https://u/a.pdf".into(), ..goody_blank() };
        assert_eq!(only_url.best_url(), Some("https://u/a.pdf"));

        let only_file_url = Goody {
            file_url: Some("https://f/a.pdf".into()),
            ..goody_blank()
        };
        assert_eq!(only_file_url.best_url(), Some("https://f/a.pdf"));

        // Nothing usable → skip the item, never fail the album.
        assert_eq!(goody_blank().best_url(), None);
        let whitespace = Goody { url: "   ".into(), ..goody_blank() };
        assert_eq!(whitespace.best_url(), None);
    }

    #[test]
    fn goody_display_name_is_never_empty() {
        use qbz_models::Goody;

        let named = Goody { name: "Booklet".into(), ..goody_blank() };
        assert_eq!(named.display_name(), "Booklet");

        let described = Goody {
            description: Some("Liner notes".into()),
            ..goody_blank()
        };
        assert_eq!(described.display_name(), "Liner notes");

        let bare = Goody { id: 42, ..goody_blank() };
        assert_eq!(bare.display_name(), "Goody 42");
    }

    fn goody_blank() -> qbz_models::Goody {
        qbz_models::Goody {
            id: 0,
            name: String::new(),
            url: String::new(),
            original_url: String::new(),
            file_url: None,
            file_format_id: None,
            description: None,
        }
    }
}
