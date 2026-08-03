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

    readonly property var doc: parseDoc()
    function parseDoc() {
        try {
            return JSON.parse(QbzSearch.cortinillaJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property var sections: doc.sections || []
    readonly property bool hasTop: doc.top !== undefined && doc.top !== null && (doc.top.id || "") !== ""
    readonly property bool empty: !hasTop && sections.length === 0 && !QbzSearch.cortinillaLoading

    // The centered header search box width (HeaderBar's responsive rule) —
    // the click-through gaps below leave it free.
    readonly property int searchBoxWidth: root.width < 960 ? 179 : 256
    readonly property int panelWidth: 320
    // Cap so the panel NEVER covers the now-playing bar (Cortinilla.slint).
    readonly property int scrollCap: root.height - 42 - 6 - 42 - 16 - 12

    // --- Idle auto-close (4.5s without hover/activity, Cortinilla.slint) --
    property bool panelHovered: false
    Timer {
        id: idleClose
        interval: 4500
        repeat: false
        running: root.visible && !root.panelHovered
        onTriggered: QbzSearch.cortinillaDismiss()
    }
    Connections {
        target: QbzSearch
        // Activity restarts the countdown (keystroke or arrow move). The
        // keystroke probe is the QUERY, not the payload: the reference
        // restarts per keystroke, and now that the debounce lives in Rust a
        // burst of typing publishes one payload but many queries.
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
        y: 42
        width: root.width
        height: root.height - 42
        onClicked: QbzSearch.cortinillaDismiss()
    }
    // 2) header strip LEFT of the search box.
    MouseArea {
        visible: root.visible
        x: 0
        y: 0
        width: (root.width - root.searchBoxWidth) / 2
        height: 42
        onClicked: QbzSearch.cortinillaDismiss()
    }
    // 3) header strip RIGHT of the search box.
    MouseArea {
        visible: root.visible
        x: (root.width + root.searchBoxWidth) / 2
        y: 0
        width: root.width - x
        height: 42
        onClicked: QbzSearch.cortinillaDismiss()
    }

    // --- The panel ----------------------------------------------------------
    Rectangle {
        id: panel
        x: (root.width - root.panelWidth) / 2
        y: 42 + 6
        width: root.panelWidth
        height: Math.min(bodyColumn.height + 12, root.scrollCap + 12)
        radius: theme.radiusSm
        color: theme.surfaceMain
        border.width: 1
        border.color: theme.borderMuted
        clip: true

        MouseArea {
            // Hover surface for the idle-close pause; clicks stay inside.
            anchors.fill: parent
            hoverEnabled: true
            onContainsMouseChanged: root.panelHovered = containsMouse
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
                    visible: QbzSearch.cortinillaLoading
                    width: parent.width
                    topPadding: 4
                    property bool pulse: false
                    Timer {
                        interval: 700
                        running: parent.visible
                        repeat: true
                        onTriggered: parent.pulse = !parent.pulse
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
                                opacity: parent.parent.pulse ? 0.48 : 0.16
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
                                    opacity: parent.parent.parent.pulse ? 0.48 : 0.16
                                    Behavior on opacity { NumberAnimation { duration: 650; easing.type: Easing.InOutQuad } }
                                }
                                Rectangle {
                                    width: 96
                                    height: 9
                                    radius: 3
                                    color: theme.surfaceElevated
                                    opacity: parent.parent.parent.pulse ? 0.48 : 0.16
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
                            required property var modelData
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
                                model: modelData.rows
                                delegate: CortRow { row: modelData }
                            }
                        }
                    }
                }
            }
        }

        QbzScrollBar {
            target: bodyFlick
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom
        }
    }

    // --- One result row (CortinillaResultRow) ------------------------------
    component CortRow: Rectangle {
        property var row: ({})
        width: parent ? parent.width : 0
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
                if (root.headerBar) root.headerBar.clearSearch()
                QbzSearch.cortinillaRowClicked(row.flatIndex)
            }
        }
    }
}
