# Testing

How Streamerz is tested: every trigger, sub-action and request, on Linux and Windows. The goal is simple:
nothing in `src/types.rs` ships without a test that runs it, or a written reason why it can't.

## What "tested" means

| item | tested when |
|---|---|
| sub-action | a scenario runs it and checks what came of it: the request it made (Helix, OBS, Kick, YouTube, Streamlabs, Spotify), the chat it sent, the arguments or globals it set, or the log line it wrote |
| trigger | a scenario feeds the platform's own payload (EventSub, Pusher, InnerTube, obs-websocket, webhook JSON...) through the real parser, and checks the trigger fired once with its documented arguments |
| request | the WebSocket, HTTP or UDP request answers the same shape as the real Streamer.bot |
| page | the smoke visits it, narrow and wide, with no crash and a screenshot |

## Layers

### 1. Unit tests (`cargo test`)

What exists today: parsers, formatting, math, storage, secrets, servers. Every bug fixed adds one.

### 2. Conformance scenarios (`tests/scenarios`, headless)

The scenario runner (`src/scenario.rs`) runs actions on the real engine with no window and nothing sent.
For this plan it gets:

- **Fake connectors** (`fakes` in a scenario): Twitch, Kick, YouTube, TikTok, OBS, Streamlabs Desktop and
  Spotify, each a stand-in for the real handle that keeps every request and answers from the scenario's
  table (`"GET users": {...}`). `expect.calls` checks the requests, in order, by partial match.
- **Platform events** (`{"event": {"platform": "twitch", "type": "channel.follow", "data": {...}}}`): the
  payload goes through the platform's real handler (`twitch::handle_notification`,
  `kick::handle_kick_event`, `youtube::handle_innertube_actions`, the OBS message handler,
  `donations::handle_*`, `tiktok::handle_tiktok_event`), so a trigger test proves the parsing, not only
  the action matching.
- **Arguments** (`expect.args`): what an action's last run had in its arguments, to check the variables a
  trigger or a sub-action sets.
- **The ratchet** (`every_code_is_tested_or_excluded`): every code in `TRIGGERS` and `SUB_ACTIONS` is used
  by some scenario, or listed in `tests/scenarios/excluded.json` with its reason (out of scope, needs
  hardware...). A new code with neither fails `cargo test`. The excluded list only shrinks.

One file per area: `core`, `logic`, `globals`, `commands`, `twitch-*`, `obs`, `kick`, `youtube`,
`tiktok`, `streamlabs-desktop`, `donations`, `webhooks`, `files`, `quotes`, `spotify`, `servers`.

They run:

- on Linux: `cargo test scenarios` (the `lint` workflow);
- on Windows: `Streamerz.exe --scenario tests/scenarios` in the Windows build job (real Windows), and under
  Wine locally;
- from an assistant: the MCP `run_scenario` tool.

### 3. Protocol parity with Streamer.bot (`dev/sb.sh`, `dev/sb-oracle.py`, `dev/sb-probe.py`)

The real Streamer.bot runs under system Wine (`dev/sb.sh`, ws://127.0.0.1:8080; Proton crashes it).
`dev/sb-oracle.py` imports actions into it and reads back what it kept (which triggers it drops),
`dev/sb-probe.py` subscribes to its Raw Action events to capture the exact arguments a trigger passes, and
both talk to Streamerz (8090) the same way to compare. Back up its `data/` folder first and put it back
after; nothing read from it goes in the repo.

### 4. UI smoke (`dev/smoke.py`)

Every page, narrow and wide, right-click menus, the user card, with screenshots and a crash check:

- Linux: the debug build on Xvfb (`DISPLAY=:99`);
- Windows: `Streamerz.exe` under Wine on the same display (`dev/smoke.py --exe`, or `dev/windows.sh smoke`).

### 5. Live checks (`dev/live.py`)

Against the real services, with `STREAMERZ_DEV=1` (chat is kept, not sent) and through the MCP tools:
OBS (scenes, sources, filters, recording, screenshots), Twitch, Kick, YouTube and TikTok channels that are
live (chat, events, user cards, emotes), Spotify. What changes a channel (ads, raids, bans, polls,
predictions, title) runs only on the test account, by hand, from a checklist in the script's output.

### 6. Budget (`dev/memory.py`, `dev/stress.py`)

Hidden RSS and idle CPU before and after, in the PR. Fakes and scenarios are never part of a normal start:
they cost nothing when the app runs.

## Linux input

`python3 -P dev/linux-input.py [target/fast/Streamerz]` runs the binary as an X11 session on the hidden
display and has xdotool press a grabbed combination (with NumLock on and off) and click every button: Key
Press, Mouse Click, its Button filter, double clicks, the pointer's place and the wheel (not a click). On
Wayland the devices are read instead; `devinput.rs`'s tests feed its reader through a FIFO, and the rest is
in [UNTESTED.md](UNTESTED.md).

## Windows under Wine

`dev/windows.sh` cross-builds `Streamerz.exe` as the Windows CI job does (cargo-xwin, MSVC, static
runtime, release), then runs it under Wine on the hidden display:

- `dev/windows.sh tests`: every unit test, built for Windows (Windows-only ones included: DPAPI, the Run key,
  Toolhelp, error texts), in a prefix of its own (`/tmp/szwinetest`);
- `dev/windows.sh scenarios`: every scenario on the exe;
- `dev/windows.sh smoke`: `dev/smoke.py --exe`, every page, ending with the session's end (`wineboot -e`,
  which sends `WM_ENDSESSION`);
