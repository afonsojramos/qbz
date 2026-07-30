//! My QBZ — collection / mixtape DETAIL controller. The Qt port of
//! `crates/qbz/src/myqbz_detail.rs` (spec 02 §6.2), publishing ONE JSON
//! document (`QbzMyQbz.detailJson`, spec 02 §5.2).
//!
//! Shape follows `playlist_qt.rs`: a module-level `Mutex` document + a cached
//! full item list backing a client-side filter -> search -> sort that
//! re-derives the visible rows. The Slint uses `thread_local!` because its
//! caches hold `!Send` `slint::Image`s; Qt documents are plain JSON, so the
//! caches are plain statics reachable from the tokio tasks with no GUI hop.
//!
//! Load order (spec 02 §6.2 `navigate`): `reset()` (clears the three caches +
//! the prefs gate, publishes `{loading:true, found:true}`) -> BLOCK fetch ->
//! `apply()` (header -> hero mosaic -> restore view-prefs -> open the persist
//! gate -> FULL_ITEMS -> rebuild) -> ONE artwork pass + ONE republish ->
//! `resolve_items` (fire-and-forget, <= 6 concurrent).
//!
//! Two divergences from the reference are REPRODUCED on purpose and must not
//! be "unified" (spec 02 §6.2 + review items 23/26):
//! - the hero mosaic's custom-cover predicate is a bare `is_some()`, while the
//!   header's `hasCustomCover` also requires the file to resolve. A collection
//!   whose stored cover file was deleted therefore shows the kind glyph in the
//!   hero and the full mosaic in the GRID.
//! - a RESOLVED album row's `typeLabel` is UNTRANSLATED (`classify_release_type`
//!   returns the marked English literal and the reference never wraps it in
//!   `t()`), while an unresolved one reads the translated `t("ALBUM")`.
//!
//! Out of scope, 1:1 with the port's own dependencies (spec 02 §1.1): the
//! offline availability filter and the offline-index badge resolution — this
//! port opens no offline-cache index.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use cxx_qt_lib::QString;
use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_models::mixtape::{
    AlbumSource, CollectionKind, CollectionPlayMode, ItemType, MixtapeCollection,
    MixtapeCollectionItem,
};
use qbz_models::QueueTrack;
use serde::Serialize;
use tokio::sync::Semaphore;

/// `resolve_items` fan-out cap (spec 02 §7 T18 / §12 #8). A 200-item
/// artist_collection would otherwise issue 200 concurrent `albums/get` calls;
/// the Slint has no cap and that is the load-bearing perf difference.
const MAX_RESOLVE_CONCURRENCY: usize = 6;

// ---------------------------------------------------------------------------
// Documents (spec 02 §5.2)
// ---------------------------------------------------------------------------

/// One inline track row for the expanded view-mode. The keys are exactly what
/// `qml/rows/TrackRow.qml` reads off its `item` (spec 02 §5.2), plus `source`,
/// which the HOST delegate needs for its `draggable` arm (a local row's `id` is
/// a library.db row id and the sidebar drop handler would forward it as a Qobuz
/// catalog id). `number` is deliberately absent — it is a delegate property
/// (`property int number`), not an item field.
#[derive(Clone, Default, Serialize)]
pub struct InlineTrackRow {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    /// Always "" — the inline list is rendered under its own album.
    pub album: String,
    #[serde(rename = "albumId")]
    pub album_id: String,
    /// "m:ss", or "--:--" when the duration is unknown (never "0:00").
    pub duration: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub explicit: bool,
    /// Seeds `TrackRow`'s own `favorite` property; never written back (local
    /// hearts are unwired in this port and MyQBZ mixes sources).
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
    /// Always "" — the rows are mounted with `showArtwork: false`.
    #[serde(rename = "artPath")]
    pub art_path: String,
    /// "qobuz" | "local" | "plex" — the delegate's `draggable` gate.
    pub source: String,
}

/// One render row (spec 02 §5.2 `DetailRow`).
#[derive(Clone, Default, Serialize)]
pub struct DetailRow {
    /// 0-based PERSISTED position — the stable key. Display is +1.
    pub position: i32,
    #[serde(rename = "itemType")]
    pub item_type: String,
    /// The STORED source ("qobuz" | "local"), which is what the source filter
    /// matches on.
    pub source: String,
    #[serde(rename = "sourceItemId")]
    pub source_item_id: String,
    pub title: String,
    pub subtitle: String,
    #[serde(rename = "subtitleIsLink")]
    pub subtitle_is_link: bool,
    /// Resolved Qobuz artist id; "" until `resolve_items` lands (and for
    /// local/Plex items, whose tracks carry none).
    #[serde(rename = "artistId")]
    pub artist_id: String,
    /// RESOLVED source ("qobuz" | "plex" | "local") — this is how a Plex item
    /// stored as `AlbumSource::Local` finally reads "plex".
    #[serde(rename = "sourceKind")]
    pub source_kind: String,
    #[serde(rename = "typeLabel")]
    pub type_label: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    /// True until `resolve_items` fills this row — drives the quality skeleton.
    #[serde(rename = "qualityResolving")]
    pub quality_resolving: bool,
    #[serde(rename = "tracksText")]
    pub tracks_text: String,
    #[serde(rename = "yearText")]
    pub year_text: String,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    #[serde(rename = "artPath")]
    pub art_path: String,
    /// Alias of `artUrl` — spec 01 §13.1 spells the row cover
    /// `artworkUrl`/`artworkPath` while spec 02 §5.2 spells it `artUrl`/`artPath`.
    /// Both are emitted (filled in `publish`) so either QML spelling resolves;
    /// an unread key costs one string per row.
    #[serde(rename = "artworkUrl")]
    pub artwork_url: String,
    #[serde(rename = "artworkPath")]
    pub artwork_path: String,
    pub selected: bool,
    #[serde(rename = "canExpand")]
    pub can_expand: bool,
    #[serde(rename = "tracksLoaded")]
    pub tracks_loaded: bool,
    #[serde(rename = "expandLoading")]
    pub expand_loading: bool,
    #[serde(rename = "inlineTracks")]
    pub inline_tracks: Vec<InlineTrackRow>,
}

/// The published detail document (spec 02 §5.2 `DetailDoc`).
#[derive(Clone, Serialize)]
pub struct DetailDoc {
    pub id: String,
    pub kind: String,
    #[serde(rename = "kindLabel")]
    pub kind_label: String,
    pub name: String,
    pub description: String,
    pub meta: String,
    #[serde(rename = "itemCount")]
    pub item_count: i32,
    #[serde(rename = "playMode")]
    pub play_mode: String,
    pub loading: bool,
    pub found: bool,
    #[serde(rename = "hasCustomCover")]
    pub has_custom_cover: bool,
    #[serde(rename = "customCoverPath")]
    pub custom_cover_path: String,
    #[serde(rename = "coverCount")]
    pub cover_count: i32,
    pub cols: i32,
    #[serde(rename = "heroCellUrls")]
    pub hero_cell_urls: Vec<String>,
    #[serde(rename = "heroCellPaths")]
    pub hero_cell_paths: Vec<String>,
    /// Alias of `heroCellUrls` — spec 01 §13.1 names the hero cells `urls`.
    /// Filled in `publish`.
    pub urls: Vec<String>,
    pub search: String,
    pub sort: String,
    #[serde(rename = "sortDir")]
    pub sort_dir: String,
    #[serde(rename = "typeFilter")]
    pub type_filter: String,
    #[serde(rename = "srcQobuz")]
    pub src_qobuz: bool,
    #[serde(rename = "srcPlex")]
    pub src_plex: bool,
    #[serde(rename = "srcLocal")]
    pub src_local: bool,
    #[serde(rename = "viewMode")]
    pub view_mode: String,
    #[serde(rename = "selectMode")]
    pub select_mode: bool,
    #[serde(rename = "selectedCount")]
    pub selected_count: i32,
    #[serde(rename = "filterCount")]
    pub filter_count: i32,
    #[serde(rename = "hasAnyFilter")]
    pub has_any_filter: bool,
    pub items: Vec<DetailRow>,
}

