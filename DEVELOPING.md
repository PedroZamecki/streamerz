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

### Fast builds and testing

Full release builds compile `wry`/`tao`/WebKitGTK (`242` crates, LTO, single codegen unit). For rapid
day-to-day iteration, `.cargo/config.toml` and `Cargo.toml` provide a `[profile.fast]` (`opt-level = 1`,
incremental, line-table debug info) and make `tiktok-web` (`streamerz-tiktok`) optional (`141` crates):

```
cargo fcheck                  # fast type-check (--no-default-features)
cargo fbuild                  # fast optimized dev binary -> target/fast/streamerz
cargo frun                    # build & run target/fast/streamerz
cargo ftest                   # run all tests without building wry/tao
cargo ftest twitch::tests     # run only matching unit tests
```

`target/fast/streamerz` automatically reuses `target/release/streamerz-tiktok` (or `target/debug/`) if
already built once. To share your existing release `streamerz.json` and `cache/` without copying files:

```
STREAMERZ_CONFIG=target/release/streamerz.json cargo frun
# or:
./target/fast/streamerz --config target/release/streamerz.json
```

Optional host tools for even faster linking and clean rebuilds on Linux:

```
RUSTFLAGS="-C link-arg=-fuse-ld=mold" cargo fbuild   # fast linker (mold or wild)
RUSTC_WRAPPER=sccache cargo fbuild                   # shared rustc artifact cache
```

Settings live in `streamerz.json` next to the binary (`target/release/streamerz.json` by default, or the
path given via `--config <path>` / `STREAMERZ_CONFIG`). It's written
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

Commands only run from chat, which the WebSocket can't inject: they were checked by posting test
messages in the broadcaster's Twitch chat (`SendMessage` with `internal: false`, the test command with
Ignore Internal off; cases take `"say": [...]`). Found: Start needs a space or the end after the alias;
matching ignores case and reports the alias as configured; rawInput is trimmed but keeps inner spaces;
input# splits on single spaces (two spaces give an empty word, no input gives an empty input0); Regex
mode's words come from the message without its leading `!`; URL encoding is WebUtility's (`+`, `%XX`
uppercase); Get Commands sorts the aliases and joins them with `, `; Clear Users From Group fires only
Cleared; WebSocket client triggers carry only `message`, custom server ones `sessionId`, `ip` and `data`
(cases take `ws_clients` and `ws_servers`, written into settings.json), and a client fires Closed on
every failed reconnect attempt; Fetch URL's JSON becomes `var.key`, `var.key.inner`, `var.list[0]` (null as empty, nothing set
on an HTTP error); Run a Program's lines are output0..N split on `\n` (keeping `\r`). The
broadcaster skips cooldowns and permissions, so those follow the docs. The cases also accept `commands`,
`groups` and `timers`, written into commands.json, users.dat and settings.json.

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
ids['G'] = sid()
ids['C'] = sid()
ids['C2'] = sid()
ids['C3'] = sid()
ids['C4'] = sid()
ids['T'] = sid()
ids['WC'] = sid()
ids['WS'] = sid()
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
        "randomAction": c.get('random', False), "concurrent": c.get('concurrent', False), "triggers": refs(c.get("triggers", [])),
        "subActions": c['subActions'], "collapsedGroups": []})
open(path, 'w', encoding='utf-8-sig').write(json.dumps(data, ensure_ascii=False, separators=(',', ':')))
for fname, key in (('users.dat', 'groups'), ('commands.json', 'commands')):
    extra = [refs(x) for c in cases for x in c.get(key, [])]
    if extra:
        fp = f'{SB}/data/{fname}'
        dd = json.load(open(fp, encoding='utf-8-sig'))
        dd[key] = [x for x in dd[key] if not x.get('name', '').startswith('szt')] + extra
        open(fp, 'w', encoding='utf-8-sig').write(json.dumps(dd, ensure_ascii=False, separators=(',', ':')))
wsc = [refs(x) for c in cases for x in c.get('ws_clients', [])]
wss = [refs(x) for c in cases for x in c.get('ws_servers', [])]
if wsc or wss:
    fp = f'{SB}/data/settings.json'
    dd = json.load(open(fp, encoding='utf-8-sig'))
    dd['websockets']['connections'] += wsc
    dd['websockets']['servers'] += wss
    open(fp, 'w', encoding='utf-8-sig').write(json.dumps(dd, ensure_ascii=False, separators=(',', ':')))
