//! Blacklist Manager controller — the Rust side of the Slint port of Tauri's
//! BlacklistManagerView (Task 11). Loads the per-user blacklist into
//! `BlacklistState`, applies search-as-you-type filtering controller-side, and
//! runs the toggle / remove / clear mutations against the
//! `crate::artist_blacklist` wrapper (the same fail-open singleton the artist
//! toggle in T9 mutates). A third "Recommendations" tab (active-tab 2) lists
//! the reco-SCOPED "Not interested" dismissals from `crate::reco_dismiss` —
//! NOT the blacklist — with a per-row undo.
//!
//! Mirrors `crate::offline_manager`'s shape: a `load` entry point invoked on
//! navigation + an action set wired in `main.rs`. There is no change-notify on
//! the blacklist store (the fav_cache pattern), so the manager reloads on every
//! `open` — a mutation from elsewhere (the T9 artist toggle) is reflected the
//! next time the manager is opened, and the manager's own mutations re-push the
//! filtered list in place.
//!
//! Search filter (Tauri parity §7): trim the query; empty → the full list;
//! else a case-insensitive substring match on `artist_name` ONLY (notes are not
//! searched), preserving the backend's name-sorted order. `count` always
//! carries the FULL list length so the view can tell "empty blacklist"
//! (count==0) from "no search results" (count>0, filtered list empty).
//!
//! Date: `added_at` is unix SECONDS; formatted controller-side to "MMM D, YYYY"
//! (English; the Slint build has no gettext for Rust strings — matches T9
//! toasts being `format!` English).

use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::artwork::{ArtworkJob, ArtworkTarget, ImageCache};
use crate::{AppWindow, BlacklistState, BlacklistedAlbumItem, BlacklistedArtistItem, DismissedArtistItem};

/// Shared image cache for resolving blocked-album cover thumbnails (the artist
/// tab has no covers; the album tab does). Set once during startup wiring.
static IMAGE_CACHE: OnceLock<ImageCache> = OnceLock::new();

/// Store the shared image cache for album-cover resolution (idempotent).
pub fn set_image_cache(cache: ImageCache) {
    let _ = IMAGE_CACHE.set(cache);
}

/// The live search query (Rust-side source of truth). The view echoes it back
/// from `BlacklistState.search-query`; this is what `refilter` reads so a
/// toggle/remove/clear re-push keeps the current filter applied.
static QUERY: StdMutex<String> = StdMutex::new(String::new());

fn current_query() -> String {
    QUERY.lock().map(|q| q.clone()).unwrap_or_default()
}

fn set_query(q: String) {
    if let Ok(mut guard) = QUERY.lock() {
        *guard = q;
    }
}

/// Format a unix-seconds timestamp as "MMM D, YYYY" (English). Falls back to an
/// empty string for a non-representable value.
fn format_added(secs: i64) -> String {
    DateTime::<Utc>::from_timestamp(secs, 0)
        .map(|dt| dt.format("%b %-d, %Y").to_string())
        .unwrap_or_default()
}

/// Build the visible (filtered) `BlacklistedArtistItem`s from the full
/// name-sorted snapshot, applying the current query.
fn build_items() -> (Vec<BlacklistedArtistItem>, i32) {
    let all = crate::artist_blacklist::get_all();
    let count = all.len() as i32;
    let query = current_query();
    let needle = query.trim().to_lowercase();

    let items: Vec<BlacklistedArtistItem> = all
        .into_iter()
        .filter(|a| needle.is_empty() || a.artist_name.to_lowercase().contains(&needle))
        .map(|a| {
            let notes = a.notes.clone().unwrap_or_default();
            BlacklistedArtistItem {
                // artist_id is u64; an int holds Qobuz ids comfortably for
                // display + passing back on click/remove (matches OfflineRow
                // carrying ids as strings — here the spec uses int).
                artist_id: a.artist_id as i32,
                artist_name: a.artist_name.into(),
                added_at: a.added_at as i32,
                added_display: format_added(a.added_at).into(),
                has_notes: !notes.is_empty(),
                notes: notes.into(),
            }
        })
        .collect();

    (items, count)
}

