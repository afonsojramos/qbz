// Plasma (immersive shader scene, mode 1) — the QML wrapper for the C++
// `PlasmaItem` (cxx/plasma_item.*, the feedback-fluid QQuickRhiItem).
// Block A2 of the 2026-08-15 immersive-completion contract (spec 01 §2.1).
//
// Its own file (the LineBedScene.qml idiom) so the RHI item's contract is
// one screen of QML — and so the C++ type name only has to resolve when
// this file compiles, which the tier gate already ensures never happens
// where the RHI can't run it (the Loader in ShaderSceneLayer.qml activates
// only while `root.active && scene === 1`).
//
// THE PULSE LAW: the item repaints ONLY from `pulseTick()`, which
// ShaderSceneLayer calls on the shared shell pulse (QbzShell.pulseMs) with
// the applied pack it stashed on the viz tick. The properties have no
// NOTIFY and nothing binds them; the palette colors ride QbzShell's ambient
// triad (they change on track change, not per tick).

import QtQuick
import QtQuick.Window
import com.blitzfc.qbz

PlasmaItem {
    id: root

    // mirrorVertically rule (2026-08-15, verified BOTH ways today): mirror
    // on Vulkan/Metal/D3D, NOT on OpenGL — the empirical matrix lives in
    // LineBedScene.qml (owner's Vulkan + no mirror = flipped; Mesa GL +
    // mirror = flipped). The feedback passes are self-consistent in texture
    // space; this only affects the composite.
    mirrorVertically: GraphicsInfo.api !== GraphicsInfo.OpenGL

    // Called by ShaderSceneLayer on the pulse edge ONLY. `p` carries the
    // applied pack values + the palette + the local scene clock; assigning
    // the MEMBER properties and repainting ONCE per pulse is the whole
    // cadence contract (the sibling ShaderEffect scenes tick their clocks
    // at the same edge).
    function pulseTick(p) {
        time = p.time
        beat = p.beat
        level = p.level
        levelSmooth = p.levelSmooth
        energyLo = p.energyLo
        energyHi = p.energyHi
        primary = p.primary
        secondary = p.secondary
        accent = p.accent
        update()
    }
}
