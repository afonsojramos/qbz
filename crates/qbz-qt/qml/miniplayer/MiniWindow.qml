// MiniWindow — the miniplayer's top-level window (2026-08-03 miniplayer/tray
// contract A-27, §4.1), port of `crates/qbz-ui/ui/app.slint:282-305`.
//
// A SECOND top-level Window in the SAME QQmlApplicationEngine, created lazily
// by Main.qml. That route is what deletes the reference's whole mirror layer:
// Slint globals are per-component-tree, so `crates/qbz/src/miniplayer.rs`
// copies 39 NowPlayingState scalars on every 450 ms tick; a Qt #[qml_singleton]
// is one object per engine, so this window reads the SAME QbzPlayer / QbzQueue /
// QbzLyrics / QbzShell the shell reads. MEASURED, not assumed (contract §16
// U-1): the probe build showed one Text bound to QbzPlayer.npTitle following
// the main window across a track change with no mirror code at all.
//
// The window is TRANSPARENT and the visible card is inset by a 6 px gutter, so
// every geometry number in §4.2-§4.5 is relative to the CARD, not to the
// window (§15 trap 3).
//
// NOT ported: the reference's CREATING_MINI double-latch
// (miniplayer.rs:128-130, :166-169). It exists because a winit surface may be
// created lazily on set_size/show() and a global attributes hook has to see the
// flag; in Qt `flags` is declarative and applies at creation (§4.1).

import QtQuick
import QtQuick.Window
import com.blitzfc.qbz