- `dev/windows.sh live`: `dev/windows-live.py`, the OS paths the scenarios fake, made real: RegisterHotKey
  from the keyboard and from Keyboard Press (SendInput), the low-level mouse hook, the clipboard listener
  (text copied in Notepad), Toolhelp (a program started from outside), winmm MIDI out and back in (through
  ALSA's "Midi Through", which Wine shows as a device), WinRT toasts, SAPI, the media session, Run a
  Program's output, the tray, and the system's language and light or dark theme from the registry. With
  `SZ_ONLINE="twitch:CHANNEL kick:CHANNEL youtube:@HANDLE"` it also reads live chats through the exe's own
  sockets and TLS (development mode sends nothing). Use a scratch prefix (`WINEPREFIX=/tmp/szwine`, the
  default): it changes the registry.

What Wine can't show, left to a real Windows box (all of it, and everything else not tested yet, is in
[UNTESTED.md](UNTESTED.md), for anyone who can try it):

- speech recognition: Wine has no SAPI recognizer (`0x80040154`), so Speech to Text says it's not available;
- the media session (Spotify local): Wine has no `GlobalSystemMediaTransportControlsSessionManager`;
- a toast's look and its click (Toast Clicked): Wine accepts the toast but shows nothing to click;
- subfolders that already exist when a File Watcher starts: Wine watches only those made later (Windows
  watches both; the unit test skips that one check under Wine);
- DPAPI and the Run key work under Wine, but only Windows proves them; the Windows CI job runs the DPAPI,
  storage and instance tests and the scenarios on real Windows.

Wine's own bug: a hotkey registered for a thread (no window) crashes wineserver when SendInput presses it.
Hotkeys are registered on a message-only window instead, which Windows treats the same.

## Writing a scenario

A scenario is JSON (see the top of `src/scenario.rs` for every key). What it can do:

- steps: `run` an action, `fire` a trigger, `chat` as a viewer, `event` (a platform's raw payload through its
  real handler), `request` (a WebSocket request, with the `reply` it must give), `app` (what the window or
  an OS hook does: started, test, addGroup, removeGroup, setGlobal, deleteGlobal, watchers, hotkey, mouse,
  midi, speech, clipboard, toastClicked, webhook), `wait`;
  a step can say how many actions it starts (`ran`) or that it must fail (`error`);
- `fakes`: twitch, kick, youtube, obs, streamlabsDesktop, spotify and http, answered from a table
  (`{"$seq": [a, b]}` for answers that change, `{"error": "..."}` for a failure);
- placeholders: `{tmp}` (a folder of its own), `{http}` (the local web server); `"only": "linux"` or
  `"windows"`;
  `"debug": true` for debug lines;
- `expect`: chat, log, notLog, calls, notCalls, runs (actions with their start and end arguments),
  globals, config.

`python3 -P dev/scenario-todo.py` takes what scenarios now cover off the todo list and says what's left.

## Status

- [x] 1. Plan and baseline: 369 unit tests, 5 of 497 triggers and 20 of 453 sub-actions in a scenario.
- [x] 2. Runner: fakes, platform events, requests, app steps, calls, runs, config, the ratchet.
- [x] 3. Engine core, commands, timers, queues, signals, globals, groups, files, devices, network.
- [x] 4. Twitch: every handled EventSub type and every Helix sub-action.
- [x] 5. OBS Studio and Streamlabs Desktop (sub-actions, events; OBS connect and disconnect over a fake
      obs-websocket server).
- [x] 6. Kick, YouTube, TikTok (sub-actions, events; Kick's Pusher and YouTube's chat over fake servers).
- [x] 7. Donations, shops, webhooks (with their signature checks), Discord, IFTTT, pronouns, remote.
- [x] 8. Spotify (through the real adapter to a local server), song requests, countdowns.
- [x] 9a. Windows: the scenarios run on the exe in the Windows build job.
- [x] 9b. Windows under Wine locally: `dev/windows.sh scenarios` cross-builds the exe as CI does
      (cargo-xwin, MSVC, release) and runs every scenario on it.
- [ ] 10. Connections and polling: Twitch EventSub sessions, raids, ads, viewer counts, present viewers,
      7TV/BTTV emote changes, YouTube monitoring and statistics, TikTok connection and stream, Streamlabs
      Desktop connection, remote instances, TreatStream, local Spotify (`todo` in `tests/excluded.json`).
- [x] 11. Parity with the real Streamer.bot: dropped triggers, file watcher and tail arguments, append
      newlines, the Stream Deck server (8059) and its pushes, GetActions.
- [x] 11a. Triggers that were never fired now are: File Watcher, File Tail, Process Started/Stopped,
      MIDI In, Stream Deck, Speech to Text, Toast Clicked, Clipboard Changed, Custom Webhook (local
      `POST /webhooks/<name>`), Remote Instance; MIDI Out, Stream Deck pushes and Send Remote Trigger now
      do what they say; shops pass Streamer.bot's argument names.
- [x] 11b. Windows under Wine, all of it: 376 unit tests built for Windows, 146 scenarios, the UI smoke
      and the OS paths (`dev/windows.sh tests`, `scenarios`, `smoke`, `live`), and live Twitch, Kick and
      YouTube chat read by the exe. Found and fixed: Run a Program kept the `\r` of Windows line endings
      in its output variables; wineserver crashed on SendInput of a registered hotkey; the Windows-only
      lint and dead code warnings; empty Windows error messages.
- [ ] 12. Live checks script and checklist.

Every sub-action the engine runs is in a scenario; the others are checked to be skipped. Every trigger is
in a scenario, a named unit test, or `tests/excluded.json` with why.
