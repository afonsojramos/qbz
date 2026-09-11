# 2.1.1 — Rebuild Q (You Can (Not) Redo)

First maintenance release for the 2.1 line.

The headline is scrolling: much better scroll response and performance in heavily populated sections. Two small features rode along because they kept coming up: both side panels are now resizable, and the My QBZ sidebar can be collapsed.

Everything else is a round of bug fixes across playback, library, artist pages, casting and the media-server integrations, plus two Linux desktop fixes for GNOME users.

---

## New

  - **Resizable panels** — drag the playlist sidebar from 240 to 480 px and the queue/lyrics column from 300 to 600 px; the sidebar snaps to its mini rail and closed states on the way down, and pushing the queue past its maximum opens the Listen List. Widths persist (#771).
  - **Collapsible My QBZ sidebar** — the My QBZ tree collapses, each element can be hidden, and its header gains a toolbar.
  - **Playlist into a mixtape** — add a whole playlist to a mixtape from its header, with the sidebar and detail updating live (#766).

---

## Fixes

  - **Scrolling** — smoother, faster scroll response in heavily populated sections.
  - **Artist page identity** — MusicBrainz data is matched by exact name and verified through the artist's own track ISRCs, so a page never shows another band's biography or members; identities are cached by id, never by name (#768).
  - **Album cards** — the title carries the edition ("50th Anniversary", "Deluxe") like the album page does, with a tooltip when it is cut.
  - **Queue** — Play next / Play later keep their order, and Q-mix multiselect, queue and create-playlist work (#442).
  - **Custom artwork** — a custom album cover or artist portrait shows on cards everywhere, not only on the page.
  - **Copy link on GNOME (Flatpak/Wayland)** — the clipboard goes through Qt, so sharing works where the old path silently failed (#684).
  - **Dock icon on GNOME/Wayland** — the window now identifies itself to the shell, so the dock shows the QBZ icon instead of a generic one, and notifications attach to the app.
  - **MPRIS after a restart** — a restored session reports Paused, not Playing (#683).
  - **Nix flake** — installs the prebuilt release tarball instead of a source build that could not find the Qt host tools (#744).
  - **Remove from playlist** — works again on every source, macOS included (#764).
  - **Playlist cover art** — Navidrome / Jellyfin / Plex tracks now show their covers in playlists (#764).
  - **Mixtapes & Collections** — a song opens its album, the row menu reads "Remove from Mixtape", and the quality / source / tracks columns are correct (#766).
  - **Account import** — no longer creates broken mixtapes and collections; their import is paused until id remap lands (#766).
  - **MusicBrainz artist data** — survives 503 and transport failures via retries behind a shared rate limiter, with cached artists and a Scene loading heads-up.
  - **Exclusive mode (macOS)** — the row is back and is preserved across an audio-backend change (#748).
  - **DLNA / casting** — renderers stop vanishing, UPnP URLs are more tolerant, and crashes now leave diagnostics (#745).
  - **Stuck output stream** — recovers from a wedged CPAL stream and rate-limits POLLERR floods (#660).
  - **Jellyfin Quick Connect** — added, and remote connections are hardened.
  - **Local Library** — offline visibility restored, filters and sorting unified.
  - **QConnect** — playback state and queue identity are preserved across handoffs.
  - **Mouse wheel** — scrolling is predictable and natural again.
  - **Focus & shortcuts** — navigation shortcuts survive and text inputs release focus correctly.
  - **ALAC** — signal format is derived from the codec, with rate-scaled ALSA buffering.
  - **Artwork** — SACD fallback and image sidecars; remote albums without a server id are skipped.

---

Thanks to everyone who reported, tested and kept the feedback coming — this release is built on it.

**Full changelog:** https://github.com/vicrodh/qbz/compare/v2.1.0...v2.1.1
