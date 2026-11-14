#!/usr/bin/env python3
"""Exercise the window's process boundary and shared forms on DISPLAY=:99 with private settings.

Build: cargo build --profile fast --features dev-ui
Use VK_DRIVER_FILES for a software Vulkan ICD when Xvfb has no DRI3.
Every bug fixed that only shows on screen adds a step here.
"""
import sys
import locale
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness as smoke
import argparse
import json
import os
import re
import struct
import subprocess
import tempfile
import time
import uuid
from concurrent.futures import ThreadPoolExecutor


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path, default=Path("target/fast/Streamerz"))
    parser.add_argument("--out", type=Path, default=Path("target/ui-smoke"))
    parser.add_argument("--theme", choices=("dark", "light"), default="dark")
    parser.add_argument("--locale", default="en_US", help="Another language runs only the layout checks (nothing cut off, dialogs reopen)")
    parser.add_argument("--wine", action="store_true", help="Run a Windows executable in a dedicated Wine prefix")
    parser.add_argument("--wine-bin", default="wine", help="Wine executable to run a Windows build with")
    parser.add_argument("--wine-prefix", type=Path, default=Path("target/ui-wine"))
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    smoke.PORT = int(os.environ.get("STREAMERZ_SMOKE_PORT", "8098"))
    smoke.require_free_port(smoke.PORT)
    name = "Streamerz UI test " + uuid.uuid4().hex
    with tempfile.TemporaryDirectory(prefix="streamerz-ui-source-") as source:
        config = Path(source) / "streamerz"
        config.mkdir()
        # The fixture deliberately has a non-default name, so the handshake also verifies --config.
        (config / "fixture.json").write_text(json.dumps({"start_hidden": False, "locale": args.locale, "theme": args.theme, "updates": "ignore", "instance_name": name, "websockets": {"port": smoke.PORT}}))
        env = dict(os.environ, DISPLAY=os.environ.get("DISPLAY", ":99"), XDG_CONFIG_HOME=source, STREAMERZ_DEV="1", STREAMERZ_DEV_PORT=str(smoke.PORT))
        env.pop("WAYLAND_DISPLAY", None)
        if args.wine:
            # Upstream Wine stubs the DirectComposition swap chain, so GPUI draws without it under Wine.
            env.update(WINEPREFIX=str(args.wine_prefix.resolve()), WINEDEBUG="-all", WINEDLLOVERRIDES="mscoree,mshtml=;", GPUI_DISABLE_DIRECT_COMPOSITION="1")
        def native_path(path):
            return "Z:" + str(Path(path).resolve()).replace("/", chr(92)) if args.wine else str(path)
        ws = None
        with (out / "runtime.log").open("w") as log:
            app = subprocess.Popen(([args.wine_bin] if args.wine else []) + [str(args.binary.resolve()), "--config", native_path(config / "fixture.json")], env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 30
                while time.monotonic() < deadline:
                    if app.poll() is not None:
                        raise RuntimeError(f"backend exited: {app.returncode}")
                    try:
                        ws = smoke.Ws(expected_name=name)
                        break
                    except OSError:
                        time.sleep(.1)
                if ws is None:
                    raise RuntimeError("no isolated backend")
                def ui(command):
                    reply = ws.ask({"request": "StreamerzUi", "id": "ui-test", "do": command})
                    if reply.get("status") != "ok":
                        raise RuntimeError(reply)
                    return reply.get("result", "")
                def load_page(name):
                    result = ui("page " + name)
                    assert result == "ok", (name, result)
                    deadline = time.monotonic() + 10
                    while ui("pageready") != "true":
                        if time.monotonic() >= deadline: raise RuntimeError(f"page did not load: {name}")
                        time.sleep(.05)
                def click(label, occurrence=0, role=None, button=1, count=1):
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        found = json.loads(ui("bounds " + label))
                        matches = [entry for entry in found if entry["label"] == label and entry.get("enabled", True) and (role is None or role in entry["role"])]
                        if len(matches) > occurrence:
                            entry = matches[occurrence]
                            x, y = round(entry["x"] + entry["w"] / 2), round(entry["y"] + entry["h"] / 2)
                            print(f"click {label}: {entry['role']} at {x},{y}", flush=True)
                            subprocess.run(["xdotool", "mousemove", str(x), str(y), "click", "--repeat", str(count), str(button)], env=env, check=True)
                            if label in ("Add", "Edit", "Cancel", "Save", "OK", "Edit JSON"):
                                time.sleep(.3)
                            return
                    (out / "lookup-failure.json").write_text(ui("bounds "))
                    ui("shot " + native_path(out / "lookup-failure.png"))
                    raise RuntimeError(f"no control {label}: {found}")
                def type_value(label, value):
                    subprocess.run(["xdotool", "windowfocus", window], env=env, check=True)
                    if label == "Search":
                        print(ui("windows"), subprocess.check_output(["xdotool", "getwindowgeometry", "--shell", window], env=env, text=True), flush=True)
                    click(label, role="TextInput")
                    if label == "Search":
                        shot("search-focused")
                        print("page after focus:", ui("currentpage"), flush=True)
                    subprocess.run(["xdotool", "key", "ctrl+a", "type", "--clearmodifiers", value], env=env, check=True)
                def get(kind, name):
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        reply = ws.ask({"request": "StreamerzGet", "what": kind, "itemId": "", "name": name, "id": "read"})
                        if reply.get("status") == "ok": return reply["item"]
                        time.sleep(.05)
                    shot("save-failure")
                    print(ws.ask({"request":"StreamerzGet", "what":kind, "itemId":"", "id":"failed-read"}), flush=True)
                    raise RuntimeError(f"the form did not save {kind} {name}")
                def shot(stem):
                    path = out / (stem + ".png")
                    result = ui("shot " + native_path(path))
                    if not result.startswith("saved "):
                        raise RuntimeError(result)
                    (out / (stem + ".txt")).write_text(ui("tree"))
                    return struct.unpack(">II", path.read_bytes()[16:24])
                def children():
                    if args.wine:
                        try:
                            response = ws.ask({"request":"StreamerzUi", "id":"renderer-process", "do":"process"})
                        except (TimeoutError, OSError):
                            # A request sent as the renderer closes is never answered: the renderer is gone.
                            return []
                        value = response.get("result", "")
                        return [int(value)] if response.get("status") == "ok" and value.isdigit() else []
                    result = []
                    for task in Path(f"/proc/{app.pid}/task").iterdir():
                        try:
                            for child in (task / "children").read_text().split():
                                if b"--frontend" in Path(f"/proc/{child}/cmdline").read_bytes():
                                    result.append(int(child))
                        except (FileNotFoundError, OSError):
                            pass
                    return list(dict.fromkeys(result))
                def native_window(renderer):
                    if not args.wine:
                        return subprocess.check_output(["xdotool", "search", "--onlyvisible", "--pid", str(renderer)], env=env, text=True).splitlines()[0]
                    candidates = subprocess.check_output(["xdotool", "search", "--onlyvisible", "--name", "Streamerz"], env=env, text=True).splitlines()
                    usable = []
                    for candidate in candidates:
                        geometry = subprocess.check_output(["xdotool", "getwindowgeometry", "--shell", candidate], env=env, text=True)
                        if re.search(r"^WIDTH=(\d+)$", geometry, re.M) and int(re.search(r"^WIDTH=(\d+)$", geometry, re.M)[1]) >= 450:
                            usable.append(candidate)
                    if len(usable) != 1: raise RuntimeError(f"ambiguous test window: {usable}")
                    return usable[0]
                def resize(width, height):
                    if args.wine:
                        # Wine on a bare Xvfb never reports X resizes to the app, so ask the app itself.
                        assert ui(f"resize {width} {height}") == "ok"
                        time.sleep(.3)
                        assert f" {width}x{height} " in ui("windows"), ui("windows")
                        return
                    subprocess.run(["xdotool", "windowsize", "--sync", window, str(width), str(height)], env=env, check=True)
                    if args.wine:
                        # Wine's X window includes native decorations; the app shot contains only its client.
                        bounds = re.search(r" at (-?\d+),(-?\d+) (\d+)x(\d+) shown", ui("windows"))
                        if not bounds: raise RuntimeError("missing client bounds")
                        actual_width, actual_height = int(bounds[3]), int(bounds[4])
                        if (actual_width, actual_height) != (width, height):
                            subprocess.run(["xdotool", "windowsize", "--sync", window, str(2 * width - actual_width), str(2 * height - actual_height)], env=env, check=True)
                deadline = time.monotonic() + 20
                while not children() and time.monotonic() < deadline:
                    time.sleep(.1)
                renderers = children()
                assert len(renderers) == 1, renderers
                renderer = renderers[0]
                ui("windows")
                (out / "renderer.txt").write_text(ui("renderer"))
                window = native_window(renderer)
                strings = json.loads((Path(__file__).resolve().parent.parent / "assets" / "locales" / f"{args.locale}.json").read_text())
                def no_overflow(where):
                    # Nothing on screen is pushed past its dialog, card or window (a button row too long for a
                    # dialog in Portuguese once was).
                    found = json.loads(ui("overflow"))
                    if found:
                        shot(f"overflow-{where}")
                        raise AssertionError(f"cut off on {where}: {found}")
                def layout_pass():
                    for page in ("Accounts", "Commands", "Timers", "Files", "Stream Apps", "Servers", "Queues", "Globals", "History", "Backups", "Crashes", "Plugins", "Overlays", "Log", "Settings", "Donations", "MCP", "Groups", "Integrations", "Community", "Polls", "Stream", "Chat", "Actions"):
                        for width, height in ((1100, 800), (540, 700)):
                            resize(width, height)
                            load_page(page)
                            no_overflow(f"{page}-{width}")
                    # TikTok's setup dialog: it fits in both of its modes, and it opens again after its footer's
                    # Close (that Close once skipped the dialog's cancel, and the page never opened it again).
                    resize(1100, 1000)
                    load_page("Accounts")
                    close = strings["dialogs.close"]
                    def footer_close():
                        return [e for e in json.loads(ui("bounds " + close)) if e["label"] == close and e["w"] > 30]
                    for attempt in range(2):
                        click(strings["btn.set_up"])
                        deadline = time.monotonic() + 10
                        while not footer_close() and time.monotonic() < deadline:
                            time.sleep(.05)
                        assert footer_close(), f"the TikTok dialog did not open (attempt {attempt + 1})"
                        time.sleep(.4)
                        no_overflow(f"tiktok-dialog-{attempt}")
                        toggle = "accounts.tiktok_sending.simple_mode" if attempt else "accounts.tiktok_sending.advanced_mode"
                        click(strings[toggle])
                        time.sleep(.4)
                        no_overflow(f"tiktok-dialog-{attempt}-toggled")
                        entry = footer_close()[0]
                        subprocess.run(["xdotool", "mousemove", str(round(entry["x"] + entry["w"] / 2)), str(round(entry["y"] + entry["h"] / 2)), "click", "1"], env=env, check=True)
                        deadline = time.monotonic() + 10
                        while footer_close() and time.monotonic() < deadline:
                            time.sleep(.05)
                        assert not footer_close(), "Close left the TikTok dialog open"
                layout_pass()
                if args.locale != "en_US":
                    print(f"ok: layout in {args.locale}", flush=True)
                    return
                for page in ("Commands", "Timers", "Files", "Stream Apps", "Servers"):
                    for width, height in ((1100, 800), (540, 700)):
                        resize(width, height)
                        page_name = page
                        load_page(page_name)
                        stem = f"{page.lower().replace(' ', '_')}-{width}"
                        assert shot(stem) == (width, height)
                        from PIL import Image
                        with Image.open(out / (stem + ".png")) as captured:
                            red, green, blue = captured.convert("RGB").getpixel((width - 3, height // 2))
                            assert (min(red, green, blue) > 200) if args.theme == "light" else (max(red, green, blue) < 80), (args.theme, red, green, blue)
                # Pages built on the core's table and section models: each loads, draws and keeps its window size.
                for page in ("Queues", "Globals", "History", "Backups", "Crashes", "Plugins", "Overlays", "Log", "Settings", "Donations", "MCP", "Groups", "Integrations", "Community", "Polls", "Stream", "Chat", "Actions"):
                    for width, height in ((1100, 800), (540, 700)):
                        resize(width, height)
                        load_page(page)
                        assert shot(f"{page.lower()}-{width}") == (width, height), page
                # Donations: three rows each have their own Edit button (they once shared one element id, and only
                # the last answered); the MCP connection's fields are inline.
                resize(1100, 800)
                load_page("Donations")
                edits = [entry for entry in json.loads(ui("bounds Edit")) if entry["label"] == "Edit"]
                assert len(edits) == 3, edits
                click("Edit", occurrence=0)
                click("Cancel")
                load_page("MCP")
                assert any(entry["label"] == "Port" for entry in json.loads(ui("bounds Port"))), "MCP port field"
                # Actions: the tree, the open action's editor, the pickers, the forms and the operations are the core's.
                def actions_page(action=""):
                    return ws.ask({"request": "StreamerzCoreView", "domain": "actions", "actionId": action, "id": "actions"})["actions"]
                assert actions_page()["editor"] is None
                added = ws.ask({"request": "StreamerzEditEntity", "kind": "act:action_add", "itemId": "", "changes": [{"field": 0, "value": "Smoke action"}], "id": "action-add"})
                assert added["status"] == "ok", added
                opened = added["saved"]
                trigger_form = ws.ask({"request": "StreamerzEntityForm", "kind": "act:trigger_add", "actionId": opened, "code": "101", "id": "trigger-form"})
                assert trigger_form["status"] == "ok", trigger_form
                trigger = ws.ask({"request": "StreamerzEditEntity", "kind": trigger_form["kind"], "itemId": "", "changes": [], "id": "trigger-add"})
                assert trigger["status"] == "ok", trigger
                sub_form = ws.ask({"request": "StreamerzEntityForm", "kind": "act:sub_add", "actionId": opened, "code": "99900", "anchor": "", "id": "sub-form"})
                assert sub_form["status"] == "ok", sub_form
                sub = ws.ask({"request": "StreamerzEditEntity", "kind": sub_form["kind"], "itemId": "", "changes": [], "id": "sub-add"})
                assert sub["status"] == "ok", sub
                editor = actions_page(opened)["editor"]
                assert [row["id"] for row in editor["triggers"]["nodes"]] == [trigger["saved"]], editor
                assert [row["id"] for row in editor["subs"]["nodes"]] == [sub["saved"]], editor
                for kind in ("trigger", "sub", "template"):
                    picker = ws.ask({"request": "StreamerzCoreView", "domain": "actions_picker", "kind": kind, "id": "picker"})
                    assert picker["status"] == "ok" and picker["nodes"], picker
                # The sidebar's fold button stays on top when a short window scrolls the page list.
                resize(1100, 320)
                load_page("Actions")
                subprocess.run(["xdotool", "mousemove", "60", "150", "click", "--repeat", "12", "5"], env=env, check=True)
                time.sleep(.3)
                fold = [e for e in json.loads(ui("bounds Show only the icons")) if e["label"] == "Show only the icons"]
                top = int(re.search(r" at (-?\d+)\.?\d*,(-?\d+)", ui("windows"))[2])
                assert fold and fold[0]["y"] - top < 40, f"the fold button scrolled away: {fold}, window top {top}"
                resize(1100, 800)
                # A notice that is only acknowledged has one button: Check now in a dev build says it doesn't update.
                load_page("Settings")
                at = re.search(r" at (-?\d+)\.?\d*,(-?\d+)", ui("windows"))
                subprocess.run(["xdotool", "mousemove", str(int(at[1]) + 600), str(int(at[2]) + 400), "click", "--repeat", "15", "5"], env=env, check=True)
                time.sleep(.3)
                click("Check now")
                deadline = time.monotonic() + 10
                while not [e for e in json.loads(ui("bounds OK")) if e["label"] == "OK"] and time.monotonic() < deadline:
                    time.sleep(.05)
                every = json.loads(ui("bounds "))
                assert [e for e in every if "cancel-prompt" in e.get("path", "")], "no OK on the notice"
                assert not [e for e in every if "answer-prompt" in e.get("path", "")], "an empty second button on the notice"
                click("OK")
                # The trigger picker: the tree starts right under its search box (the box once took half the
                # dialog), and Cancel and OK sit in the dialog's footer, with no Close beside them.
                load_page("Actions")
                click("Smoke action")
                click("Triggers: Add")
                deadline = time.monotonic() + 10
                while not [e for e in json.loads(ui("bounds Core")) if e["label"] == "Core"] and time.monotonic() < deadline:
                    time.sleep(.05)
                # Measured once the dialog has finished opening: while it opens, its parts are still moving.
                gap, deadline = -1, time.monotonic() + 3
                while True:
                    search = [e for e in json.loads(ui("bounds Search triggers")) if e["label"] == "Search triggers"][0]
                    first = [e for e in json.loads(ui("bounds Core")) if e["label"] == "Core"][0]
                    gap = first["y"] - (search["y"] + search["h"])
                    if 0 <= gap < 48 or time.monotonic() > deadline:
                        break
                    time.sleep(.1)
                shot("trigger-picker")
                assert 0 <= gap < 48, f"the picker's tree is {gap}px under its search box"
                # The header's X is named Close too; a footer's Close button is wider.
                assert not [e for e in json.loads(ui("bounds Close")) if e["label"] == "Close" and e["w"] > 30], "a Close button under the picker's own"
                click("Cancel")
                for operation, item in (("sub_toggle", sub["saved"]), ("trigger_toggle", trigger["saved"])):
                    done = ws.ask({"request": "StreamerzCoreAction", "domain": "actions", "action": operation, "itemId": item, "actionId": opened, "id": operation})
                    assert done["status"] == "ok", done
                assert ws.ask({"request": "StreamerzCoreAction", "domain": "actions", "action": "action_delete", "itemId": opened, "id": "action-delete"})["status"] == "ok"
                assert actions_page()["tree"]["nodes"] == []
                # Operations reach the core and the page follows: a new global shows up, then goes.
                load_page("Globals")
                saved = ws.ask({"request": "StreamerzEditEntity", "kind": "global", "itemId": "", "changes": [{"field": 0, "value": "smoke_global"}, {"field": 1, "value": "7"}], "id": "global-save"})
                assert saved["status"] == "ok", saved
                rows = ws.ask({"request": "StreamerzCoreView", "domain": "globals", "id": "globals"})["tables"][0]["rows"]
                assert any(row["id"] == "1|smoke_global" for row in rows), rows
                assert ws.ask({"request": "StreamerzDelete", "what": "global", "itemId": "1|smoke_global", "id": "global-delete"})["status"] == "ok"
                queues = ws.ask({"request": "StreamerzCoreAction", "domain": "queues", "action": "pause_all", "id": "pause"})
                assert queues["status"] == "ok", queues
                assert ws.ask({"request": "StreamerzCoreAction", "domain": "queues", "action": "resume_all", "id": "resume"})["status"] == "ok"
                assert ws.ask({"request": "StreamerzCoreSetting", "domain": "settings", "key": "instance_name", "value": name, "id": "name"})["status"] == "ok"
                assert ws.ask({"request": "StreamerzCoreStatus", "id": "status"})["status"] == "ok"
                # Chat: the page streams by cursor, sending needs text and a place to go, options read as sections.
                chat = ws.ask({"request": "StreamerzCoreView", "domain": "chat", "id": "chat"})
                assert chat["status"] == "ok" and len(chat["chat"]["platforms"]) == 4, chat
                assert ws.ask({"request": "StreamerzChat", "op": "send", "text": "  ", "id": "chat-empty"})["status"] == "error"
                assert ws.ask({"request": "StreamerzChat", "op": "show", "platform": "kick", "shown": False, "id": "chat-hide"})["status"] == "ok"
                assert ws.ask({"request": "StreamerzChat", "op": "show", "platform": "kick", "shown": True, "id": "chat-show"})["status"] == "ok"
                assert ws.ask({"request": "StreamerzChat", "op": "clear", "id": "chat-clear"})["status"] == "ok"
                options = ws.ask({"request": "StreamerzCoreView", "domain": "chat_options", "id": "chat-options"})
                assert len(options["sections"]) == 2, options
                assert ws.ask({"request": "StreamerzCoreSetting", "domain": "chat_options", "key": "chat_time", "value": True, "id": "chat-time"})["status"] == "ok"
                # Polls: with no account every button is off and says why; the form refuses an empty poll.
                polls = ws.ask({"request": "StreamerzCoreView", "domain": "polls", "id": "polls"})
                assert [table["kind"] for table in polls["tables"]] == ["poll", "prediction"], polls
                assert ws.ask({"request": "StreamerzCoreAction", "domain": "polls", "action": "lock", "id": "polls-lock"})["status"] == "error"
                refused = ws.ask({"request": "StreamerzEditEntity", "kind": "poll", "itemId": "", "changes": [], "id": "polls-empty"})
                assert refused["status"] == "error", refused
                created = ws.ask({"request": "StreamerzEditEntity", "kind": "user_group", "itemId": "", "changes": [{"field": 0, "value": "Smoke group"}], "id": "group-add"})
                assert created["status"] == "ok", created
                group = ws.ask({"request": "StreamerzCoreView", "domain": "groups", "id": "groups"})["tables"][0]["rows"][0]
                assert group["name"] == "Smoke group", group
                assert ws.ask({"request": "StreamerzCoreView", "domain": "group_members", "itemId": group["id"], "id": "members"})["tables"][0]["rows"] == []
                assert ws.ask({"request": "StreamerzCoreAction", "domain": "groups", "action": "delete", "itemId": group["id"], "id": "group-delete"})["status"] == "ok"
                quote = ws.ask({"request": "StreamerzEditEntity", "kind": "quote", "itemId": "", "changes": [{"field": 0, "value": "Smoke quote"}], "id": "quote-add"})
                assert quote["status"] == "ok", quote
                community = ws.ask({"request": "StreamerzCoreView", "domain": "community", "id": "community"})
                assert [tab["id"] for tab in community["tabs"]["options"]] == ["community_user", "quote", "credit"], community
                quotes = ws.ask({"request": "StreamerzCoreView", "domain": "community", "view": {"tab": "quote"}, "id": "community-quotes"})["tables"]
                assert [table["kind"] for table in quotes] == ["quote"], quotes
                assert quotes[0]["rows"][0]["cells"][1] == "Smoke quote", quotes[0]
                assert ws.ask({"request": "StreamerzCoreAction", "domain": "community", "action": "quote_delete", "itemId": quotes[0]["rows"][0]["id"], "id": "quote-delete"})["status"] == "ok"
                assert ws.ask({"request": "StreamerzCoreAction", "domain": "community", "action": "credits_test", "id": "credits-test"})["status"] == "ok"
                assert ws.ask({"request": "StreamerzCoreAction", "domain": "community", "action": "credits_reset", "id": "credits-reset"})["status"] == "ok"
                model = ws.ask({"request": "StreamerzCoreView", "domain": "commands", "id": "model"})
                assert model["tables"][0]["rows"][0]["cells"][4]["platforms"] == ["twitch", "youtube", "kick", "tiktok"], model
                load_page("Commands")
                click("Add")
                time.sleep(.25)
                shot("command-form-narrow")
                type_value("Name", "Smoke command")
                click("OK")
                command_id = get("command", "Smoke command")["id"]
                shot("command-created")
                click("On/Off")
                reply = ws.ask({"request": "StreamerzGet", "what": "command", "itemId": command_id, "id": "read"})
                assert reply["item"]["enabled"] is False, reply
                click("Edit")
                type_value("Name", "Cancelled name")
                click("Cancel")
                reply = ws.ask({"request": "StreamerzGet", "what": "command", "itemId": command_id, "id": "read"})
                assert reply["item"]["name"] == "Smoke command", reply
                # Filtering and both row and empty-list menus use the same shared actions.
                type_value("Search", "Smoke command")
                click("Smoke command", role="Cell", button=3)
                shot("command-context-menu")
                click("Edit")
                type_value("Name", "Filtered command")
                click("OK")
                edited = ws.ask({"request":"StreamerzGet", "what":"command", "itemId":command_id, "id":"filtered-edit"})
                assert edited["item"]["name"] == "Filtered command", edited
                type_value("Search", "no matching command")
                click("Nothing matches the search.", button=3)
                shot("empty-context-menu")
                click("Add")
                click("Cancel")
                type_value("Search", "")
                # Invalid numeric values stay in the dialog and leave the core unchanged.
                load_page("Timers")
                click("Add")
                type_value("Name", "Smoke timer")
                type_value("Interval (s)", "invalid")
                click("OK")
                assert 'whole number' in ui("tree")
                shot("timer-validation")
                type_value("Interval (s)", "45")
                click("OK")
                assert get("timer", "Smoke timer")["interval"] == 45
                click("Edit")
                type_value("Name", "Unsaved JSON timer")
                click("Edit JSON")
                shot("timer-json-draft")
                assert get("timer", "Smoke timer")["interval"] == 45
                click("Cancel")
                click("Edit")
                click("Edit JSON")
                type_value("Edit JSON", "broken")
                click("OK")
                assert "Not valid JSON" in ui("tree")
                item = get("timer", "Smoke timer")
                item["name"] = "JSON timer"
                type_value("Edit JSON", json.dumps(item))
                click("OK")
                assert get("timer", "JSON timer")["interval"] == 45
                # Two independently selected lists share the controller and forms.
                load_page("Files")
                click("Add")
                type_value("Name", "Smoke watcher")
                click("Enabled", role="CheckBox")
                click("OK")
                assert get("file_watcher", "Smoke watcher")["enabled"] is False
                shot("file-watcher-created")
                click("Add", occurrence=1)
                type_value("Name", "Smoke tail")
                click("Enabled", role="CheckBox")
                click("OK")
                assert get("file_tail", "Smoke tail")["enabled"] is False
                shot("file-tail-created")
                # Deleting a row has a cancellable in-window confirmation.
                click("Delete", occurrence=1)
                click("Cancel")
                assert get("file_tail", "Smoke tail")["name"] == "Smoke tail"
                # The click can land while the cancelled confirmation is still leaving: ask again if none opens.
                for attempt in range(3):
                    click("Delete", occurrence=1)
                    time.sleep(.5)
                    if any(entry["label"] == "Delete Smoke tail" for entry in json.loads(ui("bounds Delete Smoke tail"))):
                        break
                shot("delete-confirmation")
                click("Delete Smoke tail")
                time.sleep(.3)
                reply = ws.ask({"request":"StreamerzGet", "what":"file_tail", "name":"Smoke tail", "itemId":"", "id":"deleted"})
                assert reply["status"] == "error", reply
                load_page("Servers")
                for title, kind in (("WebSocket server","server_websocket"), ("HTTP server","server_http"), ("UDP server","server_udp"), ("Stream Deck server","server_stream_deck")):
                    click("Edit: " + title)
                    time.sleep(.3)
                    shot(kind + "-form")
                    type_value("Port", "0")
                    click("OK")
                    assert "1 to 65535" in ui("tree")
                    click("Cancel")
                click("HTTP server", role="Group", button=3)
                shot("server-settings-context-menu")
                click("Edit", role="MenuItem")
                time.sleep(.3)
                assert "HTTP server" in ui("tree")
                type_value("Port", "7488")
                click("OK")
                settings = ws.ask({"request":"StreamerzGet","what":"server_http","id":"http"})
                assert settings["item"]["port"] == 7488 and settings["item"]["autoStart"] is False, settings
                # Client changes keep disabled clients free of sockets and timers.
                click("Add")
                type_value("Name", "Smoke client")
                click("OK")
                assert get("websocket_client", "Smoke client")["autoConnect"] is False
                shot("websocket-client-created")
                controls = json.loads(ui("bounds Smoke client"))
                switches = [control for control in controls if "Switch" in control["role"]]
                assert switches and all(not control["enabled"] for control in switches), controls
                # Pending core input survives ending and recreating the renderer process.
                def ask_input():
                    client = smoke.Ws(expected_name=name)
                    try:
                        return client.ask({"request":"StreamerzRunSubaction", "item":{"type":1023,"title":"Smoke prompt","prompt":"Your name","outputVariable":"answer"}, "id":"prompt-run"})
                    finally:
                        client.s.close()
                with ThreadPoolExecutor(max_workers=1) as executor:
                    response = executor.submit(ask_input)
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        prompts = ws.ask({"request":"StreamerzPrompts", "id":"pending"})["prompts"]
                        if prompts: break
                        time.sleep(.05)
                    assert prompts
                    prompt_id = prompts[0]["id"]
                    ui("toggle")
                    deadline = time.monotonic() + 10
                    while children() and time.monotonic() < deadline: time.sleep(.05)
                    assert not children()
                    assert ws.ask({"request":"StreamerzPrompts", "id":"pending"})["prompts"][0]["id"] == prompt_id
                    ui("toggle")
                    deadline = time.monotonic() + 10
                    while not children() and time.monotonic() < deadline: time.sleep(.05)
                    assert children(), "the renderer did not reopen"
                    ui("windows")
                    renderer = children()[0]
                    window = native_window(renderer)
                    time.sleep(.3)
                    type_value("Your name", "Streamer")
                    shot("core-input-restored")
                    click("OK")
                    assert response.result(timeout=10)["args"]["answer"] == "Streamer"
                load_page("Commands")
                click("Hide")
                deadline = time.monotonic() + 15
                while (bool(children()) if args.wine else Path(f"/proc/{renderer}").exists()) and time.monotonic() < deadline:
                    time.sleep(.05)
                assert not (bool(children()) if args.wine else Path(f"/proc/{renderer}").exists()), "hiding left the renderer alive"
                assert app.poll() is None, "hiding stopped the backend"
                reply = ws.ask({"request": "GetInfo", "id": "hidden"})
                assert reply.get("status") == "ok", reply
                time.sleep(3)
                def ticks():
                    fields = Path(f"/proc/{app.pid}/stat").read_text().rsplit(")", 1)[1].split()
                    return int(fields[11]) + int(fields[12])
                started, began = ticks(), time.monotonic()
                time.sleep(5)
                elapsed = time.monotonic() - began
                cpu = 100 * (ticks() - started) / os.sysconf("SC_CLK_TCK") / elapsed
                status = Path(f"/proc/{app.pid}/status").read_text().splitlines()
                rss = int(next(line.split()[1] for line in status if line.startswith("VmRSS:")))
                (out / "smaps-rollup.txt").write_text(Path(f"/proc/{app.pid}/smaps_rollup").read_text())
                # Where the memory is, by what is mapped (the program, each library, the heap), largest first.
                # The program's own pages are dropped when hidden only if no other running copy of the same
                # file shares them (an MCP server started from this build does): measure with none running.
                by_file, name = {}, ""
                for line in Path(f"/proc/{app.pid}/smaps").read_text().splitlines():
                    head = line.split()
                    if head and "-" in head[0] and not head[0].endswith(":"):
                        name = head[5] if len(head) > 5 else "(anonymous)"
                    elif line.startswith("Rss:"):
                        by_file[name] = by_file.get(name, 0) + int(head[1])
                (out / "smaps-by-file.txt").write_text("".join(f"{kib:8} KiB  {name}\n" for name, kib in sorted(by_file.items(), key=lambda item: -item[1]) if kib))
                (out / "measurements.json").write_text(json.dumps({"theme":args.theme, "hidden_rss_kib": rss, "idle_cpu_percent_one_core":cpu, "sample_seconds":elapsed, "setup": "isolated development backend after renderer exit, no live connections", "wine":args.wine}, indent=2) + "\n")
                print(f"ok: shared forms, cancellation, toggle, native window captures, renderer exit, live backend; RSS {rss} KiB, idle CPU {cpu:.2f}%", flush=True)
            finally:
                if ws is not None:
                    ws.s.close()
                if app.poll() is None:
                    app.terminate()
                    app.wait(timeout=15)
                if args.wine:
                    subprocess.run([os.environ.get("WINESERVER", "wineserver"), "-k"], env=env, check=False)


if __name__ == "__main__":
    main()
