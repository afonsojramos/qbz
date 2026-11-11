# 2.2.0

The original QBZ repository and its package channels shut down in September
2026. This release comes from [afonsojramos's fork](https://github.com/afonsojramos/qbz),
which is maintained in keep-the-lights-on mode: security fixes, bug fixes and
compatibility updates, with no new features planned. The app itself is the
same as 2.1.2.

## What changed

- Update checks, What's New, and the About, wiki and bug-report links use the
  fork at github.com/afonsojramos/qbz.
- Updates are signed with the fork's release key. QBZ installs only updates
  signed with it.
- MusicBrainz and ListenBrainz requests identify QBZ by the fork's repository.

## Where to get QBZ

GitHub Releases is the only distribution channel: AppImage, `.deb`, `.rpm`,
Flatpak bundle, tarballs, macOS DMGs and the experimental Windows MSI. The
APT, RPM, AUR, Gentoo, Snap and Flathub channels no longer exist. The README
explains how to install each format.

On macOS, the signed and notarized Homebrew cask continues:
`brew install --cask afonsojramos/qbz/qbz`.

## Upgrading from 2.1.2 or earlier

Earlier versions check the original repository and trust its release key, so
they never offer this update. If 2.1.2 offers "6.6.6", choose **Skip this
version**: that is the original repository's farewell post, not a QBZ release.

Install 2.2.0 once by hand from Releases, the one-line installer or Homebrew.
Your settings, library and login carry over. Later updates arrive in the app
as before.
