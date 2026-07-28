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


    // Track list row (TrackRow.slint replica: number cell, no artwork,
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
                        text: QbzBridge.tr("Read more", QbzBridge.trRev)
                        color: readMoreArea.containsMouse ? theme.accentHover : theme.accent
                        font.pixelSize: theme.fontLegal
                        MouseArea {
                            id: readMoreArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                var shell = root.parent
                                while (shell && shell.openTextModal === undefined) shell = shell.parent
                                if (shell) shell.openTextModal(QbzBridge.tr("About this album", QbzBridge.trRev), header.description || "")
                            }
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
                        QbzCircleAction {
                            name: "shuffle"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzBridge.playAlbumShuffled(header.id)
                        }
                        QbzCircleAction {
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
                        QbzCircleAction { name: "radio"; anchors.verticalCenter: parent.verticalCenter }
                        QbzCircleAction { name: "cassette-tape"; anchors.verticalCenter: parent.verticalCenter }
                        QbzCircleAction { name: "info"; anchors.verticalCenter: parent.verticalCenter }
                        QbzCircleAction {
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
                                text: QbzBridge.tr("Loading album…", QbzBridge.trRev)
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
                                        text: QbzBridge.tr("Search tracks...", QbzBridge.trRev)
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
                        Text { text: QbzBridge.tr("Title", QbzBridge.trRev); width: parent.width - 32 - 80 - 80 - 28 - 28 - 32 - 5 * 16 - 24; color: theme.textMuted; font.pixelSize: 13; font.letterSpacing: 0.5; anchors.verticalCenter: parent.verticalCenter; elide: Text.ElideRight }
                        Text { text: QbzBridge.tr("Duration", QbzBridge.trRev); width: 80; color: theme.textMuted; font.pixelSize: 13; font.letterSpacing: 0.5; horizontalAlignment: Text.AlignHCenter; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: QbzBridge.tr("Quality", QbzBridge.trRev); width: 80; color: theme.textMuted; font.pixelSize: 13; font.letterSpacing: 0.5; horizontalAlignment: Text.AlignHCenter; anchors.verticalCenter: parent.verticalCenter }
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
                                    text: QbzBridge.tr("Disc", QbzBridge.trRev) + " " + hdr.disc
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
                            TrackRow {
                                item: modelData
                                number: index + 1
                                zebra: true
                                clickPlays: false
                                artistLink: true
                                qualityStyle: "text"
                                showDownload: true
                                downloadGlyph: true
                                menuShowLater: false
                                menuShowGoTo: false
                                onPlayRequested: QbzBridge.playAlbumFrom(header.id, item.id)
                                onEnqueueRequested: function (m) {
                                    QbzBridge.enqueueAlbumTrack(header.id, item.id, m === "next" ? "next" : "later")
                                }
                            }
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
                        SidebarHeading { text: QbzBridge.tr("LABEL", QbzBridge.trRev) }
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
                        SidebarHeading { text: QbzBridge.tr("AWARDS", QbzBridge.trRev) }
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
                title: QbzBridge.tr("From the same artist", QbzBridge.trRev)
                items: album.moreFromArtist || []
                coverMap: root.coverMap
            }
            Item { visible: (album.suggestions || []).length > 0; width: 1; height: 40 }
            SectionRail {
                visible: (album.suggestions || []).length > 0
                title: QbzBridge.tr("Listening suggestions", QbzBridge.trRev)
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

    // Album ⋯ menu (AlbumContextMenu subset — card menu + pin).
    QbzContextMenu {
        id: albumMenu
        menuWidth: 196
            Repeater {
                model: [
                    { "label": QbzBridge.tr("Play", QbzBridge.trRev), "icon": "play-fill", "action": "play" },
                    { "label": QbzBridge.tr("Play next", QbzBridge.trRev), "icon": "list-plus", "action": "next" },
                    { "label": QbzBridge.tr("Add to queue", QbzBridge.trRev), "icon": "list-end", "action": "queue" },
                    { "label": header.isFavorite ? QbzBridge.tr("Remove from Library", QbzBridge.trRev) : QbzBridge.tr("Add to Library", QbzBridge.trRev), "icon": header.isFavorite ? "heart-filled" : "heart", "action": "favorite" },
                    { "label": header.isPinned ? QbzBridge.tr("Unpin", QbzBridge.trRev) : QbzBridge.tr("Pin", QbzBridge.trRev), "icon": header.isPinned ? "pin-filled" : "pin", "action": "pin" },
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
