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
    // The real fix is NOT here: it is to stop rebuilding these views at all
    // (keep the top-level tabs alive across navigation) and to make the heavy
    // views build incrementally. Until one of those exists, synchronous is the
    // honest behaviour — the same one that shipped before 2026-08-13.
    Loader {
        id: viewLoader
        anchors.fill: parent
        source: QbzShell.currentView === "home"
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
            // contract as the rest of this chain (nav_qt.rs:9-16):
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
            // Sidebar > Playlists ⋯ > Manage playlists. Not a sidebar
            // section — playlist_manager_qt::navigate() records the route.
            // The route is a TWO-FILE contract (nav_qt.rs:9-16): the caller
            // records the id, this arm mounts it, and the failure mode for a
            // missing arm is a BLANK content pane, logged nowhere.
            : QbzShell.currentView === "playlistmanager" ? "../views/PlaylistManagerView.qml"
            // KIOSK-ONLY route (nav_qt.rs:24-29): the NavRail's fifth tile,
            // the full-screen player. The desktop shell has no equivalent —
            // its transport is the persistent bar — so outside kiosk this id
            // falls through to "" and the pane is blank, which is the desktop
            // behaviour before this router existed.
            : root.kiosk && QbzShell.currentView === "nowplaying" ? "../kiosk/KioskNowPlaying.qml" : ""
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
