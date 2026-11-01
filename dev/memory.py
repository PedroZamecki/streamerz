#!/usr/bin/env python3
"""Profile synthetic chat retention on Linux without touching real settings.

Build first: cargo build --profile fast
Run: python3 dev/memory.py --out /tmp/streamerz-memory
Optional allocation tracing: --heaptrack-lib /path/to/libheaptrack_preload.so
Raw traces need heaptrack_interpret before heaptrack_print. Profile RSS in a
separate uninstrumented run: the tracer itself uses memory and CPU.
"""
import sys
# dev/locale.py is a repository tool, not Python's standard library locale.
sys.path = [p for p in sys.path if p != __file__.rsplit("/", 1)[0]]
import argparse
from collections import Counter
import glob
import hashlib
import json
import os
import re
import shutil
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import threading
import time


class Ws:
    def __init__(self, port):
        self.s = socket.create_connection(("127.0.0.1", port), timeout=60)
        self.s.sendall(b"GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\n"
                       b"Connection: Upgrade\r\nSec-WebSocket-Key: MDEyMzQ1Njc4OWFiY2RlZg==\r\n"
                       b"Sec-WebSocket-Version: 13\r\n\r\n")
        head = b""
        while not head.endswith(b"\r\n\r\n"):
            head += self.read_n(1)
        if b" 101 " not in head:
            raise RuntimeError(head)
        self.read()

    def read_n(self, count):
        data = b""
        while len(data) < count:
            part = self.s.recv(count - len(data))
            if not part:
                raise EOFError("WebSocket closed")
            data += part
        return data

    def read(self):
        head = self.read_n(2)
        size = head[1] & 127
        if size == 126:
            size = struct.unpack(">H", self.read_n(2))[0]
        elif size == 127:
            size = struct.unpack(">Q", self.read_n(8))[0]
        if head[0] & 15 != 1:
            raise RuntimeError("Expected a text response")
        return json.loads(self.read_n(size))

    def ask(self, **req):
        data = json.dumps(dict(req, id="profile")).encode()
        mask = os.urandom(4)
        head = b"\x81" + (bytes([128 | len(data)]) if len(data) < 126
                          else b"\xfe" + struct.pack(">H", len(data)))
        self.s.sendall(head + mask + bytes(v ^ mask[i % 4] for i, v in enumerate(data)))
        response = self.read()
        if response.get("status") != "ok":
            raise RuntimeError(response)
        return response


def sample(pid):
    values = {}
    for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
        key, _, rest = line.partition(":")
        if rest.strip().endswith("kB"):
            values[key] = int(rest.split()[0]) * 1024
    stat = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    values["cpu_seconds"] = (int(stat[11]) + int(stat[12])) / os.sysconf("SC_CLK_TCK")
    values["USS"] = values.get("Private_Clean", 0) + values.get("Private_Dirty", 0)
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        key, _, rest = line.partition(":")
        if key == "VmHWM":
            values["VmHWM"] = int(rest.split()[0]) * 1024
        elif key == "Threads":
            values["threads"] = int(rest)
    for line in Path(f"/proc/{pid}/io").read_text().splitlines():
        key, value = line.split(":")
        values["io_" + key] = int(value)
    return values


