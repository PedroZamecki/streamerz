#!/usr/bin/env python3
"""Run local CI checks without publishing or contacting extension stores.

Use --release for Linux packaging and --windows for the MSVC cross-check.
Windows runtime, signing and GitHub artifact reuse still need their own report.
Requires actionlint and cargo-audit on PATH, as well as the build dependencies.
"""
import argparse
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true", help="Build and package the Linux release")
    parser.add_argument("--windows", action="store_true", help="Type-check Windows with cargo-xwin")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    python = sys.executable
    checks = [
        ["git", "diff", "--check"],
        ["cargo", "fmt", "--check"],
        ["cargo", "test", "--profile", "ci-test", "--locked"],
        ["cargo", "clippy", "--profile", "ci-test", "--all-targets", "--locked", "--", "-D", "warnings"],
        ["cargo", "test", "--profile", "ci-test", "--locked", "--", "--ignored", "--list"],
        [python, "-P", "dev/locale.py", "check"],
        ["node", "--test", "dev/tiktok-extension.test.mjs"],
        ["node", "dev/test-subathon-overlay.cjs"],
        [python, "-P", "-m", "unittest", "discover", "-s", "dev", "-p", "test_*.py"],
        [python, "-P", "dev/scan_secrets.py"],
        [python, "-P", "dev/scan_secrets.py", "--history"],
        ["cargo", "audit"],
        ["actionlint"],
        [python, "-P", "dev/package-extensions.py"],
    ]
    if args.windows:
        checks.append(["cargo", "xwin", "check", "--locked", "--target", "x86_64-pc-windows-msvc", "--target-dir", "target/xwin"])
    if args.release:
        checks += [
            ["cargo", "build", "--release", "--locked"],
            ["target/release/Streamerz", "--scenario", "tests/scenarios"],
            ["sh", "dev/appimage.sh", "dev"],
        ]
    failures = []
    for command in checks:
        label = " ".join(command)
        print(f"\nRunning: {label}", flush=True)
        try:
            passed = subprocess.run(command, cwd=root).returncode == 0
        except OSError as error:
            print(error, flush=True)
            passed = False
        if not passed:
            failures.append(label)
    for label in failures:
        print(f"FAILED: {label}")
    return bool(failures)


if __name__ == "__main__":
    sys.exit(main())
