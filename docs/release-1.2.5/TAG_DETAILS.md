# 1.2.5 — Accolade Watch

The sprint that earns QBZ its ribbon. Dedicated Award view, Release Watch promoted to its own tab, Labels you can follow, and a long-overdue visual identity pass: new matte-vinyl logo across every surface, login chrome, and KDE Plasma / Klassy integration for the custom title bar. A self-update path finally lands for users outside Flathub and the Snap Store.

---

## Accolades (Awards + Press)

  - **Dedicated Award page** — new `AwardView` with header matching the Label/Album conventions, gold press ribbon, and an "Other awards" carousel; wired to V2 commands for `/award/page` and `/award/getAlbums`
  - **AwardAlbumsView** — See-all grid for every award, follow-award as a first-class entity
  - **Award resolution by name** — when `/album/get` omits the award id, the backend now resolves it via `/award/explore`, normalises ids end-to-end as strings, and tolerates inconsistent Qobuz shapes on `AwardPageData` / `AwardMagazine`
  - **Editorial seed** — `/award/explore` seeds across all locales with diacritic-insensitive lookup; hard-coded seed removed
  - **Album-view right sidebar** — shows the full awards stack plus the album's label
  - **Album ribbons extended to press accolades** — last award wins on the card; the dedicated sidebar still shows the full stack
  - **Gold laurel wreath icon** — replaces the trophy on `AwardView`; new SVG asset
  - **Home cleanup** — editorial ribbons added for Qobuzissime and Album of the Week on the home rails; Essential Discography keeps cards clean; ribbon moved to bottom-left so the action overlay covers it on hover
  - **Editor's Picks tab cleanup** — redundant "Album of the Week" and "Qobuzissime" ribbons removed from cards that live inside their own eponymous sections

---

## Release Watch

  - **Dedicated tabbed view** — mirrors the Qobuz mobile layout with its own entry in Discover; moved below Your Mixes in the home grid
  - **Backend** — `/favorite/getNewReleases?type=artists` as REST (not signed RPC), projected onto `SearchResultsPage`, artist field backfilled from `artists[0]`
  - **Persistence** — Release Watch now survives home-cache invalidations so it renders immediately on revisit

---

## Labels

  - **Follow / unfollow labels** — mirrors Follow Artist; heart overlay on `LabelView`'s more-labels cards
  - **Favorites → Labels tab** — new tab in the Favorites view
  - **Label follow button** — pill replaced by a 6px rounded rectangle to match the rest of the UI; label-card background hover dropped so the follow button hover stays visible

---

## Visual identity

  - **New matte-vinyl logo** (Flathub-compliant: square canvas, 74% footprint, no baked shadow/gloss) applied to every icon asset: Tauri standard sizes, Windows Store squares, Android icons and mipmaps, `static/`, README, tray
  - **Monochrome symbolic variants** — `tray-light.png` (black glyph) and `tray-dark.png` (white glyph) shipped alongside `tray.png`; the Linux tray picks the matching variant at runtime by probing GNOME `color-scheme`, GTK `prefer-dark-theme`, and KDE `kdeglobals`
  - **Login screen refresh** — logo resized to 175px with a subtle `--text-muted` drop-shadow using `color-mix()` for real alpha, plus QBZ and "Qobuz™ Player" branding under the mark; `app.tagline` rewritten in en/es
  - **About dialog** — fixed a duplicated "License" label (the second row was meant to show the platform); codename updated to "Accolade Watch"
  - **DAC Setup Wizard icon contrast** — button icon drops `invert(1)` so it reads on light accent themes (Catppuccin Lavender, Dracula…); the tooltip copy of the same icon now has `invert(1)` so it reads on the dark surface bg

---

