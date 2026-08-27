#!/usr/bin/env bash
# scripts/cargo-test.sh — the EXACT local mirror of .github/workflows/test-crates.yml.
#
# Rule (owner, 2026-08-26): test-crates is a living regression gate. Every
# step it gains lands here in the same commit, and vice versa; the workflow
# calls THIS script so the two cannot diverge. Bounded: CARGO_BUILD_JOBS=2,
# --no-fail-fast, the `test` job's 20-minute limit.
#
# Default (job `test`, no Qt):
#   cargo test --workspace --exclude qbz-qt
#   --workspace today = the 42 members of crates/Cargo.toml minus qbz-qt: the
#   audio/player/cache/DSD/disc/rip core, qbz-app/core/models/theme/i18n,
#   the Qobuz client and the source seam, Plex/Jellyfin/Subsonic + media
#   cache, local library + catalog, integrations/reco/lyrics/radio/mixtape/
#   playlist-import/media-controls/cast, credentials/secrets/offline-cache,
#   the four qconnect-* crates, the HiFi wizard core, and qbzd. Nothing here
#   needs Qt; wayland-sys enters the graph only through qbz-qt.
#   The Slint crates (qbz, qbz-ui, qbz-dac-wizard, qbz-slint-common) are gone
#   from the workspace: no exclusions for them, and never bring them back.
#
# QT=1 (jobs `shader-gate` + `qt-gate`, needs a Qt >= 6.8 install):
#   1. the five static QML audits (scripts/qml-audits)
#   2. the shader bake gate with the qsb on PATH (CI: the pinned aqt qsb)
#   3. the Slint-free dep-graph gate for qbz-qt
#   4. cargo test -p qbz-qt (debug)
#   5. an offscreen boot of the debug binary: >= 10 log lines, zero QML
#      complaints, QbzCore initialized (the login screen; `home published`
#      needs a session CI does not have)
#
# Usage:
#   ./scripts/cargo-test.sh                 # job `test`
#   ./scripts/cargo-test.sh -- --lib        # extra cargo test args
#   QT=1 ./scripts/cargo-test.sh            # the Qt gates too
#   QBZ_QT_QML_DIR=/path/to/qt/qml QT=1 ./scripts/cargo-test.sh   # point the audits at a Qt
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
say() { printf '[cargo-test] %s\n' "$*"; }

say "job test: cargo test --workspace --exclude qbz-qt (jobs=$CARGO_BUILD_JOBS)"
cargo test \
  --manifest-path crates/Cargo.toml \
  --workspace \
  --exclude qbz-qt \
  --no-fail-fast \
  "$@"

say "gate: qbzd resolves no Slint crate"
hits=$(cargo tree --manifest-path crates/Cargo.toml -p qbzd -e normal \
       | grep -E '\b(slint|qbz-ui|qbz-slint-common|qbz-dac-wizard) v' || true)
[[ -z "$hits" ]] || { echo "qbzd graph resolves Slint crates:"; echo "$hits"; exit 1; }

[[ "${QT:-0}" == 1 ]] || { say "done (set QT=1 for the Qt gates)"; exit 0; }

say "qt gate 1/5: QML audits"
for a in qml_resolution_audit qml_singleton_xref qml_eager_tab_audit qml_module_registration_audit qml_icon_bake_audit; do
  python3 "scripts/qml-audits/$a.py" "$ROOT/crates/qbz-qt"
done

say "qt gate 2/5: shader bake gate"
scripts/qml-audits/shader_bake_gate.sh "${QSB:-$(command -v qsb || echo /usr/lib64/qt6/bin/qsb)}"

say "qt gate 3/5: qbz-qt resolves no Slint crate"
hits=$(cargo tree --manifest-path crates/Cargo.toml -p qbz-qt -e normal \
       | grep -E '\b(slint|qbz-ui|qbz-slint-common|qbz-dac-wizard) v' || true)
[[ -z "$hits" ]] || { echo "qbz-qt graph resolves Slint crates:"; echo "$hits"; exit 1; }

say "qt gate 4/5: cargo test -p qbz-qt (debug)"
cargo test --manifest-path crates/Cargo.toml -p qbz-qt --no-fail-fast

say "qt gate 5/5: offscreen boot of the debug binary"
cargo build --manifest-path crates/Cargo.toml -p qbz-qt
log="$(mktemp "${TMPDIR:-/tmp}/qbz-test-smoke-XXXXXX")"
# Isolated: its own XDG dirs (never the developer's config/session) and a
# PRIVATE session bus — the single-instance lock is a D-Bus well-known name,
# so a running QBZ would otherwise make this instance present-and-exit with
# one log line. CI has no other instance; the same command keeps both equal.
iso="$(mktemp -d "${TMPDIR:-/tmp}/qbz-test-xdg-XXXXXX")"
bus=(); command -v dbus-run-session >/dev/null && bus=(dbus-run-session --)
XDG_CONFIG_HOME="$iso/config" XDG_DATA_HOME="$iso/data" XDG_CACHE_HOME="$iso/cache" XDG_STATE_HOME="$iso/state" \
  QT_QPA_PLATFORM=offscreen RUST_LOG=info "${bus[@]}" timeout 75 ./crates/target/debug/qbz > "$log" 2>&1 || true
rm -rf "$iso"
lines=$(wc -l < "$log")
(( lines >= 10 )) || { cat "$log"; echo "smoke: the app did not start ($lines lines)"; exit 1; }
pat='is not a type|unavailable|ReferenceError|TypeError|Cannot read|Unable to assign|Cannot open|no such method|non-existent property|failed to load component|is not installed'
errs=$(grep -av 'propertyCache' "$log" | grep -aciE "$pat" || true)
if (( errs > 0 )); then grep -av 'propertyCache' "$log" | grep -aiE "$pat" | head -20; echo "smoke: $errs QML complaint(s) — $log"; exit 1; fi
grep -aq 'QbzCore initialized' "$log" || { tail -20 "$log"; echo "smoke: never reached QbzCore init — $log"; exit 1; }
say "smoke OK (0 QML complaints, core initialized)"
say "done"
