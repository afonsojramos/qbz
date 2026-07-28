// Right-side Lyrics panel — QML port of
// crates/qbz-ui/ui/shell/LyricsSidebar.slint.
//
// Header (LYRICS overline + "{title} - {artist}" meta + display-settings
// trigger + close X), body by status (loading / no-lyrics / offline-miss
// / error / the lines view), source attribution line. Synced rendering:
// LINE-level highlight + auto-follow driven by the 1 Hz position feed
// (QbzBridge.npElapsedSecs; the Slint engine ticks ~30 Hz and can fill
// word-level karaoke — both noted POC-NOTEs). Lines with timestamps are
// click-to-seek (LyricsLinesView behavior).
//
// POC-NOTEs: display settings flyout (size/dimming/uppercase/font/active
// color — Slint defaults hardcoded here: 13/15px, accent, auto-follow
// ON, no dimming of surrounding lines), the translate toggle, word-level
// karaoke, the LRCLIB contribute link is rendered but opens via the
// system browser like upstream.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz

Rectangle {
    id: root
    color: "transparent"
    clip: true

    readonly property var doc: JSON.parse(QbzBridge.lyricsJson)
    readonly property var lines: doc.lines || []
    readonly property bool synced: doc.synced === true

    // Active line (line-level sync, 1 Hz cadence).
    readonly property int activeIndex: {
        if (!synced) return -1
        var nowMs = (QbzBridge.npElapsedSecs || 0) * 1000
        var idx = -1
        for (var i = 0; i < lines.length; i++) {
            if (lines[i].timeMs !== null && lines[i].timeMs <= nowMs) idx = i
        }
        return idx
    }

    QbzTheme { id: theme }

    // Auto-follow hook (the poll updates npElapsedSecs 1 Hz).
    onActiveIndexChanged: followActive()
    function followActive() {
        if (root.activeIndex < 0 || linesColumn.children.length === 0) return
        var row = linesColumn.children[Math.min(root.activeIndex, linesColumn.children.length - 1)]
        var target = row.y + row.height / 2 - linesFlick.height / 2
        target = Math.max(0, Math.min(target, linesFlick.contentHeight - linesFlick.height))
        if (Math.abs(linesFlick.contentY - target) > 30) {
            followAnim2.to = target
            followAnim2.restart()
        }
    }

    Column {
        anchors.fill: parent
        spacing: 0

        // --- Header: overline/meta + controls trigger + close ----------
        Item {
            width: parent.width
            height: 56

            Column {
                anchors.left: parent.left
                anchors.leftMargin: theme.spacingSm
                anchors.right: controlsRow.left
                anchors.rightMargin: theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    text: QbzBridge.tr("LYRICS")
                    color: theme.textMuted
                    font.pixelSize: 11
                    font.weight: theme.weightSemibold
                    font.letterSpacing: 1.5
                }
                Text {
                    visible: QbzBridge.npTitle !== ""
                    width: parent.width
                    text: QbzBridge.npTitle + " - " + QbzBridge.npArtist
                    color: theme.textSecondary
                    font.pixelSize: 12
                    elide: Text.ElideRight
                }
            }
            Row {
                id: controlsRow
                anchors.right: parent.right
                anchors.rightMargin: theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                // Display settings (IconButton) — opens the flyout stub.
                Rectangle {
                    width: 32
                    height: 32
                    radius: theme.radiusSm
                    color: dsArea.containsMouse ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        anchors.centerIn: parent
                        name: "sliders-horizontal"
                        width: 16
                        height: 16
                        tintName: dsArea.containsMouse ? "primary" : "secondary"
                    }
                    MouseArea {
                        id: dsArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: dsFlyout.open()
                    }
                }
                // Close X (Slint-convention, homologated with Queue).
                Rectangle {
                    width: 28
                    height: 28
                    radius: theme.radiusSm
                    color: closeArea.containsMouse ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        anchors.centerIn: parent
                        name: "x"
                        width: 15
                        height: 15
                        tintName: closeArea.containsMouse ? "primary" : "secondary"
                    }
                    MouseArea {
                        id: closeArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzBridge.toggleLyrics()
                    }
                }
            }
        }
        Rectangle {
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // --- Body by status ----------------------------------------------
        Item {
            width: parent.width
            height: parent.height - 57 - sourceLine.height

            // Loading.
            Column {
                visible: doc.status === 1
                anchors.centerIn: parent
                spacing: 10
                QbzSpinner { size: 24; anchors.horizontalCenter: parent.horizontalCenter }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: QbzBridge.tr("Fetching lyrics...")
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                }
            }

            // No lyrics (idle / not-found / ready-with-zero-lines).
            Column {
                visible: doc.status === 0 || doc.status === 3 || (doc.status === 2 && root.lines.length === 0)
                anchors.centerIn: parent
                width: parent.width - 32
                spacing: 12
                Text {
                    width: parent.width
                    text: QbzBridge.tr("No lyrics available")
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                }
                Column {
                    width: parent.width
                    spacing: 2
                    Text {
                        width: parent.width
                        text: QbzBridge.tr("Missing lyrics? You can contribute them to LRCLIB:")
                        color: theme.textMuted
                        font.pixelSize: 10
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "lrclibup.boidu.dev"
                        color: lrcArea.containsMouse ? theme.accentHover : theme.accent
                        font.pixelSize: 10
                        MouseArea {
                            id: lrcArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: Qt.openUrlExternally("https://lrclibup.boidu.dev/")
                        }
                    }
                }
            }

            // Offline with nothing cached.
            Text {
                visible: doc.status === 5
                anchors.centerIn: parent
                width: parent.width - 32
                text: QbzBridge.tr("Lyrics unavailable offline")
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
            }

            // Error.
            Text {
                visible: doc.status === 4
                anchors.centerIn: parent
                width: parent.width - 32
                text: doc.error || ""
                color: "#e0564f"
                font.pixelSize: theme.fontLegal
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
            }

            // Lyrics lines (status 2, lines > 0).
            Flickable {
                id: linesFlick
                visible: doc.status === 2 && root.lines.length > 0
                anchors.fill: parent
                clip: true
                contentWidth: width
                contentHeight: linesColumn.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: linesColumn
                    width: parent.width
                    leftPadding: theme.spacingMd
                    rightPadding: theme.spacingMd
                    topPadding: theme.spacingMd
                    bottomPadding: 48
                    spacing: 10

                    Repeater {
                        model: root.lines
                        delegate: Text {
                            required property var modelData
                            required property int index
                            property bool isActive: index === root.activeIndex

                            width: linesColumn.width
                            text: modelData.text
                            // Synced: active line = accent, 15px; the rest
                            // secondary 13px. Unsynced: uniform secondary.
                            color: !root.synced ? theme.textSecondary
                                : isActive ? theme.accent : theme.textSecondary
                            font.pixelSize: root.synced && isActive ? 15 : 13
                            font.weight: root.synced && isActive ? theme.weightSemibold : theme.weightRegular
                            wrapMode: Text.WordWrap
                            MouseArea {
                                anchors.fill: parent
                                enabled: modelData.timeMs !== null
                                cursorShape: modelData.timeMs !== null ? Qt.PointingHandCursor : Qt.ArrowCursor
                                onClicked: {
                                    if (modelData.timeMs !== null && QbzBridge.npDurationSecs > 0)
                                        QbzBridge.seek(modelData.timeMs / 1000 / QbzBridge.npDurationSecs)
                                }
                            }
                        }
                    }
                }
            }
        }

        // Auto-follow hook (the poll updates npElapsedSecs 1 Hz).
        NumberAnimation {
            id: followAnim2
            target: linesFlick
            property: "contentY"
            duration: 250
            easing.type: Easing.InOutQuad
        }

        // --- Source attribution ------------------------------------------
        Row {
            id: sourceLine
            visible: doc.status === 2 && root.lines.length > 0
            width: parent.width
            leftPadding: theme.spacingSm
            rightPadding: theme.spacingSm
            topPadding: 3
            bottomPadding: 5
            spacing: 3
            Text {
                text: QbzBridge.tr("Lyrics source") + ":"
                color: theme.textMuted
                font.pixelSize: 9
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                text: doc.provider === "lrclib" ? "LRCLIB"
                    : doc.provider === "ovh" ? "lyrics.ovh" : "Qobuz"
                color: theme.accent
                font.pixelSize: 9
                font.weight: theme.weightSemibold
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }

    // --- Display-settings flyout (visual stub — POC-NOTE: the prefs) ----
    Popup {
        id: dsFlyout
        x: parent.width - 260
        y: 64
        width: 260
        padding: 14
        closePolicy: Popup.CloseOnPressOutside
        background: Rectangle {
            color: theme.surfaceCard
            radius: theme.radiusSm
            border.width: 1
            border.color: theme.borderSubtle
        }
        contentItem: Text {
            width: parent ? parent.width : 0
            text: "QBZ Qt POC — display settings are not wired yet (Slint defaults apply)"
            color: theme.textMuted
            font.pixelSize: 12
            wrapMode: Text.WordWrap
        }
    }
}
