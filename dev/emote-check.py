#!/usr/bin/env python3
"""Checks third-party emotes in chat on the real app, and measures what they cost.

  dev/emote-check.py BINARY [hidden|visible]

It starts BINARY on the hidden display :99 (see dev/smoke.py) reading someone else's Twitch channel (xqc,
no login, as a viewer would), and prints its RSS and the CPU ticks (1/100 s) used in 30 s idle:
  hidden:  start hidden, nothing drawn (the budget: under 25 MB RSS, ~0 ticks).
  visible: chat open, 60 different chatters and two that matter: a channel emote (xqcWOT) sent by a
           stranger, and a chatter's own 7TV emotes (monkaS, myreact, cuh, sent by xqc's account id).
           Screenshot in /tmp/emotecheck/. Needs the network (7TV, the CDN) and xdotool.
"""
import importlib.util, json, os, subprocess, sys, tempfile, time

spec = importlib.util.spec_from_file_location("smoke", os.path.join(os.path.dirname(__file__), "smoke.py"))
smoke = importlib.util.module_from_spec(spec)
ARGS = sys.argv[1:]
sys.argv = sys.argv[:1]
spec.loader.exec_module(smoke)


def main(binary, mode):
    out = "/tmp/emotecheck"
    os.makedirs(out, exist_ok=True)
    tmp = tempfile.mkdtemp()
    os.makedirs(os.path.join(tmp, "streamerz"))
    port = smoke.PORT + 1
    cfg = {
        "start_hidden": mode == "hidden",
        "locale": "en_US",
        "websockets": {"port": port},
        "twitch": {"channel": "xqc", "channelId": "71092938"},
    }
    json.dump(cfg, open(os.path.join(tmp, "streamerz", "streamerz.json"), "w"))
    env = dict(smoke.env, XDG_CONFIG_HOME=tmp, STREAMERZ_DEV_PORT=str(port))
    smoke.PORT = port
    app = subprocess.Popen([binary], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def rss():
        for l in open(f"/proc/{app.pid}/status"):
            if l.startswith("VmRSS"):
                return int(l.split()[1]) / 1024

    def ticks():
        f = open(f"/proc/{app.pid}/stat").read().rsplit(")", 1)[1].split()
        return int(f[11]) + int(f[12])

    try:
        time.sleep(6)
        ws = None
        for _ in range(20):
            try:
                ws = smoke.Ws()
                break
            except OSError:
                time.sleep(0.5)
        if mode == "visible":
            ws.ask({"request": "SimulateChatConnection", "id": "c", "platform": "twitch", "connected": True})
            ws.ask({"request": "StreamerzUi", "id": "p", "do": "page Chat"})
            for i in range(60):
                ws.ask({"request": "InjectChat", "id": f"i{i}", "platform": "twitch", "user": f"viewer{i}",
                        "userId": str(900000 + i), "text": f"hello number {i}"})
            ws.ask({"request": "InjectChat", "id": "a", "platform": "twitch", "user": "stranger", "userId": "5",
                    "text": "channel emote xqcWOT here"})
            ws.ask({"request": "InjectChat", "id": "b", "platform": "twitch", "user": "xqc", "userId": "71092938",
                    "text": "personal monkaS myreact cuh here"})
            time.sleep(10)
            ws.ask({"request": "StreamerzUi", "id": "s", "do": f"shot {out}/{os.path.basename(binary)}.png"})
        time.sleep(4)
        r0, t0 = rss(), ticks()
        time.sleep(30)
        print(f"{os.path.basename(binary)} {mode}: RSS {r0:.1f} MB (after idle {rss():.1f}), idle CPU {ticks() - t0} ticks/30s")
    finally:
        app.kill()


if __name__ == "__main__":
    if not ARGS:
        sys.exit(__doc__)
    main(os.path.abspath(ARGS[0]), ARGS[1] if len(ARGS) > 1 else "hidden")
