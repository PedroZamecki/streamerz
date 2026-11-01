#!/usr/bin/env python3
"""Check concurrent development instances on the hidden display.

Run after cargo fbuild: env -u WAYLAND_DISPLAY DISPLAY=:99 python3 dev/isolation.py
"""
import glob
import json
import os
import socket
import subprocess
import tempfile
import time

with tempfile.TemporaryDirectory(prefix="streamerz-isolation-") as root:
    os.mkdir(os.path.join(root, "streamerz"))
    source = os.path.join(root, "streamerz", "streamerz.json")
    settings = json.dumps({"start_hidden": False, "websockets": {"port": 8080}}).encode()
    with open(source, "wb") as f:
        f.write(settings)
    env = dict(os.environ, XDG_CONFIG_HOME=root, DISPLAY=":99", STREAMERZ_DEV="1")
    env.pop("WAYLAND_DISPLAY", None)
    apps = []
    try:
        for _ in range(2):
            apps.append(subprocess.Popen(["target/fast/Streamerz"], env=env))
        paths = []
        for _ in range(100):
            paths = [glob.glob(os.path.join(tempfile.gettempdir(), f"streamerz-dev-{p.pid}-*", "streamerz.json")) for p in apps]
            if all(paths):
                try:
                    configs = [json.load(open(path[0])) for path in paths]
                    ports = [c["websockets"]["port"] for c in configs]
                    if len(set(ports)) == 2 and all(p.poll() is None for p in apps):
                        for port in ports:
                            with socket.create_connection(("127.0.0.1", port), timeout=1):
                                pass
                        break
                except (OSError, ValueError, KeyError):
                    pass
            time.sleep(0.1)
        else:
            raise AssertionError("two development instances did not start on separate ports")
        with open(source, "rb") as f:
            assert f.read() == settings, "development changed the global settings"
    finally:
        for app in apps:
            if app.poll() is None:
                app.terminate()
        for app in apps:
            app.wait(timeout=15)
            assert not glob.glob(os.path.join(tempfile.gettempdir(), f"streamerz-dev-{app.pid}-*")), "development directory survived normal exit"
print("ok: development instances use separate ports and clean up their settings")
