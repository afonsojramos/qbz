// Right-side Queue panel — QML port of
// crates/qbz-ui/ui/shell/QueueSidebar.slint.
//
// Tabs (Queue / History), NOW PLAYING card (live heart), UP NEXT with the
// #442 "Next in queue" / "Next up" section markers, 40-row pagination,
// live search filter, row actions (play / remove / remove-all-after /
// heart), drag reorder (basic press-drag), footer (count line + Clear /
// stubs + search field), History tab (thumbnail rows, click replays as a
// fresh single-track queue), exact empty states. Data: QbzQueue.queueJson
// (queue_qt.rs QueueDoc).
//
// POC-NOTEs: save-as-playlist (opens a picker modal upstream — inert),
// infinite-play + sleep timer engines, stop-after marker, row menu's
// "Add to playlist" / "Track info" entries, ephemeral rows, the ghost
// drag pill (rows move with a plain displaced animation instead).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root
    color: "transparent"
    clip: true

    // Queue (0) / History (1) — Slint QueueState.tab.
    property int tab: 0
    readonly property var doc: JSON.parse(QbzQueue.queueJson)
    readonly property var currentRow: doc.current || null
    readonly property var upcoming: doc.upcoming || []
    readonly property var historyRows: doc.history || []
    readonly property bool queueEmpty: currentRow === null && (doc.upcomingTotal || 0) === 0

    // url-keyed cover map (shared artwork pipeline).
    property var coverMap: ({})

    // Drag-reorder state (basic press-drag; ghost is the row's own y).
    property int dragFrom: -1

    QbzTheme { id: theme }

    Connections {
        target: QbzLibrary
        function onLibraryArtworkReady(key, path) {
            var m = root.coverMap
            m[key] = path
            root.coverMap = Object.assign({}, m)
        }
    }
    Component.onCompleted: dispatchCovers()
    onDocChanged: dispatchCovers()
    function dispatchCovers() {
        var urls = []
        if (currentRow && currentRow.artUrl) urls.push(currentRow.artUrl)
        var i
        for (i = 0; i < upcoming.length; i++) if (upcoming[i].artUrl) urls.push(upcoming[i].artUrl)
        for (i = 0; i < historyRows.length; i++) if (historyRows[i].artUrl) urls.push(historyRows[i].artUrl)
        if (urls.length > 0) QbzShell.sidebarArtworkWindow(JSON.stringify(urls))
    }

    component QueueTab: Text {
        property bool active: false
        signal clicked()

        font.pixelSize: theme.fontBody
        font.weight: theme.weightSemibold
        color: active ? theme.textPrimary
             : tabArea.containsMouse ? theme.textSecondary : theme.textMuted
        verticalAlignment: Text.AlignVCenter
        height: parent ? parent.height : 0

        MouseArea {
            id: tabArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }

    // Row action icon button (the panel's small IconButton).


    // One UP NEXT / History row (QueueRow.slint).
    component QueueRow: Rectangle {
        property var row: ({})
        property int rowIndex: 0
        property bool showNumber: true
        property bool inQueue: true

        readonly property bool hovered: qrArea.containsMouse || menuArea.containsMouse
        readonly property bool isActive: inQueue && QbzPlayer.npTrackId !== "" && QbzPlayer.npTrackId === row.id

        height: 44
        radius: theme.radiusSm
        color: hovered ? theme.surfaceHover
             : (rowIndex % 2 === 1 ? "#592a2a2a" : "transparent")
        border.width: root.dragFrom === rowIndex ? 1 : 0
        border.color: theme.accent
        opacity: root.dragFrom === rowIndex ? 0.4 : 1.0

        Row {
            anchors.fill: parent
            anchors.leftMargin: theme.spacingSm
            anchors.rightMargin: theme.spacingXs
            anchors.topMargin: 4
            anchors.bottomMargin: 4
            spacing: 9

            // Leading: track number (UP NEXT) or thumbnail (History).
            Text {
                visible: showNumber
                width: 22
                anchors.verticalCenter: parent.verticalCenter
                text: rowIndex + 1 + (doc.page || 0) * 40
                color: theme.textMuted
                font.pixelSize: 12
                horizontalAlignment: Text.AlignHCenter
            }
            Rectangle {
                visible: !showNumber
                width: 34
                height: 34
                radius: 4
                anchors.verticalCenter: parent.verticalCenter
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    source: root.coverMap[row.artUrl] || ""
                    radius: 4
                }
            }

            // Title + artist.
            Column {
                width: parent.width - (showNumber ? 22 : 34) - durText.width - 32 - 3 * 9
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Row {
                    spacing: 5
                    Text {
                        text: row.title
                        color: theme.textPrimary
                        font.pixelSize: 12
                        font.weight: theme.weightMedium
                        elide: Text.ElideRight
                        width: Math.min(implicitWidth, parent.parent.width - (row.explicit ? 21 : 0))
                    }
                    Rectangle {
                        visible: row.explicit
                        width: 16
                        height: 16
                        radius: 3
                        anchors.verticalCenter: parent.verticalCenter
                        color: theme.surfaceElevated
                        Text {
                            anchors.centerIn: parent
                            text: "E"
                            color: theme.textMuted
                            font.pixelSize: 9
                            font.weight: theme.weightSemibold
                        }
                    }
                }
                Text {
                    width: parent.width
                    text: row.artist
                    color: theme.textMuted
                    font.pixelSize: 10
                    elide: Text.ElideRight
                }
            }
            Text {
                id: durText
                anchors.verticalCenter: parent.verticalCenter
                text: row.duration
                color: theme.textMuted
                font.pixelSize: 11
            }
            // ⋯ menu trigger (UP NEXT rows only; always visible, 32px hit).
            Rectangle {
                visible: inQueue
                width: 32
                height: 32
                radius: theme.radiusSm
                anchors.verticalCenter: parent.verticalCenter
                color: menuArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon {
                    anchors.centerIn: parent
                    name: "ellipsis"
                    width: 16
                    height: 16
                    tintName: menuArea.containsMouse ? "primary" : "muted"
                }
                MouseArea {
                    id: menuArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: function (mouse) { rowMenu.openAtCursor(menuArea, mouse.x, mouse.y) }
                }
                QbzContextMenu {
                    id: rowMenu
                    menuWidth: 196
                        Repeater {
                            model: [
                                { "label": QbzSession.tr("Remove from queue", QbzSession.trRev), "icon": "trash-2", "action": "remove" },
                                { "label": QbzSession.tr("Remove all after", QbzSession.trRev), "icon": "list-x", "action": "remove-after" },
                                { "label": row.isFavorite ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev), "icon": row.isFavorite ? "heart-filled" : "heart", "action": "favorite" },
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                width: parent ? parent.width : 0
                                height: 33
                                radius: 5
                                color: rmiArea.containsMouse ? theme.surfaceHover : "transparent"
                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: 8
                                    spacing: 8
                                    QbzIcon { name: modelData.icon; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                                    Text {
                                        height: parent.height
                                        width: parent.width - 23
                                        text: modelData.label
                                        color: theme.textSecondary
                                        font.pixelSize: 13
                                        verticalAlignment: Text.AlignVCenter
                                        elide: Text.ElideRight
                                    }
                                }
                                MouseArea {
                                    id: rmiArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        rowMenu.close()
                                        var a = modelData.action
                                        if (a === "remove") QbzQueue.queueRemoveUpcoming(rowIndex)
                                        else if (a === "remove-after") QbzQueue.queueRemoveAllAfter(rowIndex)
                                        else if (a === "favorite") {
                                            row.isFavorite = !row.isFavorite
                                            QbzQueue.queueToggleFavorite("track", row.id)
                                        }
                                    }
                                }
                            }
                        }
                    }
            }
        }

        MouseArea {
            id: qrArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                if (inQueue) QbzQueue.queuePlayUpcoming(rowIndex)
                else QbzQueue.queuePlayHistory(rowIndex)
            }
        }
    }

    Column {
        anchors.fill: parent
        spacing: 0

        // --- Header: tabs + close button -------------------------------
        Item {
            width: parent.width
            height: 44

            Row {
                anchors.left: parent.left
                anchors.leftMargin: theme.spacingMd
                anchors.right: closeBtn.left
                anchors.rightMargin: theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                height: 28
                spacing: 14

                QueueTab {
                    text: QbzSession.tr("Queue", QbzSession.trRev)
                    active: root.tab === 0
                    onClicked: root.tab = 0
                }
                Text {
                    text: "|"
                    color: theme.textDisabled
                    font.pixelSize: theme.fontBody
                    height: parent.height
                    verticalAlignment: Text.AlignVCenter
                }
                QueueTab {
                    text: QbzSession.tr("History", QbzSession.trRev)
                    active: root.tab === 1
                    onClicked: root.tab = 1
                }
            }
            Rectangle {
                id: closeBtn
                anchors.right: parent.right
                anchors.rightMargin: theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                width: 28
                height: 28
                radius: theme.radiusSm
                color: closeArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: "x"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: closeArea.containsMouse ? "primary" : "secondary"
                }
                MouseArea {
                    id: closeArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzShell.toggleQueue()
                }
            }
        }
        Rectangle {
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // --- Body --------------------------------------------------------
        Item {
            width: parent.width
            height: parent.height - 45 - footerBlock.height

            // ===== Queue tab =====
            Flickable {
                visible: root.tab === 0
                anchors.fill: parent
                clip: true
                contentWidth: width
                contentHeight: queueBody.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: queueBody
                    width: parent.width
                    padding: 10
                    spacing: 12

                    // NOW PLAYING section.
                    Column {
                        visible: root.currentRow !== null
                        width: parent.width - 20
                        spacing: 10
                        Text {
                            text: QbzSession.tr("NOW PLAYING", QbzSession.trRev)
                            color: theme.textMuted
                            font.pixelSize: 11
                            font.weight: theme.weightSemibold
                            font.letterSpacing: 0.5
                        }
                        Rectangle {
                            width: parent.width
                            height: 44
                            radius: theme.radiusSm
                            color: theme.surfaceElevated
                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: 6
                                anchors.rightMargin: 6
                                anchors.topMargin: 4
                                anchors.bottomMargin: 4
                                spacing: 9
                                Rectangle {
                                    width: 34
                                    height: 34
                                    radius: 4
                                    anchors.verticalCenter: parent.verticalCenter
                                    color: theme.surfaceCard
                                    clip: true
                                    RoundedImage {
                                        anchors.fill: parent
                                        source: root.currentRow ? (root.coverMap[root.currentRow.artUrl] || "") : ""
                                        radius: 4
                                    }
                                }
                                Column {
                                    width: parent.width - 34 - 28 - 2 * 9
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 2
                                    Text {
                                        width: parent.width
                                        text: root.currentRow ? root.currentRow.title : ""
                                        color: theme.textPrimary
                                        font.pixelSize: 12
                                        font.weight: theme.weightMedium
                                        elide: Text.ElideRight
                                    }
                                    Text {
                                        width: parent.width
                                        text: root.currentRow ? root.currentRow.artist : ""
                                        color: theme.textMuted
                                        font.pixelSize: 10
                                        elide: Text.ElideRight
                                    }
                                }
                                // Favorite toggle (wired).
                                Rectangle {
                                    width: 28
                                    height: 28
                                    radius: theme.radiusSm
                                    anchors.verticalCenter: parent.verticalCenter
                                    color: npFavArea.containsMouse ? theme.surfaceHover : "transparent"
                                    QbzIcon {
                                        anchors.centerIn: parent
                                        name: root.currentRow && root.currentRow.isFavorite ? "heart-filled" : "heart"
                                        width: 17
                                        height: 17
                                        tintName: root.currentRow && root.currentRow.isFavorite ? "favorite" : (npFavArea.containsMouse ? "primary" : "muted")
                                    }
                                    MouseArea {
                                        id: npFavArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: {
                                            if (root.currentRow) {
                                                root.currentRow.isFavorite = !root.currentRow.isFavorite
                                                QbzQueue.queueToggleFavorite("track", root.currentRow.id)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // UP NEXT section.
                    Column {
                        visible: (doc.upcomingTotal || 0) > 0
                        width: parent.width - 20
                        spacing: 8
                        Text {
                            text: QbzSession.tr("UP NEXT", QbzSession.trRev)
                            color: theme.textMuted
                            font.pixelSize: 11
                            font.weight: theme.weightSemibold
                            font.letterSpacing: 0.5
                        }

                        Repeater {
                            model: root.upcoming
                            delegate: Column {
                                required property var modelData
                                required property int index
                                width: parent ? parent.width : 0
                                // #442 section header above the row.
                                Row {
                                    visible: modelData.section !== ""
                                    height: 22
                                    spacing: 6
                                    QbzIcon {
                                        visible: modelData.section === "next-in-queue"
                                        name: "list-start"
                                        width: 13
                                        height: 13
                                        anchors.verticalCenter: parent.verticalCenter
                                        tintName: "accent"
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: modelData.section === "next-in-queue"
                                            ? QbzSession.tr("Next in queue", QbzSession.trRev) : QbzSession.tr("Next up", QbzSession.trRev)
                                        color: theme.textMuted
                                        font.pixelSize: 11
                                        font.weight: theme.weightSemibold
                                        font.letterSpacing: 0.5
                                    }
                                }
                                QueueRow {
                                    width: parent.width
                                    row: modelData
                                    rowIndex: index
                                    showNumber: true
                                    inQueue: true
                                    // Basic press-drag reorder (POC: no ghost pill).
                                    MouseArea {
                                        anchors.fill: parent
                                        onPressAndHold: root.dragFrom = index
                                        onReleased: {
                                            if (root.dragFrom >= 0) {
                                                var target = Math.max(0, Math.min(root.upcoming.length - 1,
                                                    Math.floor((parent.mapToItem(null, mouseX, mouseY).y - queueBody.mapToItem(null, 0, 0).y - 40) / 44)))
                                                QbzQueue.queueMoveTrack((doc.page || 0) * 40 + root.dragFrom, (doc.page || 0) * 40 + target)
                                                root.dragFrom = -1
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Paginator (plain text controls, no pills).
                        Row {
                            visible: (doc.pageCount || 1) > 1
                            width: parent.width
                            anchors.horizontalCenter: parent.horizontalCenter
                            spacing: theme.spacingMd
                            Item { width: (parent.width - paginator.width) / 2; height: 1 }
                            Row {
                                id: paginator
                                spacing: theme.spacingMd
                                QbzIconButton { btnSize: 30 
                                    name: "chevron-left"
                                    iconSize: 15
                                    btnEnabled: (doc.page || 0) > 0
                                    onClicked: QbzQueue.queueSetPage((doc.page || 0) - 1)
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: QbzSession.tr("Page {} of {}", QbzSession.trRev).replace("{}", (doc.page || 0) + 1).replace("{}", doc.pageCount || 1)
                                    color: theme.textMuted
                                    font.pixelSize: 12
                                }
                                QbzIconButton { btnSize: 30 
                                    name: "chevron-right"
                                    iconSize: 15
                                    btnEnabled: (doc.page || 0) < (doc.pageCount || 1) - 1
                                    onClicked: QbzQueue.queueSetPage((doc.page || 0) + 1)
                                }
                            }
                            Item { width: (parent.width - paginator.width) / 2; height: 1 }
                        }
                    }

                    // Empty state — no current track and no upcoming.
                    Column {
                        visible: root.queueEmpty
                        width: parent.width - 20
                        topPadding: 48 - 10
                        spacing: 6
                        Text {
                            width: parent.width
                            text: QbzSession.tr("Your queue is empty", QbzSession.trRev)
                            color: theme.textSecondary
                            font.pixelSize: theme.fontBody
                            font.weight: theme.weightMedium
                            horizontalAlignment: Text.AlignHCenter
                        }
                        Text {
                            width: parent.width
                            text: QbzSession.tr("Play an album or track to get started", QbzSession.trRev)
                            color: theme.textMuted
                            font.pixelSize: 12
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                        }
                    }
                }
            }

            // ===== History tab =====
            Flickable {
                visible: root.tab === 1
                anchors.fill: parent
                clip: true
                contentWidth: width
                contentHeight: historyBody.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: historyBody
                    width: parent.width
                    padding: 10
                    spacing: 8

                    Text {
                        text: QbzSession.tr("RECENTLY PLAYED", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 11
                        font.weight: theme.weightSemibold
                        font.letterSpacing: 0.5
                    }
                    Repeater {
                        model: root.historyRows
                        delegate: QueueRow {
                            width: parent.width
                            row: modelData
                            rowIndex: index
                            showNumber: false
                            inQueue: false
                        }
                    }
                    Text {
                        visible: root.historyRows.length === 0
                        width: parent.width
                        topPadding: 48 - 10
                        text: QbzSession.tr("Nothing played yet", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: theme.fontBody
                        horizontalAlignment: Text.AlignHCenter
                    }
                }
            }
        }

        // --- Footer: count + actions + inline search (Queue tab) ---------
        Column {
            id: footerBlock
            visible: root.tab === 0 && !root.queueEmpty
            width: parent.width
            spacing: 0
            Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
            Text {
                visible: (doc.upcomingTotal || 0) > 0
                width: parent.width
                leftPadding: theme.spacingMd
                rightPadding: theme.spacingMd
                topPadding: 8
                text: (doc.upcomingTotal === 1
                        ? QbzSession.tr("Showing {} of {} track.", QbzSession.trRev)
                        : QbzSession.tr("Showing {} of {} tracks.", QbzSession.trRev))
                      .replace("{}", doc.pageEnd || 0).replace("{}", doc.upcomingTotal || 0)
                color: theme.textMuted
                font.pixelSize: 11
            }
            Row {
                id: footerRow
                width: parent.width
                leftPadding: theme.spacingMd
                rightPadding: theme.spacingMd
                topPadding: 10
                bottomPadding: 10
                spacing: theme.spacingXs

                // The row's usable width — `parent.width` here is the PANEL
                // width, padding included; using it raw is what pushed the
                // search field out of the 300px column.
                readonly property int contentWidth:
                    width - leftPadding - rightPadding

                // Action 1 — Clear queue (wired).
                QbzIconButton { btnSize: 30
                    name: "trash-list"
                    onClicked: QbzQueue.queueClear()
                }
                // Action 2 — Save as playlist (INERT: opens a picker modal
                // upstream — POC-NOTE).
                QbzIconButton { btnSize: 30; name: "add-to-list" }
                // Action 3 — infinite play (INERT engine — POC-NOTE).
                QbzIconButton { btnSize: 30; name: "infinity" }
                // Action 4 — sleep timer (INERT engine — POC-NOTE).
                QbzIconButton { btnSize: 30; name: "clock" }

                // Filler so the collapsed magnifier sits at the right edge;
                // the field opens LEFT across it (5 gaps of spacingXs).
                Item {
                    width: Math.max(0, footerRow.contentWidth - 4 * 30 - 30
                                       - 5 * theme.spacingXs)
                    height: 1
                }
                // Inline queue search (wired) — the shared expandable control.
                QbzLineEdit {
                    searchMode: true
                    expandable: true
                    sm: true
                    // Bound to the PANEL, never wider: it opens leftward over
                    // the four action buttons, exactly like the Slint
                    // ExpandableSearch does over its neighbours.
                    openWidth: Math.max(90, footerRow.contentWidth)
                    placeholder: QbzSession.tr("Search queue", QbzSession.trRev)
                    onEdited: function (v) { QbzQueue.queueSetSearch(v) }
                }
            }
        }
    }
}
