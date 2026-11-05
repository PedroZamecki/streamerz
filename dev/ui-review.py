#!/usr/bin/env python3
"""Capture every page at review sizes using isolated settings and the app's own UI tool.

Run UI input checks serially on DISPLAY=:99. This never reads the user's configuration.
  python3 dev/ui-review.py [binary] --out /tmp/streamerz-ui-current
"""
import sys

# Load the standard locale module before this folder's locale.py can shadow it.
_script_path = sys.path.pop(0)
import locale
sys.path.insert(0, _script_path)

import argparse
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import tempfile
import time
import uuid

import smoke


PAGES = ("Chat", "Accounts", "Community", "Polls", "Actions", "Commands", "Timers", "Files",
         "Queues", "Groups", "Plugins", "Servers", "Stream Apps", "Integrations", "Donations",
         "Globals", "History", "Settings", "MCP", "Log", "Crashes", "Backups")
SIZES = ((1100, 800), (540, 700))


def capture(binary, out, port, theme, pages=PAGES, history=True):
    smoke.require_free_port(port)
    instance_name = "Streamerz UI review " + uuid.uuid4().hex
    with tempfile.TemporaryDirectory(prefix="streamerz-ui-review-") as config:
        folder = Path(config) / "streamerz"
        folder.mkdir()
        (folder / "streamerz.json").write_text(json.dumps({
            "start_hidden": False, "locale": "en_US", "theme": theme, "instance_name": instance_name,
            "websockets": {"port": port},
            "window": {"chat": {"saveHistory": history}},
        }))
        env = dict(os.environ, DISPLAY=":99", STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(port),
                   XDG_CONFIG_HOME=config)
        env.pop("WAYLAND_DISPLAY", None)
        app = subprocess.Popen([str(binary)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        ws = None
        try:
            smoke.PORT = port
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if app.poll() is not None:
                    raise RuntimeError(f"app exited: {app.returncode}")
                try:
                    ws = smoke.Ws(expected_name=instance_name)
                    break
                except OSError:
                    time.sleep(.2)
            if ws is None:
                raise RuntimeError("cannot reach the isolated development instance")

            def ui(command):
                answer = ws.ask({"request": "StreamerzUi", "id": "review", "do": command})
                return answer.get("result", "")

            window = subprocess.check_output([
                "xdotool", "search", "--onlyvisible", "--pid", str(app.pid), "--name", "Streamerz"
            ], env=env, text=True).splitlines()[0]
            for width, height in SIZES:
                subprocess.run(["xdotool", "windowsize", "--sync", window, str(width), str(height)],
                               env=env, check=True)
                time.sleep(.2)
                for page in pages:
                    ui("page " + page)
                    time.sleep(.08)
                    stem = f"{theme}-{page.lower().replace(' ', '_')}-{width}x{height}"
                    shot = out / (stem + ".png")
                    result = ui("shot " + str(shot))
                    if "saved " not in result:
                        raise RuntimeError(f"{page}: {result}")
                    actual = struct.unpack(">II", shot.read_bytes()[16:24])
                    if actual != (width, height):
                        raise RuntimeError(f"{page}: expected {width}x{height}, got {actual}")
                    tree = ui("tree")
                    (out / (stem + ".txt")).write_text(tree)
                    if page == "Chat" and not history:
                        if not re.search(r'"\s*Message all"[^\n]*\(disabled\)', tree):
                            raise RuntimeError("Message all is available while chat history is off")
            if "Actions" in pages:
                # Add data only to the throwaway instance, never to production settings.
                ws.ask({"request": "StreamerzSave", "what": "action", "item": {
                    "name": "Welcome message", "enabled": True,
                    "triggers": [{"type": 16001, "keyCode": 68, "modifierMask": 6, "enabled": False}],
                    "subActions": [{"type": 99001, "enabled": False}],
                }})
                for width, height in SIZES:
                    subprocess.run(["xdotool", "windowsize", "--sync", window, str(width), str(height)],
                                   env=env, check=True)
                    ui("page Actions")
                    time.sleep(.2)
                    found = ui("find Filter actions")
                    point = re.search(r" at (\d+),(\d+)", found)
                    if point is None:
                        raise RuntimeError("cannot find the Actions search field")
                    subprocess.run(["xdotool", "mousemove", *point.groups(), "click", "1", "key", "Tab", "space"],
                                   env=env, check=True)
                    time.sleep(.1)
                    if "Welcome message" not in ui("find Welcome"):
                        raise RuntimeError("keyboard selection did not open the first action")
                    ui(f"shot {out}/{theme}-actions-populated-{width}x{height}.png")
            ui("toggle")
            time.sleep(3)
            stat = Path(f"/proc/{app.pid}/stat")
            def ticks():
                fields = stat.read_text().split()
                return int(fields[13]) + int(fields[14])
            start, began = ticks(), time.monotonic()
            time.sleep(5)
            elapsed = time.monotonic() - began
            cpu = 100 * (ticks() - start) / os.sysconf("SC_CLK_TCK") / elapsed
            rss = next(line.split()[1] for line in Path(f"/proc/{app.pid}/status").read_text().splitlines()
                       if line.startswith("VmRSS:"))
            return {"theme": theme, "hidden_rss_kib": int(rss), "idle_cpu_percent_one_core": cpu,
                    "sample_seconds": elapsed, "binary": str(binary),
                    "chat_history": history, "setup": "isolated development instance, no live connections"}
        finally:
            if ws is not None:
                ws.s.close()
            app.terminate()
            try:
                app.wait(10)
            except subprocess.TimeoutExpired:
                app.kill()
                app.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", default="target/fast/Streamerz")
    parser.add_argument("--out", type=Path, default=Path("/tmp/streamerz-ui-current"))
    parser.add_argument("--port", type=int, default=8097)
    parser.add_argument("--pages", nargs="+", choices=PAGES, default=PAGES)
    parser.add_argument("--history-off", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    results = [capture(Path(args.binary).resolve(), out, args.port, theme, args.pages, not args.history_off) for theme in ("dark", "light")]
    (out / "measurements.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"Captured {len(args.pages)} pages in both themes and sizes: {out}")


if __name__ == "__main__":
    main()
