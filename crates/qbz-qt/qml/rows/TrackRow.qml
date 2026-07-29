// TrackRow — THE track list row (primitives/TrackRow.slint, POC arm
// subset), consolidated in phase 22 from FOUR copies (AlbumView
// .AlbumTrackRow, PlaylistView.PlTrackRow, LibraryView.TrackListRow,
// SearchView.SearchTrackRow). 50px, radius 8: number→play cell (pause
// swap + accent pill when it is the now-playing row — Slint-universal),
// optional 36px art cell, title+explicit / artist column, optional 220px
// album link, duration, quality (BARE QualityBadgeFull — the tier label over
// the exact bit-depth / sample-rate line), optional
// heart, optional download slot (offline column — glyph stub or reserved
// spacer until the offline cache lands, POC-NOTE), ⋯ CardMenu.
//
// item contract: { id, title, artist, artistId, album, albumId, number?,
// duration, qualityTier, qualityDetail, explicit, isFavorite, artPath? }
// (plus playlistTrackId for the remove-from-playlist arm). `qualityDetail`
// is the bare exact-quality string ("16-bit / 44.1 kHz"); when a producer
// does not carry it yet the badge degrades to the tier label alone rather
// than to a blank cell.
//
// Arms: showArtwork / showAlbum / showFavorite / showDownload /
// downloadGlyph / showMenu / zebra / artistLink / clickPlays /
// menuShowLater / menuShowGoTo / menuShowFavorite / menuShowRemove.
// (`qualityStyle` is RETIRED — kept declared, inert, see below.)
// Signals: playRequested() (per-site play: album-scoped, playlist-scoped
// or plain), enqueueRequested(mode) ("next"|"later"|"queue"),
// removeRequested(), bodyDragStarted(index) (fired BEFORE the shared
// dragStart — the #589 reorder pre-hook).
// Favorite toggling, Go-to-artist/album, Share and Track info are
// identical on every site — handled internally. The row BODY is the drag
// source (6px threshold, ghost + sidebar drops in main.rs) and its RIGHT
// press opens the very same menu the ⋯ button does.
//
// --- Menu inventory vs primitives/TrackContextMenu.slint ----------------
// The .slint menu is FLAT (no separators) in this order: Play now · Play
// next · Play later · Add to queue · Create QBZ radio · Create Qobuz radio ·
// Add to library · Add to mixtape · Add to playlist · Remove from
// playlist(danger) · Share Qobuz link · Share Song.link · Make available
// offline | Refresh + Remove offline copy(danger) · Go to album · Go to
// artist · Track info. Everything the Qt bridge has a seam for is here, in
// that order; the rest is gated OFF by the `has*Seam` constants below
// rather than rendered as a row that silently does nothing.
//
// Deliberately NOT consolidated here (Slint-distinct): QueuePanel.QueueRow
// (QueueItem data + queue-op menu + press-and-hold reorder) and
// LibraryView.FeedListRow (mixed-kind feed row — inline in Slint too).

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../shell"
import "../theme"

