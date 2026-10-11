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
# the sub-action classes: the base class is the one ~280 classes extend (find it with
# grep on `Text,Pin,PinToStreamEnd` in subattrs' output if its name changed)
uv run --with dnfile python -I subattrs.py $SB/Streamer.bot.exe _zKUoC4es1iOK9RftlTlQVoKyawi | sort -t'|' -k1 -n > subs.txt
python3 -I gen.py all.txt subs.txt > ~/Documents/repos/streamerz/src/types.rs
```

If the obfuscated names changed, put the new ones in gen.py and the command above. Sub-action classes
carry an attribute with their type code, display name and category; their JSON fields are their
property names in camelCase, or the `[JsonProperty]` name for obfuscated ones (Streamer.bot's own typos,
like `runImmedately`, are the real field names). Trigger classes carry no type code, so triggers have
no field table; their fields come from the docs and the real Streamer.bot.

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

`subattrs.py`:

```python
# Sub-action classes: type code, display name, categories (from their class attribute) and their
# readable property names (base first). Prints: code|name|categories|props
import sys, struct, dnfile
pe = dnfile.dnPE(sys.argv[1]); t = pe.net.mdtables; BASE = sys.argv[2]
tds = list(t.TypeDef)
props = {}
for pm in t.PropertyMap:
    props[pm.Parent.row_index] = [str(p.row.Name) for p in pm.PropertyList]
def base_idx(td):
    e = td.Extends
    return e.row_index if e and e.table and e.table.name == 'TypeDef' else None
def cint(b, i):
    x = b[i]
    if x < 0x80: return x, i+1
    if x < 0xc0: return ((x & 0x3f) << 8) | b[i+1], i+2
    return ((x & 0x1f) << 24) | (b[i+1] << 16) | (b[i+2] << 8) | b[i+3], i+4
def sstr(b, i):
    if b[i] == 0xff: return None, i+1
    n, i = cint(b, i); return b[i:i+n].decode('utf8', 'replace'), i+n
def val(b, i, ty):
    if ty == 0x0e or ty == 0x50: return sstr(b, i)
    if ty == 0x02: return bool(b[i]), i+1
    if ty == 0x08 or ty == 0x09: return struct.unpack_from('<i', b, i)[0], i+4
    if isinstance(ty, tuple):
        et = ty[1]
        n = struct.unpack_from('<i', b, i)[0]; i += 4
        out = []
        for _ in range(max(n, 0)):
            v, i = val(b, i, et); out.append(v)
        return out, i
    raise ValueError(hex(ty))
def named(b):
    i = 2  # prolog; this attribute's ctor takes no fixed args
    n = struct.unpack_from('<H', b, i)[0]; i += 2
    out = {}
    for _ in range(n):
        i += 1; ty = b[i]; i += 1
        if ty == 0x55:
            _, i = sstr(b, i); ty = 0x08
        elif ty == 0x1d:
            et = b[i]; i += 1
            if et == 0x55:
                _, i = sstr(b, i); et = 0x08
            ty = (0x1d, et)
        name, i = sstr(b, i)
        v, i = val(b, i, ty); out[name] = v
    return out
jname, ignored = {}, set()
for ca in t.CustomAttribute:
    p = ca.Parent
    if not (p and p.table and p.table.name == 'Property'):
        continue
    typ = ca.Type.row; cls = getattr(typ, 'Class', None)
    an = str(getattr(cls.row, 'TypeName', '')) if cls and cls.row else ''
    if an == 'JsonIgnoreAttribute':
        ignored.add(p.row_index)
    elif an == 'JsonPropertyAttribute' and ca.Value:
        b = bytes(ca.Value.value)
        if len(b) > 3 and b[2] != 0xff:
            jname[p.row_index] = sstr(b, 2)[0]
pidx = {}
for pm in t.PropertyMap:
    pidx[pm.Parent.row_index] = [(p.row_index, str(p.row.Name)) for p in pm.PropertyList]
def jprops(c):
    out = []
    for r, n in pidx.get(c, []):
        if r in ignored: continue
        if r in jname: out.append(jname[r])
        elif not n.startswith('_'): out.append(n[0].lower() + n[1:])
        else: out.append('?' + n)
    return out
