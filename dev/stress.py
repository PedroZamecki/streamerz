#!/usr/bin/env python3
"""Reads the busiest public chats for a while and compares what Streamerz delivers with what the platforms say.

  ONLY=kick,twitch HIDDEN=1 dev/stress.py [seconds] [path to the Streamerz binary]     (defaults: 300, target/fast/Streamerz)

Streamerz reads Twitch, Kick, YouTube and TikTok without a login. Meanwhile two independent readers count the
same chats straight from Twitch (anonymous IRC) and Kick (the pusher socket): the truth. Streamerz's count is
what its WebSocket server sends as ChatMessage events (what triggers and clients get) and the lines its chat
page took in. Its memory and CPU are sampled all along. It prints a table; nothing is written to the repo.
HIDDEN=1 starts it hidden (the memory budget is for that). Needs the hidden display (:99) like dev/smoke.py.
"""
import base64, json, os, socket, ssl, struct, subprocess, sys, tempfile, threading, time

SECS = int(sys.argv[1]) if len(sys.argv) > 1 else 300
BIN = sys.argv[2] if len(sys.argv) > 2 else "target/fast/Streamerz"
PORT = 8096
ROOMS = {"twitch": "pgl", "kick": "cry", "youtube": "Gg6mV8Fv4og", "tiktok": "@.z_o_n_g_a"}
ONLY = [p for p in os.environ.get("ONLY", "").split(",") if p]
if ONLY:
    ROOMS = {p: r for p, r in ROOMS.items() if p in ONLY}
env = dict(os.environ, DISPLAY=":99", STREAMERZ_DEV="1")
env.pop("WAYLAND_DISPLAY", None)


