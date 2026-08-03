// ImmersiveHeader — the immersive chrome band (ImmersiveView.slint:470-1184,
// 1618-1685), mounted by ImmersiveView at x=24 / y=16 / height=36 /
// width=parent-48 (§5.2 of the 2026-08-02 immersive-port contract).
//
// Three zones:
//   LEFT    — the view-menu trigger (44x36 glass) + a 256px QbzContextMenu
//             popup with the FOCUS and SPLIT groups in SLINT ORDER. Every row
//             fires QbzImmersive.setView(...); the data-panel rows also fire
//             their §5.5 entry load (Queue / Track Info / Suggestions —
//             Lyrics needs nothing: Qt fetches lyrics automatically per
//             track, §5.5).
//   CENTER  — the QbzLineEdit-based search field, absolutely centered,
//             min(band.width, 420) wide. The cortinilla itself ships in B5;
//             the Rust search invokables are the inert B1 surface until then
//             (that is CORRECT — do not implement them here).
//   RIGHT   — the window-controls capsule: a 44px circle (sliders glyph) that
//             expands LEFTWARD to 112px on hover (150ms ease-in-out), revealing
//             the fullscreen toggle (the FIRST FullScreen write in the Qt
//             frontend, §5.2) and the X exit.
//
// The whole band fades with the auto-hide chrome (ImmersiveView drives the
// opacity); nothing here knows about the hide timer.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"

