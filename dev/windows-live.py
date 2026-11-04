#!/usr/bin/env python3
"""Checks the Windows build's own OS paths, end to end, under Wine on the hidden display.

  dev/windows-live.py path/to/Streamerz.exe       (dev/windows.sh live builds it first)
  SZ_ONLINE="twitch:gaules kick:xqc youtube:@LofiGirl" dev/windows-live.py ...   (also reads live chats)

The scenarios fake what the OS reports (a hotkey, a click, the clipboard, MIDI); this runs the real exe and
makes the OS report it: RegisterHotKey and SendInput, the low-level mouse hook, the clipboard listener,
winmm MIDI out and in (looped through ALSA's "Midi Through", which Wine shows as a MIDI device), Toolhelp,
WinRT toasts, SAPI, the media session, the tray, the system language and theme from the registry.
Each check says ok, FAIL, or "Wine:" when Wine can't do it (those need real Windows).
Use a scratch prefix (WINEPREFIX=/tmp/szwine): it sets the registry.
"""
import base64, glob, json, os, re, shutil, socket, struct, subprocess, sys, time

ONLINE = dict(x.split(":", 1) for x in os.environ.get("SZ_ONLINE", "").split())
EXE = sys.argv[1] if len(sys.argv) > 1 else "target/xwin/x86_64-pc-windows-msvc/release/Streamerz.exe"
PORT = int(os.environ.get("SZ_LIVE_PORT", "8097"))
env = dict(os.environ, DISPLAY=":99", STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(PORT), WINEDEBUG="-all")
env.pop("WAYLAND_DISPLAY", None)
env.setdefault("WINEPREFIX", "/tmp/szwine")
USER = os.path.join(env["WINEPREFIX"], "drive_c", "users", os.environ["USER"])
TEMP = os.path.join(USER, "AppData", "Local", "Temp")
ROAMING = os.path.join(USER, "AppData", "Roaming")
results = []


def report(name, ok, detail="", wine=False):
    tag = "ok" if ok else ("Wine" if wine else "FAIL")
    results.append((tag, name, detail))
    print(f"{tag:4}  {name}" + (f": {detail}" if detail and not ok else ""), flush=True)


class Ws:
    def __init__(self):
        self.s = socket.create_connection(("127.0.0.1", PORT), timeout=20)
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall(f"GET / HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                       f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += self.s.recv(1)
        self.read()

    def recv(self, n):
        data = b""
        while len(data) < n:
            data += self.s.recv(n - len(data))
        return data

    def read(self):
        h = self.recv(2)
        n = h[1] & 127
        if n == 126:
            n = struct.unpack(">H", self.recv(2))[0]
        elif n == 127:
            n = struct.unpack(">Q", self.recv(8))[0]
        return json.loads(self.recv(n))

    def ask(self, req):
        req.setdefault("id", str(time.time()))
        data = json.dumps(req).encode()
        mask = os.urandom(4)
        n = len(data)
        hdr = bytes([0x81]) + (bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack(">H", n)
                               if n < 65536 else bytes([0x80 | 127]) + struct.pack(">Q", n))
        self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))
        while True:
            r = self.read()
            if r.get("id") == req["id"]:
                return r


def wine(*a, **kw):
    return subprocess.run(["wine", *a], env=env, capture_output=True, text=True, **kw)


def reg_add(key, name, kind, value):
    wine("reg", "add", key, "/v", name, "/t", kind, "/d", str(value), "/f")


def xdo(*a):
    subprocess.run(["xdotool", *a], env=env, check=False, capture_output=True)


