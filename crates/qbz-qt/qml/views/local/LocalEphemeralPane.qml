// Ephemeral-folder pane (LocalLibraryView.slint:114 EphemeralPane) — shown
// in the Folders tab while an ad-hoc folder is open. Album-grouped (a folder
// can hold several albums): each block is a 56px cover + title/artist/meta,
// with a per-album play button ONLY for multi-album sessions, then the
// block's tracks.
//
// Metadata-bound actions (favorite / playlist / album+artist links) are off:
// ephemeral tracks have no DB row. The CUE badge marks a single-file rip.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../rows"
import "../../theme"

Item {
    id: root

    property var view: null

    QbzTheme { id: theme }

    readonly property var eph: view ? view.ephemeral : null
    readonly property var albums: eph && eph.albums ? eph.albums : []

    /// Art for the 224px header cell: the FIRST block's key. For the common
    /// one-album session that is simply "the cover"; for a multi-album folder
    /// it is a representative, and each block keeps drawing its own 56px art
    /// below. Empty when the session has no art at all, which collapses the
    /// cell instead of reserving an empty square.
    readonly property string headerArtKey:
        albums.length > 0 && albums[0].artKey ? albums[0].artKey : ""

    // Same rule as LocalFolderDetail: the host reports this pane's covers as
    // one window when the document changes, so re-opening the pane on an
    // unchanged document (leaving and returning to the Folders tab) needs its
    // own trigger, or the evicted covers never come back.
    Component.onCompleted: if (view && visible) view.reportEphemeralWindow()
    Component.onDestruction: if (view) view.releaseWindow("ephemeral")
    onVisibleChanged: {
        if (!view) return
        if (visible) view.reportEphemeralWindow()
        else view.releaseWindow("ephemeral")
    }
    Connections {
        target: root.view
        function onArtworkRefresh() {
            if (root.visible) root.view.reportEphemeralWindow()
            else root.view.releaseWindow("ephemeral")
        }
    }

    Flickable {
        id: pane
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: page.height + 100
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: page
            x: 32
            y: 16
            width: parent.width - 64
            spacing: 14

            // ---- Header: cover + name / count · duration, and the actions ----
            //
            // Shaped after LocalAlbumHeader, because for the common case this
            // pane IS an album page: one folder, one album, one cover. The art
            // is 224px there (`LocalAlbumHeader.qml:45`) and 224 here, so an
            // opened folder and an indexed album do not present the same record
            // at two different sizes.
            //
            // The cover cell collapses to zero width when the session has no
            // art at all, rather than reserving a 224px hole — a CUE rip with
            // no cover.jpg would otherwise open on a mostly empty header.
            Row {
                width: parent.width
                spacing: root.headerArtKey === "" ? 0 : 20

                readonly property int actionsW: 3 * 40 + 2 * 12

                Rectangle {
                    id: heroArt
                    visible: root.headerArtKey !== ""
                    width: visible ? 224 : 0
                    height: visible ? 224 : 0
                    radius: 8
                    color: theme.surfaceHover
                    clip: true
                    QbzIcon {
                        name: "disc-3"
                        width: 64
                        height: 64
                        anchors.centerIn: parent
                        tintName: "muted"
                    }
                    RoundedImage {
                        id: heroImg
                        anchors.fill: parent
                        source: root.view ? root.view.artPathOf(root.headerArtKey) : ""
                        radius: 8
                    }
                    QbzSkeleton {
                        variant: "art"
                        anchors.fill: parent
                        blockRadius: 8
                        pending: root.view ? root.view.artWanted(root.headerArtKey) : false
                        coverReady: heroImg.ready
                        phase: root.view ? root.view.skelPhase : false
                        settleMs: root.view ? root.view.artSettleMs : 0
                        settleHold: root.view ? root.view.artPulse : false
                    }
                }

                Column {
                    width: parent.width - heroArt.width
                        - (root.headerArtKey === "" ? 0 : 20)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8

                    Text {
                        width: parent.width
                        text: root.eph ? (root.eph.name || "") : ""
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightBold
                        elide: Text.ElideRight
                    }
                    // "10 tracks · 42 min". The PATH used to live here and no
                    // longer does: the medium's name is already the line above
                    // and the tab, and the duration is what a listener actually
                    // wants before pressing play.
                    Text {
                        width: parent.width
                        text: {
                            var n = root.eph ? (root.eph.trackCount || 0) : 0
                            var d = root.eph ? (root.eph.totalDuration || "") : ""
                            var t = n + " " + QbzSession.tr("tracks", QbzSession.trRev)
                            return d === "" ? t : t + " · " + d
                        }
                        color: theme.textMuted
                        font.pixelSize: theme.fontLegal
                        elide: Text.ElideRight
                    }
                    Row {
                        spacing: 12
                        topPadding: 4
                        QbzCircleAction {
                            name: "play-fill"
                            primary: true
                            onClicked: QbzLocal.ephemeralPlayAll(false)
                        }
                        QbzCircleAction {
                            name: "shuffle"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzLocal.ephemeralPlayAll(true)
                        }
                        // Rip — a PHYSICAL disc only. An image is already a
                        // file, so offering it there would be offering to copy
                        // something the user already has.
                        QbzCircleAction {
                            visible: QbzLocal.localEphemeralIsCd && !QbzLocal.localRipActive
                            name: "disc-folder"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzLocal.ripDisc()
                        }
                        // While it runs the button gives way to its progress —
                        // one control, one state, rather than a button that
                        // looks pressable during a job it cannot start twice.
                        Row {
                            visible: QbzLocal.localRipActive
                            spacing: 8
                            anchors.verticalCenter: parent.verticalCenter
                            QbzSpinner {
                                anchors.verticalCenter: parent.verticalCenter
                                size: 15
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: QbzSession.tr("Ripping", QbzSession.trRev)
                                    + " " + QbzLocal.localRipProgress
                                color: theme.textSecondary
                                font.pixelSize: theme.fontLegal
                            }
                        }
                    }
                }
            }

            // Scanning — the shape of the album blocks that are coming
            // (56px cover + two text bars per block), not a spinner.
            QbzSkeleton {
                visible: QbzLocal.localEphemeralLoading
                variant: "rowList"
                width: parent.width
                height: visible ? 3 * 68 : 0
                rowH: 56
                rowGap: 12
                rowArtSize: 56
                phase: root.view ? root.view.skelPhase : false
            }

            // Empty (no playable tracks).
            Text {
                visible: !QbzLocal.localEphemeralLoading && root.albums.length === 0
                width: parent.width
                topPadding: 24
                text: QbzSession.tr("No playable tracks in this folder.", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: theme.fontBody
                horizontalAlignment: Text.AlignHCenter
            }

            // ---- Album blocks ----
            Column {
                visible: !QbzLocal.localEphemeralLoading
                width: parent.width
                spacing: 22
                Repeater {
                    model: root.albums
                    delegate: Column {
                        id: block
                        required property var modelData
                        width: page.width
                        spacing: 6

                        // The per-block header is for MULTI-album sessions,
                        // where it is the only thing separating one record
                        // from the next. A single-album session already has
                        // all of it — cover, title, artist, count — in the
                        // 224px header above, and drawing it twice made the
                        // pane read like a page with a stutter.
                        Row {
                            visible: root.eph && root.eph.multiAlbum === true
                            height: visible ? implicitHeight : 0
                            width: parent.width
                            spacing: 12
                            Rectangle {
                                width: 56
                                height: 56
                                radius: 6
                                color: theme.surfaceHover
                                clip: true
                                QbzIcon {
                                    name: "disc-3"
                                    width: 24
                                    height: 24
                                    anchors.centerIn: parent
                                    tintName: "muted"
                                }
                                RoundedImage {
                                    id: blockArt
                                    anchors.fill: parent
                                    source: root.view
                                        ? root.view.artPathOf(block.modelData.artKey) : ""
                                    radius: 6
                                }
                                // Per-item: hands over when THIS album's
                                // cover is actually painted, settles out when
                                // it has none.
                                QbzSkeleton {
                                    variant: "art"
                                    anchors.fill: parent
                                    blockRadius: 6
                                    pending: root.view
                                        ? root.view.artWanted(block.modelData.artKey) : false
                                    coverReady: blockArt.ready
                                    phase: root.view ? root.view.skelPhase : false
                                    settleMs: root.view ? root.view.artSettleMs : 0
                                    settleHold: root.view ? root.view.artPulse : false
                                }
                            }
                            Column {
                                width: parent.width - 56 - 12
                                    - (root.eph && root.eph.multiAlbum ? 46 : 0)
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 2
                                Row {
                                    width: parent.width
                                    spacing: 6
                                    Text {
                                        width: parent.width - (block.modelData.isCue ? 44 : 0)
                                        text: block.modelData.title || ""
                                        color: theme.textPrimary
                                        font.pixelSize: theme.fontBody
                                        font.weight: theme.weightSemibold
                                        elide: Text.ElideRight
                                    }
                                    // CUE badge (single-file rip) — a 16px
                                    // rounded rect, not a pill (ADR-008).
                                    Rectangle {
                                        visible: block.modelData.isCue === true
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: cueText.implicitWidth + 12
                                        height: 16
                                        radius: 3
                                        color: theme.surfaceElevated
                                        Text {
                                            id: cueText
                                            anchors.centerIn: parent
                                            text: "CUE"
                                            color: theme.textSecondary
                                            font.pixelSize: theme.fontLegal
                                            font.weight: theme.weightSemibold
                                        }
                                    }
                                }
                                Text {
                                    width: parent.width
                                    text: block.modelData.artist || ""
                                    color: theme.textSecondary
                                    font.pixelSize: theme.fontLegal
                                    elide: Text.ElideRight
                                }
                                Text {
                                    visible: (block.modelData.meta || "") !== ""
                                    width: parent.width
                                    text: block.modelData.meta || ""
                                    color: theme.textMuted
                                    font.pixelSize: theme.fontLegal
                                    elide: Text.ElideRight
                                }
                            }
                            QbzCircleAction {
                                visible: root.eph && root.eph.multiAlbum === true
                                anchors.verticalCenter: parent.verticalCenter
                                name: "play-fill"
                                onClicked: QbzLocal.ephemeralPlayAlbum(block.modelData.groupKey)
                            }
                        }

                        Repeater {
                            model: block.modelData.tracks || []
                            delegate: TrackRow {
                                required property var modelData
                                required property int index
                                width: page.width
                                item: modelData
                                number: index + 1
                                showArtwork: false
                                showAlbum: false
                                showFavorite: false
                                showDownload: false
                                showMenu: false
                                draggable: false
                                zebra: true
                                onPlayRequested: QbzLocal.ephemeralPlayTrack(modelData.id)
                            }
                        }
                    }
                }
            }
        }
    }
    // Close, pinned to the pane's top-right corner.
    //
    // It used to sit in the action row between Shuffle and the rest, which is
    // where a hand goes without looking — a destructive action one pixel from
    // "play". Up here it is deliberate, and it is the same place every other
    // dismissable surface in this app puts its close.
    //
    // OUTSIDE the Flickable on purpose: scrolling a long disc must not carry
    // the only way to close the session off the top of the screen.
    QbzCircleAction {
        name: "x"
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.rightMargin: 32
        anchors.topMargin: 16
        onClicked: QbzLocal.ephemeralClear()
    }

    QbzScrollBar {
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        target: pane
        visible: pane.contentHeight > pane.height
    }
}
