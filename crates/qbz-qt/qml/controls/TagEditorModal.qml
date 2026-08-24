// Local album metadata editor. The selected physical version is immutable for
// the lifetime of this modal: QML edits row ids and values only; Rust owns the
// file paths, preflights the whole batch and verifies direct writes.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    id: root
    visible: QbzTagEditor.editorOpen
    enabled: visible
    z: 3150

    property string albumTitle: ""
    property string albumArtist: ""
    property string year: ""
    property string genre: ""
    property string catalogNumber: ""
    property string persistence: "sidecar"
    property string id3Version: "2.4"
    property bool syncSecondary: false
    property var tracks: []
    property var inspection: ({})
    property bool canDirectWrite: false
    property string directReason: ""
    property bool seeded: false
    property var remoteResults: []
    property var remoteMetadata: null
    property string remoteProvider: "musicbrainz"

    function tr(s) { return QbzSession.tr(s, QbzSession.trRev) }
    function parse(value) {
        try { return JSON.parse(value) } catch (e) { return ({}) }
    }
    function cloneRows(rows) {
        var copy = []
        for (var i = 0; i < rows.length; ++i) {
            copy.push({
                "id": rows[i].id || "",
                "fileName": rows[i].fileName || "",
                "title": rows[i].title || "",
                "trackNumber": rows[i].trackNumber || "",
                "discNumber": rows[i].discNumber || "",
                "cueBased": rows[i].cueBased === true
            })
        }
        return copy
    }
    function layerSummary(layers) {
        if (!layers || layers.length === 0)
            return root.tr("None")
        return layers.map(function(layer) {
            return layer.name + " (" + layer.fileCount + "/"
                    + (root.inspection.fileCount || 0) + ")"
        }).join(", ")
    }
    function seed(doc) {
        if (!doc || !doc.tracks)
            return
        albumTitle = doc.albumTitle || ""
        albumArtist = doc.albumArtist || ""
        year = doc.year || ""
        genre = doc.genre || ""
        catalogNumber = doc.catalogNumber || ""
        tracks = cloneRows(doc.tracks)
        inspection = doc.inspection || ({})
        canDirectWrite = doc.canDirectWrite === true
        directReason = doc.directWriteReason || ""
        persistence = "sidecar"
        id3Version = "2.4"
        syncSecondary = false
        remoteResults = []
        remoteMetadata = null
        seeded = true
        keyScope.forceActiveFocus()
    }
    function draft() {
        var rows = tracks.map(function(row) {
            return {
                "id": row.id,
                "title": row.title,
                "trackNumber": row.trackNumber,
                "discNumber": row.discNumber
            }
        })
        return JSON.stringify({
            "albumTitle": albumTitle,
            "albumArtist": albumArtist,
            "year": year,
            "genre": genre,
            "catalogNumber": catalogNumber,
            "persistence": persistence,
            "id3v2Version": id3Version,
            "synchronizeSecondaryTags": syncSecondary,
            "tracks": rows
        })
    }
    function requestSave() {
        if (QbzTagEditor.editorSaving)
            return
        if (persistence === "direct")
            directConfirm.open()
        else
            QbzTagEditor.save(draft())
    }
    function applyRemote() {
        var m = remoteMetadata
        if (!m)
            return
        if (m.title) albumTitle = m.title
        if (m.artist) albumArtist = m.artist
        year = m.year ? String(m.year) : ""
        genre = m.genres && m.genres.length ? m.genres.join("; ") : ""
        catalogNumber = m.catalog_number || ""

        var byPosition = ({})
        var remoteTracks = m.tracks || []
        for (var j = 0; j < remoteTracks.length; ++j) {
            var rt = remoteTracks[j]
            byPosition[String(rt.disc_number) + ":" + String(rt.track_number)] = rt
        }
        var next = cloneRows(tracks)
        for (var i = 0; i < next.length; ++i) {
            var key = String(Number(next[i].discNumber || "1")) + ":"
                    + String(Number(next[i].trackNumber || String(i + 1)))
            var match = byPosition[key]
            if (!match && remoteTracks.length === next.length)
                match = remoteTracks[i]
            if (match) {
                next[i].title = match.title || next[i].title
                next[i].discNumber = String(match.disc_number || next[i].discNumber)
                next[i].trackNumber = String(match.track_number || next[i].trackNumber)
            }
        }
        tracks = next
    }
    function closeEditor() {
        if (!QbzTagEditor.editorSaving)
            QbzTagEditor.close()
    }
    function restoreShellFocus() {
        var p = root
        while (p.parent) {
            if (p.parent.isQbzShellRoot === true) {
                p.parent.forceActiveFocus()
                return
            }
            p = p.parent
        }
    }

    onVisibleChanged: {
        if (visible && !QbzTagEditor.editorLoading)
            seed(parse(QbzTagEditor.editorJson))
        if (!visible) {
            seeded = false
            restoreShellFocus()
        }
    }

    Connections {
        target: QbzTagEditor
        function onEditorJsonChanged() {
            if (root.visible && !QbzTagEditor.editorLoading)
                root.seed(root.parse(QbzTagEditor.editorJson))
        }
        // Rust publishes the immutable document before it clears loading so
        // the UI can never observe a ready flag with stale data. Consume the
        // document on that second signal as well; otherwise the JSON change is
        // deliberately ignored while loading and the modal spins forever.
        function onEditorLoadingChanged() {
            if (root.visible && !QbzTagEditor.editorLoading)
                root.seed(root.parse(QbzTagEditor.editorJson))
        }
        function onRemoteSeqChanged() {
            var event = root.parse(QbzTagEditor.remoteJson)
            if (event.kind === "results") {
                root.remoteResults = event.value || []
                root.remoteMetadata = null
            } else if (event.kind === "metadata") {
                root.remoteMetadata = event.value || null
            }
        }
    }

    QbzTheme { id: theme }

    FocusScope {
        id: keyScope
        anchors.fill: parent
        Keys.onEscapePressed: function(event) {
            root.closeEditor()
            event.accepted = true
        }
    }

    Rectangle {
        anchors.fill: parent
        color: "#bf000000"
        MouseArea { anchors.fill: parent; onClicked: root.closeEditor() }
    }

    Rectangle {
        id: panel
        anchors.centerIn: parent
        width: Math.min(parent.width - 64, 1040)
        height: Math.min(parent.height - 48, 820)
        radius: theme.radiusLg
        color: theme.surfaceMain
        border.width: 1
        border.color: theme.borderSubtle
        clip: true
        MouseArea { anchors.fill: parent }

        Item {
            id: header
            width: parent.width
            height: 70
            Column {
                anchors.left: parent.left
                anchors.leftMargin: 24
                anchors.right: closeButton.left
                anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                spacing: 3
                Text {
                    width: parent.width
                    text: root.tr("Edit metadata")
                    color: theme.textPrimary
                    font.pixelSize: theme.fontHeading
                    font.weight: theme.weightSemibold
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    text: root.tr("Editing the selected physical version")
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    elide: Text.ElideRight
                }
            }
            QbzIconButton {
                id: closeButton
                anchors.right: parent.right
                anchors.rightMargin: 20
                anchors.verticalCenter: parent.verticalCenter
                name: "x"
                tooltipText: root.tr("Close without saving")
                btnEnabled: !QbzTagEditor.editorSaving
                onClicked: root.closeEditor()
            }
        }
        Rectangle { y: header.height; width: parent.width; height: 1; color: theme.borderSubtle }

        Item {
            id: body
            y: header.height + 1
            width: parent.width
            height: parent.height - header.height - footer.height - 2

            QbzSpinner {
                anchors.centerIn: parent
                size: 28
                visible: QbzTagEditor.editorLoading || !root.seeded
            }

            Flickable {
                id: scroll
                visible: root.seeded && !QbzTagEditor.editorLoading
                anchors.fill: parent
                anchors.margins: 20
                contentWidth: width
                contentHeight: content.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: content
                    width: scroll.width - 14
                    spacing: 18

                    Text {
                        text: root.tr("Album")
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightSemibold
                    }
                    Grid {
                        width: parent.width
                        columns: 2
                        columnSpacing: 16
                        rowSpacing: 12

                        Column {
                            width: (content.width - 16) / 2
                            spacing: 5
                            Text { text: root.tr("Album title"); color: theme.textSecondary; font.pixelSize: theme.fontLegal }
                            QbzLineEdit {
                                width: parent.width
                                text: root.albumTitle
                                onEdited: function(value) { root.albumTitle = value }
                                onCommitted: function(value) { root.albumTitle = value }
                            }
                        }
                        Column {
                            width: (content.width - 16) / 2
                            spacing: 5
                            Text { text: root.tr("Album artist"); color: theme.textSecondary; font.pixelSize: theme.fontLegal }
                            QbzLineEdit {
                                width: parent.width
                                text: root.albumArtist
                                onEdited: function(value) { root.albumArtist = value }
                                onCommitted: function(value) { root.albumArtist = value }
                            }
                        }
                        Column {
                            width: (content.width - 16) / 2
                            spacing: 5
                            Text { text: root.tr("Genre"); color: theme.textSecondary; font.pixelSize: theme.fontLegal }
                            QbzLineEdit {
                                width: parent.width
                                text: root.genre
                                onEdited: function(value) { root.genre = value }
                                onCommitted: function(value) { root.genre = value }
                            }
                        }
                        Row {
                            width: (content.width - 16) / 2
                            spacing: 12
                            Column {
                                width: (parent.width - 12) * 0.34
                                spacing: 5
                                Text { text: root.tr("Year"); color: theme.textSecondary; font.pixelSize: theme.fontLegal }
                                QbzLineEdit {
                                    width: parent.width
                                    text: root.year
                                    onEdited: function(value) { root.year = value }
                                    onCommitted: function(value) { root.year = value }
                                }
                            }
                            Column {
                                width: (parent.width - 12) * 0.66
                                spacing: 5
                                Text { text: root.tr("Catalog number"); color: theme.textSecondary; font.pixelSize: theme.fontLegal }
                                QbzLineEdit {
                                    width: parent.width
                                    text: root.catalogNumber
                                    onEdited: function(value) { root.catalogNumber = value }
                                    onCommitted: function(value) { root.catalogNumber = value }
                                }
                            }
                        }
                    }

                    Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }

                    Row {
                        width: parent.width
                        spacing: 16

                        Column {
                            width: (parent.width - 16) * 0.53
                            spacing: 10
                            Text {
                                text: root.tr("Tag layers")
                                color: theme.textPrimary
                                font.pixelSize: theme.fontSection
                                font.weight: theme.weightSemibold
                            }
                            Text {
                                width: parent.width
                                text: root.inspection.error
                                    ? root.inspection.error
                                    : root.tr("Canonical") + ": "
                                      + root.layerSummary(root.inspection.canonicalLayers || [])
                                color: root.inspection.error ? theme.danger : theme.textSecondary
                                font.pixelSize: theme.fontBody
                                wrapMode: Text.WordWrap
                            }
                            Text {
                                width: parent.width
                                text: root.tr("Detected") + ": "
                                    + root.layerSummary(root.inspection.presentLayers || [])
                                color: theme.textMuted
                                font.pixelSize: theme.fontLegal
                                wrapMode: Text.WordWrap
                            }
                            Rectangle {
                                visible: (root.inspection.conflictingFiles || 0) > 0
                                width: parent.width
                                height: conflictText.implicitHeight + 18
                                radius: theme.radiusSm
                                color: theme.warningBg
                                Text {
                                    id: conflictText
                                    anchors.fill: parent
                                    anchors.margins: 9
                                    text: root.tr("Some files contain conflicting tag layers. The canonical layer wins unless synchronization is enabled.")
                                    color: theme.textPrimary
                                    font.pixelSize: theme.fontLegal
                                    wrapMode: Text.WordWrap
                                }
                            }
                        }

                        Column {
                            width: (parent.width - 16) * 0.47
                            spacing: 10
                            Text {
                                text: root.tr("Find metadata")
                                color: theme.textPrimary
                                font.pixelSize: theme.fontSection
                                font.weight: theme.weightSemibold
                            }
                            Row {
                                width: parent.width
                                spacing: 8
                                QbzSelect {
                                    width: 150
                                    menuWidth: 150
                                    options: ["MusicBrainz", "Discogs"]
                                    currentIndex: root.remoteProvider === "discogs" ? 1 : 0
                                    enabled: !QbzTagEditor.remoteSearching && !QbzTagEditor.remoteLoading
                                    onSelected: function(index) {
                                        root.remoteProvider = index === 1 ? "discogs" : "musicbrainz"
                                    }
                                }
                                SettingsButton {
                                    text: QbzTagEditor.remoteSearching ? root.tr("Searching…") : root.tr("Search")
                                    iconName: "search"
                                    minWidth: 0
                                    enabled: !QbzTagEditor.remoteSearching && !QbzTagEditor.remoteLoading
                                    onClicked: QbzTagEditor.searchRemote(root.remoteProvider, root.albumTitle, root.albumArtist)
                                }
                            }
                            ListView {
                                id: resultList
                                width: parent.width
                                height: 116
                                clip: true
                                spacing: 4
                                model: root.remoteResults
                                delegate: Rectangle {
                                    required property var modelData
                                    width: resultList.width
                                    height: 54
                                    radius: theme.radiusSm
                                    color: resultArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                                    Column {
                                        anchors.left: parent.left
                                        anchors.leftMargin: 10
                                        anchors.right: openResult.left
                                        anchors.rightMargin: 8
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 2
                                        Text {
                                            width: parent.width
                                            text: modelData.title || ""
                                            color: theme.textPrimary
                                            font.pixelSize: theme.fontBody
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            width: parent.width
                                            text: (modelData.artist || "")
                                                + (modelData.year ? " · " + modelData.year : "")
                                                + (modelData.track_count ? " · " + modelData.track_count + " " + root.tr("tracks") : "")
                                            color: theme.textMuted
                                            font.pixelSize: theme.fontLegal
                                            elide: Text.ElideRight
                                        }
                                    }
                                    QbzIcon {
                                        id: openResult
                                        anchors.right: parent.right
                                        anchors.rightMargin: 10
                                        anchors.verticalCenter: parent.verticalCenter
                                        name: QbzTagEditor.remoteLoading ? "loader-circle" : "chevron-right"
                                        width: 15
                                        height: 15
                                        tintName: "muted"
                                    }
                                    MouseArea {
                                        id: resultArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        enabled: !QbzTagEditor.remoteLoading
                                        onClicked: QbzTagEditor.loadRemote(root.remoteProvider, parent.modelData.provider_id)
                                    }
                                }
                            }
                            Row {
                                visible: root.remoteMetadata !== null
                                spacing: 8
                                SettingsButton {
                                    text: root.tr("Open source")
                                    iconName: "external-link"
                                    minWidth: 0
                                    onClicked: QbzTagEditor.openRemote(String(root.remoteMetadata.provider || root.remoteProvider), root.remoteMetadata.provider_id || "")
                                }
                                SettingsButton {
                                    text: root.tr("Apply to form")
                                    iconName: "download"
                                    minWidth: 0
                                    onClicked: root.applyRemote()
                                }
                            }
                        }
                    }

                    Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }

                    Row {
                        width: parent.width
                        Text {
                            text: root.tr("Tracks") + " · " + root.tr("Disc") + " / " + root.tr("Track")
                            color: theme.textPrimary
                            font.pixelSize: theme.fontSection
                            font.weight: theme.weightSemibold
                        }
                        Text {
                            width: parent.width - parent.children[0].implicitWidth
                            horizontalAlignment: Text.AlignRight
                            text: root.tr("File names are shown for reference and cannot be changed here.")
                            color: theme.textMuted
                            font.pixelSize: theme.fontLegal
                            elide: Text.ElideRight
                        }
                    }
                    ListView {
                        id: trackList
                        width: parent.width
                        height: Math.min(280, Math.max(108, content.height > scroll.height ? 180 : 280))
                        clip: true
                        spacing: 4
                        model: root.tracks
                        delegate: Rectangle {
                            required property var modelData
                            width: trackList.width
                            height: 44
                            radius: theme.radiusSm
                            color: index % 2 ? theme.surfaceElevated : theme.surfaceCard
                            Row {
                                anchors.fill: parent
                                anchors.margins: 5
                                spacing: 8
                                QbzLineEdit {
                                    width: 52
                                    text: modelData.discNumber
                                    placeholder: root.tr("Disc")
                                    onEdited: function(value) { modelData.discNumber = value }
                                    onCommitted: function(value) { modelData.discNumber = value }
                                }
                                QbzLineEdit {
                                    width: 58
                                    text: modelData.trackNumber
                                    placeholder: root.tr("Track")
                                    onEdited: function(value) { modelData.trackNumber = value }
                                    onCommitted: function(value) { modelData.trackNumber = value }
                                }
                                QbzLineEdit {
                                    width: parent.width - 52 - 58 - fileName.width - 24
                                    text: modelData.title
                                    onEdited: function(value) { modelData.title = value }
                                    onCommitted: function(value) { modelData.title = value }
                                }
                                Text {
                                    id: fileName
                                    width: Math.min(220, implicitWidth)
                                    height: parent.height
                                    text: modelData.fileName
                                    color: theme.textMuted
                                    font.pixelSize: theme.fontLegal
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideMiddle
                                }
                            }
                        }
                    }
                }
                QbzScrollBar {
                    target: scroll
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                }
            }
        }

        Rectangle { y: body.y + body.height; width: parent.width; height: 1; color: theme.borderSubtle }
        Item {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            height: 78

            Row {
                anchors.left: parent.left
                anchors.leftMargin: 20
                anchors.verticalCenter: parent.verticalCenter
                spacing: 12
                Column {
                    spacing: 4
                    Text { text: root.tr("Save changes to"); color: theme.textMuted; font.pixelSize: theme.fontLegal }
                    QbzSelect {
                        menuWidth: 210
                        options: root.canDirectWrite
                            ? [root.tr("Sidecar file (recommended)"), root.tr("Write to audio files")]
                            : [root.tr("Sidecar file (recommended)")]
                        currentIndex: root.persistence === "direct" ? 1 : 0
                        enabled: !QbzTagEditor.editorSaving
                        onSelected: function(index) {
                            if (index === 1 && !root.canDirectWrite)
                                return
                            root.persistence = index === 1 ? "direct" : "sidecar"
                        }
                    }
                }
                Column {
                    visible: root.persistence === "direct"
                    spacing: 4
                    Text { text: root.tr("ID3 writing"); color: theme.textMuted; font.pixelSize: theme.fontLegal }
                    QbzSelect {
                        menuWidth: 130
                        options: ["ID3v2.4", "ID3v2.3"]
                        currentIndex: root.id3Version === "2.3" ? 1 : 0
                        enabled: !QbzTagEditor.editorSaving
                        onSelected: function(index) { root.id3Version = index === 1 ? "2.3" : "2.4" }
                    }
                }
                Row {
                    visible: root.persistence === "direct"
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    QbzToggle {
                        checked: root.syncSecondary
                        enabled: !QbzTagEditor.editorSaving
                        onToggled: function(value) { root.syncSecondary = value }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.tr("Synchronize compatible tag layers")
                        color: theme.textSecondary
                        font.pixelSize: theme.fontLegal
                    }
                }
            }

            Text {
                visible: root.persistence === "sidecar" && !root.canDirectWrite && root.directReason !== ""
                anchors.left: parent.left
                anchors.leftMargin: 246
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 5
                width: Math.min(400, parent.width * 0.4)
                text: root.directReason
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                elide: Text.ElideRight
            }

            Row {
                anchors.right: parent.right
                anchors.rightMargin: 20
                anchors.verticalCenter: parent.verticalCenter
                spacing: 10
                Text {
                    visible: QbzTagEditor.editorSaving
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzTagEditor.editorProgressTotal > 0
                        ? root.tr("Writing") + " " + QbzTagEditor.editorProgressCurrent + "/" + QbzTagEditor.editorProgressTotal
                        : root.tr("Saving…")
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                }
                SettingsButton {
                    text: root.tr("Cancel")
                    minWidth: 0
                    enabled: !QbzTagEditor.editorSaving
                    onClicked: root.closeEditor()
                }
                QbzPrimaryButton {
                    label: QbzTagEditor.editorSaving ? root.tr("Saving…") : root.tr("Save")
                    btnHeight: 36
                    labelSize: theme.fontBody
                    btnEnabled: root.seeded && !QbzTagEditor.editorSaving
                    onClicked: root.requestSave()
                }
            }
        }
    }

    QbzConfirmModal {
        id: directConfirm
        anchors.fill: parent
        title: root.tr("Write metadata into audio files?")
        body: root.tr("QBZ will preflight every file, update its canonical tag layer, then read it back to verify the change. Keep a backup if these files cannot be replaced.")
        confirmLabel: root.tr("Write and verify")
        danger: false
        onConfirmed: QbzTagEditor.save(root.draft())
    }
}