tim = [refs(x) for c in cases for x in c.get('timers', [])]
if tim:
    fp = f'{SB}/data/settings.json'
    dd = json.load(open(fp, encoding='utf-8-sig'))
    dd['timedActions']['timers'] = [x for x in dd['timedActions']['timers'] if not x.get('name','').startswith('szt')] + tim
    open(fp, 'w', encoding='utf-8-sig').write(json.dumps(dd, ensure_ascii=False, separators=(',', ':')))
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
            for m in c.get('say', []):
                await ws.send(json.dumps({"request": "SendMessage", "id": "say", "platform": "twitch", "bot": False, "internal": False, "message": m}))
                print('say', m, await ws.recv())
                await asyncio.sleep(2.5)
            if c.get('say'): continue
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

## Servers side by side

streamerz's servers are compared with the real Streamer.bot's: streamerz on 8090 (HTTP 7475, UDP 4243) with
the same password and a copy of the real `actions.json` in its own `streamerz.json` (both local, never
committed), the real one on 8080 (turn its HTTP and UDP servers on for the HTTP comparison, and put its
settings back after). `cmp.py` sends the same WebSocket requests to both, with and without
authentication, and reports differences in shape (keys, order, types) and value; HTTP answers are compared
byte for byte with curl. Differences by design: GetInfo/Hello's extra `streamerz` field, platform data
until the platforms are in, TestCredits (credits are a later phase), and the JSON parser's error message.

`cmp.py`:

```python
# Same requests to the real Streamer.bot (8080) and streamerz (8090); report shape differences
# (keys, key order, value types) and value differences for stable fields.
import asyncio, json, base64, hashlib, websockets
PW = json.load(open('/home/zamecki/Games/streamerbot/data/settings.json', encoding='utf-8-sig'))['websockets']['authPassword']
REQS = [
 {"request":"GetInfo"}, {"request":"GetEvents"}, {"request":"GetActions"}, {"request":"GetBroadcaster"},
 {"request":"GetCredits"}, {"request":"ClearCredits"}, {"request":"GetActiveViewers"}, {"request":"GetCodeTriggers"},
 {"request":"ExecuteCodeTrigger","triggerName":"x","args":{}}, {"request":"GetCommands"},
 {"request":"GetGlobals","persisted":True}, {"request":"GetGlobals","persisted":False},
 {"request":"GetGlobal","variable":"nope","persisted":True}, {"request":"TwitchGetUserGlobals","variable":"x","persisted":True},
 {"request":"TwitchGetUserGlobal","userId":"1","persisted":True}, {"request":"YouTubeGetEmotes"},
 {"request":"GetUserPronouns","platform":"twitch","userLogin":"nobody_xyz_123"},
 {"request":"Bogus"}, {"request":"DoAction","action":{"name":"nope"}},
 {"request":"Subscribe","events":{"General":["Custom"]}}, {"request":"UnSubscribe","events":{"General":["Custom"]}},
]
def shape(v):
    if isinstance(v, dict): return {k: shape(x) for k, x in v.items()}
    if isinstance(v, list): return [shape(v[0])] if v else []
    return type(v).__name__
async def talk(port, auth):
    out = []
    async with websockets.connect(f'ws://127.0.0.1:{port}/') as ws:
        hello = json.loads(await ws.recv()); out.append(('Hello', hello))
        if auth:
            a = hello['authentication']; h = lambda x: base64.b64encode(hashlib.sha256(x.encode()).digest()).decode()
            await ws.send(json.dumps({"request":"Authenticate","id":"auth","authentication":h(h(PW+a['salt'])+a['challenge'])}))
            out.append(('Authenticate', json.loads(await ws.recv())))
        for i, r in enumerate(REQS):
            r = dict(r, id=str(i)); await ws.send(json.dumps(r))
            out.append((r['request'], json.loads(await ws.recv())))
        await ws.send('notjson'); out.append(('malformed', await ws.recv()))
    return out
async def main():
    for auth in (True, False):
        a, b = await talk(8080, auth), await talk(8090, auth)
        print('=== auth' if auth else '=== no auth')
        for (n, x), (_, y) in zip(a, b):
            if isinstance(x, str):
                print('  ', n, 'SAME' if x == y else f'DIFF {x!r} vs {y!r}'); continue
            sx, sy = shape(x), shape(y)
            if json.dumps(sx) != json.dumps(sy):
                print('  ', n, 'SHAPE', json.dumps(sx)[:300], '\n      vs', json.dumps(sy)[:300])
            else:
                vals = {k: (x[k], y[k]) for k in x if x[k] != y[k] and k not in ('id',)}
                print('  ', n, 'same shape', ('values differ: ' + json.dumps(vals)[:200]) if vals else '')
asyncio.run(main())
```