/// Build the visible (filtered) blocked-album items from the full title-sorted
/// snapshot, applying the current query (matches album title OR artist name).
/// Also returns the cover-load jobs for rows that carry a cover URL (resolved
/// async; rows render the blind-eye fallback until the image lands).
fn build_album_items() -> (Vec<BlacklistedAlbumItem>, i32, Vec<ArtworkJob>) {
    let all = crate::artist_blacklist::get_all_albums();
    let count = all.len() as i32;
    let query = current_query();
    let needle = query.trim().to_lowercase();

    let mut jobs: Vec<ArtworkJob> = Vec::new();
    let items: Vec<BlacklistedAlbumItem> = all
        .into_iter()
        .filter(|a| {
            needle.is_empty()
                || a.album_title.to_lowercase().contains(&needle)
                || a.artist_name.to_lowercase().contains(&needle)
        })
        .enumerate()
        .map(|(idx, a)| {
            let notes = a.notes.clone().unwrap_or_default();
            if !a.cover_url.is_empty() {
                jobs.push(ArtworkJob {
                    url: a.cover_url.clone(),
                    target: ArtworkTarget::BlacklistAlbum { idx },
                });
            }
            BlacklistedAlbumItem {
                album_id: a.album_id.into(),
                album_title: a.album_title.into(),
                artist_name: a.artist_name.into(),
                cover_url: a.cover_url.into(),
                cover: slint::Image::default(),
                added_at: a.added_at as i32,
                added_display: format_added(a.added_at).into(),
                has_notes: !notes.is_empty(),
                notes: notes.into(),
            }
        })
        .collect();

    (items, count, jobs)
}

/// Build the visible (filtered) dismissed-artist items — the "Not interested"
/// reco-scoped list — from the store snapshot, applying the current query
/// (name match, same rule as the artist axis). `count` carries the FULL list
/// length for the tab badge + empty/no-results split.
fn build_dismissed_items() -> (Vec<DismissedArtistItem>, i32) {
    let all = crate::reco_dismiss::list();
    let count = all.len() as i32;
    let needle = current_query().trim().to_lowercase();

    let items: Vec<DismissedArtistItem> = all
        .into_iter()
        .filter(|a| needle.is_empty() || a.name.to_lowercase().contains(&needle))
        .map(|a| DismissedArtistItem {
            // Same int pass-through as the blacklist artist id.
            artist_id: a.artist_id as i32,
            artist_name: a.name.into(),
        })
        .collect();

    (items, count)
}

/// Push the filtered items + full count + enabled flag + query into Slint (all
/// three axes). Album covers resolve asynchronously via the shared image cache.
fn push(w: &AppWindow) {
    let (items, count) = build_items();
    let (album_items, album_count, jobs) = build_album_items();
    let (dismissed_items, dismissed_count) = build_dismissed_items();
    let st = w.global::<BlacklistState>();
    st.set_items(ModelRc::new(VecModel::from(items)));
    st.set_count(count);
    st.set_album_items(ModelRc::new(VecModel::from(album_items)));
    st.set_album_count(album_count);
    st.set_dismissed_items(ModelRc::new(VecModel::from(dismissed_items)));
    st.set_dismissed_count(dismissed_count);
    st.set_enabled(crate::artist_blacklist::is_enabled());
    st.set_search_query(SharedString::from(current_query()));
    // Kick off cover loads (best-effort; needs the cache + a weak handle).
    if let Some(cache) = IMAGE_CACHE.get() {
        if !jobs.is_empty() {
            crate::artwork::spawn_loads(jobs, w.as_weak(), cache.clone());
        }
    }
}

/// Load (or refresh) the manager: mark loading, read the store, push state,
/// clear loading. Synchronous — the wrapper reads are in-memory / a single
/// SQLite query, so there is no worker hop (unlike the offline manager's
/// index scan).
pub fn load(weak: slint::Weak<AppWindow>) {
    let _ = weak.upgrade_in_event_loop(|w| {
        w.global::<BlacklistState>().set_loading(true);
        push(&w);
        w.global::<BlacklistState>().set_loading(false);
    });
}

// --- Actions ------------------------------------------------------------

/// Search-as-you-type: store the query and re-push the filtered list. `count`
/// stays the full length (so the empty-vs-no-results split stays correct).
pub fn search_changed(w: &AppWindow, query: String) {
    set_query(query);
    push(w);
}

/// Toggle the global enable flag; on success re-read + re-push and info-toast.
/// On error, surface the wrapper's message (no state change).
pub fn toggle_enabled(w: &AppWindow) {
    let new_state = !crate::artist_blacklist::is_enabled();
    match crate::artist_blacklist::set_enabled(new_state) {
        Ok(()) => {
            push(w);
            let msg = if new_state {
                qbz_i18n::t("Blacklist enabled")
            } else {
                qbz_i18n::t("Blacklist disabled")
            };
            crate::toast::info(w, msg);
        }
        Err(e) => {
            log::error!("[qbz-slint] blacklist toggle-enabled failed: {e}");
            crate::toast::error(w, qbz_i18n::t("Failed to toggle blacklist"));
        }
    }
}

