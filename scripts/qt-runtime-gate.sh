#!/usr/bin/env bash
# Shared Linux CI/local runtime gate. Serial builds, no personal profile.
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.."
export QBZ_PREBUILT_SHADERS=1
python3 scripts/qt-cargo.py test --manifest-path crates/Cargo.toml -p qbz-qt --no-fail-fast
target_dir="${CARGO_TARGET_DIR:-$PWD/crates/target}"
logs="$(mktemp -d "${TMPDIR:-/tmp}/qbz-qt-gate-XXXXXX")"
printf '[qt-gate] startup logs: %s\n' "$logs"
for profile in debug release; do
  args=()
  [[ "$profile" == release ]] && args=(--release)
  python3 scripts/qt-cargo.py build "${args[@]}" --manifest-path crates/Cargo.toml -p qbz-qt
  python3 scripts/qt-smoke.py "$target_dir/$profile/qbz" --log "$logs/$profile.log"
done
