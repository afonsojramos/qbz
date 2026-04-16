# 1.2.6 — Accessibility patch

A small follow-up on the heels of 1.2.5 driven by user reports. Accessibility takes the front seat: four new WCAG-compliant themes (including a true high-contrast mode), mouse-drag scrolling on every horizontal album row, and a Qobuz Connect badge that no longer disappears when the queue is empty. Two regressions are also fixed, including Infinite Play stopping at the end of the queue.

---

## Accessibility

- **Four new WCAG-compliant themes** in Settings > Appearance > Theme:
  - **WCAG Light** — AAA contrast (7:1+) on every text level, blue accent
  - **WCAG Dark** — AAA contrast on dark backgrounds
  - **High Contrast** — pure white on pure black with yellow accent (21:1 contrast). Built explicitly for severely vision-impaired users
  - **Colorblind** — deuteranopia-safe palette (Wong colors), no red/green adjacency
- **Horizontal album rows now support mouse drag** — click and drag sideways on Home, For You, Award and Label views, just like on iPad or in a browser. Arrow buttons still work as before
- **Qobuz Connect badge always visible** — even with an empty queue (e.g. on app start) you can connect to a remote renderer instead of having to start playing something first

---

## Regression fixes

- **Infinite Play (∞ in the queue) restored** — the toggle now persists across restarts and refills the queue with related tracks (Qobuz `/radio/artist`, seeded from your last played tracks) when the queue ends, instead of stopping. V2 reimplementation, no legacy wrappers
- **Gentoo binary package** — `qbz-bin` ebuild now correctly inherits `xdg-utils`, so the hicolor icon cache and the desktop database are refreshed on install/uninstall. This was the cause of stale icons after upgrading to 1.2.5

---

## Branding polish

- **App icon resized to fill the canvas** (matches Steam/Chrome/Papirus footprint, ~97%) with a subtle blue rim and inner highlight so the matte-vinyl disc doesn't disappear on dark backgrounds; new 512x512 PNG export added for distros that prefer it

---

## Issues addressed

- #318 — Toggle on/Enable Qobuz Connect when the player controls are not visible
- #319 — Infinite Play doesn't queue new tracks
- discussion #313 — accessibility / drag-to-scroll requests from a vision-impaired user

---

Saludos a todos y muchas gracias.
