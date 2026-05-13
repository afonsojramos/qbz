#!/usr/bin/env bash
# Force QBZ to run on the integrated GPU (Intel/AMD), bypassing the
# discrete NVIDIA GPU. Unlike dev-cpu-mode.sh (which forces pure
# software rendering), this keeps WebKit GPU compositing enabled so
# we can observe how QBZ behaves with modest-but-real HW accel.
#
# Why this matters: the current `hardware_accel_enabled` detector is
# binary (on / off). A user with an iGPU shouldn't be punished with
# the lite path designed for true software rendering — but right now
# the only way to test that intermediate tier is to actually constrain
# the runtime to the iGPU. This script is the manual harness for that.
#
# Layers forced to iGPU / Mesa:
#   __EGL_VENDOR_LIBRARY_FILENAMES  — pin EGL to Mesa only, skipping
#                                     /usr/share/glvnd/egl_vendor.d/10_nvidia.json.
#                                     This is the critical knob —
#                                     WebKitGTK prefers EGL and the
#                                     NVIDIA EGL ICD wins by priority
#                                     unless we point it elsewhere.
#   __GLX_VENDOR_LIBRARY_NAME=mesa  — same idea for GLX (mostly belt
#                                     and braces; default already
#                                     resolves to Mesa+Intel on this box).
#   DRI_PRIME=0                     — explicit primary GPU selection;
#                                     no PRIME render offload.
#   __NV_PRIME_RENDER_OFFLOAD unset — make sure we are not asking
#                                     nvidia-prime to bring the dGPU
#                                     back via offload.
#   QBZ_HARDWARE_ACCEL unset        — let QBZ honor its persisted
#                                     graphics_settings (HW on by
#                                     default). We want the iGPU
#                                     active, not software rendering.
#   GSK_RENDERER unset              — let GTK4 pick its best renderer
#                                     under the constrained GL/EGL.
#
# Layers explicitly NOT set (the opposite of dev-cpu-mode.sh):
#   WEBKIT_DISABLE_COMPOSITING_MODE — leave at default so WebKit
#                                     keeps GPU compositing.
#   WEBKIT_DISABLE_DMABUF_RENDERER  — leave at default so DMA-BUF
#                                     texture sharing works on the
#                                     Intel/Mesa side.
#   LIBGL_ALWAYS_SOFTWARE           — leave unset so Mesa runs on the
#                                     real Intel HW, not llvmpipe.
#
# Verification after launch (in a separate shell):
#   nvidia-smi --query-compute-apps=pid,used_memory --format=csv
#     → QBZ pid must NOT appear in the list.
#   nvtop / intel_gpu_top
#     → Activity on Intel iGPU when QBZ paints; idle on NVIDIA.
#
# Usage:
#   bash ./scripts/dev-igpu-mode.sh           # kill stale + start tauri dev
#   bash ./scripts/dev-igpu-mode.sh --no-kill # skip process cleanup
#   bash ./scripts/dev-igpu-mode.sh --print   # print env and exit

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

KILL_STALE=true
PRINT_ONLY=false

for arg in "$@"; do
  case "$arg" in
    --no-kill)
      KILL_STALE=false
      ;;
    --print)
      PRINT_ONLY=true
      ;;
    *)
      echo "[qbz] Unknown option: ${arg}"
      echo "Usage: bash ./scripts/dev-igpu-mode.sh [--no-kill] [--print]"
      exit 1
      ;;
  esac
done

MESA_EGL_JSON="/usr/share/glvnd/egl_vendor.d/50_mesa.json"
if [[ ! -f "${MESA_EGL_JSON}" ]]; then
  echo "[qbz] dev-igpu-mode: ${MESA_EGL_JSON} not found — your distro packages Mesa EGL elsewhere."
  echo "[qbz] dev-igpu-mode: list the candidates with:  ls /usr/share/glvnd/egl_vendor.d/"
  exit 1
fi

export __EGL_VENDOR_LIBRARY_FILENAMES="${MESA_EGL_JSON}"
export __GLX_VENDOR_LIBRARY_NAME=mesa
export DRI_PRIME=0
unset __NV_PRIME_RENDER_OFFLOAD
unset QBZ_HARDWARE_ACCEL
unset GSK_RENDERER
unset WEBKIT_DISABLE_COMPOSITING_MODE
unset WEBKIT_DISABLE_DMABUF_RENDERER
unset LIBGL_ALWAYS_SOFTWARE

echo "[qbz] dev-igpu-mode: iGPU-only env applied"
echo "[qbz]   __EGL_VENDOR_LIBRARY_FILENAMES = ${__EGL_VENDOR_LIBRARY_FILENAMES}"
echo "[qbz]   __GLX_VENDOR_LIBRARY_NAME      = ${__GLX_VENDOR_LIBRARY_NAME}"
echo "[qbz]   DRI_PRIME                      = ${DRI_PRIME}"
echo "[qbz]   QBZ_HARDWARE_ACCEL             = (unset → DB default, HW on)"
echo "[qbz]   compositing / DMA-BUF / llvmpipe = NOT forced (real iGPU paint path)"

if [[ "${PRINT_ONLY}" == "true" ]]; then
  exit 0
fi

if [[ "${KILL_STALE}" == "true" ]]; then
  echo "[qbz] dev-igpu-mode: stopping stale dev processes..."
  pkill -f '[n]ode .*vite dev' || true
  pkill -f '[n]ode .*tauri dev' || true
  pkill -f '[c]argo run' || true
  pkill -f '[t]arget/debug/qbz' || true
  pkill -f '[t]arget/debug/qbz-nix' || true

  for port in 1420 1421; do
    pids="$(lsof -ti :"${port}" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -n "${pids}" ]]; then
      kill ${pids} 2>/dev/null || true
      sleep 0.5
      still_alive="$(lsof -ti :"${port}" -sTCP:LISTEN 2>/dev/null || true)"
      if [[ -n "${still_alive}" ]]; then
        kill -9 ${still_alive} 2>/dev/null || true
      fi
    fi
  done
fi

echo "[qbz] dev-igpu-mode: starting tauri dev in foreground..."
exec npm run tauri dev
