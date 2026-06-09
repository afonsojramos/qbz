//! My QBZ — Collection / Mixtape DETAIL view controller (read-only slice).
//!
//! Mirrors `crate::playlist` (a cached full item list backs a client-side
//! filter -> search -> sort that re-derives the visible model) and reuses the
//! grid controller's mosaic + URL-downscale helpers from `crate::myqbz`. It
//! loads ONE `MixtapeCollection` (items come hydrated) via
//! `qbz_mixtape::repo::get_collection` through `library_db::with_db` +
//! `with_connection`, precomputes every display string (type label, source
//! kind, quality detail, tracks/year columns, downscaled `_50` row artwork
//! URL, up-to-9 hero-mosaic URLs), and pushes ready-to-render
//! `MixtapeDetailItem`s into `MyQbzDetailState`. The view does NO per-row
//! lookups.
//!
//! READ-ONLY SCOPE (Phase-2 Slice 3): nav-in (the grid card click) routes here
//! and loads real data — that is the testable path. The hero CTAs
//! (play/shuffle/dj-mix/edit/delete/sync), per-row context-menu items, and the
//! select-mode bulk bar are VISIBLE 1:1 but their handlers are logging stubs
//! (wired in main.rs). DEFERRED to a later slice: the live source/quality
//! `resolveItems` resolution (so quality badges + plex/local source kinds are
//! placeholders here, derived only from the stored `source`), the per-item
//! inline track expansion (the "expanded" view-mode renders its toggle + shell
//! only), the rename/description/delete/cover/DJ-mix modals, and persisted
//! per-collection view-prefs.
//!
//! The backend (`qbz-mixtape`) is reused directly — no Tauri command wrappers
//! (ADR-005), headless (ADR-006).

use qbz_models::mixtape::{
    AlbumSource, CollectionKind, CollectionPlayMode, ItemType, MixtapeCollection,
    MixtapeCollectionItem,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::artwork::{self, ArtworkJob, ArtworkTarget, ImageCache};
use crate::{AppWindow, ContentView, MixtapeDetailItem, MyQbzDetailState, NavState};

thread_local! {
    /// The full, original-order item list for the open collection — the
    /// canonical source the toolbar derives the visible list from. UI thread
    /// only (mirrors `playlist::FULL_ITEMS`).
    static FULL_ITEMS: std::cell::RefCell<Vec<MixtapeCollectionItem>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

// ──────────────────────────── DB read path ────────────────────────────

/// Load one collection (items hydrated by the repo) from the per-user
/// library.db. Returns `None` when the DB is unavailable or the id is unknown.
pub fn get_collection(id: &str) -> Option<MixtapeCollection> {
    crate::library_db::with_db(|db| {
        Ok(db.with_connection(|conn| {
            qbz_mixtape::repo::get_collection(conn, id).unwrap_or_else(|e| {
                log::warn!("[qbz-slint] myqbz_detail get_collection({id}) failed: {e}");
                None
            })
        }))
    })
    .flatten()
}

// ──────────────────────────── string helpers ──────────────────────────

fn kind_str(kind: CollectionKind) -> &'static str {
    match kind {
        CollectionKind::Mixtape => "mixtape",
        CollectionKind::Collection => "collection",
        CollectionKind::ArtistCollection => "artist_collection",
    }
}

/// Eyebrow label (Tauri `kindLabel`): mixtapes.label / collections.artistLabel
/// / collections.label, uppercased to match the grid card eyebrow.
fn kind_label(kind: CollectionKind) -> &'static str {
    match kind {
        CollectionKind::Mixtape => "MIXTAPE",
        CollectionKind::ArtistCollection => "ARTIST",
        CollectionKind::Collection => "COLLECTION",
    }
}

fn play_mode_str(mode: CollectionPlayMode) -> &'static str {
    match mode {
        CollectionPlayMode::InOrder => "in_order",
        CollectionPlayMode::AlbumShuffle => "album_shuffle",
    }
}

fn source_str(source: AlbumSource) -> &'static str {
    match source {
        AlbumSource::Qobuz => "qobuz",
        AlbumSource::Local => "local",
    }
}

fn item_type_str(t: ItemType) -> &'static str {
    match t {
        ItemType::Album => "album",
        ItemType::Track => "track",
        ItemType::Playlist => "playlist",
    }
}