def run(args, name):
    output = args.out / name
    output.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="streamerz-memory-source-") as root:
        folder = Path(root) / "streamerz"
        folder.mkdir()
        cfg = {"memory_saver": args.memory_saver, "start_hidden": not args.shown, "locale": "en_US", "updates": "ignore",
               "window": {"chat": {"saveHistory": name != "off", "lifetime": 3 if name == "ttl" else 0}},
               "websockets": {"port": args.port}}
        (folder / "streamerz.json").write_text(json.dumps(cfg))
        if args.seed:
            shutil.copyfile(args.seed, folder / "community.json")
        env = dict(os.environ, XDG_CONFIG_HOME=root, DISPLAY=":99", STREAMERZ_DEV="1",
                   STREAMERZ_DEV_PORT=str(args.port))
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("STREAMERZ_CONFIG", None)
        for entry in args.env:
            key, value = entry.split("=", 1)
            env[key] = value
        if args.heaptrack_lib:
            env["LD_PRELOAD"] = str(args.heaptrack_lib.resolve())
            env["DUMP_HEAPTRACK_OUTPUT"] = str(output / "heaptrack.raw")
        rows = []
        with (output / "process.log").open("w") as log:
            app = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=log, stderr=log)
            start = time.monotonic()
            stop = threading.Event()
            timeline = []
            def monitor():
                while not stop.wait(.2):
                    try:
                        timeline.append(dict(seconds=time.monotonic() - start, **sample(app.pid)))
                    except (OSError, ProcessLookupError):
                        return
            sampler = threading.Thread(target=monitor)
            sampler.start()
            try:
                ws = None
                for _ in range(200):
                    if app.poll() is not None:
                        raise RuntimeError(f"App exited: see {output / 'process.log'}")
                    try:
                        ws = Ws(args.port)
                        break
                    except OSError:
                        time.sleep(.1)
                if ws is None:
                    raise RuntimeError("No development WebSocket")
                time.sleep(2)
                policy = ws.ask(request="StreamerzMemory")
                if policy["active"] != args.memory_saver:
                    raise AssertionError(f"Wrong running memory policy: {policy}")
                if args.shown:
                    ws.ask(request="SimulateChatConnection", platform="twitch", connected=True)
                    ws.ask(request="StreamerzUi", do="page Chat")
                def record(phase, count):
                    row = dict(phase=phase, messages=count, seconds=time.monotonic() - start,
                               **sample(app.pid))
                    rows.append(row)
                    (output / "samples.json").write_text(json.dumps(rows, indent=2))
                    print(name, phase, count, "RSS", round(row["Rss"] / 1048576, 2),
                          "MiB, USS", round(row["USS"] / 1048576, 2), flush=True)
                record("baseline", 0)
                latencies = []
                traffic_start = time.monotonic()
                for i in range(args.messages):
                    if args.rate:
                        delay = traffic_start + i / args.rate - time.monotonic()
                        if delay > 0:
                            time.sleep(delay)
                    user = i % 10 if name == "repeat" else i
                    sent = time.monotonic()
                    ws.ask(request="InjectChat", platform="twitch", user=f"viewer{user}",
                           userId=str(user), text="Synthetic memory profile message " + "x" * 128)
                    if args.first_words:
                        first = ws.ask(request="StreamerzFirstWords", platform="twitch", user=f"viewer{user}")["first"]
                        if first != (name != "repeat" or i < 10):
                            raise AssertionError(f"Wrong First Words result for message {i}")
                    latencies.append(time.monotonic() - sent)
                    if (i + 1) % 1000 == 0:
                        record("traffic", i + 1)
                record("traffic_end", args.messages)
                traffic_seconds = time.monotonic() - traffic_start
                ordered = sorted(latencies)
                quantile = lambda fraction: ordered[min(len(ordered) - 1, int(len(ordered) * fraction))] * 1000
                first, end = rows[0], rows[-1]
                summary = dict(traffic_seconds=traffic_seconds, delivered_rate=args.messages / traffic_seconds,
                               cpu_seconds=end["cpu_seconds"] - first["cpu_seconds"],
                               cpu_percent_one_core=100 * (end["cpu_seconds"] - first["cpu_seconds"]) / traffic_seconds,
                               latency_ms=dict(p50=quantile(.5), p95=quantile(.95), p99=quantile(.99), maximum=max(latencies)*1000),
                               write_chars=end["io_wchar"] - first["io_wchar"],
                               write_syscalls=end["io_syscw"] - first["io_syscw"])
                for seconds in (1, 5, 15):
                    time.sleep(seconds - (0 if seconds == 1 else 1 if seconds == 5 else 5))
                    record(f"idle_{seconds}s", args.messages)
                if args.visit_community:
                    if not args.shown:
                        ws.ask(request="StreamerzUi", do="toggle")
                        time.sleep(1)
                    ws.ask(request="StreamerzUi", do="page Community")
                    time.sleep(1)
                    record("community_shown", args.messages)
                    count = int(ws.ask(request="StreamerzUi", do="community")["result"])
                    expected_users = 10 if name == "repeat" else args.messages
                    if args.seed:
                        expected_users = max(expected_users, len(json.loads(args.seed.read_bytes()).get("users", [])))
                    if args.memory_saver:
                        expected_users = 0
                    if (args.memory_saver and count != 0) or count < expected_users:
                        raise AssertionError(f"Community shows {count}, expected at least {expected_users}")
                    wid = subprocess.check_output(["xdotool", "search", "--pid", str(app.pid)], env=env, text=True).splitlines()[0]
                    for label, width, height in [("wide", "1200", "800"), ("narrow", "700", "600")]:
                        subprocess.run(["xdotool", "windowsize", wid, width, height], env=env, check=True)
                        time.sleep(.3)
                        ws.ask(request="StreamerzUi", do=f"shot {output / ('community-' + label + '.png')}")
                    ws.ask(request="StreamerzUi", do="page Chat")
                    time.sleep(1)
                    record("community_left", args.messages)
                    if int(ws.ask(request="StreamerzUi", do="community")["result"]) != 0:
                        raise AssertionError("Community retained presentation rows after leaving the page")
                    ws.ask(request="StreamerzUi", do="page Community")
                    time.sleep(1)
                    if int(ws.ask(request="StreamerzUi", do="community")["result"]) != count:
                        raise AssertionError("Community did not refill after returning to the page")
                    ws.ask(request="StreamerzUi", do="toggle")
                    time.sleep(2)
                    record("community_hidden", args.messages)
                    if int(ws.ask(request="StreamerzUi", do="community")["result"]) != 0:
                        raise AssertionError("Community retained presentation rows while hidden")
                if args.visit_settings:
                    if not args.shown and not args.visit_community:
                        ws.ask(request="StreamerzUi", do="toggle")
                    ws.ask(request="StreamerzUi", do="page Settings")
                    time.sleep(.5)
                    wid = subprocess.check_output(["xdotool", "search", "--pid", str(app.pid)], env=env, text=True).splitlines()[0]
                    for label, width, height in [("wide", "1200", "800"), ("narrow", "700", "600")]:
                        subprocess.run(["xdotool", "windowsize", wid, width, height], env=env, check=True)
                        time.sleep(.3)
                        ws.ask(request="StreamerzUi", do=f"shot {output / ('settings-' + label + '.png')}")
                    pending = ws.ask(request="StreamerzMemory", enabled=not args.memory_saver)
                    if not pending["restartRequired"] or pending["active"] != args.memory_saver:
                        raise AssertionError("Changing saved mode changed the running policy")
                    restored = ws.ask(request="StreamerzMemory", enabled=args.memory_saver)
                    if restored["restartRequired"]:
                        raise AssertionError("Restoring saved mode still requests restart")
                    if not args.shown:
                        ws.ask(request="StreamerzUi", do="toggle")
                if args.shown:
                    ws.ask(request="StreamerzUi", do=f"shot {output / 'chat.png'}")
                    found = ws.ask(request="StreamerzUi", do="find Hide")["result"]
                    match = re.search(r" at (\d+),(\d+)", found)
                    if not match:
                        raise RuntimeError(f"Hide button not found: {found}")
                    subprocess.run(["xdotool", "mousemove", match[1], match[2], "click", "1"],
                                   env=env, check=True)
                    time.sleep(2)
                    windows = ws.ask(request="StreamerzUi", do="windows")["result"]
                    (output / "windows-after-hide.txt").write_text(windows)
                    if "shown" in windows:
                        raise RuntimeError(f"Window stayed shown: {windows}")
                    record("hidden_2s", args.messages)
                dirs = glob.glob(os.path.join(tempfile.gettempdir(), f"streamerz-dev-{app.pid}-*"))
                if not dirs:
                    raise RuntimeError("Isolated development settings directory was not found")
                dirs = [max(dirs, key=os.path.getmtime)]
                if args.sync_storage and not args.memory_saver:
                    with (Path(dirs[0]) / "community.json").open("rb") as stored:
                        os.fsync(stored.fileno())
                sizes = {str(p.relative_to(dirs[0])): {"bytes": p.stat().st_size,
                         "allocated_bytes": p.stat().st_blocks * 512}
                         for p in Path(dirs[0]).rglob("*") if p.is_file()}
                (output / "disk.json").write_text(json.dumps(sizes, indent=2))
                community = Path(dirs[0]) / "community.json"
                if args.memory_saver:
                    expected = args.seed.read_bytes() if args.seed else None
                    data = community.read_bytes() if community.exists() else None
                    if data != expected:
                        raise AssertionError("Memory Saver changed the preserved Community file")
                    (output / "community-size.json").write_text(json.dumps(dict(preserved=True, bytes=len(data) if data else 0)))
                else:
                    if not community.exists():
                        raise AssertionError("No saved Community file to verify")
                    import gzip
                    data = community.read_bytes()
                    shutil.copyfile(community, output / "community.json")
                    users = json.loads(data).get("users", [])
                    old = json.loads(args.seed.read_bytes()).get("users", []) if args.seed else []
                    expected = sum(u.get("messages", 0) for u in old) + args.messages
                    actual = sum(u.get("messages", 0) for u in users)
                    if actual != expected:
                        raise AssertionError(f"Persisted message count: {actual}, expected {expected}")
                    identity = lambda u: (u.get("type", ""), u.get("id", ""), u.get("channel", ""))
                    expected_counts = Counter({identity(u): u.get("messages", 0) for u in old})
                    for i in range(args.messages):
                        expected_counts[("twitch", str(i % 10 if name == "repeat" else i), "")] += 1
                    actual_counts = Counter({identity(u): u.get("messages", 0) for u in users})
                    if actual_counts != expected_counts or len(users) != len(expected_counts):
                        raise AssertionError("Persisted users or individual message counts differ")
                    info = dict(users=len(json.loads(data).get("users", [])), bytes=len(data),
                                messages=actual, gzip_bytes=len(gzip.compress(data, mtime=0)))
                    (output / "community-size.json").write_text(json.dumps(info, indent=2))
                summary.update(peak_rss=max(r["Rss"] for r in rows + timeline), kernel_peak_rss=rows[-1]["VmHWM"],
                           final_rss=rows[-1]["Rss"], idle_cpu_seconds=next(r for r in rows if r["phase"]=="idle_15s")["cpu_seconds"]-end["cpu_seconds"])
                (output / "summary.json").write_text(json.dumps(summary, indent=2))
                ws.s.close()
            finally:
                stop.set()
                sampler.join()
                (output / "timeline.json").write_text(json.dumps(timeline, indent=2))
                app.terminate()
                try:
                    app.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()
        return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/fast/Streamerz"))
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--messages", type=int, default=10000)
    parser.add_argument("--port", type=int, default=8097)
    parser.add_argument("--cases", nargs="+", choices=["repeat", "unique", "off", "ttl"],
                        default=["repeat", "unique", "off", "ttl"])
    parser.add_argument("--heaptrack-lib", type=Path)
    parser.add_argument("--shown", action="store_true", help="Show the Chat page on DISPLAY=:99")
    parser.add_argument("--rate", type=int, default=0, help="Target messages/second; 0 means unthrottled")
    parser.add_argument("--seed", type=Path, help="Synthetic saved Community file to load before traffic")
    parser.add_argument("--env", action="append", default=[], metavar="KEY=VALUE", help="Experiment process environment")
    parser.add_argument("--visit-community", action="store_true", help="Measure Community page, leaving it, reopening and hiding")
    parser.add_argument("--sync-storage", action="store_true", help="Flush the synthetic Community file before measuring allocated disk blocks")
    parser.add_argument("--visit-settings", action="store_true", help="Capture and test the memory policy settings")
    parser.add_argument("--memory-saver", action="store_true", help="Enable aggressive background policy")
    parser.add_argument("--first-words", action="store_true", help="Also exercise and verify First Words on every message")
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    if args.messages < 1 or args.messages % 50 or args.rate < 0:
        parser.error("--messages must be a positive multiple of 50 (the save interval), and --rate nonnegative")
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", args.port))
    with args.binary.open("rb") as binary_file:
        checksum = hashlib.file_digest(binary_file, "sha256").hexdigest()
    metadata = dict(binary=str(args.binary.resolve()),
                    sha256=checksum,
                    memory_saver=args.memory_saver, first_words=args.first_words,
                    instrumented=bool(args.heaptrack_lib), messages=args.messages,
                    cases=args.cases, shown=args.shown, sample_interval_seconds=.2,
                    visit_community=args.visit_community, visit_settings=args.visit_settings,
                    rate=args.rate, seed=str(args.seed) if args.seed else None, env=args.env,
                    sync_storage=args.sync_storage,
                    kernel=os.uname().release)
    (args.out / "metadata.json").write_text(json.dumps(metadata, indent=2))
    for name in args.cases:
        run(args, name)


if __name__ == "__main__":
    main()
