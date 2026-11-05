#!/usr/bin/env python3
"""Opens the real app on the hidden display and pokes at it, so a crash or an empty screen is caught.

  dev/smoke.py [path to the Streamerz binary]      (default target/fast/Streamerz)
  dev/smoke.py --exe path/to/Streamerz.exe          (the Windows build, under Wine: see dev/windows.sh)

It starts a dev instance on DISPLAY=:99 with a scratch config, feeds it chat, visits every page, opens the
user card and the emote picker, right-clicks around, and checks after each step that the app is alive, that
no crash report was written, and that a screenshot of its own window can be taken (saved in /tmp/szsmoke).
Needs xdotool, and the nested display from the project's notes running as :99.
"""
import base64, json, os, re, socket, struct, subprocess, sys, tempfile, time

WINE = len(sys.argv) > 2 and sys.argv[1] == "--exe"
BIN = sys.argv[2] if WINE else sys.argv[1] if len(sys.argv) > 1 else "target/fast/Streamerz"
PORT = 8095
OUT = "/tmp/szsmoke-wine" if WINE else "/tmp/szsmoke"
env = dict(os.environ, DISPLAY=":99", STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(PORT))
env.pop("WAYLAND_DISPLAY", None)
# Under Wine the app's folders are the prefix's (APPDATA and TEMP), and its pid is Wine's own, so the
# development folder is found by name. The window manager is Wine's: clicks are moved by where the window is.
if WINE:
    env.setdefault("WINEPREFIX", os.path.expanduser("~/.wine"))
    env.setdefault("WINEDEBUG", "-all")
    _user = os.path.join(env["WINEPREFIX"], "drive_c", "users", os.environ["USER"])
    TEMP = os.path.join(_user, "AppData", "Local", "Temp")
    ROAMING = os.path.join(_user, "AppData", "Roaming")
else:
    TEMP = tempfile.gettempdir()
OFFSET = [0, 0]


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


def xdo(*a, found=False):
    # Places from `find` are already on the screen; the others are the window's at 1,20.
    if a[0] == "mousemove" and not found:
        a = ("mousemove", str(int(a[1]) + OFFSET[0]), str(int(a[2]) + OFFSET[1]))
    subprocess.run(["xdotool", *a], env=env, check=True)


def dev_dirs(pid):
    import glob
    return glob.glob(os.path.join(TEMP, "streamerz-dev-*" if WINE else f"streamerz-dev-{pid}-*"))


