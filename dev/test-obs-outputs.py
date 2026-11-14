#!/usr/bin/env python3
"""Exercise installed OBS stream plugins through the Streamerz backend, using loopback receivers.

Requires OBS, FFmpeg, Pillow, installed Aitum/Multistream-websocket plugins and an X11 test display.
Run with env -u WAYLAND_DISPLAY DISPLAY=:99 python3 -P dev/test-obs-outputs.py.
All settings, scenes, results and logs stay in target/obs-outputs-verification. Production settings
and destinations are never copied or changed. Public streaming services are not used.
"""
import argparse
import base64
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent
PLUGINS = Path.home() / ".config/obs-studio/plugins"
SOURCES = {"suite": "aitum-stream-suite", "vertical": "aitum-vertical-canvas", "multi": "aitum-multistream", "rtmp": "sorayuki.multi_rtmp", "stock": "obs"}
MODULES = ["aitum-stream-suite", "vertical-canvas", "aitum-multistream", "obs-multi-rtmp"]
CASES = ["stock", "suite", "suite-vertical", "vertical", "multi", "multi-vertical", "rtmp", "stock-reconnect", "multi-no-main", "suite-browser"]


def free_port(port):
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", port))


def settings(folder, case, port):
    base = case.split("-")[0]
    config = folder / "obs-studio"
    profile = config / "basic/profiles/Untitled"
    profile.mkdir(parents=True)
    (config / "plugins").symlink_to(PLUGINS, target_is_directory=True)
    def write(path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, indent=2) + "\n")
    # Xvfb has no GPU. Keep the browser module enabled, using its software renderer.
    (config / "global.ini").write_text("[General]\nBrowserHWAccel=false\n")
    (config / "user.ini").write_text("[General]\nFirstRun=true\n[Basic]\nProfile=Untitled\nProfileDir=Untitled\nSceneCollection=Untitled\nSceneCollectionFile=Untitled\n[BasicWindow]\nWarnBeforeStartingStream=false\nWarnBeforeStoppingStream=false\n")
    (profile / "basic.ini").write_text("[General]\nName=Untitled\n[Output]\nMode=Simple\n[SimpleOutput]\nStreamEncoder=x264\nVBitrate=500\nABitrate=64\nUseAdvanced=true\nx264Settings=keyint=10\n[Video]\nBaseCX=320\nBaseCY=180\nOutputCX=320\nOutputCY=180\nFPSType=0\nFPSCommon=10\n")
    write(profile / "service.json", {"type": "rtmp_custom", "settings": {"server": "rtmp://127.0.0.1:19450/live", "key": "test", "use_auth": False}})
    scene_id, color_id = str(uuid.uuid4()), str(uuid.uuid4())
    write(config / "basic/scenes/Untitled.json", {"name": "Untitled", "current_scene": "Scene", "current_program_scene": "Scene", "current_transition": "Fade", "transition_duration": 300, "scene_order": [{"name": "Scene"}], "sources": [
        {"name": "Test color", "uuid": color_id, "id": "color_source", "versioned_id": "color_source_v3", "settings": {"color": 4281532569, "width": 320, "height": 180}, "mixers": 0, "enabled": True},
        {"name": "Scene", "uuid": scene_id, "id": "scene", "versioned_id": "scene", "settings": {"id_counter": 1, "items": [{"name": "Test color", "source_uuid": color_id, "visible": True, "id": 1, "pos": {"x": 0, "y": 0}, "scale": {"x": 1, "y": 1}, "align": 5}]}, "mixers": 0, "enabled": True},
    ], "groups": [], "canvases": [], "transitions": [], "modules": {}, "resolution": {"x": 320, "y": 180}, "version": 2})
    enabled = {"stock": [], "suite": ["aitum-stream-suite"], "vertical": ["vertical-canvas"], "multi": ["aitum-multistream", "vertical-canvas"], "rtmp": ["obs-multi-rtmp"]}[base]
    modules = MODULES
    write(config / "plugin_manager/modules.json", [{"module_name": name, "display_name": name, "id": "", "version": "", "enabled": name in enabled, "sources": [], "outputs": [], "encoders": [], "services": []} for name in modules])
    write(config / "plugin_config/obs-websocket/config.json", {"server_enabled": True, "server_port": port, "auth_required": False, "first_load": False, "alerts_enabled": False})
    write(profile / "aitum.json", {"canvas": [{"name": "Vertical", "width": 180, "height": 320}], "main_stream_output_show": True, "outputs": [
        {"name": "YouTube test", "type": "stream", "enabled": True, "stream_server": "rtmp://127.0.0.1:19451/live", "stream_key": "test"},
        {"name": "TikTok test", "type": "stream", "enabled": True, "canvas": "Vertical", "advanced": True, "video_encoder": "obs_x264", "video_encoder_settings": {"bitrate": 500, "keyint_sec": 1, "preset": "ultrafast"}, "stream_server": "rtmp://127.0.0.1:19452/live", "stream_key": "test"},
        {"name": "Recording filter test", "type": "record", "enabled": False},
    ]})
    write(config / "plugin_config/vertical-canvas/config.json", {"canvas": [{"current_scene": "Vertical Scene", "width": 180, "height": 320, "streaming_video_bitrate": 500, "audio_bitrate": 64, "stream_advanced_settings": True, "stream_encoder": "obs_x264", "stream_encoder_settings": {"bitrate": 500, "keyint_sec": 1, "preset": "ultrafast"}, "stream_audio_track": 0, "backtrack": False, "stream_outputs": [{"name": "TikTok test", "stream_server": "rtmp://127.0.0.1:19452/live", "stream_key": "test", "enabled": True}]}]})
    write(config / "plugin_config/aitum-multistream/config.json", {"profiles": [{"name": "Untitled", "outputs": [{"name": "Kick test", "advanced": True, "server": "rtmp://127.0.0.1:19453/live", "key": "test"}]}]})


