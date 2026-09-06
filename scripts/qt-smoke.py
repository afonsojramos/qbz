#!/usr/bin/env python3
"""Bounded, isolated startup gate; surviving until the deadline is required.

The process must both initialize QbzCore and remain alive for the observation
window. Never turn a SIGSEGV (or a single-instance early exit) into a green
gate just because the log contains a success marker.
"""

import argparse
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile

QML_ERROR = re.compile(
    r"is not a type|unavailable|ReferenceError|TypeError|Cannot read|"
    r"Unable to assign|Cannot open|no such method|non-existent property|"
    r"failed to load component|is not installed", re.IGNORECASE
)


def check_result(output, survived, returncode):
    if not survived:
        raise RuntimeError(f"process exited before the smoke deadline (status {returncode})")
    # A child may fault just as its dbus-run-session parent reaches the
    # timeout. Do not accept a reported crash as our intentional shutdown.
    if returncode not in (0, -signal.SIGTERM, 128 + signal.SIGTERM,
                          -signal.SIGKILL, 128 + signal.SIGKILL):
        raise RuntimeError(f"unexpected process exit status {returncode}")
    lines = output.splitlines()
    if len(lines) < 10:
        raise RuntimeError(f"app produced only {len(lines)} log lines")
    complaints = [line for line in lines if "propertyCache" not in line and QML_ERROR.search(line)]
    if complaints:
        raise RuntimeError("QML complaints:\n" + "\n".join(complaints[:20]))
    if "QbzCore initialized" not in output:
        raise RuntimeError("app never reached QbzCore initialization")


def smoke(binary, seconds, log):
    if seconds <= 0:
        raise RuntimeError("observation window must be positive")
    command = [str(Path(binary).resolve())]
    if sys.platform.startswith("linux"):
        bus = shutil.which("dbus-run-session")
        if not bus:
            raise RuntimeError("dbus-run-session is required for an isolated Linux smoke")
        command = [bus, "--", *command]
    with tempfile.TemporaryDirectory(prefix="qbz-smoke-") as directory:
        root = Path(directory)
        env = dict(os.environ)
        # Do not inherit the developer's renderer overrides or session bus.
        for name in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS",
                     "DBUS_SESSION_BUS_PID", "QBZ_RENDERER", "QT_QUICK_BACKEND",
                     "QSG_RHI_BACKEND"):
            env.pop(name, None)
        for name, folder in (("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                             ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state"),
                             ("XDG_RUNTIME_DIR", "run")):
            path = root / folder
            path.mkdir(mode=0o700)
            env[name] = str(path)
        env.update(QT_QPA_PLATFORM="offscreen", RUST_LOG="info")
        # dirs::data_dir ignores XDG on macOS/Windows. This helper deliberately
        # refuses there until native profile isolation is implemented.
        if not sys.platform.startswith("linux"):
            raise RuntimeError("isolated qt-smoke.py currently supports Linux only")
        # A deliberately crashing regression binary must not leave a huge
        # core dump in the checkout. This limit is private to the helper and
        # its children, not a system setting.
        import resource
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        with open(log, "wb") as output:
            process = subprocess.Popen(command, env=env, stdout=output,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            survived = False
            try:
                process.wait(timeout=seconds)
            except subprocess.TimeoutExpired:
                survived = process.poll() is None
            finally:
                # Kill/reap the entire *private* session, including D-Bus.
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=5)
        check_result(Path(log).read_text(errors="replace"), survived, process.returncode)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary")
    # Keep the existing CI's 75s budget: first-run bundle discovery is async
    # network work and can legitimately outlast 30s before core init logs.
    parser.add_argument("--seconds", type=float, default=75)
    parser.add_argument("--log", required=True)
    args = parser.parse_args()
    try:
        smoke(args.binary, args.seconds, args.log)
    except (OSError, RuntimeError, subprocess.SubprocessError):
        if Path(args.log).is_file():
            print(f"Last startup messages — {args.log}:", file=sys.stderr)
            print("\n".join(Path(args.log).read_text(errors="replace").splitlines()[-30:]),
                  file=sys.stderr)
        raise
    print(f"smoke OK: core initialized, process survived {args.seconds:g}s — {args.log}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"smoke FAILED: {error}", file=sys.stderr)
        sys.exit(1)
