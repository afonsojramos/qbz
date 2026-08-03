//! Audio-visualizer glue for the Qt shell — the QML counterpart of
//! `qbz/src/visualizer.rs` (Slint).
//!
//! Spawns the frontend-agnostic FFT producer (`qbz_audio::visualizer`) against
//! the runtime's [`VisualizerTap`], latches each [`VizFrame`] into a
//! single-slot cell, and hands the latest frame to the `QbzViz` bridge
//! properties from a drain thread that hops to the Qt event loop.
//!
//! The drain is SIGNAL-DRIVEN, not polled. The producer runs its own clock
//! (`TARGET_FPS = 30`); a drain on a second, independent 33 ms clock beats
//! against it and intermittently misses a frame, which reads on screen as
//! "slow motion". Instead the sink unparks the drain right after it latches a
//! frame of the stream the visible mode consumes, and the drain parks again as
//! soon as it has published it: exactly one publish per produced frame, no
//! aliasing, and zero idle wakeups (cheaper than the old poll, not just
//! smoother).
//!
//! Cost controls (the Slint install() has the same three, and they are why the
//! ambient field sits under 3% CPU during playback):
//!   1. The tap starts DISABLED — nothing is captured and the producer idles
//!      until the dock's eye toggle calls `set_enabled(true)`.
//!   2. While disabled the drain thread PARKS (no timer, no locks); enabling
//!      unparks both it and the producer for an instant wake.
//!   3. `set_paused` mirrors the transport, so a paused player parks the
//!      producer instead of re-running the FFT over a stale ring buffer.
//! Plus the mode gate: only the ONE stream the visible mode renders is
//! marshalled into a `QList` (in Bars mode the 512-float waveform is latched
//! for an instant mode switch but never published) — UNLESS the immersive
//! overlay is open, which owns the marshalling set and publishes all three
//! streams every frame (§4.2 of the immersive-port contract).
//!
//! Protected-audio note: this lives entirely downstream of the read-only ring
//! buffer. It touches none of the device/stream init (CLAUDE.md "Audio
//! backend — PROTECTED"); the tap is a passive sample copy.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::Thread;

use cxx_qt_lib::QList;
use qbz_audio::visualizer::{spawn_visualizer_thread, VizFrame, VizSink};
use qbz_audio::VisualizerTap;

use crate::viz_bridge;

/// Single-slot, latest-wins frame store shared with the FFT producer thread.
/// A stalled UI drops intermediate frames instead of growing a queue.
#[derive(Default)]
struct VizCells {
    bars: Mutex<Option<[f32; 16]>>,
    energy: Mutex<Option<[f32; 5]>>,
    waveform: Mutex<Option<Box<[f32; 512]>>>,
}

/// Producer-side sink: latches frames into the shared cells and signals the
/// drain. Never touches Qt (this runs on the FFT thread).
struct QtVizSink {
    cells: Arc<VizCells>,
}

impl VizSink for QtVizSink {
    fn submit(&self, frame: VizFrame) {
        // The producer emits several variants per FFT frame (spectral, bars,
        // energy, transient, waveform). Waking the drain on each of them would
        // cost 3-4 wakeups per frame for one useful publish, so only the
        // stream the visible mode renders signals — UNLESS the immersive
        // overlay is open (MARSHAL_ALL, §4.2b), which consumes all three. The
        // other cells are still latched so a mode switch has a frame ready
        // immediately.
        let mode = ACTIVE_MODE.load(Ordering::Relaxed);
        let all = MARSHAL_ALL.load(Ordering::Relaxed);
        let wake = match frame {
            VizFrame::Viz16(b) => {
                *self.cells.bars.lock().unwrap() = Some(b);
                stream_wakes(mode, all, Stream::Bars)
            }
            VizFrame::Wave256x2(b) => {
                *self.cells.waveform.lock().unwrap() = Some(b);
                stream_wakes(mode, all, Stream::Waveform)
            }
            VizFrame::Energy5(b) => {
                *self.cells.energy.lock().unwrap() = Some(b);
                stream_wakes(mode, all, Stream::Energy)
            }
            // The dock renders none of these; ignoring them costs nothing (the
            // producer computes the spectral ribbon only for the shader scenes).
            VizFrame::Spectral512(_) | VizFrame::Transient1(_) => false,
        };
        if wake {
            wake_drain();
        }
    }
}

