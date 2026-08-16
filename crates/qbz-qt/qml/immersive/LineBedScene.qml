// Line Bed (immersive shader scene, mode 5) — the QML wrapper for the C++
// `LineBedItem` (cxx/linebed_item.*, the project's first QQuickRhiItem).
// Block A4 of the 2026-08-15 immersive-completion contract (spec 01 §2.5).
//
// Its own file (not an inline Component in ShaderSceneLayer.qml) so the RHI
// item's contract is one screen of QML — and so the C++ type name only has
// to resolve when this file compiles, which the tier gate already ensures
// never happens where the RHI can't run it (the Loader in
// ShaderSceneLayer.qml activates only while `root.active && scene === 5`).
//
// THE PULSE LAW: the item repaints ONLY from `pulseTick()`, which
// ShaderSceneLayer calls on the shared shell pulse (QbzShell.pulseMs) with
// the ring publish it stashed on the viz tick. `heights` has no NOTIFY and
// nothing binds it; the palette colors ride QbzShell's ambient triad (the
// same ambient_qt source every other scene uses — they change on track
// change, not per tick).

import QtQuick
import QtQuick.Window
import com.blitzfc.qbz

LineBedItem {
    id: root

    // The empirical orientation matrix (2026-08-15, BOTH verified today):
    //   owner's Vulkan + mirror=false  -> bed UPSIDE-DOWN
    //   Xvfb Mesa GL  + mirror=true    -> bed UPSIDE-DOWN (peaks hang down)
    // So the correct rule is mirror on everything EXCEPT OpenGL — my first
    // guess (GL-only) had it exactly backwards, and "unconditional" broke GL.
    mirrorVertically: GraphicsInfo.api !== GraphicsInfo.OpenGL

    // The ambient palette triad (shell_bridge.rs; pushed per track by
    // playback_qt.rs). The Slint reference defaults #00dcc8 / #3fd9c8 live
    // in the C++ member initializers, so a pre-art track still looks right.
    primary: QbzShell.ambientPrimary
    accent: QbzShell.ambientAccent

    // Called by ShaderSceneLayer on the pulse edge ONLY. `h` is the stashed
    // 256x200 f32 ring (QbzShaderScene.linebedHeights, raw bytes) or null
    // when the viz drain published nothing new (player paused); the repaint
    // still runs so the item tracks its parent's resizes/palette changes at
    // the same cadence the sibling ShaderEffect scenes tick their clocks.
    function pulseTick(h) {
        if (h !== null)
            heights = h
        update()
    }
}