Window {
    id: root

    // §15 trap 1, and it is the one that would bite hardest: a Window declared
    // inside another Window is auto-parented (QQuickWindowQmlImpl carries
    // `updateTransientParent`), and a transient child is tied to its parent's
    // mapping by compositor policy — while hiding the main window is the LAST
    // step of enter() (§4.6). Explicitly null, never inherited.
    transientParent: null

    // Borderless, 1:1 with `no-frame: true` (app.slint:293), plus always-on-top
    // — owner ruling K1, §13-D15, and it DEFAULTS ON.
    //
    // The AOT term is NEW WORK rather than a port: §4.8's two probes found
    // three comments in the reference promising the affordance
    // (state.slint:4497, miniplayer.rs:170-171, MiniWindowControls.slint:2) and
    // no code anywhere that sets it. The property is seeded from
    // `mini_always_on_top` at bridge construction and toggled by the capsule's
    // pin button.
    //
    // ⚠ THE ONE RUNTIME RISK, and it is §16 U-6: reassigning `flags` on a
    // MAPPED window can make Qt destroy and recreate the platform window, which
    // on Wayland loses the compositor-chosen position. §4.8 pre-specifies the
    // fallback rather than leaving it to be improvised — evaluate the flags at
    // enter() only and keep the pref and the button state — and the fallback is
    // NOT taken: driven over RFB, toggling the pin left the window's geometry,
    // its contents and its input focus untouched (no flicker, no jump, no
    // re-created surface). A real compositor is still the owner's smoke item;
    // the pref persists either way, so the next open honours it regardless.
    flags: Qt.Window | Qt.FramelessWindowHint
           | (QbzMini.alwaysOnTop ? Qt.WindowStaysOnTopHint : 0)

    // Literal, never translated (app.slint:283 uses a plain string, not @tr).
    title: "QBZ Mini Player"

    // Per-window alpha buffer: QQuickWindow::setColor requests it natively
    // (§3.1). The Slint side needs a vendored femtovg fork for the same effect.
    color: "transparent"

    // --- Geometry (§4.2). Every per-mode value is CACHED into a named
    // readonly property, never a ternary repeated per binding (§7-M2): this is
    // a permanently-visible surface and cost per frame scales with the number
    // of live bindings, not with area.
    //
    // Switching the surface RESIZES THE WINDOW and moves its min-height floor
    // with it (§15 trap 5) — these are plain bindings on QbzMini.surface, so
    // that happens by construction. A WM-driven resize writes width/height from
    // C++, which does not disturb a binding (the same property Main.qml:32-35
    // relies on).
    //
    // ⚠ OPEN, and it is an OWNER-SMOKE item, not a known-good: `height` and
    // `minimumHeight` are sibling bindings re-evaluated off the same
    // `surfaceId` change, and QML gives no guaranteed delivery ORDER between
    // them. On a SHRINK — artwork (540, floor 420) to compact (178, floor 170)
    // — a `height` that lands while the OLD floor is still in force is a
    // request qtwayland clamps to the current minimum, and Qt's later min/max
    // update only ever expands the current size. The card would stay stuck at
    // 420 px showing a 178 px layout.
    //
    // **It is left declarative on purpose.** The obvious fix — lower the floor
    // imperatively, then assign the size — was written and MEASURED to break
    // the resize outright on this platform: breaking the two bindings left the
    // window at its old height with the new surface's layout inside it. The
    // declarative form is what RFB shows working (540 -> 178 -> 540, measured).
    // RFB cannot settle the Wayland question either way: the VNC platform
    // enforces no size constraints at all, which is exactly why the shrink
    // measures clean there. If the owner's smoke shows the compact card
    // refusing to shrink, THAT is this note, and the fix needs a real
    // compositor to develop against.
    readonly property int surfaceId: QbzMini.surface
    readonly property bool expanded: root.surfaceId >= 2
    // The expanded size is the PERSISTED one, exposed by the bridge because
    // Rust cannot reach a QQuickWindow (A-6b). The reference reads the same two
    // prefs on every geometry apply (miniplayer.rs:404).
    readonly property int winWidth: root.expanded ? Math.max(QbzMini.miniWidth, 340) : 380
    readonly property int winHeight: root.expanded
                                     ? Math.max(QbzMini.miniHeight, 420)
                                     : (root.surfaceId === 0 ? 57 : 178)
    // The ONLY floor that moves. Min WIDTH is 340 in every surface
    // (miniplayer.rs:408-411).
    readonly property int winMinHeight: root.expanded ? 420 : (root.surfaceId === 0 ? 57 : 170)

    width: root.winWidth
    height: root.winHeight
    minimumWidth: 340
    minimumHeight: root.winMinHeight

    // --- The geometry persist (§4.7, A-14, §13-D14) ------------------------
    // Qt has no `WindowEvent::Resized`, so the resize handler is this debounce.
    // The template is the main window's own (qml/Main.qml:377-385), and the
    // 400 ms interval is the same.
    //
    // It is DEBOUNCED because the alternative is what the reference does: a
    // whole-file ui_prefs.json load + save per qualifying resize event, with no
    // dirty check (`crates/qbz/src/miniplayer.rs:460-463`), which in Qt would
    // be blocking file IO on the GUI thread inside a resize drag (§7-M10).
    //
    // It needs NO arming latch of its own, unlike Main.qml's 1200 ms one: the
    // two guards inside `mini_qt::geometry_to_persist` already reject exactly
    // the frames a latch would — `surface >= 2` drops micro and compact, whose
    // sizes are constants, and `height >= 320` drops the mid-transition frames
    // a surface switch produces (a 178 px compact frame is precisely what that
    // filter is for). Note the filter is 320 and the stored floor is 420 — two
    // different numbers, both behavioural (§15 trap 17).
    Timer {
        id: geometrySaveTimer
        interval: 400
        repeat: false
        // A Timer has no `parent` (§8 rule 2, §15 trap 29) — the window is
        // reached through its own id.
        onTriggered: QbzMini.saveGeometry(root.width, root.height)
    }

    onWidthChanged: geometrySaveTimer.restart()
    onHeightChanged: geometrySaveTimer.restart()

    // A compositor close (Alt+F4, a panel's close entry) behaves EXACTLY like
    // the capsule's expand button — §13-D13, and it closes a hole the reference
    // has: Slint has no on_close_requested on the mini, so its default
    // HideWindow leaves OPEN true, the main window hidden and the app alive but
    // invisible. Accepting the event is also what would arm Qt's quit check
    // (§3.1), so `accepted = false` is load-bearing twice.
    onClosing: function (close) {
        close.accepted = false
        // Flush a pending resize (§4.7). Only when one is actually pending —
        // an unconditional call would put a prefs write on every close, and the
        // bridge's own dirty check would then be the only thing stopping it.
        if (geometrySaveTimer.running) {
            geometrySaveTimer.stop()
            QbzMini.saveGeometry(root.width, root.height)
        }
        QbzMini.exit()
    }

    // --- The key host (§4.9, A-15) ----------------------------------------
    // BOTH halves are required. Qt delivers key events only to activeFocusItem
    // and propagates up its parent chain, so a tree with no focus item receives
    // nothing; and `focus: true` ALONE did not survive in a WM-less (VNC)
    // session — measured on AppShell (qml/shell/AppShell.qml:75-81), where the
    // dispatcher got nothing until the first click. The idiom is that file's
    // :72 / :81 / :91-97.
    //
    // NOTHING is handled locally: the ordered table lives in Rust behind
    // QbzMini.keyPressed, which resolves through hotkeys_qt::action_for_mini_key
    // (A-16) over the user's ACTIVE bindings. No text-input gate is computed
    // here because the mini has no text fields, exactly as the reference's
    // dispatch_mini has no such guard (crates/qbz/src/keybindings.rs:623-648).
    Item {
        id: keyHost
        anchors.fill: parent
        focus: true
        Component.onCompleted: keyHost.forceActiveFocus()
        Keys.onPressed: function (event) {
            event.accepted = QbzMini.keyPressed(event.key, event.modifiers, event.text)
        }

        // The card: window MINUS 12 px in both axes, a 6 px transparent gutter
        // (app.slint:299-304). §15 trap 3.
        MiniShell {
            id: card
            x: 6
            y: 6
            width: root.width - 12
            height: root.height - 12
            // §8 rule 3: children reach the window through THIS explicit
            // reference, never through parent.parent — a chain qmllint cannot
            // verify. Same seam as Main.qml -> AppShell.hostWindow. The footer's
            // drag handle and the capsule (B3) are its consumers.
            hostWindow: root
        }
    }
}