def start(extra_env=None, config=None):
    os.makedirs(os.path.join(ROAMING, "streamerz"), exist_ok=True)
    cfg = {"start_hidden": False, "theme": "system", "websockets": {"port": PORT}}
    cfg.update(config or {})
    json.dump(cfg, open(os.path.join(ROAMING, "streamerz", "streamerz.json"), "w"))
    for d in glob.glob(os.path.join(TEMP, "streamerz-dev-*")):
        shutil.rmtree(d, ignore_errors=True)
    app = subprocess.Popen(["wine", EXE], env=dict(env, **(extra_env or {})),
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(60):
        try:
            return app, Ws()
        except OSError:
            time.sleep(0.5)
    sys.exit("can't reach the app")


def stop(app):
    subprocess.run(["wineboot", "-e"], env=env, capture_output=True)
    try:
        app.wait(15)
    except subprocess.TimeoutExpired:
        app.kill()
    subprocess.run(["wineserver", "-w"], env=env, timeout=30)


def log(ws, n=400):
    lines = ws.ask({"request": "GetLog", "lines": n}).get("lines") or []
    return "\n".join(x if isinstance(x, str) else json.dumps(x) for x in lines)


def wait_log(ws, text, secs=8):
    end = time.time() + secs
    while time.time() < end:
        if text in log(ws):
            return True
        time.sleep(0.3)
    return False


def action(ws, name, triggers=(), subs=()):
    item = {"name": name, "enabled": True, "group": "Live", "triggers": list(triggers), "subActions": list(subs)}
    r = ws.ask({"request": "StreamerzSave", "what": "action", "item": item})
    if r.get("status") != "ok":
        sys.exit(f"saving {name}: {r}")


def say(msg):
    return {"type": 1020, "logLevel": 2, "message": msg}


def crashed():
    return [f for d in glob.glob(os.path.join(TEMP, "streamerz-dev-*")) for f in glob.glob(os.path.join(d, "logs", "crash*"))]


def main():
    for tool in ("xdotool", "wine", "aconnect"):
        if not shutil.which(tool):
            sys.exit(f"needs {tool}")
    # The system's theme and language, as Windows keeps them.
    personalize = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
    reg_add(personalize, "AppsUseLightTheme", "REG_DWORD", 1)
    reg_add(personalize, "SystemUsesLightTheme", "REG_DWORD", 1)
    app, ws = start({"LANG": "pt_BR.UTF-8", "LC_ALL": "pt_BR.UTF-8"})
    try:
        time.sleep(2)
        # Language: no locale set, so the system's (GetUserDefaultLocaleName) is used.
        ws.ask({"request": "StreamerzUi", "do": "page Settings"})
        found = json.dumps(ws.ask({"request": "StreamerzUi", "do": "find Configurações"}))
        report("system language (pt-BR) picks the Portuguese UI", "at " in found, found[:200])
        # Theme: the registry says light, so the window is light.
        shot = "/tmp/szlive-theme.png"
        ws.ask({"request": "StreamerzUi", "do": f"shot {shot}"})
        px = subprocess.run(["magick", shot, "-format", "%[fx:int(255*mean)]", "info:"], capture_output=True, text=True).stdout
        report("system theme (light, from the registry) makes a light window", px.strip().isdigit() and int(px) > 150, f"mean {px}")
    finally:
        stop(app)
    reg_add(personalize, "AppsUseLightTheme", "REG_DWORD", 0)
    reg_add(personalize, "SystemUsesLightTheme", "REG_DWORD", 0)

    app, ws = start(config={"locale": "en_US"})
    try:
        time.sleep(2)
        shot = "/tmp/szlive-dark.png"
        ws.ask({"request": "StreamerzUi", "do": f"shot {shot}"})
        px = subprocess.run(["magick", shot, "-format", "%[fx:int(255*mean)]", "info:"], capture_output=True, text=True).stdout
        report("system theme (dark) makes a dark window", px.strip().isdigit() and int(px) < 100, f"mean {px}")

        # Tray: no warning about it.
        report("tray icon starts", "no tray icon" not in log(ws), log(ws)[-300:])

        # Hotkeys: RegisterHotKey, pressed on the keyboard (X, through Wine) and sent by Keyboard Press
        # (SendInput).
        action(ws, "hotkey", [{"type": 16001, "keyCode": 120, "modifierMask": 2}], [say("LIVE hotkey %key%")])
        action(ws, "press", [], [{"type": 1012, "key": "F9", "modifiers": 2}])
        time.sleep(1.5)
        ws.ask({"request": "StreamerzUi", "do": "page Log"})
        xdo("mousemove", "400", "300")
        xdo("click", "1")
        xdo("key", "ctrl+F9")
        report("hotkey (RegisterHotKey) fires Key Press from the keyboard", wait_log(ws, "LIVE hotkey"),
               "Wine passes keys only to a focused Wine window", wine=True)
        before = log(ws).count("LIVE hotkey")
        ws.ask({"request": "DoAction", "action": {"name": "press"}, "args": {}})
        time.sleep(2)
        report("Keyboard Press (SendInput) reaches the hotkey", log(ws).count("LIVE hotkey") > before,
               "Keyboard Press: " + ";".join(re.findall(r"Keyboard Press[^\n]*", log(ws))[-1:]), wine=True)

        # Mouse Click: the low-level mouse hook.
        action(ws, "click", [{"type": 16002}], [say("LIVE click %mouse.button%")])
        time.sleep(1.5)
        xdo("mousemove", "500", "400")
        xdo("click", "1")
        report("mouse hook (WH_MOUSE_LL) fires Mouse Click", wait_log(ws, "LIVE click"), wine=True)

        # Clipboard: Notepad copies, the clipboard listener tells.
        action(ws, "clip", [{"type": 42001}], [say("LIVE clip %clipboard.text%")])
        time.sleep(1.5)
        note = subprocess.Popen(["wine", "notepad"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(3)
        win = subprocess.run(["xdotool", "search", "--name", "Notepad"], env=env, capture_output=True, text=True).stdout.split()
        if win:
            xdo("windowactivate", "--sync", win[-1])
            xdo("windowfocus", "--sync", win[-1])
            xdo("type", "--delay", "40", "copied from notepad")
            xdo("key", "ctrl+a")
            xdo("key", "ctrl+c")
        report("clipboard listener fires Clipboard Changed (copied in Notepad)",
               wait_log(ws, "LIVE clip copied from notepad"), "notepad window " + ("found" if win else "missing"))
        note.kill()
        subprocess.run(["wine", "taskkill", "/f", "/im", "notepad.exe"], env=env, capture_output=True)

        # Processes: Toolhelp sees a program start and end.
        action(ws, "started", [{"type": 703, "processName": "ping"}], [say("LIVE started %processName%")])
        action(ws, "stopped", [{"type": 704, "processName": "ping.exe"}], [say("LIVE stopped")])
        time.sleep(1.5)
        wine("ping", "-n", "3", "127.0.0.1", timeout=30)
        report("process watcher sees a program started from outside", wait_log(ws, "LIVE started", 6))
        report("and ended", wait_log(ws, "LIVE stopped", 6))

        # MIDI: winmm out to Midi Through, back in through winmm.
        action(ws, "midi in", [{"type": 15001, "commandCode": 1, "data1": "60"}],
               [say("LIVE midi %midi.device% %midi.note% %midi.velocity%")])
        time.sleep(2)
        lines = log(ws)
        dev = re.findall(r"MIDI: listening to ([^\n]+)", lines)
        report("winmm lists MIDI inputs", bool(dev), lines[-300:])
        through = next((d.strip() for d in dev if "Through" in d), dev[0].strip() if dev else "Midi Through Port-0")
        action(ws, "midi out", [], [{"type": 9001, "device": through, "channel": 1, "note": 60, "velocity": 100}])
        time.sleep(0.5)
        ws.ask({"request": "DoAction", "action": {"name": "midi out"}, "args": {}})
        report(f"MIDI Out to {through} comes back as MIDI Message", wait_log(ws, "LIVE midi"),
               ";".join(re.findall(r"MIDI[^\n]*", log(ws))[-3:]))

        # Toasts: WinRT. Wine has no notification platform: a warning, not a crash.
        action(ws, "toast", [], [{"type": 1024, "toastId": "t1", "title": "Live toast", "text": "hi",
                                  "attribution": "", "iconPath": ""}])
        ws.ask({"request": "DoAction", "action": {"name": "toast"}, "args": {}})
        time.sleep(3)
        warned = re.findall(r"Toast \(Live toast\)[^\n]*", log(ws))
        report("Toast Notification shows through WinRT", not warned and not crashed(), "; ".join(warned), wine=True)
        report("a toast that can't show is a warning, not a crash", not crashed())

        # Speech: SAPI's recognizer. Wine has none: a warning, not a crash.
        action(ws, "speech", [{"type": 301}], [say("LIVE heard %spokenText%")])
        time.sleep(3)
        sp = re.findall(r"(?i)speech[^\n]*", log(ws))
        report("SAPI recognizer starts", any("listening" in x.lower() for x in sp), "; ".join(sp[-2:]), wine=True)
        report("a missing recognizer is a warning, not a crash", not crashed())

        # The media session (Spotify's local player): WinRT again.
        r = ws.ask({"request": "StreamerzSpotifyLocal"})
        report("media session (GSMTC) answers", r.get("status") == "ok" and "error" not in json.dumps(r).lower(),
               json.dumps(r)[:200], wine=True)
        report("a missing media session is an answer, not a crash", not crashed())

        # Run a Program: a console program with no window, its output in variables.
        action(ws, "run", [], [{"type": 6, "command": "cmd", "arguments": "/c echo one& echo k=v", "waitForExit": 5},
                               say("LIVE ran %outputCount% [%output0%] %k%")])
        ws.ask({"request": "DoAction", "action": {"name": "run"}, "args": {}})
        report("Run a Program reads a Windows program's output", wait_log(ws, "LIVE ran 3 [one] v"),
               ";".join(re.findall(r"LIVE ran[^\n]*", log(ws))))

        report("nothing crashed", not crashed(), ", ".join(crashed()))
    finally:
        stop(app)
    report("quits on session end and removes its development folder", not glob.glob(os.path.join(TEMP, "streamerz-dev-*")))

    if ONLINE:
        online()

    print()
    for tag in ("FAIL", "Wine"):
        n = [r for r in results if r[0] == tag]
        if n:
            print(f"{len(n)} {'failed' if tag == 'FAIL' else 'need real Windows (Wine cannot)'}:")
            for _, name, detail in n:
                print(f"  {name}" + (f" ({detail})" if detail else ""))
    sys.exit(1 if any(r[0] == "FAIL" for r in results) else 0)


def online():
    """Real chats, read without a login (development mode sends nothing): the exe's own TLS and sockets."""
    cfg = {"locale": "en_US"}
    if "twitch" in ONLINE:
        cfg["twitch"] = {"channel": ONLINE["twitch"]}
    if "kick" in ONLINE:
        cfg["kick"] = {"channel": ONLINE["kick"]}
    if "youtube" in ONLINE:
        cfg["youtube"] = {"channelId": ONLINE["youtube"]}
    app, ws = start(config=cfg)
    try:
        # Development mode doesn't connect at start: the Accounts page's Connect buttons do.
        ws.ask({"request": "StreamerzUi", "do": "page Accounts"})
        time.sleep(1)
        found = ws.ask({"request": "StreamerzUi", "do": "find Connect"}).get("result", "")
        for x, y in re.findall(r'^"Connect" at (\d+),(\d+)', found, re.M):
            xdo("mousemove", x, y)
            xdo("click", "1")
            time.sleep(0.5)
        for code, name in ((133, "twitch"), (35010, "kick"), (4003, "youtube")):
            if name in ONLINE:
                action(ws, f"{name} chat", [{"type": code}], [say(f"LIVE {name} chat from %user%")])
        for name, channel in ONLINE.items():
            report(f"{name} chat ({channel}) reaches a trigger", wait_log(ws, f"LIVE {name} chat from", 60),
                   "; ".join(l for l in log(ws).splitlines() if name.lower() in l.lower())[-400:])
        report("nothing crashed online", not crashed())
    finally:
        stop(app)


if __name__ == "__main__":
    main()
