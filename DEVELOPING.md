# Developing streamerz

Rules for commits, branches and releases are in `CLAUDE.md` (also `AGENTS.md`); the plan is `PLAN.md`,
and what's covered is `COVERAGE.md`.

## Setup

You need Rust (https://rustup.rs) and Node (for commitlint in the git hooks). Then, once per clone:

```
git config core.hooksPath .githooks
```

## Build and run

The window (FLTK) builds from source: it needs CMake, a C++ compiler and the X11/Wayland/dbus
development packages listed in `.github/workflows/lint.yml`.

```
cargo build --release
./target/release/streamerz
```

Settings live in `streamerz.json` next to the binary (`target/release/streamerz.json` here). It's written
to a temp file and renamed over the old one, and the previous version is kept as `streamerz.json.bak`;
when the file is invalid, the `.bak` is loaded instead and the broken file is left alone.

## Checks (same as the hooks and CI)

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Windows-only code (tray, autostart) can be type-checked from Linux with the official rustc and the
Windows std (e.g. the `rustc` and both `rust-std` components from static.rust-lang.org installed to
`/tmp/rt`):

```
RUSTC=/tmp/rt/bin/rustc cargo check --target x86_64-pc-windows-msvc --features fltk/fltk-bundled --target-dir /tmp/wincheck
```

## Memory and CPU

Every PR measures the hidden window (start hidden, wait a few seconds):

```
pid=$(pgrep -x streamerz); grep VmRSS /proc/$pid/status
t1=$(awk '{print $14+$15}' /proc/$pid/stat); sleep 30; t2=$(awk '{print $14+$15}' /proc/$pid/stat); echo $((t2-t1))
```

The second number is CPU ticks (1/100 s) used in 30 s idle. The tray can be toggled without clicking:
`busctl --user call org.kde.StatusNotifierItem-$pid-1 /StatusNotifierItem org.kde.StatusNotifierItem Activate ii 0 0`.

## Icon

`icon_streamerz.png` is the source. After changing it:

```
magick icon_streamerz.png -define icon:auto-resize=64,48,32,16 icon_streamerz.ico
magick icon_streamerz.png -resize 64x64 -depth 8 rgba:src/icon.rgba
```
