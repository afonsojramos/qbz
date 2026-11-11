# 2.2.0 — New home

QBZ now lives at [afonsojramos/qbz](https://github.com/afonsojramos/qbz). The
original repository and its package channels shut down in September 2026. This
release moves updates, release notes and every link in the app to the new home.
The app itself is the same as 2.1.2.

## What changed

- Update checks, What's New, and the About, wiki and bug-report links use
  github.com/afonsojramos/qbz.
- Updates are signed with a new release key. QBZ installs only updates signed
  with it.
- MusicBrainz and ListenBrainz requests identify QBZ by the new repository.

## Where to get QBZ

GitHub Releases is the only distribution channel: AppImage, `.deb`, `.rpm`,
Flatpak bundle, tarballs, macOS DMGs and the experimental Windows MSI. The
APT, RPM, AUR, Gentoo, Snap and Flathub channels no longer exist. The README
explains how to install each format.

On macOS, the signed and notarized Homebrew cask continues:
`brew install --cask afonsojramos/qbz/qbz`.

## Upgrading from 2.1.2 or earlier

Earlier versions check the old repository and trust the old release key, so
they never offer this update. If 2.1.2 offers "6.6.6", choose **Skip this
version**: that is the old repository's farewell post, not a QBZ release.

Install 2.2.0 once by hand from Releases, the one-line installer or Homebrew.
Your settings, library and login carry over. Later updates arrive in the app
as before.