attrs = {}
for ca in t.CustomAttribute:
    p = ca.Parent
    if p and p.table and p.table.name == 'TypeDef' and ca.Value:
        b = bytes(ca.Value.value)
        if b'Type' in b and b'Name' in b:
            try: attrs[p.row_index] = named(b)
            except Exception as e: print('ERR', p.row_index, e, b.hex()[:80], file=sys.stderr)
for i, td in enumerate(tds, 1):
    chain, j = [], i
    while j:
        chain.append(j); j = base_idx(tds[j-1])
    if BASE not in [str(tds[c-1].TypeName) for c in chain[1:]] or i not in attrs:
        continue
    a = attrs[i]
    own = [p for c in reversed(chain[:-1]) for p in jprops(c)]
    basep = jprops(chain[-1])
    print(f"{a.get('Type')}|{a.get('Name')}|{'/'.join(x for x in (a.get('Categories') or a.get(next((k for k in a if isinstance(a[k], list)), ''), []) or []) if x)}|{','.join(own)}|{','.join(basep)}")
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

## The real Streamer.bot as a test oracle

Behaviour the docs don't spell out (field values, formatting, flow) is checked against the real
Streamer.bot (`~/Games/streamerbot`, under Wine through Lutris): test actions are written into its
`actions.json`, it's restarted, each is run with `DoAction` over its WebSocket, and its log shows what
their Log sub-actions printed. Back up its `data/` first and put it back after; nothing from it goes in
the repo.

```
cp -a ~/Games/streamerbot/data /tmp/sb-data-backup
uv run --with websockets python -I oracle.py cases.json
/tmp/sbx/sb.sh stop; rm -rf ~/Games/streamerbot/data; cp -a /tmp/sb-data-backup ~/Games/streamerbot/data
```

`cases.json` is a list of `{"name", "subActions", "args", "triggers", "queue": "Q", "run": false, "wait"}`;
`"@name"` in a sub-action is the id of the case called `name` (and `"@Q"` a test queue). Found this way:
`%x:format%` uses the invariant culture in Streamer.bot (streamerz uses the OS locale, by choice);
`$math()$` prints 15 significant digits and NaN on errors; Contains ignores case; While's operators are
its own (0 equals, 1 not, 2 less, 3 greater); Set Global's source is 0 value, 1 add, 2 subtract,
3 remove; queue pause state 0 pauses, blocking state 0 blocks; an inline Run Action shares the caller's
arguments and its Break stops the caller too; global variable triggers' `persisted` is 0 either,
1 yes, 2 no (a bool there crashes Streamer.bot on load).

`oracle.py`:

```python
# Run test actions in the real Streamer.bot and print what their Log sub-actions wrote.
# usage: oracle.py cases.json   (a list of {"name", "subActions": [...], "args": {...}, "wait": seconds})
# Each case becomes an action "[SZT] name" (with its own sub-actions); the user's actions are kept.
import json, sys, uuid, time, subprocess, os, glob, asyncio, base64, hashlib, websockets
SB = os.path.expanduser('~/Games/streamerbot')
cases = json.load(open(sys.argv[1]))
def sid(): return str(uuid.uuid4())
def fill(subs, parent=None):
    for i, s in enumerate(subs):
        s.setdefault('id', sid()); s.setdefault('weight', 0.0); s.setdefault('parentId', parent)
        s.setdefault('enabled', True); s['index'] = i
        # base fields last, as Streamer.bot writes them
        for k in ('id', 'weight', 'type', 'parentId', 'enabled', 'index'):
            s[k] = s.pop(k)
        if 'subActions' in s: fill(s['subActions'], s['id'])
def stop():
    subprocess.run(['pkill', '-f', 'Streamer.bot.exe'])
    for _ in range(30):
        if subprocess.run(['pgrep', '-f', 'Streamer.bot.exe'], capture_output=True).returncode: return
        time.sleep(1)
    subprocess.run(['pkill', '-9', '-f', 'Streamer.bot.exe']); time.sleep(2)
def start():
    subprocess.Popen(['setsid', 'lutris', 'lutris:rungameid/2'], env=dict(os.environ, LUTRIS_SKIP_INIT='1'),
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(90):
        time.sleep(2)
        if b':8080 ' in subprocess.run(['ss', '-ltn'], capture_output=True).stdout: time.sleep(4); return
    sys.exit('Streamer.bot did not start')
subprocess.run(['/tmp/sbx/sb.sh', 'stop'])
path = f'{SB}/data/actions.json'
data = json.load(open(path, encoding='utf-8-sig'))
data['actions'] = [a for a in data['actions'] if not a['name'].startswith('[SZT]')]
names = []
ids = {c['name']: sid() for c in cases}
ids['Q'] = sid()
data['queues'] = [q for q in data['queues'] if not q.get('name','').startswith('szt')] + [{'id': ids['Q'], 'name': 'sztQ', 'blocking': True}]
def refs(o):
    if isinstance(o, dict): return {k: refs(v) for k, v in o.items()}
    if isinstance(o, list): return [refs(v) for v in o]
    if isinstance(o, str) and o.startswith('@') and o[1:] in ids: return ids[o[1:]]
    return o
for c in cases:
    c['subActions'] = refs(c['subActions'])
    fill(c['subActions'])
    name = '[SZT] ' + c['name']; names.append(name)
    data['actions'].append({"id": ids[c['name']], "queue": ids[c["queue"]] if c.get("queue") else "00000000-0000-0000-0000-000000000000", "enabled": True,
        "excludeFromHistory": False, "excludeFromPending": False, "name": name, "group": "SZT", "alwaysRun": False,
        "randomAction": c.get('random', False), "concurrent": c.get('concurrent', False), "triggers": c.get('triggers', []),
        "subActions": c['subActions'], "collapsedGroups": []})
open(path, 'w', encoding='utf-8-sig').write(json.dumps(data, ensure_ascii=False, separators=(',', ':')))
subprocess.run(['/tmp/sbx/sb.sh', 'start'])
log = max(glob.glob(f'{SB}/logs/log_*.log'), key=os.path.getmtime)
pos = os.path.getsize(log)
def pw():
    return json.load(open(f'{SB}/data/settings.json', encoding='utf-8-sig'))['websockets']['authPassword']
async def run():
    async with websockets.connect('ws://127.0.0.1:8080/') as ws:
        a = json.loads(await ws.recv()).get('authentication')
        if a:
            h = lambda x: base64.b64encode(hashlib.sha256(x.encode()).digest()).decode()
            await ws.send(json.dumps({"request": "Authenticate", "id": "a", "authentication": h(h(pw() + a['salt']) + a['challenge'])})); await ws.recv()
        for c, name in zip(cases, names):
            if c.get('run') is False: continue
            await ws.send(json.dumps({"request": "DoAction", "id": name, "action": {"name": name}, "args": c.get('args', {})}))
            await ws.recv()
            await asyncio.sleep(c.get('wait', 0.7))
        for r in cases[-1:] and json.loads(json.dumps(sys.argv[2:])):
            await ws.send(r); print('>>', (await ws.recv())[:2000])
asyncio.run(run())
time.sleep(1)
with open(log, encoding='utf-8', errors='replace') as f:
    f.seek(pos); out = f.read()
for line in out.splitlines():
    if 'SZT' in line or 'ERR' in line or 'WRN' in line: print(line[:400])
```

`sb.sh` (don't run `pkill -f` with these patterns from a shell whose own command line contains them):

```bash
#!/bin/bash
# Stop or start the real Streamer.bot (Lutris/Wine).
if [ "$1" = stop ]; then
  pkill -f 'Streamer.bot.exe'; pkill -f 'lutris-wrapper Streamer.bot'
  for i in $(seq 1 30); do pgrep -f 'Streamer.bot.exe' >/dev/null || break; sleep 1; done
  pkill -9 -f 'Streamer.bot.exe'
  pkill -9 -x winedevice.exe; WINEPREFIX=$HOME/Games/streamerbot/prefix/pfx wineserver -k 2>/dev/null
  sleep 2
  for i in $(seq 1 30); do pgrep -f 'lutris' >/dev/null || break; sleep 1; done
  sleep 2
else
  (env -u VIRTUAL_ENV -u PYTHONHOME -u PYTHONPATH PATH=/usr/local/bin:/usr/bin:/bin LUTRIS_SKIP_INIT=1 setsid lutris lutris:rungameid/2 >/tmp/sb-launch.log 2>&1 &)
  for i in $(seq 1 90); do sleep 2; ss -ltn | grep -q ':8080 ' && break; done
  sleep 4
fi
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