Rectangle {
    id: root

    property var item: ({})
    property int number: 0
    property bool showArtwork: false
    property bool showAlbum: false
    property bool showFavorite: true
    property bool showDownload: false
    property bool downloadGlyph: false
    property bool showMenu: true
    property bool zebra: false
    property bool artistLink: false
    property bool clickPlays: true
    /// RETIRED and INERT. primitives/TrackRow.slint has exactly ONE quality
    /// form (the bare QualityBadgeFull below) — there was never an icon/text
    /// split to arm. Kept declared ONLY so the two existing call sites that
    /// still assign it (AlbumView.qml `"text"`, local/LocalTrackRow.qml
    /// `"icon"`) keep loading; assigning a non-existent property is a hard
    /// QML instantiation error. Both assignments are listed for removal.
    property string qualityStyle: "icon"
    property bool menuShowLater: true
    property bool menuShowGoTo: true
    property bool menuShowFavorite: true
    property bool menuShowRemove: false
    // Drag source arm (additive; default = the existing behaviour). The
    // Local Library rows set this false: their `item.id` is a local DB row
    // id, and the sidebar drop handler forwards whatever it receives to
    // `playlist add-tracks` as a QOBUZ catalog id — a local row dropped on a
    // playlist would silently add an unrelated catalog track.
    property bool draggable: true
    // Per-row artwork placeholder (see the 36px cell below). The host view
    // owns the phase clock so one timer drives every row.
    property bool artPending: false
    property bool skelPhase: false
    property int artSettleMs: 0

    // Catalog-backed row? `draggable` is ALREADY the "item.id is a Qobuz
    // catalog track id" predicate (the Local Library / ephemeral rows set it
    // false because their id is a local DB row id — see the note above), so
    // the two entries that hit the Qobuz catalog by id ride it instead of
    // asking every host to pass a new arm.
    readonly property bool catalogRow: root.draggable
    property bool menuShowShare: root.catalogRow
    property bool menuShowTrackInfo: root.catalogRow

    // --- Seams the Qt bridge does NOT have yet (menu-parity round) -------
    // Each maps to a TrackContextMenu.slint entry. OFF = the row is not
    // built at all, never rendered-and-inert. Flip the constant AND fill the
    // matching `menuAction` branch when the invokable lands; the entry then
    // appears in its .slint-correct slot. Why each is missing:
    //   radio        no radio invokable on any Qt bridge object
    //   mixtape      QbzLocal.albumAddToMixtape is LOCAL-album only
    //   playlistAdd  no picker modal, no by-id add (only the sidebar drag)
    //   songlink     needs the ISRC -> Deezer -> Odesli round-trip (backend)
    //   offlineCache no per-track cache bridge (the download cell is a stub)
    readonly property bool hasRadioSeam: false
    readonly property bool hasMixtapeSeam: false
    readonly property bool hasPlaylistAddSeam: false
    readonly property bool hasSonglinkSeam: false
    readonly property bool hasOfflineCacheSeam: false

    signal playRequested()
    signal enqueueRequested(string mode)
    signal removeRequested()
    signal bodyDragStarted(int index)

    QbzTheme { id: theme }

    // --- Polarity-baked tokens for the play cell (#638) -------------------
    // `theme.alphaTier(pct)` IS Slint's `Theme.alpha-N`: one ramp per theme,
    // white-based on dark, black-based on light. The `.length` guard covers
    // the pre-publish frame QbzTheme documents (its baked fallback carries no
    // ramp) — without it the disc would render fill-less AND ring-less, which
    // is the exact failure this block exists to prevent. The literals are the
    // Slint dark values and their light-polarity mirrors.
    readonly property color playCellFill: theme.alphaTiers.length > 0
        ? theme.alphaTier(10) : (theme.isDark ? "#1affffff" : "#1a000000")
    readonly property color playCellRing: theme.alphaTiers.length > 0
        ? theme.alphaTier(15) : (theme.isDark ? "#26ffffff" : "#26000000")
    /// Slint's `Theme.text-primary` icon tint in baked-variant terms: the
    /// "primary" bake is a literal #ffffff (QbzIcon.qml), so a light theme
    /// has to take "black" or the glyph is white-on-white. Also drives the
    /// hover tints of the row's trailing controls, which had the same hole.
    readonly property string playGlyphTint: theme.isDark ? "primary" : "black"
    /// TrackRow.slint:123-125 — the row hover is `Theme.alpha-8`, not
    /// surface-hover, and the .slint says why: "Hover uses the polarity-baked
    /// alpha ramp so the hover state is visible on light themes too". The
    /// zebra stripe stays the literal, per the same comment ("Zebra kept
    /// as-is to avoid a visible dark-theme stripe shift").
    readonly property color rowHoverBg: theme.alphaTiers.length > 0
        ? theme.alphaTier(8) : (theme.isDark ? "#14ffffff" : "#14000000")

    width: parent ? parent.width : 0
    height: 50
    radius: 8
    color: hovered ? rowHoverBg : (zebra && number % 2 === 0 ? "#07ffffff" : "transparent")

    // `playArea` is in here for the reason TrackPlayCell.slint:76-79 spells
    // out: the cell owns a hover-enabled area of its own, and "Slint's
    // has-hover does not propagate to ancestor TouchAreas" — Qt's does not
    // either, so without this the row (and the play circle with it) would go
    // un-hovered exactly while the pointer sits on the play button.
    readonly property bool hovered: trArea.containsMouse || favArea.containsMouse
        || moreArea.containsMouse || playArea.containsMouse
    readonly property bool isActive: QbzPlayer.npTrackId === (item.id || "")
    readonly property int cellsRight: 70 + 92 + (showFavorite ? 28 : 0) + (showDownload ? 28 : 0) + (showMenu ? 32 : 0)
    readonly property int cellsLeft: 32 + (showArtwork ? 36 : 0) + (showAlbum ? 220 : 0)
    readonly property int gaps: (3 + (showArtwork ? 1 : 0) + (showAlbum ? 1 : 0)
        + (showFavorite ? 1 : 0) + (showDownload ? 1 : 0) + (showMenu ? 1 : 0)) * 14

    // Static now-playing mark: 3px accent pill on the left edge.
    Rectangle {
        visible: root.isActive
        x: 2
        y: 7
        width: 3
        height: parent.height - 14
        radius: 1.5
        color: theme.accent
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        spacing: 14

        // Number cell — the number-variant of primitives/TrackPlayCell.slint
        // (:172-232), the play affordance shared by every track row.
        //
        // --- LIGHT-THEME LEGIBILITY (Slint 2.0.2 / #638, "Track-row play
        // --- glyph … legible on light themes") ---------------------------
        // TrackPlayCell.slint:201-207 states the defect and the fix in its
        // own words: "Circular backing so the play/pause glyph is legible on
        // EVERY theme (the old bare #ffffff glyph vanished on light themes —
        // white-on-white, no scrim like the artwork variant has). Fill/border/
        // shadow use the polarity-baked alpha ramp (black-alpha on light,
        // white-alpha on dark) and the glyph uses text-primary."
        //
        // The port had the pre-fix shape: a hardcoded `#3dffffff` disc, no
        // ring, and the "primary" icon bake — which is a literal #ffffff, not
        // text-primary (QbzIcon.qml bakes tints into the SVG). Both halves are
        // now the polarity-baked ramp: theme.alphaTier() is white-based on
        // dark themes and black-based on light (qbz-theme colors.rs:125-129,
        // republished per theme by theme_qt.rs), and `playGlyphTint` resolves
        // text-primary to the only baked dark variant when the theme is light.
        //
        // Numbers off the .slint: 24px disc, 1.5px ring ("a 1px stroked circle
        // aliases badly in femtovg", :215), 16px glyph.
        // The accent-ring arm (:208-229) is the playing row: transparent fill,
        // accent circumference, accent glyph.
        // POC-NOTE: the .slint also drops a 6px `card-shadow` under the disc.
        // Qt's DropShadow is a Qt5Compat graphical effect and renders NOTHING
        // on this port's software path (the finding that killed ColorOverlay
        // in QbzIcon), so the shadow is dropped rather than faked; the ring
        // carries the separation on its own.
        Item {
            id: playCell
            width: 32
            height: 40
            anchors.verticalCenter: parent.verticalCenter
            // `show-overlay: row-hovered || is-active` (TrackPlayCell.slint
            // :93) — the playing row keeps the affordance at rest, which is
            // what the accent ring is FOR. The port showed it on hover only,
            // so the ring never appeared.
            readonly property bool showOverlay: root.hovered || root.isActive
            // `accent-ring` (:108): the playing row, static form.
            readonly property bool accentRing: root.isActive && QbzPlayer.npPlaying
            Text {
                visible: !playCell.showOverlay
                anchors.centerIn: parent
                text: root.number
                color: theme.textMuted
                font.pixelSize: 13
            }
            Rectangle {
                visible: playCell.showOverlay
                anchors.centerIn: parent
                width: 24
                height: 24
                radius: 12
                color: playCell.accentRing ? "transparent" : root.playCellFill
                border.width: 1.5
                border.color: playCell.accentRing ? theme.accent : root.playCellRing
                QbzIcon {
                    anchors.centerIn: parent
                    // `show-pause` (:101-104): PAUSE only while the playing
                    // row is hovered — at rest the playing row shows PLAY,
                    // "an affordance, not a state badge: the accent ring +
                    // the row's edge mark carry the state".
                    name: playCell.accentRing && root.hovered ? "pause" : "play-fill"
                    width: 16
                    height: 16
                    tintName: playCell.accentRing ? "accent" : root.playGlyphTint
                }
            }
            // TrackPlayCell.slint:234-243 — the cell is its OWN click target:
            // a non-current cell plays the track, the current cell toggles
            // play/pause. Without it the circle was a control that rendered
            // and did nothing on the album view (clickPlays:false there, so
            // the press fell through to the row body, which only reacts to a
            // double-click). Declared LAST so it sits ABOVE the glyph, and it
            // is above the row-body area regardless (that one is pinned z:-1).
            MouseArea {
                id: playArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    if (root.isActive) QbzPlayer.togglePlay()
                    else root.playRequested()
                }
            }
        }
        // 36px artwork cell (showArtwork arm).
        Rectangle {
            visible: root.showArtwork
            width: 36
            height: 36
            anchors.verticalCenter: parent.verticalCenter
            radius: 4
            color: theme.surfaceElevated
            clip: true
            RoundedImage {
                anchors.fill: parent
                source: root.item.artPath || ""
                radius: 4
            }
            // Per-item cover placeholder — clears when THIS row's cover lands,
            // which is what makes a long list read as progressive instead of
            // filling in one lump. Host views drive the three properties;
            // default-off, so no existing call site changes.
            QbzSkeleton {
                variant: "art"
                anchors.fill: parent
                blockRadius: 4
                visible: root.artPending
                phase: root.skelPhase
                settleMs: root.artSettleMs
            }
        }
        // Title (+ explicit) / artist.
        Column {
            width: parent.width - root.cellsLeft - root.cellsRight - root.gaps
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Row {
                spacing: 6
                Text {
                    text: root.item.title || ""
                    color: theme.textPrimary
                    font.pixelSize: 14
                    font.weight: theme.weightMedium
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth, parent.parent.width - (root.item.explicit ? 22 : 0))
                }
                Rectangle {
                    visible: root.item.explicit === true
                    width: 16
                    height: 16
                    radius: 3
                    anchors.verticalCenter: parent.verticalCenter
                    color: theme.surfaceElevated
                    Text {
                        anchors.centerIn: parent
                        text: "E"
                        color: theme.textMuted
                        font.pixelSize: 10
                        font.weight: theme.weightSemibold
                    }
                }
            }
            Text {
                width: parent.width
                visible: (root.item.artist || "") !== ""
                text: root.item.artist || ""
                color: root.artistLink && root.item.artistId && artistLinkArea.containsMouse
                    ? theme.textPrimary : theme.textMuted
                font.pixelSize: 12
                elide: Text.ElideRight
                MouseArea {
                    id: artistLinkArea
                    anchors.fill: parent
                    enabled: root.artistLink && !!root.item.artistId
                    hoverEnabled: true
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: QbzArtist.openArtist(root.item.artistId)
                }
            }
        }
        // Album (link) column (showAlbum arm).
        Text {
            visible: root.showAlbum
            width: 220
            anchors.verticalCenter: parent.verticalCenter
            text: root.item.album || ""
            color: albumArea.containsMouse ? theme.accent : theme.textMuted
            font.pixelSize: 12
            elide: Text.ElideRight
            MouseArea {
                id: albumArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: root.item.albumId ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: if (root.item.albumId) QbzAlbum.openAlbum(root.item.albumId)
            }
        }
        // Duration.
        Text {
            width: 70
            anchors.verticalCenter: parent.verticalCenter
            text: root.item.duration || ""
            color: theme.textMuted
            font.pixelSize: 12
            horizontalAlignment: Text.AlignHCenter
        }
        // Quality (92px) — the BARE badge, 1:1 with primitives/TrackRow.slint
        // 578-592: a 92px cell, `alignment: center` on both axes, holding a
        // QualityBadgeFull with `show-icon: false` + `bare: true`. That is the
        // tier label ("CD"/"HI-RES"/"MP3"/"LOSSLESS") stacked over the exact
        // bit-depth / sample-rate line, with no chip background or border, so
        // it blends into the row instead of reading as a contained badge.
        // The .slint has ONE form here — no icon variant, no bare-text
        // variant — which is why `qualityStyle` above is inert.
        //
        // The cell keeps its FIXED 92px. That number is a term of
        // `cellsRight`, which is what sizes the title column, so a wider badge
        // MUST NOT widen the cell or the title would be pushed into the
        // trailing controls. The badge is centred and the cell clips; at 8/9px
        // the longest real detail ("24-bit / 352.8 kHz") measures ~80px, so
        // the clip is a guard, never the normal path.
        //
        // Cheaper than what it replaces, too: QualityMini resolves to
        // QualityBadge, which carries a ToolTip popup and a hover MouseArea
        // per row; this one is two Texts.
        Item {
            width: 92
            height: parent.height
            clip: true
            QualityBadgeFull {
                anchors.centerIn: parent
                tier: root.item.qualityTier || ""
                detail: root.item.qualityDetail || ""
                showIcon: false
                bare: true
            }
        }
        // Favorite (showFavorite arm).
        Rectangle {
            visible: root.showFavorite
            width: 28
            height: 28
            radius: theme.radiusSm
            anchors.verticalCenter: parent.verticalCenter
            color: favArea.containsMouse ? theme.surfaceElevated : "transparent"
            QbzIcon {
                anchors.centerIn: parent
                name: root.item.isFavorite ? "heart-filled" : "heart"
                width: 16
                height: 16
                // TrackRow.slint:619 — hover raises the glyph to
                // Theme.text-primary, NOT to white (same #638 hole).
                tintName: root.item.isFavorite
                    ? "favorite"
                    : (favArea.containsMouse ? root.playGlyphTint : "muted")
            }
            MouseArea {
                id: favArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    root.item.isFavorite = !root.item.isFavorite
                    QbzLibrary.libraryToggleFavorite("track", root.item.id)
                }
            }
        }
        // Download slot (showDownload arm): inert glyph stub or reserved
        // spacer (the offline-cache column — not ported, POC-NOTE; the
        // Slint reserves the slot so the grid stays aligned).
        Item {
            visible: root.showDownload
            width: 28
            height: 28
            anchors.verticalCenter: parent.verticalCenter
            QbzIcon {
                visible: root.downloadGlyph
                anchors.centerIn: parent
                name: "cloud-download"
                width: 16
                height: 16
                tintName: "muted"
            }
        }
        // ⋯ menu (showMenu arm).
        Rectangle {
            visible: root.showMenu
            width: 32
            height: 32
            radius: theme.radiusSm
            anchors.verticalCenter: parent.verticalCenter
            color: moreArea.containsMouse ? theme.surfaceElevated : "transparent"
            QbzIcon {
                anchors.centerIn: parent
                name: "ellipsis"
                width: 16
                height: 16
                // TrackRow.slint:750 — Theme.text-primary on hover.
                tintName: moreArea.containsMouse ? root.playGlyphTint : "muted"
            }
            MouseArea {
                id: moreArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: function (mouse) { rowMenu.openAtCursor(moreArea, mouse.x, mouse.y) }
            }
        }
    }
    CardMenu {
        id: rowMenu
        menuWidth: 224
        entries: root.menuModel()
        onPicked: function (a) { root.menuAction(a) }
    }

    // TrackContextMenu.slint, in its order. Every row here reaches a live
    // seam; the seamless ones are gated off above.
    function menuModel() {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        var m = [
            { "label": t("Play now", r), "icon": "play-fill", "action": "play" },
            { "label": t("Play next", r), "icon": "list-start", "action": "next" },
        ]
        // #442 "Play later" — the end of the MANUAL block (after everything
        // already queued by hand, before the source resumes).
        if (root.menuShowLater)
            m.push({ "label": t("Play later", r), "icon": "list-plus", "action": "later" })
        m.push({ "label": t("Add to queue", r), "icon": "list-end", "action": "queue" })
        if (root.hasRadioSeam) {
            m.push({ "label": t("Create QBZ radio", r), "icon": "radio", "action": "radio-qbz" })
            m.push({ "label": t("Create Qobuz radio", r), "icon": "radio", "action": "radio-qobuz" })
        }
        if (root.menuShowFavorite)
            m.push({ "label": root.item.isFavorite ? t("Remove from Library", r) : t("Add to Library", r),
                     "icon": root.item.isFavorite ? "heart-filled" : "heart", "action": "favorite" })
        if (root.hasMixtapeSeam)
            m.push({ "label": t("Add to mixtape", r), "icon": "cassette-tape", "action": "mixtape" })
        if (root.hasPlaylistAddSeam)
            m.push({ "label": t("Add to playlist", r), "icon": "list-music", "action": "add-to-playlist" })
        if (root.menuShowRemove)
            m.push({ "label": t("Remove from playlist", r), "icon": "trash-2",
                     "action": "remove", "danger": true })
        if (root.menuShowShare)
            m.push({ "label": t("Share Qobuz link", r), "icon": "link", "action": "share-qobuz" })
        if (root.hasSonglinkSeam)
            m.push({ "label": t("Share Song.link", r), "icon": "link", "action": "share-songlink" })
        if (root.hasOfflineCacheSeam)
            m.push({ "label": t("Make available offline", r), "icon": "cloud-download", "action": "cache" })
        if (root.menuShowGoTo && root.item.albumId)
            m.push({ "label": t("Go to album", r), "icon": "disc-3", "action": "go-album" })
        if (root.menuShowGoTo && root.item.artistId)
            m.push({ "label": t("Go to artist", r), "icon": "user", "action": "go-artist" })
        if (root.menuShowTrackInfo)
            m.push({ "label": t("Track info", r), "icon": "info", "action": "track-info" })
        return m
    }

    function menuAction(a) {
        if (a === "play") root.playRequested()
        else if (a === "next") root.enqueueRequested("next")
        else if (a === "later") root.enqueueRequested("later")
        else if (a === "queue") root.enqueueRequested("queue")
        else if (a === "go-artist") QbzArtist.openArtist(root.item.artistId)
        else if (a === "go-album") QbzAlbum.openAlbum(root.item.albumId)
        else if (a === "favorite") {
            root.item.isFavorite = !root.item.isFavorite
            QbzLibrary.libraryToggleFavorite("track", root.item.id)
        } else if (a === "remove") root.removeRequested()
        else if (a === "share-qobuz") root.copyToClipboard(
            "https://open.qobuz.com/track/" + (root.item.id || ""))
        else if (a === "track-info") root.openTrackInfo()
    }

    // --- Share (share.rs::qobuz_track_url + copy_to_clipboard) -----------
    // The .slint arm copies the link and raises a toast; the Qt port has no
    // toast seam yet (GLUE NEEDED), so the copy is silent — but it IS a real
    // clipboard write. QtQuick exposes no Clipboard type; TextEdit.copy() is
    // the supported route, kept in an INACTIVE Loader so a 16K-row list does
    // not carry one TextEdit per row.
    Loader {
        id: clipLoader
        active: false
        sourceComponent: TextEdit { visible: false }
    }
    function copyToClipboard(text) {
        if (!text || text === "")
            return
        clipLoader.active = true
        clipLoader.item.text = text
        clipLoader.item.selectAll()
        clipLoader.item.copy()
        clipLoader.active = false
    }

    // --- Track info (QbzAlbum.openTrackInfo + shell/TrackInfoModal) ------
    // Same lazy-Loader reason: the modal is a full Popup tree and a list row
    // must not instantiate one per delegate. Activated on demand, torn down
    // on close (Qt.callLater so the Popup is not destroyed mid-signal).
    Loader {
        id: trackInfoLoader
        active: false
        sourceComponent: TrackInfoModal { }
    }
    Connections {
        target: trackInfoLoader.item
        ignoreUnknownSignals: true
        function onClosed() { Qt.callLater(function () { trackInfoLoader.active = false }) }
    }
    function openTrackInfo() {
        if (!root.item.id)
            return
        trackInfoLoader.active = true
        trackInfoLoader.item.openFor(root.item.id)
    }

    // Shared drag (the row BODY is the source — TrackRow.slint): press-drag
    // >6px starts it (bodyDragStarted fires FIRST, the #589 pre-hook);
    // release plays (clickPlays) or ignores (album view: double-click).
    property bool dragging: false
    property point downPos: Qt.point(0, 0)
    MouseArea {
        id: trArea
        anchors.fill: parent
        // BEHIND the row's own controls. Declaration order is z-order in QML,
        // and this fills the whole row from the LAST declaration — so it sat on
        // top of the heart / cloud / menu buttons and swallowed their presses.
        // propagateComposedEvents does not save them: `pressed` is not a
        // composed event, so only the row's own double-click still worked.
        z: -1
        hoverEnabled: true
        propagateComposedEvents: true
        // Right press opens the SAME menu as ⋯ (rowMenu), at the pointer.
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: root.clickPlays ? Qt.PointingHandCursor : Qt.ArrowCursor
        onPressed: function (mouse) {
            if (mouse.button === Qt.RightButton) {
                if (root.showMenu)
                    rowMenu.openAtCursor(trArea, mouse.x, mouse.y)
                mouse.accepted = true
                return
            }
            root.downPos = Qt.point(mouse.x, mouse.y)
        }
        onPositionChanged: function (mouse) {
            // Only a LEFT press drags — a right press is the context gesture.
            if (!pressed || !(pressedButtons & Qt.LeftButton) || !root.draggable) return
            const g = mapToItem(null, mouse.x, mouse.y)
            if (!root.dragging
                && (Math.abs(mouse.x - root.downPos.x) > 6
                    || Math.abs(mouse.y - root.downPos.y) > 6)) {
                root.dragging = true
                root.bodyDragStarted(root.number)
                QbzShell.dragStart(root.item.id, root.item.title || "",
                    (root.item.artist || "") + " · " + (root.item.album || ""), g.x, g.y)
            }
            if (root.dragging) QbzShell.dragMove(g.x, g.y)
        }
        onReleased: function (mouse) {
            if (mouse.button === Qt.RightButton)
                return
            if (root.dragging) {
                QbzShell.dragEnd()
                root.dragging = false
                mouse.accepted = true
            } else if (root.clickPlays) {
                root.playRequested()
            } else {
                mouse.accepted = false
            }
        }
        onDoubleClicked: function (mouse) {
            if (mouse.button === Qt.LeftButton)
                root.playRequested()
        }
    }
}
