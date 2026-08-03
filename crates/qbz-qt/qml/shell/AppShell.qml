// App shell — the QML port of crates/qbz-ui/ui/shell/AppShell.slint's
// chrome: HeaderBar (top, 42px) / { Sidebar | content frame | queue
// column } / NowPlayingBarSmall (bottom).
//
// The window chrome is surface-card throughout; the content area is a
// ROUNDED surface-main panel inset 8px left/right/bottom (0 top — it
// butts the header), the "Slack-style bezel on all four corners" of
// AppShell.slint:358-390. The recovery affordance and logout live in the
// header (offline badge flyout + app menu), like the Slint shell.
//
// POC-NOTE: the artwork-derived ambient background (AppearanceState
// "Ambient"/"Blurred art" modes) is not implemented — the Slint default
// is Off, which is what this static dark treatment matches.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../immersive"
import "../theme"

Rectangle {
    id: root
    // OPAQUE base (the Slint root background is opaque surface-main too —
    // invisible while the ambient field mounts over it; this IS the D4
    // no-track fallback). The translucent chrome comes from each chrome
    // piece's OWN background going surface-card @ 0.5 above the field.
    color: theme.surfaceCard

    // App-wide dynamic background (phase 14): active when the mode pref is
    // on AND a track is loaded (D4: no track -> opaque theme restored).
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
    // The bottom-most visual layer (AppShell.slint:206-242, declared FIRST
    // so every chrome surface paints above it): the ambient album-colored
    // field + the dark dim scrim that keeps chrome text legible
    // (QBZ_BG_DIM-tunable, default 0.35).
    // Window-level history navigation: mouse side buttons + two-finger
    // horizontal touchpad swipe (#653 parity). Declared FIRST so the whole UI
    // hit-tests on top of it — a scroll reaches it only when nothing above
    // consumed it, which is what keeps horizontal carousels scrolling instead
    // of navigating. Never consumes a wheel event; accepts only the two side
    // buttons.
    NavGestureLayer {
        anchors.fill: parent
    }

    AmbientField {
        anchors.fill: parent
        visible: root.ambientOn
        running: root.ambientOn
    }
    Rectangle {
        anchors.fill: parent
        visible: root.ambientOn
        color: "#000000"
        opacity: QbzShell.ambientDim
    }

    // The host ApplicationWindow (custom chrome: drag / maximize / resize).
    property var hostWindow: null

    QbzTheme { id: theme }

    HeaderBar {
        id: header
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: theme.headerHeight
        hostWindow: root.hostWindow
        // Square corners (phase 12: the window is opaque; any rounding is
        // the compositor's business).
    }

    NowPlayingBar {
        id: npb
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        // Mode-aware height (AppShell.slint:396): Small collapses to one
        // header-tall row; New/Classic/Large keep the full 112px.
        height: QbzShell.npbMode === 2 ? theme.npbSmallHeight : theme.npbLargeHeight
        // The shared hover-tooltip overlay (declared further down — id
        // references resolve at completion). Only the FULL bar consumes it
        // (the Qobuz Connect button's "Qobuz Connect: On/Off" bubble); the
        // small bar's button has no tooltip in the reference.
        tooltip: tooltipOverlay
    }

    Sidebar {
        id: sidebar
        anchors.left: parent.left
        anchors.top: header.bottom
        anchors.bottom: npb.top
        // The shared hover-tooltip overlay (declared last, below). The sidebar
        // clips its own overflow, so the collapsed rail's name bubble HAS to be
        // rendered out here — same reason Slint mounts SidebarTooltip at the
        // AppShell level rather than inside Sidebar.slint.
        tooltip: tooltipOverlay
        // Animated 3-state width lives inside the component.
    }

    // Right-side panel column — Queue and/or Lyrics, stacked vertically in
    // a shared 300px column (Feishin-style, AppShell.slint:684-707). Each
    // is toggled from its bar button and closed from its own X; the column
    // is visible when either is open, animated 0 <-> 300 (160ms).
    Rectangle {
        id: queueColumn
        anchors.right: parent.right
        anchors.top: header.bottom
        anchors.bottom: npb.top
        width: (QbzShell.queueOpen || QbzShell.lyricsOpen) ? theme.queuePanelWidth : 0
        clip: true
        color: root.ambientOn ? theme.surfaceCardA50 : theme.surfaceCard

        Behavior on width {
            NumberAnimation { duration: 160; easing.type: Easing.InOutQuad }
        }

        QueuePanel {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: QbzShell.lyricsOpen
                ? (QbzShell.queueOpen ? parent.height / 2 : 0)
                : parent.height
            visible: QbzShell.queueOpen
        }
        Rectangle {
            visible: QbzShell.queueOpen && QbzShell.lyricsOpen
            anchors.left: parent.left
            anchors.right: parent.right
            y: QbzShell.lyricsOpen && QbzShell.queueOpen ? parent.height / 2 : 0
            height: 1
            color: theme.borderSubtle
        }
        LyricsPanel {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: QbzShell.lyricsOpen
                ? (QbzShell.queueOpen ? parent.height / 2 - 1 : parent.height)
                : 0
            visible: QbzShell.lyricsOpen
        }
    }

    // Content frame — the rounded, inset surface-main panel (Radius.md,
    // 8px gaps left/right/bottom, flush to the header).
    Rectangle {
        id: contentFrame
        anchors.left: sidebar.right
        anchors.right: queueColumn.left
        anchors.top: header.bottom
        anchors.bottom: npb.top
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        anchors.bottomMargin: 8
        radius: theme.radiusMd
        // Frosted content panel while the ambient background is active
        // (AppShell.slint: surface-main @ 0.22 + 1px #ffffff@0.10 hairline).
        color: root.ambientOn ? theme.surfaceMainA22 : theme.surfaceMain
        border.width: root.ambientOn ? 1 : 0
        border.color: theme.frostBorder
        clip: true

        Loader {
            id: viewLoader
            anchors.fill: parent
            source: QbzShell.currentView === "home" ? "../views/HomeView.qml"
                : QbzShell.currentView === "library" ? "../views/LibraryView.qml"
                : QbzShell.currentView === "local" ? "../views/LocalLibraryView.qml"
                : QbzShell.currentView === "localalbum" ? "../views/LocalAlbumView.qml"
                : QbzShell.currentView === "album" ? "../views/AlbumView.qml"
                : QbzShell.currentView === "artist" ? "../views/ArtistView.qml"
                : QbzShell.currentView === "settings" ? "../settings/SettingsView.qml"
                : QbzShell.currentView === "search" ? "../views/SearchView.qml"
                : QbzShell.currentView === "playlist" ? "../views/PlaylistView.qml"
                : QbzShell.currentView === "discoverbrowse" ? "../views/DiscoverBrowseView.qml"
                : QbzShell.currentView === "playlistbrowse" ? "../views/PlaylistBrowseView.qml"
                : QbzShell.currentView === "recentalbums" ? "../views/PlayHistoryView.qml"
                : QbzShell.currentView === "mostplayedalbums" ? "../views/PlayHistoryView.qml"
                : QbzShell.currentView === "label" ? "../views/LabelView.qml"
                : QbzShell.currentView === "labelreleases" ? "../views/LabelReleasesView.qml"
                // Artist page > a release section > "See discography", and the
                // album page's "From the same artist" View all. Same two-file
                // contract as the rest of this chain (nav_qt.rs:9-16):
                // artist_releases_qt::open records the id, this arm mounts it.
                // Forgetting the arm is NOT a crash — the ternary falls through
                // to "" and the pane goes blank with nothing logged, i.e. the
                // dead "See discography" in a new costume.
                : QbzShell.currentView === "artistreleases" ? "../views/ArtistReleasesView.qml"
                // For You > Qobuz Mixes. The whole chain existed —
                // HomeView's tile -> QbzHome.openMix -> foryou_qt -> nav_qt
                // records the "mix" view — and MixView.qml is written and
                // registered in build.rs, but this arm was missing, so every
                // mix tile navigated to a view nobody mounted and the content
                // pane simply went blank (back recovered, which is why it read
                // as "nothing happens" rather than a crash).
                : QbzShell.currentView === "mix" ? "../views/MixView.qml"
                // MyQBZ. Mixtapes and Collections are ONE file — the two
                // pages differ only in their filter row and their empty
                // state, so MyQbzGridView carries a `kind` discriminator
                // (see the Binding below, which is how it gets set).
                : QbzShell.currentView === "mixtapes"
                  || QbzShell.currentView === "collections" ? "../views/myqbz/MyQbzGridView.qml"
                : QbzShell.currentView === "mixtapedetail" ? "../views/myqbz/MyQbzDetailView.qml"
                : QbzShell.currentView === "discobuilder" ? "../views/myqbz/DiscoBuilderView.qml"
                // Settings > Blacklist > Manage. Not reachable from the
                // sidebar — blacklist_qt::open_manager records the route.
                : QbzShell.currentView === "blacklist" ? "../views/BlacklistManagerView.qml"
                // Sidebar > Playlists ⋯ > Manage playlists. Not a sidebar
                // section — playlist_manager_qt::navigate() records the route.
                // The route is a TWO-FILE contract (nav_qt.rs:9-16): the caller
                // records the id, this arm mounts it, and the failure mode for a
                // missing arm is a BLANK content pane, logged nowhere.
                : QbzShell.currentView === "playlistmanager" ? "../views/PlaylistManagerView.qml" : ""
        }

        // MyQbzGridView serves BOTH the "mixtapes" and "collections" routes
        // and needs to know which. A Loader cannot pass properties through a
        // declarative `source:` ternary, and the view's own default is
        // "mixtape" — so without this the Collections route would render the
        // Mixtapes page, permanently, with nothing logged.
        //
        // A Binding rather than `onLoaded`/`setSource`: it applies the moment
        // `item` exists (the Loader creates it synchronously, so still before
        // the first render pass), it re-applies if the route flips
        // mixtapes <-> collections while the same instance is mounted, and it
        // keeps the whole router declarative. Same shape as the seeded-field
        // pattern at controls/QbzLineEdit.qml:174-179.
        //
        // RestoreNone: the target is DESTROYED on unload, and the default
        // RestoreBindingOrValue would try to write the old value back into a
        // dying object.
        Binding {
            target: viewLoader.item
            property: "kind"
            value: QbzShell.currentView === "collections" ? "collection" : "mixtape"
            when: viewLoader.item !== null
                  && (QbzShell.currentView === "mixtapes"
                      || QbzShell.currentView === "collections")
            restoreMode: Binding.RestoreNone
        }
    }

    // --- Bezel corners ----------------------------------------------------
    // `clip: true` on contentFrame is a RECTANGULAR scissor in Qt Quick — it
    // is a glScissor / QPainter setClipRect and it does NOT follow `radius`.
    // The radius only shapes that Rectangle's own fill, so anything that
    // paints opaquely at the frame's corners paints straight over the bezel
    // and the panel reads square. Slint has no such split: its `clip: true`
    // on a Rectangle clips to the ROUNDED shape (AppShell.slint:409-414 sets
    // border-radius + clip on the same element and the album atmosphere
    // inside it is cut by the arc), which is why the identical structure is
    // correct there and wrong here. The visible offender is the album/artist
    // header band (controls/HeaderGradient.qml, full-bleed at y:0), but the
    // defect is the frame's, not the band's: the band scrolls, so rounding
    // the band itself would only hold at scroll offset 0 — measured, the
    // corners go square again the moment the page scrolls and the band's body
    // reaches the top edge. The mask therefore belongs here, once, and it
    // covers every present and future view.
    //
    // SIBLINGS of contentFrame, not children of it. Inside the frame they sat
    // at z:0 with the view Loader, so any overlay a VIEW mounts painted over
    // them and the corners read square while it was open — the pane-filling
    // modals are exactly that: DiscoverConfigModal z:3100 (HomeView:1130),
    // GenreFilterPopup z:3000 (Home/Library/DiscoverBrowse/PlaylistBrowse),
    // TrackInfoModal z:3000 (rows/TrackRow.qml:565, and TrackRow mounts
    // inside views). Raising the nubs to z:3200 would only win until the next
    // modal, because ADR-009 fixes in-pane modals at z >= 3000 and says
    // nothing about a ceiling. Out here the question does not arise: the mask
    // describes the FRAME's silhouette, so nothing the frame contains can
    // reach it, whatever z it picks. Window-level overlays declared after
    // this block still paint over the nubs — Cortinilla, the shared text
    // modal, the drag ghost, ArtPreviewOverlay — and that is correct: a
    // window-wide scrim covers the pane corners too. SidebarNowPlayingDock is
    // also later but lives inside the 240px sidebar (x:16, w:208) and never
    // touches the frame.
    //
    // Mechanism: four small Canvas nubs, each filling its corner square with
    // the shell colour and punching the quarter-disc back out.
    //
    // DOCTRINE CORRECTION (2026-07-29). This paragraph used to justify the
    // Canvas by claiming "ShaderEffect / OpacityMask render NOTHING on the
    // software path, and `layer.enabled` would cost an FBO every frame".
    // Both halves are FALSE as stated: effects need shaders, and this port
    // runs on the GPU (OpenGL RHI, measured via QSG_INFO 2026-07-29) — the
    // "renders nothing" note came from an offscreen session, which forces the
    // software renderer by definition. A `layer` FBO is also rendered once and
    // CACHED for static content, not re-rendered per frame. Where a software
    // path is genuinely possible, detect it with `GraphicsInfo.api` rather
    // than assuming it (theme/RoundedImage.qml does exactly that and is the
    // canonical statement of this rule — read its "HOW THE ROUNDING IS DONE"
    // header before touching any masking code).
    //
    // The Canvas here is still the right call, but on its own merits, not that
    // one: the nubs are 4 x 12x12 px rastered ONCE and repainted only when the
    // theme colour or the radius changes, whereas masking the frame would mean
    // an FBO the size of the whole content pane, re-rendered on every window
    // resize and invalidated by anything animating inside the pane.
    //
    // The fill is the shell base (`root.color`) because that is literally
    // what shows through the bezel: the 8px gutter, the HeaderBar above
    // and the NPB gap below are all surface-card, and nothing else paints
    // between `root` and the frame while the corners are on.
    //
    // Gated on the ambient background being OFF, and that is not a
    // shortcut: with the dynamic background active the frame goes
    // translucent (surface-main @ 0.22 + hairline) and the ambient field
    // is SUPPOSED to show through the corners, so an opaque nub would be
    // the regression. The same flag also suppresses the album/artist
    // atmosphere (AlbumView.qml:71 `headerAtmoOn = pref && !ambientOn`,
    // AlbumPageView.slint:168) — the two states are complementary, never
    // both.
    //
    // WHAT COVERS THE AMBIENT-ON CASE, since this block cannot: nothing that
    // paints opaquely may reach the frame's corners while the field is
    // showing through them, and by construction almost nothing does — every
    // view root goes `color: "transparent"` under ambient (HomeView.qml:53 and
    // its twelve siblings), and the ones that stay opaque round their own fill
    // (`radius: 12`, the same trick, applied by the offender instead of by a
    // mask). Sweeping the pane-level overlay set (ADR-009's z >= 3000) turned
    // up ONE that did neither: controls/DiscoverConfigModal.qml's scrim, a
    // full-bleed #bf000000 Rectangle at z:3100.
    // It now carries `radius: theme.radiusMd` too. GenreFilterPopup's backdrop
    // is a bare MouseArea (paints nothing) and TrackInfoModal / CastPicker are
    // parented to the window Overlay, where covering the pane corners is the
    // correct behaviour. A future full-bleed opaque pane child must round
    // itself the same way — there is no mask that can do it for it here.
    BezelCorner { corner: 0; fill: root.color; r: contentFrame.radius
                  visible: !root.ambientOn
                  x: contentFrame.x; y: contentFrame.y }
    BezelCorner { corner: 1; fill: root.color; r: contentFrame.radius
                  visible: !root.ambientOn
                  x: contentFrame.x + contentFrame.width - width; y: contentFrame.y }
    BezelCorner { corner: 2; fill: root.color; r: contentFrame.radius
                  visible: !root.ambientOn
                  x: contentFrame.x + contentFrame.width - width
                  y: contentFrame.y + contentFrame.height - height }
    BezelCorner { corner: 3; fill: root.color; r: contentFrame.radius
                  visible: !root.ambientOn
                  x: contentFrame.x
                  y: contentFrame.y + contentFrame.height - height }

    // The bezel nub itself. Kept as an inline component (no outer `id` is
    // referenced inside it — inline components do not share the document's
    // scope, so the colour and the radius arrive as properties).
    //
    // `corner`: 0 = top-left, 1 = top-right, 2 = bottom-right, 3 = bottom-left.
    // The arc centre is the corner of the r x r square that points INTO the
    // panel, so the punched-out quarter-disc is the panel side and the painted
    // remainder is the sliver outside the arc — the exact pixels Qt's
    // rectangular clip fails to cut.
    component BezelCorner: Canvas {
        id: nub
        property int corner: 0
        property color fill: "#000000"
        property int r: 12

        width: nub.r
        height: nub.r
        // Purely decorative: never take a click meant for the view underneath.
        enabled: false
        // Same pair as RoundedImage/PlaylistCollage — CPU raster, and Immediate
        // so a repaint cannot land on a pixmap whose Canvas is already gone.
        renderTarget: Canvas.Image
        renderStrategy: Canvas.Immediate

        onFillChanged: nub.requestPaint()
        onRChanged: nub.requestPaint()
        onCornerChanged: nub.requestPaint()

        onPaint: {
            var ctx = nub.getContext("2d")
            if (!ctx || nub.r <= 0)
                return
            ctx.reset()
            ctx.clearRect(0, 0, nub.width, nub.height)
            ctx.fillStyle = nub.fill
            ctx.fillRect(0, 0, nub.width, nub.height)
            // destination-out = QPainter's CompositionMode_DestinationOut, so
            // the disc erases the fill instead of drawing over it. Its edge is
            // antialiased against transparency, which is what makes the nub
            // blend into the arc rather than staircase along it.
            ctx.globalCompositeOperation = "destination-out"
            ctx.beginPath()
            ctx.arc(nub.corner === 0 || nub.corner === 3 ? nub.r : 0,
                    nub.corner === 0 || nub.corner === 1 ? nub.r : 0,
                    nub.r, 0, 2 * Math.PI)
            ctx.fill()
            ctx.globalCompositeOperation = "source-over"
        }
    }

    // --- Large NPB (mode 3) cover dock (phase 18) -------------------------
    // The L's vertical arm: the square now-playing cover + FFT band, pinned
    // flush to the window bottom-left over the sidebar
    // (SidebarNowPlayingDock.slint, AppShell.slint:747). Only while Large is
    // ACTIVE (mode 3 + sidebar open).
    //
    // The height comes from QbzShell.largeDockHeight (it changes with the
    // band's eye toggle) and Sidebar.qml reserves the same value minus the
    // bar height — pinning here with a literal would desync the two.
    readonly property bool largeActive: QbzShell.npbMode === 3 && QbzShell.sidebarState === 0
    SidebarNowPlayingDock {
        visible: root.largeActive
        // Equal 16px gutters inside the 240px sidebar; the cover is square,
        // so this width also sets the art size.
        x: 16
        width: 208
        y: parent.height - QbzShell.largeDockHeight
        ambientOn: root.ambientOn
    }

    // Search cortinilla (phase 15): the live-search dropdown overlay, LAST
    // child so it renders above every surface (Cortinilla.slint's mount).
    Cortinilla {
        anchors.fill: parent
        headerBar: header
    }

    // --- Shared text modal (phase 16) ---------------------------------------
    // The AppShell-level modal layer (the Slint AppShell modals mount at
    // window level, ADR-009): the scrim covers the WHOLE window — sidebar /
    // header / NPB included — and the panel centers on the WINDOW, not on
    // the content frame. Views reach it via `openTextModal(title, body)`.
    property bool textModalOpen: false
    property string textModalTitle: ""
    property string textModalBody: ""
    function openTextModal(title, body) {
        textModalTitle = title
        textModalBody = body
        textModalOpen = true
    }

    Rectangle {
        visible: root.textModalOpen
        anchors.fill: parent
        color: "#bf000000"
        MouseArea {
            anchors.fill: parent
            onClicked: root.textModalOpen = false
        }
        Rectangle {
            anchors.centerIn: parent
            width: Math.min(root.width - 80, 560)
            height: Math.min(root.height - 120, 460)
            radius: theme.radiusMd
            color: theme.surfaceCard
            border.width: 1
            border.color: theme.borderSubtle
            MouseArea { anchors.fill: parent }
            Column {
                anchors.fill: parent
                anchors.margins: 24
                spacing: 14
                Row {
                    width: parent.width
                    Text {
                        width: parent.width - 28
                        text: root.textModalTitle
                        color: theme.textPrimary
                        font.pixelSize: theme.fontHeading
                        font.weight: theme.weightSemibold
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Rectangle {
                        width: 28
                        height: 28
                        color: tmCloseArea.containsMouse ? theme.surfaceHover : "transparent"
                        radius: 6
                        QbzIcon {
                            name: "x"
                            width: 18
                            height: 18
                            anchors.centerIn: parent
                            tintName: tmCloseArea.containsMouse ? "textPrimary" : "muted"
                        }
                        MouseArea {
                            id: tmCloseArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.textModalOpen = false
                        }
                    }
                }
                Flickable {
                    width: parent.width
                    height: parent.height - 42
                    clip: true
                    contentWidth: width
                    contentHeight: tmText.implicitHeight
                    Text {
                        id: tmText
                        width: parent.width
                        text: root.textModalBody
                        color: theme.textSecondary
                        font.pixelSize: theme.fontBody
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }
    }

    // --- Drag ghost (DragGhost.slint, phase 17) -----------------------------
    // The dark pill that follows the cursor while tracks are dragged onto
    // the sidebar: "N tracks" for a group, or title + "artist · album" for
    // one. Visual only (never blocks the pointer).
    Rectangle {
        visible: QbzShell.dragActive
        x: QbzShell.dragX + 10
        y: QbzShell.dragY + 14
        width: ghostCol.width + 28
        height: ghostCol.height + 16
        radius: 8
        color: "#e01e1e28"
        border.width: 1
        border.color: "#1fffffff"
        Column {
            id: ghostCol
            anchors.centerIn: parent
            spacing: 2
            Text {
                text: QbzShell.dragCount > 1
                    ? QbzShell.dragCount + " " + QbzSession.tr("tracks", QbzSession.trRev)
                    : QbzShell.dragTitle
                color: "#ffffff"
                font.pixelSize: 12
                font.weight: theme.weightMedium
            }
            Text {
                visible: QbzShell.dragCount === 1 && QbzShell.dragSubtitle !== ""
                text: QbzShell.dragSubtitle
                color: "#8cffffff"
                font.pixelSize: 10
            }
        }
    }

    // --- MyQBZ global overlays + the shared toast host ---------------------
    // Three window-level overlays, mounted ONCE here because each is reachable
    // from anywhere and must outlive the view that triggered it:
    //
    //   AddToMixtapeModal — the "Add to Mixtape/Collection" picker, opened from
    //     a TrackRow menu, an album/playlist page, a Local Library bulk bar or
    //     the now-playing bar. The view that opened it is often unmounted by
    //     the time the user picks a target.
    //   MyQbzModals       — Create / Edit (rename · description · delete) / DJ
    //     Mix, three sibling panels in one file (spec 01 §10/§11). Delete in
    //     particular navigates away from the page that raised it.
    //   QbzToast          — the ONE in-app toast host (toast_qt.rs publishes
    //     onto QbzShell.toastJson; the auto-hide timer lives in the QML).
    //
    // NO `z` on any of them, and that is the point. This file's convention is
    // DECLARATION ORDER (see the note at ArtPreviewOverlay below: "LAST child =
    // above every surface"), and the bezel-corner note at :200-264 explains why
    // `z` is the wrong tool out here — ADR-009 pins in-pane modals at z >= 3000
    // with no ceiling, so any number picked here loses to the next one added.
    // Position in the chain is byte-equivalent to Slint, where `Toast { }` sits
    // at AppShell.slint:950: after DragGhost, before TooltipOverlay and
    // ArtPreviewOverlay. Covering the content pane's rounded corners is CORRECT
    // for a window-level overlay — a window-wide scrim covers them too.
    //
    // All three self-gate on their own document, so an unopened overlay is an
    // invisible, non-interactive Item and costs nothing.
    AddToMixtapeModal {
        anchors.fill: parent
    }
    // PlaylistPickerModal — the "Add to playlist" picker, opened from the
    // queue footer (save the queue as a playlist) and the queue row menu.
    // Same reason it lives out here as its Mixtape sibling: the surface that
    // opened it is often gone by the time the user picks a target, and the
    // queue panel in particular is `visible`-gated.
    PlaylistPickerModal {
        anchors.fill: parent
    }
    // PlaylistImportModal — the public-playlist importer (Spotify / Apple
    // Music / Tidal / Deezer), opened from the sidebar `...` menu and from the
    // closed-sidebar flyout. It mounts HERE and not in the sidebar by explicit
    // contract (05 §5.8.5): closing it must not cancel an in-flight import, and
    // neither surface that opens it has a lifetime guarantee across a
    // navigation — the importer's own completion arm navigates to the imported
    // playlist while the modal is still up.
    PlaylistImportModal {
        anchors.fill: parent
    }
    MyQbzModals {
        anchors.fill: parent
    }
    // FolderModals — the "New folder" create panel, the full folder editor and
    // their delete confirm (contract D21). Out here for the same reason as its
    // neighbours: it is opened from the SIDEBAR's "..." menu and row menu as
    // well as from the Playlist Manager view, and the sidebar animates to
    // width 0 with clip: true, so a modal parented into it would disappear
    // with it. Self-gates on QbzFolderEdit.createJson / editJson, so while
    // both are closed it is an invisible, non-interactive Item.
    FolderModals {
        anchors.fill: parent
    }
    // PlaylistEditModal — the ONE playlist editor (rename · description ·
    // offline-only · delete), for both `local:` and Qobuz playlists (contract
    // D20/D21). Out here for the same reason as its neighbours: it is opened
    // from the manager's cards and rows, from the SIDEBAR's row context menu
    // and from the playlist detail header — and its delete arm navigates away
    // from the page that raised it. It replaces the inline Popup that used to
    // live in views/PlaylistView.qml, which could express neither a
    // description nor the offline-only flag. Self-gates on
    // QbzPlaylistEdit.editJson, so while closed it is an invisible,
    // non-interactive Item.
    PlaylistEditModal {
        anchors.fill: parent
    }
    QbzToast {
        anchors.fill: parent
    }

    // Immersive mode root overlay (2026-08-02 immersive-port contract §5.1):
    // plain Item, LAST-child class like its Slint twin (a Popup would grab
    // the pointer and kill typing in the immersive search field — the
    // cortinilla doctrine, Cortinilla.qml:1-5). Self-gates on
    // QbzImmersive.open, so while closed it is an invisible, non-interactive
    // Item. Sits AFTER QbzToast and BEFORE ArtPreviewOverlay/QbzTooltip so
    // those two keep painting above it; covering the content pane's rounded
    // corners is correct for a window-level overlay (:555-563 above).
    ImmersiveView {
        anchors.fill: parent
    }

    // LAST child = above every surface, exactly like ArtPreviewOverlay.slint's
    // mount in AppShell.slint. Non-interactive, so it never steals the hover
    // that is keeping it open (see the file header).
    ArtPreviewOverlay { }

    // The shell's ONE hover-tooltip overlay — the port of Slint's
    // TooltipOverlay/SidebarTooltip mechanism, mounted LAST for the reason
    // TooltipOverlay.slint's header gives: "mounted last in AppShell so no
    // neighbour can cover it". Surfaces do not own a popup; they call
    // showRight()/showAbove()/hide() on this instance (see QbzTooltip.qml).
    //
    // WIRED SO FAR: the collapsed sidebar rail (Sidebar.qml). Everything else
    // Slint tooltips is still un-wired — the list is in the handoff notes; each
    // one is a two-line change in its own file, not a change here.
    QbzTooltip {
        id: tooltipOverlay
        anchors.fill: parent
    }

    // Qobuz Connect diagnostics modal (DeveloperSettings > QOBUZ CONNECT) —
    // mounted LAST, mirroring QconnectDevModal.slint's topmost mount in
    // AppShell.slint. Ordering against the tooltip above is a non-issue: this
    // is a Popup parented to Overlay.overlay (the CastPicker pattern), so it
    // renders in the overlay layer above EVERY plain Item in this tree —
    // the tooltip's "LAST child so no neighbour can cover it" invariant is
    // about Items, and no Item was added after it. Self-gates on
    // QbzQConnect.diagOpen, so while closed it is an invisible,
    // non-interactive Item.
    QconnectDevModal { }
}
