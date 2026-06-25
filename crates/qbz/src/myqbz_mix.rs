//! My QBZ — DJ-mix "Random queue" sampler (Phase-2 Slice 10).
//!
//! The Rust side of the [`MyQbzMixModal`] (spec 21 §C / spec 12 §12). Replaces
//! Tauri's `v2_collection_unique_track_count` + `v2_collection_shuffle_tracks`
//! commands (spec 40 §6) with direct calls into the shared `qbz-mixtape`
//! shuffle pipeline + the `qbz-core` queue — no Tauri wrappers (ADR-005/006).
//!
//! ## Data path
//!
//! - **On open** ([`open`]): resolve the collection's items **in-order**
//!   (`play_mode` is ignored for DJ-mix, spec §4.B — always InOrder) via the
//!   shared resolver reused from [`crate::myqbz_play::resolve_collection`], then
//!   run the DETERMINISTIC [`qbz_mixtape::shuffle::unique_track_count`] (no RNG)
//!   to get the slider's max. The discrete size set is [`build_size_options`]
//!   (50,100,150,…,All(N)); the slider indexes into it. A "Loading…" state shows
//!   while resolving.
//! - **On shuffle** ([`shuffle`]): re-resolve in-order, then — in a SYNC scope
//!   that ends BEFORE any `.await` — run [`qbz_mixtape::shuffle::dedup_by_similarity`]
//!   + [`qbz_mixtape::shuffle::hybrid_sample`] with `rand::rng()` (the thread
//!   RNG). The sampled queue is then handed to
//!   [`crate::myqbz_play::play_all_tracks`] (replace + start at 0 + stamp the
//!   queue-source collection + best-effort `touch_play`). When the sampled
//!   `actual` count is below the requested size (the per-album cap can shrink the
//!   pool, spec §9.16) a "Playing N of M" info toast is shown.
//!
//! ## RNG confinement (load-bearing, spec 40 §6)
//!
//! `rand::rng()` returns a `ThreadRng`, which is `!Send`. Holding it across an
//! `.await` would make the spawned future non-`Send` and fail to compile. So the
//! resolve (`.await`) happens FIRST into an owned `Vec<QueueTrack>`; the
//! dedup+sample then run inside a plain synchronous block `{ … }` that creates,
//! uses, and DROPS the `ThreadRng` entirely before the next `.await`
//! (`play_all_tracks`). The RNG never crosses an await point.

use std::sync::Arc;

use qbz_models::QueueTrack;
use slint::ComponentHandle;

use crate::adapter::SlintAdapter;
use crate::{AppWindow, MyQbzMixState};
use qbz_app::shell::AppRuntime;

type Runtime = Arc<AppRuntime<SlintAdapter>>;

/// Below this unique-count the only option is a single "All (N)" entry.
const SMALL_THRESHOLD: i32 = 50;
/// Intermediate options step (50, 100, 150, …).
const STEP: i32 = 50;

/// Build the discrete size options for `unique_count` (spec 21 §C.4 /
/// `buildSizeOptions`). The returned vec is the ordered size list; the LAST
/// entry is ALWAYS the "All (unique_count)" option (so `index == len - 1` ⇒ the
/// "All" entry).
///
/// - `unique_count <= 0` → `[]` (no options; the modal stays empty).
/// - `unique_count < 50` → `[unique_count]` (one "All (N)" entry).
/// - else → `[50,100,150,…]` for each `s < unique_count`, then a trailing
///   `unique_count`. If `unique_count` is itself a multiple of 50 the loop stops
///   `< unique_count`, so there is no duplicate intermediate entry
///   (e.g. 100 → `[50, All(100)]`, NOT `[50, 100, All(100)]`).
pub fn build_size_options(unique_count: i32) -> Vec<i32> {
    if unique_count <= 0 {
        return Vec::new();
    }
    if unique_count < SMALL_THRESHOLD {
        return vec![unique_count];
    }
    let mut out = Vec::new();
    let mut s = STEP;
    while s < unique_count {
        out.push(s);
        s += STEP;
    }
    out.push(unique_count); // trailing "All (N)".
    out
}

/// Push the resolved size options into `MyQbzMixState` for `unique_count`,
/// defaulting the selection to the FIRST option (= 50 for large collections, or
/// the only "All (N)" entry for small ones). UI thread.
fn apply_options(window: &AppWindow, unique_count: i32) {
    let options = build_size_options(unique_count);
    let state = window.global::<MyQbzMixState>();
    state.set_unique_count(unique_count);
    state.set_size_options(slint::ModelRc::new(slint::VecModel::from(options.clone())));
    state.set_loading(false);
    // Default selection = first option.
    apply_index(window, 0);
}

/// Set the slider index and derive the selected size + is-all flag from the
/// current options. Clamps `index` to the valid range. UI thread.
pub fn apply_index(window: &AppWindow, index: i32) {
    use slint::Model;
    let state = window.global::<MyQbzMixState>();
    let options = state.get_size_options();
    let len = options.row_count() as i32;
    if len == 0 {
        state.set_selected_index(0);
        state.set_selected_size(0);
        state.set_selected_is_all(false);
        return;
    }
    let idx = index.clamp(0, len - 1);
    let size = options.row_data(idx as usize).unwrap_or(0);
    state.set_selected_index(idx);
    state.set_selected_size(size);
    // The trailing entry is always the "All (N)" option.
    state.set_selected_is_all(idx == len - 1);
}

