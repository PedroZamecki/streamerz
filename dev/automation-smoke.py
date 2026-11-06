#!/usr/bin/env python3
"""Check automation forms on :99 using the app's UI metadata and window screenshots.

Usage: python3 -P dev/automation-smoke.py [target/fast/Streamerz]
Uses isolated settings, no accounts, no browser changes and no real chat sends.
"""
import importlib.util
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import tempfile
import time
import uuid


def main():
    root = Path(__file__).resolve().parent.parent
    spec = importlib.util.spec_from_file_location("smoke", root / "dev/smoke.py")
    smoke = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(smoke)
    binary = Path(sys.argv[1] if len(sys.argv) > 1 else root / "target/fast/Streamerz").resolve()
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        smoke.PORT = probe.getsockname()[1]
    output = Path(tempfile.mkdtemp(prefix="streamerz-automation-ui-"))
    with tempfile.TemporaryDirectory(prefix="streamerz-automation-settings-") as scratch:
        config = Path(scratch) / "streamerz"
        config.mkdir()
        name = "Streamerz automation " + uuid.uuid4().hex
        (config / "streamerz.json").write_text(json.dumps({
            "start_hidden": False, "locale": "en_US", "instance_name": name,
            "websockets": {"port": smoke.PORT}, "window": {"w": 1280, "h": 820},
        }))
        env = dict(os.environ, DISPLAY=":99", STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(smoke.PORT), XDG_CONFIG_HOME=scratch)
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("STREAMERZ_CONFIG", None)
        log = (output / "app.log").open("w")
        app = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)
        try:
            for attempt in range(150):
                try:
                    ws = smoke.Ws(name)
                    break
                except OSError:
                    if app.poll() is not None:
                        raise RuntimeError("app exited during startup")
                    time.sleep(.1)
            else:
                raise RuntimeError("app did not start")

            def ui(command):
                reply = ws.ask({"request": "StreamerzUi", "id": "automation", "do": command})
                assert reply.get("status") == "ok", reply
                return reply.get("result", "")

            def xdo(*args):
                subprocess.run(["xdotool", *map(str, args)], env=env, check=True)

            def click(label):
                found = ui("find " + label)
                points = re.findall(r" at (-?\d+),(-?\d+)", found)
                assert points, (label, found)
                x, y = points[-1]
                xdo("mousemove", x, y)
                xdo("click", 1)
                time.sleep(.25)

            def shot(label):
                path = output / (label + ".png")
                ui("shot " + str(path))
                assert path.is_file() and path.stat().st_size > 1000
                assert app.poll() is None

            def memory():
                return {line.split(':', 1)[0]: int(line.split()[1])
                        for line in Path(f"/proc/{app.pid}/smaps_rollup").read_text().splitlines()
                        if line.split()[0] in ("Rss:", "Private_Clean:", "Private_Dirty:")}

            time.sleep(.4)
            ui("toggle")
            time.sleep(.5)
            before_editors = memory()
            ui("toggle")
            time.sleep(.3)
            window = subprocess.check_output(["xdotool", "search", "--pid", str(app.pid)], env=env, text=True).splitlines()[0]
            for width, height, size in [(1280, 820, "wide"), (760, 620, "narrow")]:
                xdo("windowsize", window, width, height)
                time.sleep(.3)
                ui("form 41002 trigger")
                time.sleep(.35)
                tree = ui("tree")
                assert "TikTok: Gift" in ui("windows"), ui("windows")
                assert "Streamer.bot's docs" not in tree
                assert "Min coins" in tree and "Max coins" in tree
                shot("gift-" + size)
                click("Help")
                shot("gift-help-" + size)
                xdo("key", "Escape")
                time.sleep(.2)
                click("Test gift matching")
                shot("gift-sample-" + size)
                click("OK")
                assert "matches all conditions" in ui("tree")
                shot("gift-match-" + size)
                xdo("key", "Escape")
                time.sleep(.2)
                xdo("key", "Escape")
                time.sleep(.2)
                ui("form 41004 trigger")
                time.sleep(.3)
                assert "Nothing to set." in ui("tree")
                shot("no-settings-" + size)
                xdo("key", "Escape")
                time.sleep(.2)
                ui("form 1002")
                time.sleep(.3)
                shot("duration-" + size)
                xdo("key", "Escape")
                time.sleep(.2)
                ui("form 43001 values")
                time.sleep(.3)
                click("Insert value")
                shot("insert-value-menu-" + size)
                xdo("key", "Home", "Return")
                time.sleep(.2)
                shot("inserted-value-" + size)
                xdo("key", "Escape")
                time.sleep(.2)

            # A hidden, disconnected sample is separate from the connected version budget.
            if " shown" in ui("windows"):
                ui("toggle")
            assert " shown" not in ui("windows")
            time.sleep(.5)
            def ticks():
                fields = Path(f"/proc/{app.pid}/stat").read_text().split()
                return int(fields[13]) + int(fields[14])
            before, started = ticks(), time.monotonic()
            time.sleep(5)
            cpu = (ticks() - before) / os.sysconf("SC_CLK_TCK") / (time.monotonic() - started) * 100
            rss = next(int(line.split()[1]) for line in Path(f"/proc/{app.pid}/status").read_text().splitlines() if line.startswith("VmRSS:"))
            (output / "metrics.json").write_text(json.dumps({"state":"hidden, disconnected, after editor checks", "rssKiB":rss, "idleCpuPercentOneCore":round(cpu, 3), "beforeEditorsKiB":before_editors, "afterEditorsKiB":memory()}, indent=2))
            print("Automation UI checks passed. Screenshots and metrics:", output)
        finally:
            if app.poll() is None:
                app.terminate()
                app.wait(timeout=15)
            log.close()


if __name__ == "__main__":
    main()
