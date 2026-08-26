#!/usr/bin/env bash
# Run the unit/doc tests of every crate that needs no Qt (crates workspace).
#
# Same command CI uses (.github/workflows/test-crates.yml, job `test`). The Qt
# frontend crate `qbz-qt` is excluded here — it needs a Qt >= 6.8 install to
# even build — and is tested by the `qt-gate` job instead (audits, its own
# `cargo test -p qbz-qt`, and an offscreen boot).
#
# Usage:
#   ./scripts/cargo-test.sh
#   ./scripts/cargo-test.sh -- --lib          # skip doctests
#   CARGO_BUILD_JOBS=1 ./scripts/cargo-test.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"

exec cargo test \
  --manifest-path crates/Cargo.toml \
  --workspace \
  --exclude qbz-qt \
  --no-fail-fast \
  "$@"
