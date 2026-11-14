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

### 4. UI smoke (`dev/ui-smoke.py`)

Every page, narrow and wide, right-click menus, the user card, with screenshots and a crash check:

- Linux: the `dev-ui` build (`cargo build --profile fast --features dev-ui`) on Xvfb (`DISPLAY=:99`);
- Windows: `Streamerz.exe` under Wine on the same display (`dev/ui-smoke.py EXE --wine`, or `dev/windows.sh smoke`).

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

### The window under Wine

Stock Wine cannot run the window: it stubs `IDWriteFontSet::GetMatchingFonts` (no font resolves) and
the Direct Manipulation classes (GPUI fails window creation), and the current Wine master (11.21) still does.
`dev/wine/build.sh` builds Wine 11.20 with `dev/wine/gpui-support.patch` (about five minutes, no root, clang
only, in `/tmp/streamerz-wine`). Run the smoke with it, a prefix of its own and Noto Sans and Liberation Sans
copied into the prefix's `drive_c/windows/Fonts`:

    WINESERVER=/tmp/streamerz-wine/full/server/wineserver env -u WAYLAND_DISPLAY DISPLAY=:99 \
      python3 -P dev/ui-smoke.py <Streamerz.exe> --wine --wine-bin /tmp/streamerz-wine/full/wine \
      --wine-prefix target/ui-wine-patched

The smoke sets `GPUI_DISABLE_DIRECT_COMPOSITION` for Wine (upstream stubs the composition swap chain) and
resizes through the app's `resize` dev command, since a bare Xvfb never tells Wine about X resizes. Compare its
captures with the Linux run's: same sizes, text differs only by font face (Wine has no Segoe UI).

`dev/windows.sh` cross-builds `Streamerz.exe` as the Windows CI job does (cargo-xwin, MSVC, static
runtime, release), then runs it under Wine on the hidden display:

- `dev/windows.sh tests`: every unit test, built for Windows (Windows-only ones included: DPAPI, the Run key,
  Toolhelp, error texts), in a prefix of its own (`/tmp/szwinetest`);
- `dev/windows.sh scenarios`: every scenario on the exe;
- `dev/windows.sh smoke`: `dev/ui-smoke.py --wine`, every page, ending with the session's end (`wineboot -e`,
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

Windows cross-target type-check passed. Native Windows runtime checks remain
pending. OBS and the reference application are installed, but their local services were not listening
during this run; these results do not establish connected live-service behavior.

The final hidden, disconnected optimized development sample used 40,916 KiB RSS before opening
editors and 62,824 KiB after the screenshot/editor checks, with zero CPU ticks over five idle seconds.
Private dirty memory was 7,404 KiB before and 8,268 KiB after. These compare states within one build;
earlier builds were sampled in different states and do not provide a comparable version baseline.
The required hidden Twitch + OBS before/after measurement and the 25 MB budget remain unverified;
these disconnected RSS samples are already above that target. Final screenshots and metrics are in
`/tmp/streamerz-automation-ui-5wwp7qvi`; the general smoke is in `/tmp/szsmoke`.

### Version 1.6.0 cleanup review (2026-11-08)

The version stays open on `release/1.6.0`. No push, pull request, tag or release was created.
Removal evidence, review scope and release blockers are in [REVIEW_1.6.0.md](REVIEW_1.6.0.md).

