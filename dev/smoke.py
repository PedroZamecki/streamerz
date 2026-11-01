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
env = dict(os.environ, DISPLAY=":99", STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(PORT))
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
    os.makedirs(os.path.join(tmp, "streamerz"))
    cfg = os.path.join(tmp, "streamerz", "streamerz.json")
    json.dump({"start_hidden": False, "locale": "en_US", "websockets": {"port": PORT}}, open(cfg, "w"))
    env["XDG_CONFIG_HOME"] = tmp
    app = subprocess.Popen([BIN], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    failures = []

    def check(step):
        time.sleep(0.5)
        import glob
        logs = glob.glob(os.path.join(tempfile.gettempdir(), f"streamerz-dev-{app.pid}-*", "logs", "crash*"))
        crashed = [os.path.basename(f) for f in logs]
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
            ws.ask({"request": "SimulateChatConnection", "id": "conn-" + p, "platform": p, "connected": True})
            ws.ask({"request": "InjectChat", "id": p, "platform": p, "user": "Ana", "text": f"hello from {p}"})
        # Rewards, power-ups and gifts are lines of the chat (each takes a line), and a message sent with a
        # reward has it over it.
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Chat"})
        # Stream end leaves connected chat visible; a disconnect hides only that platform's messages,
        # and reconnecting restores them from the retained history.
        time.sleep(1)
        kept = lambda: int(ws.ask({"request": "StreamerzUi", "id": "k", "do": "chatlines"})["result"])
        before_state = kept()
        ws.ask({"request": "SimulateStream", "id": "end", "platform": "twitch", "live": False})
        time.sleep(1)
        if kept() != before_state:
            failures.append("ending a stream changed the connected chat")
        ws.ask({"request": "SimulateChatConnection", "id": "down", "platform": "twitch", "connected": False})
        time.sleep(1)
        if kept() >= before_state:
            failures.append("a disconnected chat remained visible in the combined view")
        ws.ask({"request": "StreamerzUi", "id": "single", "do": "chatview Twitch"})
        time.sleep(0.5)
        where = ws.ask({"request": "StreamerzUi", "id": "find-disconnected", "do": "find Disconnected"})["result"]
        location = re.search(r" at (\d+),(\d+)", where)
        if not location or int(location.group(2)) < 500:
            failures.append(f"the disconnected state is not in the composer row ({where})")
        found = ws.ask({"request": "StreamerzUi", "id": "find-reconnect", "do": "find Reconnect"})["result"]
        coords = re.search(r" at (\d+),(\d+)", found)
        if coords:
            xdo("mousemove", coords.group(1), coords.group(2))
            xdo("click", "1")
            check("reconnecting chat")
        else:
            failures.append("the reconnect action is missing from the disconnected chat")
        ws.ask({"request": "StreamerzUi", "id": "together", "do": "chatview All together"})
        ws.ask({"request": "SimulateChatConnection", "id": "up", "platform": "twitch", "connected": True})
        time.sleep(1)
        if kept() != before_state:
            failures.append("reconnecting did not restore the chat history")
        seen = lambda: int(ws.ask({"request": "StreamerzUi", "id": "c", "do": "chat"})["result"])
        time.sleep(1)
        before = seen()
        ws.ask({"request": "InjectChat", "id": "rw", "platform": "twitch", "user": "Bia", "text": "drink water",
                "args": {"messageRewardId": "r1", "messageRewardTitle": "Hydrate", "messageRewardCost": 500}})
        for trig, args in (("TwitchAutomaticRewardRedemption", {"rewardType": "send_highlighted_message", "rewardCost": 40}),
                           ("TwitchPowerUpRedemption", {"powerUpType": "celebration", "bits": 50}),
                           ("TwitchWatchStreak", {"streak": 7}),
                           ("KickRewardRedemption", {"reward.title": "Hydrate", "rewardCost": 500})):
            ws.ask({"request": "FireTrigger", "id": "ft", "trigger": trig, "args": dict(args, user="Fabi")})
        time.sleep(1)
        if seen() - before < 5:
            failures.append(f"rewards and power-ups didn't all reach the chat ({before} to {seen()})")
        ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/chat_rewards.png"})
        # A login that ran out asks for a new one in a popup, once.
        ws.ask({"request": "StreamerzUi", "id": "rl", "do": "relogin Kick"})
        time.sleep(1.5)
        popup = json.dumps(ws.ask({"request": "StreamerzUi", "id": "w", "do": "windows"}))
        if "Log in" not in json.dumps(ws.ask({"request": "StreamerzUi", "id": "f", "do": "find Log in"})):
            failures.append(f"the login popup didn't open ({popup})")
        ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/relogin.png"})
        xdo("key", "Escape")
        check("relogin popup")
        # Changing a channel asks what to do with the people saved from the old one.
        ws.ask({"request": "StreamerzUi", "id": "rm", "do": "room"})
        time.sleep(1)
        if "Delete them" not in json.dumps(ws.ask({"request": "StreamerzUi", "id": "f", "do": "find Delete them"})):
            failures.append("the question about the old channel's people didn't open")
        ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/room.png"})
        xdo("key", "Escape")
        check("room question")
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
        # The platform funnel (left of the titles) hides Twitch, the first in its menu, and shows it again.
        shown = lambda: json.dumps(ws.ask({"request": "StreamerzUi", "id": "c", "do": "community"}))
        before = shown()
        for _ in range(2):
            xdo("mousemove", "205", "68")
            xdo("click", "1")
            time.sleep(0.5)
            xdo("mousemove", "240", "80")
            xdo("click", "1")
            time.sleep(0.5)
            check("filtering the community by platform")
            if _ == 0 and shown() == before:
                failures.append(f"the platform filter hid nothing ({before})")
        if shown() != before:
            failures.append(f"the platform filter didn't show Twitch again ({shown()})")
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
            failures.append(f"the user card: no screenshot ({r})")
        # The chat keeps going under the card.
        before = json.dumps(ws.ask({"request": "StreamerzUi", "id": "c", "do": "chat"}))
        ws.ask({"request": "InjectChat", "id": "n", "platform": "twitch", "user": "Bia", "text": "still here?"})
        time.sleep(1)
        after = json.dumps(ws.ask({"request": "StreamerzUi", "id": "c", "do": "chat"}))
        if before == after:
            failures.append(f"the chat froze under the user card ({after})")
        xdo("key", "Escape")
        check("user card")
        # The chat keeps at most 400 lines of a platform (the default) however many come, still shows them,
        # and after the window is hidden and shown again it fills from the engine.
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Chat"})
        for i in range(450):
            ws.ask({"request": "InjectChat", "id": f"m{i}", "platform": "kick", "user": f"u{i % 7}", "text": f"line {i}"})
        time.sleep(1)
        kept = lambda: int(ws.ask({"request": "StreamerzUi", "id": "c", "do": "chatlines"})["result"])
        if not 100 < kept() <= 430:
            failures.append(f"the chat kept {kept()} lines of 450 (400 of Kick, and the few lines of others)")
        ws.ask({"request": "StreamerzUi", "id": "t", "do": "toggle"})
        time.sleep(1)
        ws.ask({"request": "StreamerzUi", "id": "t", "do": "toggle"})
        time.sleep(1.5)
        check("hiding and showing the chat")
        if not 100 < kept() <= 430:
            failures.append(f"the chat filled with {kept()} lines after being shown again")
    finally:
        app.terminate()
    # Asked to end (as a logout does), it saves and quits: the next start mustn't see a crash.
    try:
        app.wait(10)
    except subprocess.TimeoutExpired:
        failures.append("it didn't quit when asked (SIGTERM)")
    import glob
    if glob.glob(os.path.join(tempfile.gettempdir(), f"streamerz-dev-{app.pid}-*")):
        failures.append("SIGTERM left the development settings directory behind")
    print("\n".join(failures) or f"ok: every page and the card, screenshots in {OUT}")
    if failures:
        print(f"the app's files (logs/crash-*.txt) are in {tmp}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
