// QueueView — a chronological, virtualized projection of playback history,
// the current track and the complete upcoming sequence. The core queue keeps
// its streaming semantics; history becomes playable again only when the user
// activates or drags one of its rows back below Now Playing.

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import com.blitzfc.qbz
import "../controls"
import "../rows"
import "../theme"

Rectangle {
    id: root

    QbzTheme { id: theme }

    color: theme.ambientOn ? "transparent" : theme.surfaceMain
    radius: theme.radiusMd

    readonly property var doc: {
        try {
            return JSON.parse(QbzQueue.extendedQueueJson)
        } catch (e) {
            return ({ "rows": [], "currentIndex": -1, "historyCount": 0,
                      "upcomingCount": 0, "stopAfterId": "",
                      "infinitePlay": false, "searchQuery": "" })
        }
    }
    readonly property var rows: root.doc.rows || []
    readonly property bool searchActive: (root.doc.searchQuery || "") !== ""
    readonly property int firstUpcoming: (root.doc.historyCount || 0)
        + (root.doc.currentIndex >= 0 ? 1 : 0)
    readonly property var currentRow: root.doc.currentIndex >= 0
        ? root.rows[root.doc.currentIndex] : null

    Component.onCompleted: {
        QbzQueue.queueExtendedOpened()
        artDispatch.restart()
    }
    Component.onDestruction: QbzQueue.queueExtendedClosed()
    onDocChanged: artDispatch.restart()

    // ----------------------------- artwork ------------------------------

    property var coverMap: ({})
    property var askedCovers: ({})

    Connections {
        target: QbzLibrary
        function onLibraryArtworkReady(key, path) {
            if (root.askedCovers[key] !== true)
                return
            var next = Object.assign({}, root.coverMap)
            next[key] = path
            root.coverMap = next
        }
    }

    function requestVisibleCovers() {
        if (root.rows.length === 0 || queueList.height <= 0)
            return
        var first = queueList.indexAt(4, queueList.contentY + 2)
        if (first < 0)
            first = 0
        var last = queueList.indexAt(4, queueList.contentY + queueList.height - 2)
        if (last < first)
            last = Math.min(root.rows.length - 1, first + 16)
        first = Math.max(0, first - 3)
        last = Math.min(root.rows.length - 1, last + 3)
        var urls = []
        var asked = Object.assign({}, root.askedCovers)
        for (var i = first; i <= last; i++) {
            var url = root.rows[i].artUrl || ""
            if (url !== "" && asked[url] !== true && !root.coverMap[url]) {
                asked[url] = true
                urls.push(url)
            }
        }
        root.askedCovers = asked
        if (urls.length > 0)
            QbzShell.sidebarArtworkWindow(JSON.stringify(urls))
    }

    Timer {
        id: artDispatch
        interval: 70
        repeat: false
        onTriggered: root.requestVisibleCovers()
    }

    // ----------------------------- actions ------------------------------

    function rowBlocked(row) {
        return QbzQConnect.qconnectConnected && row.qconnectCompatible !== true
    }

    function dragAllowed(row) {
        return !root.searchActive && row.phase !== "current" && !root.rowBlocked(row)
    }

    function sectionText(index, row) {
        if (index === 0 && row.phase === "history")
            return QbzSession.tr("HISTORY", QbzSession.trRev)
        if (row.phase === "current")
            return QbzSession.tr("NOW PLAYING", QbzSession.trRev)
        if (row.section === "next-in-queue")
            return QbzSession.tr("NEXT IN QUEUE", QbzSession.trRev)
        if (row.section === "next-up")
            return QbzSession.tr("NEXT UP", QbzSession.trRev)
        return ""
    }

    function queueMenu(row) {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        var items = []
        if (row.phase === "upcoming") {
            items.push({ "label": t("Remove from queue", r), "icon": "trash-2",
                         "action": "remove", "external": true })
            items.push({ "label": row.id === (root.doc.stopAfterId || "")
                             ? t("Cancel stop after this", r) : t("Stop after this", r),
                         "icon": "circle-stop", "action": "stop-after", "external": true })
            items.push({ "label": t("Remove all after", r), "icon": "list-x",
                         "action": "remove-after", "external": true })
        } else if (row.phase === "history" && !root.rowBlocked(row)) {
            items.push({ "label": t("Play now", r), "icon": "play-fill", "action": "play" })
        } else if (row.phase === "current") {
            items.push({ "label": row.id === (root.doc.stopAfterId || "")
                             ? t("Cancel stop after this", r) : t("Stop after this", r),
                         "icon": "circle-stop", "action": "stop-after", "external": true })
        }
        if (row.isEphemeral !== true) {
            if (row.isLocal !== true)
                items.push({ "label": t("Add to playlist", r), "icon": "list-plus",
                             "action": "add-to-playlist", "external": true })
            items.push({ "label": t("Track info", r), "icon": "info", "action": "track-info" })
            if (row.isLocal !== true)
                items.push({ "label": row.isFavorite === true
                                 ? t("Remove from Library", r) : t("Add to Library", r),
                             "icon": row.isFavorite === true ? "heart-filled" : "heart",
                             "action": "favorite" })
        }
        return items
    }

    function rowPlay(row) {
        if (root.rowBlocked(row))
            return
        if (row.phase === "current")
            QbzPlayer.togglePlay()
        else
            QbzQueue.queueExtendedPlay(row.phase, row.phaseIndex, row.id)
    }

    function menuAction(row, action) {
        if (action === "remove")
            QbzQueue.queueRemoveUpcomingFlat(row.phaseIndex)
        else if (action === "remove-after")
            QbzQueue.queueRemoveAllAfterFlat(row.phaseIndex)
        else if (action === "stop-after")
            QbzQueue.queueToggleStopAfter(row.id)
        else if (action === "add-to-playlist")
            QbzPlaylistPicker.openForTrack(row.id)
    }

    // -------------------------- local reorder ----------------------------

    property string dragPhase: ""
    property int dragPhaseIndex: -1
    property string dragTrackId: ""
    property int dropSlot: -1
    property real dropLineY: -1
    property bool dropHot: false

    function beginQueueDrag(row) {
        root.dragPhase = row.phase
        root.dragPhaseIndex = row.phaseIndex
        root.dragTrackId = row.id
        root.dropSlot = -1
        root.dropHot = false
    }

    function recomputeDrop() {
        if (!QbzShell.dragActive || root.dragTrackId === "" || root.searchActive) {
            root.dropHot = false
            root.dropSlot = -1
            return
        }
        var p = listHost.mapFromItem(null, QbzShell.dragX, QbzShell.dragY)
        if (p.x < 0 || p.x > queueList.width || p.y < 0 || p.y > listHost.height) {
            root.dropHot = false
            root.dropSlot = -1
            return
        }
        var contentPoint = queueList.mapFromItem(null, QbzShell.dragX, QbzShell.dragY)
        var cy = queueList.contentY + contentPoint.y
        var index = queueList.indexAt(Math.max(1, queueList.width / 2), cy)
        var insertion
        if (index < 0) {
            insertion = cy <= queueList.originY ? 0 : root.rows.length
        } else {
            var target = queueList.itemAtIndex(index)
            insertion = target && cy >= target.y + target.height / 2 ? index + 1 : index
        }
        insertion = Math.max(root.firstUpcoming, insertion)
        root.dropSlot = Math.max(0, Math.min(root.doc.upcomingCount || 0,
                                             insertion - root.firstUpcoming))
        root.dropHot = true

        var fullIndex = root.firstUpcoming + root.dropSlot
        var lineItem = fullIndex < root.rows.length ? queueList.itemAtIndex(fullIndex) : null
        if (lineItem)
            root.dropLineY = queueList.y + lineItem.y - queueList.contentY
        else {
            var last = queueList.itemAtIndex(root.rows.length - 1)
            root.dropLineY = last ? queueList.y + last.y + last.height
                                      - queueList.contentY
                                      : listHost.height / 2
        }
    }

    function finishQueueDrag() {
        var shouldCommit = root.dropHot && root.dropSlot >= 0
        var phase = root.dragPhase
        var phaseIndex = root.dragPhaseIndex
        var trackId = root.dragTrackId
        var slot = root.dropSlot
        root.dragPhase = ""
        root.dragPhaseIndex = -1
        root.dragTrackId = ""
        root.dropSlot = -1
        root.dropHot = false
        if (shouldCommit)
            QbzQueue.queueExtendedDrop(phase, phaseIndex, trackId, slot)
    }

    Connections {
        target: QbzShell
        function onDragXChanged() { root.recomputeDrop() }
        function onDragYChanged() { root.recomputeDrop() }
        function onDragActiveChanged() {
            if (QbzShell.dragActive)
                root.recomputeDrop()
            else if (root.dragTrackId !== "")
                root.finishQueueDrag()
        }
    }

    Timer {
        interval: 32
        repeat: true
        running: QbzShell.dragActive && root.dragTrackId !== "" && root.dropHot
        onTriggered: {
            var p = listHost.mapFromItem(null, QbzShell.dragX, QbzShell.dragY)
            if (p.y < 34)
                queueList.contentY = Math.max(queueList.originY, queueList.contentY - 18)
            else if (p.y > listHost.height - 34)
                queueList.contentY = Math.min(queueList.originY
                    + Math.max(0, queueList.contentHeight - queueList.height),
                    queueList.contentY + 18)
            root.recomputeDrop()
        }
    }

    // ------------------------------- UI ---------------------------------

    Column {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            width: parent.width
            height: 54
            topLeftRadius: theme.radiusMd
            topRightRadius: theme.radiusMd
            color: theme.ambientOn ? theme.surfaceMainA30 : theme.surfaceMain

            Row {
                anchors.left: parent.left
                anchors.leftMargin: theme.spacingMd
                anchors.verticalCenter: parent.verticalCenter
                spacing: theme.spacingSm

                QbzIconButton {
                    btnSize: 32
                    name: "panel-right-close"
                    tooltip: tips
                    tooltipKey: "queue-view-close"
                    tooltipText: QbzSession.tr("Back", QbzSession.trRev)
                    onClicked: QbzShell.navigateBack()
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.width >= 720
                    text: QbzSession.tr("Queue View", QbzSession.trRev)
                    color: theme.textPrimary
                    font.pixelSize: theme.fontSection
                    font.weight: theme.weightSemibold
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.width >= 900
                    text: root.rows.length + " " + QbzSession.tr("tracks", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: 11
                }
            }

            Row {
                anchors.right: parent.right
                anchors.rightMargin: theme.spacingMd
                anchors.verticalCenter: parent.verticalCenter
                spacing: theme.spacingXs

                QbzLineEdit {
                    width: Math.max(110, Math.min(220, root.width
                        - (root.width >= 720 ? 490 : 390)))
                    searchMode: true
                    text: root.doc.searchQuery || ""
                    placeholder: QbzSession.tr("Search queue", QbzSession.trRev)
                    onEdited: function (value) { QbzQueue.queueSetSearch(value) }
                }
                QbzIconButton {
                    btnSize: 32
                    name: "trash-list"
                    btnEnabled: root.rows.length > 0
                    tooltip: tips
                    tooltipKey: "queue-view-clear"
                    tooltipText: QbzSession.tr("Clear", QbzSession.trRev)
                    onClicked: QbzQueue.queueClear()
                }
                QbzIconButton {
                    btnSize: 32
                    name: "add-to-list"
                    btnEnabled: root.rows.length > 0
                        && !(root.currentRow && root.currentRow.isEphemeral === true)
                    tooltip: tips
                    tooltipKey: "queue-view-save"
                    tooltipText: QbzSession.tr("Add to Playlist", QbzSession.trRev)
                    onClicked: QbzQueue.queueSaveAsPlaylist()
                }
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: QbzSession.tr("Continuous playback", QbzSession.trRev)
                        color: theme.textSecondary
                        font.pixelSize: 11
                    }
                    QbzToggle {
                        anchors.verticalCenter: parent.verticalCenter
                        checked: root.doc.infinitePlay === true
                        onToggled: QbzQueue.queueToggleInfinitePlay()
                    }
                }
                QbzIconButton {
                    id: sleepButton
                    btnSize: 32
                    name: "clock"
                    active: QbzQueue.sleepActive
                    tooltip: tips
                    tooltipKey: "queue-view-sleep"
                    tooltipText: QbzSession.tr("Set Timer", QbzSession.trRev)
                    onClicked: sleepMenu.openAtCursor(sleepButton, 0, sleepButton.height)

                    CardMenu {
                        id: sleepMenu
                        menuWidth: 196
                        entries: QbzQueue.sleepActive
                            ? [{ "label": QbzSession.tr("Cancel Timer", QbzSession.trRev),
                                 "icon": "x", "action": "cancel" }]
                            : [
                                { "label": QbzSession.tr("30 min", QbzSession.trRev), "icon": "clock", "action": "30" },
                                { "label": QbzSession.tr("1 hr", QbzSession.trRev), "icon": "clock", "action": "60" },
                                { "label": QbzSession.tr("2 hr", QbzSession.trRev), "icon": "clock", "action": "120" },
                                { "label": QbzSession.tr("3 hr", QbzSession.trRev), "icon": "clock", "action": "180" },
                                { "label": QbzSession.tr("5 hr", QbzSession.trRev), "icon": "clock", "action": "300" },
                                { "sep": true },
                                { "label": QbzSession.tr("Custom…", QbzSession.trRev), "icon": "clock", "action": "custom" }
                              ]
                        onPicked: function (action) {
                            if (action === "cancel")
                                QbzQueue.sleepTimerCancel()
                            else if (action === "custom")
                                customSleep.open()
                            else
                                QbzQueue.sleepTimerSet(parseInt(action, 10))
                        }
                    }
                    Popup {
                        id: customSleep
                        x: -184
                        y: sleepButton.height + 6
                        width: 216
                        padding: 12
                        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
                        background: Rectangle {
                            radius: theme.radiusMd
                            color: theme.surfaceCard
                            border.width: 1
                            border.color: theme.borderSubtle
                        }
                        contentItem: Column {
                            spacing: 8
                            Text {
                                text: QbzSession.tr("Stop playback after:", QbzSession.trRev)
                                color: theme.textMuted
                                font.pixelSize: 11
                            }
                            Row {
                                spacing: 8
                                Rectangle {
                                    width: 116
                                    height: 32
                                    radius: theme.radiusSm
                                    color: theme.surfaceElevated
                                    border.width: 1
                                    border.color: customMinutes.activeFocus
                                        ? theme.accent : theme.borderSubtle
                                    TextInput {
                                        id: customMinutes
                                        anchors.fill: parent
                                        anchors.leftMargin: 9
                                        anchors.rightMargin: 9
                                        text: "60"
                                        color: theme.textPrimary
                                        font.pixelSize: 12
                                        verticalAlignment: Text.AlignVCenter
                                        validator: IntValidator { bottom: 1; top: 1440 }
                                        inputMethodHints: Qt.ImhDigitsOnly
                                        selectByMouse: true
                                    }
                                }
                                Rectangle {
                                    width: 64
                                    height: 32
                                    radius: theme.radiusSm
                                    color: setSleepArea.containsMouse ? theme.accentHover : theme.accent
                                    Text {
                                        anchors.centerIn: parent
                                        text: QbzSession.tr("Set", QbzSession.trRev)
                                        color: theme.accentText
                                        font.pixelSize: 12
                                    }
                                    MouseArea {
                                        id: setSleepArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: {
                                            var minutes = parseInt(customMinutes.text, 10)
                                            if (!isNaN(minutes) && minutes > 0)
                                                QbzQueue.sleepTimerSet(minutes)
                                            customSleep.close()
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }

        Item {
            id: listHost
            width: parent.width
            height: parent.height - 55

            ListView {
                id: queueList
                anchors.left: parent.left
                anchors.right: queueScroll.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.leftMargin: theme.spacingMd
                anchors.topMargin: theme.spacingSm
                anchors.bottomMargin: theme.spacingSm
                clip: true
                spacing: 3
                boundsBehavior: Flickable.StopAtBounds
                reuseItems: true
                model: root.rows
                onContentYChanged: artDispatch.restart()

                delegate: Item {
                    id: rowHost
                    required property var modelData
                    required property int index

                    readonly property string heading: root.sectionText(index, modelData)
                    readonly property var displayItem: Object.assign({}, modelData, {
                        "artPath": root.coverMap[modelData.artUrl || ""] || ""
                    })

                    width: queueList.width
                    height: (heading !== "" ? 24 : 0) + 50
                    ListView.onPooled: {
                        trackRow.recycleActive = false
                        trackRow.releaseForReuse()
                    }
                    ListView.onReused: trackRow.recycleActive = true

                    Text {
                        visible: rowHost.heading !== ""
                        x: theme.spacingSm
                        width: parent.width - 2 * theme.spacingSm
                        height: 24
                        text: rowHost.heading
                        color: rowHost.modelData.phase === "current"
                            ? theme.accent : theme.textMuted
                        font.pixelSize: 10
                        font.weight: theme.weightSemibold
                        font.letterSpacing: 0.5
                        verticalAlignment: Text.AlignVCenter
                    }

                    TrackRow {
                        id: trackRow
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        item: rowHost.displayItem
                        number: rowHost.index + 1
                        showArtwork: true
                        showAlbum: root.width >= 720
                        showFavorite: false
                        showDownload: false
                        showMenu: true
                        zebra: true
                        artistLink: true
                        draggable: root.dragAllowed(rowHost.modelData)
                        reorderDrag: true
                        playBlocked: root.rowBlocked(rowHost.modelData)
                        activeBackground: true
                        overrideActiveMatch: true
                        activeMatch: rowHost.modelData.phase === "current"
                        leadingMarkerIcon: rowHost.modelData.id === (root.doc.stopAfterId || "")
                            ? "circle-stop" : ""
                        menuEntriesOverride: root.queueMenu(rowHost.modelData)
                        artPending: (rowHost.modelData.artUrl || "") !== ""
                            && !root.coverMap[rowHost.modelData.artUrl]
                        skelPhase: (Math.floor(Math.abs(QbzShell.pulseMs) / 900) % 2) === 1
                        artSettleMs: 2500

                        onPlayRequested: root.rowPlay(rowHost.modelData)
                        onBodyDragStarted: root.beginQueueDrag(rowHost.modelData)
                        onMenuActionRequested: function (action) {
                            root.menuAction(rowHost.modelData, action)
                        }
                    }
                }
            }

            QbzScrollBar {
                id: queueScroll
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: queueList
            }

            Rectangle {
                visible: root.dropHot && root.dropLineY >= 0
                x: queueList.x + theme.spacingSm
                y: Math.max(0, Math.min(parent.height - height, root.dropLineY))
                width: queueList.width - 2 * theme.spacingSm
                height: 2
                radius: 1
                color: theme.accent
            }

            QbzEmptyState {
                visible: root.rows.length === 0
                anchors.centerIn: parent
                iconName: root.searchActive ? "search" : "list-music"
                title: root.searchActive
                    ? QbzSession.tr("No tracks match your search", QbzSession.trRev)
                    : QbzSession.tr("Your queue is empty", QbzSession.trRev)
                body: root.searchActive ? ""
                    : QbzSession.tr("Play an album or track to get started", QbzSession.trRev)
            }
        }
    }

    QbzTooltip {
        id: tips
        anchors.fill: parent
        z: 4000
    }
}