## Chat without live channels (dev mode)

Started with `STREAMERZ_DEV=1`, streamerz sends no chat: every message an action would send (Twitch, YouTube,
Kick, TikTok send/reply) is kept instead, and the WebSocket server answers two extra requests (unknown
otherwise, so a release never has them unless the variable is set):

- `{"request": "InjectChat", "id": "1", "platform": "kick", "user": "bo", "role": "viewer", "text": "!hi"}`
  runs `text` through the commands as a chat message from that platform (`twitch`, `youtube`, `kick`,
  `tiktok`; role `broadcaster`, `moderator`, `vip`, `subscriber` or `viewer`), answering `{"ran": n}`.
- `{"request": "GetChatOut", "id": "2"}` returns and clears what would have been sent:
  `{"messages": [{"platform": "kick", "text": "hi bo", "bot": false}, ...]}` (the last 500).

Node has a WebSocket client built in, so no package is needed:

```js
// node ws.mjs '[{"request":"InjectChat",...},{"request":"GetChatOut","id":"2"}]'
const reqs = JSON.parse(process.argv[2]), ws = new WebSocket("ws://127.0.0.1:8090/");
ws.onmessage = (m) => { const v = JSON.parse(m.data); if (v.request !== "Hello") console.log(m.data);
  reqs.length ? setTimeout(() => ws.send(JSON.stringify(reqs.shift())), 200) : ws.close(); };
```

## TikTok live (src/webcast.rs)

Reading needs a channel that is live right now. `api-live/user/room?...&uniqueId=NAME` (the URL in
`webcast::room_id`) says so: `statusCode` 19881007 is no such user, `data.liveRoom.status` 2 is live, 4
offline. It answers 429 to a few quick lookups in a row (less with a `ttwid` cookie), so check names one
every few seconds. Then run streamerz with `"tiktok": {"channel": NAME, "autoConnect": true}` and subscribe
to the TikTok events on its WebSocket. Chat on a quiet live can take minutes; joins and likes come at once.
An emote-only comment has no text (its emotes are in field 13).

Sending (src/tiktok_web.rs) types into the live page's chat. To test it without posting anywhere, point
the window at a local copy of that page: `STREAMERZ_TIKTOK_URL=http://127.0.0.1:8765` loads
`{url}/@{channel}/live`, so serve files `@NAME/live` holding a `contenteditable` element with
`data-e2e="room-chat-input-field"` (add `disabled` and a `placeholder` for a room that refuses comments)
and a `data-e2e="room-chat-send-btn"`, whose Enter or click handler records the text (e.g. fetches
`/sent?text`). Run `streamerz-tiktok send <profile dir> NAME` and write JSON strings to its stdin; each
gets `{"ok":true}` or `{"error":...}`. `STREAMERZ_TIKTOK_SHOW=1` shows the window.

## OBS sub-actions against a real OBS

A throwaway OBS on the hidden display, so the user's own profile and devices aren't touched: its own config
(`XDG_CONFIG_HOME=/tmp/obscfg`), obs-websocket on 4456 with a password, the scene collection's
`DesktopAudioDevice1`/`AuxAudioDevice1` removed (no microphone or desktop capture) and recordings in
`/tmp/szo/rec` (`[SimpleOutput] FilePath=`, `RecRB=true` for the replay buffer). Then `run.py` builds one
streamerz action per case, runs each with `DoAction` and checks OBS's own state with `obs.mjs`, an
independent obs-websocket client. The scenes it expects (SZT A/B, SZT Text, Color, Image, Browser, Audio,
Media, the filters) are created with `obs.mjs` `CreateScene`/`CreateInput`/`CreateSourceFilter` first.

```
mkdir -p /tmp/obscfg/obs-studio/plugin_config/obs-websocket   # + config.json: server_port 4456, auth
DISPLAY=:99 QT_QPA_PLATFORM=xcb XDG_CONFIG_HOME=/tmp/obscfg setsid obs --disable-shutdown-check --multi &
python3 /tmp/szo/run.py      # PASS/FAIL per case
```

