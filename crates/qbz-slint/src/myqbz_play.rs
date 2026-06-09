//! My QBZ — Collection / Mixtape DETAIL **playback** (Phase-2 Slice 5).
//!
//! Wires the detail view's hero Play / Shuffle CTAs, the per-row Play action,
//! and the per-row context menu (play / play-next / add-to-queue) to the
//! shared `qbz-mixtape` ENQUEUE resolver, then drives the already-shared
//! `qbz-core` queue + `qbz-app` `RuntimeManager` queue-source stamp.
//!
//! Behavior is 1:1 with Tauri's `v2_enqueue_collection` /
//! `v2_enqueue_collection_item` (spec 40 §5/§6, gotchas §9):
//! - **Resolve all** uses the collection's persisted `play_mode`; the hero
//!   Shuffle forces `AlbumShuffle` ordering (time-seeded, whole-item shuffle).
//! - Failed items are logged + skipped (partial playback > total failure) —
//!   that is `resolve_collection_tracks`' own contract; the per-item path
//!   mirrors it manually.
//! - `play_next` inserts in **REVERSE** so the first resolved track lands
//!   immediately after the current track.
//! - The queue-source-collection stamp is set **only on replace** (hero
//!   play/shuffle + per-row replace-play); append/play_next preserve context.
//! - `touch_play` is best-effort and runs **only** on the whole-collection
//!   replace paths (hero play/shuffle), never per-row.
//!
//! Frontend-agnostic (ADR-005/006): the `qbz-mixtape` crate holds all the
//! resolution logic; this module only builds a `ProdItemResolver` (Qobuz client
//! + a `Send + Sync` local closure that runs `with_db` synchronously — no
//! `&LibraryDatabase` is ever held across an `.await`) and applies the result
//! to the queue.

use std::sync::Arc;

use qbz_models::mixtape::{CollectionPlayMode, MixtapeCollection, MixtapeCollectionItem};
use qbz_models::QueueTrack;
use qbz_mixtape::enqueue::{resolve_collection_tracks, ProdItemResolver};

use crate::adapter::SlintAdapter;
use crate::playback::{after_track_change, refresh_sidebar};
use crate::AppWindow;
use qbz_app::shell::AppRuntime;

/// Convenience alias for the runtime handle threaded through every call
/// (mirrors `playback::Runtime`).
type Runtime = Arc<AppRuntime<SlintAdapter>>;

/// The per-row context-menu mode parsed from the Slint `action` string.
enum RowMode {
    /// Replace-play this single item (queue + start at 0). No queue-source
    /// stamp, no `touch_play` (per-row action, not "play the whole collection").
    Play,
    /// Insert the item's resolved tracks immediately after the current track.
    PlayNext,
    /// Append the item's resolved tracks at the end of the queue.
    AddToQueue,
}

impl RowMode {
    fn parse(action: &str) -> Option<Self> {
        match action {
            "play" => Some(Self::Play),
            "play-next" | "play_next" => Some(Self::PlayNext),
            "add-to-queue" | "add_to_queue" | "append" => Some(Self::AddToQueue),
            _ => None,
        }
    }
}

/// The synchronous local-item resolver closure handed to `ProdItemResolver`.
///
/// `with_db` is synchronous (it opens the per-user `library.db` fresh on the
/// current blocking thread), so `&LibraryDatabase` never crosses an `.await`.
/// Error semantics are preserved: the crate's `resolve_local_item` error (the
/// load-bearing user-meaningful messages — e.g. the plex "cache empty" hint,
/// the local-playlist hard error) is surfaced verbatim so
/// `resolve_collection_tracks` logs + skips the item exactly as it would for a
/// Qobuz failure. A DB-open failure becomes its own `Err` string (the item is
/// then skipped too, not silently dropped as success).
fn resolve_local(item: &MixtapeCollectionItem) -> Result<Vec<QueueTrack>, String> {
    // with_db -> Option<Result<.., String>>: Some(inner) when the DB opened
    // (inner carries the resolver's own Ok/Err); None when the DB could not be
    // opened at all. We map None to an Err so the item is skipped, not treated
    // as an empty success.
    crate::library_db::with_db(|db| Ok(qbz_mixtape::enqueue::resolve_local_item(db, item)))
        .unwrap_or_else(|| Err("library database unavailable".to_string()))
}

