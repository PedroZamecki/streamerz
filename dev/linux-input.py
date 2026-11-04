#!/usr/bin/env python3
"""Checks the Key Press and Mouse Click triggers end to end on Linux under X11, on the hidden display.

  dev/linux-input.py [path/to/Streamerz]      (default target/fast/Streamerz)

Runs the real binary as an X11 session on Xvfb (DISPLAY=:99), saves a few actions over the WebSocket, then
makes the X server report input with xdotool: a grabbed key combination, clicks with each button (XInput2's
raw events) and the pointer's place. Nothing to allow on X11, so nothing on the system changes.
Wayland reads the devices instead (devinput.rs); that needs the access a user allows, so it's checked by
hand (docs/UNTESTED.md).
"""
import base64, json, os, shutil, socket, struct, subprocess, sys, tempfile, time

BIN = sys.argv[1] if len(sys.argv) > 1 else "target/fast/Streamerz"
PORT = int(os.environ.get("SZ_INPUT_PORT", "8098"))
DIR = tempfile.mkdtemp(prefix="szinput-")
env = dict(os.environ, DISPLAY=":99", XDG_SESSION_TYPE="x11", STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(PORT),
           STREAMERZ_CONFIG=os.path.join(DIR, "streamerz.json"))
env.pop("WAYLAND_DISPLAY", None)
env.pop("HYPRLAND_INSTANCE_SIGNATURE", None)
failed = []


def report(name, ok, detail=""):
    print(f"{'ok' if ok else 'FAIL':4}  {name}" + (f": {detail}" if detail and not ok else ""), flush=True)
    if not ok:
        failed.append(name)


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
        hdr = bytes([0x81]) + (bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack(">H", n))
        self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))
        while True:
            r = self.read()
            if r.get("id") == req["id"]:
                return r


def log(ws):
    lines = ws.ask({"request": "GetLog", "lines": 400}).get("lines") or []
    return "\n".join(x if isinstance(x, str) else json.dumps(x) for x in lines)


def wait_log(ws, text, secs=5):
    end = time.time() + secs
    while time.time() < end:
        if text in log(ws):
            return True
        time.sleep(0.2)
    return False


def xdo(*a):
    subprocess.run(["xdotool", *a], env=env, check=False, capture_output=True)


def action(ws, name, triggers, msg):
    item = {"name": name, "enabled": True, "group": "Input", "triggers": triggers,
            "subActions": [{"type": 1020, "logLevel": 2, "message": msg}]}
    r = ws.ask({"request": "StreamerzSave", "what": "action", "item": item})
    if r.get("status") != "ok":
        sys.exit(f"saving {name}: {r}")


def main():
    json.dump({"start_hidden": True, "websockets": {"port": PORT}}, open(env["STREAMERZ_CONFIG"], "w"))
    app = subprocess.Popen([BIN], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(60):
            try:
                ws = Ws()
                break
            except OSError:
                time.sleep(0.5)
        else:
            sys.exit("can't reach the app")
        action(ws, "keys", [{"type": 16001, "keyCode": 112, "modifierMask": 2}], "KEY %key% ctrl=%hasCtrl%")
        action(ws, "other keys", [{"type": 16001, "keyCode": 113, "modifierMask": 0}], "WRONG KEY")
        action(ws, "any click", [{"type": 16002}], "CLICK %mouse.button% n=%mouse.clicks% at %mouse.x%,%mouse.y%")
        action(ws, "right click", [{"type": 16002, "button": 2}], "RIGHT ONLY")
        report("listening on X11", wait_log(ws, "taken from X11") and wait_log(ws, "listening to X11's clicks"),
               log(ws)[-600:])
        xdo("key", "ctrl+F1")
        report("Ctrl+F1 fires its Key Press trigger", wait_log(ws, "KEY 112 ctrl=True"))
        # With NumLock on, the grab still matches.
        xdo("key", "Num_Lock")
        xdo("key", "ctrl+F1")
        time.sleep(1)
        report("with NumLock on too", log(ws).count("KEY 112 ctrl=True") == 2)
        xdo("key", "Num_Lock")
        xdo("key", "F1")
        time.sleep(0.5)
        report("F1 alone fires nothing", log(ws).count("KEY 112") == 2 and "WRONG KEY" not in log(ws))
        xdo("mousemove", "123", "234")
        xdo("click", "1")
        report("a left click, with the pointer's place", wait_log(ws, "CLICK 1 n=1 at 123,234"), log(ws)[-400:])
        xdo("click", "1")
        report("a second click soon after counts as a double click", wait_log(ws, "CLICK 1 n=2"))
        time.sleep(0.7)
        xdo("click", "3")
        report("a right click fires the right-button trigger too",
               wait_log(ws, "CLICK 2 n=1") and wait_log(ws, "RIGHT ONLY"))
        xdo("click", "2")
        report("middle", wait_log(ws, "CLICK 3 n=1"))
        xdo("click", "8")
        report("back", wait_log(ws, "CLICK 4 n=1"))
        xdo("click", "9")
        report("forward", wait_log(ws, "CLICK 5 n=1"))
        before = log(ws).count("CLICK")
        xdo("click", "4")
        xdo("click", "5")
        time.sleep(0.7)
        report("the wheel isn't a click", log(ws).count("CLICK") == before)
        report("RIGHT ONLY ran once", log(ws).count("RIGHT ONLY") == 1)
    finally:
        app.terminate()
        try:
            app.wait(10)
        except subprocess.TimeoutExpired:
            app.kill()
        shutil.rmtree(DIR, ignore_errors=True)
    print("all ok" if not failed else f"{len(failed)} failed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
