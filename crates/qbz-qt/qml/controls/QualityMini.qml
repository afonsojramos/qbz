// QualityMini — the icon-only quality badge (hi-res SVG / CD glyph),
// promoted from LibraryView in phase 21 so the shared TrackCard (and the
// Library track rows) mount the one replica.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    property string tier: ""
    visible: tier !== ""
    width: tier === "hires" ? 42 : 30
    height: 30

    QbzTheme { id: theme }

    Image {
        visible: tier === "hires"
        source: "qrc:/qt/qml/com/blitzfc/qbz/qml/assets/hi-res.svg"
        width: 42
        height: 28
        anchors.centerIn: parent
        sourceSize: Qt.size(84, 56)
        fillMode: Image.PreserveAspectFit
    }
    Rectangle {
        visible: tier === "cd"
        width: 30
        height: 30
        radius: 3
        color: theme.surfaceElevated
        border.width: 1
        border.color: theme.borderSubtle
        QbzIcon { name: "cd"; width: 16; height: 16; anchors.centerIn: parent; tintName: "muted" }
    }
}
