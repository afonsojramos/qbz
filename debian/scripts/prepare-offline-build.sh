#!/bin/sh
# Run this locally, with network access, before building a Launchpad source
# upload (debuild -S -sa). NEVER run as part of debian/rules — Launchpad's
# build farm has no general internet access, so everything this script
# fetches must already be sitting in the working directory by the time
# dpkg-source tars it up.
#
# IMPORTANT: vendor with the SAME cargo version the target Ubuntu series
# ships (check with `rmadison -u ubuntu cargo`), not whatever's locally
# "stable" — a newer cargo vendoring the tree can silently omit
# Cargo.toml.orig companion files an OLDER cargo needs at build time to
# verify a vendored crate's checksum against Cargo.lock (hit for real on
# linux-hello, see its own copy of this script). Set RUST_TOOLCHAIN to the
# version to vendor with; defaults to "stable", which is very likely wrong —
# pass the real one explicitly:
#   RUST_TOOLCHAIN=1.93.1 ./debian/scripts/prepare-offline-build.sh
#
# See docs/LAUNCHPAD.md for how this fits into the release process.

set -eu

RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-stable}"
MANIFEST="crates/Cargo.toml"

cd "$(dirname "$0")/../.."

echo "==> Removing crates/target (must not exist when dpkg-source tars up the tree)"
rm -rf crates/target

echo "==> Vendoring Cargo dependencies into vendor/ (toolchain: $RUST_TOOLCHAIN)"
if [ "$RUST_TOOLCHAIN" = "stable" ]; then
  echo "    WARNING: no RUST_TOOLCHAIN given, using 'stable' — check the target" >&2
  echo "    series' cargo version first (rmadison -u ubuntu cargo) and pass" >&2
  echo "    RUST_TOOLCHAIN=<version> explicitly if it differs." >&2
fi
rustup toolchain install "$RUST_TOOLCHAIN" > /dev/null 2>&1 || true
rm -rf vendor
cargo "+$RUST_TOOLCHAIN" vendor --manifest-path "$MANIFEST" --locked --versioned-dirs vendor > /dev/null

# .cargo/config.toml at repo root is a TRACKED file (a deliberate
# "no global x86_64 target-feature flags" guard, all comments, no active
# keys as of writing — see its own header for the SIGILL/SIGSEGV history
# this repo already paid for). Never overwrite it wholesale like a
# from-scratch vendoring script would: append the vendor source-replacement
# block instead, and only once — safe to re-run.
if ! grep -q '^\[source\.vendored-sources\]' .cargo/config.toml 2>/dev/null; then
  {
    echo ""
    echo "# --- appended by debian/scripts/prepare-offline-build.sh ---"
    echo "# Regenerated per release right before packaging; never meant to be"
    echo "# committed (see .gitignore). Do not hand-edit."
    echo "[source.crates-io]"
    echo "replace-with = \"vendored-sources\""
    echo ""
    echo "[source.vendored-sources]"
    echo "directory = \"vendor\""
    echo ""
    echo "[net]"
    echo "offline = true"
  } >> .cargo/config.toml
fi
echo "    $(du -sh vendor | cut -f1) in vendor/, $(find vendor -name '*.orig' | wc -l) .orig files"

echo "==> Disabling cargo's per-file checksum verification for vendored crates"
# dpkg-source's native-tarball builder has a hardcoded exclude list (VCS
# control files, backup/swap files — .git, .gitignore, .svn, CVS, *.orig,
# DEADJOE, ...) that cannot be turned off via debian/source/options. Some
# vendored crate, somewhere, will always have a test fixture or metadata
# file that happens to match one of those generic names — dpkg-source
# silently drops it from the tarball, and cargo's offline build then fails
# verifying that crate's per-file checksums against .cargo-checksum.json.
# Documented Debian Rust-packaging fix: blank out each vendored crate's
# "files" checksum map so cargo only trusts the vendor directory as-is (the
# "package" checksum, verified against Cargo.lock, is untouched).
find vendor -maxdepth 2 -name ".cargo-checksum.json" |
  while IFS= read -r f; do
    jq '.files = {}' "$f" > "$f.tmp" && mv "$f.tmp" "$f"
  done

echo "==> Working around dpkg-source's built-in *.so/*.a/*.o/*.la exclusion"
# Same documented dpkg-source behavior as above, different trigger: "3.0
# (native)" source packages silently drop any file whose name ends in
# exactly .so, .a, .o or .la when dpkg-source tars up the tree — some
# vendored crate (aws-lc-sys, ring, rusqlite's bundled sqlite, etc. all
# appear in this workspace's lock) may ship a prebuilt object or import
# library under one of those exact names. Versioned names (libfoo.so.1.2)
# are NOT affected. debian/rules' build step reverses this before cargo
# runs.
find vendor -type f \( -name '*.so' -o -name '*.a' -o -name '*.o' -o -name '*.la' \) |
  while IFS= read -r f; do mv "$f" "$f.dpkg-source-workaround"; done

cat <<EOF

Ready. From this same working directory (with vendor/ and .cargo/config.toml
populated), debian/rules will build fully offline. Proceed with the dch /
debuild -S -sa / dput cycle from docs/LAUNCHPAD.md.

vendor/ and the appended .cargo/config.toml block are not meant to be
committed to git — regenerated per release right before packaging (see
.gitignore).
EOF
