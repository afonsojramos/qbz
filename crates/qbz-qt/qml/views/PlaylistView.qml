// Playlist detail view — the QML port of playlist/PlaylistView.slint
// (phase 17). ONE JSON document (QbzBridge.playlistJson, playlist_qt.rs
// PlaylistDoc: header + track rows + ownership/follow/pin/sort/search).
//
// Header: 150px cover (the playlist's OWN Qobuz graphic contain-fitted,
// else the member-cover mosaic via the shared cards/PlaylistCollage.qml,
// which also supplies the placeholder glyph),
// "PLAYLIST" eyebrow, name, description-short + Read
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
// LOADING: header + track-row QbzSkeleton placeholders in the shape of what
// is coming (the Slint mounts a bare LoadingSpinner), plus a per-item cover
// placeholder that clears when the playlist's own cover lands.
//
// POC-NOTEs (playlist_qt.rs has the full list): local playlists,
// custom-cover set/clear, multi-select + bulk bar, Suggested Songs,
// offline download, share — not ported.

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import com.blitzfc.qbz
import "../cards"
import "../controls"
import "../rows"
import "../theme"

Rectangle {
    id: root
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
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

    // --- Settled state from Rust (`libraryFavoriteChanged` / `pinChanged`) -
    // Two jobs, and the first one is why this exists at all.
    //
    // TRACK ROWS. The list is a ListView with cacheBuffer 500 — it RECYCLES.
    // A row's heart lives on the delegate (rows/TrackRow.qml), so a delegate
    // that is destroyed and rebuilt reads its state back out of `modelData`,
    // i.e. out of `allTracks`. Without patching that array, favouriting a
    // track and scrolling past it and back showed the OLD glyph again — and
    // `playlist_qt` is one of the page-lifetime documents that main.rs's
    // `emit_library_favorite` deliberately does NOT fan out to, so nothing
    // else was going to fix it either.
    //
    // The row objects are patched IN PLACE. Re-deriving `tracks` (or
    // re-parsing `doc`) would push a NEW array into `model:`, and
    // QQuickItemView::setModel() resets the scroll offset to 0 — the user
    // would be thrown to the top of the playlist on every heart click. The
    // live delegates need no array change anyway: each one settles itself
    // from this same signal.
    //
    // BOUNDARY, stated so nobody hunts it twice: this patch lasts until the
    // NEXT republish of `QbzBridge.playlistJson`, because that hands over a
    // freshly parsed object graph. `playlist_qt` keeps its document in a Rust
    // `PAGE` cache and re-serializes it as-is (covers landing, a sort change,
    // an in-playlist search), so a republish restores the heart the rows were
    // STAMPED with. Closing that needs `playlist_qt::apply_favorite_change`
    // on the Rust side — main.rs's `emit_library_favorite` doc lists it as a
    // known gap — not more QML.
    //
    // HEADER. Two one-slot overrides so the heart and the pin also answer a
    // flip made somewhere else (a sidebar card, the Home Pinned rail) instead
    // of waiting for a document republish that those paths do not trigger.
    // They carry their OWN playlist id (one slot each, never a shared one —
    // a shared id would make a pin on playlist B resurrect playlist A's stale
    // heart), so an override expires the moment the page shows anything else.
    property string favOverrideId: ""
    property bool favOverrideValue: false
    property string pinOverrideId: ""
    property bool pinOverrideValue: false
    readonly property bool headerFavorite: (root.favOverrideId !== ""
        && root.favOverrideId === (doc.id || ""))
        ? root.favOverrideValue : (doc.isFavorite === true)
    readonly property bool headerPinned: (root.pinOverrideId !== ""
        && root.pinOverrideId === (doc.id || ""))
        ? root.pinOverrideValue : (doc.pinned === true)
    Connections {
        target: QbzLibrary
        function onLibraryFavoriteChanged(key, value) {
            if (key.indexOf("track:") === 0) {
                var tid = key.substring(6)
                var t = root.allTracks
                for (var i = 0; i < t.length; i++) {
                    if (String(t[i].id) === tid) { t[i].isFavorite = value; break }
                }
                return
            }
            var pid = root.doc.id || ""
            if (pid !== "" && key === "playlist:" + pid) {
                root.favOverrideId = pid
                root.favOverrideValue = value
            }
        }
        function onPinChanged(key, value) {
            var pid = root.doc.id || ""
            if (pid !== "" && key === "playlist:" + pid) {
                root.pinOverrideId = pid
                root.pinOverrideValue = value
            }
        }
    }

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

    // --- skeleton pulse (QbzSkeleton's preferred drive mode) -------------
    // ONE 900ms Timer drives EVERY placeholder in this view. GATING RULE:
    // freeze on NOT VISIBLE — the view hidden, or the window minimized /
    // hidden. NEVER on lost focus (a tiling desktop keeps windows visible
    // and unfocused). Same rule as HomeView / LibraryView / AmbientField.
    property bool skelPhase: false
    readonly property bool windowShowing: root.Window.window
        ? (root.Window.window.visibility !== Window.Minimized
           && root.Window.window.visibility !== Window.Hidden)
        : true
    // True when this playlist carries artwork of its OWN (`image_rectangle`
    // — editorial playlists). playlist_qt.rs publishes coverUrl/coverPath for
    // that graphic ONLY; everything else falls back to the member-cover
    // mosaic in `doc.covers`.
    readonly property bool hasOwnArt: (doc.coverUrl || "") !== ""
    // The cover is its OWN progressive item: it clears the moment the
    // playlist's own cover lands, independently of the track rows.
    readonly property bool coverPending: root.hasOwnArt
        && (doc.coverPath || "") === ""
    // The header has no data at all until the first document lands
    // (playlist_qt.rs publishes `{ loading: true }` with empty fields).
    readonly property bool headerPending: root.loading && (doc.name || "") === ""
    readonly property bool listPending: root.loading && root.allTracks.length === 0
    Timer {
        interval: 900
        repeat: true
        running: (root.headerPending || root.listPending || root.coverPending)
            && root.visible && root.windowShowing
        onTriggered: root.skelPhase = !root.skelPhase
    }

    function sortLabel() {
        return sortField === "title" ? QbzSession.tr("Title", QbzSession.trRev)
            : sortField === "artist" ? QbzSession.tr("Artist", QbzSession.trRev)
            : sortField === "album" ? QbzSession.tr("Album", QbzSession.trRev)
            : sortField === "duration" ? QbzSession.tr("Duration", QbzSession.trRev)
            : sortField === "added" ? QbzSession.tr("Date added", QbzSession.trRev)
            : sortField === "custom" ? QbzSession.tr("Custom", QbzSession.trRev)
            : QbzSession.tr("Default", QbzSession.trRev)
    }

    // --- Reorder state (the view-level half of the shared drag) ----------
    property int reorderFrom: -1
    property int reorderOver: -1
    property string reorderDropPlaylist: ""

    // The guarded mirror (PlaylistView.slint: Rust clears over-playlist-id
    // in the same turn it clears active, so the value must be captured
    // WHILE the drag is live).
    Connections {
        target: QbzShell
        function onDragOverPlaylistIdChanged() {
            if (QbzShell.dragActive) root.reorderDropPlaylist = QbzShell.dragOverPlaylistId
        }
        function onDragXChanged() { root.updateSlot() }
        function onDragYChanged() { root.updateSlot() }
        function onDragActiveChanged() {
            if (QbzShell.dragActive) return
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
        return QbzShell.dragX >= tl.x && QbzShell.dragX <= br.x
            && QbzShell.dragY >= tl.y && QbzShell.dragY <= br.y
    }
    function slotFromPointer() {
        const tl = trackList.mapToItem(null, 0, 0)
        return Math.max(0, Math.min(tracks.length,
            Math.round((QbzShell.dragY - tl.y + trackList.contentY) / 50)))
    }
    function updateSlot() {
        if (root.reorderFrom >= 0 && QbzShell.dragActive) {
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

        // Header placeholder — the SHAPE of the header that is coming
        // (150px cover + eyebrow/title/description/meta bars + the 7 round
        // action buttons), not a spinner. Replaced wholesale by the real
        // header the moment the document lands.
        QbzSkeleton {
            visible: root.headerPending
            variant: "header"
            width: parent.width
            height: 150
            coverSize: 150
            coverGap: 24
            actionCount: 7
            phase: root.skelPhase
        }

        // --- Header ---------------------------------------------------------
        Row {
            visible: !root.headerPending
            width: parent.width
            spacing: 24

            // Cover — the SAME two arms as PlaylistCard.qml (the card fix of
            // this session, applied to the detail header):
            //   1. the playlist's OWN Qobuz graphic (`image_rectangle`,
            //      published as coverUrl/coverPath) — CONTAIN, because those
            //      are landscape and cropping cuts the wordmark;
            //   2. otherwise the member-cover MOSAIC through the shared
            //      cards/PlaylistCollage.qml (no second collage), which also
            //      owns the list-music empty state.
            // `doc.covers` are MEMBER-ALBUM covers, never the playlist's own
            // artwork — binding one of them as the header image is the bug
            // this replaces.
            Rectangle {
                width: 150
                height: 150
                radius: theme.radiusMd
                color: theme.surfaceElevated
                clip: true
                // Arm 2 first: the collage is the BACKDROP. Declaration order
                // is z-order, so the resolved single cover and the skeleton
                // below must come after it.
                PlaylistCollage {
                    anchors.fill: parent
                    visible: (doc.coverPath || "") === ""
                    // A playlist that HAS its own graphic passes no urls, so
                    // it shows the placeholder while that graphic downloads
                    // instead of flashing a member mosaic (PlaylistCard rule).
                    urls: root.hasOwnArt ? [] : (doc.covers || [])
                    radius: theme.radiusMd
                }
                // `pad`, not `contain`: the playlist's own graphic is the
                // 800x380 `image_rectangle`, so a 150px square leaves two
                // ~35px bands. Flat theme grey there made the header read as
                // a small picture floating in a box; the bands now carry a
                // gradient sampled from the image's own top and bottom edges
                // (theme/RoundedImage.qml). Slint crops this same header
                // (`playlist/PlaylistView.slint:481` is `image-fit: cover`)
                // and loses half the banner — this is one of the few places
                // the reference is not the target.
                RoundedImage {
                    visible: (doc.coverPath || "") !== ""
                    anchors.fill: parent
                    source: doc.coverPath || ""
                    radius: theme.radiusMd
                    fit: root.hasOwnArt ? "pad" : "auto"
                }
                // Per-item: the cover shimmers until THIS playlist's cover
                // lands, then clears on its own. settleMs bounds it — a
                // failed download republishes the document with an empty
                // coverPath and would otherwise shimmer forever; on settle
                // the placeholder fades and the collage below takes over
                // (empty urls -> its list-music glyph, the old empty state).
                QbzSkeleton {
                    id: plCoverSkel
                    visible: root.coverPending
                    variant: "art"
                    anchors.fill: parent
                    blockRadius: theme.radiusMd
                    phase: root.skelPhase
                    settleMs: 4000
                }
            }

            // Metadata.
            Column {
                width: parent.width - 150 - 24
                spacing: 0
                Text {
                    text: QbzSession.tr("Playlist", QbzSession.trRev).toUpperCase()
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
                    text: QbzSession.tr("Read more", QbzSession.trRev)
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
                    text: (doc.owner || "") + "  •  " + (doc.trackCount || 0) + " " + QbzSession.tr("tracks", QbzSession.trRev)
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
                        name: root.headerFavorite ? "heart-filled" : "heart"
                        active: root.headerFavorite
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playlistToggleFavorite()
                    }
                    QbzCircleAction {
                        name: root.headerPinned ? "pin-filled" : "pin"
                        active: root.headerPinned
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
                                    text: QbzSession.tr("Search tracks", QbzSession.trRev)
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
                                text: QbzSession.tr("Sort", QbzSession.trRev) + ": " + root.sortLabel()
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
                                    { "field": "default", "label": QbzSession.tr("Default", QbzSession.trRev) },
                                    { "field": "title", "label": QbzSession.tr("Title", QbzSession.trRev) },
                                    { "field": "artist", "label": QbzSession.tr("Artist", QbzSession.trRev) },
                                    { "field": "album", "label": QbzSession.tr("Album", QbzSession.trRev) },
                                    { "field": "duration", "label": QbzSession.tr("Duration", QbzSession.trRev) },
                                    { "field": "added", "label": QbzSession.tr("Date added", QbzSession.trRev) },
                                    { "field": "custom", "label": QbzSession.tr("Custom", QbzSession.trRev), "ownerOnly": true },
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

        // Empty. (The loading state is the row placeholders inside the
        // track-list area below — the Slint's bare LoadingSpinner said
        // "busy" but not "this is the shape of what is coming".)
        Text {
            visible: !root.loading && root.allTracks.length === 0
            text: QbzSession.tr("This playlist is empty.", QbzSession.trRev)
            color: theme.textMuted
            font.pixelSize: theme.fontBody
        }

        // --- Column header ---------------------------------------------------
        // rows/TrackListHeader.qml — the same rows/TrackCols.qml geometry the
        // TrackRows below use. Two things were wrong when this was hand-rolled
        // here, and both are now structural:
        //   * `anchors.leftMargin: 12` on a Column child is a NO-OP (a
        //     positioner owns its children's x), so the labels started 12px
        //     left of the row cells; the header component pads itself;
        //   * it was as wide as the page while the rows are the ListView's
        //     width, which is inset 14px for the scrollbar gutter — 14px of
        //     extra stretch pushed Album/Duration/Quality right. The width
        //     below is the ROW width, which is the component's contract.
        TrackListHeader {
            visible: root.tracks.length > 0
            width: parent.width - 14
            showArtwork: true
            showAlbum: true
            showDownload: true
        }
        Rectangle { visible: root.tracks.length > 0; width: 1; height: 3; color: "transparent" }
        Rectangle { visible: root.tracks.length > 0; width: parent.width; height: 1; color: theme.borderSubtle }
        Item { width: 1; height: 6 }

        // --- Track list -------------------------------------------------------
        Item {
            width: parent.width
            height: parent.height - 28 - 150 - 18 - 50

            // Track-list placeholder: the exact TrackRow footprint (50px
            // rows, no gap, 36px art cell), one instance for the whole
            // viewport — ONE animator, not one per row (see QbzSkeleton's
            // COST note). It unmounts the moment the first rows land.
            QbzSkeleton {
                visible: root.listPending
                variant: "rowList"
                anchors.fill: parent
                anchors.rightMargin: 14
                rowH: 50
                rowGap: 0
                rowArtSize: 36
                phase: root.skelPhase
            }

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
                    // MyQBZ "Add to mixtape" — the HOST builds the AddItem
                    // array (TrackRow does not know itemType/source).
                    //
                    // SOURCE: `PlaylistTrackRow` (playlist_qt.rs:47-78) has no
                    // source field because this whole view has one source —
                    // LOCAL playlists are out of scope everywhere in it
                    // (playlist_qt.rs:23-25), the document is mapped from
                    // Qobuz `Track` values, and `playlistTrackId` is the
                    // Qobuz membership row id. `item.id` is a Qobuz catalog
                    // id by construction of the view, not by assumption here.
                    // If local playlists are ever ported, the row must start
                    // carrying its source and this must read it.
                    onMixtapeRequested: QbzMyQbzAdd.open(JSON.stringify([{
                        "itemType": "track", "source": "qobuz",
                        "sourceItemId": item.id, "title": item.title || "",
                        "subtitle": item.artist || "", "artworkUrl": "",
                        "year": null, "trackCount": null
                    }]))
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
                text: QbzSession.tr("Edit playlist", QbzSession.trRev)
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
                        text: QbzSession.tr("Delete", QbzSession.trRev)
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
                        text: QbzSession.tr("Save", QbzSession.trRev)
                        // Was a literal #ffffff, 1:1 with
                        // primitives/EditPlaylistModal.slint:171 — and that
                        // white is 1.70:1 on high-contrast, 1.74 on ikari,
                        // 1.82 on wcag-dark, under 2.6:1 on 16 of the 35
                        // palettes, on the SAME accent fill every other
                        // on-accent site was just fixed against. The colour
                        // twin of the glyph selector; see
                        // theme/QbzTheme.qml, "ON AN ACCENT FILL".
                        color: theme.accentGlyphColor
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