/// Remove one artist (optimistic — re-push drops the row immediately) + toast.
pub fn remove(w: &AppWindow, artist_id: i32) {
    // Capture the name before removing, for the toast.
    let name = crate::artist_blacklist::get_all()
        .into_iter()
        .find(|a| a.artist_id == artist_id as u64)
        .map(|a| a.artist_name)
        .unwrap_or_else(|| qbz_i18n::t("Artist"));
    match crate::artist_blacklist::remove(artist_id as u64) {
        Ok(()) => {
            push(w);
            crate::toast::success(w, qbz_i18n::t_args("{} removed from blacklist", &[&name]));
        }
        Err(e) => {
            log::error!("[qbz-slint] blacklist remove failed: {e}");
            crate::toast::error(w, qbz_i18n::t("Failed to remove artist"));
        }
    }
}

/// Clear every blacklisted artist + toast (the count is captured before).
pub fn clear_all(w: &AppWindow) {
    let count = crate::artist_blacklist::count();
    match crate::artist_blacklist::clear_all() {
        Ok(()) => {
            push(w);
            crate::toast::success(w, qbz_i18n::tf("Removed {} artist from blacklist", "Removed {} artists from blacklist", count as i64, &[&count.to_string()]));
        }
        Err(e) => {
            log::error!("[qbz-slint] blacklist clear-all failed: {e}");
            crate::toast::error(w, qbz_i18n::t("Failed to clear blacklist"));
        }
    }
}

// --- Album axis actions -------------------------------------------------

/// Switch the manager's active tab (0 = Artists, 1 = Albums, 2 = Recommendations).
pub fn set_tab(w: &AppWindow, tab: i32) {
    w.global::<BlacklistState>().set_active_tab(tab);
}

/// Block an album from a context menu (grid card / list row). Adds it and
/// re-pushes the manager state + count badges; the source grid drops the card
/// on its next navigation (no global observer — the artist-block convention).
pub fn block_album(w: &AppWindow, id: String, title: String, artist: String, cover: String) {
    if id.is_empty() {
        return;
    }
    match crate::artist_blacklist::add_album(&id, &title, &artist, &cover, None) {
        Ok(()) => {
            // If the blocked album is the one currently open, reflect the header
            // toggle immediately.
            let album_st = w.global::<crate::AlbumState>();
            if album_st.get_id().as_str() == id {
                album_st.set_is_album_blocked(true);
            }
            push(w);
            crate::toast::success(w, qbz_i18n::t_args("Album \"{}\" blocked", &[&title]));
        }
        Err(e) => {
            log::error!("[qbz-slint] album block failed: {e}");
            crate::toast::error(w, qbz_i18n::t("Failed to block album"));
        }
    }
}

/// Remove one album from the blacklist (optimistic re-push) + toast.
pub fn remove_album(w: &AppWindow, album_id: String) {
    let title = crate::artist_blacklist::get_all_albums()
        .into_iter()
        .find(|a| a.album_id == album_id)
        .map(|a| a.album_title)
        .unwrap_or_else(|| qbz_i18n::t("Album"));
    match crate::artist_blacklist::remove_album(&album_id) {
        Ok(()) => {
            let album_st = w.global::<crate::AlbumState>();
            if album_st.get_id().as_str() == album_id {
                album_st.set_is_album_blocked(false);
            }
            push(w);
            crate::toast::success(w, qbz_i18n::t_args("Album \"{}\" unblocked", &[&title]));
        }
        Err(e) => {
            log::error!("[qbz-slint] album remove failed: {e}");
            crate::toast::error(w, qbz_i18n::t("Failed to unblock album"));
        }
    }
}

/// Clear every blocked album + toast (count captured before).
pub fn clear_all_albums(w: &AppWindow) {
    let count = crate::artist_blacklist::album_count();
    match crate::artist_blacklist::clear_all_albums() {
        Ok(()) => {
            push(w);
            crate::toast::success(w, qbz_i18n::tf("Removed {} album from blacklist", "Removed {} albums from blacklist", count as i64, &[&count.to_string()]));
        }
        Err(e) => {
            log::error!("[qbz-slint] album clear-all failed: {e}");
            crate::toast::error(w, qbz_i18n::t("Failed to clear album blacklist"));
        }
    }
}


// --- Reco-dismissal axis actions -----------------------------------------

/// Undo one "Not interested" dismissal (optimistic — the re-push drops the row
/// immediately) + toast. The artist becomes eligible for the Recommendations
/// rails again on their next paint (the §B filter reads the store).
pub fn remove_dismissed(w: &AppWindow, artist_id: i32) {
    // Capture the name before removing, for the toast (falls back to the
    // generic "Artist" for rows persisted without a resolved name).
    let name = crate::reco_dismiss::list()
        .into_iter()
        .find(|a| a.artist_id == artist_id as u64)
        .map(|a| a.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| qbz_i18n::t("Artist"));
    crate::reco_dismiss::remove(artist_id as u64);
    push(w);
    crate::toast::success(w, qbz_i18n::t_args("{} restored to Recommendations", &[&name]));
}
