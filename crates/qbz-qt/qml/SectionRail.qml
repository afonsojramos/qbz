// Horizontal album rail (Carousel.slint) — extracted for reuse by the
// detail views (AlbumView / ArtistView): section header + page chevrons +
// clipped ListView of the shared AlbumCard, with Cider-style edge fades.

import QtQuick
import com.blitzfc.qbz

Column {
    id: root
    property string title: ""
    property var items: []
    // The host's url-keyed cover map ({url: file://path}).
    property var coverMap: ({})

    QbzTheme { id: theme }
    width: parent ? parent.width : 0
    spacing: 12

    readonly property int perPage: Math.max(1, Math.floor((rail.width + 32) / 232))
    readonly property int step: perPage * 232
    readonly property real maxScroll: Math.max(0, rail.contentWidth - rail.width)

    component RailNavBtn: Rectangle {
        property string name: ""
        property bool btnEnabled: true
        signal clicked()
        width: 28
        height: 28
        radius: 14
        opacity: btnEnabled ? 1.0 : 0.4
        color: (nbArea.containsMouse && btnEnabled) ? theme.surfaceHover : theme.surfaceElevated
        QbzIcon {
            name: parent.name
            width: 15
            height: 15
            anchors.centerIn: parent
            tintName: parent.btnEnabled ? "primary" : "muted"
        }
        MouseArea {
            id: nbArea
            anchors.fill: parent
            enabled: parent.btnEnabled
            hoverEnabled: true
            cursorShape: parent.btnEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: parent.clicked()
        }
    }

    Item {
        width: parent.width
        height: 28
        Text {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            text: root.title
            color: theme.textPrimary
            font.pixelSize: theme.fontSection
            font.weight: theme.weightSemibold
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            RailNavBtn {
                name: "chevron-left"
                btnEnabled: rail.contentX > 1
                onClicked: rail.contentX = Math.max(0, rail.contentX - root.step)
            }
            RailNavBtn {
                name: "chevron-right"
                btnEnabled: rail.contentX < root.maxScroll - 1
                onClicked: rail.contentX = Math.min(root.maxScroll, rail.contentX + root.step)
            }
        }
    }

    Item {
        width: parent.width
        height: 246
        ListView {
            id: rail
            anchors.fill: parent
            orientation: ListView.Horizontal
            spacing: 32
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: root.items
            delegate: AlbumCard {
                albumId: modelData.id
                title: modelData.title
                artist: modelData.artist
                artistId: modelData.artistId
                genre: modelData.genre
                year: modelData.year
                qualityTier: modelData.qualityTier
                artSource: root.coverMap[modelData.artUrl] || ""
                isFavorite: false
            }
        }
        Rectangle {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: 56
            opacity: rail.contentX > 1 ? 1.0 : 0.0
            Behavior on opacity { NumberAnimation { duration: 150 } }
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0; color: theme.surfaceMain }
                GradientStop { position: 1.0; color: "transparent" }
            }
        }
        Rectangle {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: 56
            opacity: rail.contentX < root.maxScroll - 1 ? 1.0 : 0.0
            Behavior on opacity { NumberAnimation { duration: 150 } }
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0; color: "transparent" }
                GradientStop { position: 1.0; color: theme.surfaceMain }
            }
        }
    }
}
