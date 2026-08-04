// KioskTrackRow — lightweight track row for the kiosk Search/Library track
// tabs (shell/KioskTrackRow.slint). Art + title/artist + duration, tap to
// play. No number cell, no favourite, no quality badge, no menu, no
// checkbox — the desktop rows/TrackRow.qml column model is exactly the weight
// the kiosk shell drops.
//
// `track` is a plain JS object carrying { id, title, artist, duration,
// artwork }; `artwork` is the RESOLVED local cover path the hosting view
// supplies (the Qt documents carry only the remote `artUrl`).
//
// `playing` marks the row whose track is the one on the deck. The Slint
// KioskTrackRow has no such state, so its treatment is taken from the kiosk's
// own current-track idiom on the album page (KioskAlbum.slint:171-175, :195):
// an accent@0.10 fill that sits BELOW focus and hover in the precedence, and
// the title in the accent colour.

import QtQuick
import "../theme"

Rectangle {
    id: root

    property var track: ({})
    /// Keyboard/gamepad nav — same ring idiom as KioskCard, 2px here.
    property bool navFocused: false
    property bool playing: false
    signal clicked()

    QbzTheme { id: theme }

    readonly property string _title: root.track && root.track.title ? root.track.title : ""
    readonly property string _artist: root.track && root.track.artist ? root.track.artist : ""
    readonly property string _duration: root.track && root.track.duration ? root.track.duration : ""
    readonly property string _artwork: root.track && root.track.artwork ? root.track.artwork : ""

    height: 62
    radius: theme.radiusSm
    color: root.navFocused
        ? Qt.rgba(theme.accent.r, theme.accent.g, theme.accent.b, 0.18)
        : rowArea.containsMouse
            ? theme.alphaTier(6)
            : root.playing
                ? Qt.rgba(theme.accent.r, theme.accent.g, theme.accent.b, 0.10)
                : "transparent"
    border.width: root.navFocused ? 2 : 0
    border.color: theme.accent

    Row {
        id: cells
        anchors.left: root.left
        anchors.leftMargin: 8
        anchors.right: root.right
        anchors.rightMargin: 14
        anchors.top: root.top
        anchors.bottom: root.bottom
        spacing: 12

        // Art slot — fixed 46x46 centred in the row height.
        Item {
            id: artCell
            width: 46
            height: cells.height

            Rectangle {
                id: artTile
                width: 46
                height: 46
                y: (artCell.height - artTile.height) / 2
                radius: 5
                color: theme.surfaceElevated
                clip: true

                RoundedImage {
                    anchors.fill: parent
                    source: root._artwork
                    radius: 5
                    fit: "crop"
                }
            }
        }

        Item {
            id: textCol
            width: Math.max(0, cells.width - artCell.width - durationLabel.width - 2 * cells.spacing)
            height: cells.height

            Column {
                anchors.verticalCenter: textCol.verticalCenter
                width: textCol.width
                spacing: 2

                Text {
                    width: textCol.width
                    text: root._title
                    color: root.playing ? theme.accent : theme.textPrimary
                    font.pixelSize: 15
                    font.weight: theme.weightMedium
                    elide: Text.ElideRight
                    wrapMode: Text.NoWrap
                    maximumLineCount: 1
                }

                Text {
                    width: textCol.width
                    text: root._artist
                    color: theme.textMuted
                    font.pixelSize: 13
                    elide: Text.ElideRight
                    wrapMode: Text.NoWrap
                    maximumLineCount: 1
                }
            }
        }

        Text {
            id: durationLabel
            height: cells.height
            text: root._duration
            color: theme.textMuted
            font.pixelSize: 13
            verticalAlignment: Text.AlignVCenter
        }
    }

    MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        onClicked: root.clicked()
    }
}
