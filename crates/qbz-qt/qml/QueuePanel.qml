// Right-side Queue panel — QML port of
// crates/qbz-ui/ui/shell/QueueSidebar.slint.
//
// Header with the two tabs ("Queue" / "History"), the NOW PLAYING card,
// the UP NEXT list, and the empty states. Phase 4: the Queue tab renders
// the REAL queue (QbzBridge.queueModel — one JSON-encoded row per QVariant,
// see the nesting POC-NOTE in playback_qt.rs `publish_queue`).
//
// POC-NOTEs:
// - The footer (count line + clear / save-as-playlist / infinite /
//  sleep-timer actions + inline queue search) is omitted — phase 4 brings
//  no queue mutations beyond play/next/previous.
// - The History tab stays empty (no local play-history store in the POC).
// - Row context menus, drag-reorder, pagination and the now-playing
//  favorite toggle are out of scope.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    id: root
    color: "transparent"
    clip: true

    // Queue (0) / History (1) — Slint QueueState.tab.
    property int tab: 0
    // Parsed queue rows: [{id, title, artist, duration, artPath, current}].
    readonly property var rows: {
        var out = []
        for (var i = 0; i < QbzBridge.queueModel.length; i++) {
            try { out.push(JSON.parse(QbzBridge.queueModel[i])) } catch (e) {}
        }
        return out
    }
    readonly property var currentRow: rows.length > 0 && rows[0].current ? rows[0] : null
    readonly property var upcomingRows: currentRow ? rows.slice(1) : rows
    readonly property bool queueEmpty: rows.length === 0

    QbzTheme { id: theme }

    // One tab — plain text, no pill (QBZ design rule): active brightest,
    // inactive dim, brighten on hover.
    component QueueTab: Text {
        property bool active: false
        signal clicked()

        font.pixelSize: theme.fontBody
        font.weight: theme.weightSemibold
        color: active ? theme.textPrimary
             : tabArea.containsMouse ? theme.textSecondary : theme.textMuted
        verticalAlignment: Text.AlignVCenter
        height: parent ? parent.height : 0

        MouseArea {
            id: tabArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }

    // One queue row (QueueRow.slint): index or thumbnail, title, artist,
    // duration.
    component QueueRow: Rectangle {
        property var row: ({})
        property int rowIndex: 0
        property bool showNumber: true

        height: 44
        radius: theme.radiusSm
        color: qrArea.containsMouse ? theme.surfaceHover
             : (rowIndex % 2 === 1 ? "#592a2a2a" : "transparent")

        Row {
            anchors.fill: parent
            anchors.leftMargin: theme.spacingSm
            anchors.rightMargin: theme.spacingXs
            anchors.topMargin: 4
            anchors.bottomMargin: 4
            spacing: 9

            Text {
                visible: showNumber
                width: 22
                anchors.verticalCenter: parent.verticalCenter
                text: rowIndex + 1
                color: theme.textMuted
                font.pixelSize: 12
                horizontalAlignment: Text.AlignHCenter
            }
            Rectangle {
                visible: !showNumber
                width: 34
                height: 34
                radius: 4
                anchors.verticalCenter: parent.verticalCenter
                color: theme.surfaceElevated
                clip: true
                Image {
                    anchors.fill: parent
                    source: row.artPath
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
            }
            Column {
                width: parent.width - (showNumber ? 22 : 34) - 9 * 2 - durationText.implicitWidth
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    text: row.title
                    color: theme.textPrimary
                    font.pixelSize: 12
                    font.weight: theme.weightMedium
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    text: row.artist
                    color: theme.textMuted
                    font.pixelSize: 10
                    elide: Text.ElideRight
                }
            }
            Text {
                id: durationText
                anchors.verticalCenter: parent.verticalCenter
                text: row.duration
                color: theme.textMuted
                font.pixelSize: 11
            }
        }
        MouseArea {
            id: qrArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            // POC-NOTE: click-to-play / context menu out of scope.
        }
    }

    Column {
        anchors.fill: parent
        spacing: 0

        // --- Header: tabs + close button -------------------------------
        Item {
            width: parent.width
            height: 44

            Row {
                anchors.left: parent.left
                anchors.leftMargin: theme.spacingMd
                anchors.right: closeBtn.left
                anchors.rightMargin: theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                height: 28
                spacing: 14

                QueueTab {
                    text: QbzBridge.tr("Queue")
                    active: root.tab === 0
                    onClicked: root.tab = 0
                }
                Text {
                    text: "|"
                    color: theme.textDisabled
                    font.pixelSize: theme.fontBody
                    height: parent.height
                    verticalAlignment: Text.AlignVCenter
                }
                QueueTab {
                    text: QbzBridge.tr("History")
                    active: root.tab === 1
                    onClicked: root.tab = 1
                }
            }
            // Close button (mirrors the LyricsSidebar X).
            Rectangle {
                id: closeBtn
                anchors.right: parent.right
                anchors.rightMargin: theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                width: 28
                height: 28
                radius: theme.radiusSm
                color: closeArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: "x"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: closeArea.containsMouse ? "primary" : "secondary"
                }
                MouseArea {
                    id: closeArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.toggleQueue()
                }
            }
        }
        Rectangle {
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // --- Body --------------------------------------------------------
        Item {
            width: parent.width
            height: parent.height - 45

            // ===== Queue tab =====
            Flickable {
                visible: root.tab === 0
                anchors.fill: parent
                clip: true
                contentWidth: width
                contentHeight: queueBody.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: queueBody
                    width: parent.width
                    padding: 10
                    spacing: 12

                    // NOW PLAYING section.
                    Column {
                        visible: root.currentRow !== null
                        width: parent.width - 20
                        spacing: 10
                        Text {
                            text: QbzBridge.tr("NOW PLAYING")
                            color: theme.textMuted
                            font.pixelSize: 11
                            font.weight: theme.weightSemibold
                            font.letterSpacing: 0.5
                        }
                        // Highlighted current-track card.
                        Rectangle {
                            width: parent.width
                            height: 44
                            radius: theme.radiusSm
                            color: theme.surfaceElevated
                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: 6
                                anchors.rightMargin: 6
                                anchors.topMargin: 4
                                anchors.bottomMargin: 4
                                spacing: 9
                                Rectangle {
                                    width: 34
                                    height: 34
                                    radius: 4
                                    anchors.verticalCenter: parent.verticalCenter
                                    color: theme.surfaceCard
                                    clip: true
                                    Image {
                                        anchors.fill: parent
                                        source: root.currentRow ? root.currentRow.artPath : ""
                                        fillMode: Image.PreserveAspectCrop
                                        asynchronous: true
                                    }
                                }
                                Column {
                                    width: parent.width - 34 - 9
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 2
                                    Text {
                                        width: parent.width
                                        text: root.currentRow ? root.currentRow.title : ""
                                        color: theme.textPrimary
                                        font.pixelSize: 12
                                        font.weight: theme.weightMedium
                                        elide: Text.ElideRight
                                    }
                                    Text {
                                        width: parent.width
                                        text: root.currentRow ? root.currentRow.artist : ""
                                        color: theme.textMuted
                                        font.pixelSize: 10
                                        elide: Text.ElideRight
                                    }
                                }
                            }
                        }
                    }

                    // UP NEXT section.
                    Column {
                        visible: root.upcomingRows.length > 0
                        width: parent.width - 20
                        spacing: 8
                        Text {
                            text: QbzBridge.tr("UP NEXT")
                            color: theme.textMuted
                            font.pixelSize: 11
                            font.weight: theme.weightSemibold
                            font.letterSpacing: 0.5
                        }
                        Repeater {
                            model: root.upcomingRows
                            delegate: QueueRow {
                                width: parent ? parent.width : 0
                                row: modelData
                                rowIndex: index
                                showNumber: true
                            }
                        }
                    }

                    // Empty state — no current track and no upcoming.
                    Column {
                        visible: root.queueEmpty
                        width: parent.width - 20
                        topPadding: 48 - 10
                        spacing: 6
                        Text {
                            width: parent.width
                            text: QbzBridge.tr("Your queue is empty")
                            color: theme.textSecondary
                            font.pixelSize: theme.fontBody
                            font.weight: theme.weightMedium
                            horizontalAlignment: Text.AlignHCenter
                        }
                        Text {
                            width: parent.width
                            text: QbzBridge.tr("Play an album or track to get started")
                            color: theme.textMuted
                            font.pixelSize: 12
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                        }
                    }
                }
            }

            // ===== History tab =====
            Column {
                visible: root.tab === 1
                anchors.fill: parent
                anchors.margins: 10
                spacing: 8

                Text {
                    text: QbzBridge.tr("RECENTLY PLAYED")
                    color: theme.textMuted
                    font.pixelSize: 11
                    font.weight: theme.weightSemibold
                    font.letterSpacing: 0.5
                }
                // POC-NOTE: history is always empty until the local
                // play-history store is wired.
                Text {
                    width: parent.width
                    topPadding: 48 - 10
                    text: QbzBridge.tr("Nothing played yet")
                    color: theme.textMuted
                    font.pixelSize: theme.fontBody
                    horizontalAlignment: Text.AlignHCenter
                }
            }
        }
    }
}