| Check | Result |
|---|---|
| `cargo fmt --check` and `cargo clippy --all-targets --locked -- -D warnings` | Passed, including the commit hooks for each change. |
| `cargo test --profile ci-test --locked` | 533 passed, zero failed, 13 ignored. |
| Four ignored automatic UI tests, run separately on DISPLAY=:99 | Passed: wrapped help, grid redraw, rapid sidebar folding, JSON draft preservation. |
| `node --test dev/tiktok-extension.test.mjs` | 12 passed. Includes idle cleanup and the extension-origin restriction. |
| `node dev/test-subathon-overlay.cjs` | Passed. |
| `python3 -P -m unittest discover -s dev -p 'test_*.py'` | Six passed. Covers instance isolation, publishing-cache failure handling and hostile PR titles. |
| `python3 -P -m compileall -q dev` | Passed (syntax only for optional manual helpers). |
| `python3 dev/locale.py check` | Passed. 28 Portuguese entries still match English, a non-failing translation notice. |
| `actionlint` 1.7.12 | Passed. Shellcheck was not installed, so its optional embedded-shell analysis was not run. |
| Staged secret scanner | Passed for every commit. |
| `python3 dev/scan_secrets.py --history` | Failed: nine existing personal-path findings in historical memory results. No suppression or history rewrite. |
| `cargo audit` 0.22.2 | Exit 0, with five allowed advisory warnings listed in the review. |
| Linux release build and `--scenario tests/scenarios` | Passed, all 149 scenarios. Platform exclusions are printed by the runner. |
| `dev/appimage.sh dev` | Failed on this Arch host's library format and linuxdeploy's old strip. |
| `NO_STRIP=1 dev/appimage.sh dev` | Passed, created the AppImage and zsync file. |
| AppImage with `APPIMAGE_EXTRACT_AND_RUN=1`, scenarios | Passed, all 149 scenarios. |
| `cargo xwin check --all-targets --locked --target x86_64-pc-windows-msvc --target-dir target/xwin` | Passed. |
| Windows MSVC release build and executable icon check | Passed. Cross-linking reports missing optional MSVC debug symbols, not a build failure. |
| Windows executable, scenarios under Wine | Passed, all 149 scenarios. |
| Windows unit tests under Wine | 519 passed, zero failed, 11 ignored, with four test threads. |
| `python3 -P dev/smoke.py target/fast/Streamerz` | Passed, every page, menus and card. Screenshots in `/tmp/szsmoke`. Visual inspection found a narrow three-button dialog defect that the smoke's text bounds check misses. |
| `python3 -P dev/mcp_smoke.py target/fast/Streamerz` | All 54 advertised tools passed. |
| `python3 -P dev/automation-smoke.py target/fast/Streamerz` | Passed. Narrow and wide editor screenshots in `/tmp/streamerz-automation-ui-grse2dbq`. |
| Settings restart-state and narrow/wide screenshots | Passed through `dev/memory.py --visit-settings`, in `/tmp/streamerz-settings-final/off`. |
| Native Windows UAC, Linux polkit/sudo installation, signing and store publication | Not executed. Requires native/manual or external-service verification. |
| GitHub permissions, artifact reuse and publication | Not executed. No Actions usage was triggered. |

The first Windows unit run aborted on a misaligned file notification. A checked byte parser and packed
record fixture fix that crash without adding allocation to event parsing. The next run completed with
517 passing tests and one failing log assertion: another test had cleared the shared history. The test
helper now uses the application's existing thread-local capture, with a regression that clears shared
history while preserving the test's own log. Both suites were rerun after these fixes.

The automatic Linux suite keeps 13 tests ignored by default. Four automatic UI tests were run explicitly;
the remaining ignored cases are manual dialogs/notifications, Spotify/live-chat checks, a benchmark,
and type/coverage report generators. They were not silently counted as passed.

A disconnected optimized development sample used 39,964 KiB RSS after 50 synthetic messages and zero
CPU seconds over 15 idle seconds. Settings were also checked at 700 and 1200 pixels. An older release
binary did not complete the same development-isolation harness, so there is no comparable version
baseline. The required hidden Twitch + OBS measurements and 25 MB budget remain unverified.

The aggregate `dev/check.py --windows` run found the historical scanner failure and Python import-path
pollution in the new publishing test. The test now restores `sys.path`; the six Python tests pass on
rerun. Final Rust and Windows results in the table supersede earlier diagnostic runs. The known history-scan
failure means this version is not yet CI-clean. Live browser accounts, authenticated store uploads,
notification interaction and connected service behavior have not been established by the fixtures.

