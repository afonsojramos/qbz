# QBZ 1.1.19 Release Highlights

## Highlights

- **Audio engine upgraded** — vendored rodio/cpal removed; now on upstream rodio 0.22 with ALSA sample rate fallback, gapless on all backends, and smart quality downgrade
- **Booklet viewer** — albums with digital booklets get an in-app PDF viewer powered by native MuPDF; includes download button
- **3 Neon visualizers** — Laser, Tunnel, and Comet; all music-reactive with bass/mid/high response. Comet extracts colors from album artwork
- **Artist discovery** — MusicBrainz tag-based recommendations with tag-scoped thumbs down and similarity percentages
- **Label Releases redesigned** — logo header, sorting, filters, group-by-artist toggle, and search
- **Explicit badges** — shown across the entire app

## Audio

- ALSA sample rate fallback when DAC doesn't support requested rate
- PipeWire suspension before ALSA Direct exclusive access
- Gapless playback now available on ALSA Direct, defaults to ON
- Smart quality downgrade with hardware compatibility tooltip

## Immersive

- 3-mode background system (Full/Lite/Off) with auto-degrade on low FPS
- Per-panel FPS settings for each visualizer
- Comet visualizer adopts album art palette and fades on silence

## Security

- Removed hardcoded key material, cryptographic session IDs, DOM-based sanitization, log redaction

## Stability

- Graceful shutdown — no more heap corruption on exit
- Window size validation and clamping
- Flatpak tray icon and Snap MPRIS fixes

## Bug Fixes

- Equalizer bars no longer animate when paused
- Navigation scroll position scoped per item with 1-hour TTL
- Home settings apply immediately without reload
- Download paths corrected to Artist/Album/track structure