def main():
    os.makedirs(OUT, exist_ok=True)
    tmp = ROAMING if WINE else tempfile.mkdtemp()
    os.makedirs(os.path.join(tmp, "streamerz"), exist_ok=True)
    cfg = os.path.join(tmp, "streamerz", "streamerz.json")
    # These checks exercise retained chat; history is opt-in in a fresh installation.
    json.dump({"start_hidden": False, "locale": "en_US", "websockets": {"port": PORT},
               "window": {"chat": {"saveHistory": True}}}, open(cfg, "w"))
    env["XDG_CONFIG_HOME"] = tmp
    if WINE:
        import shutil
        for d in dev_dirs(0):
            shutil.rmtree(d, ignore_errors=True)
    app = subprocess.Popen(["wine", BIN] if WINE else [BIN], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    failures = []

    def check(step):
        time.sleep(0.5)
        import glob
        logs = [f for d in dev_dirs(app.pid) for f in glob.glob(os.path.join(d, "logs", "crash*"))]
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
        if WINE:
            # The places below are measured with the window at 1,20 (no window manager).
            geo = subprocess.run(["xdotool", "search", "--name", "^Streamerz", "getwindowgeometry"], env=env,
                                 capture_output=True, text=True).stdout
            at = re.search(r"Position: (\d+),(\d+)", geo)
            if at:
                OFFSET[:] = [int(at.group(1)) - 1, int(at.group(2)) - 20]
        for p in ("twitch", "kick", "youtube", "tiktok"):
            ws.ask({"request": "SimulateChatConnection", "id": "conn-" + p, "platform": p, "connected": True})
            ws.ask({"request": "InjectChat", "id": p, "platform": p, "user": "Ana", "text": f"hello from {p}"})
        # Rewards, power-ups and gifts are lines of the chat (each takes a line), and a message sent with a
        # reward has it over it.
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Chat"})
        for platform in ("twitch", "kick"):
            ws.ask({"request": "SimulateChatModes", "id": "mode-" + platform, "platform": platform,
                    "emote_only": True, "slow_seconds": 30})
        time.sleep(1)
        for platform in ("Twitch", "Kick"):
            if platform not in ws.ask({"request": "StreamerzUi", "id": "modes", "do": "find " + platform + ": Emote-only mode"})["result"]:
                failures.append(f"{platform} chat modes are missing")
        ws.ask({"request": "StreamerzUi", "id": "modes-wide", "do": f"shot {OUT}/modes-wide.png"})
        if not WINE:
            window = subprocess.check_output(["xdotool", "search", "--pid", str(app.pid), "--onlyvisible", "--name", "Streamerz"], env=env).decode().splitlines()[0]
            xdo("windowsize", window, "540", "700")
            time.sleep(1)
            ws.ask({"request": "StreamerzUi", "id": "modes-narrow", "do": f"shot {OUT}/modes-narrow.png"})
            xdo("windowsize", window, "1100", "700")
        ws.ask({"request": "SimulateChatModes", "id": "mode-off", "platform": "kick"})
        time.sleep(1)
        if "Kick: Emote-only mode" in ws.ask({"request": "StreamerzUi", "id": "modes-off", "do": "find Kick: Emote-only mode"})["result"]:
            failures.append("Kick chat mode indicator did not clear")
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
            xdo("mousemove", coords.group(1), coords.group(2), found=True)
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
        # This question has three explicit choices, so answer it before navigating away.
        keep = ws.ask({"request": "StreamerzUi", "id": "keep", "do": "find Keep them apart"})["result"]
        where = re.search(r" at (\d+),(\d+)", keep)
        if where:
            xdo("mousemove", *where.groups(), found=True)
            xdo("click", "1")
        else:
            failures.append("the keep-people choice is missing")
        check("room question")
        # A built-in template makes a group with settings: its settings dialog opens, shows them and closes.
        if ws.ask({"request": "StreamerzTemplateAdd", "id": "tpl", "name": "Subathon timer"}).get("status") != "ok":
            failures.append("the subathon template was not added")
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Actions"})
        time.sleep(1)
        # The group's name is the first line of the tree and its settings button the first of the third row
        # of the button grid; places are the window's own, so the window's corner (from the heading) is added.
        heading = ws.ask({"request": "StreamerzUi", "id": "f", "do": "find Actions"})["result"]
        # Besides the page's own label, "Actions" is the list's heading, at the left.
        corner = next(((int(x), int(y)) for x, y in re.findall(r'"Actions" at (\d+),(\d+)', heading) if int(x) < 300), None)
        if not corner:
            failures.append(f"group settings: the Actions page is not shown ({heading})")
        for rel in ((254, 52), (237, 631)) if corner else ():
            xdo("mousemove", str(corner[0] - 214 + rel[0]), str(corner[1] - 21 + rel[1]), found=True)
            xdo("click", "1")
            time.sleep(1)
        if corner:
            ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/group_settings.png"})
            if "Group settings: Subathon" not in json.dumps(ws.ask({"request": "StreamerzUi", "id": "w", "do": "windows"})):
                failures.append("group settings: the dialog did not open")
            xdo("key", "Escape")
            check("group settings")
        # A switched off trigger and sub-action are dimmed with a power-off icon, not tagged with text.
        ws.ask({"request": "StreamerzSave", "what": "action", "item": {
            "name": "Off demo", "enabled": True,
            "triggers": [{"type": 16001, "keyCode": 68, "modifierMask": 6, "enabled": False}, {"type": 16001, "keyCode": 69, "modifierMask": 0, "enabled": True}],
            "subActions": [{"type": 99001, "enabled": False}]}})
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Actions"})
        time.sleep(1)
        if corner:
            # The action's name is the line after the Subathon group and its three actions.
            xdo("mousemove", str(corner[0] - 214 + 250), str(corner[1] - 21 + 124), found=True)
            xdo("click", "1")
            time.sleep(1)
        ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/actions_off.png"})
        check("disabled trigger and sub-action")
        pages = ["Chat", "Accounts", "Community", "Polls", "Actions", "Commands", "Timers", "Queues", "Groups", "Plugins",
                 "Servers", "Stream Apps", "Integrations", "Donations", "Globals", "History", "Settings", "MCP", "Log", "Crashes", "Backups"]
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
        # The Spotify request form keeps the main action, rules and replies in separate tabs.
        ws.ask({"request": "StreamerzUi", "id": "f", "do": "form 99802"})
        time.sleep(0.4)
        if "Cancel" not in ws.ask({"request": "StreamerzUi", "id": "f", "do": "find Cancel"}).get("result", ""):
            failures.append("the long Spotify form has no visible Cancel button")
        tree = ws.ask({"request": "StreamerzUi", "id": "t", "do": "tree"}).get("result", "")
        for label in ('"Action"', '"Rules"', '"Replies"', '"Skip own song"'):
            if label not in tree:
                failures.append(f"Spotify form: missing {label}")
        ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/spotify_form.png"})
        xdo("key", "Escape")
        # The Accounts page fills in (it's in a scroll: it once never did).
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Accounts"})
        if "Bot (optional)" not in json.dumps(ws.ask({"request": "StreamerzUi", "id": "f", "do": "find optional"})):
            failures.append("the Accounts page didn't fill in its accounts")
        # TikTok reads without setup; sending goes through the browser extension once it's set up.
        find = lambda text: json.dumps(ws.ask({"request": "StreamerzUi", "id": "f", "do": "find " + text}))
        def tiktok_chat():
            ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Chat"})
            ws.ask({"request": "StreamerzUi", "id": "v", "do": "chatview TikTok"})
            time.sleep(0.5)
            return find("set up sending")
        if "set up sending" not in tiktok_chat():
            failures.append("TikTok's chat doesn't say how to send before sending is set up")
        ws.ask({"request": "StreamerzTikTokBridge", "id": "tb", "action": "install"})
        ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Accounts"})
        time.sleep(0.5)
        if "Waiting for the browser extension" not in find("Waiting for the browser"):
            failures.append("Accounts doesn't show TikTok sending waiting for the extension")
        ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/tiktok_sending.png"})
        if "set up sending" in tiktok_chat():
            failures.append("TikTok's chat stays read-only once sending is set up")
        ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {OUT}/tiktok_chat_box.png"})
        ws.ask({"request": "StreamerzTikTokBridge", "id": "tb", "action": "remove"})
        if "set up sending" not in tiktok_chat():
            failures.append("TikTok's chat isn't read-only again after removing sending")
        ws.ask({"request": "StreamerzUi", "id": "together", "do": "chatview All together"})
        check("TikTok sending setup")
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
        # Hidden Community rows must release their presentation data and refill on return.
        count = int(ws.ask({"request": "StreamerzUi", "id": "count", "do": "community"})["result"])
        ws.ask({"request": "StreamerzUi", "id": "leave", "do": "page Chat"})
        if int(ws.ask({"request": "StreamerzUi", "id": "released", "do": "community"})["result"]) != 0:
            failures.append("Community kept its rows after leaving the page")
        ws.ask({"request": "StreamerzUi", "id": "return", "do": "page Community"})
        if int(ws.ask({"request": "StreamerzUi", "id": "refilled", "do": "community"})["result"]) != count:
            failures.append("Community did not refill its rows on return")
        # A queued chat refresh must not refill released rows after hide.
        ws.ask({"request": "InjectChat", "id": "late-chat", "platform": "kick", "user": "CleanupTest", "text": "before hide"})
        count += 1
        ws.ask({"request": "StreamerzUi", "id": "hide", "do": "toggle"})
        time.sleep(.5)
        if int(ws.ask({"request": "StreamerzUi", "id": "hidden", "do": "community"})["result"]) != 0:
            failures.append("Community kept its rows after hiding the window")
        ws.ask({"request": "StreamerzUi", "id": "show", "do": "toggle"})
        time.sleep(.5)
        if int(ws.ask({"request": "StreamerzUi", "id": "shown", "do": "community"})["result"]) != count:
            failures.append("Community did not refill its rows after showing the window")
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
        if WINE:
            # Windows ends a session with WM_ENDSESSION; Wine's wineboot -e sends it.
            subprocess.run(["wineboot", "-e"], env=env)
        else:
            app.terminate()
    # Asked to end (as a logout does), it saves and quits: the next start mustn't see a crash.
    try:
        app.wait(10)
    except subprocess.TimeoutExpired:
        failures.append("it didn't quit when asked (" + ("the session ending" if WINE else "SIGTERM") + ")")
        app.kill()
    if dev_dirs(app.pid):
        failures.append("quitting left the development settings directory behind")
    print("\n".join(failures) or f"ok: every page and the card, screenshots in {OUT}")
    if failures:
        print(f"the app's files (logs/crash-*.txt) are in {tmp}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
