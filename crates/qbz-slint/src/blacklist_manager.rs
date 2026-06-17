//! Blacklist Manager controller — the Rust side of the Slint port of Tauri's
//! BlacklistManagerView (Task 11). Loads the per-user blacklist into
//! `BlacklistState`, applies search-as-you-type filtering controller-side, and
//! runs the toggle / remove / clear mutations against the
//! `crate::artist_blacklist` wrapper (the same fail-open singleton the artist
//! toggle in T9 mutates).
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

use chrono::{DateTime, Utc};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{AppWindow, BlacklistState, BlacklistedArtistItem};

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

/// Push the filtered items + full count + enabled flag + query into Slint.
fn push(w: &AppWindow) {
    let (items, count) = build_items();
    let st = w.global::<BlacklistState>();
    st.set_items(ModelRc::new(VecModel::from(items)));
    st.set_count(count);
    st.set_enabled(crate::artist_blacklist::is_enabled());
    st.set_search_query(SharedString::from(current_query()));
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
                "Blacklist enabled"
            } else {
                "Blacklist disabled"
            };
            crate::toast::info(w, msg);
        }
        Err(e) => {
            log::error!("[qbz-slint] blacklist toggle-enabled failed: {e}");
            crate::toast::error(w, "Failed to toggle blacklist");
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
        .unwrap_or_else(|| "Artist".to_string());
    match crate::artist_blacklist::remove(artist_id as u64) {
        Ok(()) => {
            push(w);
            crate::toast::success(w, format!("{name} removed from blacklist"));
        }
        Err(e) => {
            log::error!("[qbz-slint] blacklist remove failed: {e}");
            crate::toast::error(w, "Failed to remove artist");
        }
    }
}

/// Clear every blacklisted artist + toast (the count is captured before).
pub fn clear_all(w: &AppWindow) {
    let count = crate::artist_blacklist::count();
    match crate::artist_blacklist::clear_all() {
        Ok(()) => {
            push(w);
            crate::toast::success(w, format!("Removed {count} artists from blacklist"));
        }
        Err(e) => {
            log::error!("[qbz-slint] blacklist clear-all failed: {e}");
            crate::toast::error(w, "Failed to clear blacklist");
        }
    }
}