/// Resolve a whole collection's items into a flat queue.
///
/// Builds a `ProdItemResolver` over the shared Qobuz client (a clone taken
/// under the client `RwLock` so the value lives for the whole resolve — its
/// `&` reference must outlive the `.await`s the Qobuz arms perform) + the
/// `Send + Sync` `resolve_local` closure, then runs
/// `resolve_collection_tracks`. `force_shuffle` overrides the persisted mode
/// with `AlbumShuffle` (time-seeded whole-item shuffle) for the hero Shuffle
/// CTA; otherwise the collection's persisted `play_mode` is used.
pub(crate) async fn resolve_collection(
    runtime: &Runtime,
    collection: &MixtapeCollection,
    force_shuffle: bool,
) -> Vec<QueueTrack> {
    let play_mode = if force_shuffle {
        CollectionPlayMode::AlbumShuffle
    } else {
        collection.play_mode
    };

    // Snapshot the Qobuz client (mirrors v2_enqueue_collection step 3 /
    // playback.rs's prefetch path). The clone lives in `client`, so the `&`
    // handed to ProdItemResolver outlives every Qobuz `.await` in resolve.
    let client_lock = runtime.core().client();
    let client = {
        let guard = client_lock.read().await;
        match guard.as_ref() {
            Some(c) => c.clone(),
            None => {
                log::warn!("[qbz-slint] myqbz_play: no Qobuz client; resolving local items only");
                // Still build a resolver — local/Plex items resolve without the
                // client; Qobuz items will error+skip inside the resolver.
                // Cloning a missing client is impossible, so bail early with the
                // local-only subset is not feasible (the resolver needs a client
                // ref). Return empty: the caller toasts "0 playable tracks".
                return Vec::new();
            }
        }
    };

    let resolver = ProdItemResolver::new(&client, resolve_local);
    resolve_collection_tracks(collection.items.clone(), play_mode, &resolver).await
}

/// Resolve a SINGLE item (per-row actions). Mirrors `v2_enqueue_collection_item`
/// (spec 40 §6): resolve the one item directly, then **stamp
/// `source_item_id_hint = item.source_item_id` INLINE** (this path bypasses
/// `resolve_collection_tracks`, so the central stamp does not run). Failed
/// resolution logs + returns empty (the caller toasts "0 playable tracks").
async fn resolve_single_item(
    runtime: &Runtime,
    item: &MixtapeCollectionItem,
) -> Vec<QueueTrack> {
    use qbz_mixtape::enqueue::ItemResolver;

    let client_lock = runtime.core().client();
    let client = {
        let guard = client_lock.read().await;
        match guard.as_ref() {
            Some(c) => c.clone(),
            None => {
                log::warn!("[qbz-slint] myqbz_play: no Qobuz client; cannot resolve item");
                return Vec::new();
            }
        }
    };

    let resolver = ProdItemResolver::new(&client, resolve_local);
    match resolver.resolve(item).await {
        Ok(mut tracks) => {
            // Inline boundary stamp (resolve_collection_tracks isn't used here).
            let hint = item.source_item_id.clone();
            for track in &mut tracks {
                track.source_item_id_hint = Some(hint.clone());
            }
            tracks
        }
        Err(e) => {
            log::warn!(
                "[qbz-slint] myqbz_play: item {}/{} resolve failed: {}",
                item.source_item_id,
                item.title,
                e
            );
            Vec::new()
        }
    }
}

/// Best-effort `repo::touch_play` (bumps last_played_at + play_count). Errors
/// ignored, exactly like the Tauri command. Runs synchronously via `with_db` —
/// safe to call from the async context (no `&Connection` crosses an `.await`).
fn touch_play(collection_id: &str) {
    let _ = crate::library_db::with_db(|db| {
        Ok(db.with_connection(|conn| {
            if let Err(e) = qbz_mixtape::repo::touch_play(conn, collection_id) {
                log::debug!("[qbz-slint] myqbz_play: touch_play({collection_id}) failed: {e}");
            }
        }))
    });
}

/// Replace the queue with `tracks`, start at index 0, stamp the queue-source
/// collection, and `touch_play`. Shared by hero Play + hero Shuffle (the two
/// whole-collection replace paths). Empty `tracks` → toast + no-op.
pub(crate) async fn play_all_tracks(
    runtime: &Runtime,
    weak: &slint::Weak<AppWindow>,
    collection_id: &str,
    tracks: Vec<QueueTrack>,
) {
    if tracks.is_empty() {
        crate::toast::error_weak(weak, "This collection resolved to 0 playable tracks");
        return;
    }
    let first_id = tracks[0].id;
    runtime.core().set_queue(tracks, Some(0)).await;
    // Queue-source stamp: ONLY on replace (spec §9.9) — this IS a replace.
    runtime
        .runtime()
        .set_queue_source_collection(Some(collection_id.to_string()))
        .await;
    after_track_change(runtime, weak, first_id).await;
    // touch_play is best-effort, replace-only.
    touch_play(collection_id);
    refresh_sidebar(true);
}

