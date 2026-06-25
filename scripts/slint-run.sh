#!/usr/bin/env bash
# QBZ Slint — build with cargo, then run the BINARY DIRECTLY.
#
# Why this exists (vs slint-dev.sh which does `cargo run`): `cargo run` launches
# the app as a cargo-managed run target — the process inherits CARGO_* env and a
# cargo launch context, which KDE plasma-systemmonitor surfaces by labelling the
# RUNNING APP as "cargo" instead of "qbz-slint" (the kernel comm is still
# qbz-slint; it's only the monitor's display name). Running the prebuilt binary
# directly (no cargo wrapper) makes process monitors show it as `qbz-slint`,
# cleanly separate from the cargo/rustc BUILD processes.
#
# ─── THE MEMORY WALL ────────────────────────────────────────────────────────
# A single RELEASE rustc for qbz-slint can hit ~20-24 GB. This box has 30 GB,
# no hibernation, and HARD-FREEZES (power-cycle, lost work) on OOM/swap-thrash.
# The old fixed `-Z threads=16` + opt-level=3 build SIGTERM'd at codegen whenever
# the desktop held ~8-11 GB. To "skip the wall" this script:
#   (a) SCALES rustc frontend threads + codegen-units + opt-level to the RAM that
#       is actually free, so the compile FITS instead of being OOM-killed, and
#   (b) runs the build under the `cargo-capped` cgroup so even a runaway dies
#       cleanly (build killed) instead of freezing the whole box.
#
# Tiers (auto, from `MemAvailable`); override any knob via env:
#   >= 26 GB free  → FAST  : threads=16 cgu=16  opt=3  (identical to slint-dev,
#                            uncapped) — best with the desktop closed / on a TTY.
#   14-26 GB free  → SAFE  : threads=2  cgu=256 opt=3  (fits a normal desktop).
#   <  14 GB free  → MIN   : threads=1  cgu=256 opt=2  (slow but never freezes).
# NOTE: the SAFE/MIN tiers change codegen-units/opt-level, so the produced binary
# is functionally identical but not byte-identical to the FAST/distribution build,
# and switching tiers (or any RUSTFLAGS/profile knob) forces a one-time rebuild.
#
# Usage: ./scripts/slint-run.sh [extra app args]
#   FAST=1                        ./scripts/slint-run.sh   # force the fast build
#   THREADS=4 CODEGEN_UNITS=128 OPT=3 ./scripts/slint-run.sh   # manual override
#   CAPPED=0                      ./scripts/slint-run.sh   # disable the cgroup cap
#   NORUN=1                       ./scripts/slint-run.sh   # build only, don't exec
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.."

avail_mb=$(free -m | awk '/^Mem:/ {print $7}')

# --- Pick build settings from available RAM (any knob overridable via env) ----
if [[ "${FAST:-0}" == 1 ]] || (( avail_mb >= 26000 )); then
  TIER=FAST;  THREADS="${THREADS:-16}"; CODEGEN_UNITS="${CODEGEN_UNITS:-16}";  OPT="${OPT:-3}"
  CAPPED="${CAPPED:-0}"          # ample RAM → earlyoom is the net, no cgroup cap
elif (( avail_mb >= 14000 )); then
  TIER=SAFE;  THREADS="${THREADS:-2}";  CODEGEN_UNITS="${CODEGEN_UNITS:-256}"; OPT="${OPT:-3}"
  CAPPED="${CAPPED:-1}"
else
  TIER=MIN;   THREADS="${THREADS:-1}";  CODEGEN_UNITS="${CODEGEN_UNITS:-256}"; OPT="${OPT:-2}"
  CAPPED="${CAPPED:-1}"
  echo "[slint-run] WARNING: only ${avail_mb} MB free — lowest-memory tier (slow). Close apps / drop to a TTY for a faster build." >&2
fi

# RUSTFLAGS OVERRIDES (does not merge) the .cargo/config.toml rustflags, so the
# AES-NI/SSSE3 features must be re-listed (keep in sync with slint-dev.sh).
export RUSTFLAGS="-C target-feature=+aes,+ssse3 -C link-arg=-fuse-ld=mold -Z threads=${THREADS}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CODEGEN_UNITS}"
export CARGO_PROFILE_RELEASE_OPT_LEVEL="${OPT}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"   # one rustc at a time (memory)

echo "[slint-run] tier=${TIER} avail=${avail_mb}MB → threads=${THREADS} codegen-units=${CODEGEN_UNITS} opt-level=${OPT} capped=${CAPPED}"

if [[ "${CAPPED}" == 1 ]] && command -v cargo-capped >/dev/null 2>&1; then
  # Cap the cgroup just under what's free so a runaway is OOM-killed inside the
  # cgroup (build dies, box lives) — leave ~3.5 GB for earlyoom's floor + desktop.
  cap=$(( avail_mb - 3500 )); (( cap > 26000 )) && cap=26000; (( cap < 10000 )) && cap=10000
  export BUILD_MEM_MAX="${cap}M"
  export BUILD_MEM_HIGH="$(( cap - 2000 ))M"
  echo "[slint-run] cgroup cap: high=${BUILD_MEM_HIGH} max=${BUILD_MEM_MAX}"
  cargo-capped cargo +nightly build --release --manifest-path crates/Cargo.toml -p qbz-slint
else
  cargo +nightly build --release --manifest-path crates/Cargo.toml -p qbz-slint
fi

[[ "${NORUN:-0}" == 1 ]] && { echo "[slint-run] build done (NORUN set)."; exit 0; }

# exec the binary directly — no `cargo run`, so no CARGO_* env / cargo context,
# so the monitor shows `qbz-slint`.
exec crates/target/release/qbz-slint "$@"
