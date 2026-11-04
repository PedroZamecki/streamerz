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
subprocess.run([os.path.join(os.path.dirname(os.path.abspath(__file__)), 'sb.sh'), 'stop'])
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
subprocess.run([os.path.join(os.path.dirname(os.path.abspath(__file__)), 'sb.sh'), 'start'])
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
            await ws.send(r); print('>>', await ws.recv())
asyncio.run(run())
time.sleep(1)
with open(log, encoding='utf-8', errors='replace') as f:
    f.seek(pos); out = f.read()
for line in out.splitlines():
    if 'SZT' in line or 'ERR' in line or 'WRN' in line: print(line[:400])
