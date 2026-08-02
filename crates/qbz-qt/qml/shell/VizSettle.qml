// VizSettle — ONE shared inter-frame interpolator for a whole visualizer stream.
//
// WHY THIS EXISTS. The FFT producer in qbz-audio publishes at TARGET_FPS = 30
// and the drain is signal-driven (one publish per produced frame), so the bar
// heights STEP 30 times a second while the compositor draws 60+. The step is
// what reads as "low frame rate" — not missing data (see the bandwidth note
// below). This object replays each 30 Hz frame across the render frames that
// follow it, so the bars move at the display rate off 30 Hz data.
//
// PARITY: the Slint dock does the same thing per bar —
// crates/qbz-ui/ui/shell/SidebarNowPlayingDock.slint:45
//   `animate bar-h { duration: 90ms; easing: ease; }`   (VBar)
//   `animate col-h { duration: 70ms; easing: ease; }`   (WaveCol)
// The QML port dropped that as "28 concurrent animations would buy nothing";
// what it actually bought was the smooth motion. This restores the behaviour
// with ONE driver for the whole stream instead of one animation per bar.
//
// WHY NOT A HIGHER PRODUCER RATE. qbz-audio smooths the bars in Rust with a
// per-frame EMA (attack 0.7 / decay 0.65, processor.rs:174-184). At 30 fps the
// decay time constant is 33.3ms / -ln(0.65) = 77ms → -3dB at 2.1Hz; the fast
// attack is 28ms → 5.8Hz. The published signal therefore carries nothing above
// ~6Hz, and 30 Hz sampling is already >5x oversampled: doubling TARGET_FPS adds
// no information, it only halves those time constants (a re-tune, not a frame
// rate) and doubles the FFT cost — for BOTH frontends, since the constant is
// shared. Interpolating is the correct fix for a step artefact.
//
// COST CONTRACT.
//  - Zero allocation per rendered frame: `from` is a REFERENCE SWAP and only
//    `progress` changes per tick. `to` is materialised into a plain JS array
//    ONCE per publish (30/s) so `at()` does not cross into the QList_f32
//    sequence wrapper twice per bar per tick — see `onTargetChanged`.
//  - The DRIVER IS A 16 ms Timer, not a FrameAnimation. The producer publishes
//    at 30/s; a per-frame trigger fired 180x/s on Linux (100x/s on macOS) for
//    it. ~62 Hz keeps two interpolated steps per published frame, which is all
//    the motion there is to buy.
//  - SELF-PARKING, exactly like the Rust drain: the driver starts on a publish
//    and stops itself the moment `progress` reaches 1 (within one 33 ms period
//    of the last publish). A paused player, a hidden band or a disabled tap
//    publishes nothing, so nothing restarts it. A Timer does NOT stop itself
//    when the window renders nothing the way a FrameAnimation did, so the
//    `onLiveChanged` gate below — fed by the dock's `vizShouldRun`, which drops
//    when the window deactivates — is what keeps an occluded window at zero
//    wakeups. Do not weaken it.
//  - Delegates bind ONE call, `at(i)`, which reads `from`, `to` and `progress`
//    unconditionally on every path — QML captures those three as dependencies,
//    so a `progress` change re-evaluates the bar heights and instantiates
//    nothing.
//
// LATENCY: the band renders between the last two published frames, i.e. one
// producer period (33ms) behind the tap. Against the pipeline that already
// exists (46ms FFT window + 77ms smoothing) that is not perceptible, and it is
// what buys motion with no overshoot and no extra damping.

import QtQuick

