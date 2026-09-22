# Publishing to Launchpad (PPA)

This is a step-by-step guide to publish QBZ as a Launchpad PPA
(`ppa:aarklendoia-edtech/qbz`), so users on a supported Ubuntu series can
`apt install` `qbz` and/or `qbzd` directly instead of downloading `.deb`
files from GitHub Releases.

- Launchpad account: `aarklendoia-edtech` (shared with this account's other
  projects — linux-hello, megatokyo, kio-protondrive).
- Personal signing key (manual uploads): fingerprint
  `86EB1CE672402B0B104049C3D4251A0893FE3895` (`aarklendoia@proton.me`) —
  already confirmed on the account from those other projects' setup; no
  new personal-key step needed for qbz.
- CI signing key (automated uploads, see
  [Automated publishing](#5-automated-publishing-ci)): fingerprint
  `F7D8F45FA911E67E87CA98262221AF44E6D29C4C` (`aarklendoia+ci@proton.me`),
  generated 2026-09-22, pushed to keyserver.ubuntu.com and registered on
  the Launchpad account — a separate, dedicated key from the other
  projects' own CI keys (same reasoning as theirs: a leaked CI secret then
  only compromises this one package's upload rights). Its private key is
  stored as the `PPA_GPG_PRIVATE_KEY` GitHub Actions secret.
- PPA: `ppa:aarklendoia-edtech/qbz`
  (<https://launchpad.net/~aarklendoia-edtech/+archive/ubuntu/qbz>) — created
  2026-09-22.

## 0. Why this PPA only targets resolute (26.04) and newer

Unlike this account's other PPAs, this one is **not multi-series**. QBZ's
Qt frontend (`crates/qbz-qt`) needs Qt >= 6.8 — `QtQuick.Effects` and
`Image.retainWhileLoading` are both used in the QML and don't exist before
that. In the Ubuntu archive, `qt6-base`/`qt6-declarative` only reach 6.8 from
**25.04 (Plucky)** onward:

| Series             | qt6-base version |
|--------------------|-------------------|
| jammy (22.04)      | 6.2.4             |
| noble (24.04, LTS) | 6.4.2             |
| oracular (24.10)   | 6.4.2             |
| plucky (25.04)     | 6.8.3             |
| questing (25.10)   | 6.9.2             |
| resolute (26.04)   | 6.10.2            |

(checked live via the Launchpad API, 2026-09-22)

This is exactly why upstream's own official Linux release
(`packaging/nfpm/nfpm.yaml`, `.github/workflows/build-qt-linux.yml`) does
**not** depend on distro Qt at all — it bundles Qt itself in
`/opt/qbz` via `linuxdeploy`/`linuxdeploy-plugin-qt`, precisely so it can
still support Ubuntu 24.04 LTS. This PPA deliberately does **not** do that:
it builds against the target series' own `qt6-*-dev` packages, which is far
simpler and more "normal" Debian packaging — but means it can only ever
target series that actually ship Qt >= 6.8. Ubuntu 24.04 LTS is not, and
will never be, a supported target for this PPA (its Qt 6.4.2 lacks the QML
modules entirely — no amount of vendoring fixes that).

If broader-than-current-devel-series support is ever needed, the two real
options are: (a) an external-dependencies PPA supplying a Qt6 backport for
noble (fragile, outside this project's control), or (b) mirroring upstream's
own AppDir-bundling approach instead of building against system Qt. Neither
is implemented here.

Launchpad's build farm also has no general internet access, so the plain
`cargo build` in `debian/rules` would fail there — see
[Vendoring](#2-vendoring-required-before-every-ppa-upload) for how that's
handled.

## 1. One-time account/PPA setup (manual, on launchpad.net)

All of the below is done — kept for reference / for setting up another
project's PPA under the same account.

1. Launchpad account. ✅ `aarklendoia-edtech` (done).
2. Personal GPG key, published to a keyserver. ✅ `86EB1CE672402B0B104049C3D4251A0893FE3895` (done).
3. Same key imported as an OpenPGP key on the Launchpad profile, confirmed
   via the encrypted-email link. ✅ (done).
4. Ubuntu Code of Conduct, `gpg --clearsign`ed with that key. ✅ (done).
5. **Create the PPA** — profile page → "Create a new PPA" → name `qbz`,
   description, public visibility. ✅ done 2026-09-22:
   `https://launchpad.net/~aarklendoia-edtech/+archive/ubuntu/qbz`

6. Upload tooling, if not already installed:

   ```bash
   sudo apt install devscripts dput debhelper lintian gnupg
   ```

## 2. Vendoring (required before every PPA upload)

Launchpad's build farm builds every package in an isolated environment with
**no general internet access** — builders can only reach a restricted,
whitelisted set of URLs through a proxy (mainly the Ubuntu archive itself).
`crates.io` is not reachable during a build.

`debian/rules`' `override_dh_auto_build` runs `cargo build`/`qt-cargo.py`
against `crates/Cargo.toml`, which fetches this workspace's ~40 crates from
crates.io on a clean checkout — that works fine locally and on GitHub
Actions (which has internet access), but fails on Launchpad unmodified. The
fix, needed only for a PPA build, is to **vendor everything before
packaging**, from a machine with network access:

```bash
# Check the target series' packaged cargo version first:
rmadison -u ubuntu cargo | grep resolute

RUST_TOOLCHAIN=1.93.1 ./debian/scripts/prepare-offline-build.sh
```

`1.93.1` is resolute's own packaged `cargo`/`rustc` version — confirmed
directly on a resolute machine (`apt-cache policy cargo`) while writing this
packaging, so it's the right default for the `resolute` series this PPA
targets. **The vendoring toolchain must match the target series' packaged
cargo version** (installed via `rustup toolchain install <version>` if you
don't have it — the script does this for you). This isn't optional: a
newer local cargo vendoring the tree can silently omit `Cargo.toml.orig`
companion files an *older* cargo needs at build time to verify a vendored
crate's checksum against `Cargo.lock` — a real, previously-hit failure mode
on this account's other Rust PPA packages (see linux-hello's own copy of
this doc for the exact error message). Verify locally before uploading:
`cargo +1.93.1 build --workspace --offline --manifest-path crates/Cargo.toml`.

This script (never run by `debian/rules` itself, and never run on a
Launchpad builder):

- Runs `cargo vendor --manifest-path crates/Cargo.toml vendor` and
  **appends** a `[source.vendored-sources]` block to the repo's tracked
  root `.cargo/config.toml` (that file already carries a deliberate
  "no global x86_64 target-feature flags" comment — see its own header;
  the script never overwrites it, only appends, and only once).
- Blanks each vendored crate's `.cargo-checksum.json` "files" map and
  renames any vendored `*.so`/`*.a`/`*.o`/`*.la` file, both working around
  `dpkg-source`'s hardcoded tar-ignore list for "3.0 (native)" packages
  (see the script's own comments for the specifics — same fix this
  account's other Rust PPA packages already carry).

Once it's run, `debian/rules`' `override_dh_auto_build` detects the
`[source.vendored-sources]` block in `.cargo/config.toml` and automatically
switches to an offline build — this is what actually gets uploaded to
Launchpad.

`vendor/` is git-ignored, and the appended `.cargo/config.toml` block is not
meant to be committed either (`git checkout -- .cargo/config.toml` after
packaging, or just don't `git add` it) — both are a per-release snapshot
regenerated right before packaging. Since this repo's source format is
`3.0 (native)`, `debuild -S` tars up the working directory as-is: whatever
is physically present when you run it (`vendor/`, the appended config
block) gets included, regardless of `.gitignore`. `debian/source/options`
explains why `crates/target/` isn't handled via a tar-ignore pattern
instead — the vendoring script removes it outright.

## 3. Building and uploading a release

```bash
# One changelog entry per series, with a ~ppa<N>~<series><N> suffix so
# versions sort correctly and never collide with an eventual Debian/Ubuntu
# archive version. No "-1" revision: this package is "3.0 (native)" format,
# which can't carry a Debian revision number.
dch --newversion "2.1.2~ppa1~resolute1" --distribution resolute \
  "Automated PPA build for Ubuntu 26.04 (resolute)."

# Build a signed, source-only package (needs the personal GPG key from step 1):
debuild -S -sa

# Upload — dput reads the changes file, uploads over SFTP, Launchpad builds
# it on its farm and publishes to the PPA once it succeeds:
dput ppa:aarklendoia-edtech/qbz ../qbz_2.1.2~ppa1~resolute1_source.changes
```

Track build status at
<https://launchpad.net/~aarklendoia-edtech/+archive/ubuntu/qbz/+packages>
once the PPA exists (step 1.5).

Do **not** repeat this for noble/jammy/oracular — see
[why this PPA only targets resolute](#0-why-this-ppa-only-targets-resolute-2604-and-newer)
above. If a newer series becomes current devel (Launchpad's own `stonking`
at time of writing) and you want to add it, repeat the vendor step with
that series' own packaged cargo version first.

## 4. Once published

`debian/changelog`'s PPA-suffixed entries (`~ppa1~resolute1` etc.) are never
committed — the `dch` step above generates them locally, right before
`debuild`, on top of whatever `release-please` already put in
`debian/changelog` for that tag.

Once the PPA has a successful build, add the `add-apt-repository`/
`apt install qbz` (or `qbzd`) quick-start line to the README, matching this
account's other projects.

## 5. Automated publishing (CI)

[.github/workflows/publish-ppa.yml](../.github/workflows/publish-ppa.yml)
automates the whole cycle above — triggered on every `vX.Y.Z` tag push
(same trigger as `build-debian.yml`'s GitHub Release build), or manually via
`workflow_dispatch`.

**Fully wired up as of 2026-09-22.** All three prerequisites are done:

1. ~~Create the PPA~~ — done. Confirmed live via the Launchpad API:
   `~aarklendoia-edtech/ubuntu/qbz`, publish=true. Its own archive signing
   key (`F26DF396593D8100D8B420A98029388CD218D148`, auto-generated by
   Launchpad to sign the repository's Release file) is unrelated to the CI
   uploader key below — nothing to do with that one.
2. ~~Generate and register qbz's own CI signing key~~ — done. Fingerprint
   `F7D8F45FA911E67E87CA98262221AF44E6D29C4C`
   (`qbz CI <aarklendoia+ci@proton.me>`, RSA 4096, sign-only, expires
   2028-09-21), pushed to keyserver.ubuntu.com and confirmed as an
   additional OpenPGP key on the `aarklendoia-edtech` Launchpad account.
3. ~~Set the `PPA_GPG_PRIVATE_KEY` repository secret~~ — done, via
   `gh secret set PPA_GPG_PRIVATE_KEY --repo Aarklendoia/qbz`.

Nothing left to configure — `publish-ppa.yml` will run end-to-end on the
next `vX.Y.Z` tag push once this branch (or its packaging) reaches `main`.
Sanity-check any time with `workflow_dispatch` before relying on the
tag trigger.

The workflow defaults `series` to `resolute` and `rust_toolchain` to
`1.93.1` — do not change the default series without re-reading
[section 0](#0-why-this-ppa-only-targets-resolute-2604-and-newer) first.