impl Default for DetailDoc {
    /// The §18 toolbar defaults (`list` / `position` / `asc` / `all` / no
    /// sources). `found` starts TRUE so the not-found state is only ever
    /// reached deliberately.
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: String::new(),
            kind_label: String::new(),
            name: String::new(),
            description: String::new(),
            meta: String::new(),
            item_count: 0,
            play_mode: "in_order".to_string(),
            loading: false,
            found: true,
            has_custom_cover: false,
            custom_cover_path: String::new(),
            cover_count: 0,
            cols: 2,
            hero_cell_urls: Vec::new(),
            hero_cell_paths: Vec::new(),
            urls: Vec::new(),
            search: String::new(),
            sort: "position".to_string(),
            sort_dir: "asc".to_string(),
            type_filter: "all".to_string(),
            src_qobuz: false,
            src_plex: false,
            src_local: false,
            view_mode: "list".to_string(),
            select_mode: false,
            selected_count: 0,
            filter_count: 0,
            has_any_filter: false,
            items: Vec::new(),
        }
    }
}

/// The live-resolved per-row display values `MixtapeCollectionItem` cannot
/// carry (spec 02 §6.2 `resolve_from_tracks`).
#[derive(Clone, Default)]
struct ResolvedItem {
    /// "qobuz" | "plex" | "local".
    source_kind: String,
    quality_tier: String,
    quality_detail: String,
    /// Uppercased TYPE label. For albums this is the UNTRANSLATED
    /// `classify_release_type` literal — see the module header.
    type_label: String,
    /// First resolved track's artwork with the `file://` prefix STRIPPED,
    /// backfilling rows stored with empty art.
    artwork_url: String,
    artist_id: String,
}

// ---------------------------------------------------------------------------
// State (spec 02 §6.2 "cache homes in Qt")
// ---------------------------------------------------------------------------

static DOC: Mutex<Option<DetailDoc>> = Mutex::new(None);

/// The full, ORIGINAL-order item list for the open collection — the canonical
/// source the toolbar derives the visible list from.
static FULL_ITEMS: Mutex<Vec<MixtapeCollectionItem>> = Mutex::new(Vec::new());

/// Expanded-mode inline tracks, keyed `"{source}|{source_item_id}"`. Survives
/// every re-derive so a sort/search does not re-fetch.
static INLINE_CACHE: LazyLock<Mutex<HashMap<String, Vec<InlineTrackRow>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// resolveItems results, same key. A MISS is what flags a row
/// `qualityResolving` (the skeleton).
static RESOLVE_CACHE: LazyLock<Mutex<HashMap<String, ResolvedItem>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Select-mode selection, by PERSISTED position.
///
/// The Slint keeps `selected` on the render row and `to_item` hardcodes it
/// `false`, so a sort/filter/search silently clears every checkbox AND leaves
/// `selected_count` stale — a bulk action on a "3 selected" bar then does
/// nothing. Holding the set here reproduces the visible behaviour (the set is
/// cleared when a filter/sort/search INPUT changed) while keeping the count and
/// the bulk actions in agreement with what the user sees (spec 02 §12 #13).
static SELECTED: LazyLock<Mutex<HashSet<i32>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Fingerprint of the last derive's filter/sort/search inputs — what decides
/// whether a rebuild is a RE-DERIVE (clears the selection) or a row patch.
static LAST_FILTER_KEY: Mutex<Option<String>> = Mutex::new(None);

/// Per-collection view-prefs hydration gate: `false` from `reset()` until
/// `apply()` has restored the stored prefs. Every persist is suppressed while
/// it is false, or an early setter clobbers the prefs about to be restored
/// (spec 02 §7 T15).
///
/// THIS IS THE ONLY COPY OF THIS LATCH. `myqbz_prefs_qt` used to declare a
/// second one and gate `save_view_prefs` on it; nothing ever opened that one, so
/// the per-collection prefs were never persisted at all. Do not re-add a gate
/// on the store side — this module owns the lifecycle (`reset` closes, `apply`
/// opens AFTER the restore, `teardown` closes).
static PREFS_HYDRATED: AtomicBool = AtomicBool::new(false);

fn with_doc<R>(f: impl FnOnce(&mut DetailDoc) -> R) -> R {
    let mut guard = DOC.lock().unwrap();
    let doc = guard.get_or_insert_with(DetailDoc::default);
    f(doc)
}

/// Serialize + hop. The two alias key pairs are filled HERE, in one place, so
/// a row patch can never leave them out of step with their canonical twin.
fn publish(doc: &DetailDoc) {
    let mut out = doc.clone();
    out.urls = out.hero_cell_urls.clone();
    for row in out.items.iter_mut() {
        row.artwork_url = row.art_url.clone();
        row.artwork_path = row.art_path.clone();
    }
    let json = serde_json::to_string(&out).unwrap_or_else(|_| "{}".into());
    crate::myqbz_bridge::ui(move |mut b| {
        b.as_mut().set_detail_json(QString::from(json.as_str()));
    });
}

fn publish_current() {
    let doc = with_doc(|d| d.clone());
    publish(&doc);
}

// ---------------------------------------------------------------------------
// String helpers (myqbz_detail.rs:177-249)
// ---------------------------------------------------------------------------

fn kind_str(kind: CollectionKind) -> &'static str {
    match kind {
        CollectionKind::Mixtape => "mixtape",
        CollectionKind::Collection => "collection",
        CollectionKind::ArtistCollection => "artist_collection",
    }
}

/// Eyebrow label, uppercase + translated, matching the grid card.
fn kind_label(kind: CollectionKind) -> String {
    match kind {
        CollectionKind::Mixtape => qbz_i18n::t("MIXTAPE"),
        CollectionKind::ArtistCollection => qbz_i18n::t("ARTIST"),
        CollectionKind::Collection => qbz_i18n::t("COLLECTION"),
    }
}

/// Same label from the already-published `kind` string (the language-switch
/// republish has no typed `CollectionKind` to hand).
fn kind_label_from_str(kind: &str) -> String {
    match kind {
        "mixtape" => qbz_i18n::t("MIXTAPE"),
        "artist_collection" => qbz_i18n::t("ARTIST"),
        "collection" => qbz_i18n::t("COLLECTION"),
        _ => String::new(),
    }
}

fn play_mode_str(mode: CollectionPlayMode) -> &'static str {
    match mode {
        CollectionPlayMode::InOrder => "in_order",
        CollectionPlayMode::AlbumShuffle => "album_shuffle",
    }
}

pub(crate) fn source_str(source: AlbumSource) -> &'static str {
    match source {
        AlbumSource::Qobuz => "qobuz",
        AlbumSource::Local => "local",
    }
}

