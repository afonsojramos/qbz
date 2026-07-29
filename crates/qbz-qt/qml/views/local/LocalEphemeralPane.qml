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

            // ---- Header: name + count · path, Play all, Clear ----
            Row {
                width: parent.width
                spacing: 12
                Column {
                    width: parent.width - 2 * 40 - 2 * 12
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Text {
                        width: parent.width
                        text: root.eph ? (root.eph.name || "") : ""
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightBold
                        elide: Text.ElideRight
                    }
                    Text {
                        width: parent.width
                        text: (root.eph ? (root.eph.trackCount || 0) : 0) + " "
                            + QbzSession.tr("tracks", QbzSession.trRev) + " · "
                            + (root.eph ? (root.eph.path || "") : "")
                        color: theme.textMuted
                        font.pixelSize: theme.fontLegal
                        elide: Text.ElideRight
                    }
                }
                QbzCircleAction {
                    name: "play-fill"
                    primary: true
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: QbzLocal.ephemeralPlayAll()
                }
                QbzCircleAction {
                    name: "x"
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: QbzLocal.ephemeralClear()
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

                        Row {
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
                                onPlayRequested: QbzLocal.ephemeralPlayTrack(modelData.id)
                            }
                        }
                    }
                }
            }
        }
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
