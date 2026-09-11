# QBZ 2.1.1

First maintenance release on top of the 2.1 Qt line. The headline is a marked
improvement in scroll response and performance in heavily populated sections;
the rest is a broad round of bug fixes across playback, library, casting and
the media-server integrations.

## UI & navigation

- Smoother, faster scroll response and rendering in heavily populated sections.
- Mouse wheel scrolling is predictable and natural again.
- Navigation shortcuts survive input focus, and text inputs release focus correctly.
- 2.1.x polish round: header selects, seekbar, keyboard shortcuts and per-tab filters.
- Qt window lifecycle and grid artwork handling restored.

## Playlists, Mixtapes & Collections

- Remove from playlist works again on every source, macOS included (#764).
- Navidrome / Jellyfin / Plex tracks now show their cover art in playlists (#764).
- Mixtape/Collection detail: a song opens its album, the row menu reads "Remove
  from Mixtape", and the quality / source / tracks columns are correct (#766).
- Add a playlist to a mixtape from its header, with a live sidebar/detail update (#766).
- Account import no longer creates broken mixtapes and collections; their import
  is paused until id remap lands (#766).

## Playback & audio

- Recover from a wedged CPAL output stream and rate-limit POLLERR floods (#660).
- ALAC signal format is derived from the codec configuration.
- Rate-scaled ALSA callback buffering.
- Exclusive mode row restored on macOS and preserved across an audio-backend change (#748).

## Library & artwork

- Local Library offline visibility restored; filters and sorting unified.
- SACD artwork fallback and image sidecars.
- Remote albums without a server album id are skipped instead of shown broken.

## Media servers, casting & QConnect

- Jellyfin Quick Connect added; remote connections hardened.
- DLNA renderers stop vanishing (http pinned to 1.4.0), UPnP service URLs are
  more tolerant, and casting crashes now leave diagnostics (#745).
- Fatal signals are reported instead of dying as a bare exit; preflight no
  longer eats the on-disk log.
- QConnect preserves playback state and queue occurrence identity across handoffs.

## Integrations

- MusicBrainz artist data survives 503 and transport failures: retries behind a
  shared rate limiter, cached resolved artists, and a Scene loading heads-up
  with a 5-minute retry timeout.

## Packaging

- AUR source packages use `jack` makedepends; Flathub runtime moved to 6.11.

---

**Full changelog:** https://github.com/vicrodh/qbz/compare/v2.1.0...v2.1.1