Final release candidates were also built with `STREAMERZ_VERSION=v1.6.0` on Linux and for MSVC.
Both binaries and the packaged AppImage passed all 149 scenarios. The Windows PE resources contain
Streamerz as ProductName and FileDescription, with FileVersion and ProductVersion both `1.6.0.0`;
the four-size executable icon check passed. AppImage packaging still required `NO_STRIP=1` locally.
Commitlint passed over every local version commit. The working tree was clean after recording results.

## Remaining bundled overlay examples (2026-11-13)

Subathon now comes from `assets/examples/subathon`, including its editable countdown actions; the Rust
builder and old page are removed. Emote wall, Top gifters, Credits roll and Song request queue complete
the 19 bundled examples. The separate Song requests (Spotify) action builder migration remains tracked
in `PLAN.md` as 9d. No commit, push, release or GitHub workflow was run for this work.

| Command or check | Result |
|---|---|
| `cargo test --locked` | 594 passed, zero failed, 8 ignored. Includes the scenario suite and queue topic coverage with the fake Spotify service. |
| `cargo test --locked examples::` | Final focused rerun: 5 passed. Includes package/share checks for all 19 examples, Subathon import/export, and gift totals with the page closed, duplicate bomb notifications, separate viewer IDs and saved periods. |
| `node dev/test-subathon-overlay.cjs` | Passed. Deadline, pause, expiry, clock ID changes and Test isolation; a paused or finished clock schedules no timer. |
| `node dev/test-examples.mjs` | 285 checks passed. Installs every example, exercises live settings and Test, checks JavaScript errors, captures normal/narrow sources, and verifies that preview data does not change saved totals. |
| `node dev/test-overlays.mjs` | 43 checks passed, including permissions, chat history, topic replay, page calls and file ranges. |
| `cargo fmt --check` and `git diff --check` | Passed. |
| `cargo clippy --all-targets -- -D warnings` | Passed. |
| `cargo clippy --all-targets --features dev-ui -- -D warnings` | Passed. |
| `cargo build --profile fast --features dev-ui --locked` | Passed. |
| `cargo xwin check --features dev-ui --target x86_64-pc-windows-msvc --target-dir target/xwin` | Passed. Native Windows runtime was not exercised. |
| `python3 -P dev/locale.py check` | Passed. 33 of 1,368 Portuguese texts still match English. |
| Secret scanner rules against `git diff HEAD` plus untracked text files | Zero findings, without staging the existing working changes. The normal scanner only examines staged changes, so its empty result alone was not used. |
| `env -u WAYLAND_DISPLAY DISPLAY=:99 STREAMERZ_SMOKE_PORT=8098 python3 -P dev/ui-smoke.py` | Passed. Shared forms, pages, cancellation, menus, renderer exit and backend access. |
| Same UI smoke with a private executable copy and `--out target/ui-smoke-examples-private` | Passed. Hidden backend: 8,632 KiB RSS, 0.00% of one CPU core over five seconds, after renderer exit, without live connections. |
| Native Examples dialog, using `StreamerzUi` bounds and window captures on `DISPLAY=:99` | Inspected at 1,100 x 800 and 540 x 700. New rows appear and fit the shared list. |

Browser tests used an isolated development instance on WebSocket 8095 and HTTP 7495, with
`STREAMERZ_DEV=1`, a fake Twitch broadcaster with an empty token and `manualConnect: true`, and
`window.chat.saveHistory: true` for the history regression. No real account token was supplied. The
local Spotify app supplied the Now playing check; authenticated live song requests and real gift
notifications were not exercised. Request, removal, playback and clear behavior for the queue topic
were exercised with the existing fake music service. The queue refresh is driven by the existing local
media watcher, only with Spotify enabled, a queue listener and pending requests; it adds no task or
polling timer.

