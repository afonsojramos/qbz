// Album detail page — QML port of album/AlbumPageView.slint.
//
// Header (224px cover, title, credited-artist line, meta with label link,
// description + Read more, CircleAction row), divider, toolbar (quality
// badge + track search), column header, track list (Disc/work headers,
// TrackRow replica with the playing-row pill + number↔play cell + live
// heart), label/awards sidebar, and the two bottom carousels ("From the
// same artist", "Listening suggestions").
//
// POC-NOTEs: multi-select + bulk bar, offline download column, booklet,
// external links, custom-cover menu, album-info modal, the header
// atmosphere (album-header-gradient pref) — out of scope (visible stubs
// are inert). Text keeps the theme colors (header-light = false here).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz

Rectangle {
    id: root
    // Transparent while the ambient background is active (phase 14 —
    // HomeView.slint:163: the frosted content panel shows through).
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzBridge.ambientMode > 0 && QbzBridge.npHasTrack
       
    // Round to the AppShell content-frame bezel (Radius.md): QML clips
    // are rectangular, so the frame's own rounding never reaches the
    // view — the view's own fill must round instead.
    radius: 12

    QbzTheme { id: theme }

    // The view's album + url-keyed cover map (artwork pipeline).
    readonly property var album: JSON.parse(QbzBridge.albumJson)
    readonly property var header: album.header || ({})
    readonly property var tracks: album.tracks || []
    readonly property var awards: album.awards || []
    property var coverMap: ({})
    // Client-side track search (AlbumActions.search equivalent).
    property string trackQuery: ""
    property bool showDescription: false

    readonly property var visibleTracks: {
        if (trackQuery === "") return tracks
        var q = trackQuery.toLowerCase()
        return tracks.filter(function (t) {
            return t.title.toLowerCase().indexOf(q) >= 0
        })
    }

    // Disc headers + work headers precede their first row (computed once
    // per track list change, mirroring AlbumState's disc-header-number /
    // work-header model fields).
    function headerFor(i) {
        var t = visibleTracks[i]
        if (!t) return null
        if (i === 0) {
            return { disc: t.disc > 1 || (visibleTracks.length > 0 && visibleTracks[visibleTracks.length - 1].disc > 1) ? t.disc : 0,
                     work: t.workHeader }
        }
        var prev = visibleTracks[i - 1]
        var multi = visibleTracks[visibleTracks.length - 1].disc > 1
        return { disc: (multi && t.disc !== prev.disc) ? t.disc : 0,
                 work: t.workHeader !== prev.workHeader ? t.workHeader : "" }
    }

    Connections {
        target: QbzBridge
        function onLibraryArtworkReady(key, path) {
            var m = root.coverMap
            m[key] = path
            root.coverMap = Object.assign({}, m)
        }
    }
    Component.onCompleted: dispatchCovers()
    onAlbumChanged: dispatchCovers()
    // The derived binding settles AFTER onAlbumChanged fires (stale race) —
    // redispatch when the header itself updates.
    onHeaderChanged: dispatchCovers()
    function dispatchCovers() {
        var urls = []
        if (header && header.artUrl) urls.push(header.artUrl)
        var more = album.moreFromArtist || []
        for (var i = 0; i < more.length; i++) if (more[i].artUrl) urls.push(more[i].artUrl)
        var sug = album.suggestions || []
        for (i = 0; i < sug.length; i++) if (sug[i].artUrl) urls.push(sug[i].artUrl)
        if (urls.length > 0) QbzBridge.sidebarArtworkWindow(JSON.stringify(urls))
    }

    // Ghost CircleAction (secondary, on-surface variant): elevated disc,
    // strong ring, text-primary icon (accent when active).
    component CircleBtn: Rectangle {
        property string name: ""
        property bool active: false
        signal clicked(var mouse)
        width: 32
        height: 32
        radius: 16
        color: (cbArea.containsMouse || active) ? theme.surfaceHover : theme.surfaceElevated
        border.width: 1.5
        border.color: theme.borderMuted
        QbzIcon {
            name: parent.name
            width: 15
            height: 15
            anchors.centerIn: parent
            tintName: parent.active ? "accent" : "primary"
        }
        MouseArea {
            id: cbArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: function (mouse) { parent.clicked(mouse) }
        }
    }

    // Track list row (TrackRow.slint replica: number cell, no artwork,
    // favorite + download + menu columns).
    component AlbumTrackRow: Rectangle {
        property var row: ({})
        property int rowIndex: 0

        readonly property bool isActive: QbzBridge.npTrackId !== "" && QbzBridge.npTrackId === row.id
        readonly property bool hovered: trArea.containsMouse || favArea.containsMouse || moreArea.containsMouse

        width: parent ? parent.width : 0
        height: 50
        radius: 8
        color: hovered ? "#14ffffff" : (rowIndex % 2 === 1 ? "#07ffffff" : "transparent")

        // Static now-playing mark: 3px accent pill on the left edge.
        Rectangle {
            visible: isActive
            x: 2
            y: 7
            width: 3
            height: parent.height - 14
            radius: 1.5
            color: theme.accent
        }

        Row {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 14

            // Number cell — swaps to play on hover (non-artwork variant).
            Item {
                width: 32
                height: 40
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    visible: !trArea.containsMouse
                    anchors.centerIn: parent
                    text: row.number
                    color: theme.textMuted
                    font.pixelSize: 13
                }
                Rectangle {
                    visible: trArea.containsMouse
                    anchors.centerIn: parent
                    width: 28
                    height: 28
                    radius: 14
                    color: isActive && QbzBridge.npPlaying ? "transparent" : "#3dffffff"
                    border.width: isActive && QbzBridge.npPlaying ? 1.5 : 0
                    border.color: theme.accent
                    QbzIcon {
                        anchors.centerIn: parent
                        name: isActive && QbzBridge.npPlaying ? "pause" : "play-fill"
                        width: 14
                        height: 14
                        tintName: isActive && QbzBridge.npPlaying ? "accent" : "primary"
                    }
                }
            }
            // Title + artist.
            Column {
                width: parent.width - 32 - 70 - 92 - 28 - 28 - 32 - 5 * 14
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Row {
                    spacing: 6
                    Text {
                        text: row.title
                        color: theme.textPrimary
                        font.pixelSize: 14
                        font.weight: theme.weightMedium
                        elide: Text.ElideRight
                        width: Math.min(implicitWidth, parent.parent.width - (row.explicit ? 22 : 0))
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
                    visible: row.artist !== ""
                    text: row.artist
                    color: row.artistId !== "" && artistLinkArea.containsMouse ? theme.textPrimary : theme.textMuted
                    font.pixelSize: 13
                    elide: Text.ElideRight
                    MouseArea {
                        id: artistLinkArea
                        anchors.fill: parent
                        enabled: row.artistId !== ""
                        hoverEnabled: true
                        cursorShape: row.artistId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: QbzBridge.openArtist(row.artistId)
                    }
                }
            }
            Text {
                width: 70
                anchors.verticalCenter: parent.verticalCenter
                text: row.duration
                color: theme.textMuted
                font.pixelSize: 13
                horizontalAlignment: Text.AlignHCenter
            }
            // Bare quality badge (tier label + detail).
            Text {
                width: 92
                anchors.verticalCenter: parent.verticalCenter
                text: row.qualityTier === "hires" ? "HI-RES" : (row.qualityTier === "cd" ? "CD" : "")
                color: theme.textMuted
                font.pixelSize: 10
                font.weight: theme.weightBold
                horizontalAlignment: Text.AlignHCenter
            }
            // Favorite (live).
            Rectangle {
                width: 28
                height: 28
                radius: theme.radiusSm
                anchors.verticalCenter: parent.verticalCenter
                color: favArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon {
                    anchors.centerIn: parent
                    name: row.isFavorite ? "heart-filled" : "heart"
                    width: 16
                    height: 16
                    tintName: row.isFavorite ? "favorite" : (favArea.containsMouse ? "primary" : "muted")
                }
                MouseArea {
                    id: favArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        row.isFavorite = !row.isFavorite
                        QbzBridge.libraryToggleFavorite("track", row.id)
                    }
                }
            }
            // Offline download — INERT stub (out of scope).
            Rectangle {
                width: 28
                height: 28
                anchors.verticalCenter: parent.verticalCenter
                color: "transparent"
                QbzIcon {
                    anchors.centerIn: parent
                    name: "cloud-download"
                    width: 16
                    height: 16
                    tintName: "muted"
                }
            }
            // ⋯ menu.
            Rectangle {
                width: 32
                height: 32
                radius: theme.radiusSm
                anchors.verticalCenter: parent.verticalCenter
                color: moreArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon {
                    anchors.centerIn: parent
                    name: "ellipsis"
                    width: 16
                    height: 16
                    tintName: moreArea.containsMouse ? "primary" : "muted"
                }
                MouseArea {
                    id: moreArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: function (mouse) { rowMenu.openAtCursor(moreArea, mouse.x, mouse.y) }
                }
                QbzContextMenu {
                    id: rowMenu
                    menuWidth: 196
                        Repeater {
                            model: [
                                { "label": QbzBridge.tr("Play"), "icon": "play-fill", "action": "play" },
                                { "label": QbzBridge.tr("Play next"), "icon": "list-plus", "action": "next" },
                                { "label": QbzBridge.tr("Add to queue"), "icon": "list-end", "action": "queue" },
                                { "label": row.isFavorite ? QbzBridge.tr("Remove from Library") : QbzBridge.tr("Add to Library"), "icon": row.isFavorite ? "heart-filled" : "heart", "action": "favorite" },
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
                                        if (a === "play") QbzBridge.playAlbumFrom(header.id, row.id)
                                        else if (a === "next") QbzBridge.enqueueAlbumTrack(header.id, row.id, "next")
                                        else if (a === "queue") QbzBridge.enqueueAlbumTrack(header.id, row.id, "later")
                                        else if (a === "favorite") {
                                            row.isFavorite = !row.isFavorite
                                            QbzBridge.libraryToggleFavorite("track", row.id)
                                        }
                                    }
                                }
                            }
                        }
                    }
            }
        }

        MouseArea {
            id: trArea
            anchors.fill: parent
            hoverEnabled: true
            propagateComposedEvents: true
            onDoubleClicked: QbzBridge.playAlbumFrom(header.id, row.id)
            onClicked: mouse.accepted = false
        }
    }

    // Sidebar label/award card (SidebarCard).
    component SidebarCard: Rectangle {
        property string name: ""
        property color gradA: "#6366f1"
        property color gradB: "#8b5cf6"
        property string iconName: "disc"
        signal clicked()
        width: parent ? parent.width : 0
        height: 48
        radius: theme.radiusSm
        color: scArea.containsMouse ? theme.surfaceHover : "transparent"
        Row {
            anchors.fill: parent
            anchors.margins: 6
            spacing: 10
            Rectangle {
                width: 28
                height: 28
                radius: 14
                anchors.verticalCenter: parent.verticalCenter
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: gradA }
                    GradientStop { position: 1.0; color: gradB }
                }
                QbzIcon {
                    name: iconName
                    width: 13
                    height: 13
                    anchors.centerIn: parent
                    tintName: "primary"
                }
            }
            Text {
                width: parent.width - 38
                anchors.verticalCenter: parent.verticalCenter
                text: name
                color: theme.textSecondary
                font.pixelSize: 12
                font.weight: theme.weightMedium
                wrapMode: Text.WordWrap
            }
        }
        MouseArea {
            id: scArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }
    component SidebarHeading: Text {
        color: theme.textMuted
        font.pixelSize: 10
        font.weight: theme.weightSemibold
        font.letterSpacing: 1
    }

    // ============================ the page ================================
    Flickable {
        id: pageFlick
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: page.implicitHeight
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: page
            width: parent.width
            leftPadding: 32
            rightPadding: 32
            topPadding: 11
            bottomPadding: 100
            spacing: 0

            // NavButtons is a 0px placeholder in the Slint source.
            Item { width: 1; height: 22 }

            // --- Album header -------------------------------------------
            Row {
                width: parent.width - 64
                spacing: 32

                Rectangle {
                    width: 224
                    height: 224
                    radius: 12
                    color: theme.surfaceElevated
                    clip: true
                    RoundedImage {
                        anchors.fill: parent
                        source: root.coverMap[header.artUrl] || ""
                        radius: 12
                    }
                }

                Column {
                    width: parent.width - 224 - 32
                    anchors.top: parent.top
                    anchors.topMargin: 4
                    spacing: 0

                    Text {
                        width: parent.width
                        text: header.title || ""
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightBold
                        elide: Text.ElideRight
                    }
                    Item { width: 1; height: 4 }
                    // Credited-artist line (links + role suffixes).
                    Flow {
                        width: parent.width
                        spacing: 0
                        Repeater {
                            model: header.credits || []
                            delegate: Row {
                                required property var modelData
                                required property int index
                                spacing: 0
                                Text {
                                    visible: index > 0
                                    text: "  •  "
                                    color: theme.textSecondary
                                    font.pixelSize: theme.fontHeading
                                    font.weight: theme.weightBold
                                }
                                Text {
                                    text: modelData[0]
                                    color: creditArea.containsMouse && modelData[1] !== "" ? theme.textPrimary : theme.textSecondary
                                    font.pixelSize: theme.fontHeading
                                    font.weight: theme.weightBold
                                    MouseArea {
                                        id: creditArea
                                        anchors.fill: parent
                                        enabled: modelData[1] !== ""
                                        hoverEnabled: true
                                        cursorShape: modelData[1] !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                                        onClicked: QbzBridge.openArtist(modelData[1])
                                    }
                                }
                                Text {
                                    visible: modelData[2] !== ""
                                    text: " (" + modelData[2] + ")"
                                    color: theme.textSecondary
                                    font.pixelSize: theme.fontHeading
                                }
                            }
                        }
                    }
                    Item { width: 1; height: 10 }
                    // Meta line (label as a clickable link when navigable).
                    Row {
                        spacing: 0
                        visible: (header.labelId || "") !== "" && (header.label || "") !== ""
                        Text {
                            visible: (header.metaPre || "") !== ""
                            text: (header.metaPre || "") + "   •   "
                            color: theme.textSecondary
                            font.pixelSize: theme.fontBody
                        }
                        Text {
                            text: header.label || ""
                            color: labelArea.containsMouse ? theme.accent : theme.textSecondary
                            font.pixelSize: theme.fontBody
                            MouseArea {
                                id: labelArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                // POC-NOTE: no label view yet.
                            }
                        }
                        Text {
                            visible: (header.metaPost || "") !== ""
                            text: "   •   " + (header.metaPost || "")
                            color: theme.textSecondary
                            font.pixelSize: theme.fontBody
                        }
                    }
                    Text {
                        visible: (header.labelId || "") === "" || (header.label || "") === ""
                        width: parent.width
                        text: header.infoLine || ""
                        color: theme.textSecondary
                        font.pixelSize: theme.fontBody
                        elide: Text.ElideRight
                    }

                    // Editorial description + Read more.
                    Item { visible: (header.description || "") !== ""; width: 1; height: 12 }
                    Text {
                        visible: (header.description || "") !== ""
                        width: parent.width
                        text: header.descriptionShort || ""
                        color: theme.textSecondary
                        font.pixelSize: theme.fontLegal
                        wrapMode: Text.WordWrap
                    }
                    Item { visible: (header.description || "") !== (header.descriptionShort || ""); width: 1; height: 4 }
                    Text {
                        visible: (header.description || "") !== (header.descriptionShort || "")
                        text: QbzBridge.tr("Read more")
                        color: readMoreArea.containsMouse ? theme.accentHover : theme.accent
                        font.pixelSize: theme.fontLegal
                        MouseArea {
                            id: readMoreArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.showDescription = true
                        }
                    }

                    Item { width: 1; height: 20 }
                    // Action row (on-surface CircleActions).
                    Row {
                        spacing: 12
                        // Play — accent disc, white glyph (on-surface primary).
                        Rectangle {
                            width: 44
                            height: 44
                            radius: 22
                            color: playHdrArea.containsMouse ? theme.accentHover : theme.accent
                            QbzIcon {
                                anchors.centerIn: parent
                                name: "play-fill"
                                width: 19
                                height: 19
                                tintName: "primary"
                            }
                            MouseArea {
                                id: playHdrArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: QbzBridge.playAlbum(header.id)
                            }
                        }
                        CircleBtn {
                            name: "shuffle"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzBridge.playAlbumShuffled(header.id)
                        }
                        CircleBtn {
                            name: header.isFavorite ? "heart-filled" : "heart"
                            active: header.isFavorite === true
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: {
                                header.isFavorite = !header.isFavorite
                                QbzBridge.libraryToggleFavorite("album", header.id)
                            }
                        }
                        // Radio / Mixtape / Info — INERT stubs (POC-NOTE:
                        // radio engines, mixtape store, album-info modal).
                        CircleBtn { name: "radio"; anchors.verticalCenter: parent.verticalCenter }
                        CircleBtn { name: "cassette-tape"; anchors.verticalCenter: parent.verticalCenter }
                        CircleBtn { name: "info"; anchors.verticalCenter: parent.verticalCenter }
                        CircleBtn {
                            id: albumMenuBtn
                            name: "ellipsis"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: function (mouse) { albumMenu.openAtCursor(albumMenuBtn, mouse.x, mouse.y) }
                        }
                    }
                }
            }

            Item { width: 1; height: 20 }
            // Header divider (the atmosphere ends here in Slint).
            Rectangle { width: parent.width - 64; height: 1; color: theme.borderSubtle }
            Item { width: 1; height: 8 }

            // --- Track list + label/awards sidebar ----------------------
            Row {
                width: parent.width - 64
                spacing: 32

                Column {
                    width: parent.width - ((header.label || "") !== "" || awards.length > 0 ? 232 : 0)
                    spacing: 0

                    // Loading.
                    Item {
                        visible: QbzBridge.albumLoading && tracks.length === 0
                        width: parent.width
                        height: 280
                        Column {
                            anchors.centerIn: parent
                            spacing: 18
                            QbzSpinner { size: 36; anchors.horizontalCenter: parent.horizontalCenter }
                            Text {
                                anchors.horizontalCenter: parent.horizontalCenter
                                text: QbzBridge.tr("Loading album…")
                                color: theme.textMuted
                                font.pixelSize: 13
                            }
                        }
                    }

                    // Toolbar — quality badge + track search (+ inert select).
                    Row {
                        visible: !QbzBridge.albumLoading
                        width: parent.width
                        height: 52
                        spacing: 16
                        Row {
                            id: qualityRow
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 7
                            Image {
                                visible: header.qualityTier === "hires"
                                source: "assets/hi-res.svg"
                                width: 42
                                height: 28
                                anchors.verticalCenter: parent.verticalCenter
                                sourceSize: Qt.size(84, 56)
                                fillMode: Image.PreserveAspectFit
                            }
                            Text {
                                visible: (header.qualityDetail || "") !== ""
                                text: header.qualityDetail || ""
                                color: theme.textMuted
                                font.pixelSize: 12
                                anchors.verticalCenter: parent.verticalCenter
                            }
                        }
                        Item { width: parent.width - qualityRow.width - 168 - 30 - 3 * 16; height: 1 }
                        Rectangle {
                            width: 168
                            height: 34
                            radius: 6
                            anchors.verticalCenter: parent.verticalCenter
                            color: theme.surfaceElevated
                            border.width: 1
                            border.color: theme.borderSubtle
                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: 10
                                anchors.rightMargin: 10
                                spacing: 7
                                QbzIcon {
                                    name: "search"
                                    width: 14
                                    height: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                    tintName: "muted"
                                }
                                TextInput {
                                    width: parent.width - 21
                                    height: parent.height
                                    color: theme.textPrimary
                                    font.pixelSize: 13
                                    verticalAlignment: Text.AlignVCenter
                                    clip: true
                                    onTextEdited: root.trackQuery = text
                                    Text {
                                        visible: parent.text === ""
                                        anchors.fill: parent
                                        text: QbzBridge.tr("Search tracks...")
                                        color: theme.textMuted
                                        font.pixelSize: 13
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                }
                            }
                        }
                        Rectangle {
                            width: 30
                            height: 30
                            radius: 6
                            anchors.verticalCenter: parent.verticalCenter
                            color: selectArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                            border.width: 1
                            border.color: theme.borderSubtle
                            QbzIcon {
                                name: "square-check-big"
                                width: 15
                                height: 15
                                anchors.centerIn: parent
                                tintName: selectArea.containsMouse ? "primary" : "secondary"
                            }
                            MouseArea {
                                id: selectArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                // POC-NOTE: multi-select out of scope.
                            }
                        }
                    }

                    // Column header.
                    Row {
                        visible: !QbzBridge.albumLoading
                        width: parent.width
                        height: 40
                        leftPadding: 12
                        rightPadding: 12
                        spacing: 16
                        Text { text: "#"; width: 32; color: theme.textMuted; font.pixelSize: 13; font.letterSpacing: 0.5; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: QbzBridge.tr("Title"); width: parent.width - 32 - 80 - 80 - 28 - 28 - 32 - 5 * 16 - 24; color: theme.textMuted; font.pixelSize: 13; font.letterSpacing: 0.5; anchors.verticalCenter: parent.verticalCenter; elide: Text.ElideRight }
                        Text { text: QbzBridge.tr("Duration"); width: 80; color: theme.textMuted; font.pixelSize: 13; font.letterSpacing: 0.5; horizontalAlignment: Text.AlignHCenter; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: QbzBridge.tr("Quality"); width: 80; color: theme.textMuted; font.pixelSize: 13; font.letterSpacing: 0.5; horizontalAlignment: Text.AlignHCenter; anchors.verticalCenter: parent.verticalCenter }
                        Item { width: 28; height: 1; QbzIcon { name: "heart"; width: 14; height: 14; anchors.centerIn: parent; tintName: "muted" } }
                        Item { width: 28; height: 1; QbzIcon { name: "cloud-download"; width: 14; height: 14; anchors.centerIn: parent; tintName: "muted" } }
                        Item { width: 32; height: 1 }
                    }

                    // Rows (with Disc / work headers).
                    Repeater {
                        model: root.visibleTracks
                        delegate: Column {
                            required property var modelData
                            required property int index
                            property var hdr: root.headerFor(index)
                            width: parent ? parent.width : 0

                            Rectangle {
                                visible: hdr && hdr.disc > 0
                                width: parent.width
                                height: 40
                                color: "transparent"
                                Text {
                                    anchors.left: parent.left
                                    anchors.leftMargin: 12
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: QbzBridge.tr("Disc") + " " + hdr.disc
                                    color: theme.textMuted
                                    font.pixelSize: 13
                                    font.weight: theme.weightSemibold
                                    font.letterSpacing: 0.5
                                }
                            }
                            Row {
                                visible: hdr && hdr.work !== ""
                                width: parent.width
                                leftPadding: 12
                                rightPadding: 12
                                topPadding: 14
                                bottomPadding: 4
                                spacing: 0
                                Text {
                                    text: hdr.work
                                    color: theme.textPrimary
                                    font.pixelSize: theme.fontBody
                                    font.weight: theme.weightBold
                                }
                                Text {
                                    visible: modelData.workComposerName !== ""
                                    text: " ("
                                    color: theme.textSecondary
                                    font.pixelSize: theme.fontBody
                                    font.weight: theme.weightBold
                                }
                                Text {
                                    visible: modelData.workComposerName !== ""
                                    text: modelData.workComposerName
                                    color: composerArea.containsMouse && modelData.workComposerId !== "" ? theme.textPrimary : theme.textSecondary
                                    font.pixelSize: theme.fontBody
                                    font.weight: theme.weightBold
                                    MouseArea {
                                        id: composerArea
                                        anchors.fill: parent
                                        enabled: modelData.workComposerId !== ""
                                        hoverEnabled: true
                                        cursorShape: modelData.workComposerId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                                        onClicked: QbzBridge.openArtist(modelData.workComposerId)
                                    }
                                }
                                Text {
                                    visible: modelData.workComposerName !== ""
                                    text: ")"
                                    color: theme.textSecondary
                                    font.pixelSize: theme.fontBody
                                    font.weight: theme.weightBold
                                }
                            }
                            AlbumTrackRow { row: modelData; rowIndex: index }
                        }
                    }
                }

                // Label / awards sidebar (200px).
                Column {
                    visible: (header.label || "") !== "" || awards.length > 0
                    width: 200
                    spacing: 24

                    Column {
                        visible: (header.label || "") !== ""
                        width: parent.width
                        spacing: 8
                        SidebarHeading { text: QbzBridge.tr("LABEL") }
                        SidebarCard {
                            name: header.label || ""
                            iconName: "disc"
                            gradA: "#6366f1"
                            gradB: "#8b5cf6"
                            // POC-NOTE: no label view yet.
                        }
                    }
                    Column {
                        visible: awards.length > 0
                        width: parent.width
                        spacing: 8
                        SidebarHeading { text: QbzBridge.tr("AWARDS") }
                        Repeater {
                            model: awards
                            delegate: SidebarCard {
                                required property var modelData
                                name: modelData[1]
                                iconName: "award"
                                gradA: "#b45309"
                                gradB: "#eab308"
                                // POC-NOTE: no award view yet.
                            }
                        }
                    }
                }
            }

            // --- Bottom carousels ---------------------------------------
            Item { visible: (album.moreFromArtist || []).length > 0; width: 1; height: 40 }
            SectionRail {
                visible: (album.moreFromArtist || []).length > 0
                title: QbzBridge.tr("From the same artist")
                items: album.moreFromArtist || []
                coverMap: root.coverMap
            }
            Item { visible: (album.suggestions || []).length > 0; width: 1; height: 40 }
            SectionRail {
                visible: (album.suggestions || []).length > 0
                title: QbzBridge.tr("Listening suggestions")
                items: album.suggestions || []
                coverMap: root.coverMap
            }
        }
    }

    // Thin auto-hiding scrollbar (ListScrollbar).
    QbzScrollBar {
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        target: pageFlick
    }

    // --- Full-description modal ------------------------------------------
    Rectangle {
        visible: root.showDescription
        anchors.fill: parent
        color: "#bf000000"
        MouseArea {
            anchors.fill: parent
            onClicked: root.showDescription = false
        }
        Rectangle {
            anchors.centerIn: parent
            width: Math.min(root.width - 80, 560)
            height: Math.min(root.height - 120, 460)
            radius: theme.radiusMd
            color: theme.surfaceCard
            border.width: 1
            border.color: theme.borderSubtle
            MouseArea { anchors.fill: parent }
            Column {
                anchors.fill: parent
                anchors.margins: 24
                spacing: 14
                Row {
                    width: parent.width
                    Text {
                        width: parent.width - 28
                        text: QbzBridge.tr("About this album")
                        color: theme.textPrimary
                        font.pixelSize: theme.fontHeading
                        font.weight: theme.weightSemibold
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Rectangle {
                        width: 28
                        height: 28
                        color: closeArea.containsMouse ? theme.surfaceHover : "transparent"
                        radius: 6
                        QbzIcon {
                            name: "x"
                            width: 18
                            height: 18
                            anchors.centerIn: parent
                            tintName: closeArea.containsMouse ? "primary" : "muted"
                        }
                        MouseArea {
                            id: closeArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.showDescription = false
                        }
                    }
                }
                Flickable {
                    width: parent.width
                    height: parent.height - 42
                    clip: true
                    contentWidth: width
                    contentHeight: descText.implicitHeight
                    Text {
                        id: descText
                        width: parent.width
                        text: header.description || ""
                        color: theme.textSecondary
                        font.pixelSize: theme.fontBody
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }
    }

    // Album ⋯ menu (AlbumContextMenu subset — card menu + pin).
    QbzContextMenu {
        id: albumMenu
        menuWidth: 196
            Repeater {
                model: [
                    { "label": QbzBridge.tr("Play"), "icon": "play-fill", "action": "play" },
                    { "label": QbzBridge.tr("Play next"), "icon": "list-plus", "action": "next" },
                    { "label": QbzBridge.tr("Add to queue"), "icon": "list-end", "action": "queue" },
                    { "label": header.isFavorite ? QbzBridge.tr("Remove from Library") : QbzBridge.tr("Add to Library"), "icon": header.isFavorite ? "heart-filled" : "heart", "action": "favorite" },
                    { "label": header.isPinned ? QbzBridge.tr("Unpin") : QbzBridge.tr("Pin"), "icon": header.isPinned ? "pin-filled" : "pin", "action": "pin" },
                ]
                delegate: Rectangle {
                    required property var modelData
                    width: parent ? parent.width : 0
                    height: 33
                    radius: 5
                    color: amiArea.containsMouse ? theme.surfaceHover : "transparent"
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
                        id: amiArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            albumMenu.close()
                            var a = modelData.action
                            if (a === "play") QbzBridge.playAlbum(header.id)
                            else if (a === "next") QbzBridge.enqueueAlbum(header.id, "next")
                            else if (a === "queue") QbzBridge.enqueueAlbum(header.id, "later")
                            else if (a === "favorite") {
                                header.isFavorite = !header.isFavorite
                                QbzBridge.libraryToggleFavorite("album", header.id)
                            } else if (a === "pin") {
                                header.isPinned = !header.isPinned
                                QbzBridge.togglePin("album", header.id, header.title, header.artist, header.artUrl)
                            }
                        }
                    }
                }
            }
        }
}
