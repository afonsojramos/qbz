// THE shared album card — QML replica of discover/AlbumCard.slint, used
// by BOTH the Home rails and the Library All grid (the Slint mounts the
// same component in both places).
//
// 200x246: 200px artwork (Radius.sm) + placeholder, hover scrim with
// genre/year meta, pin badge (top-right), favorite / play / more overlay
// buttons, award ribbon, source badge (opt-in), then the title/artist
// lines with the icon-only quality badge.
//
// Live wiring: play (art click + overlay play), favorite heart (optimistic
// + signal), pin badge (pinned store), ⋯ context menu — and the SAME menu
// on a right press anywhere on the artwork or the title.
//
// --- Menu inventory vs discover/AlbumCard.slint ------------------------
//   Open album · Play · Play next · Play later · Add to queue ·
//   Add to/Remove from Library (show-favorite) · Block this album
//     (source != local && source != plex)
// (the first entry's NOUN is overridable per host — see `openLabel`; the last
//  two are gated on `catalogAffordances`)
// …plus whatever the host appends through `extraMenuEntries` (My QBZ's
// "Remove from collection" — see that property).
// All but the last are live. "Block this album" is gated OFF by
// `hasBlacklistSeam` below: the Qt bridge exposes the blacklist COUNTERS
// (settings_qt/devtools.rs) but no block-album invokable, and the row used
// to render and do nothing at all.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root

    // --- card data (the ONE contract both hosts fill) --------------------
    property string albumId: ""
    property string title: ""
    property string artist: ""
    property string artistId: ""
    property string genre: ""
    property string year: ""
    property string qualityTier: ""
    property string ribbon: ""
    property string ribbonKind: ""
    // Artwork image source (file://… or "") — the host's cache lookup.
    property string artSource: ""
    // REMOTE cover url, for the pin payload only (the AlbumCard.slint pin
    // TouchArea passes `album.artwork-url`). The pinned store keeps a
    // denormalized display snapshot taken at pin time, so a host that
    // leaves this empty pins a row the Pinned rail can only draw as a
    // placeholder — `artSource` is NOT a substitute, it is a local
    // file:// path that means nothing on another machine or after a
    // cache wipe.
    property string artworkUrl: ""
    property bool isFavorite: false
    property bool isPinned: false
    // The row's SOURCE word — "local" | "offline" | "plex" (the Local Library
    // badge triple, src/local_rows.rs `badge_source`) or "qobuz" (the Library
    // ALL feed, library_qt.rs `FeedItem.source`). "" = a catalog surface that
    // publishes no source at all.
    //
    // Read by TWO consumers, which is why it is NOT the badge's on/off switch:
    // the badge below, and the "Block this album" menu gate
    // (AlbumCard.slint:434 keeps the entry off local/plex rows). Hosts that
    // want the badge hidden set `showSourceBadge`, never `source: ""`.
    property string source: ""
    // discover/AlbumCard.slint:79 `show-source-badge` — default OFF; the two
    // hosts that turn it on are the Library ALL grid (gated on the toolbar's
    // show-local toggle, FavoritesView.slint:1097) and the Local Library
    // grid (LocalLibraryView.slint:1267).
    property bool showSourceBadge: false
    // Local play count — rendered as the muted "{} plays" line under the
    // artist, and ONLY there (discover/AlbumCard.slint:508-513). Every
    // surface but Most Played Albums publishes 0, which is why that page's
    // card is 286px tall and every other one is 266px: this line IS the
    // 20px difference.
    property int plays: 0

    // --- Local Library mode (additive; default = the Qobuz behaviour) ----
    // The Local Library mounts THIS card, but its `albumId` is a group key
    // (folder or metadata identity), not a Qobuz catalog id — routing it
    // through QbzAlbum.openAlbum / QbzPlayer.playAlbum would fire a
    // catalog fetch for a folder path. In localMode every action is emitted
    // to the host instead. Nothing else about the card changes, so the two
    // surfaces stay pixel-identical.
    //
    // ROUTING ONLY, since the My QBZ round (2026-08-01). It used to ALSO hide
    // the heart, the pin badge and "Block this album" — see
    // `catalogAffordances` for why that turned out to be two different
    // questions wearing one flag.
    property bool localMode: false

    // --- Catalog-only affordances: heart, pin badge, "Block this album" -----
    //
    // Split out of `localMode` because a host can need one answer and not the
    // other, and My QBZ is that host. Its grid routes EVERY action through
    // `QbzMyQbz` (only it knows how to open a Plex album, a track or a
    // playlist), so it needs `localMode: true` — but a Qobuz ALBUM cell's
    // `albumId` IS a catalog album id, and its heart / pin / block are the same
    // entity every other AlbumCard in the app talks about. The blanket flag
    // hid the heart on those cells, which is the owner's report ("al overlay le
    // falta el botón de favorite"): a container that is multi-source PER ITEM
    // cannot be described by one grid-wide boolean.
    //
    // Defaults to `!localMode`, so the two pre-existing hosts (Local Library
    // grid, Local album view) keep byte-identical behaviour and no caller has
    // to be updated. A host that wants the split sets it explicitly.
    //
    // The heart still goes through `toggleFavorite()` -> the shared
    // `QbzLibrary.libraryToggleFavorite` seam and still settles on
    // `libraryFavoriteChanged`, so a heart flipped here and one flipped on the
    // album page agree. Do NOT let a host paint its own heart on top of the
    // card — see the `selectMode` note below for the same mistake's other half.
    property bool catalogAffordances: !root.localMode

    // --- Multi-select mode — discover/AlbumCard.slint:83, :179-197, :207,
    // :239, :465. NOT an invention of this port and NOT a host concern: the
    // reference card owns the whole presentation of select mode, and it owns
    // it because three things have to move together —
    //   * the hover action buttons are HIDDEN (:239) — in select mode the
    //     gesture is "pick this card", and a live play button there plays an
    //     album the user was trying to tick;
    //   * the pin badge is HIDDEN (:207), because the checkbox takes its
    //     corner;
    //   * the checkbox is drawn in THAT corner (top-right, :179-197) and the
    //     card click toggles instead of opening (:169, :465).
    // A host that paints a tick on top of the card can only do the third.
    // `views/local/LocalAlbumCollection.qml` used to do exactly that — a third
    // tick geometry, top-left, with the card's play button still live
    // underneath it — and it was moved onto these members on 2026-08-01. Both
    // grid hosts (Local Library, My QBZ) are on the card's select mode now;
    // there is no host-painted tick left in the tree, and a new one is a
    // regression, not a shortcut.
    property bool selectMode: false
    property bool selected: false
    signal selectToggled()

    // Album blacklist ("Block this album") — LIVE since QbzBlacklist landed
    // (`blockAlbum(id, title, artist, coverUrl)`). Kept as a property, not
    // inlined, because a host whose `albumId` is not a Qobuz catalog id must
    // be able to turn it off; `localMode` already covers the two shipped
    // cases. One-way, exactly like the .slint: the card drops on the grid's
    // next reload, there is no live removal.
    property bool hasBlacklistSeam: true

    // --- Host-supplied extras (additive; every default is the old behaviour)
    //
    // 1. `placeholderIcon` — the glyph drawn in the empty artwork well when
    //    `artSource` is "". The Qobuz/Local hosts leave it "" and get the bare
    //    `surface-elevated` square they always had. My QBZ needs it because its
    //    grid is HETEROGENEOUS (album / track / playlist) and the reference
    //    card for that grid draws a type glyph there
    //    (MixtapeDetailView.slint:522-533, 28px, text-muted) — dropping it when
    //    that grid moved onto this card would have been a visual regression.
    property string placeholderIcon: ""
    // 2. `extraMenuEntries` / `extraMenuAction` — a host-owned TAIL on the ONE
    //    menu this card already owns (the ⋯ overlay button AND the right press
    //    open the same `albumMenu`). My QBZ has to offer "Remove from
    //    collection", which is a CONTAINER action no album card can know about,
    //    and the alternative in the tree — `views/local/LocalAlbumCollection`
    //    mounting a SECOND CardMenu for the right press — leaves the ⋯ button
    //    showing a different menu than the right click. Entries use CardMenu's
    //    shape ({ label, icon, action, danger } / { sep: true }); an action that
    //    matches one of them is emitted here instead of being interpreted.
    property var extraMenuEntries: []
    signal extraMenuAction(string action)
    // 3. `openLabel` — the NOUN on the first menu entry. "" (default) keeps the
    //    card's own `tr("Open album")`, which is what every catalog surface
    //    wants. My QBZ's grid is heterogeneous: a PLAYLIST cell routes to the
    //    playlist page (`myqbz_detail_qt::open_item`), so "Open album" on it was
    //    simply the wrong word — the action was never wrong, only its label.
    //    A TRACK cell keeps "Open album" on purpose: `open_item` sends
    //    "album" | "track" to the same album page (1:1 with the reference,
    //    `qbz/src/main.rs:6275-6277`), so the noun is accurate there.
    //    It is a LABEL, not a second action: the entry's `action` stays "open"
    //    and still reaches `openRequested()` / `QbzAlbum.openAlbum`.
    property string openLabel: ""

    signal openRequested()
    signal playRequested()
    signal enqueueRequested(string mode)

    QbzTheme { id: theme }

    width: 200
    // 246 normally; +20 for the "{} plays" line (see `plays`) — the same
    // 266 -> 286 step MostPlayedAlbumsView.slint:23 takes for its grid.
    height: 246 + (root.plays > 0 ? 20 : 0)
    color: "transparent"

    readonly property bool overlayOn: artArea.containsMouse || pinArea.containsMouse
        || favBtn.hovered || playBtn.hovered || moreBtn.hovered

    function toggleFavorite() {
        root.isFavorite = !root.isFavorite
        QbzLibrary.libraryToggleFavorite("album", root.albumId)
    }
    function togglePin() {
        root.isPinned = !root.isPinned
        QbzLibrary.togglePin("album", root.albumId, root.title, root.artist,
            root.artworkUrl)
    }

    // Pin fan-out. The pinned store has no change-notify, so `pinChanged`
    // (key `{kind}:{id}`, emitted by main.rs::toggle_pin after the write) is
    // the ONLY signal that this album's state moved — and it moves from
    // anywhere: another rail, another tab, the album page header. This is the
    // port's equivalent of the Slint `set_album_row_pinned` walk over every
    // live model, and the reason no surface has to republish its document to
    // keep a glyph honest. Assigning breaks the host's `isPinned` binding on
    // purpose (the optimistic flip above already does).
    Connections {
        target: QbzLibrary
        function onPinChanged(key, value) {
            if (root.albumId !== "" && key === "album:" + root.albumId)
                root.isPinned = value
        }
        // Favourite fan-out — the SAME shape, one signal later. `isFavorite`
        // was the odd one out: the optimistic flip above breaks the host's
        // binding (by design), and nothing settled it afterwards, so a write
        // that FAILED stayed visibly wrong until the user navigated away, and
        // a heart flipped on another surface (the album page header, a search
        // result, the queue) never reached this card. `libraryFavoriteChanged`
        // carries the value the write actually produced — the flipped one on
        // success, the UNCHANGED one on failure — so this is both the
        // cross-surface walk and the rollback.
        function onLibraryFavoriteChanged(key, value) {
            if (root.albumId !== "" && key === "album:" + root.albumId)
                root.isFavorite = value
        }
    }

    // AlbumCard.slint's album-menu, in its order. `catalogAffordances: false`
    // drops the catalog-only rows (heart + block); `localMode` routes the five
    // navigation/playback rows to the host's signals instead. The two are
    // independent — see the property notes.
    function menuModel() {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        var m = [
            { "label": root.openLabel !== "" ? root.openLabel : t("Open album", r),
              "icon": "library-big", "action": "open" },
            { "label": t("Play", r), "icon": "play-fill", "action": "play" },
            { "label": t("Play next", r), "icon": "list-start", "action": "next" },
            // #442 "Play later" — end of the manual block.
            { "label": t("Play later", r), "icon": "list-plus", "action": "later" },
            { "label": t("Add to queue", r), "icon": "list-end", "action": "queue" },
        ]
        if (root.catalogAffordances) {
            m.push({ "label": root.isFavorite ? t("Remove from Library", r) : t("Add to Library", r),
                     "icon": root.isFavorite ? "heart-filled" : "heart", "action": "favorite" })
            // .slint gates this on a non-local/plex source as well.
            if (root.hasBlacklistSeam && root.source !== "local" && root.source !== "plex")
                m.push({ "label": t("Block this album", r), "icon": "blind-eye", "action": "block" })
        }
        // Host tail (see `extraMenuEntries`). `concat` so the host's array is
        // never mutated — it is usually a binding's return value.
        var extra = root.extraMenuEntries || []
        return extra.length > 0 ? m.concat(extra) : m
    }

    function menuAction(a) {
        // A host entry is HANDED BACK, never interpreted: the card has no idea
        // what "remove from this collection" means.
        var extra = root.extraMenuEntries || []
        for (var i = 0; i < extra.length; i++) {
            if (extra[i].sep !== true && extra[i].action === a) {
                root.extraMenuAction(a)
                return
            }
        }
        // CATALOG-only rows first, because they are orthogonal to routing: they
        // are in the menu at all only when `catalogAffordances` is on, which
        // means `albumId` IS a catalog album id — so they target the catalog
        // whatever `localMode` says about open/play. Handling them after the
        // `localMode` early-return (the old shape) made them unreachable for
        // My QBZ, i.e. a menu row that rendered and did nothing.
        // `artworkUrl` (the REMOTE url), never `artSource` — the store keeps a
        // denormalized snapshot and a file:// cache path is dead on any other
        // machine, the same reason the pin payload uses it.
        if (a === "favorite") { root.toggleFavorite(); return }
        if (a === "block") {
            QbzBlacklist.blockAlbum(root.albumId, root.title, root.artist,
                root.artworkUrl)
            return
        }
        if (root.localMode) {
            if (a === "open") root.openRequested()
            else if (a === "play") root.playRequested()
            else if (a === "next") root.enqueueRequested("next")
            else if (a === "later") root.enqueueRequested("later")
            else if (a === "queue") root.enqueueRequested("queue")
            return
        }
        if (a === "open") QbzAlbum.openAlbum(root.albumId)
        else if (a === "play") QbzPlayer.playAlbum(root.albumId)
        else if (a === "next") QbzPlayer.enqueueAlbum(root.albumId, "next")
        else if (a === "later") QbzPlayer.enqueueAlbum(root.albumId, "later")
        else if (a === "queue") QbzPlayer.enqueueAlbum(root.albumId, "queue")
    }

    Column {
        spacing: 0

        // --- Artwork + hover overlay -----------------------------------
        Rectangle {
            width: 200
            height: 200
            radius: theme.radiusSm
            color: theme.surfaceElevated
            clip: true

            RoundedImage {
                anchors.fill: parent
                source: root.artSource
                radius: theme.radiusSm
            }

            // Empty-well glyph — opt-in, see `placeholderIcon`. Declared
            // before every hit area so it never takes the pointer (it is an
            // Image with no MouseArea, but z-order is declaration order and
            // the scrim must still paint over it).
            QbzIcon {
                visible: root.placeholderIcon !== "" && root.artSource === ""
                anchors.centerIn: parent
                name: root.placeholderIcon
                width: 28
                height: 28
                tintName: "muted"
            }

            // Hover scrim.
            Rectangle {
                anchors.fill: parent
                radius: theme.radiusSm
                color: "#000000"
                opacity: root.overlayOn ? 0.6 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
            }

            // Hover meta — genre + year, top-left.
            Column {
                x: 12
                y: 12
                spacing: 2
                opacity: root.overlayOn ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                Text {
                    visible: root.genre !== ""
                    text: root.genre
                    height: 20
                    color: "#ebffffff"
                    font.pixelSize: 13
                    font.weight: theme.weightBold
                    verticalAlignment: Text.AlignVCenter
                }
                Text {
                    visible: root.year !== ""
                    text: root.year
                    height: 17
                    color: "#ccffffff"
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                }
            }

            // Card-open + hover detector (declared before the action
            // buttons so those win the pointer).
            MouseArea {
                id: artArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                // Right press opens the SAME menu as the ⋯ button.
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                // Phase 8: the card opens the album view (the overlay play
                // button carries the play affordance).
                onClicked: function (mouse) {
                    if (mouse.button === Qt.RightButton) {
                        albumMenu.openAtCursor(artArea, mouse.x, mouse.y)
                        return
                    }
                    // .slint:169 — in select mode the card click TOGGLES.
                    if (root.selectMode) {
                        root.selectToggled()
                        return
                    }
                    if (root.localMode)
                        root.openRequested()
                    else
                        QbzAlbum.openAlbum(root.albumId)
                }
            }

            // Multi-select checkbox — top-right of the art, select-mode only
            // (.slint:179-197, number for number): 24px disc at
            // `width - 24 - 8, 8`, accent when selected else `#00000099`
            // (Slint #RRGGBBAA -> Qt #AARRGGBB = #99000000), 1.5px border,
            // accent when selected else `#ffffffcc` -> `#ccffffff`, 120ms
            // colour animation, and a 14px white check that only shows when
            // selected. It is the INDICATOR: the card click above is what
            // toggles, so it carries no MouseArea of its own — exactly like
            // the .slint, which gives it no TouchArea either.
            Rectangle {
                visible: root.selectMode
                x: parent.width - width - 8
                y: 8
                width: 24
                height: 24
                radius: 12
                color: root.selected ? theme.accent : "#99000000"
                border.width: 1.5
                border.color: root.selected ? theme.accent : "#ccffffff"
                Behavior on color { ColorAnimation { duration: 120 } }
                QbzIcon {
                    visible: root.selected
                    name: "check"
                    width: 14
                    height: 14
                    anchors.centerIn: parent
                    // .slint:196 hardcodes #ffffff here, and unlike
                    // SelectCheck's 8px glyph this one sits on a 24px disc —
                    // the contrast argument that made SelectCheck diverge does
                    // not reach this size, so the reference stands.
                    tintName: "white"
                }
            }

            // Pin badge — top-right. Hover-revealed like the overlay
            // buttons (AlbumCard.slint: opacity follows overlay-on even
            // when pinned — the pinned state reads in the icon swap only:
            // filled accent pin vs outline). Always-mounted (opacity) so
            // its hover joins overlayOn.
            Rectangle {
                // …and hidden in select mode too (.slint:207 — the checkbox
                // owns this corner). Pinning writes `album:{albumId}` into the
                // pinned store, so it needs a catalog id, not a routing mode.
                visible: root.catalogAffordances && !root.selectMode
                x: parent.width - width - 8
                y: 8
                width: 26
                height: 26
                radius: 13
                color: pinArea.containsMouse ? "#cc000000" : "#99000000"
                opacity: root.overlayOn ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                QbzIcon {
                    name: root.isPinned ? "pin-filled" : "pin"
                    width: 14
                    height: 14
                    anchors.centerIn: parent
                    // On a #99000000/#cc000000 scrim — dark under every theme.
                    tintName: root.isPinned ? "accent" : "white"
                }
                MouseArea {
                    id: pinArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.togglePin()
                }
            }

            // Hover action buttons — favorite / play / more (y=120, h=44,
            // centered, spacing 12).
            CardOverlayRow {
                // .slint:239 `visible: !root.select-mode` — in select mode the
                // click toggles selection, so a live play/⋯ row here would act
                // on a card the user is only trying to tick.
                visible: !root.selectMode
                y: 120
                width: parent.width
                shown: root.overlayOn

                CardOverlayButton {
                    id: favBtn
                    // ABSENT, not present-and-dead, when `albumId` is not a
                    // catalog album id — the same gate as the menu's heart row
                    // and the pin badge.
                    visible: root.catalogAffordances
                    name: root.isFavorite ? "heart-filled" : "heart"
                    active: root.isFavorite
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: root.toggleFavorite()
                }
                CardOverlayButton {
                    id: playBtn
                    name: "play-fill"
                    primary: true
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: root.localMode ? root.playRequested()
                                              : QbzPlayer.playAlbum(root.albumId)
                }
                CardOverlayButton {
                    id: moreBtn
                    name: "ellipsis"
                    anchors.verticalCenter: parent.verticalCenter
                    // CardOverlayButton.clicked() carries no mouse payload,
                    // so fall back to the disc's centre — the menu still
                    // opens under the ⋯ (worst case 18px off the pointer).
                    // Stays correct if the signal ever forwards the event.
                    onClicked: function (mouse) {
                        albumMenu.openAtCursor(moreBtn,
                            mouse ? mouse.x : moreBtn.width / 2,
                            mouse ? mouse.y : moreBtn.height / 2)
                    }
                }
            }

            // Context menu (AlbumCard.slint's album-menu) — the shared
            // CardMenu surface, not a second copy of its delegate.
            CardMenu {
                id: albumMenu
                menuWidth: 196
                entries: root.menuModel()
                onPicked: function (a) { root.menuAction(a) }
            }

            // Award ribbon — content-width, capped at the card width.
            Rectangle {
                visible: root.ribbon !== ""
                x: 0
                y: parent.height - height - 8
                height: 20
                width: Math.min(ribbonRow.width, 200)
                color: root.ribbonKind === "press" ? "#d49511" : "#e0000000"
                topRightRadius: 3
                bottomRightRadius: 3
                clip: true
                Rectangle {
                    width: root.ribbonKind === "press" ? 0 : 3
                    height: parent.height
                    color: root.ribbonKind === "qobuzissime" ? "#8b5cf6" : "#eab308"
                }
                Row {
                    id: ribbonRow
                    height: parent.height
                    leftPadding: 10
                    rightPadding: 10
                    width: ribbonText.implicitWidth + 20
                    Text {
                        id: ribbonText
                        height: parent.height
                        text: root.ribbon
                        color: root.ribbonKind === "press" ? "#1f1407" : "#ffffff"
                        font.pixelSize: 9
                        font.weight: theme.weightSemibold
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
            }

            // Always-visible source badge — bottom-right of the art, ON TOP of
            // the scrim and the hover buttons (declaration order = z-order).
            // 24x24 rounded SQUARE (ADR-008: not a pill), 6px inset.
            // discover/AlbumCard.slint:326-354, number for number.
            //
            // The glyph goes through controls/SourceIcon.qml, never QbzIcon:
            // the Qobuz and Plex marks are MULTI-COLOUR and a tint flattens
            // them to a silhouette. This card used to draw an accent-tinted
            // `hard-drive` for Plex — a blue hard drive where the design calls
            // for the Plex mark.
            Rectangle {
                visible: root.showSourceBadge && root.source !== ""
                x: parent.width - width - 6
                y: parent.height - height - 6
                width: 24
                height: 24
                radius: 4
                // .slint:335-338. Slint literals are #RRGGBBAA, Qt's are
                // #AARRGGBB: #1e1400d9 -> #d91e1400, #000000b3 -> #b3000000,
                // #eab30880 -> #80eab308.
                //   qobuz_purchase -> §8.7 gold chip (dark-amber translucent
                //                     fill + 1px gold border)
                //   qobuz_download -> no chip at all (the wordmark stands
                //                     alone at 22px)
                //   everything else -> the near-black chip.
                color: root.source === "qobuz_purchase" ? "#d91e1400"
                     : (root.source === "offline" ? "transparent" : "#b3000000")
                border.width: root.source === "qobuz_purchase" ? 1 : 0
                border.color: "#80eab308"
                SourceIcon {
                    kind: root.source
                    // .slint:347-349 — one size per arm: plex 16, download 22,
                    // qobuz/purchase 18, local 14. "offline" IS the Qt word for
                    // `qobuz_download` (local_rows.rs `badge_source`).
                    glyphSize: 14
                    plexSize: 16
                    qobuzSize: root.source === "offline" ? 22 : 18
                    // .slint:345-346 — the monochrome glyph is #ffffff here (the
                    // chip is dark under every theme), not the rows' muted.
                    localTint: "white"
                    // The .slint centres with explicit pixel rounding
                    // (:351-352); anchors.centerIn would leave a half pixel on
                    // the odd sizes and blur the mark.
                    x: Math.round((parent.width - width) / 2)
                    y: Math.round((parent.height - height) / 2)
                }
            }
        }
        Item { width: 1; height: 6 }

        // --- Title / artist + quality badge ------------------------------
        Row {
            width: 200
            height: 40 + (root.plays > 0 ? 20 : 0)
            spacing: theme.spacingSm
            Column {
                width: parent.width - (qBadge.visible ? qBadge.width + theme.spacingSm : 0)
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    height: 20
                    text: root.title
                    color: titleArea.containsMouse ? theme.accent : theme.textPrimary
                    font.pixelSize: theme.fontBody - 2
                    font.weight: theme.weightMedium
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: titleArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: function (mouse) {
                            if (mouse.button === Qt.RightButton) {
                                albumMenu.openAtCursor(titleArea, mouse.x, mouse.y)
                                return
                            }
                            // .slint:465 — the title carries the SAME target
                            // as the artwork, select mode included.
                            if (root.selectMode) {
                                root.selectToggled()
                                return
                            }
                            if (root.localMode)
                                root.openRequested()
                            else
                                QbzAlbum.openAlbum(root.albumId)
                        }
                    }
                }
                Text {
                    width: parent.width
                    height: 18
                    text: root.artist
                    color: root.artistId !== "" && artistArea.containsMouse
                        ? theme.textPrimary : theme.textMuted
                    font.pixelSize: theme.fontLink - 1
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: artistArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: root.artistId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: if (root.artistId !== "") QbzArtist.openArtist(root.artistId)
                    }
                }
                // Play count — Most Played Albums only (see `plays`).
                Text {
                    visible: root.plays > 0
                    width: parent.width
                    height: visible ? 18 : 0
                    text: QbzSession.tr("{} plays", QbzSession.trRev).replace("{}", root.plays)
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
            }
            // Icon-only quality badge (QualityBadge.slint).
            QualityMini {
                id: qBadge
                tier: root.qualityTier
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
}
