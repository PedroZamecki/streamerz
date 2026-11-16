#!/usr/bin/env python3
"""Open pages of the window on DISPLAY=:99 and save screenshots, for quick visual checks.

Usage: dev/ui-shot.py PAGE [PAGE...] [--bin target/fast/Streamerz] [--out target/ui-shots]
                        [--size 1100x800] [--theme dark|light] [--config JSON-FILE]
The window shot is the one of the app started here (found by pid), on the hidden display :99. A page is a navigation id ("Queues", "Stream Apps"). A page may be followed by ":STEPS" (separated by ;) run after it loads: a frontend command
or "click LABEL[#N]" (dblclick: twice) (the Nth match top to bottom), "wait SECONDS", "type TEXT" (into what has the focus), "at X Y" (a click there), "key NAME" (Return, Escape), for example "Queues:click Add". Each capture is PAGE-SIZE.png.
"""
import argparse
import glob
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
import locale

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness as smoke  # noqa: E402


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("pages", nargs="+")
    parser.add_argument("--bin", type=Path, default=Path("target/fast/Streamerz"))
    parser.add_argument("--out", type=Path, default=Path("target/ui-shots"))
    parser.add_argument("--size", default="1100x800")
    parser.add_argument("--theme", default="dark")
    parser.add_argument("--display", default=":99", help="the hidden display (never the real desktop)")
    parser.add_argument("--config", type=Path, help="extra settings merged into the fixture")
    parser.add_argument("--ask", action="append", default=[], help="a WebSocket request (JSON) sent once the app is up")
    parser.add_argument("--seed", type=Path, help="a folder copied next to the settings (backups, logs/crash-*)")
    parser.add_argument("--files", type=Path, help="a folder whose contents are copied next to the settings file (plugins/)")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    width, height = (int(part) for part in args.size.split("x"))
    with tempfile.TemporaryDirectory() as folder:
        config = {"start_hidden": False, "locale": "en_US", "theme": args.theme, "updates": "ignore",
                  "instance_name": "ui-shot", "websockets": {"port": smoke.PORT}}
        if args.config:
            config.update(json.loads(args.config.read_text()))
        home = Path(folder)
        home.mkdir(exist_ok=True)
        path = home / "fixture.json"
        path.write_text(json.dumps(config))
        if args.files:
            shutil.copytree(args.files, home, dirs_exist_ok=True)
        env = dict(os.environ, DISPLAY=args.display, STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(smoke.PORT),
                   XDG_CONFIG_HOME=folder)
        env.pop("WAYLAND_DISPLAY", None)
        log = (args.out / "run.log").open("w")
        app = subprocess.Popen([str(args.bin), "--config", str(path)], env=env, stdout=log, stderr=log)
        try:
            ws = None
            deadline = time.monotonic() + 30
            while ws is None and time.monotonic() < deadline:
                try:
                    ws = smoke.Ws(expected_name="ui-shot")
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
            def descendants(root):
                """The pids of a process and everything it started (the window belongs to the renderer child)."""
                found, queue = {str(root)}, [str(root)]
                while queue:
                    kids = subprocess.run(["pgrep", "-P", queue.pop()], capture_output=True, text=True).stdout.split()
                    queue.extend(kid for kid in kids if kid not in found)
                    found.update(kids)
                return found

            def own_window():
                """The visible window of the app started here, by pid: another Streamerz window on the display is ignored."""
                mine = descendants(app.pid)
                for candidate in subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "Streamerz"], env=env,
                                                capture_output=True, text=True).stdout.split():
                    owner = subprocess.run(["xdotool", "getwindowpid", candidate], env=env, capture_output=True,
                                           text=True).stdout.strip()
                    if owner in mine:
                        return candidate
                return None

            window = None
            for _ in range(50):
                window = own_window()
                if window:
                    break
                time.sleep(0.2)
            if window:
                subprocess.run(["xdotool", "windowsize", "--sync", window, str(width), str(height)], env=env)
                time.sleep(0.3)
                if f" {width}x{height} " not in ui("windows"):
                    # The window did not follow the X resize (a narrow one, or a bare display): ask the app itself.
                    ui(f"resize {width} {height}")
                    time.sleep(0.3)
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
                        window = own_window()
                        subprocess.run(["xdotool", "mousemove", *(["--window", window] if window else []), x, y, "click", "1"], env=env)
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
