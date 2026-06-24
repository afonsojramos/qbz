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
//! `legacy_compat.rs:2594`). The single-track download primitive (Slice 5) is
//! added later.

use std::collections::{HashMap, HashSet};

use qbz_models::{
    Album, PurchaseAlbum, PurchaseFormatOption, PurchaseResponse, PurchaseTrack, SearchResultsPage,
};
use qbz_qobuz::QobuzClient;
use qbz_qobuz::Result as QobuzResult;

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
/// Per album (the all-mode / by-type path where albums and tracks are sibling
/// pages): collect the ids of `response.tracks.items` whose
/// `track.album.id == album.id`; then
/// `album.downloaded = !ids.is_empty() && all ids ∈ downloaded_ids`.
/// An album with NO matching tracks in this response → `downloaded = false`
/// (the empty-set rule; partial-page albums may flip to not-downloaded — this
/// is the documented page-mode gotcha and is replicated verbatim).
///
/// `downloaded_ids`/`format_map` are keyed by `i64` (registry track ids); track
/// ids are `u64` and compared via `track.id as i64`, exactly as the source.
pub fn apply_download_flags(
    response: &mut PurchaseResponse,
    downloaded_ids: &HashSet<i64>,
    format_map: &HashMap<i64, Vec<u32>>,
) {
    for track in &mut response.tracks.items {
        let tid = track.id as i64;
        track.downloaded = downloaded_ids.contains(&tid);
        track.downloaded_format_ids = format_map.get(&tid).cloned().unwrap_or_default();
    }

    for album in &mut response.albums.items {
        let album_track_ids: Vec<i64> = response
            .tracks
            .items
            .iter()
            .filter(|track| {
                track
                    .album
                    .as_ref()
                    .map(|album_ref| album_ref.id == album.id)
                    .unwrap_or(false)
            })
            .map(|track| track.id as i64)
            .collect();

        album.downloaded = !album_track_ids.is_empty()
            && album_track_ids
                .iter()
                .all(|track_id| downloaded_ids.contains(track_id));
    }
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

        apply_download_flags(&mut resp, &downloaded, &format_map);

        assert!(resp.tracks.items[0].downloaded);
        assert_eq!(resp.tracks.items[0].downloaded_format_ids, vec![27, 6]);
        // track not in dlIds → not downloaded, empty format ids.
        assert!(!resp.tracks.items[1].downloaded);
        assert!(resp.tracks.items[1].downloaded_format_ids.is_empty());
    }

    #[test]
    fn apply_flags_album_downloaded_when_all_nested_track_ids_present() {
        // Every track whose album.id == "a1" is in dlIds → album downloaded.
        let mut resp = response(
            vec![album_id("a1")],
            vec![track_for_album(10, "a1"), track_for_album(20, "a1")],
        );
        apply_download_flags(&mut resp, &dl_ids(&[10, 20]), &HashMap::new());
        assert!(resp.albums.items[0].downloaded);
    }

    #[test]
    fn apply_flags_album_not_downloaded_when_partially_owned() {
        // One of the two album tracks missing from dlIds → album NOT downloaded.
        let mut resp = response(
            vec![album_id("a1")],
            vec![track_for_album(10, "a1"), track_for_album(20, "a1")],
        );
        apply_download_flags(&mut resp, &dl_ids(&[10]), &HashMap::new());
        assert!(!resp.albums.items[0].downloaded);
    }

    #[test]
    fn apply_flags_album_with_no_matching_tracks_is_not_downloaded() {
        // No tracks reference this album (empty set rule) → false, never panic.
        let mut resp = response(vec![album_id("a1")], vec![track_for_album(10, "other")]);
        apply_download_flags(&mut resp, &dl_ids(&[10]), &HashMap::new());
        assert!(!resp.albums.items[0].downloaded);
    }

    #[test]
    fn apply_flags_frontend_overrides_stale_backend_downloaded() {
        // Backend wrongly set downloaded=true; frontend recomputes to false.
        let mut track = track_for_album(10, "a1");
        track.downloaded = true;
        track.downloaded_format_ids = vec![99];
        let mut backend_true_album = album_id("a1");
        backend_true_album.downloaded = true;

        let mut resp = response(vec![backend_true_album], vec![track]);
        // dlIds empty → both must be overridden to false / cleared.
        apply_download_flags(&mut resp, &dl_ids(&[]), &HashMap::new());
        assert!(!resp.tracks.items[0].downloaded);
        assert!(resp.tracks.items[0].downloaded_format_ids.is_empty());
        assert!(!resp.albums.items[0].downloaded);
    }
}