The first full Rust run reproduced the previously reported category-test failure: another test had
remembered Just Chatting in the shared catalog. The catalog fixture now starts empty and its writers
use the shared test lock. A later full run passed. The request/export test now installs Subathon
through the asynchronous overlay request instead of the removed action-template builder.

The example browser harness now closes its Chromium targets, so finished pages release their feeds
and do not keep running while later examples are checked. The gifters fixture uses its own period,
since Subathon's earlier gift check also runs the leaderboard actions. A topic-change check reopens
the queue page before sending Test, so the initial live replay has settled.

Screenshots are in `/tmp/szoverlay/example-*.png`; the gallery captures are
`/tmp/szoverlay/example-gallery-1100.png` and `example-gallery-540.png`. Smoke output and memory maps
are in `target/ui-smoke-examples-private`. The first smoke used the same executable as another running
development instance and measured 31,904 KiB, including 24,720 KiB mapped from that executable. The
private copy avoids that shared-file effect; it reuses the same build, without rebuilding it. These are
synthetic disconnected measurements, not the required connected Twitch + OBS version baseline.

Version closure remains open: connected service profiling, native Windows behavior, release packaging,
signing/store checks and GitHub artifact reuse/publication were not performed in this task. The full
`python3 -P dev/check.py` release/push gate was not run; no push was requested.

## OBS output discovery and control (2026-11-13)

Scope: backend and MCP only. Accounts and the Stream page were not changed for this task.
Tests used OBS Studio 32.2.2 with obs-websocket 5.7.4 on `DISPLAY=:99`, temporary settings and
plugin directories under `/tmp/streamerz-outputs-research`. The existing desktop OBS instance was
not configured, restarted or used to publish. Test streams went only to loopback FFmpeg receivers.