struct VizHandles {
    tap: VisualizerTap,
    fft_thread: Thread,
}

static HANDLES: OnceLock<VizHandles> = OnceLock::new();
/// Drain thread handle. Published BEFORE the producer is spawned so a latched
/// frame can never find itself with nowhere to signal.
static DRAIN_THREAD: OnceLock<Thread> = OnceLock::new();
/// Drives the drain loop: false parks it indefinitely, true lets it publish.
/// This is the EFFECTIVE enable — the OR of the per-source bits in
/// `ENABLED_MASK` (§4.2a); nothing writes it except `apply_effective_enabled`.
static ENABLED: AtomicBool = AtomicBool::new(false);
/// The band's render mode (0 Bars / 1 Waveform / 2 Energy). The drain only
/// publishes the ONE stream that mode consumes: in Bars mode, marshalling the
/// 512-float waveform into a QList 30 times a second would be pure waste.
static ACTIVE_MODE: AtomicI32 = AtomicI32::new(0);

// ---------------------------------------------------------------------------
// §4.2 (immersive-port contract): two-source enable + immersive marshal-all
// ---------------------------------------------------------------------------

/// Enable-source bits (§4.2a): the tap runs while EITHER consumer wants it.
/// The dock's bit is driven by the existing `set_enabled` call sites
/// (`viz_bridge.rs:137`, `main.rs:1518`); the immersive bit by
/// `QbzImmersive`'s open funnel.
pub(crate) const DOCK_BIT: u32 = 0b01;
pub(crate) const IMMERSIVE_BIT: u32 = 0b10;
static ENABLED_MASK: AtomicU32 = AtomicU32::new(0);

/// §4.2b: while the immersive overlay is open it owns the marshalling set —
/// bars AND energy AND waveform publish every frame, regardless of
/// `ACTIVE_MODE`. Dock mode-cycle writes while it is set still record
/// `ACTIVE_MODE` (and the dock's pref) but have no effect on the marshalled
/// set; the close edge restores `ACTIVE_MODE` from the pref anyway.
static MARSHAL_ALL: AtomicBool = AtomicBool::new(false);

/// A consumer of the FFT tap (§4.2a).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum VizSource {
    Dock,
    Immersive,
}

impl VizSource {
    fn bit(self) -> u32 {
        match self {
            VizSource::Dock => DOCK_BIT,
            VizSource::Immersive => IMMERSIVE_BIT,
        }
    }
}

fn mask_with(mask: u32, bit: u32, on: bool) -> u32 {
    if on { mask | bit } else { mask & !bit }
}

fn mask_enabled(mask: u32) -> bool {
    mask != 0
}

/// The three marshalled streams (the wake table's rows).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stream {
    Bars,
    Waveform,
    Energy,
}

/// Wake table for the drain signal: marshal-all wakes on EVERY stream;
/// otherwise exactly the ONE stream the dock's active mode renders.
fn stream_wakes(mode: i32, marshal_all: bool, stream: Stream) -> bool {
    if marshal_all {
        return true;
    }
    match stream {
        Stream::Bars => !matches!(mode, 1 | 2),
        Stream::Waveform => mode == 1,
        Stream::Energy => mode == 2,
    }
}

/// Signal the drain that a frame it cares about is waiting. `unpark` is a
/// single atomic swap when the drain is already awake, and the permit it
/// leaves behind is remembered by a later `park()`, so a wake can never be
/// lost to the latch/park race.
#[inline]
fn wake_drain() {
    if let Some(t) = DRAIN_THREAD.get() {
        t.unpark();
    }
}

fn to_qlist(values: &[f32]) -> QList<f32> {
    let mut list = QList::<f32>::default();
    for v in values {
        list.append(*v);
    }
    list
}

