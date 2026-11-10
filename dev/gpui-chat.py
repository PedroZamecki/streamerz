#!/usr/bin/env python3
"""Fill the GPUI Chat page with injected messages and save screenshots (DISPLAY=:99).

Usage: dev/gpui-chat.py [--bin target/fast/Streamerz] [--out target/gpui-shots] [--size 1100x800] [--theme dark]
Writes chat-SIZE.png (lines), chat-card-SIZE.png (a user card) and chat-options-SIZE.png (the options dialog).
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness as smoke  # noqa: E402

MESSAGES = [
    ("twitch", "ana_dev", "moderator", "Hello everyone, welcome to the stream Kappa"),
    ("kick", "bruno", "viewer", "this is a much longer message that has to wrap over more than one line of the chat list so the row grows and we can check how the layout copes with it"),
    ("youtube", "Carla", "subscriber", "oi gente, tudo bem? \U0001F600"),
    ("tiktok", "dani", "viewer", "first time here!"),
    ("twitch", "edu", "vip", "@ana_dev can you check the settings page?"),
    ("twitch", "fabi", "broadcaster", "!commands"),
    ("kick", "umana", "broadcaster", "@bruno welcome to my channel"),
    ("kick", "pedro", "moderator", "keep it civil please"),
    ("kick", "vera", "vip", "good game"),
    ("kick", "sol", "subscriber", "sub here"),
    ("youtube", "Dono", "broadcaster", "owner talking"),
    ("youtube", "Mod", "moderator", "mod talking"),
    ("youtube", "Mem", "subscriber", "member talking"),
    ("tiktok", "host", "broadcaster", "host talking"),
    ("tiktok", "mod", "moderator", "mod talking"),
]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", type=Path, default=Path("target/fast/Streamerz"))
    parser.add_argument("--out", type=Path, default=Path("target/gpui-shots"))
    parser.add_argument("--size", default="1100x800")
    parser.add_argument("--theme", default="dark")
    parser.add_argument("--view", default="", help="the Show choice: side, or a platform id (default: all together)")
    parser.add_argument("--down", default="", help="a platform whose chat is disconnected after its lines came")
    parser.add_argument("--fade", type=int, default=0, help="the fade in of new lines, in ms")
    parser.add_argument("--lines", type=int, default=0, help="extra lines to inject, to test scrolling")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    width, height = (int(part) for part in args.size.split("x"))
    with tempfile.TemporaryDirectory() as folder:
        config = {"start_hidden": False, "locale": "en_US", "theme": args.theme, "updates": "ignore",
                  "instance_name": "gpui-shot", "websockets": {"port": smoke.PORT},
                  "window": {"chat": {"saveHistory": True, "fadeIn": args.fade}, "chat_view": args.view}}
        path = Path(folder) / "fixture.json"
        path.write_text(json.dumps(config))
        env = dict(os.environ, DISPLAY=":99", STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(smoke.PORT), XDG_CONFIG_HOME=folder)
        env.pop("WAYLAND_DISPLAY", None)
        log = (args.out / "chat-run.log").open("w")
        app = subprocess.Popen([str(args.bin), "--config", str(path)], env=env, stdout=log, stderr=log)
        try:
            ws = None
            deadline = time.monotonic() + 30
            while ws is None and time.monotonic() < deadline:
                try:
                    ws = smoke.Ws(expected_name="gpui-shot")
                except OSError:
                    time.sleep(0.2)
            if ws is None:
                raise SystemExit("the app did not start; see " + str(args.out / "chat-run.log"))

            def ui(command):
                reply = ws.ask({"request": "StreamerzUi", "id": "shot", "do": command})
                return reply.get("result", reply.get("error", ""))

            for platform in ("twitch", "kick", "youtube", "tiktok"):
                ws.ask({"request": "SimulateChatConnection", "platform": platform, "connected": True, "id": "c"})
            if args.view:
                ws.ask({"request": "StreamerzChat", "op": "view", "view": args.view, "id": "v"})
            # Emotes of several shapes, in the picture cache under made-up addresses (nothing is downloaded).
            from PIL import Image, ImageDraw
            import hashlib
            shapes = {"WIDE": (112, 28, (220, 60, 60)), "SQUARE": (56, 56, (60, 200, 90)), "TALL": (28, 56, (70, 120, 240))}
            pairs = []
            import glob
            # A dev run keeps its settings and cache in its own temporary folder: the newest one is ours.
            dev_dirs = sorted(glob.glob(os.path.join(tempfile.gettempdir(), "streamerz-dev-*")), key=os.path.getmtime)
            for name, (w, h, colour) in shapes.items():
                url = f"https://emotes.test/{name}.png"
                folder_img = Path(dev_dirs[-1] if dev_dirs else folder) / "cache" / "img"
                folder_img.mkdir(parents=True, exist_ok=True)
                image = Image.new("RGBA", (w, h), colour + (255,))
                ImageDraw.Draw(image).rectangle([0, 0, w - 1, h - 1], outline=(255, 255, 255, 255))
                image.save(folder_img / (hashlib.sha256(url.encode()).hexdigest()[:24] + ".png"))
                pairs.append([name, url])
            ws.ask({"request": "InjectChat", "platform": "twitch", "user": "emoter", "role": "viewer",
                    "text": "look WIDE then SQUARE and TALL end", "emotes": pairs, "id": "e1"})
            ws.ask({"request": "InjectChat", "platform": "twitch", "user": "giant", "role": "viewer",
                    "text": "big SQUARE", "emotes": pairs, "args": {"rewardType": "gigantify_an_emote"}, "id": "e2"})
            ws.ask({"request": "InjectChat", "platform": "twitch", "user": "giantwide", "role": "viewer",
                    "text": "bigger WIDE", "emotes": pairs, "args": {"rewardType": "gigantify_an_emote"}, "id": "e3"})
            for platform, user, role, text in MESSAGES:
                ws.ask({"request": "InjectChat", "platform": platform, "user": user, "role": role, "text": text, "id": "m"})
            for number in range(args.lines):
                ws.ask({"request": "InjectChat", "platform": "twitch", "user": f"user{number % 7}", "role": "viewer",
                        "text": f"message number {number}", "id": "m"})
            time.sleep(2)
            window = None
            for _ in range(50):
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "Streamerz"], env=env,
                                       capture_output=True, text=True).stdout.split()
                if found:
                    window = found[0]
                    break
                time.sleep(0.2)
            if window:
                subprocess.run(["xdotool", "windowsize", "--sync", window, str(width), str(height)], env=env)
            print("page", ui("page Chat"))
            if args.down:
                ws.ask({"request": "SimulateChatConnection", "platform": args.down, "connected": False, "id": "d"})
            for _ in range(100):
                if ui("pageready") == "true":
                    break
                time.sleep(0.05)
            time.sleep(1)
            print(ui("shot " + str(args.out / f"chat-{args.size}.png")))
            # A message that arrives while the page is open streams in.
            ws.ask({"request": "InjectChat", "platform": "kick", "user": "gabi", "role": "viewer", "text": "streamed in live", "id": "m"})
            if args.fade:
                time.sleep(args.fade / 4000)
                print(ui("shot " + str(args.out / f"chat-fading-{args.size}.png")))
            time.sleep(max(1, args.fade / 1000 + 0.5))
            print(ui("shot " + str(args.out / f"chat-live-{args.size}.png")))
            # Hover: an emote shows its picture large; a line shows its actions.
            options = json.loads(ui("bounds Options"))
            ox = round(options[0]["x"] + options[0]["w"] / 2 - 947) if options else 0
            oy = round(options[0]["y"] + options[0]["h"] / 2 - 22) if options else 0
            for name, point in (("emote", (372, 62)), ("line", (700, 140)), ("badge", (290, 140))):
                subprocess.run(["xdotool", "mousemove", str(point[0] + ox), str(point[1] + oy)], env=env)
                time.sleep(1.3)
                print(ui("shot " + str(args.out / f"chat-hover-{name}-{args.size}.png")))
            page = ws.ask({"request": "StreamerzCoreView", "domain": "chat", "id": "p"})
            first = page["chat"]["lines"][0]["seq"]
            print("card op:", json.dumps(ws.ask({"request": "StreamerzChat", "op": "card", "seq": first, "id": "k"}))[:300])
            # Click the first name: its card opens (the first line is Twitch's ana_dev).
            options = json.loads(ui("bounds Options"))
            dx = round(options[0]["x"] + options[0]["w"] / 2 - 947) if options else 0
            dy = round(options[0]["y"] + options[0]["h"] / 2 - 22) if options else 0
            names = [e for e in json.loads(ui("bounds ana_dev:")) if e.get("label", "").startswith("ana_dev")]
            if names:
                at = (round(names[0]["x"] + names[0]["w"] / 2), round(names[0]["y"] + names[0]["h"] / 2))
            else:
                at = (320 + dx, 62 + dy)
            subprocess.run(["xdotool", "mousemove", str(at[0]), str(at[1]), "click", "1"], env=env)
            time.sleep(2)
            print(ui("shot " + str(args.out / f"chat-card-{args.size}.png")))
            # Details swaps what they said for what actions get.
            more = [e for e in json.loads(ui("bounds Details")) if e["label"] == "Details" and e.get("enabled", True)]
            if more:
                subprocess.run(["xdotool", "mousemove", str(round(more[0]["x"] + more[0]["w"] / 2)), str(round(more[0]["y"] + more[0]["h"] / 2)), "click", "1"], env=env)
                time.sleep(0.5)
                print(ui("shot " + str(args.out / f"chat-card-details-{args.size}.png")))
            subprocess.run(["xdotool", "key", "Escape"], env=env)
            time.sleep(0.5)
            for _ in range(20):
                if json.loads(ui("bounds Options")):
                    break
                time.sleep(0.25)
            for label, name in (("Options", "chat-options"),):
                found = [e for e in json.loads(ui("bounds " + label)) if e["label"] == label and e.get("enabled", True)]
                if found:
                    e = found[0]
                    subprocess.run(["xdotool", "mousemove", str(round(e["x"] + e["w"] / 2)), str(round(e["y"] + e["h"] / 2)), "click", "1"], env=env)
                    time.sleep(1)
                    print(ui("shot " + str(args.out / f"{name}-{args.size}.png")))
        finally:
            app.terminate()
            try:
                app.wait(timeout=5)
            except subprocess.TimeoutExpired:
                app.kill()


if __name__ == "__main__":
    main()
