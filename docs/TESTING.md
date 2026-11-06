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

## Version 1.5.0 local verification

Linux, 2026-11-04: formatting and clippy passed, with 420 unit and scenario tests passed and 10 ignored.
TikTok sending was confirmed in a real LIVE chat by the user. Accounts setup and removal, the sending
composer, and narrow and wide screenshots were checked on the hidden display.

Hidden optimized development builds, scratch settings, no Twitch or OBS connection, 30-second samples
at 100 ticks per second:

| Build | RSS across three runs (KiB) | Idle CPU ticks per run |
| --- | --- | --- |
| Main (`c48ffb4`) | 28,344 to 29,044 | 0 |
| Version, sending not set up | 28,788 to 29,300 | 0 |

These disconnected measurements do not verify the hidden Twitch + OBS budget of less than 25 MB.
Windows runtime verification remains pending. The local Windows check could not run because the
Windows Rust toolchain was unavailable.

Browser measurements must first verify that the extension is connected and its TikTok tab is open.
An initial idle run failed this check because Developer mode was off in the lab profile; its readings
are discarded. Measure the tab and extension worker separately from shared browser, network and GPU
processes. Linux PSS accounts proportionally for shared pages; browser Task Manager memory figures use
a different definition and should not be compared directly with PSS.

With sending confirmed connected, the TikTok renderer measured 105.60 MiB PSS and 0.57% of one CPU
core in the first 60-second sample, then 96.04 MiB PSS and 0.33% in a second sample starting four
minutes later. Streamerz held 30,056 KiB RSS and used zero CPU ticks in both samples. These figures
exclude shared browser overhead and the extension workers. The lab browser also loaded its default
extensions, so worker totals cannot be attributed solely to Streamerz. CPU percentages use the OS tick
rate and elapsed time. The first valid sample started about one minute after the extension connected.

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

### TikTok convenience follow-up

The separate TikTok login and DevTools browser driver were removed. The browser extension supplies the
account, transient reader cookies, signed channel search and sending. Its own offscreen frame replaces
the visible helper tab. `node --test dev/tiktok-extension.test.mjs` checks context sharing, read recovery,
cleanup, session-cookie filtering, login redirects, logged-out resource use and no automatic send retry.
The lint workflow runs these checks alongside the Rust suite. The WebSocket integration test covers
pairing, signed search routing, readiness, session invalidation and disconnection.

Live Linux checks: signed search returned real users through Streamerz, and the offscreen frame's chat
request posted `:)` with HTTP 200 and TikTok status code 0. There were zero visible TikTok tabs. Search
recovered after deliberately discarding the offscreen document. Closing all lab browser windows left
its extension running in the background. The lab session subsequently became unavailable; the new checks report a
required login and avoid loading a full page while logged out. The accepted send was not independently
confirmed through retained chat, which was off in that scratch configuration. Windows runtime checks
and authenticated background memory profiling remain pending.

Final disconnected optimized development sample: 29,564 KiB RSS, unchanged over 30 seconds, with zero
CPU ticks (100 Hz). This does not establish the authenticated browser cost or the Twitch + OBS budget.

Final verification: 420 Rust tests passed, 10 ignored; eight extension lifecycle tests passed. Formatting,
clippy, locale validation, documentation tests, narrow and wide Accounts screenshots, populated channel
search, setup cancellation and the full UI smoke passed. Screenshot and live-test artifacts remain
outside the repository.

### TikTok account and chat audit

Linux, 2026-11-05: 426 Rust tests passed, 10 ignored; eight extension lifecycle tests passed. Formatting,
clippy, locale validation and the full UI smoke passed. The smoke checks the single browser-account
popup, idle popup CPU, TikTok in Message all, and the Send Message form without bot or fallback controls.

Narrow and wide screenshots verified browser-account pictures, the channel picture, the account popup,
Message all, mention replies and native sticker previews. A development reply captured
`@actual.handle :)` with `bot: false`; this follow-up sent nothing to a real chat. The WebSocket test
covers a missing passport avatar filled through signed search and clearing the picture and role when
the session changes. Native stickers remain receive-only; Unicode emoji can be sent.

The open account popup used three CPU ticks over 10 idle seconds after its avatar loaded (100 Hz).
It refreshes only when account data or pictures change.