/// Take the pending frame(s) of the stream(s) the current marshalling set
/// renders and hop them onto the Qt thread. Each cell's guard is released by
/// the end of its `let` statement — the mutex is NEVER held across the
/// `viz_bridge::ui` hop, so the producer never blocks on the drain.
fn publish_active(cells: &VizCells) {
    // §4.2b: immersive open -> publish ALL THREE streams each frame (the
    // immersive panels consume bars + energy + waveform simultaneously, like
    // the Slint immersive does). ACTIVE_MODE is irrelevant here.
    if MARSHAL_ALL.load(Ordering::Relaxed) {
        let bars = cells.bars.lock().unwrap().take();
        if let Some(b) = bars {
            viz_bridge::ui(move |mut v| v.as_mut().set_bars(to_qlist(&b)));
        }
        let energy = cells.energy.lock().unwrap().take();
        if let Some(b) = energy {
            viz_bridge::ui(move |mut v| v.as_mut().set_energy(to_qlist(&b)));
        }
        let waveform = cells.waveform.lock().unwrap().take();
        if let Some(b) = waveform {
            viz_bridge::ui(move |mut v| v.as_mut().set_waveform(to_qlist(b.as_ref())));
        }
        return;
    }
    match ACTIVE_MODE.load(Ordering::Relaxed) {
        1 => {
            let frame = cells.waveform.lock().unwrap().take();
            if let Some(b) = frame {
                viz_bridge::ui(move |mut v| v.as_mut().set_waveform(to_qlist(b.as_ref())));
            }
        }
        2 => {
            let frame = cells.energy.lock().unwrap().take();
            if let Some(b) = frame {
                viz_bridge::ui(move |mut v| v.as_mut().set_energy(to_qlist(&b)));
            }
        }
        _ => {
            let frame = cells.bars.lock().unwrap().take();
            if let Some(b) = frame {
                viz_bridge::ui(move |mut v| v.as_mut().set_bars(to_qlist(&b)));
            }
        }
    }
}

/// Wire the visualizer. Call once at startup, after the runtime is built.
/// No-op when the runtime carries no tap (i.e. it was built with
/// [`qbz_app::shell::AppRuntime::new`] instead of `with_visualizer`).
pub fn install(tap: VisualizerTap) {
    if HANDLES.get().is_some() {
        log::warn!("[qbz-qt][viz] install called twice; ignoring");
        return;
    }

    let cells = Arc::new(VizCells::default());

    // The drain: park until the sink says a frame of the visible stream is
    // ready (or until `set_enabled` wakes it), publish exactly that one frame,
    // park again. No timer, no polling — while disabled or paused it blocks
    // outright instead of spinning.
    let drain_cells = cells.clone();
    let drain_thread = std::thread::Builder::new()
        .name("qbz-qt-viz-drain".to_string())
        .spawn(move || loop {
            if !ENABLED.load(Ordering::Relaxed) {
                // Disabled: block until `set_enabled(true)` unparks us. A
                // leftover permit only costs one extra trip round this check.
                std::thread::park();
                continue;
            }
            publish_active(&drain_cells);
            // Wait for the next produced frame. If the sink unparked us while
            // we were publishing, the permit is already set and this returns
            // immediately — no wakeup is lost, we just re-check the cell.
            std::thread::park();
        })
        .expect("spawn viz drain thread")
        .thread()
        .clone();
    // Publish the handle BEFORE the producer exists: from here on, every
    // latched frame has a thread to signal.
    let _ = DRAIN_THREAD.set(drain_thread);

    let sink = Arc::new(QtVizSink {
        cells: cells.clone(),
    });
    let fft_thread = spawn_visualizer_thread(tap.clone(), sink).thread().clone();

    let _ = HANDLES.set(VizHandles { tap, fft_thread });
    log::info!("[qbz-qt][viz] producer + signal-driven drain installed (idle until enabled)");
}

/// Capture gate — the `QbzViz.setEnabled` invokable. Drives the DOCK source
/// bit (§4.2a): both existing call sites (`viz_bridge.rs:137`, `main.rs:1518`)
/// stay dock-sourced; the effective enable only flips when the OR of the bits
/// does, so a dock toggle while immersive is open (or vice versa) never parks
/// a tap the other consumer still needs.
pub fn set_enabled(on: bool) {
    set_enabled_source(VizSource::Dock, on);
}

