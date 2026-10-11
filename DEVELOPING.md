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

## Streamer.bot's type codes (src/types.rs)

Sub-action and trigger `type` codes come from Streamer.bot's own enums, read from its .NET metadata
(interoperability only; nothing else is taken). Streamer.bot.exe is obfuscated, but enum member names
aren't: the sub-action enum is the one with `Nothing=0 PlaySound=1 ...` (402 members in 1.0.x), and the
triggers are `Streamer.bot.Common.Events.EventType` in Streamer.bot.Common.dll. Regenerate after a
Streamer.bot update, from a throwaway folder outside the repo (needs uv):

```
mkdir -p /tmp/sbx && cd /tmp/sbx    # put enums.py and gen.py (below) here
SB=~/Games/streamerbot
uv run --with dnfile python -I enums.py $SB/Streamer.bot.exe $SB/Streamer.bot.Common.dll > all.txt
grep -E '\|402\||EventType' all.txt | cut -c1-120   # find the sub-action enum's name
python3 -I gen.py all.txt > ~/Documents/repos/streamerz/src/types.rs
```

If the obfuscated sub-action enum's name changed, put the new one in gen.py.

`enums.py`:

```python
# Lists every enum in the given .NET assemblies: "Namespace.Type: Name=value ...".
import sys, struct, dnfile
for path in sys.argv[1:]:
    pe = dnfile.dnPE(path)
    t = pe.net.mdtables
    consts = {}
    for c in t.Constant or []:
        p = c.Parent.row_index if c.Parent else None
        if c.Parent and c.Parent.table and c.Parent.table.name == "Field":
            consts[c.Parent.row_index] = c.Value
    defs = list(t.TypeDef or [])
    nfields = len(t.Field or [])
    for i, td in enumerate(defs):
        ext = td.Extends
        name = getattr(ext.row, "TypeName", None) if ext and ext.row else None
        if name != "Enum":
            continue
        start = td.FieldList[0].row_index if td.FieldList else None
        if start is None:
            continue
        vals = []
        for f in td.FieldList:
            fi = f.row_index
            fr = f.row
            if fr.Name == "value__":
                continue
            v = consts.get(fi)
            v = getattr(v, "value", v)
            if isinstance(v, (bytes, bytearray)):
                v = int.from_bytes(v, "little", signed=True)
            vals.append(f"{fr.Name}={v}")
        print(f"{path.split('/')[-1]}|{td.TypeNamespace}.{td.TypeName}|{len(vals)}|" + " ".join(vals))
```

`gen.py`:

```python
# all.txt (from enums.py) -> src/types.rs
import sys
rows = {l.split('|')[1]: l.rstrip('\n').split('|')[3] for l in open(sys.argv[1])}
def table(name, enum, doc):
    kv = sorted(((int(v), k) for k, v in (x.split('=') for x in rows[enum].split())))
    out = [f"/// {doc}", f"pub const {name}: &[(u32, &str)] = &["]
    out += [f'    ({v}, "{k}"),' for v, k in kv]
    return "\n".join(out + ["];", ""])
print("""//! Streamer.bot's type codes and their names, generated from Streamer.bot.exe's metadata (see
//! DEVELOPING.md). Don't edit by hand.
#![allow(dead_code)] // ponytail: the engine (phase 3) and the editor (phase 5) use it.

""" + table("SUB_ACTIONS", "._HCQLjN8if3KpK6kJJBaaDLfHpSF", "Sub-action `type` codes (`subActions[].type`).")
 + "\n" + table("TRIGGERS", "Streamer.bot.Common.Events.EventType", "Trigger `type` codes (`triggers[].type`).") + """
/// The name of a code in one of the tables above.
pub fn name(table: &[(u32, &'static str)], code: u32) -> Option<&'static str> {
    table.binary_search_by_key(&code, |&(c, _)| c).ok().map(|i| table[i].1)
}
""", end="")
```

## Streamer.bot's data (src/model.rs)

`actions.json` (v24) is parsed into typed structs in Streamer.bot's field order; sub-actions and
triggers stay JSON objects (their fields depend on their type). Import strings are
`base64("SBAE" + gzip(JSON))`, the JSON indented the Newtonsoft way (2 spaces, `\r\n`); exports of
older versions have another shape (v3/v10 keep sub-actions under `actions`, with `actionGroups`), so
they stay plain JSON. Both are written back byte-for-byte.

The committed fixtures in `tests/data` are hand-made. To check against the real thing (not committed:
the docs have no license, and `actions.json` is user data):

```
mkdir -p /tmp/sbsamples
git clone --depth 1 https://github.com/Streamerbot/docs /tmp/sbdocs
grep -rhoE "U0JBRR[A-Za-z0-9+/=]{40,}" /tmp/sbdocs > /tmp/sbsamples/exports.txt
cp ~/Games/streamerbot/data/actions.json /tmp/sbsamples/
STREAMERZ_SAMPLES=/tmp/sbsamples cargo test real_samples
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