pub(crate) fn item_type_str(t: ItemType) -> &'static str {
    match t {
        ItemType::Album => "album",
        ItemType::Track => "track",
        ItemType::Playlist => "playlist",
    }
}

/// Always "album(s)" regardless of the item types, 1:1 with the grid meta.
fn album_count_label(count: usize) -> String {
    qbz_i18n::tf("{} album", "{} albums", count as i64, &[&count.to_string()])
}

/// TYPE cell for an UNRESOLVED row — translated, uppercase.
fn type_label(t: ItemType) -> String {
    match t {
        ItemType::Album => qbz_i18n::t("ALBUM"),
        ItemType::Track => qbz_i18n::t("TRACK"),
        ItemType::Playlist => qbz_i18n::t("PLAYLIST"),
    }
}

/// Track-count release heuristic (`qbz/src/album_map.rs:193`): `<=3` Single,
/// `<=6` EP, else Album. The reference marks these literals at the definition
/// and the MyQBZ call site does NOT translate them — reproduced verbatim.
fn classify_release_type(track_count: Option<u32>) -> &'static str {
    match track_count {
        Some(n) if n <= 3 => "Single",
        Some(n) if n <= 6 => "EP",
        _ => "Album",
    }
}

/// TRACKS column: "1" for a track, else the count or an em-dash (U+2014).
fn tracks_text(item: &MixtapeCollectionItem) -> String {
    match item.item_type {
        ItemType::Track => "1".to_string(),
        _ => match item.track_count {
            Some(n) => n.to_string(),
            None => "\u{2014}".to_string(),
        },
    }
}

fn year_text(item: &MixtapeCollectionItem) -> String {
    item.year.map(|y| y.to_string()).unwrap_or_default()
}

/// Stable per-item cache key. `source_item_id` alone is the logical key;
/// pairing it with the source makes a qobuz-vs-local collision impossible.
fn cache_key(source: &str, source_item_id: &str) -> String {
    format!("{source}|{source_item_id}")
}

