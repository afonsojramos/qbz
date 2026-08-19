// Content router — the ONE view-mount chain, shared by BOTH shells
// (2026-08-02 kiosk-port contract §4.2, divergence D3).
//
// The Slint kiosk's own content block is a VERBATIM copy of AppShell's, and
// KioskShell.slint:10-17 declares that copy to be temporary debt whose named
// follow-up is exactly this extraction. So the chain lives here once:
// AppShell.qml mounts it with `kiosk: false`, KioskShell.qml with
// `kiosk: true`, and the route -> file mapping is stated in a single place.
//
// This file MUST stay in qml/shell/: every `source` below is a relative path
// resolved against this document's directory (`../views/…`, `../settings/…`,
// `../kiosk/…`). Moving it silently breaks all of them.
//
// MOUNT PLUMBING ONLY (D3 guard). No view component is modified by the
// existence of this file, and the desktop chain below is the AppShell chain it
// replaced — same route ids, same files, same order, same comments.
//
// KIOSK OVERRIDES — EIGHT routes, and only these eight, resolve to a kiosk
// view instead (KioskShell.slint:361-608 is the authority):
//
//     home       -> ../kiosk/KioskDiscover.qml       (:367)
//     album      -> ../kiosk/KioskAlbum.qml          (:402)
//     artist     -> ../kiosk/KioskArtist.qml         (:416)
//     library    -> ../kiosk/KioskLibrary.qml        (:481, ContentView.favorites)
//     local      -> ../kiosk/KioskLocalLibrary.qml   (:526, ContentView.local-library)
//     mixtapes   -> ../kiosk/KioskMyQBZ.qml          (:512)
//     collections-> ../kiosk/KioskMyQBZ.qml          (:517)
//     search     -> ../kiosk/KioskSearch.qml         (:589)
//     nowplaying -> ../kiosk/KioskNowPlaying.qml     (:598) — KIOSK-ONLY route
//
// Every OTHER route mounts the SAME desktop view in both shells — that is the
// whole point of sharing the router, and KioskShell.slint:381-586 confirms the
// kiosk re-hosts the desktop components for them.

import QtQuick
import com.blitzfc.qbz
import "../controls"