Commands and results:

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test --profile ci-test core::outputs --locked`: six passed, one integration test ignored.
  Covers source-specific request parameters, filtering recordings, duplicate output names, connection
  identity, unknown versus stopped state, platform hints, omission of service secrets, partial errors,
  selected main start confirmation, failed-main handling, stop ordering and subscription cleanup.
- `cargo test --profile ci-test --locked`: 600 passed, nine integration tests ignored.
- `python3 -P dev/locale.py check`: passed (33 existing Portuguese texts still match English).
- `STREAMERZ_TEST_OBS_PORT=14457 STREAMERZ_TEST_OBS_LOCAL_STREAM=1 cargo test --profile ci-test
  real_obs_discovery_through_shared_dispatcher --locked -- --ignored --nocapture`: passed. Stock OBS
  discovery, authentication denial, selected main start confirmation, actual active state and stop request
  through the shared dispatcher. OBS logged a successful connection to the loopback RTMP receiver and
  output frames. The opt-in local-stream test requires an isolated profile configured for a local receiver.
- `STREAMERZ_TEST_OBS_PORT=14456 STREAMERZ_TEST_OBS_VENDOR=aitum-multistream cargo test --profile
  ci-test real_obs_discovery_through_shared_dispatcher --locked -- --ignored --nocapture`: passed.
  Real Multistream 1.0.9 returned the configured destination and Vertical 1.6.7 returned its status.
  Both Main and Aitum Vertical canvases were found.
- `STREAMERZ_TEST_OBS_PORT=14455 STREAMERZ_TEST_OBS_VENDOR=aitum-stream-suite cargo test --profile
  ci-test real_obs_discovery_through_shared_dispatcher --locked -- --ignored --nocapture`: passed for
  the Stream Suite 1.2.5 API and main stream discovery. The isolated plugin had an empty configured-output
  list despite the seeded profile. Its destination start/stop was not verified. An earlier attempt to
  start the main stream in that instance timed out waiting for confirmation; it was not counted as live.
- Temporary protocol exercise against Multistream 1.0.9: passed. Started the main stream, received
  frames in FFmpeg, requested the selected plugin output, received frames in a second FFmpeg receiver,
  read both runtime outputs as active and requested plugin stop, then main stop. This checks the real
  vendor API; source-specific backend routing is covered by the unit tests above. The receiver needed
  time for the next keyframe when joining a shared encoder. Earlier receiver setup errors and a short
  receiver deadline were corrected before the passing run.

Remaining verification: Stream Suite configured-destination start/stop, Vertical destination start/stop,
the Multi-RTMP websocket fork on a real installation (fixture coverage only), Windows runtime and
cross-check, release packaging, signing, stores, GitHub-only artifact reuse, and the version's hidden
Twitch + OBS RSS/idle CPU comparison. No version was closed or pushed. There is no new idle task,
connection or periodic refresh in the output adapter. No UI change needed a screenshot for this task.

Shared check run: `python3 -P dev/check.py` completed with four failures. Rust tests, CI-profile
Clippy, formatting, locales, Subathon browser test, six Python tests and extension packaging passed.
The TikTok extension tests had three failures in existing browser mocks (`chrome.storage.local` is
missing, with asynchronous `URLSearchParams` errors). The history secret scan reported nine historical
findings, unrelated to the output adapter; they were not copied into these notes. `cargo audit` and
`actionlint` were unavailable on PATH, so neither check passed. The staged secret scan passed but did
not cover unstaged work; the scanner was also applied directly to the output-change files, including
the untracked module, with no findings. These unrelated failures and unavailable tools remain open;
no push or release was attempted.

### Persistent OBS plugin installation (2026-11-13)

Installed upstream release binaries in OBS's supported per-user plugin directory,
`~/.config/obs-studio/plugins`, with their data files and locales. Each downloaded package matched its
GitHub release asset's SHA-256 digest and had no missing runtime libraries. No OBS upgrade or privileged
system file changes were needed. Versions: Aitum Multistream 1.0.9, Aitum Vertical 1.6.7, Aitum Stream
Suite 1.2.5 and the Multi-RTMP websocket fork 0.7.4.3-ws0.3. A backup of the previous Plugin Manager
state and an installation manifest live in `~/.local/share/Streamerz/obs-plugin-install-backups`.

The normal OBS Plugin Manager enables Multistream, Vertical and Multi-RTMP; Stream Suite is installed
but disabled because it replaces the older Aitum plugins. The normal desktop OBS log confirms all three
enabled plugins loaded and the Multi-RTMP vendor registered. Separate test configurations under
`~/.local/share/Streamerz/obs-plugin-tests` share the installed binaries but use private scenes, settings
and plugin enablement: the legacy configuration disables Suite; the Suite configuration disables the
older Aitum plugins. Neither configuration copies production credentials.

Both installed-binary discovery tests passed through `StreamerzOutputs` (ports 14456 and 14455, with
`STREAMERZ_TEST_OBS_VENDOR=sorayuki.multi_rtmp` and `aitum-stream-suite`, respectively). The installed
Multi-RTMP fork also passed a local receiver exercise: create a temporary target, configure a loopback
server, start the main stream, receive frames, start that target by ID, receive frames in a second
receiver, read its running state, request stop and delete the temporary target. No public stream ran.

This live test found that released Multi-RTMP 0.7.4.3-ws0.3 uses `isRunning` in its target list/state,
although its README describes `streaming`. The backend now accepts both fields, with a regression
fixture for the actual release format. The first protocol exercise received frames but failed its old
`streaming` assertion; the corrected exercise passed, including stop and target cleanup. Formatting and
`cargo clippy --all-targets -- -D warnings` passed after the adapter fix. `cargo test --profile ci-test core::outputs --locked` also passed after the fix
(six passed, one real-OBS integration test ignored by default). Stream Suite configured destination streaming and Vertical destination streaming
remain unverified; successful module/API loading alone does not count as stream verification.

### Complete output verification (2026-11-13)

Repeatable live test: `env -u WAYLAND_DISPLAY DISPLAY=:99 python3 -P dev/test-obs-outputs.py`.
It uses the installed binaries, generated scenes with a color source, isolated OBS profiles and fixed
loopback RTMP destinations. It never copies production scenes, logins or stream services. `FirstRun`
marks the setup wizard as already completed, and the harness waits for OBS's main window rather than
assuming an open websocket means the plugins finished loading. `src/core/outputs/live_tests.rs` drives
`StreamerzOutputs` through the real backend, not a parallel implementation of its controls. FFmpeg
must receive video frames. Repeated start must be idempotent, selected stop must close that receiver
and leave the main stream live, repeated stop must be idempotent, and final stop must report inactive.
A stale output ID is rejected before anything starts. Temporary instances and receivers are removed.

Initial results: nine live scenarios passed and the browser-enabled Suite test failed. The failure
was fixed and the full matrix repeated below. The final Suite tests all keep obs-browser enabled:

| Scenario | Result |
| --- | --- |
| Stock OBS main stream | Passed |
| Stream Suite horizontal destination | Passed with obs-browser enabled and rendered local-page pixels verified |
| Stream Suite vertical destination on its extra canvas | Passed with obs-browser enabled and rendered local-page pixels verified |
| Aitum Vertical canvas stream group | Passed |
| Multistream horizontal destination sharing the main encoder | Passed |
| Multistream vertical destination | Passed |
| Multi-RTMP websocket fork target by ID | Passed |
| Main stream reconnection after replacing its local receiver | Passed; no second start request |
| Shared encoder requested without selecting the main stream | Passed; acknowledgement stayed unconfirmed and the main stayed stopped |
| Stream Suite with obs-browser enabled on X11 | Passed after the clean-profile startup fix below |

The original browser-enabled Suite failure happened before configured outputs loaded. A plain OBS
browser source also failed with the same fresh profile. A diagnostic browser build showed
`CefInitialize` never returning on the browser thread; running initialization on the main thread
exposed Chromium exit code 28. Passing `--no-first-run` to the installed OBS fixes the fresh-cache
startup flow. This matches the [CEF embedder fix](https://github.com/tauri-apps/tauri/pull/15980)
for Chromium's Linux first-run EULA flow. Rebuilding against the installed CEF alone did not fix it.
The final test uses the original installed browser module and helper, with no replacement, preload,
Qt event-loop override or D-Bus override. No system package was changed.

The harness waits for Suite's configured output to appear, including retrying OBS's temporary
not-ready response, then creates a local browser source and checks screenshot pixels against its
solid green page. This is required for every Suite scenario, including vertical streaming. Browser
hardware acceleration is disabled only in the generated Xvfb profile; `--disable-gpu` and
`--password-store=basic` keep that disposable profile on software rendering and away from the desktop
credential store. The test still fails if the page does not render or local receivers get no video.
The normal OBS keeps Suite disabled because it conflicts with the older Aitum plugins. Its process,
plugins and configuration were not changed during this fix.

Verification found and fixed:

- Multistream's stopped runtime objects disappear from OBS's list. Its existing vendor start/stop events
  now preserve confirmed state, scoped to the connection and cleared on disconnect or profile/scene
  collection changes. No watcher, timer or poll was added. Vertical Multistream runtime names are also
  matched, so the configured destination gets its actual state without a duplicate runtime row.
- A start during native reconnection now waits for the existing `RECONNECTED` event. A stop can cancel
  reconnection even if OBS temporarily reports `outputActive: false`.
- Native OBS and Suite reports of the same canvas are merged by UUID. Multi-RTMP's shared internal
  `multi-output` runtime objects are not exposed as duplicate destinations or paired with a target by
  name. An unrelated runtime label cannot override another plugin's state.
- Missing optional vendors still return their capability result but do not create error-level log noise.
- The TikTok extension test fixture now supplies the browser storage and URLSearchParams APIs already
  used by the extension. All twelve tests pass without asynchronous failures.
- Audit identified Tokio's I/O safety advisory RUSTSEC-2026-0386. Tokio was patched from 1.53.2 to
  1.53.3; the five Linux AsyncFd sites now use its explicit registration API with documented ownership
  guarantees. Existing tests and platform checks passed after the migration. Filesystem watcher tests
  exercise real Linux descriptors; physical input and MIDI devices were not tested.

Final commands and results:

- `python3 -P dev/check.py`, with the verified temporary cargo-audit 0.23.0 and actionlint 1.7.12 on PATH:
  Rust tests **605 passed, 10 ignored**, all scenarios included; CI-profile Clippy, formatting, locales,
  twelve TikTok extension tests, Subathon browser test, fourteen Python tests, workflow validation and
  extension packaging passed. Audit passed with eight unmaintained transitive-dependency warnings
  reporting no fixed upgrade; the Tokio advisory is gone. Ignored service/platform tests are listed by
  the runner and are not counted as passed.
- The shared runner now exits 0. `dev/scan_secrets.py --history` reports nine reviewed legacy home-path
  occurrences across three old commits of `docs/memory-results.json`. The current file was already
  redacted. `dev/secrets-history.json` records exact commit, file, rule and full-line SHA-256
  fingerprints with a reason, without storing the personal path. This preserves published history,
  so the old paths remain in those historical commits. It exempts no credentials, staged changes,
  changed lines, other files or new commits. Reintroducing a path in the current file invalidates
  the baseline. Git errors fail closed. Eight new scanner tests cover these boundaries.
- `cargo xwin check --locked --all-targets --features dev-ui --target x86_64-pc-windows-msvc
  --target-dir target/xwin`: passed. This is type-checking, not Windows runtime or plugin verification.
- The original nine-scenario matrix is in `target/obs-outputs-verification/run-2km30tg5/results.json`.
  The canvas/runtime deduplication checks were also exercised against Suite horizontal/vertical and the
  fork in `target/obs-outputs-verification/run-nl3qjwk2/results.json`, all passed. Each retained test run
  has its OBS and backend logs; the browser-enabled debugger trace is
  `target/obs-outputs-verification/suite-browser-backtrace.txt`.

Not performed: public Twitch/Kick/YouTube/TikTok publication, native Windows plugin/runtime tests,
macOS runtime, release/AppImage packaging, signing, store uploads, GitHub artifact reuse/publication,
and the full version's connected Twitch + OBS RSS/idle CPU comparison. Extension packages were built
locally only. No UI changed in this task. No version was closed, no PR was pushed and no release ran.

Failure-fix verification:

- `python3 -P -m unittest discover -s dev -p test_scan_secrets.py`: eight passed.
- `PATH=<verified temporary tools>:$PATH python3 -P dev/check.py`: exit 0; log in
  `target/obs-outputs-verification/failure-fix-checks.log`, 605 Rust tests and fourteen Python tests
  passed, ten Rust service/platform tests ignored, audit passed with eight maintenance warnings.
- `env -u WAYLAND_DISPLAY DISPLAY=:99 python3 -P dev/test-obs-outputs.py --case suite-browser`:
  passed with installed binaries, exact page pixel `(20, 190, 70)` and full backend stream lifecycle;
  results in `target/obs-outputs-verification/run-bq608kbi/results.json`.
- `env -u WAYLAND_DISPLAY DISPLAY=:99 python3 -P dev/test-obs-outputs.py`: all ten scenarios
  passed after the fix, including actual browser pixels in all three Suite runs. Results:
  `target/obs-outputs-verification/run-f2chvrz2/results.json`; combined log:
  `target/obs-outputs-verification/fixed-full-matrix.log`. All private OBS instances and FFmpeg
  receivers exited; the original desktop OBS remained running. No public stream was started.
- `python3 -P -m unittest discover -s dev -p 'test_*.py'`: fourteen passed after the final harness
  changes. `python3 -P -m py_compile dev/scan_secrets.py dev/test_scan_secrets.py
  dev/test-obs-outputs.py` and `git diff --check` passed.
