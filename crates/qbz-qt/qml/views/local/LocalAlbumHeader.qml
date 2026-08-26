// Local album header block (album/LocalAlbumView.slint:213-404): the 224px
// cover, the title / artist line with the "+N more artists" expander and its
// expanded list, the info line, the local action row (play / shuffle / edit
// tags / add to playlist / add to Mixtape) and the version picker.
//
// `artistsExpanded` is view-local and resets when another album opens — the
// Slint watches its own mirror property for exactly this (:156).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Row {
    id: root

    property var album: null
    /// De-duplicated contributor list (>1 turns the expander on).
    property var allArtists: []
    property string infoLine: ""
    property string compactInfoLine: ""
    property bool compact: false
    property var versions: []
    property string coverSource: ""
    /// Per-item cover state, owned by the page (LocalAlbumView).
    /// `coverPending` means "this album EXPECTS a cover" — whether one has
    /// arrived is decided here, by the image's own readiness, never by the
    /// path being non-empty (see theme/RoundedImage.qml's contract).
    property bool coverPending: false
    /// Optional: true while the page's artwork pass is still in flight. It
    /// suspends the settle countdown so a slow cold decode cannot expire the
    /// placeholder a beat before the cover lands. Defaults to false, so the
    /// page need not supply it.
    property bool artPassActive: false
    property bool skelPhase: false
    property int artSettleMs: 0
    signal openArtist(string name)
    signal compactToggled()

    QbzTheme { id: theme }

    property bool artistsExpanded: false
    onAlbumChanged: artistsExpanded = false

    readonly property int coverPx: compact ? 128 : 224
    spacing: compact ? 20 : 32

    Rectangle {
        width: root.coverPx
        height: root.coverPx
        radius: 12
        color: theme.surfaceElevated
        clip: true
        RoundedImage {
            id: headerArt
            anchors.fill: parent
            source: root.coverSource
            radius: 12
        }
        // Per-item: hands over on the paint, not the path; settles out when
        // the album has none (local artwork drops keys with no cover).
        QbzSkeleton {
            variant: "art"
            anchors.fill: parent
            blockRadius: 12
            pending: root.coverPending
            coverReady: headerArt.ready
            phase: root.skelPhase
            settleMs: root.artSettleMs
            settleHold: root.artPassActive
        }
    }

    Column {
        width: parent.width - root.coverPx - root.spacing
        topPadding: 4
        spacing: 0

        Item {
            width: parent.width
            height: Math.max(localAlbumTitle.implicitHeight, 32)
            Text {
                id: localAlbumTitle
                anchors.left: parent.left
                anchors.right: localHeaderModeButton.left
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                text: root.album ? (root.album.title || "") : ""
                color: theme.textPrimary
                font.pixelSize: theme.fontSection
                font.weight: theme.weightBold
                elide: Text.ElideRight
            }
            QbzCircleAction {
                id: localHeaderModeButton
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                name: root.compact ? "maximize-2" : "minimize-2"
                onClicked: root.compactToggled()
                HoverHandler { id: localHeaderModeHover }
                ToolTip.visible: localHeaderModeHover.hovered
                ToolTip.text: root.compact
                    ? QbzSession.tr("Show full album header", QbzSession.trRev)
                    : QbzSession.tr("Show compact album header", QbzSession.trRev)
                ToolTip.delay: 350
            }
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
                visible: !root.compact && root.allArtists.length > 1
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
            visible: !root.compact && root.artistsExpanded
                     && root.allArtists.length > 1
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
        Item { width: 1; height: root.compact ? 4 : 10 }
        Text {
            width: parent.width
            text: root.compact ? root.compactInfoLine : root.infoLine
            color: theme.textSecondary
            font.pixelSize: theme.fontBody
            elide: Text.ElideRight
        }
        Item { width: 1; height: root.compact ? 8 : 20 }

        // ---- Local action row ----
        Row {
            spacing: 12
            QbzCircleAction {
                name: "play-fill"
                primary: true
                onClicked: QbzLocal.albumSelectedAction("play", "")
            }
            QbzCircleAction {
                name: "shuffle"
                anchors.verticalCenter: parent.verticalCenter
                onClicked: QbzLocal.albumSelectedAction("shuffle", "")
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
            visible: !root.compact && root.versions.length > 1
            width: 1
            height: visible ? 16 : 0
        }
        Row {
            visible: !root.compact && root.versions.length > 1
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