/// "m:ss"; a zero/missing duration renders "--:--", never "0:00".
fn track_duration_str(secs: u64) -> String {
    if secs == 0 {
        "--:--".to_string()
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

fn inline_track_title(track: &QueueTrack) -> String {
    match track.version.as_deref().filter(|v| !v.is_empty()) {
        Some(version) => format!("{} ({version})", track.title),
        None => track.title.clone(),
    }
}

/// SPEC-03: album blacklist drop inside the filter chain
/// (`myqbz_detail.rs:443` = `artist_blacklist::is_album_blacklisted`). The
/// blacklist manager is spec 03; until it lands this is the one-line swap point.
fn album_blocked(_album_id: &str) -> bool {
    false
}

// ---------------------------------------------------------------------------
// DB read path
// ---------------------------------------------------------------------------

/// Load one collection (items hydrated by the repo). `None` when the DB is
/// unavailable or the id is unknown. SYNCHRONOUS — callers wrap it in
/// `spawn_blocking`.
pub(crate) fn get_collection(id: &str) -> Option<MixtapeCollection> {
    crate::library_db_qt::with_db(false, |db| {
        Ok(db.with_connection(|conn| {
            qbz_mixtape::repo::get_collection(conn, id).unwrap_or_else(|e| {
                log::warn!("[qbz-qt] myqbz_detail get_collection({id}) failed: {e}");
                None
            })
        }))
    })
    .flatten()
}

// ---------------------------------------------------------------------------
// Row building
// ---------------------------------------------------------------------------

fn to_item(
    item: &MixtapeCollectionItem,
    resolved_cache: &HashMap<String, ResolvedItem>,
    inline_cache: &HashMap<String, Vec<InlineTrackRow>>,
    selected: &HashSet<i32>,
) -> DetailRow {
    let source = source_str(item.source);
    // `small_qobuz_url` only rewrites Qobuz CDN `_<size>.jpg` urls; running it
    // on a local filesystem path (or a Plex `/library/...` path) corrupts it.
    let mut art_url = item
        .artwork_url
        .as_deref()
        .filter(|u| !u.is_empty())
        .map(|u| {
            if item.source == AlbumSource::Qobuz {
                crate::myqbz_qt::small_qobuz_url(u, 50)
            } else {
                u.to_string()
            }
        })
        .unwrap_or_default();

    let key = cache_key(source, &item.source_item_id);
    let (source_kind, quality_tier, quality_detail, type_label_text, artist_id, quality_resolving) =
        match resolved_cache.get(&key) {
            Some(r) => {
                // Backfill the row cover from the resolved track when the
                // stored `artwork_url` was empty (disco-builder local items).
                if art_url.is_empty() && !r.artwork_url.is_empty() {
                    art_url = r.artwork_url.clone();
                }
                (
                    r.source_kind.clone(),
                    r.quality_tier.clone(),
                    r.quality_detail.clone(),
                    r.type_label.clone(),
                    r.artist_id.clone(),
                    false,
                )
            }
            None => (
                source.to_string(),
                String::new(),
                String::new(),
                type_label(item.item_type),
                String::new(),
                true,
            ),
        };

    let (inline_tracks, tracks_loaded) = match inline_cache.get(&key) {
        Some(tracks) => (tracks.clone(), true),
        None => (Vec::new(), false),
    };

    DetailRow {
        position: item.position,
        item_type: item_type_str(item.item_type).to_string(),
        source: source.to_string(),
        source_item_id: item.source_item_id.clone(),
        title: item.title.clone(),
        subtitle: item.subtitle.clone().unwrap_or_default(),
        subtitle_is_link: item.source == AlbumSource::Qobuz
            && item
                .subtitle
                .as_deref()
                .map(|s| !s.is_empty())
                .unwrap_or(false),
        artist_id,
        source_kind,
        type_label: type_label_text,
        quality_tier,
        quality_detail,
        quality_resolving,
        tracks_text: tracks_text(item),
        year_text: year_text(item),
        art_path: crate::artwork_qt::cached_path(&art_url),
        art_url,
        artwork_url: String::new(),
        artwork_path: String::new(),
        selected: selected.contains(&item.position),
        can_expand: matches!(item.item_type, ItemType::Album | ItemType::Playlist),
        tracks_loaded,
        expand_loading: false,
        inline_tracks,
    }
}

fn build_rows(items: &[MixtapeCollectionItem]) -> Vec<DetailRow> {
    let resolved = RESOLVE_CACHE.lock().unwrap();
    let inline = INLINE_CACHE.lock().unwrap();
    let selected = SELECTED.lock().unwrap();
    items
        .iter()
        .map(|it| to_item(it, &resolved, &inline, &selected))
        .collect()
}

/// One inline track row from a resolved `QueueTrack` (spec 02 §5.2).
fn track_to_item(track: &QueueTrack) -> InlineTrackRow {
    let quality_tier = match track.bit_depth {
        Some(d) if d >= 24 => "hires",
        Some(_) => "cd",
        None if track.hires => "hires",
        None => "",
    };
    let quality_detail = if quality_tier.is_empty() {
        String::new()
    } else {
        crate::home_qt::quality_detail_from_parts(track.bit_depth, track.sample_rate)
    };
    let source = track.source.clone().unwrap_or_else(|| {
        if track.is_local {
            "local".to_string()
        } else {
            "qobuz".to_string()
        }
    });

    InlineTrackRow {
        id: track.id.to_string(),
        title: inline_track_title(track),
        artist: track.artist.clone(),
        artist_id: track
            .artist_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        album: String::new(),
        album_id: track.album_id.clone().unwrap_or_default(),
        duration: track_duration_str(track.duration_secs),
        quality_tier: quality_tier.to_string(),
        quality_detail,
        explicit: track.parental_warning,
        is_favorite: false,
        art_path: String::new(),
        source,
    }
}

// ---------------------------------------------------------------------------
// Filter -> search -> sort (myqbz_detail.rs:423-496)
// ---------------------------------------------------------------------------

/// Fingerprint of every input a re-derive depends on.
fn filter_key(d: &DetailDoc) -> String {
    format!(
        "{}|{}|{}|{}|{}{}{}",
        d.search.trim().to_lowercase(),
        d.sort,
        d.sort_dir,
        d.type_filter,
        u8::from(d.src_qobuz),
        u8::from(d.src_plex),
        u8::from(d.src_local),
    )
}

fn filtered_sorted(d: &DetailDoc) -> Vec<MixtapeCollectionItem> {
    let query = d.search.trim().to_lowercase();
    let type_filter = d.type_filter.as_str();
    let (sq, sp, sl) = (d.src_qobuz, d.src_plex, d.src_local);
    let any_source = sq || sp || sl;
    let desc = d.sort_dir == "desc";

    let mut view: Vec<MixtapeCollectionItem> = {
        let items = FULL_ITEMS.lock().unwrap();
        items
            .iter()
            // Blacklist drop FIRST: album-type Qobuz items only, by their own
            // id (`source_item_id` IS the Qobuz album id).
            .filter(|it| {
                !(matches!(it.item_type, ItemType::Album)
                    && it.source == AlbumSource::Qobuz
                    && album_blocked(&it.source_item_id))
            })
            .filter(|it| type_filter == "all" || item_type_str(it.item_type) == type_filter)
            // Source filter on the STORED source, not the resolved one — 1:1.
            .filter(|it| {
                if !any_source {
                    return true;
                }
                let kind = source_str(it.source);
                (sq && kind == "qobuz") || (sp && kind == "plex") || (sl && kind == "local")
            })
            .filter(|it| {
                if query.is_empty() {
                    return true;
                }
                it.title.to_lowercase().contains(&query)
                    || it
                        .subtitle
                        .as_deref()
                        .map(|s| s.to_lowercase().contains(&query))
                        .unwrap_or(false)
            })
            .cloned()
            .collect()
    };

    match d.sort.as_str() {
        "name" => view.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        "year" => view.sort_by(|a, b| a.year.unwrap_or(0).cmp(&b.year.unwrap_or(0))),
        "tracks" => {
            view.sort_by(|a, b| a.track_count.unwrap_or(0).cmp(&b.track_count.unwrap_or(0)))
        }
        _ => view.sort_by(|a, b| a.position.cmp(&b.position)),
    }
    if desc {
        view.reverse();
    }
    view
}

/// Re-derive the render model + the derived toolbar badges. Does NOT publish —
/// `apply` and `refresh_view` own that, so a load does one publish, not three.
fn rebuild(d: &mut DetailDoc) {
    let key = filter_key(d);
    let inputs_changed = {
        let mut last = LAST_FILTER_KEY.lock().unwrap();
        let changed = last.as_deref() != Some(key.as_str());
        *last = Some(key);
        changed
    };
    if inputs_changed {
        // 1:1 with the reference (a re-derive drops every checkbox); unlike the
        // reference, the COUNT below is recomputed from the same set, so the
        // bulk bar can never claim a selection the rows do not show.
        SELECTED.lock().unwrap().clear();
    }

    let view = filtered_sorted(d);
    d.items = build_rows(&view);
    d.selected_count = SELECTED.lock().unwrap().len() as i32;

    let source_count = i32::from(d.src_qobuz) + i32::from(d.src_plex) + i32::from(d.src_local);
    d.filter_count = source_count + i32::from(d.type_filter != "all");
    d.has_any_filter = d.type_filter != "all"
        || source_count > 0
        || d.sort != "position"
        || d.sort_dir == "desc";
}

/// Re-derive, resolve the newly-visible rows' artwork, publish, and — while in
/// expanded view-mode — fetch the inline tracks of the rows the re-derive just
/// revealed.
///
/// The artwork pass belongs here as well as in `load`: the restored view-prefs
/// may have filtered rows OUT on the initial load, so a toolbar change can
/// reveal a row whose cover was never fetched. This is the reference's
/// `refresh_row_covers` (`myqbz_detail.rs:1294` names it as the second caller of
/// the artwork dispatch) — without it those rows keep the placeholder until the
/// collection is re-opened.
///
/// The `ensure_expanded` tail is the reference's `ensure_expanded_if_active`
/// (`qbz/src/main.rs:6153-6161`), which it calls after ALL FIVE toolbar setters
/// (search, sort, type filter, source filter, reset). A row that was filtered
/// out has never been fetched, so without this it appears in expanded mode with
/// no inline tracks and no spinner until the user toggles the mode again.
/// Idempotent — rows already loaded or loading are skipped, and a row cached in
/// `INLINE_CACHE` re-hydrates in `to_item` rather than re-fetching. Deliberately
/// NOT called from `apply`: the reference's load path does not auto-expand
/// either, even when the restored view-mode is `expanded`.
pub(crate) fn refresh_view() {
    with_doc(rebuild);
    let missing = attach_artwork();
    publish_current();
    dispatch_downloads(missing);
    if with_doc(|d| d.view_mode == "expanded") {
        ensure_expanded();
    }
}

// ---------------------------------------------------------------------------
// Hero mosaic (myqbz_detail.rs:359-416)
// ---------------------------------------------------------------------------

/// The hero's 0/4/9-cell mosaic at hero size 186 (cell ~93 for 2x2 -> `_150`,
/// ~62 for 3x3 -> `_50`).
///
/// DIVERGENCE, reproduced: `has_custom` here is a bare `is_some()` — no
/// `is_empty()` filter, no `exists()` check, unlike the grid card. Do not
/// unify the two predicates (spec 02 §6.2 / review item 23).
fn apply_hero_mosaic(d: &mut DetailDoc, c: &MixtapeCollection) {
    let item_count = c.items.len();
    let has_custom = c.custom_artwork_path.is_some();

    let cols: usize = if c.kind == CollectionKind::Collection && item_count >= 9 {
        3
    } else {
        2
    };
    let cell_count = cols * cols;
    let cover_count = if has_custom || item_count == 0 {
        0
    } else {
        cell_count
    };
    let target: u32 = if cols == 3 { 50 } else { 150 };

    let mut urls: Vec<String> = Vec::with_capacity(cover_count);
    for i in 0..cover_count {
        let url = match c.items.get(i) {
            Some(it) => match it.artwork_url.as_deref() {
                // Qobuz cells only — local/Plex paths pass through raw.
                Some(u) if !u.is_empty() && it.source == AlbumSource::Qobuz => {
                    crate::myqbz_qt::small_qobuz_url(u, target)
                }
                Some(u) if !u.is_empty() => u.to_string(),
                _ => String::new(),
            },
            None => String::new(),
        };
        urls.push(url);
    }

    d.cols = cols as i32;
    d.cover_count = cover_count as i32;
    // Reset the resolved cells so a re-open cannot show a stale tile.
    d.hero_cell_paths = vec![String::new(); urls.len()];
    d.hero_cell_urls = urls;
}

// ---------------------------------------------------------------------------
// Artwork pass (ONE cached_path sweep, ONE download, ONE republish — T17)
// ---------------------------------------------------------------------------

/// Resolve every hero cell + row cover through the disk cache, returning the
/// urls still missing (deduped, non-empty).
fn attach_artwork() -> Vec<String> {
    with_doc(|d| {
        // The custom cover was already resolved through `cached_path` in
        // `apply` (it is a local file, never a download).
        if d.hero_cell_paths.len() != d.hero_cell_urls.len() {
            d.hero_cell_paths = vec![String::new(); d.hero_cell_urls.len()];
        }
        let mut missing: Vec<String> = Vec::new();
        let push = |url: &str, missing: &mut Vec<String>| {
            if !url.is_empty() && !missing.iter().any(|u| u == url) {
                missing.push(url.to_string());
            }
        };
        for i in 0..d.hero_cell_urls.len() {
            let url = d.hero_cell_urls[i].clone();
            if url.is_empty() {
                continue;
            }
            let hit = crate::artwork_qt::cached_path(&url);
            if hit.is_empty() {
                push(&url, &mut missing);
            } else {
                d.hero_cell_paths[i] = hit;
            }
        }
        for row in d.items.iter_mut() {
            if row.art_url.is_empty() || !row.art_path.is_empty() {
                continue;
            }
            let hit = crate::artwork_qt::cached_path(&row.art_url);
            if hit.is_empty() {
                let url = row.art_url.clone();
                push(&url, &mut missing);
            } else {
                row.art_path = hit;
            }
        }
        missing
    })
}

/// ONE background download pass, then ONE republish (T17 — never republish per
/// cover). No-op on an empty list.
fn dispatch_downloads(missing: Vec<String>) {
    if missing.is_empty() {
        return;
    }
    crate::spawn(async move {
        crate::artwork_qt::download_missing(missing).await;
        refresh_artwork_paths();
    });
}

/// Re-resolve whatever is still unresolved after a download, then publish.
fn refresh_artwork_paths() {
    let doc = with_doc(|d| {
        if d.hero_cell_paths.len() != d.hero_cell_urls.len() {
            d.hero_cell_paths = vec![String::new(); d.hero_cell_urls.len()];
        }
        for i in 0..d.hero_cell_urls.len() {
            if d.hero_cell_paths[i].is_empty() {
                let url = d.hero_cell_urls[i].clone();
                d.hero_cell_paths[i] = crate::artwork_qt::cached_path(&url);
            }
        }
        for row in d.items.iter_mut() {
            if row.art_path.is_empty() {
                row.art_path = crate::artwork_qt::cached_path(&row.art_url);
            }
        }
        d.clone()
    });
    publish(&doc);
}

// ---------------------------------------------------------------------------
// View-prefs bridge (spec 02 §12 #11 — the file is SHARED with the Slint build)
// ---------------------------------------------------------------------------
//
// The store lives in `myqbz_prefs_qt` (`collection_view_prefs.json`). The two
// calls are isolated here so the coupling to that module's naming is in ONE
// place.

fn restore_prefs(d: &mut DetailDoc, id: &str) {
    let prefs = crate::myqbz_prefs_qt::load_view_prefs(id);
    d.view_mode = prefs.view_mode;
    d.sort = prefs.sort_by;
    d.sort_dir = prefs.sort_dir;
    d.type_filter = prefs.type_filter;
    d.src_qobuz = prefs.src_qobuz;
    d.src_plex = prefs.src_plex;
    d.src_local = prefs.src_local;
}

/// Persist the open collection's toolbar prefs — gated on the hydration flag,
/// no-op on an empty id. The 7 §18 fields only; search + select-mode stay
/// transient.
fn persist_prefs() {
    if !PREFS_HYDRATED.load(Ordering::SeqCst) {
        return;
    }
    let payload = with_doc(|d| {
        if d.id.is_empty() {
            return None;
        }
        Some((
            d.id.clone(),
            crate::myqbz_prefs_qt::Prefs {
                view_mode: d.view_mode.clone(),
                sort_by: d.sort.clone(),
                sort_dir: d.sort_dir.clone(),
                type_filter: d.type_filter.clone(),
                src_qobuz: d.src_qobuz,
                src_plex: d.src_plex,
                src_local: d.src_local,
            },
        ))
    });
    if let Some((id, prefs)) = payload {
        crate::myqbz_prefs_qt::save_view_prefs(&id, &prefs);
    }
}

// ---------------------------------------------------------------------------
// reset / apply
// ---------------------------------------------------------------------------

/// Clear the view to its loading state before a fresh load, so a re-open does
/// not flash the previous collection's hero + rows.
fn reset() {
    FULL_ITEMS.lock().unwrap().clear();
    INLINE_CACHE.lock().unwrap().clear();
    RESOLVE_CACHE.lock().unwrap().clear();
    SELECTED.lock().unwrap().clear();
    *LAST_FILTER_KEY.lock().unwrap() = None;
    // Close the persist gate until `apply` restores this collection's prefs.
    PREFS_HYDRATED.store(false, Ordering::SeqCst);
    let doc = with_doc(|d| {
        *d = DetailDoc {
            loading: true,
            found: true,
            ..DetailDoc::default()
        };
        d.clone()
    });
    publish(&doc);
}

/// Apply a freshly-loaded collection. The ORDER is load-bearing: the persist
/// gate opens AFTER the restore, so the restore itself is not re-persisted.
fn apply(c: MixtapeCollection) {
    with_doc(|d| {
        let item_count = c.items.len();
        d.id = c.id.clone();
        d.kind = kind_str(c.kind).to_string();
        d.kind_label = kind_label(c.kind);
        d.name = c.name.clone();
        d.description = c.description.clone().unwrap_or_default();
        d.meta = album_count_label(item_count);
        d.item_count = item_count as i32;
        d.play_mode = play_mode_str(c.play_mode).to_string();
        d.found = true;

        // Header custom cover: non-empty AND resolvable. `cached_path` returns
        // "" for a path that is not a readable file, which folds the
        // reference's `exists()` + decodable pair into one call.
        let resolved_cover = c
            .custom_artwork_path
            .as_deref()
            .filter(|p| !p.is_empty())
            .map(crate::artwork_qt::cached_path)
            .unwrap_or_default();
        d.has_custom_cover = !resolved_cover.is_empty();
        d.custom_cover_path = resolved_cover;

        apply_hero_mosaic(d, &c);

        restore_prefs(d, &c.id);
        PREFS_HYDRATED.store(true, Ordering::SeqCst);

        *FULL_ITEMS.lock().unwrap() = c.items;
        rebuild(d);
        d.loading = false;
    });
}

/// Mark the load as not-found (the id resolved to no collection).
fn apply_not_found() {
    let doc = with_doc(|d| {
        d.loading = false;
        d.found = false;
        d.clone()
    });
    publish(&doc);
}

// ---------------------------------------------------------------------------
// resolveItems (spec 02 §6.2, capped at MAX_RESOLVE_CONCURRENCY)
// ---------------------------------------------------------------------------

/// Derive a row's resolved display values from its resolved tracks. Album-level
/// quality = the first resolved track's quality (24-bit+ = Hi-Res), the same
/// rule every other surface uses.
fn resolve_from_tracks(item: &MixtapeCollectionItem, tracks: &[QueueTrack]) -> ResolvedItem {
    let stored = source_str(item.source);
    let first = tracks.first();

    let source_kind = match first {
        Some(t) => t.source.clone().unwrap_or_else(|| {
            if t.is_local {
                "local".to_string()
            } else {
                "qobuz".to_string()
            }
        }),
        None => stored.to_string(),
    };

    let quality_tier = match first {
        Some(t) => match t.bit_depth {
            Some(d) if d >= 24 => "hires",
            Some(_) => "cd",
            None if t.hires => "hires",
            None => "",
        },
        None => "",
    };
    let quality_detail = match (first, quality_tier.is_empty()) {
        (Some(t), false) => crate::home_qt::quality_detail_from_parts(t.bit_depth, t.sample_rate),
        _ => String::new(),
    };

    // Albums resolve their release type from the resolved track count; the
    // literal is NOT translated here — see the module header.
    let type_label_text = match item.item_type {
        ItemType::Album => classify_release_type(Some(tracks.len() as u32)).to_uppercase(),
        other => type_label(other),
    };

    // First resolved track's artwork, with the `file://` prefix STRIPPED: the
    // artwork channel reads a bare filesystem path.
    let artwork_url = first
        .and_then(|t| t.artwork_url.clone())
        .map(|u| u.strip_prefix("file://").map(str::to_string).unwrap_or(u))
        .unwrap_or_default();

    let artist_id = first
        .and_then(|t| t.artist_id)
        .map(|id| id.to_string())
        .unwrap_or_default();

    ResolvedItem {
        source_kind,
        quality_tier: quality_tier.to_string(),
        quality_detail,
        type_label: type_label_text,
        artwork_url,
        artist_id,
    }
}

/// Insert into `RESOLVE_CACHE` and patch the RENDERED row IN PLACE. A full
/// array replacement would reset `ListView.contentY` and throw the user to the
/// top (spec 02 §9.1) — and this is not a re-derive, so it must not clear the
/// selection either.
fn apply_resolved(item: &MixtapeCollectionItem, resolved: ResolvedItem, collection_id: &str) {
    let key = cache_key(source_str(item.source), &item.source_item_id);
    RESOLVE_CACHE
        .lock()
        .unwrap()
        .insert(key, resolved.clone());

    let stored_art_empty = item
        .artwork_url
        .as_deref()
        .map(|u| u.is_empty())
        .unwrap_or(true);

    let (doc, backfill) = with_doc(|d| {
        if d.id != collection_id {
            return (None, None);
        }
        let mut backfill: Option<String> = None;
        if let Some(row) = d
            .items
            .iter_mut()
            .find(|r| r.source_item_id == item.source_item_id)
        {
            row.source_kind = resolved.source_kind.clone();
            row.quality_tier = resolved.quality_tier.clone();
            row.quality_detail = resolved.quality_detail.clone();
            row.type_label = resolved.type_label.clone();
            row.artist_id = resolved.artist_id.clone();
            row.quality_resolving = false;
            if row.art_url.is_empty() && !resolved.artwork_url.is_empty() {
                row.art_url = resolved.artwork_url.clone();
                row.art_path = crate::artwork_qt::cached_path(&row.art_url);
                if row.art_path.is_empty() && stored_art_empty {
                    backfill = Some(row.art_url.clone());
                }
            }
        }
        (Some(d.clone()), backfill)
    });
    if let Some(doc) = doc {
        publish(&doc);
    }
    if let Some(url) = backfill {
        crate::spawn(async move {
            crate::artwork_qt::download_missing(vec![url]).await;
            refresh_artwork_paths();
        });
    }
}

/// Resolve every not-yet-cached item's tracks and hydrate its row.
/// Fire-and-forget: a failure leaves the stored-source defaults in place.
fn resolve_items(runtime: Arc<AppRuntime<LoggingAdapter>>) {
    let pending: Vec<MixtapeCollectionItem> = {
        let items = FULL_ITEMS.lock().unwrap();
        let cache = RESOLVE_CACHE.lock().unwrap();
        items
            .iter()
            .filter(|it| !cache.contains_key(&cache_key(source_str(it.source), &it.source_item_id)))
            .cloned()
            .collect()
    };
    if pending.is_empty() {
        return;
    }
    let collection_id = current_id();
    if collection_id.is_empty() {
        return;
    }
    crate::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(MAX_RESOLVE_CONCURRENCY));
        let mut handles = Vec::with_capacity(pending.len());
        for item in pending {
            let sem = Arc::clone(&semaphore);
            let runtime = Arc::clone(&runtime);
            let collection_id = collection_id.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore open");
                // The user may have navigated to another collection while this
                // task queued behind the cap — its result is then dead weight.
                if current_id() != collection_id {
                    return;
                }
                let tracks = crate::myqbz_play_qt::fetch_item_tracks(&runtime, &item).await;
                let resolved = resolve_from_tracks(&item, &tracks);
                apply_resolved(&item, resolved, &collection_id);
            }));
        }
        for handle in handles {
            let _ = handle.await;
        }
    });
}

