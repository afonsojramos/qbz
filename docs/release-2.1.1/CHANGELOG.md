# QBZ 2.1.1

First maintenance release on top of the 2.1 Qt line. The headline is a marked
improvement in scroll response and performance in heavily populated sections;
both side panels become resizable and the My QBZ sidebar collapsible; the rest
is a broad round of bug fixes across playback, library, artist pages, casting
and the media-server integrations.

## UI & navigation

- Resizable playlist sidebar (240-480 px, snapping to the mini rail and closed
  on the way down) and queue/lyrics column (300-600 px, pushing past the
  maximum opens the Listen List); widths persist across restarts (#771).
- Collapsible My QBZ sidebar with per-element hide and a header toolbar.
- Album cards carry the edition/version in the title, with a tooltip when the
  title is elided.
- Custom album covers and artist portraits show on cards app-wide.
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

- Play next / Play later keep their order; Q-mix multiselect, queue and
  create-playlist work (#442).
- Recover from a wedged CPAL output stream and rate-limit POLLERR floods (#660).
- MPRIS reports Paused, not Playing, after a session restore (#683).
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

- Artist page identity: MusicBrainz artists are matched by exact quoted name
  and verified through the page's own top-track ISRCs; an unrelated first
  result is never accepted, Artist Scene never picks a duplicate by album
  count, and identities are cached by Qobuz id / MBID with real expiry (#768).
- MusicBrainz artist data survives 503 and transport failures: retries behind a
  shared rate limiter, cached resolved artists, and a Scene loading heads-up
  with a 5-minute retry timeout.

## Linux desktop

- Copy link works on GNOME under Wayland and inside the Flatpak sandbox: the
  clipboard now goes through Qt instead of arboard (#684).
- The window publishes its desktop-file name, so GNOME's dock shows the QBZ
  icon for the running window and notifications attach to the app.

## Packaging

- The Nix flake installs the prebuilt release tarball instead of building from
  source (#744).
- AUR source packages use `jack` makedepends; Flathub runtime moved to 6.11.

---

**Full changelog:** https://github.com/vicrodh/qbz/compare/v2.1.0...v2.1.1
