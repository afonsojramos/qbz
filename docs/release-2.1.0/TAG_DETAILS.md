# 2.1.0 — __CODENAME__

This one is a little unusual.

After 2.0.x, I honestly thought we had settled down: Slint, no WebView, a fully native UI, and everything seemed to be in the right place.

Naturally, I decided to migrate the UI again.

QBZ is now Qt.

---

## 1. The weird

I know, being able to say "QBZ is written in Slint" was cool. I still like Slint, I think it has a lot of future, and I'll probably use it again somewhere else.

For QBZ, though, resource usage, development speed, and Qt's maturity eventually made the decision pretty easy.

More importantly, this migration fixed something that had been following QBZ since the Tauri days: **QBZ is finally modular**. Playback, library, integrations and UI are now separate pieces instead of growing around each other.

Ironically, Slint deserves some credit for that. Working around its constraints forced better boundaries and architectural decisions that QBZ had never really had before.

So this time changing the face of QBZ didn't mean rewriting the application around it.

Much better.

---

## 2. The good

**Local Library got a lot of love.**

Its backend has been heavily reworked and optimized with one goal in mind: a library with hundreds of thousands of tracks should behave much like one with a few thousand.

I think we're finally there.

And there is plenty to see on the surface too: a classic Library Explorer, broader format support, audio CD playback and ripping, SACD images, a much better metadata editing and matching experience, and tighter integration between your own music and Qobuz.

Plex has been around for a while because that's what I use. **Jellyfin and Navidrome/Subsonic are now first-class sources too**, mostly because people kept asking for them.

Your music can increasingly just be *your music*, regardless of where it happens to live.

---

## 3. The necessary

A lot of 2.1 isn't about adding another button somewhere. It's about making things that already existed behave the way they probably should have from the beginning.

Playlist importing grew considerably and can now bring in playlist files, JSON, ListenBrainz and Last.fm.

The queue finally has a proper full view. If we have a classic Library view, why shouldn't the queue get one too?

**Qobuz Connect and casting got a serious hardening pass.** Pairing, handoffs, buffering, shuffle, volume and casting from local or self-hosted sources are all considerably more solid. Multi-device listening should simply feel faster and less fragile now.

Performance in general is one of the biggest winners of this release. QBZ still looks like QBZ; it just spends much less effort doing it.

macOS also became much less annoying to install. [@afonsojramos](https://github.com/afonsojramos), maintainer of the Mac port, now provides notarized DMGs, and QBZ can also be installed through Homebrew.

No more convincing macOS that yes, you really did mean to open the application.

---

## 4. The bad

The **aarch64 native builds** have unfortunately left some older distributions behind because they don't provide versions of some packages QBZ now depends on.

That includes Debian-based systems older than Bookworm.

I haven't found a clean solution to this yet. If someone has one, I would genuinely love to hear it.

This only affects the native `.deb` and AppImage builds. **Flatpak and Snap remain available** on those systems without this problem.

---

## 5. The unexpected

QBZ now runs on **Windows**.

There is an MSI installer, native playback, bit-perfect output, WASAPI exclusive mode and the things needed to make it actually behave like QBZ.

What it currently lacks is love.

There is a rather explicit disclaimer attached to the Windows build, and I strongly recommend reading it before going straight for the installer:

**Windows is not currently a supported QBZ platform. The build is provided as-is.**

I'll also be transparent about why. I hated using Windows while testing the port — from installing and configuring the box to dealing with its audio setup — and I don't want to spend more time than absolutely necessary inside Redmond's OS for every release.

Sorry. Long-time Gentoo Linux user. Some biases are too old to fix.

Still, the port works and the hard part is already there, so keeping it hidden would be silly.

It is also open for adoption. If someone wants to maintain Windows and eventually give it the same treatment [@afonsojramos](https://github.com/afonsojramos) gave macOS, please get in touch.

---

That's the short version.

I made a GitHub Discussion with screenshots and a more human tour through 2.1.0 if you want to see what actually changed:

[Read the 2.1.0 Discussion →](https://github.com/vicrodh/qbz/discussions)

If implementation details, fixes and issue numbers are more your thing, there is a changelog for that:

[Read the 2.1.0 CHANGELOG →](https://github.com/vicrodh/qbz/blob/main/docs/release-2.1.0/CHANGELOG.md)

Full diff: https://github.com/vicrodh/qbz/compare/v2.0.2...v2.1.0