`obs.mjs`:

```js
// Minimal obs-websocket v5 client (the oracle): node obs.mjs '[["Request", {data}], ...]'
import crypto from "node:crypto";
const reqs = JSON.parse(process.argv[2]); const pass = "szt-test-pass";
const ws = new WebSocket("ws://127.0.0.1:4456"); let i = 0; const out = [];
const sha = (s) => crypto.createHash("sha256").update(s).digest("base64");
ws.onmessage = (m) => {
  const v = JSON.parse(m.data);
  if (v.op === 0) { const a = v.d.authentication; const auth = sha(sha(pass + a.salt) + a.challenge);
    ws.send(JSON.stringify({ op: 1, d: { rpcVersion: 1, authentication: auth } })); }
  else if (v.op === 2) next();
  else if (v.op === 7) { out.push({ type: v.d.requestType, ok: v.d.requestStatus.result, comment: v.d.requestStatus.comment, data: v.d.responseData }); next(); }
};
function next() { if (i >= reqs.length) { console.log(JSON.stringify(out)); ws.close(); return; }
  const [t, d] = reqs[i++]; ws.send(JSON.stringify({ op: 6, d: { requestType: t, requestId: String(i), requestData: d || {} } })); }
```

`ws.mjs`:

```js
// node ws.mjs '[{"request":...}, ...]' : send each (waiting `wait` ms first), print answers.
const reqs = JSON.parse(process.argv[2]); const ws = new WebSocket("ws://127.0.0.1:8090/");
ws.onmessage = (m) => { const v = JSON.parse(m.data); if (v.request !== "Hello") console.log(m.data);
  if (v.request === "Hello" || v.id) next(); };
function next() { if (!reqs.length) { ws.close(); return; } const r = reqs.shift();
  setTimeout(() => ws.send(JSON.stringify(r)), r.wait || 0); }
```

`run.py`:

```python
import json, subprocess, time, os, sys, uuid
D='/tmp/szo'
def oracle(reqs):
    return json.loads(subprocess.check_output(['node', f'{D}/obs.mjs', json.dumps(reqs)], timeout=20))
def ws(reqs):
    return subprocess.check_output(['node', f'{D}/ws.mjs', json.dumps(reqs)], timeout=60).decode()
def item(scene, src):
    for i in oracle([["GetSceneItemList", {"sceneName": scene}]])[0]['data']['sceneItems']:
        if i['sourceName']==src: return i['sceneItemId']
def log(m): return {"type": 1020, "logLevel": 1, "message": m}
A, T = "SZT A", "SZT Text"
cases = [  # (name, sub-actions, check(lines) -> bool or str)
 ("set active scene", [{"type":34,"sceneName":A}], lambda L: oracle([["GetCurrentProgramScene"]])[0]['data']['sceneName']==A),
 ("get current scene", [{"type":43}, log("T scene %currentScene%")], lambda L: "T scene SZT A" in L),
 ("hide source", [{"type":30,"sceneName":A,"sourceName":"SZT Color","state":1}], lambda L: oracle([["GetSceneItemEnabled",{"sceneName":A,"sceneItemId":item(A,"SZT Color")}]])[0]['data']['sceneItemEnabled'] is False),
 ("toggle source", [{"type":30,"sceneName":A,"sourceName":"SZT Color","state":2}], lambda L: oracle([["GetSceneItemEnabled",{"sceneName":A,"sceneItemId":item(A,"SZT Color")}]])[0]['data']['sceneItemEnabled'] is True),
 ("filter off", [{"type":35,"sceneName":A,"sourceName":T,"filterName":"SZT Filter","state":1}], lambda L: oracle([["GetSourceFilter",{"sourceName":T,"filterName":"SZT Filter"}]])[0]['data']['filterEnabled'] is False),
 ("scene filter off", [{"type":47,"sceneName":A,"filterName":"SZT SceneFilter","state":1}], lambda L: oracle([["GetSourceFilter",{"sourceName":A,"filterName":"SZT SceneFilter"}]])[0]['data']['filterEnabled'] is False),
 ("flip horizontal", [{"type":36,"sceneName":A,"sourceName":"SZT Image","flipMode":0}], lambda L: oracle([["GetSceneItemTransform",{"sceneName":A,"sceneItemId":item(A,"SZT Image")}]])[0]['data']['sceneItemTransform']['scaleX'] * FLIP0 < 0),
 ("rotate 45", [{"type":37,"sceneName":A,"sourceName":"SZT Image","degrees":45,"additive":False}], lambda L: abs(oracle([["GetSceneItemTransform",{"sceneName":A,"sceneItemId":item(A,"SZT Image")}]])[0]['data']['sceneItemTransform']['rotation']-45)<0.01),
 ("rotate +10", [{"type":37,"sceneName":A,"sourceName":"SZT Image","degrees":10,"additive":True}], lambda L: abs(oracle([["GetSceneItemTransform",{"sceneName":A,"sceneItemId":item(A,"SZT Image")}]])[0]['data']['sceneItemTransform']['rotation']-55)<0.01),
 ("browser url", [{"type":38,"sceneName":A,"sourceName":"SZT Browser","url":"https://example.com/%user%"}], lambda L: oracle([["GetInputSettings",{"inputName":"SZT Browser"}]])[0]['data']['inputSettings'].get('url')=="https://example.com/tester"),
 ("set text", [{"type":39,"sceneName":A,"sourceName":T,"text":"hello %user%"}], lambda L: oracle([["GetInputSettings",{"inputName":T}]])[0]['data']['inputSettings'].get('text')=="hello tester"),
 ("hide source filters", [{"type":41,"sceneName":A,"sourceName":T}], lambda L: all(not f['filterEnabled'] for f in oracle([["GetSourceFilterList",{"sourceName":T}]])[0]['data']['filters'])),
 ("random filter on", [{"type":40,"sceneName":A,"sourceName":T,"state":0}], lambda L: sum(f['filterEnabled'] for f in oracle([["GetSourceFilterList",{"sourceName":T}]])[0]['data']['filters'])==1),
 ("mute", [{"type":42,"sceneName":A,"sourceName":"SZT Audio","state":0}], lambda L: oracle([["GetInputMute",{"inputName":"SZT Audio"}]])[0]['data']['inputMuted'] is True),
 ("unmute", [{"type":42,"sceneName":A,"sourceName":"SZT Audio","state":1}], lambda L: oracle([["GetInputMute",{"inputName":"SZT Audio"}]])[0]['data']['inputMuted'] is False),
 ("audio track 2 off", [{"type":44,"sceneName":A,"sourceName":"SZT Audio","track":1,"state":1}], lambda L: oracle([["GetInputAudioTracks",{"inputName":"SZT Audio"}]])[0]['data']['inputAudioTracks']['2'] is False),
 ("raw GetVersion", [{"type":46,"raw":json.dumps({"requestType":"GetVersion","requestData":{}}),"resultsToArgs":True,"prefix":"v"}, log("T raw %v.obsVersion%")], lambda L: "T raw 32.2.2" in L),
 ("screenshot", [{"type":48,"sceneName":A,"sourceName":"","path":f"{D}/shot.png","quality":-1}], lambda L: os.path.exists(f"{D}/shot.png")),
 ("image file", [{"type":323,"sceneName":A,"sourceName":"SZT Image","fileName":f"{D}/b.png"}], lambda L: oracle([["GetInputSettings",{"inputName":"SZT Image"}]])[0]['data']['inputSettings'].get('file')==f"{D}/b.png"),
 ("item props", [{"type":324,"sceneName":A,"sourceName":"SZT Image"}, log("T props %props.rotation% %props.visible% %props.name%")], lambda L: any(l.startswith("T props 55") for l in L)),
 ("color", [{"type":327,"sceneName":A,"sourceName":"SZT Color","color":"#FF112233","random":False}], lambda L: oracle([["GetInputSettings",{"inputName":"SZT Color"}]])[0]['data']['inputSettings'].get('color')==0xFF332211),
 ("get status", [{"type":328}, log("T status %obs.isConnected% %obs.isRecording%")], lambda L: "T status True False" in L),
 ("hide scene sources", [{"type":325,"sceneName":A}], lambda L: not any(i['sceneItemEnabled'] for i in oracle([["GetSceneItemList",{"sceneName":A}]])[0]['data']['sceneItems'])),
 ("random scene source", [{"type":326,"sceneName":A}], lambda L: sum(i['sceneItemEnabled'] for i in oracle([["GetSceneItemList",{"sceneName":A}]])[0]['data']['sceneItems'])==1),
 ("media file", [{"type":322,"sceneName":A,"sourceName":"SZT Media","fileName":f"{D}/v2.mp4"}], lambda L: oracle([["GetInputSettings",{"inputName":"SZT Media"}]])[0]['data']['inputSettings'].get('local_file')==f"{D}/v2.mp4"),
 ("media pause", [{"type":49,"sceneName":A,"sourceName":"SZT Media","state":1}], lambda L: (time.sleep(0.5), oracle([["GetMediaInputStatus",{"inputName":"SZT Media"}]])[0]['data']['mediaState'])[1]=="OBS_MEDIA_STATE_PAUSED"),
 ("media play", [{"type":49,"sceneName":A,"sourceName":"SZT Media","state":0}], lambda L: (time.sleep(0.5), oracle([["GetMediaInputStatus",{"inputName":"SZT Media"}]])[0]['data']['mediaState'])[1]=="OBS_MEDIA_STATE_PLAYING"),
 ("media stop", [{"type":49,"sceneName":A,"sourceName":"SZT Media","state":3}], lambda L: (time.sleep(0.5), oracle([["GetMediaInputStatus",{"inputName":"SZT Media"}]])[0]['data']['mediaState'])[1]=="OBS_MEDIA_STATE_STOPPED"),
 ("replay start", [{"type":321,"state":0}], lambda L: (time.sleep(1.5), oracle([["GetReplayBufferStatus"]])[0]['data']['outputActive'])[1] is True),
 ("replay stop", [{"type":321,"state":1}], lambda L: (time.sleep(1.5), oracle([["GetReplayBufferStatus"]])[0]['data']['outputActive'])[1] is False),
 ("record start", [{"type":45,"state":0}], lambda L: (time.sleep(1.5), oracle([["GetRecordStatus"]])[0]['data']['outputActive'])[1] is True),
 ("record stop", [{"type":45,"state":1}], lambda L: (time.sleep(1.5), oracle([["GetRecordStatus"]])[0]['data']['outputActive'])[1] is False),
]
acts=[]
for i,(n,subs,_) in enumerate(cases):
    acts.append({"id":f"a{i}","name":f"obs {i}","enabled":True,"queue":"","group":"","triggers":[],
      "subActions":[dict(s, id=str(uuid.uuid4()), index=j, enabled=True, connectionId="") for j,s in enumerate(subs)]})
cfg={"start_hidden":True,"websockets":{"autoStart":True,"address":"127.0.0.1","port":8090,"endpoint":"/"},
     "obs":{"connections":[{"id":"obs1","name":"OBS","host":"127.0.0.1","port":4456,"password":"szt-test-pass","autoConnect":True,"reconnect":True,"retryInterval":5,"version":5}]},
     "actions":{"version":24,"actions":acts,"queues":[],"groups":[]}}
json.dump(cfg, open(f'{D}/streamerz.json','w'))
if os.path.exists(f"{D}/shot.png"): os.remove(f"{D}/shot.png")
logf=open(f'{D}/log.txt','w')
p=subprocess.Popen([f'{D}/streamerz'], cwd=D, stdout=logf, stderr=subprocess.STDOUT, env=dict(os.environ, DISPLAY=':99', WAYLAND_DISPLAY='', STREAMERZ_DEV='1'))
time.sleep(3)
FLIP0=oracle([["GetSceneItemTransform",{"sceneName":A,"sceneItemId":item(A,"SZT Image")}]])[0]['data']['sceneItemTransform']['scaleX']
fails=0
try:
    for i,(n,subs,check) in enumerate(cases):
        ws([{"request":"DoAction","id":"1","action":{"name":f"obs {i}"},"args":{"user":"tester"}}])
        time.sleep(0.8)
        L=[l.strip() for l in open(f'{D}/log.txt').read().splitlines()]
        try: ok=check(L)
        except Exception as e: ok=f"error {e!r}"
        print(("PASS" if ok is True else "FAIL"), n, "" if ok is True else ok)
        fails+= ok is not True
finally:
    p.terminate()
print("failures:", fails)
print("\n".join(l for l in open(f'{D}/log.txt').read().splitlines() if "OBS" in l)[:3000])
```

