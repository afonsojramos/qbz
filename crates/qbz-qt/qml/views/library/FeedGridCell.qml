// FeedGridCell — one 200x246 slot of the Library GRID, dispatched by `kind`
// to the SAME card family the Slint mounts (FavoritesView.slint:1084-1187):
// track -> discover/TrackCard, album -> discover/AlbumCard, artist ->
// discover/ArtistGridCard, playlist -> discover/PlaylistCard, label ->
// discover/LabelCard.
//
// EXTRACTED from views/LibraryView.qml's GridView delegate (track rule 2).
// The per-cell cover skeleton rides along, because it is a property of the
// SLOT (it must sit above the card and below nothing) rather than of the view.

import QtQuick
import com.blitzfc.qbz
import "../../cards"
import "../../controls"
import "../../theme"

Item {
    id: cell

    QbzTheme { id: theme }

    /// The LibraryView root (artMap, skelPhase, showLocal, activeTab).
    property var view: null
    /// The feed row this slot renders.
    property var item: ({})
    /// Index INSIDE the reported artwork window — the skeleton's 48-instance
    /// animation cap counts from the viewport, never from a 10k-row model.
    property int cellIndex: 0

    width: 200
    height: 246

    Component {
        id: albumCardComp
        AlbumCard {
            albumId: cell.item.id
            title: cell.item.title
            artist: cell.item.artist
            artistId: cell.item.artistId
            genre: cell.item.genre
            year: cell.item.year
            qualityTier: cell.item.qualityTier
            artSource: cell.view.artMap[cell.item.artKey] || ""
            isFavorite: cell.item.isFavorite
            isPinned: cell.item.isPinned
            // The pin payload's display snapshot: the REMOTE url, never
            // `artMap` (a local file:// cache path that means nothing after a
            // cache wipe). Without it every album pinned from the Library grid
            // landed in the Home Pinned rail as a permanent grey placeholder,
            // across restarts.
            artworkUrl: cell.item.imageUrl || ""
            // The source word is ALWAYS passed (the "Block this album" menu
            // gate reads it); the toolbar's show-local toggle only decides
            // whether the BADGE is drawn — FavoritesView.slint:1097
            // `show-source-badge: LibraryAllState.show-local`, the same gate
            // the track card below already uses.
            source: cell.item.source
            showSourceBadge: cell.view.showLocal
        }
    }
    // Group separator. These are pseudo-rows injected into the SAME flat model
    // by `visibleItems`, so the list stays virtualized and the windowed
    // artwork report is unaffected. Only the tracks LIST produces them today;
    // the arm stays so a grouped grid cannot render a card for a header row.
    Component {
        id: groupHeaderComp
        Item {
            Text {
                anchors.left: parent.left
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 6
                text: cell.item.title
                color: theme.textSecondary
                font.pixelSize: 13
                font.weight: theme.weightSemibold
                elide: Text.ElideRight
                width: parent.width
            }
        }
    }
    Component {
        id: trackCardComp
        TrackCard {
            item: cell.item
            artSource: cell.view.artMap[cell.item.artKey] || ""
            showSourceBadge: cell.view.showLocal
        }
    }
    Component {
        id: artistCardComp
        ArtistCard {
            item: cell.item
            artSource: cell.view.artMap[cell.item.artKey] || ""
            isPinned: cell.item.isPinned === true
            // Snapshot url for the pin payload (see the album card above).
            artworkUrl: cell.item.imageUrl || ""
            // The Slint reference SPLITS here, and this one component serves
            // both of its call sites: the ARTISTS tab grids pass follow-mode
            // "none" (FavoritesView.slint:1831/1865 — every artist in that
            // grid is followed by definition, so the chip would be a permanent
            // tick), while the mixed ALL feed keeps the default and seeds
            // `following` from `is-favorite` (:1143-1147), which is exactly
            // the second spelling ArtistCard reads.
            followMode: cell.view.activeTab === "artists" ? "none" : "toggle"
        }
    }
    Component {
        id: playlistCardComp
        PlaylistCard {
            // artworkUrl is not passed on purpose: the card defaults it to
            // `item.imageUrl`, which IS this row's remote cover.
            item: cell.item
            artSource: cell.view.artMap[cell.item.artKey] || ""
            isPinned: cell.item.isPinned === true
        }
    }
    Component {
        id: labelCardComp
        LabelCard {
            item: cell.item
            artSource: cell.view.artMap[cell.item.artKey] || ""
        }
    }
    Loader {
        anchors.fill: parent
        sourceComponent: cell.item.kind === "group-header" ? groupHeaderComp
            : cell.item.kind === "album" ? albumCardComp
            : cell.item.kind === "track" ? trackCardComp
            : cell.item.kind === "artist" ? artistCardComp
            : cell.item.kind === "playlist" ? playlistCardComp
            : labelCardComp
    }
    // THE fix for "many grey squares that fill in unevenly": each pending
    // cover shimmers on its own and stops the moment ITS file:// path lands in
    // artMap, so the grid resolves progressively instead of looking like a
    // wall of dead tiles. Artists and labels are excluded — their cards
    // already draw a designed round gradient+glyph portrait placeholder
    // (ArtistGridCard/LabelCard). A bare Rectangle: it does not take pointer
    // events, so the card's hover/click areas keep working underneath.
    QbzSkeleton {
        variant: "art"
        width: 200
        height: 200
        visible: (cell.item.kind === "album"
                  || cell.item.kind === "track"
                  || cell.item.kind === "playlist")
            && cell.item.imageUrl !== ""
            && (cell.view.artMap[cell.item.artKey] || "") === ""
        phase: cell.view.skelPhase
        cellIndex: cell.cellIndex
    }
}
