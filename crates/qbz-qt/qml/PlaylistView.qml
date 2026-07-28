// Playlist detail view — the QML port of playlist/PlaylistView.slint
// (phase 17). ONE JSON document (QbzBridge.playlistJson, playlist_qt.rs
// PlaylistDoc: header + track rows + ownership/follow/pin/sort/search).
//
// Header: 150px cover (server image, else first-track cover, else the
// placeholder glyph), "PLAYLIST" eyebrow, name, description-short + Read
// more (the shared AppShell text modal), owner • N tracks • duration,
// the action row: Play / Shuffle / heart (owned, wired) / pin (wired) /
// follow (foreign, subscribe API) / copy (foreign, create+add) / edit
// (owner: rename + delete modal). Right edge: in-playlist search + the
// sort dropdown (Default/Title/Artist/Album/Duration/Date added/Custom).
//
// Track list: the exact PlaylistView row (# / 36px art / title+artist /
// Album 220px link / Duration 70 / Quality 92 / heart / cloud reserve /
// ⋯ menu with Remove-from-playlist for owners), column header, empty and
// loading states. Per-row ⋯: Play / Play next / Play later / Add to
// queue / Go to artist / Go to album / Add|Remove from Library / Remove
// from playlist (owner).
//
// Drag & drop (issue #589): the row BODY is the drag source (press-drag
// >6px — the same gesture that drops onto sidebar playlists). A release
// INSIDE this list reorders (owner, switches the sort to Custom and
// persists the order); a release on a sidebar playlist row is the shared
// add-to-playlist drop (main.rs drag_end). The 2px accent line marks the
// insertion slot while dragging.
//
// POC-NOTEs (playlist_qt.rs has the full list): local playlists,
// custom-cover set/clear, multi-select + bulk bar, Suggested Songs,
// offline download, share — not ported.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz

Rectangle {
    id: root
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzBridge.ambientMode > 0 && QbzBridge.npHasTrack
    radius: 12

    QbzTheme { id: theme }

    readonly property var doc: parseDoc()
    function parseDoc() {
        try {
            return JSON.parse(QbzBridge.playlistJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property var allTracks: doc.tracks || []
    readonly property bool isOwner: doc.isOwner === true
    readonly property bool loading: doc.loading === true
    readonly property string sortField: doc.sortField || "default"
    readonly property bool sortAsc: doc.sortAsc === true
    readonly property string searchQuery: doc.search || ""

    // Visible rows after the in-playlist search filter.
    readonly property var tracks: {
        if (searchQuery.trim() === "") return allTracks
        const needle = searchQuery.trim().toLowerCase()
        return allTracks.filter(function (t) {
            return (t.title || "").toLowerCase().indexOf(needle) >= 0
                || (t.artist || "").toLowerCase().indexOf(needle) >= 0
                || (t.album || "").toLowerCase().indexOf(needle) >= 0
        })
    }

    function sortLabel() {
        return sortField === "title" ? QbzBridge.tr("Title", QbzBridge.trRev)
            : sortField === "artist" ? QbzBridge.tr("Artist", QbzBridge.trRev)
            : sortField === "album" ? QbzBridge.tr("Album", QbzBridge.trRev)
            : sortField === "duration" ? QbzBridge.tr("Duration", QbzBridge.trRev)
            : sortField === "added" ? QbzBridge.tr("Date added", QbzBridge.trRev)
            : sortField === "custom" ? QbzBridge.tr("Custom", QbzBridge.trRev)
            : QbzBridge.tr("Default", QbzBridge.trRev)
    }

    // --- Reorder state (the view-level half of the shared drag) ----------
    property int reorderFrom: -1
    property int reorderOver: -1
    property string reorderDropPlaylist: ""

    // The guarded mirror (PlaylistView.slint: Rust clears over-playlist-id
    // in the same turn it clears active, so the value must be captured
    // WHILE the drag is live).
    Connections {
        target: QbzBridge
        function onDragOverPlaylistIdChanged() {
            if (QbzBridge.dragActive) root.reorderDropPlaylist = QbzBridge.dragOverPlaylistId
        }
        function onDragXChanged() { root.updateSlot() }
        function onDragYChanged() { root.updateSlot() }
        function onDragActiveChanged() {
            if (QbzBridge.dragActive) return
            // Drag ended (main.rs drag_end already ran — it handled a
            // sidebar-playlist drop, if any). A release INSIDE the list and
            // NOT on a sidebar playlist reorders.
            if (root.reorderFrom >= 0
                && root.reorderDropPlaylist === ""
                && root.pointerInList()
                && root.slotFromPointer() !== root.reorderFrom
                && root.slotFromPointer() !== root.reorderFrom + 1) {
                QbzBridge.playlistReorder(root.reorderFrom, root.slotFromPointer())
            }
            root.reorderFrom = -1
            root.reorderOver = -1
            root.reorderDropPlaylist = ""
        }
    }
    function pointerInList() {
        if (!trackList) return false
        const tl = trackList.mapToItem(null, 0, 0)
        const br = trackList.mapToItem(null, trackList.width, trackList.height)
        return QbzBridge.dragX >= tl.x && QbzBridge.dragX <= br.x
            && QbzBridge.dragY >= tl.y && QbzBridge.dragY <= br.y
    }
    function slotFromPointer() {
        const tl = trackList.mapToItem(null, 0, 0)
        return Math.max(0, Math.min(tracks.length,
            Math.round((QbzBridge.dragY - tl.y + trackList.contentY) / 50)))
    }
    function updateSlot() {
        if (root.reorderFrom >= 0 && QbzBridge.dragActive) {
            root.reorderOver = root.pointerInList() ? root.slotFromPointer() : -1
        }
    }

    // Read more → the shared AppShell text modal (phase 16 pattern).
    function openDescription() {
        var shell = root.parent
        while (shell && shell.openTextModal === undefined) shell = shell.parent
        if (shell) shell.openTextModal(doc.name || "", doc.description || "")
    }

    // --- Circular header action (CircleAction, on-surface variant) -------


    // ============================ the view ================================
    Column {
        anchors.fill: parent
        anchors.leftMargin: 32
        anchors.rightMargin: 16
        anchors.topMargin: 11
        anchors.bottomMargin: 16
        spacing: 0

        Item { width: 1; height: 22 }

        // --- Header ---------------------------------------------------------
        Row {
            width: parent.width
            spacing: 24

            // Cover.
            Rectangle {
                width: 150
                height: 150
                radius: theme.radiusMd
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    visible: (doc.coverPath || "") !== ""
                    anchors.fill: parent
                    source: doc.coverPath || ""
                    radius: theme.radiusMd
                }
                QbzIcon {
                    visible: (doc.coverPath || "") === ""
                    name: "list-music"
                    width: 56
                    height: 56
                    anchors.centerIn: parent
                    tintName: "muted"
                }
            }

            // Metadata.
            Column {
                width: parent.width - 150 - 24
                spacing: 0
                Text {
                    text: QbzBridge.tr("Playlist", QbzBridge.trRev).toUpperCase()
                    color: theme.textMuted
                    font.pixelSize: 11
                    font.weight: theme.weightSemibold
                    font.letterSpacing: 1.5
                }
                Item { width: 1; height: 4 }
                Text {
                    width: parent.width
                    text: doc.name || ""
                    color: theme.textPrimary
                    font.pixelSize: theme.fontSection
                    font.weight: theme.weightBold
                    wrapMode: Text.WordWrap
                }
                Item { visible: (doc.descriptionShort || "") !== ""; width: 1; height: 6 }
                Text {
                    visible: (doc.descriptionShort || "") !== ""
                    width: Math.min(parent.width, 700)
                    text: doc.descriptionShort || ""
                    color: theme.textSecondary
                    font.pixelSize: theme.fontLegal
                    wrapMode: Text.WordWrap
                }
                Text {
                    visible: (doc.description || "") !== (doc.descriptionShort || "")
                    height: 18
                    text: QbzBridge.tr("Read more", QbzBridge.trRev)
                    color: rmArea.containsMouse ? theme.accentHover : theme.accent
                    font.pixelSize: theme.fontLegal
                    verticalAlignment: Text.AlignVCenter
                    MouseArea {
                        id: rmArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.openDescription()
                    }
                }
                Item { width: 1; height: 8 }
                Text {
                    text: (doc.owner || "") + "  •  " + (doc.trackCount || 0) + " " + QbzBridge.tr("tracks", QbzBridge.trRev)
                        + "  •  " + (doc.totalDuration || "")
                    color: theme.textSecondary
                    font.pixelSize: theme.fontLegal
                }
                Item { width: 1; height: 14 }
                // Actions — buttons left, search + sort floating right.
                Row {
                    width: parent.width
                    spacing: 12
                    QbzCircleAction {
                        name: "play-fill"
                        primary: true
                        btnEnabled: root.allTracks.length > 0
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistPlayAll()
                    }
                    QbzCircleAction {
                        name: "shuffle"
                        btnEnabled: root.allTracks.length > 0
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistShuffle()
                    }
                    QbzCircleAction {
                        name: (doc.isFavorite === true) ? "heart-filled" : "heart"
                        active: doc.isFavorite === true
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistToggleFavorite()
                    }
                    QbzCircleAction {
                        name: (doc.pinned === true) ? "pin-filled" : "pin"
                        active: doc.pinned === true
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistTogglePin()
                    }
                    QbzCircleAction {
                        visible: !root.isOwner
                        name: (doc.isFollowing === true) ? "check" : "user-plus"
                        active: doc.isFollowing === true
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistToggleFollow()
                    }
                    QbzCircleAction {
                        visible: !root.isOwner && doc.isCopied !== true
                        name: "copy"
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistCopy()
                    }
                    QbzCircleAction {
                        visible: root.isOwner
                        name: "pen-line"
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: editModal.open()
                    }

                    Item { width: parent.width - 40 - 32 * 6 - 7 * 12 - 220 - 140; height: 1 }

                    // In-playlist search.
                    Rectangle {
                        width: 220
                        height: 30
                        radius: 6
                        anchors.verticalCenter: parent.verticalCenter
                        color: theme.surfaceElevated
                        border.width: 1
                        border.color: theme.borderSubtle
                        Row {
                            anchors.fill: parent
                            anchors.leftMargin: 9
                            anchors.rightMargin: 9
                            spacing: 6
                            QbzIcon { name: "search"; width: 13; height: 13; anchors.verticalCenter: parent.verticalCenter; tintName: "muted" }
                            TextInput {
                                width: parent.width - 19
                                height: parent.height
                                color: theme.textPrimary
                                font.pixelSize: 13
                                verticalAlignment: Text.AlignVCenter
                                clip: true
                                onTextEdited: QbzBridge.playlistSetSearch(text)
                                Text {
                                    visible: parent.text === ""
                                    anchors.fill: parent
                                    text: QbzBridge.tr("Search tracks", QbzBridge.trRev)
                                    color: theme.textMuted
                                    font.pixelSize: 13
                                    verticalAlignment: Text.AlignVCenter
                                }
                            }
                        }
                    }
                    // Sort dropdown.
                    Rectangle {
                        width: 132
                        height: 34
                        radius: 6
                        anchors.verticalCenter: parent.verticalCenter
                        color: sortArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                        border.width: 1
                        border.color: theme.borderSubtle
                        Row {
                            anchors.fill: parent
                            anchors.leftMargin: 11
                            anchors.rightMargin: 8
                            spacing: 5
                            Text {
                                width: parent.width - 13 - 5
                                anchors.verticalCenter: parent.verticalCenter
                                text: QbzBridge.tr("Sort", QbzBridge.trRev) + ": " + root.sortLabel()
                                color: theme.textSecondary
                                font.pixelSize: theme.fontLegal
                                elide: Text.ElideRight
                            }
                            QbzIcon { name: "chevron-down"; width: 13; height: 13; anchors.verticalCenter: parent.verticalCenter; tintName: "muted" }
                        }
                        MouseArea {
                            id: sortArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: sortMenu.openBelowRight(sortArea)
                        }
                        QbzContextMenu {
                            id: sortMenu
                            menuWidth: 172
                            Repeater {
                                model: [
                                    { "field": "default", "label": QbzBridge.tr("Default", QbzBridge.trRev) },
                                    { "field": "title", "label": QbzBridge.tr("Title", QbzBridge.trRev) },
                                    { "field": "artist", "label": QbzBridge.tr("Artist", QbzBridge.trRev) },
                                    { "field": "album", "label": QbzBridge.tr("Album", QbzBridge.trRev) },
                                    { "field": "duration", "label": QbzBridge.tr("Duration", QbzBridge.trRev) },
                                    { "field": "added", "label": QbzBridge.tr("Date added", QbzBridge.trRev) },
                                    { "field": "custom", "label": QbzBridge.tr("Custom", QbzBridge.trRev), "ownerOnly": true },
                                ]
                                delegate: Rectangle {
                                    required property var modelData
                                    visible: modelData.ownerOnly !== true || root.isOwner
                                    width: parent ? parent.width : 0
                                    height: visible ? 33 : 0
                                    radius: 5
                                    color: soArea.containsMouse ? theme.surfaceHover : "transparent"
                                    Row {
                                        anchors.fill: parent
                                        anchors.leftMargin: 8
                                        spacing: 6
                                        Text {
                                            width: parent.width - 26
                                            height: parent.height
                                            text: modelData.label
                                            color: theme.textSecondary
                                            font.pixelSize: 13
                                            font.weight: root.sortField === modelData.field ? theme.weightSemibold : theme.weightRegular
                                            verticalAlignment: Text.AlignVCenter
                                        }
                                        QbzIcon {
                                            visible: root.sortField === modelData.field
                                            name: root.sortAsc ? "chevron-up" : "chevron-down"
                                            width: 12
                                            height: 12
                                            anchors.verticalCenter: parent.verticalCenter
                                            tintName: "accent"
                                        }
                                    }
                                    MouseArea {
                                        id: soArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: {
                                            sortMenu.close()
                                            QbzBridge.playlistSetSort(modelData.field)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Item { width: 1; height: 18 }

        // Loading / empty.
        QbzSpinner {
            visible: root.loading
            anchors.horizontalCenter: parent.horizontalCenter
            size: 36
        }
        Text {
            visible: !root.loading && root.allTracks.length === 0
            text: QbzBridge.tr("This playlist is empty.", QbzBridge.trRev)
            color: theme.textMuted
            font.pixelSize: theme.fontBody
        }

        // --- Column header ---------------------------------------------------
        Row {
            visible: root.tracks.length > 0
            width: parent.width
            anchors.leftMargin: 12
            spacing: 14
            Text { text: "#"; width: 32; color: theme.textMuted; font.pixelSize: theme.fontLegal; horizontalAlignment: Text.AlignHCenter }
            Rectangle { width: 36; height: 1; color: "transparent" }
            Text { text: QbzBridge.tr("Title", QbzBridge.trRev); width: parent.width - 32 - 36 - 220 - 70 - 92 - 28 - 28 - 32 - 8 * 14; color: theme.textMuted; font.pixelSize: theme.fontLegal }
            Text { text: QbzBridge.tr("Album", QbzBridge.trRev); width: 220; color: theme.textMuted; font.pixelSize: theme.fontLegal }
            Text { text: QbzBridge.tr("Duration", QbzBridge.trRev); width: 70; color: theme.textMuted; font.pixelSize: theme.fontLegal; horizontalAlignment: Text.AlignHCenter }
            Text { text: QbzBridge.tr("Quality", QbzBridge.trRev); width: 92; color: theme.textMuted; font.pixelSize: theme.fontLegal; horizontalAlignment: Text.AlignHCenter }
            Rectangle { width: 28; height: 1; color: "transparent" }
            Rectangle { width: 28; height: 1; color: "transparent" }
            Rectangle { width: 32; height: 1; color: "transparent" }
        }
        Rectangle { visible: root.tracks.length > 0; width: 1; height: 3; color: "transparent" }
        Rectangle { visible: root.tracks.length > 0; width: parent.width; height: 1; color: theme.borderSubtle }
        Item { width: 1; height: 6 }

        // --- Track list -------------------------------------------------------
        Item {
            width: parent.width
            height: parent.height - 28 - 150 - 18 - 50

            ListView {
                id: trackList
                anchors.fill: parent
                anchors.rightMargin: 14
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                cacheBuffer: 500
                model: root.tracks
                delegate: TrackRow {
                    required property var modelData
                    required property int index
                    width: parent ? parent.width : 0
                    item: modelData
                    number: index + 1
                    showArtwork: true
                    showAlbum: true
                    showDownload: true
                    menuShowRemove: root.isOwner
                    onPlayRequested: QbzBridge.playlistPlayTrack(item.id)
                    onEnqueueRequested: function (m) { QbzBridge.playlistEnqueueTrack(item.id, m) }
                    onRemoveRequested: QbzBridge.playlistRemoveTrack(item.playlistTrackId)
                    onBodyDragStarted: function (n) {
                        // #589: report the source index BEFORE the shared drag.
                        if (root.isOwner) {
                            root.reorderFrom = index
                            root.reorderOver = -1
                            root.reorderDropPlaylist = ""
                        }
                    }
                }
            }

            // Drop indicator: a 2px accent line at the insertion slot,
            // hidden on the no-op slots and while outside the list.
            Rectangle {
                visible: root.reorderFrom >= 0
                    && root.reorderOver >= 0
                    && root.reorderOver !== root.reorderFrom
                    && root.reorderOver !== root.reorderFrom + 1
                x: 0
                y: Math.max(0, Math.min(parent.height - 2,
                    root.reorderOver * 50 - trackList.contentY - 1))
                width: parent.width - 14
                height: 2
                radius: 1
                color: theme.accent
            }

            QbzScrollBar {
                target: trackList
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
            }
        }
    }

    // --- Edit modal (rename + delete; owner) ---------------------------------
    Popup {
        id: editModal
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: 380
        padding: 20
        closePolicy: Popup.CloseOnEscape
        background: Rectangle {
            color: theme.surfaceCard
            radius: theme.radiusMd
            border.width: 1
            border.color: theme.borderSubtle
        }
        contentItem: Column {
            spacing: 14
            Text {
                text: QbzBridge.tr("Edit playlist", QbzBridge.trRev)
                color: theme.textPrimary
                font.pixelSize: theme.fontHeading
                font.weight: theme.weightSemibold
            }
            Rectangle {
                width: parent.width
                height: 36
                radius: 6
                color: theme.surfaceElevated
                border.width: 1
                border.color: theme.borderSubtle
                TextInput {
                    id: nameInput
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 10
                    text: root.doc.name || ""
                    color: theme.textPrimary
                    font.pixelSize: 14
                    verticalAlignment: Text.AlignVCenter
                    clip: true
                }
            }
            Row {
                spacing: 10
                anchors.right: parent.right
                Rectangle {
                    width: delText.implicitWidth + 28
                    height: 34
                    radius: 6
                    color: delArea.containsMouse ? "#33ef4444" : "transparent"
                    border.width: 1
                    border.color: "#66ef4444"
                    Text {
                        id: delText
                        anchors.centerIn: parent
                        text: QbzBridge.tr("Delete", QbzBridge.trRev)
                        color: "#ef4444"
                        font.pixelSize: 13
                        font.weight: theme.weightMedium
                    }
                    MouseArea {
                        id: delArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            editModal.close()
                            QbzBridge.playlistDelete()
                        }
                    }
                }
                Rectangle {
                    width: saveText.implicitWidth + 28
                    height: 34
                    radius: 6
                    color: saveArea.containsMouse ? theme.accentHover : theme.accent
                    Text {
                        id: saveText
                        anchors.centerIn: parent
                        text: QbzBridge.tr("Save", QbzBridge.trRev)
                        color: "#ffffff"
                        font.pixelSize: 13
                        font.weight: theme.weightMedium
                    }
                    MouseArea {
                        id: saveArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            editModal.close()
                            QbzBridge.playlistRename(nameInput.text)
                        }
                    }
                }
            }
        }
    }
}