Item {
    id: header

    // §5.4 text-input gate: the view root reads this to inert the seek
    // arrows while the search field has focus (D16 — the QbzLineEdit
    // `fieldActive` seam).
    readonly property bool searchActive: searchField.fieldActive

    // --- LEFT: the view menu ---------------------------------------------
    Rectangle {
        id: viewTrigger
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: 44
        height: 36
        radius: 18
        // Slint glass #00000080 / border #ffffff2e (RRGGBBAA), converted.
        color: "#80000000"
        border.width: 1
        border.color: "#2effffff"
        QbzIcon {
            name: "layout-grid"
            width: 16
            height: 16
            anchors.centerIn: parent
            tintName: "white"
        }
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: viewMenu.openBelowLeft(viewTrigger)
        }
    }

    QbzContextMenu {
        id: viewMenu
        menuWidth: 256
        contentSpacing: 1
        // §5.2 popup chrome: #0b0b0ef7, radius 16 (Slint RRGGBBAA converted).
        background: Rectangle {
            color: "#f70b0b0e"
            radius: 16
            border.width: 1
            border.color: "#2effffff"
        }

        // One row model entry: vm 0 = FOCUS (mode m), vm 1 = SPLIT (panel sp).
        // `entry` names the §5.5 entry load fired alongside setView:
        //   "queue"       -> QbzQueue.queuePanelOpened()
        //   "trackinfo"   -> QbzAlbum.openTrackInfo(QbzPlayer.npTrackId)
        //   "suggestions" -> QbzSuggestions.load(QbzPlayer.npTrackId) (B4)
        //   ""            -> none (Lyrics rows: Qt fetches lyrics
        //                    automatically per track, §5.5).
        function fireRow(row) {
            QbzImmersive.setView(
                row.vm === 0 ? 0 : 1,
                row.vm === 0 ? row.m : QbzImmersive.mode,
                row.vm === 0 ? QbzImmersive.splitPanel : row.sp)
            if (row.entry === "queue")
                QbzQueue.queuePanelOpened()
            else if (row.entry === "trackinfo")
                QbzAlbum.openTrackInfo(QbzPlayer.npTrackId)
            else if (row.entry === "suggestions")
                QbzSuggestions.load(QbzPlayer.npTrackId)
            viewMenu.close()
        }

        Component {
            id: menuRow
            Rectangle {
                required property var modelData
                readonly property bool active: modelData.vm === 0
                    ? (QbzImmersive.viewMode === 0 && QbzImmersive.mode === modelData.m)
                    : (QbzImmersive.viewMode === 1 && QbzImmersive.splitPanel === modelData.sp)
                width: parent ? parent.width : 0
                height: 33
                radius: 8
                // Active row #ffffff26 (Slint RRGGBBAA converted).
                color: active ? "#26ffffff"
                    : (rowArea.containsMouse ? "#14ffffff" : "transparent")
                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: 10
                    anchors.right: dot.left
                    anchors.rightMargin: 8
                    height: parent.height
                    text: modelData.label
                    color: "#f2ffffff"
                    font.pixelSize: 13
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
                // Trailing 7px active dot (:529-537).
                Rectangle {
                    id: dot
                    visible: parent.active
                    anchors.right: parent.right
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    width: 7
                    height: 7
                    radius: 3.5
                    color: "#ffffff"
                }
                MouseArea {
                    id: rowArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: viewMenu.fireRow(modelData)
                }
            }
        }

        // Group headers: 11px / 700 / #ffffff80 / letter-spacing 1.5 /
        // ALL-CAPS (:481-487). The msgids are already caps.
        Text {
            height: 26
            leftPadding: 10
            verticalAlignment: Text.AlignVCenter
            text: QbzSession.tr("FOCUS", QbzSession.trRev)
            color: "#80ffffff"
            font.pixelSize: 11
            font.weight: Font.Bold
            font.letterSpacing: 1.5
        }
        Repeater {
            // FOCUS rows IN SLINT ORDER (:586-650): Album Reactive(0),
            // Static(1), Coverflow(2), Spectrum(3), Wave Bed(6), Lyrics(4),
            // Queue(5). NO shader rows (owner ruling 1).
            model: [
                { "vm": 0, "m": 0, "sp": -1, "entry": "",
                  "label": QbzSession.tr("Album Reactive", QbzSession.trRev) },
                { "vm": 0, "m": 1, "sp": -1, "entry": "",
                  "label": QbzSession.tr("Static", QbzSession.trRev) },
                { "vm": 0, "m": 2, "sp": -1, "entry": "",
                  "label": QbzSession.tr("Coverflow", QbzSession.trRev) },
                { "vm": 0, "m": 3, "sp": -1, "entry": "",
                  "label": QbzSession.tr("Spectrum", QbzSession.trRev) },
                { "vm": 0, "m": 6, "sp": -1, "entry": "",
                  "label": QbzSession.tr("Wave Bed", QbzSession.trRev) },
                { "vm": 0, "m": 4, "sp": -1, "entry": "",
                  "label": QbzSession.tr("Lyrics", QbzSession.trRev) },
                { "vm": 0, "m": 5, "sp": -1, "entry": "queue",
                  "label": QbzSession.tr("Queue", QbzSession.trRev) },
            ]
            delegate: menuRow
        }
        Text {
            height: 26
            leftPadding: 10
            verticalAlignment: Text.AlignVCenter
            text: QbzSession.tr("SPLIT", QbzSession.trRev)
            color: "#80ffffff"
            font.pixelSize: 11
            font.weight: Font.Bold
            font.letterSpacing: 1.5
        }
        Repeater {
            // SPLIT rows (:741-780): Lyrics(0), Track Info(1),
            // Suggestions(2), Queue(3).
            model: [
                { "vm": 1, "m": -1, "sp": 0, "entry": "",
                  "label": QbzSession.tr("Lyrics", QbzSession.trRev) },
                { "vm": 1, "m": -1, "sp": 1, "entry": "trackinfo",
                  "label": QbzSession.tr("Track Info", QbzSession.trRev) },
                { "vm": 1, "m": -1, "sp": 2, "entry": "suggestions",
                  "label": QbzSession.tr("Suggestions", QbzSession.trRev) },
                { "vm": 1, "m": -1, "sp": 3, "entry": "queue",
                  "label": QbzSession.tr("Queue", QbzSession.trRev) },
            ]
            delegate: menuRow
        }
    }

    // --- CENTER: the search field -----------------------------------------
    // QbzLineEdit-based (§5.2). 36px tall, min(band.width, 420) wide,
    // absolutely centered in the band. The 220ms debounce + >=2-char gate
    // live in the Rust immersive controller (§3.4, B5) — the field reports
    // every keystroke.
    QbzLineEdit {
        id: searchField
        searchMode: true
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(header.width, 420)
        height: 36
        placeholder: QbzSession.tr("Search", QbzSession.trRev)
        // Two-way with the bridge (the B5 controller clears it on
        // activation); the B1 surface is inert until then — correct.
        text: QbzImmersive.searchInputText
        onEdited: function (value) { QbzImmersive.searchLive(value) }
        onAccepted: function (value) {
            QbzImmersive.searchRowActivated(QbzImmersive.immSearchSelectedIndex)
        }
        // Up/Down on the field ROOT (Qt handles them here; Slint rejects
        // them in the field because winit owns them — D15, same observable
        // behavior). Escape is NOT handled here: the inner input declines it
        // (QbzLineEdit non-expandable arm) and it propagates to the view
        // root, whose handler dismisses search first / exits otherwise
        // (§3.4).
        Keys.onPressed: function (event) {
            if (event.key === Qt.Key_Down) {
                QbzImmersive.searchMoveSelection(1)
                event.accepted = true
            } else if (event.key === Qt.Key_Up) {
                QbzImmersive.searchMoveSelection(-1)
                event.accepted = true
            }
        }
    }
    // "↵ Enter" hint, visible only while the cortinilla is open (:985-993) —
    // inert in B2 because immSearchOpen cannot flip true before B5.
    Text {
        visible: QbzImmersive.immSearchOpen
        anchors.left: searchField.right
        anchors.leftMargin: 10
        anchors.verticalCenter: searchField.verticalCenter
        text: "↵  " + QbzSession.tr("Enter", QbzSession.trRev)
        color: "#bfffffff"
        font.pixelSize: 12
    }

    // --- RIGHT: the window-controls capsule --------------------------------
    // Collapsed: 44px circle, sliders glyph. Hover expands it LEFTWARD to
    // 112px (150ms ease-in-out, anchored right) revealing the fullscreen
    // toggle and the X exit. Minimize/maximize/drag parked (Slint :1173-1183).
    Rectangle {
        id: capsule
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: 36
        width: capHover.containsMouse ? 112 : 44
        radius: 18
        color: "#80000000"
        border.width: 1
        border.color: "#2effffff"
        clip: true
        Behavior on width {
            NumberAnimation { duration: 150; easing.type: Easing.InOutQuad }
        }

        MouseArea {
            id: capHover
            anchors.fill: parent
            hoverEnabled: true
        }

        // Collapsed-state glyph (fades out as the capsule opens).
        QbzIcon {
            name: "sliders-horizontal"
            width: 16
            height: 16
            anchors.centerIn: parent
            tintName: "white"
            opacity: capsule.width < 100 ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 100 } }
        }

        // Fullscreen toggle (glyph swaps on state) + X exit — revealed by the
        // expansion, pinned to the capsule's right half.
        QbzIcon {
            id: fsGlyph
            name: (header.Window.window
                   && header.Window.window.visibility === Window.FullScreen)
                  ? "minimize-2" : "maximize-2"
            width: 15
            height: 15
            x: capsule.width - 72
            anchors.verticalCenter: parent.verticalCenter
            tintName: fsArea.containsMouse ? "white" : "muted"
            opacity: capsule.width > 100 ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 100 } }
            MouseArea {
                id: fsArea
                anchors.fill: parent
                anchors.margins: -9
                // Gated on the expansion: the two button MouseAreas are
                // INVISIBLE while collapsed but would still cover the 44px
                // circle (the -9 margins make the X's hit area span almost
                // all of it), so a click on the collapsed capsule exited
                // immersive instead of doing nothing.
                enabled: capsule.width > 100
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    var w = header.Window.window
                    if (!w)
                        return
                    w.visibility = w.visibility === Window.FullScreen
                        ? Window.Windowed : Window.FullScreen
                }
            }
        }
        QbzIcon {
            name: "x"
            width: 14
            height: 14
            x: capsule.width - 36
            anchors.verticalCenter: parent.verticalCenter
            tintName: xArea.containsMouse ? "white" : "muted"
            opacity: capsule.width > 100 ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 100 } }
            MouseArea {
                id: xArea
                anchors.fill: parent
                anchors.margins: -9
                enabled: capsule.width > 100
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                // Exit path 2 (§5.4) — every exit funnels through the bridge's
                // open setter.
                onClicked: QbzImmersive.open = false
            }
        }
    }
}
