// Search "cortinilla" — the live as-you-type dropdown under the header
// search box (search/Cortinilla.slint, phase 15). A plain Item overlay
// mounted as a LAST child of AppShell (NOT a Popup — a Popup grabs the
// pointer and would kill typing in the header field, the exact reason the
// Slint uses a plain Rectangle too).
//
// Payload: QbzSearch.cortinillaJson (search_qt.rs CortinillaData: query /
// top / sections with controller flat indices). Keyboard selection rides
// selectedIndex + cortinillaScrollY (content-space top-y of the selected
// row — scrolled into view here). Row activation bubbles to the
// QbzSearch.cortinilla* invokables; the field clear goes through
// root.header.clearSearch().

import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import com.blitzfc.qbz
import "../theme"

Item {
    id: root
    anchors.fill: parent
    // Open AND the live query long enough to have meaningful results — the
    // same >= 2 gate the controller applies, so a 1-char query never flashes
    // a half-empty panel (Cortinilla.slint:190-191). cortinillaQuery is
    // published synchronously per keystroke, so this tracks what the user
    // has typed rather than what last finished loading.
    visible: QbzSearch.cortinillaOpen && QbzSearch.cortinillaQuery.length >= 2

    // The host HeaderBar (for clearSearch on row activation).
    property var headerBar: null

    QbzTheme { id: theme }

    // The document is applied IMPERATIVELY, not bound, so the scroll offset
    // survives a republish. `cortinillaJson` is republished wholesale when
    // the artwork pass lands (~1.5s after the rows), and a bound Repeater
    // model would be a full model reset: every delegate destroyed and
    // rebuilt, contentHeight momentarily 0, and StopAtBounds clamping
    // contentY to 0 — the user is yanked back to the top mid-read. This is
    // the port's own documented cure (MyQbzGridView.applyItems).
    property var doc: ({})
    property var sections: []
    readonly property bool hasTop: doc.top !== undefined && doc.top !== null
    readonly property bool empty: !hasTop && sections.length === 0 && !QbzSearch.cortinillaLoading

    function parseDoc() {
        try {
            return JSON.parse(QbzSearch.cortinillaJson)
        } catch (e) {
            return ({})
        }
    }

    property real _restoreY: 0
    function applyDoc() {
        root._restoreY = bodyFlick.contentY
        root.doc = parseDoc()
        root.sections = root.doc.sections || []
        // NOT forceLayout(): that is a QQuickItemView method (ListView /
        // GridView), and this is a plain Flickable + Column. The Column
        // re-layouts on the polish pass, so contentHeight is still the OLD
        // value here and clamping against it would be wrong. Restore once
        // layout has settled instead; Qt.callLater coalesces, so a burst of
        // republishes costs one restore.
        Qt.callLater(root._restoreScroll)
    }
    function _restoreScroll() {
        var maxY = Math.max(0, bodyFlick.contentHeight - bodyFlick.height)
        bodyFlick.contentY = Math.max(0, Math.min(root._restoreY, maxY))
    }

    Component.onCompleted: root.applyDoc()

    // The centered header search box width (HeaderBar's responsive rule) —
    // the click-through gaps below leave it free.
    // Must track HeaderBar's own rule EXACTLY (HeaderBar.qml:642), including
    // the 60px the field gives up when the section nav sits in the header —
    // the scrims below leave this span free, and a span 60px too wide leaves
    // a 30px dead strip on each side of the field that neither dismisses the
    // panel nor reaches the input.
    readonly property int searchBoxWidth:
        (root.width < 960 ? 179 : 256) - (QbzShell.navInSidebar ? 0 : 60)
    readonly property int panelWidth: 320
    // Cap so the panel NEVER covers the now-playing bar
    // (Cortinilla.slint:263-275). The bar term is NPB-MODE AWARE: mode 2 is
    // the small bar, every other mode is the large one. A hardcoded 42 here
    // let the panel run up to 70px into a 112px bar — latent while the
    // Qobuz-only panel held <= 14 rows, reachable the moment the local
    // sections make it scroll.
    readonly property int npbHeight:
        QbzShell.npbMode === 2 ? theme.npbSmallHeight : theme.npbLargeHeight
    readonly property int scrollCap:
        root.height - theme.headerHeight - 6 - root.npbHeight - 16 - 12

    // --- Idle auto-close (4.5s without hover/activity, Cortinilla.slint) --
    // panelHovered can LATCH: if the panel goes invisible while the pointer
    // is over it, the MouseArea never reports containsMouse=false, so the
    // idle timer stays paused forever on the next open. The reference resets
    // it on every open/close for exactly this reason.
    property bool panelHovered: false
    onVisibleChanged: root.panelHovered = false
    // OWNER DIVERGENCE from the reference (2026-08-03): hovering the panel
    // must not let it close under the cursor, but a cursor left there BY
    // ACCIDENT must not hold it open forever either. So hover does not stop
    // the countdown, it stretches it to 30s. The reference pauses the timer
    // outright, which fails the second half.
    //
    // Assigning `interval` on a running Timer restarts it, so entering the
    // panel gives a full 30s and leaving it gives a full 4.5s — which is
    // exactly the wanted behaviour, not an accident.
    Timer {
        id: idleClose
        interval: root.panelHovered ? 30000 : 4500
        repeat: false
        running: root.visible
        onTriggered: QbzSearch.cortinillaDismiss()
    }
    Connections {
        target: QbzSearch
        // Activity restarts the countdown (keystroke or arrow move). The
        // keystroke probe is the QUERY, not the payload: the reference
        // restarts per keystroke, and now that the debounce lives in Rust a
        // burst of typing publishes one payload but many queries.
        function onCortinillaJsonChanged() { root.applyDoc() }
        function onCortinillaQueryChanged() { if (root.visible && !root.panelHovered) idleClose.restart() }
        function onCortinillaSelectedIndexChanged() { if (root.visible && !root.panelHovered) idleClose.restart() }
        // Keyboard scroll-into-view: the controller publishes the selected
        // row's content-space top-y; nudge the Flickable so it is visible.
        function onCortinillaScrollYChanged() {
            var y = QbzSearch.cortinillaScrollY
            if (y < bodyFlick.contentY) {
                bodyFlick.contentY = Math.max(0, y)
            } else if (y + 56 > bodyFlick.contentY + bodyFlick.height) {
                bodyFlick.contentY = y + 56 - bodyFlick.height
            }
        }
    }

    Connections {
        target: QbzShell
        // A page change dismisses (AppShell's tracked-view close hook).
        function onCurrentViewChanged() { QbzSearch.cortinillaDismiss() }
    }

    // --- Click-outside scrims (any click outside the panel dismisses) ----
    // 1) everything BELOW the header.
    MouseArea {
        visible: root.visible
        x: 0
        y: theme.headerHeight
        width: root.width
        height: root.height - theme.headerHeight
        onClicked: QbzSearch.cortinillaDismiss()
    }
    // 2) header strip LEFT of the search box.
    MouseArea {
        visible: root.visible
        x: 0
        y: 0
        width: (root.width - root.searchBoxWidth) / 2
        height: theme.headerHeight
        onClicked: QbzSearch.cortinillaDismiss()
    }
    // 3) header strip RIGHT of the search box.
    MouseArea {
        visible: root.visible
        x: (root.width + root.searchBoxWidth) / 2
        y: 0
        width: root.width - x
        height: theme.headerHeight
        onClicked: QbzSearch.cortinillaDismiss()
    }

    // --- The panel ----------------------------------------------------------
    // Drop shadow (Cortinilla.slint:314-316: blur 24, offset-y 6,
    // Theme.card-shadow). Same idiom as the immersive twin
    // (ImmersiveSearchCortinilla.qml): a mirror Rectangle behind the panel,
    // blurred by MultiEffect, skipped when there are no shaders. This port
    // runs on the GPU (OpenGL RHI, measured), but the offscreen smoke forces
    // software, so the guard is real rather than defensive.
    //
    // NOTE: NavFlyout.qml still has no shadow and says so; adding one there
    // is its own parity pass, not this contract's. The two dropdowns will
    // look slightly different until that lands.
    readonly property bool noShaders: GraphicsInfo.api === GraphicsInfo.Software
        || GraphicsInfo.api === GraphicsInfo.Null
    Rectangle {
        visible: root.visible && !root.noShaders
        x: panel.x
        y: panel.y + 6
        width: panel.width
        height: panel.height
        radius: panel.radius
        color: theme.cardShadow
        layer.enabled: true
        layer.effect: MultiEffect {
            blurEnabled: true
            blurMax: 48
            blur: 0.5 // 24/48
        }
    }
    Rectangle {
        id: panel
        x: (root.width - root.panelWidth) / 2
        y: theme.headerHeight + 6
        width: root.panelWidth
        height: Math.min(bodyColumn.height + 12, root.scrollCap + 12)
        radius: theme.radiusSm
        color: theme.surfaceMain
        border.width: 1
        border.color: theme.borderMuted
        clip: true

        // Hover detection for the idle stretch. A HoverHandler, NOT the
        // MouseArea this replaces: that MouseArea was declared first, so the
        // Flickable and every row's own MouseArea sat ON TOP of it and took
        // the hover events, and `containsMouse` stayed false for most of the
        // panel's surface — which is why hovering did not hold the dropdown
        // open. A HoverHandler reports for its item's whole area regardless
        // of what is stacked above it.
        HoverHandler {
            id: panelHover
            onHoveredChanged: root.panelHovered = hovered
        }
        // Swallow clicks that land on the panel background so they do not
        // fall through to the dismiss scrims underneath.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onClicked: {}
        }

        Flickable {
            id: bodyFlick
            anchors.fill: parent
            anchors.topMargin: 6
            anchors.bottomMargin: 6
            contentWidth: width
            contentHeight: bodyColumn.height
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: bodyColumn
                width: bodyFlick.width

                // --- Loading skeleton (no cached instant-paint — one clean
                // apply, Cortinilla.slint) -------------------------------
                Column {
                    id: skeleton
                    visible: QbzSearch.cortinillaLoading
                    width: parent.width
                    topPadding: 4
                    property bool pulse: false
                    // ONE host Timer flipping ONE bool for all five rows —
                    // never a per-delegate animation. Under reduce-motion it
                    // drops to a ~8fps coarse tick, which is what the
                    // reference switches its pulse's time source to
                    // (Cortinilla.slint:128-131). That file records why: this
                    // pulse was the main cost of opening the cortinilla on
                    // weak GPUs, because every frame was a full-window
                    // repaint.
                    Timer {
                        interval: QbzShell.reduceMotion ? 2000 : 700
                        running: skeleton.visible
                        repeat: true
                        onTriggered: skeleton.pulse = !skeleton.pulse
                    }
                    Repeater {
                        model: 5
                        delegate: Item {
                            width: bodyColumn.width
                            height: 56
                            Rectangle {
                                x: 10
                                anchors.verticalCenter: parent.verticalCenter
                                width: 40
                                height: 40
                                radius: 4
                                color: theme.surfaceElevated
                                opacity: skeleton.pulse ? 0.48 : 0.16
                                Behavior on opacity { NumberAnimation { duration: 650; easing.type: Easing.InOutQuad } }
                            }
                            Column {
                                x: 62
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 8
                                Rectangle {
                                    width: 150
                                    height: 11
                                    radius: 3
                                    color: theme.surfaceElevated
                                    opacity: skeleton.pulse ? 0.48 : 0.16
                                    Behavior on opacity { NumberAnimation { duration: 650; easing.type: Easing.InOutQuad } }
                                }
                                Rectangle {
                                    width: 96
                                    height: 9
                                    radius: 3
                                    color: theme.surfaceElevated
                                    opacity: skeleton.pulse ? 0.48 : 0.16
                                    Behavior on opacity { NumberAnimation { duration: 650; easing.type: Easing.InOutQuad } }
                                }
                            }
                        }
                    }
                }

                // --- Loaded content ----------------------------------------
                Column {
                    visible: !QbzSearch.cortinillaLoading
                    width: parent.width

                    // No results.
                    Item {
                        visible: root.empty
                        width: parent.width
                        height: 28
                        Text {
                            x: 14
                            anchors.verticalCenter: parent.verticalCenter
                            // The LIVE query, matching the reference
                            // (Cortinilla.slint:373 reads
                            // SearchState.cortinilla-query, not the payload's
                            // echo) — otherwise the empty state quotes the
                            // previous query while the user reads it.
                            text: QbzSession.tr("No results for", QbzSession.trRev) + " “" + QbzSearch.cortinillaQuery + "”"
                            color: theme.textMuted
                            font.pixelSize: 12
                        }
                    }

                    // Top result (flat-index 0).
                    Column {
                        visible: root.hasTop
                        width: parent.width
                        leftPadding: 6
                        rightPadding: 6
                        topPadding: 4
                        Text {
                            x: 10
                            height: 22
                            text: QbzSession.tr("Top result", QbzSession.trRev)
                            color: theme.textMuted
                            font.pixelSize: 11
                            font.weight: theme.weightSemibold
                            verticalAlignment: Text.AlignVCenter
                        }
                        CortRow { row: root.hasTop ? root.doc.top : ({}) }
                    }

                    // Sections.
                    Repeater {
                        model: root.sections
                        delegate: Column {
                            id: sectionCol
                            required property var modelData
                            // Named alias: the inner Repeater below also has
                            // a `modelData`, and which one an unqualified
                            // reference resolves to depends on whether the
                            // inner delegate declares required properties.
                            // It happens to work today; one added `required`
                            // would silently flip every row to render its
                            // SECTION instead. Both are named explicitly now.
                            readonly property var section: modelData
                            width: bodyColumn.width
                            leftPadding: 6
                            rightPadding: 6
                            topPadding: 4

                            Item {
                                width: parent.width
                                height: 24
                                Text {
                                    anchors.left: parent.left
                                    anchors.leftMargin: 10
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.title
                                    color: theme.textMuted
                                    font.pixelSize: 11
                                    font.weight: theme.weightSemibold
                                }
                                Text {
                                    visible: modelData.hasMore === true
                                    anchors.right: parent.right
                                    anchors.rightMargin: 10
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: QbzSession.tr("View more", QbzSession.trRev)
                                    color: vmArea.containsMouse ? theme.textPrimary : theme.accent
                                    font.pixelSize: 11
                                    font.weight: theme.weightMedium
                                    MouseArea {
                                        id: vmArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: {
                                            if (root.headerBar) root.headerBar.clearSearch()
                                            QbzSearch.cortinillaViewMore(modelData.kind)
                                        }
                                    }
                                }
                            }
                            Repeater {
                                model: sectionCol.section.rows
                                delegate: CortRow {
                                    required property var modelData
                                    row: modelData
                                }
                            }
                        }
                    }
                }
            }
        }

        QbzScrollBar {
            target: bodyFlick
            // 10px gutter, not the primitive's default 14 — the reference
            // overrides it the same way (Cortinilla.slint:471).
            width: 10
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom
        }
    }

    // --- One result row (CortinillaResultRow) ------------------------------
    component CortRow: Rectangle {
        property var row: ({})
        // A QML Column's padding does NOT size its children, so `parent.width`
        // made every row 12px wider than its block. clip hid the overflow, but
        // the highlight ran flush to the right edge while staying inset 6px on
        // the left — asymmetric. Subtract the padding explicitly.
        width: parent ? parent.width - (parent.leftPadding || 0) - (parent.rightPadding || 0) : 0
        height: 56
        radius: theme.radiusSm
        readonly property bool active: QbzSearch.cortinillaSelectedIndex === (row.flatIndex ?? -2)
        color: (active || rowArea.containsMouse) ? theme.surfaceHover : "transparent"

        // Accent bar on the keyboard-active row (distinct from plain hover).
        Rectangle {
            visible: parent.active
            x: 0
            y: 4
            width: 3
            height: parent.height - 8
            radius: 2
            color: theme.accent
        }
        Rectangle {
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            width: 40
            height: 40
            // Artists read better as a circle; everything else is a tile.
            radius: row.kind === "artist" ? 20 : 4
            color: theme.surfaceElevated
            clip: true
            RoundedImage {
                anchors.fill: parent
                source: row.artPath || ""
                radius: row.kind === "artist" ? 20 : 4
            }
        }
        Column {
            x: 62
            width: parent.width - 62 - 10
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                text: row.title || ""
                color: theme.textPrimary
                font.pixelSize: 14
                font.weight: theme.weightMedium
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: row.subtitle || ""
                color: theme.textMuted
                font.pixelSize: 12
                elide: Text.ElideRight
            }
        }
        MouseArea {
            id: rowArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                // Guard the i32 boundary: an undefined flatIndex coerces to
                // 0 on the way into Rust, which is the TOP RESULT's index —
                // a malformed row would activate the wrong thing instead of
                // doing nothing. The highlight above already guards with
                // `?? -2`; this is the same guard on the action.
                var fi = row.flatIndex
                if (fi === undefined || fi === null) return
                if (root.headerBar) root.headerBar.clearSearch()
                QbzSearch.cortinillaRowClicked(fi)
            }
        }
    }
}