Item {
    id: settle

    // ---- input -------------------------------------------------------------
    // Newest published frame, DISPLAY-READY (0..1). Replaced ~30x/s. Whatever
    // per-sample mapping a mode needs (abs, downsample) belongs in the binding
    // that feeds this, so it runs once per publish instead of once per bar per
    // frame.
    property var target: []
    // Master gate — mirror the band's capture gate. false freezes on the last
    // frame and guarantees the driver is stopped.
    property bool live: true

    // ---- output (read via at(i)) -------------------------------------------
    property var from: []
    property var to: []
    property real progress: 1.0

    // Publish period estimate, ms. Seeded at 1000/TARGET_FPS and tracked with a
    // slow EMA so a producer that drifts off 30 Hz still lands on time.
    property real periodMs: 33.3
    property real lastTickMs: 0

    visible: false
    width: 0
    height: 0

    // Interpolated, clamped amplitude for bar `i`. Reads all three outputs on
    // every path — do NOT add an early return, it would drop the dependency.
    function at(i) {
        var a = settle.from[i] || 0;
        var b = settle.to[i] || 0;
        var v = a + (b - a) * settle.progress;
        return v < 0 ? 0 : (v > 1 ? 1 : v);
    }

    onTargetChanged: {
        if (!settle.live) {
            settle.from = settle.target;
            settle.to = settle.target;
            settle.progress = 1.0;
            return;
        }
        var now = Date.now();
        var dt = now - settle.lastTickMs;
        // Ignore the resume/first-frame gap and any absurd outlier; 8..200ms is
        // 5..125 fps, everything a sane producer can emit.
        if (dt > 8 && dt < 200)
            settle.periodMs = settle.periodMs * 0.8 + dt * 0.2;
        // Plain-JS copy, ONCE per publish (30/s), so `at()` indexes a JS array
        // instead of crossing into the QList_f32 sequence wrapper 2x per bar
        // per tick (56 crossings for bars). `from = to` stays a reference
        // swap — no copy, no allocation per rendered frame (the COST CONTRACT
        // above).
        //
        // The waveform arm ALREADY hands over a plain Array (it folds |sample|
        // in its own binding, SpectrumBand.qml:108-117), so skip the copy
        // there instead of paying a second 48-element pass per publish.
        // `t ? … : 0` is not defensive padding: the pre-fix body was a bare
        // `settle.to = settle.target` and never dereferenced `target`, so the
        // copy below is the FIRST deref in this file. All three producers
        // (QbzViz.bars / .energy sequences, the waveform arm's own Array)
        // always hand over a value, but a future fourth arm that publishes
        // undefined would now throw inside a 30/s handler instead of degrading
        // to a hold — which is the contract stated in the header.
        var t = settle.target;
        var arr;
        if (Array.isArray(t)) {
            arr = t;
        } else {
            var n = t ? t.length : 0;
            arr = new Array(n);
            for (var i = 0; i < n; ++i)
                arr[i] = t[i];
        }
        settle.from = settle.to;
        settle.to = arr;
        settle.lastTickMs = now;
        settle.progress = 0.0;
        if (!driver.running)
            driver.running = true;
    }

    onLiveChanged: {
        if (!settle.live) {
            driver.running = false;
            settle.progress = 1.0;
        }
    }

    // Wall clock, not a frame-driven elapsedTime: the time source must not
    // quantise the segment start to vsync (that is the beat this object
    // removes). The TRIGGER, however, must not run at the frame rate either —
    // the producer publishes at TARGET_FPS = 30 (qbz-audio visualizer/mod.rs:32)
    // and FrameAnimation fired 180x/s on Linux and 100x/s on macOS for it,
    // i.e. 3-6x the frames the data justifies. 16 ms = ~62 Hz gives two
    // interpolated steps per published frame, which is what buys the motion
    // (Slint's equivalent is `animate bar-h { duration: 90ms; easing: ease; }`,
    // SidebarNowPlayingDock.slint:45).
    //
    // `repeat: true` is MANDATORY — Timer defaults to false, and without it the
    // driver fires once per publish and `progress` freezes at ~0.5.
    // `triggeredOnStart` stays default false: `running` is only set from
    // `onTargetChanged`, which has just written `progress = 0.0`, so an
    // immediate tick would recompute p≈0 and do nothing.
    // Coarse timer resolution is irrelevant: `progress` is derived from
    // Date.now(), so a late tick lands a correspondingly larger `p` and a
    // missed tick degrades to a hold, never a jump.
    Timer {
        id: driver
        interval: 16
        repeat: true
        running: false
        onTriggered: {
            var p = (Date.now() - settle.lastTickMs) / settle.periodMs;
            if (p >= 1.0) {
                settle.progress = 1.0;
                driver.running = false; // park until the next publish
            } else {
                settle.progress = p;
            }
        }
    }
}