// ---------------------------------------------------------------------------
// Expanded mode (spec 02 §6.2 ensure_expanded)
// ---------------------------------------------------------------------------

fn full_item_by_source_id(source_item_id: &str) -> Option<MixtapeCollectionItem> {
    FULL_ITEMS
        .lock()
        .unwrap()
        .iter()
        .find(|it| it.source_item_id == source_item_id)
        .cloned()
}

/// Ensure every expandable row's inline tracks are loaded. Marks the pending
/// rows loading in ONE pass, publishes, then resolves each. Idempotent —
/// already-cached rows are skipped, so re-entering expanded mode is instant.
///
/// DEVIATION (reported): the fan-out is capped at `MAX_RESOLVE_CONCURRENCY`,
/// the same cap T18 mandates for `resolve_items`. The reference is uncapped and
/// an expanded 200-item artist_collection would open 200 sockets; the visible
/// behaviour (rows filling in progressively) is identical.
pub(crate) fn ensure_expanded() {
    let (collection_id, pending) = with_doc(|d| {
        let mut pending: Vec<String> = Vec::new();
        for row in d.items.iter_mut() {
            if row.can_expand && !row.tracks_loaded && !row.expand_loading {
                row.expand_loading = true;
                pending.push(row.source_item_id.clone());
            }
        }
        (d.id.clone(), pending)
    });
    if pending.is_empty() {
        return;
    }
    publish_current();

    let runtime = crate::app();
    crate::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(MAX_RESOLVE_CONCURRENCY));
        let mut handles = Vec::with_capacity(pending.len());
        for source_item_id in pending {
            let sem = Arc::clone(&semaphore);
            let runtime = Arc::clone(&runtime);
            let collection_id = collection_id.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore open");
                if current_id() != collection_id {
                    return;
                }
                let Some(item) = full_item_by_source_id(&source_item_id) else {
                    // No backing item (should not happen) — clear the spinner.
                    patch_expanded(&source_item_id, &collection_id, None);
                    return;
                };
                let tracks = crate::myqbz_play_qt::fetch_item_tracks(&runtime, &item).await;
                let rows: Vec<InlineTrackRow> = tracks.iter().map(track_to_item).collect();
                let key = cache_key(source_str(item.source), &item.source_item_id);
                INLINE_CACHE.lock().unwrap().insert(key, rows.clone());
                patch_expanded(&source_item_id, &collection_id, Some(rows));
            }));
        }
        for handle in handles {
            let _ = handle.await;
        }
    });
}

