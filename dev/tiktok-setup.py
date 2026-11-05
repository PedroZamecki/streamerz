#!/usr/bin/env python3
"""Verify TikTok setup with an isolated Chromium profile on DISPLAY=:99.

Usage: python3 dev/tiktok-setup.py [path/to/Streamerz]
Requires a dev build, Chromium, Xvfb and xdotool. Never uses the user's browser profile.
Screenshots are saved in /tmp/streamerz-setup-shots. No messages are sent.
"""
import sys
# Keep dev/locale.py from shadowing the standard library module.
sys.path = [p for p in sys.path if p != __file__.rsplit("/", 1)[0]]
import base64
import json
import os
import re
import shutil
import signal
import socket
import struct
import subprocess
import time
import tempfile
import urllib.parse
import uuid

DISPLAY = ":99"
PORT = 8096
BIN = sys.argv[1] if len(sys.argv) > 1 else "target/fast/Streamerz"
ROOT = tempfile.mkdtemp(prefix="streamerz-tiktok-setup-")
SHOT_DIR = "/tmp/streamerz-setup-shots"


class Ws:
    def __init__(self, port):
        self.s = socket.create_connection(("127.0.0.1", port), timeout=15)
        key = base64.b64encode(os.urandom(16)).decode()
        req = (
            f"GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n"
            f"Upgrade: websocket\r\nConnection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
        self.s.sendall(req.encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            ch = self.s.recv(1)
            if not ch:
                raise RuntimeError("EOF during WS handshake")
            buf += ch
        self.hello = self.read()

    def read(self):
        while True:
            h = self.s.recv(2)
            if len(h) < 2:
                raise RuntimeError("EOF reading frame")
            opcode = h[0] & 0x0F
            n = h[1] & 127
            if n == 126:
                n = struct.unpack(">H", self.s.recv(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self.s.recv(8))[0]
            data = b""
            while len(data) < n:
                chunk = self.s.recv(n - len(data))
                if not chunk:
                    raise RuntimeError("EOF reading payload")
                data += chunk
            if opcode == 0x8:
                raise RuntimeError("WS closed")
            if opcode in (0x9, 0xA):
                continue
            msg = json.loads(data)
            # Ignore async broadcast events while waiting for request replies
            if "event" in msg and "id" not in msg and "request" not in msg:
                continue
            return msg

    def ask(self, req):
        data = json.dumps(req).encode()
        mask = os.urandom(4)
        hdr = bytes([0x81])
        if len(data) < 126:
            hdr += bytes([0x80 | len(data)])
        else:
            hdr += bytes([0x80 | 126]) + struct.pack(">H", len(data))
        self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))
        while True:
            msg = self.read()
            if msg.get("id") == req.get("id"):
                return msg


def kill_pid(pid):
    if not pid:
        return
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    for _ in range(30):
        try:
            os.kill(pid, 0)
            time.sleep(0.1)
        except ProcessLookupError:
            return
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def chromium_main_pids(profile_dir):
    out = subprocess.run(["pgrep", "-f", profile_dir], capture_output=True, text=True).stdout.split()
    pids = []
    for p in out:
        try:
            full = open(f"/proc/{p}/cmdline", "rb").read().decode("utf-8", "replace").replace("\0", " ")
        except OSError:
            continue
        if "/usr/lib/chromium/chromium" in full and "--type=" not in full and "crashpad_handler" not in full:
            pids.append(int(p))
    return pids


def x11_chromium_windows(env):
    r = subprocess.run(
        ["xdotool", "search", "--onlyvisible", "--name", "Chromium"],
        env=env,
        capture_output=True,
        text=True,
    )
    titles = []
    for wid in r.stdout.split():
        name = subprocess.run(
            ["xdotool", "getwindowname", wid], env=env, capture_output=True, text=True
        ).stdout.strip()
        if name:
            titles.append((wid, name))
    return titles


def main():
    with socket.socket() as probe:
        probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        probe.bind(("127.0.0.1", PORT))
    os.makedirs(SHOT_DIR, exist_ok=True)
    bin_dir = os.path.join(ROOT, "bin")
    cfg_dir = os.path.join(ROOT, "config")
    cache_dir = os.path.join(ROOT, "cache")
    browser_dir = os.path.join(ROOT, "browser")
    for d in (bin_dir, os.path.join(cfg_dir, "streamerz"), cache_dir, browser_dir):
        os.makedirs(d, exist_ok=True)

    # Create isolated PATH with only essential X11/system tools + our Chromium X11 wrapper (hiding other browsers if any)
    shim = os.path.join(bin_dir, "chromium")
    with open(shim, "w") as f:
        f.write(
            "#!/bin/sh\n"
            "export TMPDIR=/tmp\n"
            "exec /usr/lib/chromium/chromium --ozone-platform=x11 "
            "--no-first-run --no-default-browser-check --password-store=basic \"$@\"\n"
        )
    os.chmod(shim, 0o755)

    instance_name = "Streamerz TT E2E " + uuid.uuid4().hex[:8]
    with open(os.path.join(cfg_dir, "streamerz", "streamerz.json"), "w") as f:
        json.dump(
            {
                "start_hidden": False,
                "locale": "en_US",
                "instance_name": instance_name,
                "websockets": {"port": PORT, "auto_start": True},
            },
            f,
        )

    env = dict(os.environ)
    env.pop("WAYLAND_DISPLAY", None)
    env["TMPDIR"] = "/tmp"
    env["DISPLAY"] = DISPLAY
    env["PATH"] = f"{bin_dir}:/usr/bin:/bin"
    env["XDG_CONFIG_HOME"] = cfg_dir
    env["XDG_CACHE_HOME"] = cache_dir
    env["CHROME_USER_DATA_DIR"] = browser_dir
    env["STREAMERZ_DEV"] = "1"
    env["STREAMERZ_DEV_PORT"] = str(PORT)

    xvfb = None
    wm = None
    app = None
    try:
        with socket.socket(socket.AF_UNIX) as display_probe:
            display_running = display_probe.connect_ex("/tmp/.X11-unix/X99") == 0
        if not display_running:
            xvfb = subprocess.Popen(
                ["/usr/bin/Xvfb", DISPLAY, "-screen", "0", "1280x900x24", "-nolisten", "tcp"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            time.sleep(0.6)
            assert xvfb.poll() is None, "Could not start Xvfb"
        if xvfb is not None and os.path.exists("/usr/bin/openbox"):
            wm = subprocess.Popen(["/usr/bin/openbox"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            time.sleep(0.4)

        app = subprocess.Popen([BIN], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        ws = None
        for _ in range(40):
            if app.poll() is not None:
                raise RuntimeError(f"Streamerz exited early: {app.stdout.read()}")
            try:
                ws = Ws(PORT)
                break
            except OSError:
                time.sleep(0.2)
        if ws is None:
            raise RuntimeError("Could not connect to Streamerz WS")
        assert ws.hello.get("info", {}).get("name") == instance_name, f"Wrong instance: {ws.hello}"
        print("[OK] Connected to isolated Streamerz instance:", instance_name)

        def ui(cmd):
            return ws.ask({"request": "StreamerzUi", "id": uuid.uuid4().hex, "do": cmd}).get("result", "")

        def bounds(text=""):
            return json.loads(ui("bounds " + text))

        def click_widget(w):
            cx = w["x"] + w["w"] // 2
            cy = w["y"] + w["h"] // 2
            subprocess.run(["/usr/bin/xdotool", "mousemove", str(cx), str(cy), "click", "1"], env=env, check=True)

        # Find X11 window ID for Streamerz
        for _ in range(50):
            found = subprocess.run(
                ["/usr/bin/xdotool", "search", "--onlyvisible", "--pid", str(app.pid), "--name", "Streamerz"],
                env=env, capture_output=True, text=True,
            ).stdout.split()
            if found:
                sz_win = found[0]
                break
            time.sleep(0.1)
        else:
            raise AssertionError("Streamerz window did not appear")
        subprocess.run(["/usr/bin/xdotool", "windowmove", sz_win, "40", "40"], env=env, check=True)
        subprocess.run(["/usr/bin/xdotool", "windowsize", sz_win, "1100", "700"], env=env, check=True)
        time.sleep(0.3)

        assert ui("page Accounts") == "ok"
        time.sleep(0.3)
        # Find TikTok card's "Set up" button
        all_b = bounds()
        tiktok_labels = [w for w in all_b if "TikTok" in w.get("label", "")]
        setup_btns = [w for w in all_b if w.get("label", "").strip() == "Set up"]
        print("TikTok labels on Accounts:", [(w["label"], w["x"], w["y"]) for w in tiktok_labels])
        print("Set up buttons on Accounts:", [(w["label"], w["x"], w["y"], w.get("tooltip")) for w in setup_btns])
        assert setup_btns, "Expected Set up button on Accounts page"
        # Pick Set up button closest in y to TikTok section
        tt_y = min(w["y"] for w in tiktok_labels)
        tt_setup = min([b for b in setup_btns if b["y"] >= tt_y - 20], key=lambda b: abs(b["y"] - tt_y))

        # Reproduce Set up with the browser already running.
        pre_chrome = subprocess.Popen(
            [shim, f"--user-data-dir={browser_dir}", "about:blank"],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for _ in range(40):
            if x11_chromium_windows(env):
                break
            time.sleep(0.2)
        wins_before = x11_chromium_windows(env)
        pids_before = chromium_main_pids(browser_dir)
        print("[OK] Pre-launched Chromium:", pids_before, wins_before)
        assert len(pids_before) == 1 and len(wins_before) == 1

        # Refocus Streamerz and click TikTok "Set up" on Accounts page to open the TikTok account dialog
        subprocess.run(["/usr/bin/xdotool", "windowraise", sz_win, "windowfocus", sz_win], env=env, check=True)
        time.sleep(0.2)
        # Refresh bounds after windowactivate
        all_b = bounds()
        setup_btns = [w for w in all_b if w.get("label", "").strip() == "Set up"]
        tt_y = min(w["y"] for w in all_b if "TikTok" in w.get("label", ""))
        tt_setup = min([b for b in setup_btns if b["y"] >= tt_y - 20], key=lambda b: abs(b["y"] - tt_y))
        click_widget(tt_setup)
        time.sleep(0.4)

        win_info = ui("windows")
        print("Windows after opening dialog:\n" + win_info)
        assert "TikTok account" in win_info, f"Expected TikTok account dialog open, got: {win_info}"

        # 1. Screenshot WIDE (1100x700)
        wide_shot = os.path.join(SHOT_DIR, "tiktok_setup_wide.png")
        print("Wide shot:", ui(f"shot {wide_shot}"))

        # Verify all dialog widgets fit inside window at 1100x700
        m = re.search(r"^0: .* at (-?\d+),(-?\d+) (\d+)x(\d+) shown$", win_info, re.MULTILINE)
        wx, wy, ww, wh = map(int, m.groups())
        dlg_widgets = bounds()
        for w in dlg_widgets:
            assert w["x"] >= wx and w["y"] >= wy and w["x"] + w["w"] <= wx + ww and w["y"] + w["h"] <= wy + wh, (
                f"Widget out of bounds at 1100x700: {w} vs {(wx, wy, ww, wh)}"
            )
        print(f"[OK] All {len(dlg_widgets)} visible widgets fit inside 1100x700 window")

        # 2. Resize to NARROW (540x700) and screenshot
        subprocess.run(["/usr/bin/xdotool", "windowsize", sz_win, "540", "700"], env=env, check=True)
        time.sleep(0.4)
        narrow_shot = os.path.join(SHOT_DIR, "tiktok_setup_narrow.png")
        print("Narrow shot:", ui(f"shot {narrow_shot}"))
        win_info_n = ui("windows")
        mn = re.search(r"^0: .* at (-?\d+),(-?\d+) (\d+)x(\d+) shown$", win_info_n, re.MULTILINE)
        nwx, nwy, nww, nwh = map(int, mn.groups())
        for w in bounds():
            assert (
                w["x"] >= nwx and w["y"] >= nwy and w["x"] + w["w"] <= nwx + nww and w["y"] + w["h"] <= nwy + nwh
            ), f"Widget out of bounds at 540x700: {w} vs {(nwx, nwy, nww, nwh)}"
        print("[OK] All visible widgets fit inside 540x700 narrow window")

        # Restore to 1100x700
        subprocess.run(["/usr/bin/xdotool", "windowsize", sz_win, "1100", "700"], env=env, check=True)
        subprocess.run(["/usr/bin/xdotool", "windowraise", sz_win, "windowfocus", sz_win], env=env, check=True)
        time.sleep(0.4)

        # 3. Scenario A: Click "Set up" next to Chromium while Chromium is ALREADY running!
        row_setups = [w for w in bounds("Set up") if w.get("tooltip") == browser_dir]
        assert len(row_setups) == 1, f"Expected 1 Set up button in dialog, got: {row_setups}"
        click_widget(row_setups[0])
        time.sleep(2.5)

        # Check Streamerz dialog stayed open!
        win_info_after = ui("windows")
        assert "TikTok account" in win_info_after, f"Dialog closed unexpectedly! {win_info_after}"
        print("[OK] Streamerz TikTok account dialog stayed open after clicking Set up")

        # Check Chromium reused existing window & process!
        pids_after = chromium_main_pids(browser_dir)
        wins_after = x11_chromium_windows(env)
        print("Chromium PIDs after Set up click:", pids_after)
        print("Chromium windows after Set up click:", wins_after)
        assert pids_after == pids_before, f"Expected same single Chromium PID {pids_before}, got {pids_after}"
        assert len(wins_after) == 1, f"Expected 1 Chromium window (reused), got {len(wins_after)}: {wins_after}"
        assert "Set up Streamerz in Chromium" in wins_after[0][1], (
            f"Expected setup tab active in existing window, got title: {wins_after[0][1]}"
        )
        print("[OK] Existing Chromium window reused with 'Set up Streamerz in Chromium' tab!")

        # Take screenshot of Streamerz dialog with Step 1 ticked
        step1_shot = os.path.join(SHOT_DIR, "tiktok_setup_step1.png")
        subprocess.run(["/usr/bin/xdotool", "windowraise", sz_win, "windowfocus", sz_win], env=env, check=True)
        time.sleep(0.2)
        print("Step 1 shot:", ui(f"shot {step1_shot}"))

        # Find where extension folder & tiktok-setup.html were written
        ext_folders = []
        for root_walk, dirs, _ in os.walk(cache_dir):
            if "tiktok-extension" in dirs:
                ext_folders.append(os.path.join(root_walk, "tiktok-extension"))
        assert len(ext_folders) == 1, f"Expected extension folder in dev cache, found: {ext_folders}"
        ext_folder = ext_folders[0]
        setup_html = os.path.join(os.path.dirname(ext_folder), "tiktok-setup.html")
        assert os.path.isfile(setup_html), f"Missing {setup_html}"
        assert os.path.isfile(os.path.join(ext_folder, "config.json")), "Missing config.json"
        print("[OK] Dev extension folder & setup HTML created in XDG_CACHE_HOME:", ext_folder)

        # 4. Scenario B: Cold start with stale SingletonLock!
        kill_pid(pre_chrome.pid)
        for p in chromium_main_pids(browser_dir):
            kill_pid(p)
        time.sleep(0.5)
        assert chromium_main_pids(browser_dir) == []
        # Create a stale SingletonLock pointing to dead PID 99999999
        lock_path = os.path.join(browser_dir, "SingletonLock")
        try:
            os.unlink(lock_path)
        except OSError:
            pass
        os.symlink(f"{socket.gethostname()}-99999999", lock_path)

        subprocess.run(["/usr/bin/xdotool", "windowraise", sz_win, "windowfocus", sz_win], env=env, check=True)
        time.sleep(0.2)
        row_setups = [w for w in bounds("Set up") if w.get("tooltip") == browser_dir]
        click_widget(row_setups[0])
        for _ in range(40):
            wins_cold = x11_chromium_windows(env)
            if any("Set up Streamerz in Chromium" in t for _, t in wins_cold):
                break
            time.sleep(0.25)
        wins_cold = x11_chromium_windows(env)
        pids_cold = chromium_main_pids(browser_dir)
        print("Cold-start Chromium PIDs:", pids_cold, "windows:", wins_cold)
        assert len(pids_cold) == 1 and len(wins_cold) == 1
        assert "Set up Streamerz in Chromium" in wins_cold[0][1]
        assert "TikTok account" in ui("windows"), "Dialog closed during cold-start launch!"
        print("[OK] Cold-start Chromium opened straight onto setup tab despite stale SingletonLock!")

        # Stop cold Chromium before Scenario C
        for p in pids_cold:
            kill_pid(p)
        time.sleep(0.5)

        # 5. Scenario C: Launch Chromium with unpacked extension loaded AND tiktok-setup.html as its ONLY initial tab!
        setup_url = "file://" + urllib.parse.quote(setup_html)
        ext_chrome = subprocess.Popen(
            [shim, f"--user-data-dir={browser_dir}", f"--load-extension={ext_folder}", setup_url],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            # Wait for pairing -> auto-close of tiktok-setup.html -> auto-open of tiktok.com/login
            paired_ok = False
            for _ in range(40):
                time.sleep(0.3)
                b_all = bounds()
                labels = [w.get("label", "") for w in b_all]
                if any("Log in" in lbl for lbl in labels) or any("Connected to Streamerz" in lbl for lbl in labels):
                    paired_ok = True
                    # Give auto-close + tiktok.com/login tab a moment to settle
                    wins_ext = x11_chromium_windows(env)
                    if wins_ext and not any("Set up Streamerz" in t for _, t in wins_ext):
                        break
            time.sleep(1.5)
            pids_ext = chromium_main_pids(browser_dir)
            wins_ext = x11_chromium_windows(env)
            print("Paired Chromium PIDs:", pids_ext)
            print("Paired Chromium windows:", wins_ext)
            assert len(pids_ext) == 1, "Chromium quit when setup tab closed!"
            assert not any("Set up Streamerz in Chromium" in t for _, t in wins_ext), (
                f"Setup tab did not auto-close after pairing: {wins_ext}"
            )
            print("[OK] Setup tab auto-closed without quitting single-tab Chromium window!")

            # Check Streamerz UI state in wide & narrow screenshots
            subprocess.run(["/usr/bin/xdotool", "windowraise", sz_win, "windowfocus", sz_win], env=env, check=True)
            time.sleep(0.3)
            paired_wide = os.path.join(SHOT_DIR, "tiktok_setup_paired_wide.png")
            print("Paired wide shot:", ui(f"shot {paired_wide}"))

            dlg_labels = [w.get("label", "").strip() for w in bounds() if w.get("label", "").strip()]
            print("Dialog labels after pairing:", dlg_labels)
            assert "TikTok account" in ui("windows"), "Dialog closed during pairing!"
            assert any(lbl == "Log in" for lbl in dlg_labels), f"Expected row button 'Log in', got {dlg_labels}"
            status = ws.ask({"request": "StreamerzTikTokBridge", "action": "status", "id": uuid.uuid4().hex})
            assert status.get("connected") and status.get("loginRequired"), status
            assert any("TikTok" in title and "Log in" in title for _, title in wins_ext), wins_ext

            subprocess.run(["/usr/bin/xdotool", "windowsize", sz_win, "540", "700"], env=env, check=True)
            time.sleep(0.4)
            paired_narrow = os.path.join(SHOT_DIR, "tiktok_setup_paired_narrow.png")
            print("Paired narrow shot:", ui(f"shot {paired_narrow}"))
        finally:
            kill_pid(ext_chrome.pid)
            for p in chromium_main_pids(browser_dir):
                kill_pid(p)

        print("\n=== ALL E2E CHECKS PASSED ===")
    finally:
        if app is not None:
            kill_pid(app.pid)
        for p in chromium_main_pids(browser_dir):
            kill_pid(p)
        if wm is not None:
            kill_pid(wm.pid)
        if xvfb is not None:
            kill_pid(xvfb.pid)
        shutil.rmtree(ROOT, ignore_errors=True)


if __name__ == "__main__":
    main()
