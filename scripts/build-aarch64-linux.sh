#!/usr/bin/env bash
#
# build-aarch64-linux.sh — build the Linux aarch64 (ARM64) `qbz` binary, e.g.
# for a Raspberry Pi 4 / other ARM single-board boxes.
#
# ── READ FIRST: where can this run? ──────────────────────────────────────────
# The generated Slint crate `qbz_ui` compiles as ONE ~1.6M-line module whose
# single `rustc` needs ~30 GB RAM (RSS+swap). Consequences:
#   • An 8 GB Mac or a 4 GB Pi CANNOT build it natively — it OOMs. Don't try.
#   • Build on a box with >= ~16 GB RAM + swap (the wall is survivable at
#     ~14 GB RAM + swap, proven on the dev box).
#
# This script has two modes, auto-selected by the host arch:
#
#   1. NATIVE  (running ON aarch64 Linux: an ARM runner, an ARM VM, etc.)
#        -> apt-installs the build deps and runs `cargo build --release -p qbz`.
#        This is exactly what a future Slint aarch64 CI job runs — treat the
#        dep list + build line below as the workflow's source of truth.
#
#   2. CROSS   (running on x86-64 Linux with Docker, e.g. the dev box, 30 GB)
#        -> uses `cross` (https://github.com/cross-rs/cross): a container with
#        the aarch64 sysroot + the arm64 dev libs (see crates/Cross.toml) does
#        the compile. Needs Docker running and ~30 GB RAM available to it.
#
# macOS is intentionally NOT a build host here: cross from macOS to
# linux-gnu means re-creating a full Linux sysroot for every native dep
# (ALSA, dbus, fontconfig, …). Run the CROSS mode on a Linux box instead.
#
# Output: dist/qbz-aarch64-linux (an aarch64 ELF — verify with `file`).
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

cd "$(dirname "$0")/.."          # repo root (qbz-nix)
REPO="$(pwd)"
TARGET="aarch64-unknown-linux-gnu"
OUT="$REPO/dist/qbz-aarch64-linux"

# The Linux build deps for the Slint `qbz` binary (kept in sync with the CI
# workflows). PipeWire is reached through its ALSA/JACK compat shims, so only
# libasound2-dev + libjack-jackd2-dev are needed.
DEPS=(
  build-essential pkg-config cmake clang libclang-dev mold nasm
  libasound2-dev libjack-jackd2-dev
  libfontconfig1-dev libfreetype-dev
  libxkbcommon-dev libwayland-dev libxcb1-dev
  libgl1-mesa-dev libegl1-mesa-dev
  libdbus-1-dev libssl-dev
)

arch="$(uname -m)"
case "$arch" in
  aarch64 | arm64)
    echo "[aarch64-build] NATIVE build on $arch"
    if command -v apt-get >/dev/null; then
      sudo apt-get update
      sudo apt-get install -y --no-install-recommends "${DEPS[@]}"
    else
      echo "[aarch64-build] non-apt distro: install the equivalents of: ${DEPS[*]}" >&2
    fi
    ( cd crates && cargo build --release -p qbz )
    install -Dm755 "crates/target/release/qbz" "$OUT"
    ;;
  x86_64 | amd64)
    echo "[aarch64-build] CROSS-compile from $arch via cross (Docker)"
    if ! docker info >/dev/null 2>&1; then
      echo "[aarch64-build] ERROR: Docker is not running. cross needs it." >&2
      exit 1
    fi
    command -v cross >/dev/null || cargo install cross --locked
    # Cap the container's RAM so a runaway qbz_ui rustc gets OOM-killed
    # (losing the build) instead of swap-thrashing the host into a hard freeze
    # — the dev box has no hibernation and dies under memory pressure. Override
    # via CROSS_CONTAINER_OPTS if your box has more headroom.
    # qbz-ui's Slint build imports fonts from the REPO ROOT (../../../../static/
    # fonts/*.ttf) — outside the `crates/` workspace that cross mounts. Mount
    # static/ at its host path so those relative imports resolve in the
    # container (cross 0.2.x mounts the project at the host path).
    # qbz_ui's single rustc demands ~30-33 GB (RSS+swap). --memory-swap must
    # exceed that or Docker OOM-kills the build; 48g leaves headroom yet still
    # caps a runaway well under the host's 30G RAM + 46G swap (no freeze). RAM
    # cap 24g keeps ~6g for the desktop (overflow swaps, doesn't exhaust).
    export CROSS_CONTAINER_OPTS="${CROSS_CONTAINER_OPTS:---memory=24g --memory-swap=48g} --volume $REPO/static:$REPO/static:ro"
    # Cross.toml (workspace root) injects the arm64 dev libs into the image.
    ( cd crates && cross build --release --target "$TARGET" -p qbz )
    install -Dm755 "crates/target/$TARGET/release/qbz" "$OUT"
    ;;
  *)
    echo "[aarch64-build] ERROR: unsupported build host arch: $arch" >&2
    exit 1
    ;;
esac

echo "[aarch64-build] done -> $OUT"
file "$OUT" || true
