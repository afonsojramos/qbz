// Cover lightbox — NEW in the Qt port (no Slint counterpart): the album art
// at its best available quality, fitted to the window. Opened by left-click
// on the AlbumView cover or by the cover menu's "View cover" entry.
//
// Shape follows TrackInfoModal: a full-window Popup parented to
// Overlay.overlay, a dim scrim whose click closes, Escape closes. The image
// asks for a decode capped at the window size (sourceSize), so a mega
// variant does not cost a full-res decode to display at ~90% of the window.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../theme"

Popup {
    id: root

    parent: Overlay.overlay
    x: 0
    y: 0
    width: parent ? parent.width : 0
    height: parent ? parent.height : 0
    padding: 0
    closePolicy: Popup.CloseOnEscape

    // file:// (custom cover) or the remote best() URL. Set through openWith.
    property string artSource: ""

    function openWith(source) {
        root.artSource = source
        root.open()
    }

    QbzTheme { id: theme }

    background: Rectangle {
        color: "#bf000000"
    }

    contentItem: Item {
        // Scrim click closes; clicks on the image itself are swallowed so
        // the lightbox stays up while the user inspects it.
        MouseArea {
            anchors.fill: parent
            onClicked: root.close()
        }

        // Fit the LONGER window axis at 90%, so the cover never touches the
        // rim. Covers are square; PreserveAspectFit keeps any odd one honest.
        readonly property real side: Math.min(root.width, root.height) * 0.9

        Image {
            id: art
            anchors.centerIn: parent
            width: parent.side
            height: parent.side
            source: root.artSource
            asynchronous: true
            fillMode: Image.PreserveAspectFit
            sourceSize: Qt.size(parent.side, parent.side)
            smooth: true
            visible: status === Image.Ready

            MouseArea {
                anchors.fill: parent
                onClicked: {} // swallow — the scrim's close must not fire here
            }
        }

        QbzSpinner {
            anchors.centerIn: parent
            visible: root.artSource !== "" && art.status === Image.Loading
        }

        Text {
            anchors.centerIn: parent
            visible: root.artSource !== "" && art.status === Image.Error
            text: QbzSession.tr("Could not load the image", QbzSession.trRev)
            color: theme.textSecondary
            font.pixelSize: theme.fontBody
        }
    }
}