## Driving the window in tests

Never drive the window on the desktop someone is using: focusing it sends their keys to streamerz. Run it
in its own X server on a hidden Hyprland workspace instead; it has its own pointer and keyboard focus.
`dbus-run-session` hides the tray from the desktop's bar, so the window shows and closing it quits.

```
hyprctl eval 'hl.window_rule({ match = { class = "org.freedesktop.Xwayland" }, workspace = "special:szt silent" })'
(setsid Xwayland :99 -geometry 1200x800 -noreset >/dev/null 2>&1 &)
cd /tmp/szform  # a copy of the binary and a throwaway streamerz.json
(DISPLAY=:99 FLTK_BACKEND=x11 WAYLAND_DISPLAY= setsid dbus-run-session ./streamerz >/dev/null 2>&1 &)
DISPLAY=:99 xdotool mousemove 130 108 click 1     # a click at window coordinates 130,108
DISPLAY=:99 import -window root shot.png          # a screenshot
```

Without a window manager keys go to the window under the pointer, and windows open at 0,0: move the pointer
onto a dialog (and click its input) before typing. Type long text in chunks (one long burst stops FLTK's
input until restart).

## Streamlabs Desktop (src/slobs.rs)

Its remote-control API is JSON-RPC over a plain WebSocket at `ws://host:59650/api/websocket` (no SockJS
framing): `TcpServerService.auth(token)`, then subscriptions (a method named like the event, answered
`{_type: "SUBSCRIPTION", resourceId: "Service.event"}`; events come as `{id: null, result: {_type:
"EVENT", resourceId, data}}`). Methods are those of stream-labs/desktop's `app/services/api/external-api`,
which falls back to the internal services (`getSceneIds`, `SceneItem.flipX`/`flipY`/`rotate`).