/// Patch ONE row's expansion state in place (never a re-derive).
fn patch_expanded(
    source_item_id: &str,
    collection_id: &str,
    rows: Option<Vec<InlineTrackRow>>,
) {
    let doc = with_doc(|d| {
        if d.id != collection_id {
            return None;
        }
        let row = d
            .items
            .iter_mut()
            .find(|r| r.source_item_id == source_item_id)?;
        row.expand_loading = false;
        if let Some(rows) = rows {
            row.tracks_loaded = true;
            row.inline_tracks = rows;
        }
        Some(d.clone())
    });
    if let Some(doc) = doc {
        publish(&doc);
    }
}

// ---------------------------------------------------------------------------
// Toolbar (spec 02 §6.2 :499-562)
// ---------------------------------------------------------------------------

/// Transient — the ONLY setter that does not persist.
pub(crate) fn search(query: &str) {
    with_doc(|d| d.search = query.to_string());
    refresh_view();
}

/// Re-selecting the active field flips asc/desc; a new field resets to asc.
pub(crate) fn set_sort(field: &str) {
    with_doc(|d| {
        if d.sort == field {
            d.sort_dir = if d.sort_dir == "asc" { "desc" } else { "asc" }.to_string();
        } else {
            d.sort = field.to_string();
            d.sort_dir = "asc".to_string();
        }
    });
    persist_prefs();
    refresh_view();
}

