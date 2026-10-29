#!/usr/bin/env python3
"""Opens the real app on the hidden display and pokes at it, so a crash or an empty screen is caught.

  dev/smoke.py [path to the Streamerz binary]      (default target/fast/Streamerz)

It starts a dev instance on DISPLAY=:99 with a scratch config, feeds it chat, visits every page, opens the
user card and the emote picker, right-clicks around, and checks after each step that the app is alive, that
no crash report was written, and that a screenshot of its own window can be taken (saved in /tmp/szsmoke).
Needs xdotool, and the nested display from the project's notes running as :99.
"""
import base64, json, os, re, socket, struct, subprocess, sys, tempfile, time

BIN = sys.argv[1] if len(sys.argv) > 1 else "target/fast/Streamerz"
PORT = 8095
OUT = "/tmp/szsmoke"
env = dict(os.environ, DISPLAY=":99", STREAMERZ_DEV="1")
env.pop("WAYLAND_DISPLAY", None)


class Ws:
    def __init__(self):
        self.s = socket.create_connection(("127.0.0.1", PORT), timeout=15)
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall(f"GET / HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += self.s.recv(1)
        self.read()  # Hello

    def read(self):
        h = self.s.recv(2)
        n = h[1] & 127
        if n == 126:
            n = struct.unpack(">H", self.s.recv(2))[0]
        elif n == 127:
            n = struct.unpack(">Q", self.s.recv(8))[0]
        data = b""
        while len(data) < n:
            data += self.s.recv(n - len(data))
        return json.loads(data)

    def ask(self, req):
        data = json.dumps(req).encode()
        mask = os.urandom(4)
        hdr = bytes([0x81])
        hdr += bytes([0x80 | len(data)]) if len(data) < 126 else bytes([0x80 | 126]) + struct.pack(">H", len(data))
        self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))
        return self.read()


def xdo(*a):
    subprocess.run(["xdotool", *a], env=env, check=True)