## Desktop integration (Linux)

  - **Window controls auto-detect** — new `desktop_theme` Rust module reads `kwinrc`, `kdeglobals`, and `klassyrc`; a runtime-derived "Klassy (auto-detect)" or "Plasma (auto-detect)" preset mirrors the system decoration colors and button shape when Plasma is detected, and stays hidden otherwise
  - **Klassy button shapes** — `mapKlassyShapeToQbz()` maps Klassy's `IntegratedRoundedRectangle`, `FullHeightRectangle`, `FullHeightRoundedRectangle`, `Tab`, `Circle`, `Square` onto QBZ's `ButtonShape` set; adds a new `full-height-rounded` variant to cover Klassy's most common preset
  - **Opt-in rounded window corners** — "Match system window chrome" toggle persists to `window_settings.db` and gates the Tauri window transparency decision at startup; on next launch the main window is rebuilt transparent and the detected corner radius (10 Klassy, 6 Breeze, 12 Adwaita, 8 fallback) is applied via `clip-path` + `border-radius` with GPU compositing for clean anti-aliasing on WebKitGTK
  - **ksni tray on Linux** — replaces the libayatana-appindicator path so left-click actually toggles the window (issue #310)
  - **Window size clamp** — persisted sizes that exceed the largest available monitor now clamp to fit instead of opening off-screen

---

## Streaming, audio, and player

  - **Request signing expanded** — all remaining Qobuz API endpoints and every search endpoint now sign their requests; stronger compliance and fewer 403 edge cases
  - **BitPerfectMode surfaced in QualityBadge** (#288) — mode now flows through `PlaybackEvent` and the badge updates on playback start
  - **DAC rate unsupported fallback** (#288) — track quality is downgraded and the backend falls back to `plughw` instead of CPAL when the DAC can't handle a requested rate; clearer error when CPAL cannot open an enumerated device
  - **Output device missing notification** (#307) — user gets a toast when the selected output device disappears
  - **Audio output badges refresh** on playback start
  - **ALSA Direct `hw` immersive volume lock** — volume slider is disabled in the immersive player when ALSA Direct hw is active (bit-perfect lock)
  - **ALSA default plugin** — `alsa_plugin` defaults to `Hw` when switching to the ALSA backend
  - **Gapless loop-one fix** — logic moved from backend to frontend; loop-one now works with gapless playback
  - **Gapless local-library fallback** — tries local library if not in any cache; fails silently when track isn't found
  - **ALSA engine shutdown** — engine stops before dropping the ALSA stream on format changes
  - **Real offline mode** (#279) — snapshot streaming, network blocked, diagnostic logs
  - **Player fixes** — `durationSecs` passed on session-restore first play so the seekbar advances (previously stuck); qconnect always persists local session to preserve track-level restore (#304)

---

## Updater (opt-in, non-store builds)

  - **tauri-plugin-updater integrated** — backend + frontend; `UpdateProgressModal` added with i18n; wired into app bootstrap and Settings
  - **Gated behind Cargo feature `updater`** — sandboxed builds (Flathub, Snap) pass `--no-default-features` so the updater stays disabled for store-managed installs
  - **Signed manifest** — minisign public key baked in; CI signs artifacts and consolidates the update manifest; updater manifest path no longer triggers an infinite `tauri dev` rebuild loop

---

## UI fixes and polish

  - **Smart positioning for sidebar menus** — the per-playlist right-click menu now flips above the cursor when it would fall below the window; the Sort submenu inside the general playlists menu uses `use:portal` to escape clipping by `.dropdown-menu`
  - **Genre filter popup** — collision check reads the CSS `max-height` (530/630 width, 500/700 height) instead of the measured rect so the popup flips above the trigger or clamps to the viewport when content grows asynchronously
  - **Right-section collapse** (#303) — hamburger at narrow widths instead of overflow
  - **QualityBadge / QconnectBadge compact variants** — keeps them at full bar height without overflow
  - **Silk animations removed** from ForYouTab mix cards and Your Mixes covers (performance)
  - **Multi-select drag** enabled in all views; track drag & drop extended to artist and search views and onto sidebar playlists; compact drag ghost with artist + album
  - **Lyrics active line** uses theme accent color
  - **For-You load order** — waits for `topArtists`/`recentAlbums` before loading dependent rails
  - **Remote-mode indicator** sits above the player bar, not inside it
  - **Banner layout** uses a CSS variable; i18n key fixed; responsive height
  - **Favorites Select All** — new checkbox in multi-select mode across track views

---

## Internal architecture

  - **`commands_v2` refactor** — former single-file module is now a module directory (`auth`, `audio`, `catalog`, `diagnostics`, `discovery`, `favorites`, `helpers`, `image_cache`, `integrations`, `legacy_compat`, `library`, `link_resolver`, `playback`, `playlists`, `queue`, `runtime`, `search`, `session`, `settings`) — easier to navigate, smaller files
  - **Gapless check flattened** — reduced from 4 nested `if`s to a flat control flow in the local-library path
  - **Custom device name** persists across restarts for QConnect
  - **Prefetch perf** — cache depth increased to 5 tracks with 2 concurrent CMAF segment downloads in parallel
  - **qbzd scaffolding** — not yet exposed to users; the CI workflow is paused (`push.tags` trigger commented out) and the CastPicker's "QBZ Daemon" tab is gated behind a feature flag that currently resolves to `false`. Re-enable when qbzd ships
  - **Nix devShell** — exports `LD_LIBRARY_PATH` for libappindicator and `LIBCLANG_PATH` for mupdf-sys bindgen
  - **Dependency bumps** — tauri-plugin-dialog 2.7.0, tauri-plugin-deep-link, rodio 0.22.2, vite 8.0.8, svelte 5.55.2, sveltejs/kit 2.57.1, rand, jsdom 29.0.2, notify-rust 4.14.0

---

## Packaging

  - **Snap** — MPRIS slot restored after snapcraft approval; exclusive-audio note added to the description
  - **CI ARM64** — native ARM runner replaces QEMU for the aarch64 build; targets glibc 2.35; dropped OpenSSL in favour of rustls across all crates

---

Full changelog: https://github.com/vicrodh/qbz/compare/v1.2.4...v1.2.5
