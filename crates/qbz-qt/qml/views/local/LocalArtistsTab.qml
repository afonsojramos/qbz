// Artists tab body (LocalLibraryView.slint:2213) — two-column
// master/detail: the merged, A-Z grouped artist rail on the left (280px,
// with its own AlphaStrip), that artist's albums on the right.
//
// The Slint rail is a plain Flickable with a nested for-per-section; here
// the sections are flattened into one entry array so the rail is a WINDOWING
// ListView — a 4k-artist library mounts a viewport, not 4k rows.

import QtQuick
import com.blitzfc.qbz
import "../../theme"

Item {
    id: root

    property var view: null

    QbzTheme { id: theme }

    readonly property int headerH: 22
    readonly property int rowH: 62

    // --------------------------- entry model ------------------------------
    property var entries: []
    property var alphaJumps: []
    function rebuild() {
        var groups = view ? view.artistsGrouped : []
        var out = []
        var jumps = []
        for (var i = 0; i < groups.length; i++) {
            var g = groups[i]
            if (!g.items || g.items.length === 0) continue
            jumps.push({ "letter": g.letter, "index": out.length })
            out.push({ "t": 0, "label": g.letter })
            for (var j = 0; j < g.items.length; j++) out.push({ "t": 1, "item": g.items[j] })
        }
        entries = out
        alphaJumps = jumps
    }
    Component.onCompleted: rebuild()
    Connections {
        target: root.view
        function onArtistsGroupedChanged() { root.rebuild() }
    }

    QbzSpinner {
        anchors.centerIn: parent
        size: 36
        visible: QbzLocal.localArtistsLoading
    }
    // Empty library — icon + line (Slint :2222).
    Column {
        visible: !QbzLocal.localArtistsLoading && root.view.artists.length === 0
        anchors.centerIn: parent
        spacing: 10
        QbzIcon {
            anchors.horizontalCenter: parent.horizontalCenter
            name: "user"
            width: 44
            height: 44
            tintName: "muted"
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: QbzSession.tr("No artists in your local library yet.", QbzSession.trRev)
            color: theme.textMuted
            font.pixelSize: theme.fontBody
        }
    }

    Row {
        anchors.fill: parent
        visible: !QbzLocal.localArtistsLoading && root.view.artists.length > 0
        spacing: 0

        // ---------------------- master rail ------------------------------
        Item {
            width: 280
            height: parent.height
            clip: true

            Column {
                anchors.fill: parent
                anchors.leftMargin: 24
                anchors.topMargin: 12
                spacing: 8

                Text {
                    text: root.view.artistsVisibleCount + " "
                        + QbzSession.tr("artists", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    font.weight: theme.weightSemibold
                }
                Text {
                    visible: root.view.artistsVisibleCount === 0
                    text: QbzSession.tr("No artists match your search.", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                }
                Item {
                    visible: root.view.artistsVisibleCount > 0
                    width: parent.width
                    height: parent.height - 30

                    ListView {
                        id: rail
                        anchors.fill: parent
                        anchors.rightMargin: 22
                        clip: true
                        cacheBuffer: 62 * 8
                        boundsBehavior: Flickable.StopAtBounds
                        model: root.entries
                        onContentYChanged: root.report()
                        onModelChanged: root.report()

                        delegate: Loader {
                            required property var modelData
                            width: rail.width
                            height: modelData.t === 0 ? root.headerH : root.rowH
                            sourceComponent: modelData.t === 0 ? letterComp : artistComp

                            Component {
                                id: letterComp
                                Item {
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: modelData.label
                                        color: theme.textSecondary
                                        font.pixelSize: theme.fontLegal
                                        font.weight: theme.weightSemibold
                                    }
                                }
                            }
                            Component {
                                id: artistComp
                                LocalArtistRow {
                                    width: rail.width
                                    item: modelData.item
                                    artSource: root.view.artMap[modelData.item.artKey] || ""
                                    selected: root.view.selectedArtist === modelData.item.name
                                    onPicked: function (name) { root.view.selectArtist(name) }
                                }
                            }
                        }
                    }
                    AlphaStrip {
                        visible: root.alphaJumps.length > 0
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.bottom: parent.bottom
                        jumps: root.alphaJumps
                        onJump: function (ordinal, index) {
                            rail.positionViewAtIndex(index, ListView.Beginning)
                        }
                    }
                }
            }
        }

        Rectangle {
            width: 1
            height: parent.height
            color: theme.borderSubtle
        }

        // ---------------------- detail pane ------------------------------
        Item {
            width: parent.width - 281
            height: parent.height
            clip: true

            Column {
                visible: root.view.selectedArtist === ""
                anchors.centerIn: parent
                spacing: 10
                QbzIcon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    name: "user"
                    width: 44
                    height: 44
                    tintName: "muted"
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: QbzSession.tr("Select an artist to view their albums", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: theme.fontBody
                }
            }

            Column {
                anchors.fill: parent
                anchors.leftMargin: 28
                anchors.rightMargin: 8
                anchors.topMargin: 14
                spacing: 12
                visible: root.view.selectedArtist !== ""

                Column {
                    width: parent.width
                    spacing: 2
                    Text {
                        width: parent.width
                        text: root.view.selectedArtist
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightBold
                        elide: Text.ElideRight
                    }
                    Text {
                        text: root.view.artistAlbums.length + " "
                            + QbzSession.tr("albums", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: theme.fontLegal
                    }
                }
                Text {
                    visible: root.view.artistAlbums.length === 0
                    width: parent.width
                    topPadding: 24
                    text: QbzSession.tr("No albums found", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: theme.fontBody
                    horizontalAlignment: Text.AlignHCenter
                }
                LocalAlbumCollection {
                    visible: root.view.artistAlbums.length > 0
                    width: parent.width
                    height: parent.height - 60
                    view: root.view
                    rows: root.view.artistAlbums
                    grouped: false
                    viewMode: "grid"
                    showSource: true
                    onOpenRequested: function (id) { root.view.openAlbum(id) }
                    onPlayRequested: function (id) { QbzLocal.playAlbum(id, false) }
                    onEnqueueRequested: function (id, m) { QbzLocal.enqueue("album", id, m) }
                }
            }
        }
    }

    // Artist avatars ride the same id-keyed artwork window as every other
    // cover; the rail reports its mounted band.
    function report() {
        if (!view || !rail) return
        var first = rail.indexAt(4, rail.contentY + 1)
        var last = rail.indexAt(4, rail.contentY + Math.max(1, rail.height) - 1)
        if (first < 0) first = 0
        if (last < 0) last = first + 10
        view.queueWindowReport(view.artistsVisible, Math.max(0, first - 4), last + 4)
    }
}
