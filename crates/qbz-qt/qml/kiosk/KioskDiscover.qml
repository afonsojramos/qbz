// KioskDiscover — lightweight Discover/Home for the kiosk (QML port of
// crates/qbz-ui/ui/shell/KioskDiscover.slint, 261 lines).
//
// Reads the SAME QbzHome documents the desktop HomeView reads (no data copy),
// but renders simple horizontal shelves of KioskCards instead of the heavy
// Carousel/AlbumCard stack — lighter for a Pi. v1 covers the album-carousel
// rows (newReleases, qobuzissimes, editorPicks, pressAwards, idealDiscography,
// mostStreamed); the non-album rows (playlists, mixes, slim rails) are skipped,
// exactly as KioskDiscover.slint:6-8 declares.
//
// Keyboard/gamepad nav: SHELF mode. The nav index space is the 3 tab pills
// (indices 0-2) followed by one index per SECTION DESCRIPTOR — including the
// ones that render nothing (columns == 1, so Up/Down walk shelves). Left/Right
// cannot move the global index (that would jump shelves), so the shell pulses
// `hmove` and the focused shelf consumes it into its LOCAL card cursor. Rows
// that render nothing are skipped by nudging the index one more step in the
// last travel direction (QbzKioskNav.dir), which is why `count` deliberately
// counts every descriptor (KioskDiscover.slint:10-17, :59-83).
//
// THE SKIP PREDICATE IS NOT THE SLINT ONE. Slint tests
// `desc.section.albums.length == 0`, which works only because a non-carousel
// descriptor is built with an EMPTY DiscoverSection (qbz/src/home.rs:856,
// qbz/src/discover_prefs.rs:203-209). The Qt document carries `items` for
// EVERY kind (src/home_qt.rs:127-143), so the equivalent test is
// `kind === "album"` — see isShelf() below.
//
// Slint's `viewport-y` / `viewport-x` are negative as you scroll; Qt's
// contentY/contentX are their positive twins — every occurrence below is
// already translated, and both are WRITTEN from a change handler on a settled
// layout, never read by a binding that feeds layout (the "Recursion detected"
// panic class).

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    // KioskDiscover.slint:53 sets no background; the kiosk content panel
    // paints surface-main behind it (shell/KioskShell.qml:415).
    color: "transparent"

    QbzTheme { id: theme }

    function trs(s) { return QbzSession.tr(s, QbzSession.trRev) }

    // DiscoverState.active-tab (state.slint:338) is a Slint GLOBAL written by
    // HomeActions.select-tab; Qt has neither, and the desktop equivalent is a
    // view-local property (views/HomeView.qml:179). The kiosk has THREE tabs —
    // HomeView's 4th (recommendations) is not part of this view.
    property string activeTab: "home"

    // A raw JSON.parse in a binding throws on the pre-publish frame and takes
    // the view down; the tree's guarded idiom is views/myqbz/MyQbzGridView.qml:94-103.
    function parseSections(doc) {
        try {
            return JSON.parse(doc)
        } catch (e) {
            return []
        }
    }
    readonly property var homeSections: root.parseSections(QbzHome.homeSectionsJson)
    readonly property var editorSections: root.parseSections(QbzHome.editorSectionsJson)
    readonly property var forYouSections: root.parseSections(QbzHome.forYouSectionsJson)

    // KioskDiscover.slint:146-150.
    readonly property var activeSections: root.activeTab === "editorPicks"
        ? root.editorSections
        : root.activeTab === "forYou"
            ? root.forYouSections
            : root.homeSections

    /// Does this descriptor render a shelf? Two terms, and the For You gate.
    ///
    /// `kind === "album"` is the Qt translation of the Slint empty-section test
    /// (see the header). The For You term reproduces a property of the
    /// reference that the Qt documents would otherwise break: Slint's
    /// foryou_descriptors (qbz/src/discover_prefs.rs:218-225) maps EVERY
    /// enabled id through bare_descriptor, so every For You descriptor carries
    /// an empty section and the Slint kiosk's For You body is BLANK — the pills
    /// stay focusable, the page below them is empty. The Qt For You document
    /// does carry kind:"album" sections with items (src/home_qt.rs:512, :522,
    /// :545, :555), so without this term the port would show content where the
    /// reference shows none.
    function isShelf(sec) {
        return root.activeTab !== "forYou"
            && sec !== undefined && sec !== null
            && sec.kind === "album"
            && (sec.items || []).length > 0
    }

    // ---- Nav geometry publish (KioskDiscover.slint:59-83): 3 tab pills + one
    // index per descriptor. `count` counts EVERY descriptor, including the ones
    // that render nothing — that is precisely what the skip cascade exists for.
    // The index clamp lives inside publishNav (src/kiosk_nav_qt.rs:266-274).
    readonly property int navSectionCount: root.activeSections.length
    function publishNav() {
        QbzKioskNav.publishNav(3, 1, 3 + root.navSectionCount, true)
    }
    Component.onCompleted: root.publishNav()
    onNavSectionCountChanged: root.publishNav()
    onActiveTabChanged: root.publishNav()

    // Enter on a tab pill → the same tab switch the pills' taps drive
    // (KioskDiscover.slint:85-98). The `index < tabs` test is what keeps this
    // watcher and the shelves' disjoint: both see the same pulse.
    Connections {
        target: QbzKioskNav

        function onActivateSeqChanged() {
            if (!QbzKioskNav.navActive || QbzKioskNav.zone !== "content"
                || QbzKioskNav.index >= QbzKioskNav.tabs)
                return
            if (QbzKioskNav.index === 0)
                root.activeTab = "home"
            else if (QbzKioskNav.index === 1)
                root.activeTab = "editorPicks"
            else if (QbzKioskNav.index === 2)
                root.activeTab = "forYou"
        }
    }

    // KioskDiscover.slint:23-51. Height 40, Radius.sm, 16px side padding, the
    // label 15px/600, and a 2px accent ring when the shell focus lands here.
    component TabPill: Rectangle {
        id: pill

        property string label: ""
        property bool active: false
        property bool navFocused: false
        signal picked()

        // The pre-publish frame carries no alpha ramp, where alphaTier()
        // silently returns "transparent" (theme/QbzTheme.qml:97-99); the
        // literal is the Slint dark value of Theme.alpha-6 and its
        // light-polarity mirror (idiom + reason: rows/TrackRow.qml:264-269).
        readonly property color hoverFill: theme.alphaTiers.length > 0
            ? theme.alphaTier(6) : (theme.isDark ? "#0fffffff" : "#0f000000")

        width: pillLabel.implicitWidth + 32
        height: 40
        radius: theme.radiusSm
        color: pill.active ? theme.accent
            : (pillArea.containsMouse ? pill.hoverFill : "transparent")
        border.width: pill.navFocused ? 2 : 0
        border.color: theme.accent

        Text {
            id: pillLabel
            anchors.centerIn: pill
            text: pill.label
            // KioskDiscover.slint:40 is Theme.accent-text on the accent fill;
            // on an accent FILL this port takes the owner-approved glyph
            // selector instead (theme/QbzTheme.qml:111-203, :314-317), because
            // white/accent-text fails 3:1 on 16 of 35 palettes.
            color: pill.active ? theme.accentGlyphColor : theme.textSecondary
            font.pixelSize: 15
            font.weight: theme.weightSemibold
            verticalAlignment: Text.AlignVCenter
        }

        MouseArea {
            id: pillArea
            anchors.fill: pill
            hoverEnabled: true
            onClicked: pill.picked()
        }
    }

    Flickable {
        id: pageFlick

        anchors.fill: root
        contentWidth: pageFlick.width
        contentHeight: page.height + 32     // the column's 16px top+bottom pad
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: page

            x: 16
            y: 16
            width: pageFlick.width - 32
            spacing: 22

            // Tab pills — Home / Editor's Picks / For You
            // (KioskDiscover.slint:109-142). The container hugs the pill row.
            Rectangle {
                width: pills.width + 10     // the row's 5px padding, both sides
                height: 50
                radius: theme.radiusMd
                color: theme.surfaceCard
                border.width: 1
                border.color: theme.borderSubtle

                Row {
                    id: pills
                    x: 5
                    y: 5
                    spacing: 4

                    TabPill {
                        label: root.trs("Home")
                        active: root.activeTab === "home"
                        navFocused: QbzKioskNav.navActive
                            && QbzKioskNav.zone === "content"
                            && QbzKioskNav.index === 0
                        onPicked: root.activeTab = "home"
                    }
                    TabPill {
                        label: root.trs("Editor's Picks")
                        active: root.activeTab === "editorPicks"
                        navFocused: QbzKioskNav.navActive
                            && QbzKioskNav.zone === "content"
                            && QbzKioskNav.index === 1
                        onPicked: root.activeTab = "editorPicks"
                    }
                    TabPill {
                        label: root.trs("For You")
                        active: root.activeTab === "forYou"
                        navFocused: QbzKioskNav.navActive
                            && QbzKioskNav.zone === "content"
                            && QbzKioskNav.index === 2
                        onPicked: root.activeTab = "forYou"
                    }
                }
            }

            // One block per descriptor of the active tab. The Repeater walks
            // the RAW array — a descriptor that renders nothing still owns its
            // nav index and still carries the skip handler below, which is how
            // the cascade walks contiguous gaps (KioskDiscover.slint:146-258).
            Repeater {
                model: root.activeSections

                delegate: Item {
                    id: shelf

                    required property var modelData
                    required property int index

                    readonly property bool isShelf: root.isShelf(shelf.modelData)
                    readonly property var items: shelf.isShelf ? (shelf.modelData.items || []) : []
                    // The focused card INSIDE this shelf.
                    property int cursor: 0
                    readonly property bool shelfFocused: QbzKioskNav.navActive
                        && QbzKioskNav.zone === "content"
                        && QbzKioskNav.index - QbzKioskNav.tabs === shelf.index

                    width: page.width
                    // 24px title + 12 + 182px strip. A descriptor that renders
                    // nothing collapses to 0, and the column's 22px spacing
                    // still separates it — the Slint VerticalLayout does the
                    // same with its empty loop item.
                    height: shelf.isShelf ? 218 : 0

                    /// Skip over descriptors that render nothing: take one more
                    /// step in the last travel direction. The step re-fires
                    /// every shelf's handler, so the cascade walks contiguous
                    /// gaps and converges (KioskDiscover.slint:158-170).
                    ///
                    /// The Slint original assigns `KioskNavState.index`
                    /// directly. Here the step goes through navDown()/navUp()
                    /// instead, because `QbzKioskNav.index` is a one-way MIRROR
                    /// of the Rust model (src/kiosk_nav_bridge.rs:109-110,
                    /// :148-163 sync()): a QML write moves the ring for one
                    /// frame and the next arrow key resyncs it back off the
                    /// untouched model — i.e. one dead keypress per skipped
                    /// row. The movement functions keep the model authoritative,
                    /// and for a shelf view (columns 1) they take exactly the
                    /// same +1 / -1 step inside the item range
                    /// (src/kiosk_nav_qt.rs:184-214, :153-181). At the two ends
                    /// the movement functions would LEAVE the content zone
                    /// (navDown() to the player bar, navUp() to the tab strip),
                    /// where Slint's `Math.max(tabs, Math.min(count - 1, ...))`
                    /// parks on the empty edge descriptor with no ring visible
                    /// — spec S2 §7.3 pins that termination as source behavior
                    /// the port must preserve. Hence the edge guards below:
                    /// step only while the step stays inside [tabs, count-1],
                    /// park otherwise.
                    ///
                    /// Deferred, not immediate: the step is taken from inside
                    /// the model's own change notification, and re-entering the
                    /// bridge there would run a nested sync() inside the
                    /// outer one (:148-163 reads a clone of the model up front).
                    function skipIfEmpty() {
                        if (QbzKioskNav.zone !== "content" || !QbzKioskNav.navActive
                            || QbzKioskNav.index - QbzKioskNav.tabs !== shelf.index
                            || shelf.isShelf || QbzKioskNav.dir === 0)
                            return
                        if (QbzKioskNav.dir > 0) {
                            if (QbzKioskNav.index < QbzKioskNav.count - 1)
                                QbzKioskNav.navDown()
                        } else if (QbzKioskNav.index > QbzKioskNav.tabs) {
                            QbzKioskNav.navUp()
                        }
                    }

                    /// Keep the focused shelf's strip inside the page viewport.
                    /// Page coords are FIXED ARITHMETIC: 16px pad + 50px tab
                    /// row + 22px spacing = 88, then a 240px pitch per block
                    /// (24 title + 12 + 182 strip + 22 spacing), block height
                    /// 218 (KioskDiscover.slint:193-208). The literals are the
                    /// source's: they assume every descriptor renders, so a
                    /// preceding empty one shifts the real geometry — that
                    /// approximation is part of the reference and is reproduced
                    /// rather than corrected.
                    function scrollShelfIntoView() {
                        if (!shelf.shelfFocused || shelf.items.length === 0)
                            return
                        var top = 88 + shelf.index * 240
                        if (top < pageFlick.contentY)
                            pageFlick.contentY = top - 8
                        else if (top + 218 > pageFlick.contentY + pageFlick.height)
                            pageFlick.contentY = top + 218 - pageFlick.height + 8
                    }

                    /// Keep the cursor card inside the strip's horizontal
                    /// viewport; card pitch = 128px art + 14px spacing
                    /// (KioskDiscover.slint:232-243). At cursor 0 after a
                    /// scroll the first arm yields contentX = -8, an 8px left
                    /// gutter — the source's behaviour, kept (StopAtBounds is
                    /// what stops Qt animating it away).
                    function scrollCursorIntoView() {
                        if (!shelf.shelfFocused)
                            return
                        var left = shelf.cursor * 142
                        if (left < stripFlick.contentX)
                            stripFlick.contentX = left - 8
                        else if (left + 128 > stripFlick.contentX + stripFlick.width)
                            stripFlick.contentX = left + 128 - stripFlick.width + 8
                    }

                    onCursorChanged: Qt.callLater(shelf.scrollCursorIntoView)

                    Connections {
                        target: QbzKioskNav

                        function onIndexChanged() {
                            Qt.callLater(shelf.skipIfEmpty)
                            Qt.callLater(shelf.scrollShelfIntoView)
                        }

                        // Left/Right shelf move: the shell pulses `hmove` and
                        // the focused shelf consumes it into its cursor.
                        // takeHmove() reads and resets in one step, so a pulse
                        // can never be applied twice (src/kiosk_nav_qt.rs:279-281);
                        // an unfocused or empty shelf must NOT consume it
                        // (KioskDiscover.slint:172-182).
                        function onHmoveChanged() {
                            if (!shelf.shelfFocused || QbzKioskNav.hmove === 0
                                || shelf.items.length === 0)
                                return
                            var pulse = QbzKioskNav.takeHmove()
                            shelf.cursor = Math.max(0,
                                Math.min(shelf.cursor + pulse, shelf.items.length - 1))
                        }

                        // Enter pulse → open the cursor album
                        // (KioskDiscover.slint:184-191).
                        function onActivateSeqChanged() {
                            if (shelf.shelfFocused && shelf.items.length > 0
                                && shelf.cursor < shelf.items.length)
                                QbzAlbum.openAlbum(shelf.items[shelf.cursor].id)
                        }
                    }

                    Column {
                        id: block

                        visible: shelf.isShelf
                        width: shelf.width
                        spacing: 12

                        // Fixed-height title wrapper (KioskDiscover.slint:213-225):
                        // it is what makes the block a constant 218px, so the
                        // arithmetic above stays exact.
                        //
                        // The label is CENTRED, both axes. Not a choice: the
                        // Slint Text sits in a plain Rectangle, and a non-layout
                        // parent centres a child that does not fill it
                        // (i-slint-compiler passes/default_geometry.rs,
                        // maybe_center_in_parent) — a bare Text in the
                        // VerticalLayout, which the comment at :213 says this
                        // replaced, would have been left-aligned.
                        Item {
                            id: titleWrap
                            width: block.width
                            height: 24

                            Text {
                                id: titleText
                                anchors.centerIn: titleWrap
                                text: shelf.modelData.title || ""
                                color: theme.textPrimary
                                font.pixelSize: 18
                                font.weight: theme.weightSemibold
                                verticalAlignment: Text.AlignVCenter
                            }
                        }

                        Flickable {
                            id: stripFlick

                            width: block.width
                            height: 182
                            contentWidth: strip.width
                            contentHeight: stripFlick.height
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds

                            Row {
                                id: strip
                                spacing: 14

                                Repeater {
                                    model: shelf.items

                                    delegate: KioskCard {
                                        id: card

                                        required property var modelData
                                        required property int index

                                        // artSize stays the primitive's 128
                                        // default, which is what makes the
                                        // 142px pitch above right.
                                        //
                                        // KioskCard reads a RESOLVED cover off
                                        // `album.artwork`; the home documents
                                        // carry the cache path as `artPath`
                                        // (src/home_qt.rs:119-120), the same
                                        // field views/HomeView.qml:385 hands
                                        // its cards.
                                        album: ({
                                            id: card.modelData.id,
                                            title: card.modelData.title,
                                            artist: card.modelData.artist,
                                            artwork: card.modelData.artPath || "",
                                            qualityTier: card.modelData.qualityTier
                                        })
                                        navFocused: shelf.shelfFocused && card.index === shelf.cursor
                                        onClicked: function (id) {
                                            QbzAlbum.openAlbum(id)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
