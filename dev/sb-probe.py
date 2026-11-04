# What the real Streamer.bot passes to an action: set it up, do something, print each run's arguments.
#
#   uv run --with websockets python -I dev/sb-probe.py probe.json
#
# probe.json: {"settings": {...merged into data/settings.json...}, "triggers": [{"type": 501, ...}],
#              "steps": [["sh", "shell command"], ["sleep", 2], ["ws", {request}]], "wait": 3}
# Each trigger gets its own "[SZP]" action (a Comment sub-action); Streamer.bot is restarted with the
# settings; the Raw Action events of those actions are printed as JSON lines (their arguments). Back up
# ~/Games/streamerbot/data first (see DEVELOPING.md); nothing from it goes in the repo.
import asyncio, json, os, subprocess, sys, uuid
import websockets

SB = os.path.expanduser("~/Games/streamerbot")
HERE = os.path.dirname(os.path.abspath(__file__))
probe = json.load(open(sys.argv[1]))


def load(name):
    return json.load(open(f"{SB}/data/{name}", encoding="utf-8-sig"))


def save(name, d):
    open(f"{SB}/data/{name}", "w", encoding="utf-8-sig").write(json.dumps(d, ensure_ascii=False, separators=(",", ":")))


def merge(into, extra):
    for k, v in extra.items():
        if isinstance(v, dict) and isinstance(into.get(k), dict):
            merge(into[k], v)
        else:
            into[k] = v


subprocess.run([f"{HERE}/sb.sh", "stop"])
settings = load("settings.json")
settings["websockets"]["autoStart"] = True
merge(settings, probe.get("settings", {}))
save("settings.json", settings)
actions = load("actions.json")
actions["actions"] = [a for a in actions["actions"] if not a["name"].startswith("[SZP]")]
names = []
for i, t in enumerate(probe["triggers"]):
    t = dict({"id": str(uuid.uuid4()), "enabled": True, "exclusions": []}, **t)
    name = f"[SZP] {i} {t['type']}"
    names.append(name)
    actions["actions"].append({
        "id": str(uuid.uuid4()), "queue": "00000000-0000-0000-0000-000000000000", "enabled": True,
        "excludeFromHistory": False, "excludeFromPending": False, "name": name, "group": "SZP",
        "alwaysRun": False, "randomAction": False, "concurrent": True, "triggers": [t],
        "subActions": [{"id": str(uuid.uuid4()), "weight": 0.0, "type": 1009, "parentId": None,
                        "enabled": True, "index": 0, "value": "probe"}], "collapsedGroups": []})
save("actions.json", actions)
subprocess.run([f"{HERE}/sb.sh", "start"])


async def run():
    async with websockets.connect("ws://127.0.0.1:8080/") as ws:
        await ws.recv()
        await ws.send(json.dumps({"request": "Subscribe", "id": "s", "events": {"Raw": ["Action"]}}))
        await ws.recv()
        for kind, what in probe.get("steps", []):
            if kind == "sh":
                subprocess.run(what, shell=True)
            elif kind == "sleep":
                await asyncio.sleep(what)
            elif kind == "ws":
                await ws.send(json.dumps(what))
        end = asyncio.get_event_loop().time() + probe.get("wait", 3)
        while (left := end - asyncio.get_event_loop().time()) > 0:
            try:
                m = json.loads(await asyncio.wait_for(ws.recv(), left))
            except asyncio.TimeoutError:
                break
            d = m.get("data", {})
            if str(d.get("name", "")).startswith("[SZP]"):
                print(json.dumps({"action": d["name"], "args": d.get("arguments")}, ensure_ascii=False))


asyncio.run(run())