/// `mixtapes.albumCount` ICU plural — always "album(s)" regardless of
/// item_type (1:1 with the PSD / the grid card meta).
fn album_count_label(count: usize) -> String {
    if count == 1 {
        "1 album".to_string()
    } else {
        format!("{count} albums")
    }
}

/// Type-cell label, uppercase (spec 12 §6.3 col-3 `itemTypeLabel`). Release-type
/// overrides (album rows showing EP/Single/…) are a later slice — albums render
/// "ALBUM" here.
fn type_label(t: ItemType) -> &'static str {
    match t {
        ItemType::Album => "ALBUM",
        ItemType::Track => "TRACK",
        ItemType::Playlist => "PLAYLIST",
    }
}

/// TRACKS column (spec 12 §6.3 col-6 `itemTracks`): "1" for a track, else the
/// count or an em-dash.
fn tracks_text(item: &MixtapeCollectionItem) -> String {
    match item.item_type {
        ItemType::Track => "1".to_string(),
        _ => match item.track_count {
            Some(n) => n.to_string(),
            None => "—".to_string(),
        },
    }
}

/// YEAR column (spec 12 §6.3 col-7 `itemYear`): the year or "".
fn year_text(item: &MixtapeCollectionItem) -> String {
    item.year.map(|y| y.to_string()).unwrap_or_default()
}

// ──────────────────────────── model builder ───────────────────────────

/// Build one ready-to-render row. The `_50` row-artwork downscale reuses the
/// grid controller's `small_qobuz_url`. Source kind defaults from the stored
/// `source` (the live plex-vs-local-vs-qobuz `resolveItems` resolution is
/// DEFERRED, so quality badge inputs stay empty here).
fn to_item(item: &MixtapeCollectionItem) -> MixtapeDetailItem {
    let source = source_str(item.source);
    let artwork_url = item
        .artwork_url
        .as_deref()
        .filter(|u| !u.is_empty())
        .map(|u| crate::myqbz::small_qobuz_url(u, 50))
        .unwrap_or_default();

    MixtapeDetailItem {
        position: item.position,
        item_type: item_type_str(item.item_type).into(),
        source: source.into(),
        source_item_id: item.source_item_id.clone().into(),
        title: item.title.clone().into(),
        subtitle: item.subtitle.clone().unwrap_or_default().into(),
        // Only qobuz items get a clickable artist subtitle (spec 12 §6.3).
        subtitle_is_link: item.source == AlbumSource::Qobuz
            && item.subtitle.as_deref().map(|s| !s.is_empty()).unwrap_or(false),
        // Resolved source kind — defaults to the raw source until resolveItems
        // lands (a later slice). qobuz -> "qobuz"; local -> "local".
        source_kind: source.into(),
        type_label: type_label(item.item_type).into(),
        // Quality resolution is deferred; no badge until then.
        quality_tier: "".into(),
        quality_detail: "".into(),
        tracks_text: tracks_text(item).into(),
        year_text: year_text(item).into(),
        artwork_url: artwork_url.into(),
        artwork: slint::Image::default(),
        selected: false,
    }
}

// ──────────────────────────── hero mosaic ─────────────────────────────

/// Decide the hero-mosaic cover-count (0 / 4 / 9) + downscaled cell URLs, and
/// push them into `MyQbzDetailState`. Mirrors the grid card's mosaic rule
/// (3x3 only for a Collection with >= 9 items; else 2x2) but at the hero
/// `size = 186` (so the downscale target differs: 2x2 -> 150, 3x3 -> 50).
fn apply_hero_mosaic(state: &MyQbzDetailState, c: &MixtapeCollection) {
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
    // Hero renders at 186px; cell ~93 (2x2) -> 150, ~62 (3x3) -> 50.
    let target: u32 = if cols == 3 { 50 } else { 150 };

    let url = |i: usize| -> slint::SharedString {
        if has_custom || item_count == 0 || i >= cell_count {
            return slint::SharedString::default();
        }
        match c.items.get(i).and_then(|it| it.artwork_url.as_deref()) {
            Some(u) if !u.is_empty() => crate::myqbz::small_qobuz_url(u, target).into(),
            _ => slint::SharedString::default(),
        }
    };

    state.set_cover_count(cover_count as i32);
    state.set_url1(url(0));
    state.set_url2(url(1));
    state.set_url3(url(2));
    state.set_url4(url(3));
    state.set_url5(url(4));
    state.set_url6(url(5));
    state.set_url7(url(6));
    state.set_url8(url(7));
    state.set_url9(url(8));
    // Reset the decoded covers so a re-open does not show stale tiles.
    state.set_cover1(slint::Image::default());
    state.set_cover2(slint::Image::default());
    state.set_cover3(slint::Image::default());
    state.set_cover4(slint::Image::default());
    state.set_cover5(slint::Image::default());
    state.set_cover6(slint::Image::default());
    state.set_cover7(slint::Image::default());
    state.set_cover8(slint::Image::default());
    state.set_cover9(slint::Image::default());
}

