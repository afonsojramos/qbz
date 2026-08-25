// Full-page local album metadata editor. The historical filename is retained
// so existing qrc registrations stay stable; ContentRouter is its only mount.
// The selected physical version is immutable for the lifetime of this view:
// QML edits row ids and values only; Rust owns paths and verifies direct writes.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    id: root
    visible: true
    enabled: true

    property string albumTitle: ""
    property string albumArtist: ""
    property string albumArtistsText: ""
    property bool compilation: false
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
    property string musicbrainzReleaseId: ""
    property string musicbrainzReleaseGroupId: ""
    property string musicbrainzAlbumArtistIdsText: ""
    property string discogsReleaseId: ""
    property var artwork: ({})
    property var artworkResults: []
    property string artworkProvider: "musicbrainz"
    property int selectedTrackIndex: -1

    function tr(s) { return QbzSession.tr(s, QbzSession.trRev) }
    function parse(value) {
        try { return JSON.parse(value) } catch (e) { return ({}) }
    }
    function listText(values) {
        return (values || []).join("; ")
    }
    function splitList(value) {
        return String(value || "").split(/[;\n]/).map(function(part) {
            return part.trim()
        }).filter(function(part) { return part.length > 0 })
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
                "artistCredit": rows[i].artistCredit || "",
                "artists": (rows[i].artists || []).slice(),
                "composers": (rows[i].composers || []).slice(),
                "performers": (rows[i].performers || []).slice(),
                "musicbrainzRecordingId": rows[i].musicbrainzRecordingId || "",
                "musicbrainzTrackId": rows[i].musicbrainzTrackId || "",
                "musicbrainzArtistIds": (rows[i].musicbrainzArtistIds || []).slice(),
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
        albumArtistsText = listText(doc.albumArtists)
        compilation = doc.compilation === true
        year = doc.year || ""
        genre = doc.genre || ""
        catalogNumber = doc.catalogNumber || ""
        musicbrainzReleaseId = doc.musicbrainzReleaseId || ""
        musicbrainzReleaseGroupId = doc.musicbrainzReleaseGroupId || ""
        musicbrainzAlbumArtistIdsText = listText(doc.musicbrainzAlbumArtistIds)
        discogsReleaseId = doc.discogsReleaseId || ""
        artwork = doc.artwork || ({})
        artworkResults = []
        tracks = cloneRows(doc.tracks)
        inspection = doc.inspection || ({})
        canDirectWrite = doc.canDirectWrite === true
        directReason = doc.directWriteReason || ""
        persistence = "sidecar"
        id3Version = "2.4"
        syncSecondary = false
        remoteResults = []
        remoteMetadata = null
        selectedTrackIndex = tracks.length > 0 ? 0 : -1
        seeded = true
        keyScope.forceActiveFocus()
    }
    function draft() {
        var rows = tracks.map(function(row) {
            return {
                "id": row.id,
                "title": row.title,
                "trackNumber": row.trackNumber,
                "discNumber": row.discNumber,
                "artistCredit": row.artistCredit,
                "artists": row.artists || [],
                "composers": row.composers || [],
                "performers": row.performers || [],
                "musicbrainzRecordingId": row.musicbrainzRecordingId || "",
                "musicbrainzTrackId": row.musicbrainzTrackId || "",
                "musicbrainzArtistIds": row.musicbrainzArtistIds || []
            }
        })
        return JSON.stringify({
            "albumTitle": albumTitle,
            "albumArtist": albumArtist,
            "albumArtists": splitList(albumArtistsText),
            "compilation": compilation,
            "year": year,
            "genre": genre,
            "catalogNumber": catalogNumber,
            "musicbrainzReleaseId": musicbrainzReleaseId,
            "musicbrainzReleaseGroupId": musicbrainzReleaseGroupId,
            "musicbrainzAlbumArtistIds": splitList(musicbrainzAlbumArtistIdsText),
            "discogsReleaseId": discogsReleaseId,
            "artworkToken": artwork.token || "",
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
        if (m.artist_credits && m.artist_credits.length)
            albumArtistsText = listText(m.artist_credits.map(function(credit) { return credit.name }))
        year = m.year ? String(m.year) : ""
        genre = m.genres && m.genres.length ? m.genres.join("; ") : ""
        catalogNumber = m.catalog_number || ""
        if (String(m.provider) === "musicbrainz") {
            musicbrainzReleaseId = m.provider_id || ""
            musicbrainzReleaseGroupId = m.release_group_id || ""
            musicbrainzAlbumArtistIdsText = listText((m.artist_credits || []).map(function(credit) {
                return credit.provider_id || ""
            }).filter(function(value) { return value.length > 0 }))
        } else if (String(m.provider) === "discogs") {
            discogsReleaseId = m.provider_id || ""
        }

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
                next[i].artistCredit = match.artist_credit || next[i].artistCredit
                if (match.artist_credits && match.artist_credits.length) {
                    next[i].artists = match.artist_credits.map(function(credit) { return credit.name })
                    next[i].musicbrainzArtistIds = match.artist_credits.map(function(credit) {
                        return credit.provider_id || ""
                    }).filter(function(value) { return value.length > 0 })
                }
                next[i].musicbrainzRecordingId = match.recording_id || next[i].musicbrainzRecordingId
                next[i].musicbrainzTrackId = match.track_id || next[i].musicbrainzTrackId
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

    Component.onCompleted: {
        if (!QbzTagEditor.editorLoading)
            seed(parse(QbzTagEditor.editorJson))
    }
    Component.onDestruction: {
        QbzTagEditor.leave()
        restoreShellFocus()
    }

    Connections {
        target: QbzTagEditor
        function onEditorJsonChanged() {
            if (!QbzTagEditor.editorLoading)
                root.seed(root.parse(QbzTagEditor.editorJson))
        }
        // Rust publishes the immutable document before it clears loading so
        // the UI can never observe a ready flag with stale data. Consume the
        // document on that second signal as well; otherwise the JSON change is
        // deliberately ignored while loading and the modal spins forever.
        function onEditorLoadingChanged() {
            if (!QbzTagEditor.editorLoading)
                root.seed(root.parse(QbzTagEditor.editorJson))
        }
        function onRemoteSeqChanged() {
            var event = root.parse(QbzTagEditor.remoteJson)
            if (event.kind === "results") {
                root.remoteResults = event.value || []
                root.remoteMetadata = null
            } else if (event.kind === "metadata") {
                root.remoteMetadata = event.value || null
            } else if (event.kind === "artwork-results") {
                root.artworkResults = event.value || []
            } else if (event.kind === "artwork-selected") {
                root.artwork = event.value || ({})
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
        id: panel
        anchors.fill: parent
        radius: 0
        color: theme.surfaceMain
        clip: true

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

            TagEditorWorkspace {
                anchors.fill: parent
                anchors.margins: 16
                visible: root.seeded && !QbzTagEditor.editorLoading
                editor: root
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
