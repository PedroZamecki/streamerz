#!/usr/bin/env python3
"""Open pages of the GPUI frontend on DISPLAY=:99 and save screenshots, for quick visual checks.

Usage: dev/gpui-shot.py PAGE [PAGE...] [--bin target/fast/Streamerz] [--out target/gpui-shots]
                        [--size 1100x800] [--theme dark|light] [--config JSON-FILE]
A page is a navigation id ("Queues", "Stream Apps"). A page may be followed by ":STEPS" (separated by ;) run after it loads: a frontend command
or "click LABEL[#N]" (dblclick: twice) (the Nth match top to bottom), "wait SECONDS", "type TEXT" (into what has the focus), "at X Y" (a click there), "key NAME" (Return, Escape), for example "Queues:click Add". Each capture is PAGE-SIZE.png.
"""
import argparse
import glob
import json
import os
import shutil
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness as smoke  # noqa: E402


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("pages", nargs="+")
    parser.add_argument("--bin", type=Path, default=Path("target/fast/Streamerz"))
    parser.add_argument("--out", type=Path, default=Path("target/gpui-shots"))
    parser.add_argument("--size", default="1100x800")
    parser.add_argument("--theme", default="dark")
    parser.add_argument("--config", type=Path, help="extra settings merged into the fixture")
    parser.add_argument("--ask", action="append", default=[], help="a WebSocket request (JSON) sent once the app is up")
    parser.add_argument("--seed", type=Path, help="a folder copied next to the settings (backups, logs/crash-*)")
    parser.add_argument("--files", type=Path, help="a folder whose contents are copied next to the settings file (plugins/)")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    width, height = (int(part) for part in args.size.split("x"))
    with tempfile.TemporaryDirectory() as folder:
        config = {"start_hidden": False, "locale": "en_US", "theme": args.theme, "updates": "ignore",
                  "instance_name": "gpui-shot", "websockets": {"port": smoke.PORT}}
        if args.config:
            config.update(json.loads(args.config.read_text()))
        home = Path(folder)
        home.mkdir(exist_ok=True)
        path = home / "fixture.json"
        path.write_text(json.dumps(config))
        if args.files:
            shutil.copytree(args.files, home, dirs_exist_ok=True)
        env = dict(os.environ, DISPLAY=os.environ.get("DISPLAY", ":99"), STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(smoke.PORT),
                   XDG_CONFIG_HOME=folder)
        env.pop("WAYLAND_DISPLAY", None)
        log = (args.out / "run.log").open("w")
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
                raise SystemExit("the app did not start; see " + str(args.out / "run.log"))

            def ui(command):
                reply = ws.ask({"request": "StreamerzUi", "id": "shot", "do": command})
                return reply.get("result", reply.get("error", ""))

            if args.seed:
                # A dev instance works on its own copy of the settings: seed the folder it made.
                for _ in range(50):
                    runs = glob.glob(f"/tmp/streamerz-dev-{app.pid}-*")
                    if runs:
                        shutil.copytree(args.seed, runs[0], dirs_exist_ok=True)
                        break
                    time.sleep(0.1)
            for request in args.ask:
                print("ask", ws.ask(json.loads(request)))
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
            else:
                print("no window found; capturing at the default size")
            for entry in args.pages:
                page, _, command = entry.partition(":")
                print(page, ui("page " + page))
                for _ in range(100):
                    if ui("pageready") == "true":
                        break
                    time.sleep(0.05)
                for step in filter(None, (part.strip() for part in command.split(";"))):
                    if step.startswith("wait "):
                        time.sleep(float(step[5:]))
                    elif step.startswith("type "):
                        subprocess.run(["xdotool", "type", "--delay", "40", step[5:]], env=env)
                    elif step.startswith("at "):
                        # A click at window coordinates, for what has no label to find (the status line).
                        x, y = step[3:].split()
                        window = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "Streamerz"],
                                                env=env, capture_output=True, text=True).stdout.split()[:1]
                        subprocess.run(["xdotool", "mousemove", *(["--window", *window] if window else []), x, y, "click", "1"], env=env)
                    elif step.startswith("key "):
                        subprocess.run(["xdotool", "key", step[4:]], env=env)
                    elif step.startswith(("click ", "dblclick ")):
                        double = step.startswith("dblclick ")
                        label, _, nth = step.split(" ", 1)[1].partition("#")
                        found = [e for e in json.loads(ui("bounds " + label)) if e["label"] == label and e.get("enabled", True)]
                        found.sort(key=lambda e: (e["y"], e["x"]))
                        if len(found) <= int(nth or 0):
                            print("   no control", label)
                            continue
                        e = found[int(nth or 0)]
                        x, y = round(e["x"] + e["w"] / 2), round(e["y"] + e["h"] / 2)
                        subprocess.run(["xdotool", "mousemove", str(x), str(y), "click", *(["--repeat", "2", "--delay", "80"] if double else []), "1"], env=env)
                        print("   clicked", label, "at", x, y)
                    else:
                        print("  ", step, "->", ui(step))
                    time.sleep(0.5)
                name = page.lower().replace(" ", "_") + ("-" + command.replace(" ", "_").replace(";", "-").replace("/", "-") if command else "")
                target = args.out / f"{name}-{args.size}.png"
                print("  ", ui("shot " + str(target)))
        finally:
            app.terminate()
            try:
                app.wait(timeout=5)
            except subprocess.TimeoutExpired:
                app.kill()


if __name__ == "__main__":
    main()