/// §4.2a two-source enable: set one consumer's bit; the EFFECTIVE enable is
/// the OR of all bits. Only the effective edges unpark/park the FFT thread —
/// flipping one bit while another stays on changes nothing (trap 4: the
/// immersive close edge NEVER clears the dock's bit).
pub(crate) fn set_enabled_source(source: VizSource, on: bool) {
    let bit = source.bit();
    let prev = ENABLED_MASK.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |m| {
        Some(mask_with(m, bit, on))
    });
    let Ok(prev) = prev else { return };
    let was_enabled = mask_enabled(prev);
    let now_enabled = mask_enabled(mask_with(prev, bit, on));
    if was_enabled == now_enabled {
        return;
    }
    apply_effective_enabled(now_enabled);
}

/// The effective enable flipped: toggle the tap, wake the parked
/// producer/drain, and (when turning off) leave the last frame on screen
/// frozen, exactly like the Slint handler.
fn apply_effective_enabled(on: bool) {
    let Some(h) = HANDLES.get() else {
        return;
    };
    ENABLED.store(on, Ordering::Relaxed);
    h.tap.set_enabled(on);
    if on {
        // Both park while idle; unpark for an instant wake instead of waiting
        // out the producer's 200ms IDLE_POLL. The drain re-reads ENABLED (now
        // true) and publishes whatever the last latched frame was, then parks
        // until the producer signals the next one.
        h.fft_thread.unpark();
        wake_drain();
    }
    // Turning OFF needs no unpark: the drain parks on its own at the top of
    // the loop and stays there, which is exactly the "costs nothing" state.
}

/// §4.2b immersive open hook (QbzImmersive's open funnel): the immersive bit
/// goes ON and the marshalling set switches to all-three. MARSHAL_ALL is set
/// BEFORE the enable so the first publish after the wake is already the full
/// set.
pub(crate) fn immersive_opened() {
    MARSHAL_ALL.store(true, Ordering::Relaxed);
    set_enabled_source(VizSource::Immersive, true);
    if ENABLED.load(Ordering::Relaxed) {
        // The panels should not wait a producer tick for the streams the dock
        // mode was not marshalling — same rationale as `set_mode`'s wake.
        wake_drain();
    }
}

/// §4.2b immersive close hook: the immersive bit goes OFF (the dock's bit is
/// untouched — trap 4/20), single-stream marshalling resumes, and
/// `ACTIVE_MODE` is restored from the dock's persisted pref (dock mode-cycles
/// while immersive was open already updated it via
/// `settings_qt::set_large_spectrum_mode`).
pub(crate) fn immersive_closed() {
    set_enabled_source(VizSource::Immersive, false);
    MARSHAL_ALL.store(false, Ordering::Relaxed);
    set_mode(crate::settings_qt::large_spectrum_mode());
}

/// Point the drain at the stream the band is actually rendering. Called at
/// install time (from the persisted pref) and on every mode cycle.
pub fn set_mode(mode: i32) {
    ACTIVE_MODE.store(mode.clamp(0, 2), Ordering::Relaxed);
    // The new stream may already have a frame latched from before the switch;
    // publish it now instead of showing the old mode's last frame until the
    // producer's next tick (and it is what unblocks a switch made while the
    // player is paused).
    if ENABLED.load(Ordering::Relaxed) {
        wake_drain();
    }
}

