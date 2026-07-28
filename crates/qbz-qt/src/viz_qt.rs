//! Audio-visualizer glue for the Qt shell — the QML counterpart of
//! `qbz/src/visualizer.rs` (Slint).
//!
//! Spawns the frontend-agnostic FFT producer (`qbz_audio::visualizer`) against
//! the runtime's [`VisualizerTap`], latches each [`VizFrame`] into a
//! single-slot cell, and drains the latest frames into the `QbzViz` bridge
//! properties on a ~30 fps thread that hops to the Qt event loop.
//!
//! Cost controls (the Slint install() has the same three, and they are why the
//! ambient field sits under 3% CPU during playback):
//!   1. The tap starts DISABLED — nothing is captured and the producer idles
//!      until the dock's eye toggle calls `set_enabled(true)`.
//!   2. While disabled the drain thread PARKS (no timer, no locks); enabling
//!      unparks both it and the producer for an instant wake.
//!   3. `set_paused` mirrors the transport, so a paused player parks the
//!      producer instead of re-running the FFT over a stale ring buffer.
//!
//! Protected-audio note: this lives entirely downstream of the read-only ring
//! buffer. It touches none of the device/stream init (CLAUDE.md "Audio
//! backend — PROTECTED"); the tap is a passive sample copy.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::Thread;
use std::time::Duration;

use cxx_qt_lib::QList;
use qbz_audio::visualizer::{spawn_visualizer_thread, VizFrame, VizSink};
use qbz_audio::VisualizerTap;

use crate::viz_bridge;

/// ~30 fps, matching the producer's TARGET_FPS and the ambient field's cap.
const DRAIN_INTERVAL: Duration = Duration::from_millis(33);

/// Single-slot, latest-wins frame store shared with the FFT producer thread.
/// A stalled UI drops intermediate frames instead of growing a queue.
#[derive(Default)]
struct VizCells {
    bars: Mutex<Option<[f32; 16]>>,
    energy: Mutex<Option<[f32; 5]>>,
    waveform: Mutex<Option<Box<[f32; 512]>>>,
}

/// Producer-side sink: latches frames into the shared cells. Never touches Qt
/// (this runs on the FFT thread).
struct QtVizSink {
    cells: Arc<VizCells>,
}

impl VizSink for QtVizSink {
    fn submit(&self, frame: VizFrame) {
        match frame {
            VizFrame::Viz16(b) => *self.cells.bars.lock().unwrap() = Some(b),
            VizFrame::Energy5(b) => *self.cells.energy.lock().unwrap() = Some(b),
            VizFrame::Wave256x2(b) => *self.cells.waveform.lock().unwrap() = Some(b),
            // The dock renders none of these; ignoring them costs nothing (the
            // producer computes the spectral ribbon only for the shader scenes).
            VizFrame::Spectral512(_) | VizFrame::Transient1(_) => {}
        }
    }
}

struct VizHandles {
    tap: VisualizerTap,
    fft_thread: Thread,
    drain_thread: Thread,
}

static HANDLES: OnceLock<VizHandles> = OnceLock::new();
/// Drives the drain loop: false parks it, true runs it at DRAIN_INTERVAL.
static ENABLED: AtomicBool = AtomicBool::new(false);
/// The band's render mode (0 Bars / 1 Waveform / 2 Energy). The drain only
/// publishes the ONE stream that mode consumes: in Bars mode, marshalling the
/// 512-float waveform into a QList 30 times a second would be pure waste.
static ACTIVE_MODE: AtomicI32 = AtomicI32::new(0);

fn to_qlist(values: &[f32]) -> QList<f32> {
    let mut list = QList::<f32>::default();
    for v in values {
        list.append(*v);
    }
    list
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
    let sink = Arc::new(QtVizSink {
        cells: cells.clone(),
    });
    let fft_thread = spawn_visualizer_thread(tap.clone(), sink).thread().clone();

    // The drain: park while disabled, otherwise take the latest frames and hop
    // them onto the Qt thread. Only streams that actually produced a new frame
    // this tick are published, so a paused producer costs three try-locks.
    let drain_thread = std::thread::Builder::new()
        .name("qbz-qt-viz-drain".to_string())
        .spawn(move || loop {
            if !ENABLED.load(Ordering::Relaxed) {
                std::thread::park();
                continue;
            }
            match ACTIVE_MODE.load(Ordering::Relaxed) {
                1 => {
                    if let Some(b) = cells.waveform.lock().unwrap().take() {
                        viz_bridge::ui(move |mut v| v.as_mut().set_waveform(to_qlist(b.as_ref())));
                    }
                }
                2 => {
                    if let Some(b) = cells.energy.lock().unwrap().take() {
                        viz_bridge::ui(move |mut v| v.as_mut().set_energy(to_qlist(&b)));
                    }
                }
                _ => {
                    if let Some(b) = cells.bars.lock().unwrap().take() {
                        viz_bridge::ui(move |mut v| v.as_mut().set_bars(to_qlist(&b)));
                    }
                }
            }
            std::thread::sleep(DRAIN_INTERVAL);
        })
        .expect("spawn viz drain thread")
        .thread()
        .clone();

    let _ = HANDLES.set(VizHandles {
        tap,
        fft_thread,
        drain_thread,
    });
    log::info!("[qbz-qt][viz] producer + 30fps drain installed (idle until enabled)");
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
        // out the producer's 200ms IDLE_POLL.
        h.fft_thread.unpark();
        h.drain_thread.unpark();
    }
}

/// Point the drain at the stream the band is actually rendering. Called at
/// install time (from the persisted pref) and on every mode cycle.
pub fn set_mode(mode: i32) {
    ACTIVE_MODE.store(mode.clamp(0, 2), Ordering::Relaxed);
}

/// Mirror the transport onto the tap so a paused player parks the producer.
/// Called from the playback path on every playing-state flip (the Slint side
/// does this from playback.rs).
pub fn set_paused(paused: bool) {
    if let Some(h) = HANDLES.get() {
        h.tap.set_paused(paused);
        if !paused && ENABLED.load(Ordering::Relaxed) {
            h.fft_thread.unpark();
        }
    }
}