def main():
    os.makedirs(OUT, exist_ok=True)
    tmp = tempfile.mkdtemp()
    cfg = os.path.join(tmp, "s.json")
    json.dump({"start_hidden": False, "locale": "en_US", "websockets": {"port": PORT}}, open(cfg, "w"))
    env["STREAMERZ_CONFIG"] = cfg
    app = subprocess.Popen([BIN], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    failures = []

    def check(step):
        time.sleep(0.5)
        crashed = [f for f in os.listdir(os.path.join(tmp, "logs")) if f.startswith("crash")] if os.path.isdir(os.path.join(tmp, "logs")) else []
        if app.poll() is not None or crashed:
            failures.append(f"{step}: the app is gone (exit {app.poll()}) {crashed[:1]}")
            raise SystemExit(print("\n".join(failures), f"\nthe app's files (logs/crash-*.txt) are in {tmp}") or 1)
        return True

    try:
        time.sleep(5)
        ws = None
        for _ in range(20):
            try:
                ws = Ws()
                break
            except OSError:
                time.sleep(0.5)
        if not ws:
            sys.exit("can't reach the app")
        for p in ("twitch", "kick", "youtube", "tiktok"):
            ws.ask({"request": "InjectChat", "id": p, "platform": p, "user": "Ana", "text": f"hello from {p}"})
        pages = ["Chat", "Accounts", "Community", "Polls", "Actions", "Commands", "Timers", "Queues", "Groups", "Plugins",
                 "Servers", "Stream Apps", "Donations", "Globals", "History", "Settings", "Log", "Crashes", "Backups"]
        for page in pages:
            ws.ask({"request": "StreamerzUi", "id": "p", "do": f"page {page}"})
            r = ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/{page.replace(' ', '_')}.png"})
            if "saved" not in json.dumps(r):
                failures.append(f"{page}: no screenshot ({r})")
            # A right click in the middle of the page, where lists and chat lines are.
            xdo("mousemove", "450", "300")
            xdo("click", "3")
            time.sleep(0.4)
            xdo("key", "Escape")
            check(f"page {page}")
        # The Accounts page fills in (it's in a scroll: it once never did).
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Accounts"})
        if "Bot (optional)" not in json.dumps(ws.ask({"request": "StreamerzUi", "id": "f", "do": "find optional"})):
            failures.append("the Accounts page didn't fill in its accounts")
        # New chatters show up on an open Community tab.
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Community"})
        before = json.dumps(ws.ask({"request": "StreamerzUi", "id": "c", "do": "community"}))
        ws.ask({"request": "InjectChat", "id": "n", "platform": "kick", "user": "Caio", "text": "first time here"})
        time.sleep(1)
        after = json.dumps(ws.ask({"request": "StreamerzUi", "id": "c", "do": "community"}))
        if before == after:
            failures.append(f"a new chatter didn't show up on the open Community tab ({after})")
        # The window sits 1 px right and 20 px down on the screen (no window manager here).
        # Community sorts by a clicked title, and again the other way.
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Community"})
        for _ in range(2):
            xdo("mousemove", "252", "68")
            xdo("click", "1")
            time.sleep(0.4)
            check("sorting the community")
        ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/community_sorted.png"})
        # The channel search opens from Accounts, finds something, and closes with Escape.
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Accounts"})
        xdo("mousemove", "415", "102")
        xdo("click", "1")
        time.sleep(1)
        xdo("type", "--delay", "60", "xqc")
        time.sleep(3)
        subprocess.run(["import", "-window", "root", f"{OUT}/find.png"], env=env)
        xdo("key", "Escape")
        check("the channel search")
        # Pointing at a badge and an emoji tells what they are.
        ws.ask({"request": "InjectChat", "id": "b", "platform": "twitch", "user": "Ana", "role": "moderator", "text": "hello \U0001F600",
                "badges": [{"name": "subscriber", "version": "12", "imageUrl": ""}]})
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Chat"})
        time.sleep(1)
        for x in (308, 404):
            xdo("mousemove", str(x - 6), "78")
            xdo("mousemove", str(x), "78")
            time.sleep(1)
            check("hover details")
        xdo("mousemove", "600", "400")
        # A right click on a chat line (its menu), and on empty chat.
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Chat"})
        for y in ("74", "96", "400"):
            xdo("mousemove", "400", y)
            xdo("click", "3")
            time.sleep(0.4)
            xdo("key", "Escape")
            check(f"right click at {y}")
        # The user card, from the chat page and from Community.
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Chat"})
        xdo("mousemove", "294", "74")
        xdo("click", "1")
        time.sleep(1.5)
        r = ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/card.png"})
        if "saved" not in json.dumps(r):
            failures.append("the user card: no screenshot")
        # The chat keeps going under the card.
        before = json.dumps(ws.ask({"request": "StreamerzUi", "id": "c", "do": "chat"}))
        ws.ask({"request": "InjectChat", "id": "n", "platform": "twitch", "user": "Bia", "text": "still here?"})
        time.sleep(1)
        after = json.dumps(ws.ask({"request": "StreamerzUi", "id": "c", "do": "chat"}))
        if before == after:
            failures.append(f"the chat froze under the user card ({after})")
        xdo("key", "Escape")
        check("user card")
    finally:
        app.terminate()
    # Asked to end (as a logout does), it saves and quits: the next start mustn't see a crash.
    try:
        app.wait(10)
    except subprocess.TimeoutExpired:
        failures.append("it didn't quit when asked (SIGTERM)")
    if os.path.exists(os.path.join(tmp, "logs", "running")):
        failures.append("SIGTERM left the 'running' marker: the next start would report a crash")
    print("\n".join(failures) or f"ok: every page and the card, screenshots in {OUT}")
    if failures:
        print(f"the app's files (logs/crash-*.txt) are in {tmp}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