pub(crate) fn set_type_filter(value: &str) {
    with_doc(|d| d.type_filter = value.to_string());
    persist_prefs();
    refresh_view();
}

/// Multi-select; the menu stays open in the view.
pub(crate) fn toggle_source_filter(kind: &str) {
    with_doc(|d| match kind {
        "qobuz" => d.src_qobuz = !d.src_qobuz,
        "plex" => d.src_plex = !d.src_plex,
        "local" => d.src_local = !d.src_local,
        _ => {}
    });
    persist_prefs();
    refresh_view();
}

/// Type `all`, no sources, sort `position` asc. The search query is left
/// INTACT (the reference's reset does not clear it, and `hasAnyFilter` excludes
/// search).
pub(crate) fn reset_filters() {
    with_doc(|d| {
        d.type_filter = "all".to_string();
        d.src_qobuz = false;
        d.src_plex = false;
        d.src_local = false;
        d.sort = "position".to_string();
        d.sort_dir = "asc".to_string();
    });
    persist_prefs();
    refresh_view();
}

/// `list` | `grid` | `expanded`. The reference does NOT re-derive here (the
/// item list is unchanged, only the layout switches), so neither do we: mutate,
/// persist, PUBLISH. The caches, the selection and the scroll position survive.
/// `expanded` additionally fires `ensure_expanded`.
pub(crate) fn set_view_mode(mode: &str) {
    with_doc(|d| d.view_mode = mode.to_string());
    persist_prefs();
    publish_current();
    if mode == "expanded" {
        ensure_expanded();
    }
}

/// Toggle multi-select edit mode. Leaving clears the selection.
pub(crate) fn toggle_select_mode() {
    let doc = with_doc(|d| {
        let on = !d.select_mode;
        if !on {
            SELECTED.lock().unwrap().clear();
            for row in d.items.iter_mut() {
                row.selected = false;
            }
            d.selected_count = 0;
        }
        d.select_mode = on;
        d.clone()
    });
    publish(&doc);
}

/// Toggle one row's selection by its stable POSITION, then recount. A row patch
/// — never a re-derive.
pub(crate) fn toggle_item_select(position: i32) {
    let doc = with_doc(|d| {
        let now_selected = {
            let mut set = SELECTED.lock().unwrap();
            if set.contains(&position) {
                set.remove(&position);
                false
            } else {
                set.insert(position);
                true
            }
        };
        if let Some(row) = d.items.iter_mut().find(|r| r.position == position) {
            row.selected = now_selected;
        }
        d.selected_count = SELECTED.lock().unwrap().len() as i32;
        d.clone()
    });
    publish(&doc);
}

/// Clear the selection (uncheck every row + zero the count) while STAYING in
/// select-mode. Used after a bulk action completes.
pub(crate) fn clear_selection() {
    let doc = with_doc(|d| {
        SELECTED.lock().unwrap().clear();
        for row in d.items.iter_mut() {
            row.selected = false;
        }
        d.selected_count = 0;
        d.clone()
    });
    publish(&doc);
}

/// The selected row POSITIONS, ascending.
pub(crate) fn selected_positions() -> Vec<i32> {
    let mut positions: Vec<i32> = SELECTED.lock().unwrap().iter().copied().collect();
    positions.sort_unstable();
    positions
}

/// The full `MixtapeCollectionItem`s for the selected positions, in ascending
/// position order (the render order may differ). The bulk add/enqueue payloads
/// need the numeric year / track_count the render row does not carry.
pub(crate) fn selected_full_items() -> Vec<MixtapeCollectionItem> {
    let positions = selected_positions();
    let items = FULL_ITEMS.lock().unwrap();
    positions
        .iter()
        .filter_map(|p| items.iter().find(|it| it.position == *p).cloned())
        .collect()
}

// ---------------------------------------------------------------------------
// Open-document accessors (the bridge invokables carry no id)
// ---------------------------------------------------------------------------

pub(crate) fn current_id() -> String {
    with_doc(|d| d.id.clone())
}

pub(crate) fn current_kind() -> String {
    with_doc(|d| d.kind.clone())
}

pub(crate) fn current_play_mode() -> String {
    with_doc(|d| d.play_mode.clone())
}

pub(crate) fn current_name() -> String {
    with_doc(|d| d.name.clone())
}

pub(crate) fn current_description() -> String {
    with_doc(|d| d.description.clone())
}

/// The RENDERED row's stored `itemType` for `source_item_id`, defaulting to
/// `"album"` when no row carries it (1:1 with `qbz/src/main.rs:6795-6804`, which
/// reads the same value off the item model with the same fallback).
///
/// Used by the inline-track menu's go-to arm: the parent's REAL type decides
/// album view vs playlist view, and hardcoding "album" would mis-route a
/// playlist parent.
pub(crate) fn row_item_type(source_item_id: &str) -> String {
    with_doc(|d| {
        d.items
            .iter()
            .find(|r| r.source_item_id == source_item_id)
            .map(|r| r.item_type.clone())
            .unwrap_or_else(|| "album".to_string())
    })
}

// ---------------------------------------------------------------------------
// Load / navigation
// ---------------------------------------------------------------------------

/// Load (or RE-load) the collection `id` into the detail document. Does NOT
/// touch the nav stack — `open` does that, and the mutation reloads must not
/// push a second entry.
pub(crate) async fn load(runtime: &Arc<AppRuntime<LoggingAdapter>>, id: String) {
    reset();

    // D11.c GAP: the reference drops items failing the offline availability
    // rule here (`myqbz.rs:231 retain_available_offline`). This port opens no
    // offline-cache index, so no item is hidden (spec 02 §1.1).
    let fetch_id = id.clone();
    let collection = tokio::task::spawn_blocking(move || get_collection(&fetch_id))
        .await
        .ok()
        .flatten();

    let Some(collection) = collection else {
        log::warn!("[qbz-qt] myqbz_detail load({id}): collection not found");
        apply_not_found();
        return;
    };

    apply(collection);
    let missing = attach_artwork();
    publish_current();
    dispatch_downloads(missing);

    resolve_items(Arc::clone(runtime));
}

/// Grid card click: push the route, then load.
pub(crate) fn open(id: String) {
    crate::nav_qt::record("mixtapedetail");
    let runtime = crate::app();
    crate::spawn(async move {
        load(&runtime, id).await;
    });
}

