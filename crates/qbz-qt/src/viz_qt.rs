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
//! for an instant mode switch but never published).
//!
//! Protected-audio note: this lives entirely downstream of the read-only ring
//! buffer. It touches none of the device/stream init (CLAUDE.md "Audio
//! backend — PROTECTED"); the tap is a passive sample copy.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
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
        // stream the visible mode renders signals. The other cells are still
        // latched so a mode switch has a frame ready immediately.
        let mode = ACTIVE_MODE.load(Ordering::Relaxed);
        let wake = match frame {
            VizFrame::Viz16(b) => {
                *self.cells.bars.lock().unwrap() = Some(b);
                !matches!(mode, 1 | 2)
            }
            VizFrame::Wave256x2(b) => {
                *self.cells.waveform.lock().unwrap() = Some(b);
                mode == 1
            }
            VizFrame::Energy5(b) => {
                *self.cells.energy.lock().unwrap() = Some(b);
                mode == 2
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
static ENABLED: AtomicBool = AtomicBool::new(false);
/// The band's render mode (0 Bars / 1 Waveform / 2 Energy). The drain only
/// publishes the ONE stream that mode consumes: in Bars mode, marshalling the
/// 512-float waveform into a QList 30 times a second would be pure waste.
static ACTIVE_MODE: AtomicI32 = AtomicI32::new(0);

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

/// Take the pending frame of the stream the visible mode renders (if any) and
/// hop it onto the Qt thread. Each cell's guard is released by the end of its
/// `let` statement — the mutex is NEVER held across the `viz_bridge::ui` hop,
/// so the producer never blocks on the drain.
fn publish_active(cells: &VizCells) {
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

/// Capture gate — the `QbzViz.setEnabled` invokable. Toggles the tap, wakes the
/// parked producer/drain, and (when turning off) leaves the last frame on
/// screen frozen, exactly like the Slint handler.
pub fn set_enabled(on: bool) {
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
