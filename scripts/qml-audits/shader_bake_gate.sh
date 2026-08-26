#!/usr/bin/env bash
# shader_bake_gate.sh — prove every shader under crates/qbz-qt/qml/assets/shaders
# still BAKES with the pinned qsb, without touching the committed .qsb files.
#
# Why this exists (2026-08-26): build.rs re-bakes a shader only when its source
# is newer than its .qsb (mtime), so a shader edit whose bake FAILS is invisible
# on a box where the .qsb happens to be newer — and a fresh checkout (CI) may
# or may not re-bake depending on checkout timing. plasma.frag shipped exactly
# that way: uint literals that GLSL 120 cannot express, a stale committed .qsb,
# and a build that broke only in CI. This gate bakes every shader to a temp dir
# with the SAME target lists build.rs uses (read from build.rs so they cannot
# drift) and fails if any one of them does not bake.
#
# The release/test builds themselves run WITHOUT qsb on PATH so they ship the
# committed .qsb deterministically; this gate is where shader health is judged.
#
# Usage: scripts/qml-audits/shader_bake_gate.sh [path/to/qsb]
set -euo pipefail
cd "$(dirname "$0")/../.."
CRATE=crates/qbz-qt
QSB="${1:-${QSB:-$(command -v qsb || true)}}"
[[ -x "$QSB" ]] || { echo "shader_bake_gate: no qsb (pass a path or put it on PATH)"; exit 2; }
glsl_all="$(grep -oE 'const SHADER_GLSL: &str = "[^"]+"' $CRATE/build.rs | sed 's/.*= "//; s/"$//')"
glsl_noes="$(grep -oE 'const SHADER_GLSL_NO_ES100: &str = "[^"]+"' $CRATE/build.rs | sed 's/.*= "//; s/"$//')"
hlsl="$(grep -oE 'const SHADER_HLSL: &str = "[^"]+"' $CRATE/build.rs | sed 's/.*= "//; s/"$//')"
msl="$(grep -oE 'const SHADER_MSL: &str = "[^"]+"' $CRATE/build.rs | sed 's/.*= "//; s/"$//')"
marker="$(grep -oE 'const NO_ES100_MARKER: &str = "[^"]+"' $CRATE/build.rs | sed 's/.*= "//; s/"$//')"
[[ -n "$glsl_all" && -n "$glsl_noes" && -n "$hlsl" && -n "$msl" && -n "$marker" ]] || { echo "shader_bake_gate: could not read the target lists from $CRATE/build.rs"; exit 2; }
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
fail=0; n=0
for src in $CRATE/qml/assets/shaders/*.frag $CRATE/qml/assets/shaders/*.vert; do
  n=$((n+1))
  if grep -q "$marker" "$src"; then glsl="$glsl_noes"; else glsl="$glsl_all"; fi
  if ! "$QSB" --glsl "$glsl" --hlsl "$hlsl" --msl "$msl" -b -o "$tmp/$(basename "$src").qsb" "$src" > "$tmp/err" 2>&1; then
    echo "FAIL $src  [glsl: $glsl]"; sed 's/^/     /' "$tmp/err" | head -3; fail=$((fail+1))
  fi
done
echo "shader_bake_gate: $n shaders, $fail failed ($("$QSB" --version 2>/dev/null | head -1))"
exit $(( fail > 0 ))
