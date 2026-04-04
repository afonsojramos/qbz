# 1.2.2 — Bye bye plain auth

About a month and a half ago, Qobuz began making changes to their authentication system. This was the reason we introduced OAuth login early on — and fortunately, doing so ahead of time meant no one was truly locked out of the app. This release simply removes direct credential login, making OAuth the sole authentication method. There is also some bugfixing for issues reported on GitHub.

We are considering requesting official API access again, and looking for ways to coordinate with the community to support this request to Qobuz — so that a more severe change in the future does not leave us out.

Thank you all for the continued support for QBZ.

---

## Authentication

  - **OAuth-only login** — username/password authentication has been removed; OAuth via browser is now the sole login method
  - **Token login migrated to V2 crates** — OAuth token handling moved to the V2 core architecture

## Bug Fixes

  - **"Go to Album" / "Go to Artist" broken after launch** — navigation from the player bar now works without starting new playback first (#252)
  - **MPRIS metadata missing on session restore** — playback context and metadata now populate correctly on startup (#240)
  - **Incorrect artist location in Artist Network** — birth/formation location display fixed for MusicBrainz area hierarchy (#235)
  - **Bulk "make available offline" not downloading** — the action now actually triggers track downloads (#231)
  - **F11 fullscreen and ESC exit** — both now work globally across all views (#202)
  - **Scroll position lost on navigation** — scroll state is now saved even without explicit scrolling
  - **Virtualized container scroll restoration** — fixed for all virtualized list and grid components

## New Features

  - **Playlists in favorites quick access** — added to both the sidebar menu and the titlebar dropdown (#233)
  - **Bulk make-offline action** — select multiple tracks and download them all at once from the selection bar (#231)
  - **Sleep inhibition** — system sleep is now prevented during active playback via XDG portal (#229)
  - **Log sanitization** — sensitive IDs and UUIDs are now stripped from logs before upload (#213)
  - **Streaming buffer indicator** — progress bar in the seekbar shows how much of the track has been buffered (#194)

## macOS Improvements

  - **CoreAudio device probing** — sample rate switching and device capability detection
  - **Deep link support** — qobuzapp:// URLs handled natively
  - **Rich notifications** — album artwork in desktop notifications via image_path
  - **Platform-aware settings** — Linux-only options hidden on macOS; menu bar terminology for tray settings
  - **Cmd+, shortcut** — opens settings (standard macOS convention)
  - **Graphics CLI flags gated** — --autoconfig-graphics, --reset-graphics, --reset-dmabuf are Linux-only
  - **Released upstream deps** — coreaudio-rs 0.14.1 and notify-rust 4.13.0 replace git forks
  - **x86_64 cross-compilation** — macOS builds now support Intel Macs

## i18n

  - **20+ views and components** — hardcoded strings replaced with translation keys
  - **Date formatting fixes** — corrected across ArtistDetailView, PurchasesView, BlacklistManagerView, and PurchaseAlbumDetailView
  - **CD quality label** — localized across all badge and quality displays
  - **macOS-specific keys** — menu bar terminology for all 5 locales

## Other

  - **Log upload migrated to paste.rs** — dpaste.org now returns 403/405
  - **Gentoo overlay** — use libayatana-appindicator instead of libappindicator (#262)
  - **Dependency updates** — vite 8, lofty 0.23, axum 0.8, rand 0.10, souvlaki 0.8.3, lucide-svelte 1.0

---

Thanks to [@afonsojramos](https://github.com/afonsojramos), [@GwendalBeaumont](https://github.com/GwendalBeaumont), and [@AdamArstall](https://github.com/AdamArstall) for their contributions to this release.

Full changelog: https://github.com/vicrodh/qbz/compare/v1.2.1...v1.2.2