Without Streamlabs Desktop (it's Windows/macOS only), the reference was the real Streamer.bot connected
to a fake API server that logs every call and answers like Streamlabs' models (scenes as `HELPER`
objects with their nodes, items with `resourceId`, filters, audio sources). Running each sub-action in
Streamer.bot showed the calls and enum values:

- Streaming: state 0 stop, 1 start (its docs list Start first); it reads `StreamingService.getModel` and
  calls `toggleStreaming` only when needed. Recording: 0 start, 1 stop; Pause and Resume do nothing.
- Visibility, filters: 0 on, 1 off, 2 toggle; mute: 0 muted, 1 not muted, 2 toggle
  (`AudioService.getSourcesForScene`, then `AudioSource.setMuted`).
- Flip: 0 `flipX`, 1 `flipY`, 2 both. Rotate: `setTransform({rotation})`, or `rotate(deg)` when Additive.
- Set Random Group Source Visible shows one of the group's hidden sources; Hide Group's Sources hides
  the visible ones. Set State of a Random Filter picks any of the source's filters.
- Triggers: Streaming Started on status `starting`, Stopped on `ending`; Recording Started on
  `recording`, Stopped on `stopping`. Arguments `sd.id`, `sd.name`, `sd.host` (and `sd.sceneName`); the
  trigger's connection field is `sdId` (all zeros: any).
- Set Browser Source URL and Set GDI Text make no call at all in Streamer.bot 1.0.7; streamerz uses
  `Source.updateSettings`.

## The window and the engine

The engine lives on the runtime thread and isn't `Send`; the window talks to it with closures
(`UiCmd`): `tell` runs one there, `query` runs one and waits for its answer (history, pending, temporary
globals, queue state). The engine never waits on the window, so `query` can't deadlock, but don't hold
a `cfg.borrow()` across it. The engine calls `ui::changed()` after config changes, runs and temporary
global writes; it only wakes the window while it's up, and the live tabs (Queues, History, Globals)
only ask the engine when they're the tab on screen.

Never hold `cfg.borrow()` across `query`: the engine may be waiting to write the config (it does at
the first start, for the instance id), and both threads stop for good.

The tabs are filled when the window opens, not when it's built: filling a list measures its text,
which opens the display and loads fontconfig and harfbuzz, about 14 MB, even with the window hidden.

Streamer.bot splits multi-line text (command aliases) on `\r\n` only: `!a\n!b` is one alias `!a!b`
there. Its text boxes write `\r\n`, and so do the forms' multi-line fields.

## Secrets (src/secrets.rs)

Every config key ending in `token`, `secret`, `password` or `cookie` (outside the user's own data:
actions, globals, commands, ...) is moved on save into one `secrets` value (`dpapi:` or `keyring:` +
base64) and put back on load. A new secret field only needs a name ending like that. On Linux the key is
read lazily from the Secret Service on its own thread, so nobody who never logs in touches the keyring.
Check it by hand: `secret-tool search application streamerz` shows the item; `secret-tool clear
application streamerz config <path>` removes it (the saved logins then can't be decrypted and are kept
as they are until the key is back). `main.rs`'s `redact` hides token-shaped values in every log line.

