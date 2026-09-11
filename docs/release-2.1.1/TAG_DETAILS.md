# 2.1.1 — Rebuild Q (You Can (Not) Redo)

First maintenance release for the 2.1 line.

The headline is scrolling: much better scroll response and performance in heavily populated sections. Everything else is a round of bug fixes across playback, library, casting and the media-server integrations.

---

## Fixes

  - **Scrolling** — smoother, faster scroll response in heavily populated sections.
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