// ──────────────────────────── public entry points ─────────────────────

/// Hero **Play** (`on_play_all`): resolve the whole collection with its
/// persisted `play_mode`, then replace-play.
pub fn play_all(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    collection_id: String,
) {
    handle.spawn(async move {
        let Some(collection) = load_collection(&collection_id).await else {
            crate::toast::error_weak(&weak, "Couldn't load this collection");
            return;
        };
        let tracks = resolve_collection(&runtime, &collection, false).await;
        play_all_tracks(&runtime, &weak, &collection_id, tracks).await;
    });
}

/// Hero **Shuffle** (`on_shuffle`): resolve with forced `AlbumShuffle`
/// ordering, then replace-play (same queue-source stamp + touch_play as Play —
/// it is a replace).
pub fn shuffle(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    collection_id: String,
) {
    handle.spawn(async move {
        let Some(collection) = load_collection(&collection_id).await else {
            crate::toast::error_weak(&weak, "Couldn't load this collection");
            return;
        };
        let tracks = resolve_collection(&runtime, &collection, true).await;
        play_all_tracks(&runtime, &weak, &collection_id, tracks).await;
    });
}

/// Per-row default **Play** (`on_play_item`) and the context-menu **Play**
/// action: resolve the SINGLE item by `source_item_id`, then replace-play just
/// that item. No queue-source stamp, no touch_play (per-row, not whole
/// collection).
pub fn play_item(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    collection_id: String,
    source_item_id: String,
) {
    item_action(runtime, weak, handle, collection_id, source_item_id, "play".to_string());
}

/// Per-row context-menu action (`on_item_action`): play / play-next /
/// add-to-queue for the single item identified by `source_item_id`.
pub fn item_action(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    collection_id: String,
    source_item_id: String,
    action: String,
) {
    let Some(mode) = RowMode::parse(&action) else {
        log::warn!("[qbz-slint] myqbz_play: unknown item action {action}");
        return;
    };
    handle.spawn(async move {
        let Some(collection) = load_collection(&collection_id).await else {
            crate::toast::error_weak(&weak, "Couldn't load this collection");
            return;
        };
        let Some(item) = collection
            .items
            .iter()
            .find(|it| it.source_item_id == source_item_id)
            .cloned()
        else {
            log::warn!(
                "[qbz-slint] myqbz_play: item {source_item_id} not found in collection {collection_id}"
            );
            return;
        };

        let tracks = resolve_single_item(&runtime, &item).await;
        if tracks.is_empty() {
            crate::toast::error_weak(&weak, "This item resolved to 0 playable tracks");
            return;
        }

        match mode {
            RowMode::Play => {
                // Replace-play this single item — NO queue-source stamp, NO
                // touch_play (per-row).
                let first_id = tracks[0].id;
                runtime.core().set_queue(tracks, Some(0)).await;
                after_track_change(&runtime, &weak, first_id).await;
                refresh_sidebar(true);
            }
            RowMode::PlayNext => {
                // Insert in REVERSE so the first resolved track lands
                // immediately after the current track (spec §9.8).
                for track in tracks.into_iter().rev() {
                    runtime.core().add_track_next(track).await;
                }
                refresh_sidebar(false);
                crate::toast::success_weak(&weak, "Playing next");
            }
            RowMode::AddToQueue => {
                runtime.core().add_tracks(tracks).await;
                refresh_sidebar(false);
                crate::toast::success_weak(&weak, "Added to queue");
            }
        }
    });
}

/// Load a collection (items hydrated) off the UI/event-loop thread, on a
/// blocking worker, reusing the detail module's read path. Returns `None` when
/// the DB is unavailable or the id is unknown.
pub(crate) async fn load_collection(collection_id: &str) -> Option<MixtapeCollection> {
    let id = collection_id.to_string();
    tokio::task::spawn_blocking(move || crate::myqbz_detail::get_collection(&id))
        .await
        .ok()
        .flatten()
}
