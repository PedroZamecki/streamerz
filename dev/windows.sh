#!/usr/bin/env bash
# Builds Streamerz.exe on Linux the way the Windows CI job does (x86_64-pc-windows-msvc, static MSVC
# runtime, release), with cargo-xwin (Microsoft's CRT and SDK, downloaded once to ~/.cache/cargo-xwin).
# Then, with an argument, runs it under Wine: `dev/windows.sh scenarios`, `smoke`, `live` (the OS paths,
# see dev/windows-live.py) or `tests` (the unit tests, built for Windows).
#
#   cargo install cargo-xwin --locked     # once; needs clang-cl, lld-link, llvm-rc
set -euo pipefail
cd "$(dirname "$0")/.."
export XWIN_ACCEPT_LICENSE=1
target=x86_64-pc-windows-msvc
dir=${STREAMERZ_WIN_TARGET:-target/xwin}
exe=$dir/$target/release/Streamerz.exe

cargo fetch --locked

# The smoke drives the app through the dev instrumentation, so it is built with it.
features=()
[ "${1:-}" = smoke ] && features=(--features dev-ui)
cargo xwin build --release --locked --bin Streamerz --target "$target" --target-dir "$dir" "${features[@]}"
echo "built $exe"

case "${1:-}" in
  tests)
    # The unit tests as a Windows exe, in a prefix of their own (they write to the registry).
    cargo xwin test --release --locked --no-run --target "$target" --target-dir "$dir"
    t=$(ls -t "$dir/$target"/release/deps/Streamerz-*.exe | head -1)
    mkdir -p /tmp/szwinetest
    WINEPREFIX=/tmp/szwinetest WINEDEBUG=-all wine "$t" --test-threads=4 ;;
  live)
    env -u WAYLAND_DISPLAY DISPLAY="${DISPLAY_SMOKE:-:99}" WINEPREFIX="${WINEPREFIX:-/tmp/szwine}" python3 -P dev/windows-live.py "$exe" ;;
  scenarios)
    # Wine prints its own noise on stderr; the scenario report is on stdout.
    WINEDEBUG=-all wine "$exe" --scenario tests/scenarios 2>/dev/null ;;
  smoke)
    env -u WAYLAND_DISPLAY DISPLAY="${DISPLAY_SMOKE:-:99}" WINEDEBUG=-all python3 -P dev/ui-smoke.py "$exe" --wine ;;
  "") ;;
  *) echo "unknown: $1 (scenarios, smoke, live or tests)"; exit 2 ;;
esac