class Ws:
    """A bare WebSocket client (text frames), plain or over TLS."""

    def __init__(self, host, port, path="/", tls=False, timeout=30):
        s = socket.create_connection((host, port), timeout=timeout)
        if tls:
            s = ssl.create_default_context().wrap_socket(s, server_hostname=host)
        self.s, self.lock = s, threading.Lock()
        key = base64.b64encode(os.urandom(16)).decode()
        s.sendall(f"GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                  f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += s.recv(1)

    def _n(self, n):
        d = b""
        while len(d) < n:
            c = self.s.recv(n - len(d))
            if not c:
                raise EOFError
            d += c
        return d

    def send(self, text, op=1):
        data = text.encode() if isinstance(text, str) else text
        mask = os.urandom(4)
        hdr = bytes([0x80 | op])
        n = len(data)
        hdr += bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack(">H", n)
        with self.lock:
            self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def read(self):
        """The next text message (pings are answered)."""
        while True:
            h = self._n(2)
            op, n = h[0] & 15, h[1] & 127
            if n == 126:
                n = struct.unpack(">H", self._n(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._n(8))[0]
            data = self._n(n)
            if op == 9:
                self.send(data, 10)
            elif op == 8:
                raise EOFError
            elif op in (1, 2):
                return data.decode(errors="replace")


truth = {"twitch": 0, "kick": 0}
got = {p: 0 for p in ROOMS}
stop = threading.Event()


def twitch_truth():
    while not stop.is_set():
        try:
            w = Ws("irc-ws.chat.twitch.tv", 443, "/", True)
            w.send("CAP REQ :twitch.tv/tags twitch.tv/commands")
            w.send("NICK justinfan12345")
            w.send(f"JOIN #{ROOMS['twitch']}")
            while not stop.is_set():
                for line in w.read().split("\r\n"):
                    if " PRIVMSG #" in line:
                        truth["twitch"] += 1
                    elif line.startswith("PING"):
                        w.send("PONG :tmi.twitch.tv")
        except (OSError, EOFError):
            time.sleep(2)


def kick_truth(chatroom):
    url = "ws-us2.pusher.com"
    while not stop.is_set():
        try:
            w = Ws(url, 443, "/app/34bf7a0ff419a2a775b9?protocol=7&client=js&version=7.6.0&flash=false", True)
            w.send(json.dumps({"event": "pusher:subscribe", "data": {"auth": "", "channel": f"chatrooms.{chatroom}.v2"}}))
            while not stop.is_set():
                m = json.loads(w.read())
                if m.get("event", "").endswith("ChatMessageEvent"):
                    truth["kick"] += 1
                elif m.get("event") == "pusher:ping":
                    w.send(json.dumps({"event": "pusher:pong", "data": {}}))
        except (OSError, EOFError, ValueError):
            time.sleep(2)


def sample(pid):
    """(resident MB, CPU seconds) of the process."""
    with open(f"/proc/{pid}/statm") as f:
        rss = int(f.read().split()[1]) * os.sysconf("SC_PAGE_SIZE") / 1e6
    with open(f"/proc/{pid}/stat") as f:
        t = f.read().rsplit(")", 1)[1].split()
    return rss, (int(t[11]) + int(t[12])) / os.sysconf("SC_CLK_TCK")


def main():
    tmp = tempfile.mkdtemp()
    cfg = os.path.join(tmp, "s.json")
    json.dump({
        "start_hidden": bool(os.environ.get("HIDDEN")), "locale": "en_US", "websockets": {"port": PORT},
        "twitch": {"channel": ROOMS.get("twitch", ""), "manualConnect": "twitch" not in ROOMS},
        "kick": {"channel": ROOMS.get("kick", ""), "manualConnect": "kick" not in ROOMS},
        "youtube": {"channelId": "@InstitutoConhecimentoLiberta", "videoId": ROOMS.get("youtube", ""),
                    "manualConnect": "youtube" not in ROOMS},
        "tiktok": {"channel": ROOMS.get("tiktok", ""), "manualConnect": "tiktok" not in ROOMS},
    }, open(cfg, "w"))
    env["STREAMERZ_CONFIG"] = cfg
    app = subprocess.Popen([BIN], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        ws = None
        for _ in range(40):
            try:
                ws = Ws("127.0.0.1", PORT)
                break
            except OSError:
                time.sleep(0.5)
        if not ws:
            sys.exit("can't reach the app")
        ws.read()  # Hello
        ws.send(json.dumps({"request": "Subscribe", "id": "s", "events": {
            "Twitch": ["ChatMessage"], "Kick": ["ChatMessage"], "YouTube": ["Message"], "TikTok": ["Chat"]}}))

        def count():
            while not stop.is_set():
                try:
                    m = json.loads(ws.read())
                except (OSError, EOFError, ValueError):
                    return
                ev = m.get("event") or {}
                src, typ = str(ev.get("source", "")).lower(), ev.get("type", "")
                if src in got and typ in ("ChatMessage", "Message", "Chat"):
                    got[src] += 1

        threading.Thread(target=count, daemon=True).start()
        if "twitch" in ROOMS:
            threading.Thread(target=twitch_truth, daemon=True).start()
        # Kick's reader needs the chatroom id Streamerz looks up (it is saved in its config).
        chatroom = None
        for _ in range(60 if "kick" in ROOMS else 0):
            try:
                chatroom = json.load(open(cfg)).get("kick", {}).get("chatroomId")
            except (OSError, ValueError):
                pass
            if chatroom:
                break
            time.sleep(1)
        if chatroom:
            threading.Thread(target=kick_truth, args=(chatroom,), daemon=True).start()

        time.sleep(20)  # let every chat come up before the count starts
        start = {p: got[p] for p in got}
        t0_truth = dict(truth)
        r0, c0 = sample(app.pid)
        t0 = time.time()
        peak, rows = r0, []
        while time.time() - t0 < SECS:
            time.sleep(10)
            r, c = sample(app.pid)
            peak = max(peak, r)
            rows.append((time.time() - t0, r, (c - c0) / (time.time() - t0) * 100))
        r1, c1 = sample(app.pid)
        el = time.time() - t0
        print(f"\nRead {el:.0f} s. Streamerz: {r1:.1f} MB now, {peak:.1f} MB peak, "
              f"{(c1 - c0) / el * 100:.1f}% of a core on average\n")
        print(f"{'platform':9} {'room':28} {'truth':>7} {'streamerz':>10} {'missing':>8}")
        for p in ROOMS:
            mine = got[p] - start[p]
            if p in truth:
                t = truth[p] - t0_truth[p]
                print(f"{p:9} {ROOMS[p]:28} {t:7} {mine:10} {t - mine:8}")
            else:
                print(f"{p:9} {ROOMS[p]:28} {'n/a':>7} {mine:10} {'':>8}")
        print("\nmemory and CPU over time (s, MB, % of a core since start):")
        for t, r, c in rows[:: max(1, len(rows) // 12)]:
            print(f"  {t:6.0f}  {r:6.1f}  {c:5.1f}")
    finally:
        stop.set()
        app.terminate()
        subprocess.run(["pkill", "-x", "Streamerz"], check=False)


main()
