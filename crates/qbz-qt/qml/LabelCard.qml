// LabelCard — THE label card (discover/LabelCard.slint), promoted from
// LibraryView.LibLabelCard in phase 21: Library Labels grid + All feed.
// "A label is opened, not played": hover scrim 0.25 only, no overlay
// buttons, no menu. POC-NOTE: the label landing page is out of scope —
// clicks are inert (the Slint mount wires open-label).
//
// item contract: { id, title, subtitle, imageUrl } plus the
// host-resolved artSource string prop — imageUrl === "" selects the
// indigo/violet glyph arm;
// logos are contain-fit, never cropped.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    id: root

    property var item: ({})
    // Host-resolved artwork path (the AlbumCard artSource pattern).
    property string artSource: ""
    // Remote cover URL for the pin payload ("" when the host has none).
    property string artworkUrl: ""

    color: "transparent"

    QbzTheme { id: theme }

    implicitWidth: 200
    implicitHeight: 246

    Column {
        spacing: 0
        Rectangle {
            width: 200
            height: 200
            radius: theme.radiusSm
            color: theme.surfaceElevated
            clip: true
            // No logo: indigo->violet gradient + disc glyph.
            Rectangle {
                visible: !root.item.imageUrl
                anchors.fill: parent
                radius: theme.radiusSm
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: "#6366f1" }
                    GradientStop { position: 1.0; color: "#8b5cf6" }
                }
                QbzIcon {
                    name: "disc-3"
                    width: 56
                    height: 56
                    anchors.centerIn: parent
                    tintName: "primary"
                }
            }
            RoundedImage {
                visible: !!root.item.imageUrl
                anchors.fill: parent
                source: root.artSource
                radius: theme.radiusSm
                // Logos are contain-fit, never cropped (LabelCard).
                fit: "contain"
            }
            Rectangle {
                anchors.fill: parent
                radius: theme.radiusSm
                color: "#000000"
                opacity: lblArea.containsMouse ? 0.25 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
            }
            MouseArea {
                id: lblArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                // Label landing page: out of scope (POC-NOTE — inert).
            }
        }
        Item { width: 1; height: 6 }
        Column {
            width: 200
            height: 40
            spacing: 2
            Text {
                width: parent.width
                height: 20
                text: root.item.title || ""
                color: lblTitleArea.containsMouse ? theme.accent : theme.textPrimary
                font.pixelSize: theme.fontBody - 2
                font.weight: theme.weightMedium
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
                MouseArea {
                    id: lblTitleArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    // Inert like the body (label landing page, POC-NOTE).
                }
            }
            Text {
                visible: !!root.item.subtitle
                width: parent.width
                text: root.item.subtitle || ""
                color: theme.textMuted
                font.pixelSize: theme.fontLink - 1
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
    }
}