The Windows check needs the official rustc and both rust-std components; /tmp is cleared on reboot, so
reinstall them with:

```
mkdir -p /tmp/rtdl && cd /tmp/rtdl && V=$(rustc -V | cut -d' ' -f2) && for c in rustc-$V-x86_64-unknown-linux-gnu \
  rust-std-$V-x86_64-unknown-linux-gnu rust-std-$V-x86_64-pc-windows-msvc; do curl -sfLO \
  https://static.rust-lang.org/dist/$c.tar.xz && tar xf $c.tar.xz && ./$c/install.sh --prefix=/tmp/rt --disable-ldconfig; done
```

## Memory and CPU

Every PR measures the hidden window (start hidden, wait a few seconds):

```
pid=$(pgrep -x streamerz); grep VmRSS /proc/$pid/status
t1=$(awk '{print $14+$15}' /proc/$pid/stat); sleep 30; t2=$(awk '{print $14+$15}' /proc/$pid/stat); echo $((t2-t1))
```

The second number is CPU ticks (1/100 s) used in 30 s idle. The tray can be toggled without clicking:
`busctl --user call org.kde.StatusNotifierItem-$pid-1 /StatusNotifierItem org.kde.StatusNotifierItem Activate ii 0 0`.

## Icons

- `assets/brand/`: the app's logo. The four SVGs are the sources: `streamerz.svg` (colour, light
  background), `streamerz-dark.svg` (colour, dark background), `streamerz-simple.svg` and
  `streamerz-simple-dark.svg` (black and white). The window's icon is the colour one, for the theme in
  Settings (`brand.rs`; it changes at once). The tray's is the simple one and follows the desktop, not the
  app: the settings portal's `color-scheme` on Linux (changes live), the taskbar's `SystemUsesLightTheme`
  on Windows (read at start). It isn't a setting.
- `assets/icons/`: the platforms' and apps' logos (`ui/img.rs`). Twitch's and YouTube's have a white
  inside, drawn under the glyph.

After changing a logo in `assets/brand/`, redraw the rest (`streamerz.png` for the desktop file and the
AppImage, `streamerz.ico` for the Windows exe, `rgba/*.rgba` built into the binary):

```
dev/icons.sh
```

## Languages

Every text the user reads goes through `i18n::tr("some.key")` (`src/i18n.rs`), never a literal. The texts
live in `assets/locales/<code>.json` (flat, key to text, e.g. `en_US.json`), built into the binary. Only
`en_US` exists for now; it is the fallback of every other language, and a key nobody has shows as itself.
The language is a setting (`locale` in the config, Settings > Appearance) and applies at the next start.

To add a language: copy `en_US.json` to `<code>.json`, translate the values, and add a line to `LOCALES` in
`i18n.rs`. The tests check that every file is valid and has exactly the keys of `en_US.json`, that every
`tr("...")` names an existing key and that no key is unused. Texts built from pieces (names, numbers) and
the long tail of older literal texts are still to be moved: do it as each is touched.

## Twitch events without a Twitch login (Twitch CLI mock)

The [Twitch CLI](https://dev.twitch.tv/docs/cli/) runs a mock EventSub WebSocket; streamerz sends its
subscriptions there too when `twitch.eventsubUrl` points at it.

1. A throwaway folder with a copy of the binary and a `streamerz.json` holding a fake broadcaster
   (`"twitch": {"autoConnect": true, "eventsubUrl": "ws://127.0.0.1:8099/ws", "broadcaster": {"userId":
   "5555", "login": "tester", "token": {"accessToken": "fake"}}}`) and one action per trigger, each a Log
   sub-action with the variables to check.
2. `twitch event websocket start-server -p 8099` (not `-S`: CLI 1.1.24 refuses the newer subscription
   types, e.g. `channel.chat.message`, that real Twitch takes).
3. Start streamerz there, then `twitch event trigger channel.ban --transport=websocket --to-user 5555` for
   each event (`--from-user 5555` for a raid sent). The log shows what fired, with its variables.

The CLI can't send chat messages or chat notifications (subs, gifts, incoming raids, cheers come that
way); `twitch::tests::events_fire_once_with_their_variables` covers those. Whether Twitch takes each
subscription needs a real login: run with the real config and look for `Create sub` lines.