Item {
    id: root

    // false = desktop shell (AppShell), true = kiosk shell (KioskShell).
    property bool kiosk: false

    // The mounted view. The replacement for AppShell's four `viewLoader.item`
    // references (multi-select: the two Ctrl+A / Escape routers and the two
    // duck-typed reporters) — a QML `id` is scoped to its own document, so
    // those cannot reach into this file and need a public surface.
    readonly property var currentItem: viewLoader.item

    // SYNCHRONOUS — and the reasoning for the failed experiment is kept here so
    // nobody re-runs it.
    //
    // The dead-click complaint ("nothing is happening after the click… ah, it
    // WAS loading, I thought it had died") is real: a route change instantiates
    // the whole view on the UI thread before anything can paint. `asynchronous:
    // true` plus a router-level placeholder looked like the answer, and it made
    // the app MUCH worse — the owner's words were "como si mi conexión fuera de
    // 56kbps" and "nos regresó 10 años atrás".
    //
    // WHY it was worse, because this is the part that is not obvious: async
    // incubation is TIME-SLICED. It yields to the UI thread between slices, so
    // it trades a short freeze for a much longer wall-clock build, and the
    // Loader stays in `Loading` until the ENTIRE tree exists — for Discover >
    // Home that means all 413 cards. A 1s freeze became a five-second skeleton.
    // On a top-level tab the user expects to be instant, that trade is simply
    // the wrong one.
    //
    // WHAT THE REAL FIX TURNED OUT TO BE — and this paragraph used to say the
    // opposite, so read it before acting on a memory of it. It claimed the
    // answer was to "keep the top-level tabs alive across navigation". That is
    // wrong, and it was wrong on the evidence available at the time: BOTH
    // references destroy and re-create the view exactly like this Loader does.
    // Slint gates every view on `if NavState.view == ...` (AppShell.slint:426),
    // which destroys the element tree; Tauri used `{#if activeView === 'home'}`
    // (+page.svelte:6286), which removes it from the DOM — and Tauri even
    // REFETCHES from the network on mount and still transitions instantly.
    // Neither gets its speed from caching the view.
    //
    // What they both do is keep the MOUNT proportional to one viewport, and
    // that is where this port had diverged. Measured 2026-08-17 by driving the
    // real app (`QBZ_QT_NAV_BENCH`, the probe below, and a frame-by-frame read
    // of a screen recording): a click on Discover froze the UI thread for
    // 1.52 s — not slow, FROZEN, zero pixels changing anywhere in the window —
    // where the Slint build painted the same page, artwork included, in a
    // single frame. Inside that time HomeView was building all FOUR of its
    // Discover tabs, because `visible: false` hides an item and does not stop
    // QML from instantiating it, so 19 of 28 section rails belonged to tabs
    // nobody was looking at. Slint runs ONE repeater whose model is chosen by
    // a ternary (HomeView.slint:321) and mounts the rest behind `if`.
    //
    // So the rule is about the SIZE of a mount, not its lifetime: a view may
    // be rebuilt freely as long as it only builds what is on screen. That rule
    // is now enforced statically — qml_eager_tab_audit.py runs in qt-run.sh
    // and fails the build on a heavy per-tab body gated only by `visible:`.
    // Synchronous stays, and is correct.
    // Route-change stopwatch. OFF by default (a LoggingCategory at Warning
    // emits nothing at Info), so the only cost on a normal run is one
    // Date.now() per route change. Turn it on with:
    //
    //     QT_LOGGING_RULES="qbz.nav.timing.info=true" ./target/release/qbz
    //
    // It answers the question no static gate can: of the wall-clock between
    // the click and a usable page, how much is QML instantiation (`built`)
    // versus everything the event loop still owes afterwards (`to-idle`).
    LoggingCategory {
        id: navTiming
        name: "qbz.nav.timing"
        defaultLogLevel: LoggingCategory.Warning
    }
    property double _routeT0: 0
    // Stamped from INSIDE the source binding rather than from a
    // currentViewChanged handler: binding re-evaluation and signal handlers
    // have no guaranteed order, and this way the stamp is by construction the
    // last thing before the Loader instantiates.
    function _stampRoute(path) {
        root._routeT0 = Date.now()
        // Drop the page to transparent HERE and not in an onCurrentViewChanged
        // handler, for the same reason the stopwatch is stamped here: binding
        // re-evaluation and signal handlers have no guaranteed order, and a
        // drop applied after the Loader had already rebuilt would flash the new
        // page at full opacity, blank it, and fade it in again. Inside the
        // binding it is by construction the last thing that happens before the
        // instantiation.
        //
        // The guard matters: two ids can resolve to the SAME file
        // ("recentalbums" / "mostplayedalbums" both mount PlayHistoryView),
        // and then the Loader does not reload, `onLoaded` never fires and
        // nothing would ever bring the page back — a permanently invisible
        // pane. Same for the "" fall-through of an unmapped route.
        if (path !== root._fade.path) {
            root._fade.path = path
            if (path !== "" && !QbzShell.reduceMotion) {
                // Stop first: a second navigation inside the 300 ms would
                // otherwise have a running render-thread Animator writing the
                // property back from underneath this assignment.
                fadeIn.stop()
                viewLoader.opacity = 0
                fadeGuard.restart()
            }
        }
        return path
    }

    // --- Page fade ---------------------------------------------------------
    // COSMETIC, FIXED DURATION, AND IT NEVER WAITS FOR CONTENT. The route
    // change below is synchronous by design (read the Loader's header), so the
    // UI thread is frozen while the new view is built and there is no frame in
    // which a fade-OUT could be shown: the page therefore snaps to transparent
    // in the same turn as the rebuild — invisible, because nothing renders
    // during it — and only the REVEAL is animated. Net added latency: zero.
    // The content is built exactly as fast as it was before; what changed is
    // that its first frame arrives transparent and on its way in.
    //
    // (A real fade-OUT is possible and costs ~32 ms: latch the route, run an
    // Animator 1 -> 0, and let a Timer defer the swap by one frame so the
    // out-fade runs on the render thread DURING the freeze. It is not here
    // because the brief was "sobre todo no añadir latencia extra", and 32 ms
    // is not zero. The seam is one latched property away if that trade is ever
    // taken.)
    //
    // OpacityAnimator, not NumberAnimation, and this is the whole trick: an
    // Animator runs on the RENDER thread, so it keeps advancing while the GUI
    // thread is still paying off everything the mount deferred (layout, the
    // first delegate refills). Measured on this shell with the thread blocked:
    // OpacityAnimator advanced 0.53 where NumberAnimation advanced 0.11. The
    // shared QbzShell.pulseMs cannot serve this: the pulse arrives through
    // `ui(...)` onto the Qt event loop (shell_bridge.rs:966), i.e. the very
    // thread the mount is blocking, so a pulse-driven fade would freeze during
    // exactly the milliseconds it exists to cover.
    //
    // GPU doctrine (qt-frontend/2026-08-11-scenegraph-batches §9) forbids
    // CONTINUOUS animation off the shared pulse, because each dirty frame
    // presents the whole window at ~1.2% GPU. This one is bounded and
    // user-initiated: one navigation buys ~18 presents at 60 Hz, and at rest
    // it writes NOTHING (the Animator is stopped and opacity is a flat 1).
    // The last path handed to the Loader, in a NON-NOTIFYING holder.
    //
    // A plain `property string` here is a BINDING LOOP, and the app said so:
    // "QML Loader: Binding loop detected for property source". The `source`
    // binding calls `_stampRoute`, which READS this to decide whether the route
    // actually changed and then WRITES it — so the binding depends on a
    // property it mutates. QML detects the cycle, warns, and stops
    // re-evaluating, which is a broken router, not just a noisy log.
    //
    // Mutating a MEMBER of an object emits no change signal. The object
    // reference never changes, so the binding's dependency on `_fade` is
    // satisfied once and never fires again.
    readonly property var _fade: ({ path: "" })
    // 300 ms. The contract's first proposal was 120-150 ms; at 140 the owner
    // could not see it at all. Part of that was the duration and part was a
    // real defect (see the note on WHAT fades, below) — 300 fixes the half
    // that is duration, and is still one bounded animation per navigation.
    property int fadeMs: 300

    // WHAT FADES, and why this is the page and not a veil over it.
    //
    // The first cut of this faded a Rectangle laid over the content, coloured
    // from the pane it sits in. That is wrong here, and invisibly so: AppShell
    // paints the pane `surface-main` normally but `surface-main @ 0.22` while
    // the ambient background is on (AppShell.qml:324, QbzTheme.qml:38), and the
    // ambient background is on whenever a track is loaded (QbzTheme.qml:132).
    // So in the configuration the app actually runs in, a veil at FULL opacity
    // covered 22% of the page and the fade read as a flicker. Fading the page
    // itself has no such failure mode, needs no colour at all, and keeps the
    // ambient art visible underneath — which a veil opaque enough to work would
    // have blacked out for the length of the fade.
    //
    // No `layer.enabled`: group opacity would cost an FBO the size of the
    // content pane, allocated at the exact moment the mount is already the most
    // expensive thing on screen. Per-node alpha lets overlapping children show
    // through each other, which is the honest trade — and at 300 ms on a page
    // that is arriving rather than sitting still, it is not visible.
    //
    // reduceMotion (shell_bridge.rs:217) skips the whole thing: the page simply
    // appears, which is the behaviour before this block existed.

    // Watchdog. `onLoaded` is the only thing that brings the page back, and an
    // invisible content pane is the worst failure this file can produce, so a
    // missed signal (a view that fails to instantiate, a source that resolves
    // to nothing) must not be able to leave it at zero.
    Timer {
        id: fadeGuard
        interval: 1500
        repeat: false
        onTriggered: viewLoader.opacity = 1
    }

    OpacityAnimator {
        id: fadeIn
        target: viewLoader
        from: 0.0
        to: 1.0
        duration: root.fadeMs
        easing.type: Easing.OutCubic
    }

    // TABS FADE TOO. A tab switch inside a view (Discover's four, Library's,
    // Local Library's) never touches the Loader's `source`, so none of the
    // machinery above fires — the page changed under the user with a hard cut
    // while a route change faded. Same transition, same duration, for what is
    // the same thing from where the user sits.
    //
    // `activeTab` is the ONE name every tabbed view uses for this (the Binding
    // further down already routes the nav flyout through it), so watching it
    // here covers all of them from one place instead of a copy per view.
    // `ignoreUnknownSignals` is what lets the untabbed views mount unharmed:
    // without it a Connections whose target has no such signal is an error.
    //
    // No watchdog and no opacity assignment: a tab body is built synchronously
    // inside the view, in this same turn, so by the time a frame renders there
    // is something to reveal — `restart()` alone sets the page to `from` (0)
    // and animates it back. The `running` guard keeps a tab that is stamped
    // right after a route change from restarting the fade that route already
    // started.
    Connections {
        target: viewLoader.item
        ignoreUnknownSignals: true
        function onActiveTabChanged() {
            if (!QbzShell.reduceMotion && !fadeIn.running)
                fadeIn.restart()
        }
    }

    Loader {
        id: viewLoader
        anchors.fill: parent
        source: root._stampRoute(QbzShell.currentView === "home"
                ? (root.kiosk ? "../kiosk/KioskDiscover.qml" : "../views/HomeView.qml")
            : QbzShell.currentView === "library"
                ? (root.kiosk ? "../kiosk/KioskLibrary.qml" : "../views/LibraryView.qml")
            : QbzShell.currentView === "local"
                ? (root.kiosk ? "../kiosk/KioskLocalLibrary.qml" : "../views/LocalLibraryView.qml")
            : QbzShell.currentView === "localalbum" ? "../views/LocalAlbumView.qml"
            : QbzShell.currentView === "album"
                ? (root.kiosk ? "../kiosk/KioskAlbum.qml" : "../views/AlbumView.qml")
            : QbzShell.currentView === "artist"
                ? (root.kiosk ? "../kiosk/KioskArtist.qml" : "../views/ArtistView.qml")
            : QbzShell.currentView === "settings" ? "../settings/SettingsView.qml"
            : QbzShell.currentView === "search"
                ? (root.kiosk ? "../kiosk/KioskSearch.qml" : "../views/SearchView.qml")
            : QbzShell.currentView === "playlist" ? "../views/PlaylistView.qml"
            : QbzShell.currentView === "discoverbrowse" ? "../views/DiscoverBrowseView.qml"
            : QbzShell.currentView === "playlistbrowse" ? "../views/PlaylistBrowseView.qml"
            : QbzShell.currentView === "recentalbums" ? "../views/PlayHistoryView.qml"
            : QbzShell.currentView === "mostplayedalbums" ? "../views/PlayHistoryView.qml"
            : QbzShell.currentView === "label" ? "../views/LabelView.qml"
            : QbzShell.currentView === "labelreleases" ? "../views/LabelReleasesView.qml"
            // Artist page > a release section > "See discography", and the
            // album page's "From the same artist" View all. Same two-file
            // contract as the rest of this chain (nav_qt.rs:9-19):
            // artist_releases_qt::open records the id, this arm mounts it.
            // Forgetting the arm is NOT a crash — the ternary falls through
            // to "" and the pane goes blank with nothing logged, i.e. the
            // dead "See discography" in a new costume.
            : QbzShell.currentView === "artistreleases" ? "../views/ArtistReleasesView.qml"
            // Artist page > Network > Origin > the location link, and the
            // header ⋯ > "Artist Scene". Both doors call QbzScene.open(...),
            // which records this id (artist_scene_qt.rs:430). Same two-file
            // contract as everything else in this chain — and the same failure
            // mode if the arm is forgotten: a blank pane, logged nowhere,
            // recoverable with Back, i.e. indistinguishable from the dead
            // click the whole feature exists to fix.
            : QbzShell.currentView === "scene" ? "../views/ArtistSceneView.qml"
            // A credited musician who did NOT resolve to a Qobuz artist:
            // musician_qt.rs:362 records this on the `contextual` branch only.
            // `weak`/`none` are a global modal, not a route — see AppShell.
            : QbzShell.currentView === "musician" ? "../views/MusicianPageView.qml"
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
            // (see the Binding below, which is how it gets set). The kiosk
            // does the same with ONE component on both routes
            // (KioskShell.slint:512,517).
            : QbzShell.currentView === "mixtapes"
              || QbzShell.currentView === "collections"
                ? (root.kiosk ? "../kiosk/KioskMyQBZ.qml" : "../views/myqbz/MyQbzGridView.qml")
            : QbzShell.currentView === "mixtapedetail" ? "../views/myqbz/MyQbzDetailView.qml"
            : QbzShell.currentView === "discobuilder" ? "../views/myqbz/DiscoBuilderView.qml"
            // Settings > Blacklist > Manage. Not reachable from the
            // sidebar — blacklist_qt::open_manager records the route.
            : QbzShell.currentView === "blacklist" ? "../views/BlacklistManagerView.qml"
            // Settings > Offline > Manage offline cache > Open manager. Same
            // shape as the blacklist arm above: not a sidebar section, and
            // offline_manager_qt::open() is what records the route.
            : QbzShell.currentView === "offlinemanager" ? "../views/OfflineManagerView.qml"
            // Awards — reached from the album sidebar's laurel, an "Other
            // awards" card or the landing page's all-awards dropdown. Both
            // routes are recorded by award_qt (open_award / open_albums); the
            // id lives in the controller, not in the route, exactly like
            // "label" / "labelreleases" next to them.
            : QbzShell.currentView === "award" ? "../views/AwardView.qml"
            : QbzShell.currentView === "awardalbums" ? "../views/AwardAlbumsView.qml"
            // Sidebar > Playlists ⋯ > Manage playlists. Not a sidebar
            // section — playlist_manager_qt::navigate() records the route.
            // The route is a TWO-FILE contract (nav_qt.rs:9-19): the caller
            // records the id, this arm mounts it, and the failure mode for a
            // missing arm is a BLANK content pane, logged nowhere.
            : QbzShell.currentView === "playlistmanager" ? "../views/PlaylistManagerView.qml"
            // Purchases — the opt-in Qobuz store surface (Settings >
            // Appearance > Show Purchases, default OFF). The sidebar's direct
            // row records "purchases"; a purchased album's card records
            // "purchase-album" through QbzPurchases.openAlbum(id), which holds
            // the id — the route carries none, exactly like "localalbum".
            //
            // BOTH arms land in the same edit on purpose. This chain's failure
            // mode is a blank pane logged nowhere, and Purchases is the one
            // feature nobody on this team can smoke-test (the owner's region
            // does not sell it, so the account returns an empty list forever):
            // a missing arm here would first be seen by a stranger.
            : QbzShell.currentView === "purchases" ? "../views/PurchasesView.qml"
            : QbzShell.currentView === "purchase-album" ? "../views/PurchaseAlbumView.qml"
            // KIOSK-ONLY route (nav_qt.rs:46-51): the NavRail's fifth tile,
            // the full-screen player. The desktop shell has no equivalent —
            // its transport is the persistent bar — so outside kiosk this id
            // falls through to "" and the pane is blank, which is the desktop
            // behaviour before this router existed.
            : root.kiosk && QbzShell.currentView === "nowplaying" ? "../kiosk/KioskNowPlaying.qml" : "")

        onLoaded: {
            // The reveal starts in the same turn the view finished building,
            // i.e. before a single frame of it has been presented. The render
            // thread picks the Animator up at that first frame, so the fade is
            // exactly as long as it says it is no matter how busy the GUI
            // thread still is.
            fadeGuard.stop()
            if (QbzShell.reduceMotion) {
                viewLoader.opacity = 1
            } else {
                fadeIn.restart()
            }
            var built = Date.now() - root._routeT0
            console.info(navTiming, "[navtiming] " + QbzShell.currentView
                         + " built=" + built + "ms")
            // One more turn of the event loop: everything the mount deferred
            // (layout, the first delegate refills, the pending property
            // updates) has run by the time this fires, so the delta is the
            // honest "how long until the page stops owing work" number.
            var t0 = root._routeT0
            var view = QbzShell.currentView
            Qt.callLater(function () {
                console.info(navTiming, "[navtiming] " + view
                             + " to-idle=" + (Date.now() - t0) + "ms")
            })
        }
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
    //
    // It applies in KIOSK too, and must: KioskMyQBZ serves the same two
    // routes as one component (KioskShell.slint:512,517), so without the
    // discriminator Collections would render the Mixtapes page there for
    // exactly the same reason.
    Binding {
        target: viewLoader.item
        property: "kind"
        value: QbzShell.currentView === "collections" ? "collection" : "mixtape"
        when: viewLoader.item !== null
              && (QbzShell.currentView === "mixtapes"
                  || QbzShell.currentView === "collections")
        restoreMode: Binding.RestoreNone
    }

    // NavFlyout landing tab (Discover > For You, Library > Albums, Local >
    // Folders...). The request rides the bridge (QbzShell.navigateToTab ->
    // navTab/navTabView/navTabSeq) because a QML id cannot cross documents;
    // this Binding is the ONLY writer. It replaces NavFlyout's depth-capped
    // scene search (findTabHost), which the ContentRouter extraction (kiosk
    // port D3) silently outran — every flyout entry landed on the section's
    // default tab instead of its own.
    //
    // `navTabSeq` in the value expression forces re-application when the
    // same entry is clicked twice in a row (a bare navTab would not
    // renotify). `navTabView === currentView` keeps a cross-view request
    // from being stamped on the outgoing view before the route lands. The
    // typeof guard leaves non-tabbed views alone rather than warning about
    // a non-existent property. RestoreNone, like the `kind` Binding above:
    // the target is destroyed on unload. An in-view tab-bar click writes
    // `activeTab` directly and simply wins until the next flyout click —
    // this Binding does not refire without a dependency change.
    Binding {
        target: viewLoader.item
        property: "activeTab"
        when: viewLoader.item !== null
              && QbzShell.navTab !== ""
              && QbzShell.navTabView === QbzShell.currentView
              && typeof viewLoader.item.activeTab === "string"
        value: (QbzShell.navTabSeq, QbzShell.navTab)
        restoreMode: Binding.RestoreNone
    }
}