// ──────────────────────────── open / close ────────────────────────────

/// Open the DJ-mix modal for the collection currently shown in the detail view
/// (`collection_id`). Shows the modal in a "computing…" state, then resolves the
/// collection in-order on a worker + counts unique tracks (deterministic) and
/// fills the slider. On a resolve failure the modal closes with an error toast.
pub fn open(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    collection_id: String,
) {
    if collection_id.is_empty() {
        return;
    }
    // Show the modal immediately in its loading state.
    {
        let weak = weak.clone();
        let _ = weak.upgrade_in_event_loop(|w| {
            let state = w.global::<MyQbzMixState>();
            state.set_loading(true);
            state.set_busy(false);
            state.set_unique_count(0);
            state.set_size_options(slint::ModelRc::new(slint::VecModel::from(Vec::<i32>::new())));
            state.set_selected_index(0);
            state.set_selected_size(0);
            state.set_selected_is_all(false);
            state.set_open(true);
        });
    }

    handle.spawn(async move {
        let Some(collection) = crate::myqbz_play::load_collection(&collection_id).await else {
            close_with_error(&weak, qbz_i18n::t("Couldn't load this collection"));
            return;
        };
        // Always InOrder for DJ-mix (force_shuffle = false): the sampler does its
        // own randomization; the resolve only needs the full track pool.
        let tracks = crate::myqbz_play::resolve_collection(&runtime, &collection, false).await;
        // Deterministic count (no RNG) — the slider max + "All" size.
        let unique = qbz_mixtape::shuffle::unique_track_count(&tracks) as i32;
        let _ = weak.upgrade_in_event_loop(move |w| {
            if unique <= 0 {
                // Nothing playable — close with a hint (mirrors the resolve-empty
                // toast on the play paths).
                close(&w);
                crate::toast::error(&w, qbz_i18n::t("This collection resolved to 0 playable tracks"));
            } else {
                apply_options(&w, unique);
            }
        });
    });
}

/// Close the modal (UI thread hop, callable from any thread).
fn close_with_error(weak: &slint::Weak<AppWindow>, msg: String) {
    let _ = weak.upgrade_in_event_loop(move |w| {
        close(&w);
        crate::toast::error(&w, msg);
    });
}

/// Close the modal + clear its busy flag. UI thread.
pub fn close(window: &AppWindow) {
    let state = window.global::<MyQbzMixState>();
    state.set_open(false);
    state.set_busy(false);
    state.set_loading(false);
}

// ──────────────────────────── confirm / sample ────────────────────────

/// Confirm: sample `sample_size` songs from the collection and replace-play the
/// queue. Re-resolves the collection in-order, runs dedup+sample with the thread
/// RNG (confined to a sync scope — see the module doc), then replaces the queue
/// (stamp queue-source + touch_play via [`crate::myqbz_play::play_all_tracks`]).
/// `requested < actual` ⇒ a "Playing N of M" info toast (spec §9.16).
pub fn shuffle(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    collection_id: String,
    sample_size: i32,
) {
    if collection_id.is_empty() || sample_size <= 0 {
        return;
    }
    // Disable the Shuffle button while in flight.
    {
        let weak = weak.clone();
        let _ = weak.upgrade_in_event_loop(|w| {
            w.global::<MyQbzMixState>().set_busy(true);
        });
    }

    handle.spawn(async move {
        let Some(collection) = crate::myqbz_play::load_collection(&collection_id).await else {
            close_with_error(&weak, qbz_i18n::t("Couldn't load this collection"));
            return;
        };
        // Resolve in-order (await completes BEFORE the RNG is created).
        let resolved = crate::myqbz_play::resolve_collection(&runtime, &collection, false).await;

        // ── RNG-confined sync scope (spec 40 §6): create, use, and DROP the
        // !Send ThreadRng entirely here — it never crosses the `.await` below.
        let requested = sample_size as usize;
        let sampled: Vec<QueueTrack> = {
            let mut rng = rand::rng();
            let deduped = qbz_mixtape::shuffle::dedup_by_similarity(resolved, &mut rng);
            qbz_mixtape::shuffle::hybrid_sample(deduped, requested, &mut rng)
        };
        // `rng` is dropped; from here on the future is `Send` again.

        let actual = sampled.len();

        // Close the modal before playback starts (mirrors handleConfirmMix).
        {
            let weak = weak.clone();
            let _ = weak.upgrade_in_event_loop(|w| close(&w));
        }

        // Replace-play: set_queue + start at 0 + stamp queue-source + touch_play.
        crate::myqbz_play::play_all_tracks(&runtime, &weak, &collection_id, sampled).await;

        // DJ-mix actualCount can be < requested (per-album cap) — surface it.
        if actual > 0 && actual < requested {
            crate::toast::info_weak(
                &weak,
                qbz_i18n::t_args("Playing {} of {}", &[&actual.to_string(), &requested.to_string()]),
            );
        }
    });
}