/// Re-run the load for the same id after a mutation (the reference's
/// "-> reload"), WITHOUT a second nav record.
pub(crate) fn reload(id: String) {
    if id.is_empty() {
        return;
    }
    let runtime = crate::app();
    crate::spawn(async move {
        load(&runtime, id).await;
    });
}

/// Reload the open detail only when `id` IS the one on screen (the hero
/// Shuffle's play-mode side effect, spec 02 §6.3).
pub(crate) fn reload_if_open(id: &str) {
    if !id.is_empty() && current_id() == id {
        reload(id.to_string());
    }
}

/// Re-publish with the Rust-built strings rebuilt in the new locale
/// (`apply_language`, W18): the kind eyebrow, the "N albums" plural and every
/// unresolved row's TYPE label.
pub(crate) fn republish() {
    let doc = with_doc(|d| {
        if !d.kind.is_empty() {
            d.kind_label = kind_label_from_str(&d.kind);
        }
        d.meta = album_count_label(d.item_count.max(0) as usize);
        rebuild(d);
        d.clone()
    });
    publish(&doc);
}

/// Drop every per-user cache (logout, W8) so the next account inherits nothing.
pub(crate) fn teardown() {
    FULL_ITEMS.lock().unwrap().clear();
    INLINE_CACHE.lock().unwrap().clear();
    RESOLVE_CACHE.lock().unwrap().clear();
    SELECTED.lock().unwrap().clear();
    *LAST_FILTER_KEY.lock().unwrap() = None;
    PREFS_HYDRATED.store(false, Ordering::SeqCst);
    let doc = with_doc(|d| {
        *d = DetailDoc::default();
        d.clone()
    });
    publish(&doc);
}

// ---------------------------------------------------------------------------
// Navigation out of a row, and the bulk-action dispatch
// ---------------------------------------------------------------------------

/// Row / card click: open the item it stands for. 1:1 with the Slint handler
/// (`qbz/src/main.rs:6270-6294`): album AND track items both open an album view
/// — `crate::open_album` is what routes Qobuz-vs-local and records history, so
/// the source argument is deliberately unused here, exactly as it is there.
pub(crate) fn open_item(_source: String, item_type: String, source_item_id: String) {
    match item_type.as_str() {
        "album" | "track" => crate::open_album(source_item_id),
        "playlist" => crate::open_playlist(source_item_id),
        other => {
            log::warn!("[qbz-qt] myqbz_detail open_item: unknown type {other}");
        }
    }
}

/// The artist link on a row, routed by SOURCE (`qbz/src/main.rs:6302-6325`).
/// Stored items only carry the artist NAME, so a Qobuz row needs the numeric id
/// the resolve pass derived from its first track — and when that resolve has not
/// landed yet the click is IGNORED rather than falling back to the name, because
/// the name would open the LocalLibrary artist, i.e. the wrong page.
pub(crate) fn open_artist(source: String, artist_name: String, artist_id: String) {
    if source == "qobuz" {
        if artist_id.trim().is_empty() {
            log::warn!(
                "[qbz-qt] myqbz_detail open_artist: qobuz item '{artist_name}' has no resolved \
                 artist id yet — ignoring click"
            );
            return;
        }
        crate::open_artist(artist_id);
        return;
    }
    if !artist_name.trim().is_empty() {
        // local / plex -> the LocalLibrary Artists tab, BY NAME.
        crate::local_album_actions::open_artist_by_name(artist_name);
    }
}

/// The multi-select bar's seven actions (`qbz/src/main.rs:6448-6541`). Every arm
/// reads the selection here rather than taking it as an argument, so the QML
/// only ever sends the action id.
pub(crate) fn bulk_action(id: String) {
    match id.as_str() {
        "add-to-queue" | "play-next" => {
            if selected_positions().is_empty() {
                return;
            }
            crate::myqbz_play_qt::bulk_enqueue(id.as_str() == "play-next");
        }
        "play-later" => {
            if selected_positions().is_empty() {
                return;
            }
            crate::myqbz_play_qt::bulk_enqueue_later();
        }
        "add-to-mixtape" => {
            let items: Vec<crate::myqbz_add_qt::AddItem> = selected_full_items()
                .iter()
                .map(|it| crate::myqbz_add_qt::AddItem {
                    item_type: item_type_str(it.item_type).to_string(),
                    source: source_str(it.source).to_string(),
                    source_item_id: it.source_item_id.clone(),
                    title: it.title.clone(),
                    subtitle: it.subtitle.clone(),
                    artwork_url: it.artwork_url.clone(),
                    year: it.year,
                    track_count: it.track_count,
                })
                .collect();
            if items.is_empty() {
                return;
            }
            crate::myqbz_add_qt::open_items(items);
        }
        "remove-selected" => crate::myqbz_edit_qt::remove_selected(),
        "clear" => clear_selection(),
        // The Slint's seventh action resolves the selection to Qobuz track ids
        // and opens the global playlist picker. This port has no playlist picker
        // yet — the same gap LabelView.qml:18 and MixView.qml:32 already record
        // for their own bars — so the entry stays inert rather than silently
        // doing something else.
        "add-to-playlist" => {
            log::warn!("[qbz-qt] myqbz_detail bulk_action: add-to-playlist has no picker in this port");
        }
        other => {
            log::warn!("[qbz-qt] myqbz_detail bulk_action: unknown id {other}");
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (pure functions only — no Qt, no network, no engine)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn item(position: i32, title: &str, kind: ItemType, source: AlbumSource) -> MixtapeCollectionItem {
        MixtapeCollectionItem {
            collection_id: "c".into(),
            position,
            item_type: kind,
            source,
            source_item_id: format!("id{position}"),
            title: title.into(),
            subtitle: Some("Artist".into()),
            artwork_url: None,
            year: Some(2000 + position),
            track_count: Some(position + 1),
            added_at: 0,
        }
    }

    #[test]
    fn tracks_text_uses_em_dash_for_unknown_counts() {
        let mut it = item(0, "A", ItemType::Album, AlbumSource::Qobuz);
        it.track_count = None;
        assert_eq!(tracks_text(&it), "\u{2014}");
        let track = item(1, "B", ItemType::Track, AlbumSource::Qobuz);
        assert_eq!(tracks_text(&track), "1");
    }

    #[test]
    fn duration_placeholder_is_never_zero() {
        assert_eq!(track_duration_str(0), "--:--");
        assert_eq!(track_duration_str(61), "1:01");
    }

    #[test]
    fn release_type_heuristic_matches_album_map() {
        assert_eq!(classify_release_type(Some(3)), "Single");
        assert_eq!(classify_release_type(Some(6)), "EP");
        assert_eq!(classify_release_type(Some(7)), "Album");
        assert_eq!(classify_release_type(None), "Album");
    }

    #[test]
    fn cache_key_never_collides_across_sources() {
        assert_ne!(cache_key("qobuz", "1"), cache_key("local", "1"));
    }

    #[test]
    fn filter_key_tracks_every_derive_input() {
        let mut a = DetailDoc::default();
        let base = filter_key(&a);
        a.search = " Foo ".into();
        assert_ne!(filter_key(&a), base);
        let mut b = DetailDoc::default();
        b.src_plex = true;
        assert_ne!(filter_key(&b), base);
        let mut c = DetailDoc::default();
        c.sort_dir = "desc".into();
        assert_ne!(filter_key(&c), base);
    }
}