/// Mirror the transport onto the tap so a paused player parks the producer.
/// Called from the playback path on every playing-state flip (the Slint side
/// does this from playback.rs).
pub fn set_paused(paused: bool) {
    if let Some(h) = HANDLES.get() {
        h.tap.set_paused(paused);
        if !paused && ENABLED.load(Ordering::Relaxed) {
            // Wake the producer only — the drain is woken by the first frame
            // the resumed producer latches.
            h.fft_thread.unpark();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §4.2a: the effective enable is the OR of the source bits.
    #[test]
    fn source_bits_or_to_the_effective_enable() {
        let mut mask = 0;
        assert!(!mask_enabled(mask));
        mask = mask_with(mask, DOCK_BIT, true);
        assert!(mask_enabled(mask));
        // Immersive on top of dock: still enabled, ONE logical state.
        mask = mask_with(mask, IMMERSIVE_BIT, true);
        assert!(mask_enabled(mask));
        assert_eq!(mask, DOCK_BIT | IMMERSIVE_BIT);
    }

    /// Trap 4/20: clearing the immersive bit NEVER clears the dock's bit, and
    /// vice versa — the tap only parks when the LAST source lets go.
    #[test]
    fn closing_one_source_keeps_the_other() {
        let both = DOCK_BIT | IMMERSIVE_BIT;
        let after_immersive_close = mask_with(both, IMMERSIVE_BIT, false);
        assert_eq!(after_immersive_close, DOCK_BIT);
        assert!(mask_enabled(after_immersive_close));
        let after_dock_off = mask_with(both, DOCK_BIT, false);
        assert_eq!(after_dock_off, IMMERSIVE_BIT);
        assert!(mask_enabled(after_dock_off));
        let none = mask_with(after_immersive_close, DOCK_BIT, false);
        assert!(!mask_enabled(none));
    }

    /// The pre-immersive wake table: exactly ONE stream per dock mode.
    #[test]
    fn single_stream_wake_table() {
        for mode in 0..=2 {
            let wakes: Vec<bool> = [Stream::Bars, Stream::Waveform, Stream::Energy]
                .into_iter()
                .map(|s| stream_wakes(mode, false, s))
                .collect();
            assert_eq!(wakes.iter().filter(|w| **w).count(), 1, "mode {mode}");
        }
        assert!(stream_wakes(0, false, Stream::Bars));
        assert!(stream_wakes(1, false, Stream::Waveform));
        assert!(stream_wakes(2, false, Stream::Energy));
    }

    /// §4.2b: marshal-all wakes on EVERY stream regardless of ACTIVE_MODE —
    /// which is also why dock mode-cycle writes while immersive is open have
    /// no effect on the marshalled set.
    #[test]
    fn marshal_all_wakes_every_stream() {
        for mode in 0..=2 {
            for s in [Stream::Bars, Stream::Waveform, Stream::Energy] {
                assert!(stream_wakes(mode, true, s), "mode {mode} stream {s:?}");
            }
        }
    }

    /// The hooks against the REAL statics (kept in ONE test so parallel test
    /// threads cannot race the shared atomics): open sets the immersive bit +
    /// marshal-all; close clears both and restores ACTIVE_MODE from the
    /// dock's persisted pref. HANDLES is unset here, so the effective-enable
    /// application is a no-op and only the masks/flags move.
    #[test]
    fn immersive_hooks_switch_and_restore() {
        // Dock was on (the common case: dock drives the tap, then immersive
        // opens on top).
        set_enabled(true);
        assert_eq!(ENABLED_MASK.load(Ordering::Relaxed), DOCK_BIT);

        immersive_opened();
        assert!(MARSHAL_ALL.load(Ordering::Relaxed));
        assert_eq!(ENABLED_MASK.load(Ordering::Relaxed), DOCK_BIT | IMMERSIVE_BIT);

        // A dock mode-cycle while immersive is open records ACTIVE_MODE but
        // the wake table stays all-three (marshal-all short-circuits it).
        set_mode(2);
        assert!(stream_wakes(2, MARSHAL_ALL.load(Ordering::Relaxed), Stream::Bars));

        immersive_closed();
        assert!(!MARSHAL_ALL.load(Ordering::Relaxed));
        // The dock's bit survived the close (trap 4/20).
        assert_eq!(ENABLED_MASK.load(Ordering::Relaxed), DOCK_BIT);
        // ACTIVE_MODE was restored from the dock pref, not left at the cycle.
        assert_eq!(
            ACTIVE_MODE.load(Ordering::Relaxed),
            crate::settings_qt::large_spectrum_mode()
        );

        // Leave the statics as found for any later test in the process.
        set_enabled(false);
        assert_eq!(ENABLED_MASK.load(Ordering::Relaxed), 0);
    }
}
