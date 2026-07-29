// Local album header block (album/LocalAlbumView.slint:213-404): the 224px
// cover, the title / artist line with the "+N more artists" expander and its
// expanded list, the info line, the local action row (play / shuffle / edit
// tags / add to playlist / add to Mixtape) and the version picker.
//
// `artistsExpanded` is view-local and resets when another album opens — the
// Slint watches its own mirror property for exactly this (:156).

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Row {
    id: root

    property var album: null
    /// De-duplicated contributor list (>1 turns the expander on).
    property var allArtists: []
    property string infoLine: ""
    property var versions: []
    property string coverSource: ""
    /// Per-item cover state, owned by the page (LocalAlbumView).
    property bool coverPending: false
    property bool skelPhase: false
    property int artSettleMs: 0
    signal openArtist(string name)

    QbzTheme { id: theme }

    property bool artistsExpanded: false
    onAlbumChanged: artistsExpanded = false

    spacing: 32

    Rectangle {
        width: 224
        height: 224
        radius: 12
        color: theme.surfaceElevated
        clip: true
        RoundedImage {
            anchors.fill: parent
            source: root.coverSource
            radius: 12
        }
        // Per-item: clears when THIS album's cover lands; settles out when
        // the album has none (local artwork drops keys with no cover).
        QbzSkeleton {
            variant: "art"
            anchors.fill: parent
            blockRadius: 12
            visible: root.coverPending
            phase: root.skelPhase
            settleMs: root.artSettleMs
        }
    }

    Column {
        width: parent.width - 224 - 32
        topPadding: 4
        spacing: 0

        Text {
            width: parent.width
            text: root.album ? (root.album.title || "") : ""
            color: theme.textPrimary
            font.pixelSize: theme.fontSection
            font.weight: theme.weightBold
            elide: Text.ElideRight
        }
        Item { width: 1; height: 4 }
        Row {
            spacing: 8
            Text {
                text: root.album ? (root.album.artist || "") : ""
                color: artistArea.containsMouse ? theme.textPrimary : theme.textSecondary
                font.pixelSize: theme.fontHeading
                font.weight: theme.weightBold
                MouseArea {
                    id: artistArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.openArtist(root.album.artist)
                }
            }
            // Multi-artist album (compilations): "+N more artists" expands
            // the full track-artist list below.
            Text {
                visible: root.allArtists.length > 1
                anchors.verticalCenter: parent.verticalCenter
                text: root.artistsExpanded
                    ? QbzSession.tr("Show less", QbzSession.trRev)
                    : "+" + (root.allArtists.length - 1) + " "
                      + QbzSession.tr("more artists", QbzSession.trRev)
                color: moreArea.containsMouse ? theme.accent : theme.textMuted
                font.pixelSize: theme.fontBody
                MouseArea {
                    id: moreArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.artistsExpanded = !root.artistsExpanded
                }
            }
        }
        // Expanded track-artist list — each entry routes to the Local
        // Library Artists tab (name-based).
        Column {
            visible: root.artistsExpanded && root.allArtists.length > 1
            width: parent.width
            topPadding: 6
            spacing: 0
            Repeater {
                model: root.allArtists
                delegate: Text {
                    id: nameRow
                    required property string modelData
                    height: 24
                    text: modelData
                    color: nameArea.containsMouse ? theme.accent : theme.textSecondary
                    font.pixelSize: theme.fontBody
                    verticalAlignment: Text.AlignVCenter
                    MouseArea {
                        id: nameArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.openArtist(nameRow.modelData)
                    }
                }
            }
        }
        Item { width: 1; height: 10 }
        Text {
            width: parent.width
            text: root.infoLine
            color: theme.textSecondary
            font.pixelSize: theme.fontBody
            elide: Text.ElideRight
        }
        Item { width: 1; height: 20 }

        // ---- Local action row ----
        Row {
            spacing: 12
            QbzCircleAction {
                name: "play-fill"
                primary: true
                onClicked: QbzLocal.playAlbum(root.album.id, false)
            }
            QbzCircleAction {
                name: "shuffle"
                anchors.verticalCenter: parent.verticalCenter
                onClicked: QbzLocal.playAlbum(root.album.id, true)
            }
            // ASSET GAP: Slint uses `pencil`; the Qt set ships pen-line.
            QbzCircleAction {
                name: "pen-line"
                anchors.verticalCenter: parent.verticalCenter
                onClicked: QbzLocal.albumEditTags(root.album.id)
            }
            QbzCircleAction {
                name: "list-plus"
                anchors.verticalCenter: parent.verticalCenter
                onClicked: QbzLocal.albumAddToPlaylist(root.album.id)
            }
            QbzCircleAction {
                name: "cassette-tape"
                anchors.verticalCenter: parent.verticalCenter
                onClicked: QbzLocal.albumAddToMixtape(root.album.id)
            }
        }

        // ---- Version picker (multiple physical copies only) ----
        Item {
            visible: root.versions.length > 1
            width: 1
            height: visible ? 16 : 0
        }
        Row {
            visible: root.versions.length > 1
            spacing: 8
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.versions.length + " "
                    + QbzSession.tr("versions", QbzSession.trRev) + ":"
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
            }
            VersionPicker {
                anchors.verticalCenter: parent.verticalCenter
                versions: root.versions
                current: root.album ? (root.album.versionIndex || 0) : 0
                onPicked: function (i) { QbzLocal.albumSelectVersion(i) }
            }
        }
    }
}
