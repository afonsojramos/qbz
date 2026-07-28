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
        return sortField === "title" ? QbzBridge.tr("Title")
            : sortField === "artist" ? QbzBridge.tr("Artist")
            : sortField === "album" ? QbzBridge.tr("Album")
            : sortField === "duration" ? QbzBridge.tr("Duration")
            : sortField === "added" ? QbzBridge.tr("Date added")
            : sortField === "custom" ? QbzBridge.tr("Custom")
            : QbzBridge.tr("Default")
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
    component CircleBtn: Rectangle {
        property string name: ""
        property bool active: false
        property bool primary: false
        property bool btnEnabled: true
        signal clicked()
        width: primary ? 40 : 32
        height: primary ? 40 : 32
        radius: width / 2
        color: primary ? (cbArea.containsMouse && btnEnabled ? theme.accentHover : theme.accent)
             : (cbArea.containsMouse || active) ? theme.surfaceHover : theme.surfaceElevated
        border.width: primary ? 0 : 1.5
        border.color: theme.borderMuted
        opacity: btnEnabled ? 1.0 : 0.4
        QbzIcon {
            name: parent.name
            width: primary ? 20 : 15
            height: primary ? 20 : 15
            anchors.centerIn: parent
            tintName: parent.primary ? "black" : (parent.active ? "accent" : "primary")
        }
        MouseArea {
            id: cbArea
            anchors.fill: parent
            enabled: parent.btnEnabled
            hoverEnabled: true
            cursorShape: parent.btnEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: parent.clicked()
        }
    }

    // --- Track row (PlaylistView TrackRow: # / 36 art / title+artist /
    // Album link / Duration / Quality / heart / cloud reserve / ⋯) -------
    component PlTrackRow: Rectangle {
        required property var modelData
        required property int index
        width: parent ? parent.width : 0
        height: 50
        radius: 8
        color: rowArea.containsMouse ? theme.surfaceHover : "transparent"

        // Shared drag (the row BODY is the source — no grip). Starts past
        // 6px of press-drag; the release either reorders (in-list, owner)
        // or adds to the sidebar target (main.rs drag_end).
        property bool dragging: false
        property point downPos: Qt.point(0, 0)

        MouseArea {
            id: rowArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onPressed: function (mouse) {
                parent.downPos = Qt.point(mouse.x, mouse.y)
            }
            onPositionChanged: function (mouse) {
                if (!pressed) return
                const g = mapToItem(null, mouse.x, mouse.y)
                if (!parent.dragging
                    && (Math.abs(mouse.x - parent.downPos.x) > 6
                        || Math.abs(mouse.y - parent.downPos.y) > 6)) {
                    parent.dragging = true
                    // body-drag-started (issue #589): report the source
                    // index BEFORE the shared drag starts.
                    if (root.isOwner) {
                        root.reorderFrom = index
                        root.reorderOver = -1
                        root.reorderDropPlaylist = ""
                    }
                    QbzBridge.dragStart(modelData.id, modelData.title,
                        modelData.artist + " · " + modelData.album, g.x, g.y)
                }
                if (parent.dragging) {
                    QbzBridge.dragMove(g.x, g.y)
                }
            }
            onReleased: function (mouse) {
                if (parent.dragging) {
                    QbzBridge.dragEnd()
                    parent.dragging = false
                } else {
                    QbzBridge.playlistPlayTrack(modelData.id)
                }
            }
        }

        Row {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 14
            // # / hover play.
            Rectangle {
                width: 32
                height: parent.height
                color: "transparent"
                Text {
                    visible: !rowArea.containsMouse
                    anchors.centerIn: parent
                    text: index + 1
                    color: theme.textMuted
                    font.pixelSize: 13
                }
                Rectangle {
                    visible: rowArea.containsMouse
                    anchors.centerIn: parent
                    width: 28
                    height: 28
                    radius: 14
                    color: "#3dffffff"
                    QbzIcon { name: "play-fill"; width: 14; height: 14; anchors.centerIn: parent; tintName: "primary" }
                }
            }
            // 36px artwork cell.
            Rectangle {
                width: 36
                height: 36
                anchors.verticalCenter: parent.verticalCenter
                radius: 4
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    source: modelData.artPath || ""
                    radius: 4
                }
            }
            // Title (+ explicit) / artist.
            Column {
                width: parent.width - 32 - 36 - 220 - 70 - 92 - 28 - 28 - 32 - 8 * 14
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Row {
                    spacing: 6
                    Text {
                        width: Math.min(implicitWidth, parent.parent.width - (modelData.explicit ? 22 : 0))
                        text: modelData.title
                        color: theme.textPrimary
                        font.pixelSize: 14
                        font.weight: theme.weightMedium
                        elide: Text.ElideRight
                    }
                    Rectangle {
                        visible: modelData.explicit
                        width: 16
                        height: 16
                        radius: 3
                        anchors.verticalCenter: parent.verticalCenter
                        color: theme.surfaceElevated
                        Text { anchors.centerIn: parent; text: "E"; color: theme.textMuted; font.pixelSize: 10; font.weight: theme.weightSemibold }
                    }
                }
                Text {
                    width: parent.width
                    text: modelData.artist
                    color: theme.textMuted
                    font.pixelSize: 12
                    elide: Text.ElideRight
                }
            }
            // Album (link).
            Text {
                width: 220
                anchors.verticalCenter: parent.verticalCenter
                text: modelData.album
                color: albumArea.containsMouse ? theme.accent : theme.textMuted
                font.pixelSize: 12
                elide: Text.ElideRight
                MouseArea {
                    id: albumArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: modelData.albumId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: if (modelData.albumId !== "") QbzBridge.openAlbum(modelData.albumId)
                }
            }
            Text {
                width: 70
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignHCenter
                text: modelData.duration
                color: theme.textMuted
                font.pixelSize: 12
            }
            Rectangle {
                width: 92
                height: parent.height
                color: "transparent"
                Row {
                    anchors.centerIn: parent
                    spacing: 6
                    Image {
                        visible: modelData.qualityTier === "hires"
                        source: "assets/hi-res.svg"
                        width: 42
                        height: 28
                        anchors.verticalCenter: parent.verticalCenter
                        sourceSize: Qt.size(84, 56)
                        fillMode: Image.PreserveAspectFit
                    }
                    Rectangle {
                        visible: modelData.qualityTier === "cd"
                        width: 30
                        height: 30
                        radius: 3
                        color: theme.surfaceElevated
                        border.width: 1
                        border.color: theme.borderSubtle
                        QbzIcon { name: "cd"; width: 16; height: 16; anchors.centerIn: parent; tintName: "muted" }
                    }
                }
            }
            // Favorite.
            Rectangle {
                width: 28
                height: 28
                radius: 6
                anchors.verticalCenter: parent.verticalCenter
                color: heartArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon {
                    name: modelData.isFavorite ? "heart-filled" : "heart"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: modelData.isFavorite ? "favorite" : "muted"
                }
                MouseArea {
                    id: heartArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        modelData.isFavorite = !modelData.isFavorite
                        QbzBridge.libraryToggleFavorite("track", modelData.id)
                    }
                }
            }
            // Cloud reserve (offline-download column — not ported; the
            // Slint reserves the slot so the grid stays aligned).
            Item { width: 28; height: 28 }
            // ⋯ menu.
            Rectangle {
                width: 32
                height: 32
                radius: 6
                anchors.verticalCenter: parent.verticalCenter
                color: menuArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon { name: "ellipsis"; width: 15; height: 15; anchors.centerIn: parent; tintName: "secondary" }
                MouseArea {
                    id: menuArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: function (mouse) { rowMenu.openAtCursor(menuArea, mouse.x, mouse.y) }
                }
            }
        }
        QbzContextMenu {
            id: rowMenu
            menuWidth: 220
            Repeater {
                model: [
                    { "label": QbzBridge.tr("Play"), "icon": "play-fill", "action": "play" },
                    { "label": QbzBridge.tr("Play next"), "icon": "list-start", "action": "next" },
                    { "label": QbzBridge.tr("Play later"), "icon": "list-plus", "action": "later" },
                    { "label": QbzBridge.tr("Add to queue"), "icon": "list-end", "action": "queue" },
                    { "label": QbzBridge.tr("Go to artist"), "icon": "user", "action": "go-artist", "show": modelData.artistId !== "" },
                    { "label": QbzBridge.tr("Go to album"), "icon": "disc", "action": "go-album", "show": modelData.albumId !== "" },
                    { "label": modelData.isFavorite ? QbzBridge.tr("Remove from Library") : QbzBridge.tr("Add to Library"),
                      "icon": modelData.isFavorite ? "heart-filled" : "heart", "action": "favorite" },
                    { "label": QbzBridge.tr("Remove from playlist"), "icon": "trash-2", "action": "remove", "show": root.isOwner },
                ]
                delegate: Rectangle {
                    required property var modelData
                    visible: modelData.show === undefined || modelData.show === true
                    width: parent ? parent.width : 0
                    height: visible ? 33 : 0
                    radius: 5
                    color: rmiArea.containsMouse ? theme.surfaceHover : "transparent"
                    Row {
                        anchors.fill: parent
                        anchors.leftMargin: 8
                        spacing: 8
                        QbzIcon { name: modelData.icon; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                        Text {
                            height: parent.height
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
                            var row = parent.parent.parent.modelData
                            if (a === "play") QbzBridge.playlistPlayTrack(row.id)
                            else if (a === "next") QbzBridge.playlistEnqueueTrack(row.id, "next")
                            else if (a === "later") QbzBridge.playlistEnqueueTrack(row.id, "later")
                            else if (a === "queue") QbzBridge.playlistEnqueueTrack(row.id, "queue")
                            else if (a === "go-artist") QbzBridge.openArtist(row.artistId)
                            else if (a === "go-album") QbzBridge.openAlbum(row.albumId)
                            else if (a === "favorite") {
                                row.isFavorite = !row.isFavorite
                                QbzBridge.libraryToggleFavorite("track", row.id)
                            }
                            else if (a === "remove") QbzBridge.playlistRemoveTrack(row.playlistTrackId)
                        }
                    }
                }
            }
        }
    }

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
                    text: QbzBridge.tr("Playlist").toUpperCase()
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
                    text: QbzBridge.tr("Read more")
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
                    text: (doc.owner || "") + "  •  " + (doc.trackCount || 0) + " " + QbzBridge.tr("tracks")
                        + "  •  " + (doc.totalDuration || "")
                    color: theme.textSecondary
                    font.pixelSize: theme.fontLegal
                }
                Item { width: 1; height: 14 }
                // Actions — buttons left, search + sort floating right.
                Row {
                    width: parent.width
                    spacing: 12
                    CircleBtn {
                        name: "play-fill"
                        primary: true
                        btnEnabled: root.allTracks.length > 0
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistPlayAll()
                    }
                    CircleBtn {
                        name: "shuffle"
                        btnEnabled: root.allTracks.length > 0
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistShuffle()
                    }
                    CircleBtn {
                        name: (doc.isFavorite === true) ? "heart-filled" : "heart"
                        active: doc.isFavorite === true
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistToggleFavorite()
                    }
                    CircleBtn {
                        name: (doc.pinned === true) ? "pin-filled" : "pin"
                        active: doc.pinned === true
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistTogglePin()
                    }
                    CircleBtn {
                        visible: !root.isOwner
                        name: (doc.isFollowing === true) ? "check" : "user-plus"
                        active: doc.isFollowing === true
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistToggleFollow()
                    }
                    CircleBtn {
                        visible: !root.isOwner && doc.isCopied !== true
                        name: "copy"
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistCopy()
                    }
                    CircleBtn {
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
                                    text: QbzBridge.tr("Search tracks")
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
                                text: QbzBridge.tr("Sort") + ": " + root.sortLabel()
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
                                    { "field": "default", "label": QbzBridge.tr("Default") },
                                    { "field": "title", "label": QbzBridge.tr("Title") },
                                    { "field": "artist", "label": QbzBridge.tr("Artist") },
                                    { "field": "album", "label": QbzBridge.tr("Album") },
                                    { "field": "duration", "label": QbzBridge.tr("Duration") },
                                    { "field": "added", "label": QbzBridge.tr("Date added") },
                                    { "field": "custom", "label": QbzBridge.tr("Custom"), "ownerOnly": true },
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
            text: QbzBridge.tr("This playlist is empty.")
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
            Text { text: QbzBridge.tr("Title"); width: parent.width - 32 - 36 - 220 - 70 - 92 - 28 - 28 - 32 - 8 * 14; color: theme.textMuted; font.pixelSize: theme.fontLegal }
            Text { text: QbzBridge.tr("Album"); width: 220; color: theme.textMuted; font.pixelSize: theme.fontLegal }
            Text { text: QbzBridge.tr("Duration"); width: 70; color: theme.textMuted; font.pixelSize: theme.fontLegal; horizontalAlignment: Text.AlignHCenter }
            Text { text: QbzBridge.tr("Quality"); width: 92; color: theme.textMuted; font.pixelSize: theme.fontLegal; horizontalAlignment: Text.AlignHCenter }
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
                delegate: PlTrackRow { }
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
                text: QbzBridge.tr("Edit playlist")
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
                        text: QbzBridge.tr("Delete")
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
                        text: QbzBridge.tr("Save")
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