The final hidden, disconnected optimized development sample held at 28,724 KiB RSS for 30 seconds,
with zero CPU ticks (100 Hz). This does not verify authenticated browser usage or the Twitch + OBS
budget. Windows runtime checks and authenticated background memory profiling remain pending.

### TikTok capability audit

Linux, 2026-11-05: the optimized serial suite passed 500 tests (13 ignored); nine extension tests
passed. Formatting, clippy with warnings denied, locale validation and the full UI smoke passed.
The first parallel debug run saw a locale catalog mismatch while another checkout shared its build
artifacts and a Twitch timeout assertion; the optimized serial run passed both.

Wire fixtures verify packed and unpacked 64-bit deletion IDs, chat pins, pause/resume and numeric room
IDs. Handler tests verify deletion and pin synchronization, stale cancellation rejection, pause event
deduplication and stream state. A WebSocket fixture verifies the browser room endpoint and its signed
lookup routing. Request tests cover dev-only event injection and authenticated state access. The extension
fixture checks that only the allowed session/device cookies reach the native reader, without a page load.

An isolated native UI fixture on DISPLAY=:99 verified pins and pause notices at 1100 and 540 pixels,
deleting the pinned message, resuming, an oversized draft retained after validation, and an accepted
`:)` captured only in development output. No real chat message was posted during this audit.
Artifacts are in /tmp/ttlab/possibilities-*.png; the complete smoke is in /tmp/szsmoke.

The new incoming event types and the browser fallback have fixture coverage. Live moderator deletion,
pin duration units, a real pause/resume and public-challenge fallback remain unverified. Windows runtime
and the hidden Twitch + OBS resource budget remain pending. See [TIKTOK.md](TIKTOK.md) for capabilities
and unsupported operations.

The shared notice helper now rewraps on host resize using existing UI events. Five focused dialog
tests passed (one ignored), and the smoke checks an open question at 540 and 1100 pixels. The TikTok
validation notice was also inspected at both widths with its draft intact.

Hidden, disconnected optimized development samples (30 seconds, 100 Hz): before this audit,
37,852 KiB RSS; final build, 38,820 KiB RSS. Both stayed stable and used zero CPU ticks. The final
workspace also contains concurrent MCP changes, so this comparison does not isolate the TikTok code.
The existing integration budget is not established by these disconnected samples.

### Automation editors, reference and TikTok gifts

Linux, 2026-11-05: 520 Rust tests passed, 13 ignored. Formatting and clippy with warnings denied
passed. All 951 registered entries have an indexed built-in reference, with nonempty Fields and
Arguments sections and When for triggers. Additional documentation checks cover Spotify outputs
and the absence of the misleading external-documentation hint. Eleven extension tests passed.

Gift fixtures cover exact IDs, inclusive total coin bounds, combo completion, legacy update behavior,
sparse event metadata, changing rooms/accounts, failed and empty catalogs, stale replies and a signed
browser request carrying the current room. Connection tests cover cancelling a slow catalog request,
releasing pending browser calls and clearing the old room before a new channel lookup completes.
No authenticated live gift list or real gift delivery was available in this isolated run.

All 54 MCP tools passed the isolated stdio smoke. The general UI smoke passed, and
`dev/automation-smoke.py` checked gift editing, Help, matching preview, a no-settings trigger,
duration editing and inserting a real event value at 1280 by 820 and 760 by 620. Screenshots were
inspected on DISPLAY=:99. The forms keep unknown fields and unchanged imported representations;
execution tests distinguish reported failures, skipped operations and successful runs.

Windows cross-target type-check passed with `fltk/fltk-bundled`. Native Windows runtime checks remain
pending. OBS and the reference application are installed, but their local services were not listening
during this run; these results do not establish connected live-service behavior.

The final hidden, disconnected optimized development sample used 40,916 KiB RSS before opening
editors and 62,824 KiB after the screenshot/editor checks, with zero CPU ticks over five idle seconds.
Private dirty memory was 7,404 KiB before and 8,268 KiB after. These compare states within one build;
earlier builds were sampled in different states and do not provide a comparable version baseline.
The required hidden Twitch + OBS before/after measurement and the 25 MB budget remain unverified;
these disconnected RSS samples are already above that target. Final screenshots and metrics are in
`/tmp/streamerz-automation-ui-5wwp7qvi`; the general smoke is in `/tmp/szsmoke`.
