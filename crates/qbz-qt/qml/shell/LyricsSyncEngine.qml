// Lyrics sync engine — the QML half of crates/qbz/src/lyrics_sync.rs.
//
// Slint runs this as a Rust `slint::Timer` that PUSHES active-index /
// line-progress into a global. cxx-qt has no UI-thread timer primitive and
// pumping 30 cross-thread hops a second would be strictly worse, so here the
// CADENCE lives in QML and the MATH lives in Rust: every tick pulls three
// scalar invokables off QbzLyrics (position, active index, karaoke fraction —
// the same `qbz_lyrics::sync` pure functions, over the same Rust-held parsed
// document, so native Qobuz wsync word stamps drive the fill).
//
// Cadence is ADAPTIVE, 1:1 with the Slint engine:
//   - ACTIVE 33ms (~30Hz) while the gate is open (surface shown + doc synced
//     + ready + effectively playing);
//   - IDLE 250ms otherwise — the tick then only re-checks the gate.
// Every tick recomputes from the ABSOLUTE position, which makes seeks
// inherently safe: `fillAnimMs` is zeroed across discontinuities (line change
// or backward move) so the fill first-paints exactly at the reported
// progress, and smoothed (45ms, just above the tick) while continuous.
// Writes are equality-guarded — an unchanged tick sets nothing, so an idle
// track drives zero repaints.
//
// `kick()` runs one immediate pass that IGNORES the playing gate (doc commit
// / panel open), so the ladder lands on the correct line even while paused.

import QtQuick
import com.blitzfc.qbz

Item {
    id: engine

    // Host gate: the surface is on screen AND the document is synced+ready.
    property bool gateOpen: false

    // ---- outputs (bind these; never poll them) ------------------------
    readonly property int activeIndex: engine._index
    readonly property real lineProgress: engine._progress
    readonly property int fillAnimMs: engine._animMs

    // Slint FILL_ANIM_MS — slightly above the tick so a late tick does not
    // stutter the sweep.
    readonly property int smoothAnimMs: 45

    property int _index: -1
    property real _progress: 0.0
    property int _animMs: 0
    // Whether the last pass saw playback running (drives the cadence).
    property bool _fast: false

    visible: false
    width: 0
    height: 0

    Timer {
        id: tick
        interval: engine._fast ? 33 : 250
        repeat: true
        running: engine.gateOpen
        onTriggered: engine.pump(true)
    }

    // Gate opening = panel opened mid-song: land on the right line at once.
    onGateOpenChanged: {
        if (gateOpen)
            engine.kick()
        else
            engine._fast = false
    }

    // Reset to the pre-first-line state — the host calls this on every doc
    // commit so the view's "initial centering is instant" rule applies again.
    function reset() {
        engine._index = -1
        engine._progress = 0.0
        engine._animMs = 0
        engine._fast = false
    }

    // One immediate pass that bypasses the playing gate.
    function kick() {
        engine.pump(false)
    }

    function pump(requirePlaying) {
        if (!engine.gateOpen) {
            engine._fast = false
            return
        }
        var playing = QbzLyrics.playing()
        if (requirePlaying && !playing) {
            // Pause freezes the ladder + fill in place (Slint parity: the
            // rAF gate stops, the position holds).
            engine._fast = false
            return
        }
        var pos = QbzLyrics.positionMs()
        var idx = QbzLyrics.activeIndexAt(pos)
        var frac = QbzLyrics.fillFractionAt(pos, idx)

        var indexChanged = idx !== engine._index
        var backward = frac < engine._progress - 0.0005
        // Discontinuity: paint EXACTLY at the reported progress, no smoothing
        // across it. Written FIRST so the row's Behavior picks up the new
        // duration before the progress write starts the animation.
        var anim = (indexChanged || backward) ? 0 : engine.smoothAnimMs
        if (engine._animMs !== anim)
            engine._animMs = anim
        if (indexChanged)
            engine._index = idx
        if (indexChanged || Math.abs(frac - engine._progress) >= 0.001)
            engine._progress = frac

        engine._fast = playing
    }
}