// ──────────────────────────── sort / filter / search ──────────────────

/// Apply the active toolbar (type filter -> source filter -> search -> sort)
/// over `FULL_ITEMS` and push the resulting render model. Non-destructive (the
/// persisted order is untouched). UI thread only. Mirrors spec 12 §19.
pub fn refresh_view(window: &AppWindow) {
    let state = window.global::<MyQbzDetailState>();
    let query = state.get_search().trim().to_lowercase();
    let type_filter = state.get_type_filter().to_string();
    let (sq, sp, sl) = (
        state.get_src_qobuz(),
        state.get_src_plex(),
        state.get_src_local(),
    );
    let any_source = sq || sp || sl;
    let sort = state.get_sort().to_string();
    let desc = state.get_sort_dir().to_string() == "desc";

    let mut view: Vec<MixtapeCollectionItem> = FULL_ITEMS.with(|cell| {
        cell.borrow()
            .iter()
            .filter(|it| {
                // Type filter (single-select).
                type_filter == "all" || item_type_str(it.item_type) == type_filter
            })
            .filter(|it| {
                // Source filter (multi-select). source_kind currently equals
                // the raw source (resolveItems deferred) — qobuz / local.
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
    });

    match sort.as_str() {
        "name" => view.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        "year" => view.sort_by(|a, b| a.year.unwrap_or(0).cmp(&b.year.unwrap_or(0))),
        "tracks" => {
            view.sort_by(|a, b| a.track_count.unwrap_or(0).cmp(&b.track_count.unwrap_or(0)))
        }
        // default "position"
        _ => view.sort_by(|a, b| a.position.cmp(&b.position)),
    }
    if desc {
        view.reverse();
    }

    let items: Vec<MixtapeDetailItem> = view.iter().map(to_item).collect();
    state.set_items(ModelRc::new(VecModel::from(items)));

    // Derived toolbar badges (Rust-computed; the view only reads them).
    let source_count = (sq as i32) + (sp as i32) + (sl as i32);
    state.set_filter_count(source_count + if type_filter != "all" { 1 } else { 0 });
    state.set_has_any_filter(
        type_filter != "all" || any_source || sort != "position" || desc,
    );
}

/// Update the search query and re-render.
pub fn search(window: &AppWindow, query: &str) {
    window.global::<MyQbzDetailState>().set_search(query.into());
    refresh_view(window);
}

/// Set the sort field. Re-selecting the active field flips asc/desc; a new
/// field resets to asc (spec 12 §5.4 `selectSort`).
pub fn set_sort(window: &AppWindow, field: &str) {
    let state = window.global::<MyQbzDetailState>();
    if state.get_sort() == field {
        let dir = if state.get_sort_dir() == "asc" { "desc" } else { "asc" };
        state.set_sort_dir(dir.into());
    } else {
        state.set_sort(field.into());
        state.set_sort_dir("asc".into());
    }
    refresh_view(window);
}

/// Single-select the type filter.
pub fn set_type_filter(window: &AppWindow, value: &str) {
    window.global::<MyQbzDetailState>().set_type_filter(value.into());
    refresh_view(window);
}

/// Toggle one source-filter flag (multi-select; menu stays open in the view).
pub fn toggle_source_filter(window: &AppWindow, kind: &str) {
    let state = window.global::<MyQbzDetailState>();
    match kind {
        "qobuz" => state.set_src_qobuz(!state.get_src_qobuz()),
        "plex" => state.set_src_plex(!state.get_src_plex()),
        "local" => state.set_src_local(!state.get_src_local()),
        _ => {}
    }
    refresh_view(window);
}

/// Reset filters + sort (spec 12 §5.6 reset: type 'all', no sources, sort
/// 'position' asc). Search query is left intact (Tauri's reset doesn't clear
/// it; `hasAnyFilter` excludes search).
pub fn reset_filters(window: &AppWindow) {
    let state = window.global::<MyQbzDetailState>();
    state.set_type_filter("all".into());
    state.set_src_qobuz(false);
    state.set_src_plex(false);
    state.set_src_local(false);
    state.set_sort("position".into());
    state.set_sort_dir("asc".into());
    refresh_view(window);
}

/// Toggle multi-select edit mode. Leaving clears any selection.
pub fn toggle_select_mode(window: &AppWindow) {
    let state = window.global::<MyQbzDetailState>();
    let on = !state.get_select_mode();
    if !on {
        let model = state.get_items();
        for i in 0..model.row_count() {
            if let Some(mut it) = model.row_data(i) {
                if it.selected {
                    it.selected = false;
                    model.set_row_data(i, it);
                }
            }
        }
        state.set_selected_count(0);
    }
    state.set_select_mode(on);
}

/// Toggle one row's selection by position. Recounts the selection.
pub fn toggle_item_select(window: &AppWindow, position: i32) {
    let state = window.global::<MyQbzDetailState>();
    let model = state.get_items();
    for i in 0..model.row_count() {
        if let Some(mut it) = model.row_data(i) {
            if it.position == position {
                it.selected = !it.selected;
                model.set_row_data(i, it);
                break;
            }
        }
    }
    let count = (0..model.row_count())
        .filter(|&i| model.row_data(i).map(|it| it.selected).unwrap_or(false))
        .count() as i32;
    state.set_selected_count(count);
}

// ──────────────────────────── reset / apply ───────────────────────────

/// Clear the view to its loading state before a fresh load (so a re-open does
/// not flash the previous collection's hero + rows).
pub fn reset(window: &AppWindow) {
    FULL_ITEMS.with(|cell| cell.borrow_mut().clear());
    let state = window.global::<MyQbzDetailState>();
    state.set_loading(true);
    state.set_found(true);
    state.set_items(ModelRc::new(VecModel::from(Vec::<MixtapeDetailItem>::new())));
    state.set_name("".into());
    state.set_description("".into());
    state.set_meta("".into());
    state.set_item_count(0);
    state.set_has_custom_cover(false);
    state.set_custom_cover(slint::Image::default());
    state.set_cover_count(0);
    state.set_selected_count(0);
    state.set_select_mode(false);
    // Toolbar is session-scoped per the slice spec: reset to defaults on open.
    state.set_search("".into());
    state.set_sort("position".into());
    state.set_sort_dir("asc".into());
    state.set_type_filter("all".into());
    state.set_src_qobuz(false);
    state.set_src_plex(false);
    state.set_src_local(false);
    state.set_view_mode("list".into());
    state.set_filter_count(0);
    state.set_has_any_filter(false);
}

/// Apply a freshly-loaded collection: header strings, hero mosaic, the full
/// item list (-> FULL_ITEMS), then render through the (reset) toolbar.
pub fn apply(window: &AppWindow, c: MixtapeCollection) {
    let state = window.global::<MyQbzDetailState>();
    let item_count = c.items.len();

    state.set_id(c.id.clone().into());
    state.set_kind(kind_str(c.kind).into());
    state.set_kind_label(kind_label(c.kind).into());
    state.set_name(c.name.clone().into());
    state.set_description(c.description.clone().unwrap_or_default().into());
    state.set_meta(album_count_label(item_count).into());
    state.set_item_count(item_count as i32);
    state.set_play_mode(play_mode_str(c.play_mode).into());
    state.set_found(true);

    // Custom cover (overrides the mosaic) — load the local file directly (it
    // lives in the artwork cache on disk; same as the playlist controller).
    let has_custom = c
        .custom_artwork_path
        .as_ref()
        .filter(|p| !p.is_empty())
        .filter(|p| std::path::Path::new(p).exists())
        .and_then(|p| slint::Image::load_from_path(std::path::Path::new(p)).ok());
    if let Some(img) = has_custom {
        state.set_has_custom_cover(true);
        state.set_custom_cover(img);
    } else {
        state.set_has_custom_cover(false);
        state.set_custom_cover(slint::Image::default());
    }

    apply_hero_mosaic(&state, &c);

    FULL_ITEMS.with(|cell| *cell.borrow_mut() = c.items);
    refresh_view(window);
    state.set_loading(false);
}

/// Mark the load as not-found (the id resolved to no collection).
pub fn apply_not_found(window: &AppWindow) {
    let state = window.global::<MyQbzDetailState>();
    state.set_loading(false);
    state.set_found(false);
}

// ──────────────────────────── artwork jobs ────────────────────────────

/// Build artwork jobs for the loaded collection: the up-to-9 hero-mosaic cells
/// (only when no custom cover) + one thumbnail per visible item row.
pub fn artwork_jobs(window: &AppWindow) -> Vec<ArtworkJob> {
    let state = window.global::<MyQbzDetailState>();
    let mut jobs = Vec::new();

    // Hero mosaic cells.
    if !state.get_has_custom_cover() {
        let urls = [
            state.get_url1(),
            state.get_url2(),
            state.get_url3(),
            state.get_url4(),
            state.get_url5(),
            state.get_url6(),
            state.get_url7(),
            state.get_url8(),
            state.get_url9(),
        ];
        for (slot, url) in urls.iter().enumerate() {
            if !url.is_empty() {
                jobs.push(ArtworkJob {
                    target: ArtworkTarget::MyQbzDetailCover { slot },
                    url: url.to_string(),
                });
            }
        }
    }

    // Row thumbnails (the rendered model — matched back by position on apply).
    let model = state.get_items();
    for i in 0..model.row_count() {
        let Some(item) = model.row_data(i) else { continue };
        if !item.artwork_url.is_empty() {
            jobs.push(ArtworkJob {
                target: ArtworkTarget::MyQbzDetailRow { position: item.position },
                url: item.artwork_url.to_string(),
            });
        }
    }
    jobs
}

/// Set a decoded row thumbnail by item position (the rendered model order may
/// differ from FULL_ITEMS after a sort, so match by the stable position).
pub fn set_row_artwork(window: &AppWindow, position: i32, image: slint::Image) {
    let model = window.global::<MyQbzDetailState>().get_items();
    for i in 0..model.row_count() {
        if let Some(mut it) = model.row_data(i) {
            if it.position == position {
                it.artwork = image;
                model.set_row_data(i, it);
                break;
            }
        }
    }
}

/// Set a decoded hero-mosaic cover by slot (0-8).
pub fn set_hero_cover(window: &AppWindow, slot: usize, image: slint::Image) {
    let state = window.global::<MyQbzDetailState>();
    match slot {
        0 => state.set_cover1(image),
        1 => state.set_cover2(image),
        2 => state.set_cover3(image),
        3 => state.set_cover4(image),
        4 => state.set_cover5(image),
        5 => state.set_cover6(image),
        6 => state.set_cover7(image),
        7 => state.set_cover8(image),
        8 => state.set_cover9(image),
        _ => {}
    }
}

// ──────────────────────────── navigation ──────────────────────────────

/// Open the collection-detail view for `id`: switch the ContentView + loading
/// state immediately, fetch the collection on a blocking worker, then apply +
/// render + spawn artwork. Mirrors `myqbz::navigate` (load/apply/render) and
/// the album/playlist detail navigators.
pub fn navigate(
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    image_cache: ImageCache,
    id: String,
) {
    handle.clone().spawn(async move {
        {
            let weak = weak.clone();
            let _ = weak.upgrade_in_event_loop(move |w| {
                reset(&w);
                w.global::<NavState>().set_view(ContentView::MixtapeDetail);
            });
        }

        let fetch_id = id.clone();
        let collection =
            tokio::task::spawn_blocking(move || get_collection(&fetch_id)).await.ok().flatten();

        let _ = weak.upgrade_in_event_loop(move |w| match collection {
            Some(c) => {
                apply(&w, c);
                let jobs = artwork_jobs(&w);
                artwork::spawn_loads(jobs, w.as_weak(), image_cache.clone());
            }
            None => {
                log::warn!("[qbz-slint] myqbz_detail navigate({id}): collection not found");
                apply_not_found(&w);
            }
        });
    });
}