def browser_ready(folder, port):
    """Wait for real Suite output initialization, then check a local browser page's pixels."""
    from PIL import Image
    spec = importlib.util.spec_from_file_location("obs_test_socket", ROOT / "dev/harness.py")
    client = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(client)
    client.PORT = port
    ws = client.Ws()
    try:
        ws.ask({"op": 1, "d": {"rpcVersion": 1, "eventSubscriptions": 0}})
        def call(kind, data):
            reply = ws.ask({"op": 6, "d": {"requestId": "browser-ready", "requestType": kind, "requestData": data}})["d"]
            if reply["requestStatus"]["code"] == 207 and kind == "CallVendorRequest":
                return {}  # The visible main window can precede frontend readiness.
            if not reply["requestStatus"]["result"]:
                raise RuntimeError(f"OBS browser probe failed: {reply['requestStatus']}")
            return reply.get("responseData", {})
        deadline = time.monotonic() + 60
        while True:
            result = call("CallVendorRequest", {"vendorName": "aitum-stream-suite", "requestType": "get_outputs", "requestData": {}})
            if any(output.get("name") == "YouTube test" for output in result.get("responseData", {}).get("outputs", [])):
                break
            if time.monotonic() > deadline:
                raise RuntimeError("Suite did not finish loading its configured outputs with obs-browser enabled")
            time.sleep(0.1)
        page = folder / "browser-check.html"
        page.write_text("<html><style>html,body{margin:0;background:rgb(20,190,70);width:100%;height:100%}</style><body></body></html>")
        call("CreateInput", {"sceneName": "Scene", "inputName": "Browser verification", "inputKind": "browser_source", "inputSettings": {"is_local_file": True, "local_file": str(page), "width": 320, "height": 180}, "sceneItemEnabled": True})
        deadline = time.monotonic() + 20
        while True:
            screenshot = call("GetSourceScreenshot", {"sourceName": "Browser verification", "imageFormat": "png", "imageWidth": 32, "imageHeight": 18})["imageData"]
            payload = base64.b64decode(screenshot.split(",", 1)[1])
            pixel = Image.open(io.BytesIO(payload)).convert("RGB").getpixel((16, 9))
            if all(abs(a - b) < 3 for a, b in zip(pixel, (20, 190, 70))):
                (folder / "browser.png").write_bytes(payload)
                print("PASS actual browser page pixels", pixel, flush=True)
                return
            if time.monotonic() > deadline:
                raise RuntimeError(f"Browser did not render the local page: {pixel}")
            time.sleep(0.25)
    finally:
        ws.s.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", nargs="+", choices=CASES, default=CASES)
    parser.add_argument("--port-base", type=int, default=14650)
    args = parser.parse_args()
    for tool in ["obs", "ffmpeg", "cargo", "xdotool"]:
        if not shutil.which(tool):
            parser.error(f"{tool} is required")
    if not PLUGINS.is_dir():
        parser.error("Install the OBS plugins before running this test")
    root = ROOT / "target/obs-outputs-verification"
    root.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix="run-", dir=root))
    print(f"Results: {run}", flush=True)
    results = []
    for index, case in enumerate(args.case):
        port = args.port_base + index
        base = case.split("-")[0]
        folder = run / case
        try:
            for number in [port, 19450, 19451, 19452, 19453, 19454]:
                free_port(number)
            settings(folder, case, port)
            env = dict(os.environ, DISPLAY=":99", QT_QPA_PLATFORM="xcb", XDG_CONFIG_HOME=str(folder), STREAMERZ_TEST_OBS_PORT=str(port), STREAMERZ_TEST_OBS_SOURCE=SOURCES[base])
            for name in ["WAYLAND_DISPLAY", "OBS_PLUGINS_PATH", "OBS_PLUGINS_DATA_PATH", "STREAMERZ_TEST_OBS_OUTPUT_NAME", "STREAMERZ_TEST_OBS_TARGET_PORT", "STREAMERZ_TEST_OBS_RECONNECT", "STREAMERZ_TEST_OBS_NO_MAIN"]:
                env.pop(name, None)
            if case == "stock-reconnect": env["STREAMERZ_TEST_OBS_RECONNECT"] = "1"
            if case == "multi-no-main": env["STREAMERZ_TEST_OBS_NO_MAIN"] = "1"
            if case in ["suite-vertical", "multi-vertical"]:
                env.update(STREAMERZ_TEST_OBS_OUTPUT_NAME="TikTok test", STREAMERZ_TEST_OBS_TARGET_PORT="19452")
            elif base == "suite":
                env["STREAMERZ_TEST_OBS_OUTPUT_NAME"] = "YouTube test"
            with (folder / "obs.log").open("w") as log:
                # CEF 151+ otherwise enters Chromium's first-run flow in a clean cache.
                obs = subprocess.Popen(["obs", "--multi", "--disable-missing-files-check", "--disable-gpu", "--password-store=basic", "--no-first-run"], env=env, stdout=log, stderr=log)
                try:
                    deadline = time.monotonic() + 20
                    while True:
                        if obs.poll() is not None:
                            raise RuntimeError(f"OBS exited: {obs.returncode}")
                        try:
                            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                                break
                        except OSError:
                            if time.monotonic() > deadline:
                                raise RuntimeError("OBS did not open its private test port")
                            time.sleep(0.1)
                    # The socket opens before OBS finishes initializing docks and outputs.
                    # Require its real main window, not merely the websocket handshake.
                    deadline = time.monotonic() + 20
                    while subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(obs.pid), "--name", "^OBS "], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
                        if obs.poll() is not None:
                            raise RuntimeError(f"OBS exited during startup: {obs.returncode}")
                        if time.monotonic() > deadline:
                            raise RuntimeError("OBS main window did not finish startup; see obs.log")
                        time.sleep(0.1)
                    if base == "suite":
                        browser_ready(folder, port)
                    with (folder / "backend.log").open("w") as output:
                        result = subprocess.run(["cargo", "test", "--profile", "ci-test", "--locked", "stream_lifecycle_through_backend", "--", "--ignored", "--nocapture"], cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT, timeout=600)
                    if "[ERROR] OBS CallVendorRequest: No vendor was found" in (folder / "backend.log").read_text():
                        raise RuntimeError("Missing optional vendors were incorrectly logged as errors")
                    if result.returncode:
                        raise RuntimeError(f"Backend test failed ({result.returncode}), see {folder / 'backend.log'}")
                    results.append({"case": case, "passed": True})
                    print(f"PASS {case}", flush=True)
                finally:
                    obs.kill()  # Only this script's disposable instance, never the desktop OBS.
                    obs.wait(timeout=10)
        except (OSError, RuntimeError, subprocess.SubprocessError) as error:
            results.append({"case": case, "passed": False, "error": str(error)})
            print(f"FAIL {case}: {error}", flush=True)
        (run / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    return int(any(not result["passed"] for result in results))


if __name__ == "__main__":
    raise SystemExit(main())
