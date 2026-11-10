# GPUI parity checklist

Reference: installed AppImage, `X-AppImage-Version=1.6.0`. Captures are in
`target/gpui-reference/captures` (isolated settings, no user data). They were regenerated on
2026-11-09 with `dev/ui-review.py`.

The list below is the visible control inventory from the wide dark reference. Controls that appear
only after selection, scrolling, login or opening a dialog still require manual inventory. A capture
does not establish functional parity.

The GPUI smoke covers shared entity forms and process lifecycle. Commands and Timers passed create,
cancel, toggle, filtering and context-menu checks. Numeric and JSON validation, unsaved drafts, both
Files lists, deletion confirmation and cancellation, and a pending core input across renderer restart
also passed. Stream Apps has narrow and wide captures; live connection behavior remains pending.
The shared smoke passed in dark and light modes. Theme assertions caught and fixed an explicit-Light
setting being ignored by the new renderer. Add-form reference captures for all eight entity kinds are
in `target/gpui-reference/entity-dialogs`, in both themes and sizes.
Servers now uses the shared core forms for all four built-in servers, with separate WebSocket client
and custom-server lists. Dark/light smoke checks passed for invalid ports, cancellation, saving a
disabled HTTP server, adding a disabled client and continued MCP connectivity. Its production dialog
captures are in `target/gpui-reference/server-dialogs`. Full connection and Wine parity remain pending.
Windows under Wine: runs with a locally patched Wine 11.20 (`dev/wine/build.sh`; see "GPUI frontend under
Wine" in `docs/TESTING.md`). Stock Wine, including master 11.21, stubs `IDWriteFontSet::GetMatchingFonts` and
Direct Manipulation, which GPUI needs. The shared smoke passes there in dark mode and its captures have the
same sizes and layout as Linux's; text differs only by font face. It also passes in light mode. The per-page
function comparison and a real Windows run are still pending.
Every page is built. Remaining gaps are listed per page below. The pages are
built on the core's table models (`core/pages.rs`) and section models (`core/sections.rs`), and every
operation is also an MCP tool (`core_view`, `core_action`, `core_setting`, `core_status`, `core_save`).
Known gaps in the built pages: Settings' Chat settings opens the
Chat options dialog, and MCP's connection is edited in a form, not inline.
Known flake: under Wine the smoke's "pending prompt survives a renderer restart" step timed out (a 15 s
request with no answer) in 2 of 4 runs, and passed on the others; Linux has not shown it. Unexplained yet.
No page has had its side-by-side visual review against 1.6.0 signed off. No page has full visual and functional parity signed off, and the
Windows runtime comparison remains pending.

| Page | Reference captures | Functions and dialogs | GPUI Linux | GPUI Windows (Wine) |
|---|---|---|---|---|
| Accounts | Captured, wide/narrow, dark/light | Built (connect, Advanced forms, Twitch device login, Kick and YouTube browser logins with cancel, bot logins, logout with revoke, channel search with leftover-people choice, keyring unlock, TikTok sending dialog, connections Retry). Reviewed 2026-11-10 with fake accounts on all platforms: platform state and buttons now share the heading line as in 1.6.0, round profile pictures next to the account and bot names (core lookup, kept a day, drawn from the images cache; checked with a cached address, not against the real services), long status lines wrap in dialogs, the expired-login note shows. Differs: Channel is a value plus Find channel (1.6.0 shows the channel as the button); no profile picture for a typed channel; no automatic relogin popup; headings are not bold in the dev font | Unit tests pass | Pending |
| Actions | Captured, wide/narrow, dark/light | Built: action tree with groups, search and context menus; add, edit, duplicate, on/off, delete, test, import, export, templates; group add, rename, delete, icon, settings; the open action's triggers and sub-actions. Reviewed 2026-11-10 with three actions, a group, triggers and sub-actions: clicked through the trigger picker (search, details, OK), the trigger and sub-action forms, the delete confirmation and the narrow stacked layout (540x700, dark and light). New: Enter edits, Delete deletes, Space switches the selected row in all three trees (not while typing in a search box); a group's picture is drawn in the tree (core writes the icon file). Differences: no drag and drop (move buttons instead), no key and mouse capture fields, group icon not checked with a picked file in the UI (core test only) | Unit tests pass | Pending |
| Backups | Captured, wide/narrow, dark/light | Built (Backup now, Restore, Open folder, Delete) | Smoke and unit tests pass | Smoke passes, same layout |
| Chat | Captured, wide/narrow, dark/light | Built (2026-11-10): the Show box (all together, side by side, one chat; saved as 1.6.0 saved it), a column per chat with its header (logo, channel picture, "Platform: Channel"), its own send box with the account picture and emotes, a bar with reconnect and hide when a chat is down, Message all and Reply in a dialog, platform badge art for Kick, YouTube and TikTok (Twitch badges are pictures from Twitch), @mention chips, fade in of new lines, the user card (banner, picture, account age, groups menu, facts, what they said as chat lines, Details table, moderation) opened at once from cache with the platform lookup streaming in, and a lookup started when the pointer rests on a name. Differs: no fade out; Twitch badges are generic icons until Twitch's badge data loads; checked with injected messages, not live services | Unit tests pass | Pending |
| Commands | Captured, wide/narrow, dark/light | Built earlier | Reviewed side by side 2026-11-10, see the page-by-page section; smoke and unit tests pass | Smoke passes, same layout |
| Community | Captured, wide/narrow, dark/light | Built: Show selector, users with core-side sort and platform filter, quotes add/edit/delete, credits, user card. The card now asks the platform too (picture, banner, followers and so on, with a looking-up line) and offers timeout and ban when the role allows (op `card`/`act` accept platform and userId; MCP `chat`). Reviewed with eight users, dark/light, wide/narrow. Not checked against live services: lookup answers and the moderation buttons (no role in the fixture) | Unit tests pass | Pending | The card is the chat card (one drawing for both).
| Crashes | Captured, wide/narrow, dark/light | Built (report, folder, delete, detail text) | Smoke and unit tests pass | Smoke passes, same layout |
| Donations | Captured, wide/narrow, dark/light | Built (three services, token and site forms) | Smoke and unit tests pass | Smoke passes, same layout |
| Files | Captured, wide/narrow, dark/light | Built earlier | Reviewed side by side 2026-11-10, see the page-by-page section; smoke and unit tests pass | Smoke passes, same layout |
| Globals | Captured, wide/narrow, dark/light | Built (add, edit, delete, user globals read-only) | Smoke and unit tests pass | Smoke passes, same layout |
| Groups | Captured, wide/narrow, dark/light | Built (add, members dialog with remove, delete) | Reviewed side by side 2026-11-10, see the page-by-page section; smoke and unit tests pass | Smoke passes, same layout |
| History | Captured, wide/narrow, dark/light | Built (variables, run again, clear, search) | Smoke and unit tests pass | Smoke passes, same layout |
| Integrations | Captured, wide/narrow, dark/light | Built (Spotify setup dialog, browser login with cancel, local monitor) | Smoke and unit tests pass | Smoke passes, same layout |
| Log | Captured, wide/narrow, dark/light | Built (level, search, folder) | Smoke and unit tests pass | Smoke passes, same layout |
| Mcp | Captured, wide/narrow, dark/light | Built (add, remove, chat, files, copy config; connection form) | Smoke and unit tests pass | Smoke passes, same layout |
| Plugins | Captured, wide/narrow, dark/light | Built (on/off, settings form, folder) | Reviewed side by side 2026-11-10, see the page-by-page section; smoke and unit tests pass | Smoke passes, same layout |
| Polls | Captured, wide/narrow, dark/light | Built (tables, status line, forms with platform choice, End, Lock, Resolve, Cancel, Refresh). Reviewed 2026-11-10: disabled buttons now say why on hover (checked), platform boxes in the New poll form carry their logo and a disabled one explains itself on hover (core model and test; the form could not be opened in the fixture, so its drawing is unchecked). Not checked against live Twitch or YouTube | Unit tests pass | Pending |
| Queues | Captured, wide/narrow, dark/light | Built (add, edit, delete, blocking, pause, clear, pending) | Reviewed side by side 2026-11-10, see the page-by-page section; smoke and unit tests pass | Smoke passes, same layout |
| Servers | Captured, wide/narrow, dark/light | Built earlier | Reviewed side by side 2026-11-10, see the page-by-page section; smoke and unit tests pass | Smoke passes, same layout |
| Settings | Captured, wide/narrow, dark/light | Built (every setting; Chat settings opens the Chat options dialog) | Smoke and unit tests pass | Smoke passes, same layout |
| Stream Apps | Captured, wide/narrow, dark/light | Built earlier | Smoke and unit tests pass | Smoke passes, same layout |
| Timers | Captured, wide/narrow, dark/light | Built earlier | Reviewed side by side 2026-11-10, see the page-by-page section; smoke and unit tests pass | Smoke passes, same layout |

## Page-by-page review (Commands, Timers, Files, Queues, Groups, Plugins, Servers)

Reviewed on 2026-11-10 against 1.6.0 run live with the same populated settings (commands in several
groups and sources, timers, watchers and tails, queues, user groups with members, three `.wasm`
plugins including a broken one, a WebSocket client and server list), dark and light, 1100x800 and
540x700. Every button, right-click menu, search, switch, double click, dialog and validation error
of the seven pages was exercised in GPUI. Not signed off as full parity: see "Still differs".

Fixed (shared blocks, so every page gets them):
- Tables shrink their columns to the width they have (the last one fills a wide table), and a cell
  that does not fit ends in an ellipsis; no more horizontal scroll or text under the next column
  (`controls/rows.rs`, `fit_widths`, tested).
- Button rows fold to icons with a tooltip when the labels no longer fit, all together
  (`controls/list.rs`).
- A right click on a row also selects it for the buttons below.
- The details box (Plugins, Crashes) is not drawn when there is nothing to show; Plugins uses the
  production 100 px box and four buttons in one row.
- Section headings carry their icon: Files (folder, scroll), Servers (Streamer.bot logo, plug, server).
- Empty group name showed the prompt label "Name:" as the error; it says "Enter a name." now (test).

Still differs:
- Dialogs are drawn at the top of the window, not centred, with no dim backdrop and a lighter title;
  the production ones are centred. The command Group field is a plain text box (production: an
  editable list of the existing groups). Edit JSON has no icon, OK is the primary (white) button.
- Files: the filter shows as plain text (production: a pill); a disabled row is dimmed less.
- Queues and Servers: the Blocking cell has a lock icon (production: plain text).
- Dev builds turn every auto-start off, so row switches for "On start" show Off in dev captures.
- The version label in the sidebar shows the build hash (dirty tree), not `v1.6.0`.

## Visible reference controls

### Accounts

Accounts,   Twitch, Off, log in or pick a channel, Connect, Advanced…, Account, Not logged in, Log in, Channel, Your own channel, Bot (optional),   YouTube,   Kick,   TikTok, Browser account, Not set up: Streamerz only reads TikTok chat, Set up, Account is who you log in as. It doesn't have to own the channel: to moderate someone else's chat, type their channel. On Twitch, what needs the owner's login (channel points, bits, polls) is left out.\nChanging the channel starts a clean chat and community for it; the old ones come back if you type it again., Navigation, Save, Nothing set up, Hide, Quit

### Actions

Actions, Add, Edit, Duplicate, Delete, Test, Import, Export, Templates, Add group, Pick an action to edit it, Triggers, On/Off, Help, Sub-actions, Navigation, Save, Nothing set up, Hide, Quit

### Backups

Backups, Each backup has the settings file and the Community data. A new version makes one when it first starts (the last 10 are kept); the ones made by hand stay until you delete them., Back up now, Restore, Open folder, Delete, Navigation, Save, Nothing set up, Hide, Quit

### Chat

Chat, Show:, Click a name for its info, hover a line for actions,  Message all, Options, Clear, No chat connected. Log in or set a channel in Accounts., Accounts, Navigation, Save, Nothing set up, Hide, Quit

### Commands

Commands, Command, Add, Edit, On/Off, Delete, Navigation, Save, Nothing set up, Hide, Quit

### Community

Community, Show:, Navigation, Save, Nothing set up, Hide, Quit

### Crashes

Crashes, Report on GitHub, Open folder, Delete, Refresh, Navigation, Save, Nothing set up, Hide, Quit

### Donations

Donations,   StreamElements, Dashboard > your account > Channels > Show secrets, copy the JWT token., off (no token), Edit,   Streamlabs, Dashboard > Settings > API Settings > API Tokens, copy Your Socket API Token.,   DonorDrive, Extra Life and other DonorDrive charities: the site (www.extra-life.org) and the number ending your page's address., Status: off, Navigation, Save, Nothing set up, Hide, Quit

### Files

Files,   File and folder watchers, Runs the File Watcher triggers when files in a folder are created, changed, renamed or deleted., File watcher, Add, Edit, On/Off, Delete,   File tails, Runs File Tail Changed for each line added to a file, such as a game's log., File tail, Navigation, Save, Nothing set up, Hide, Quit

### Globals

Globals, Globals are values your actions keep between runs. Persisted ones survive a restart. Per-user globals can only be read here., Add, Edit, Delete, Refresh, Navigation, Save, Nothing set up, Hide, Quit

### Groups

Groups, Groups are lists of users. Actions use them to check who may run a command or to pick users., Add, Members, Delete, Navigation, Save, Nothing set up, Hide, Quit

### History

History, Variables when queued, Variables after run, Run again, Clear, Refresh, Navigation, Save, Nothing set up, Hide, Quit

### Integrations

Integrations, Services your actions can control. Each one stays off, with nothing running, until you set it up., Spotify, Off, Configure, Song requests from chat and playback control from actions (Spotify Premium). Configure connects your own Spotify app., Navigation, Save, Nothing set up, Hide, Quit

### Log

Log, Show:, Open folder, Navigation, Save, Nothing set up, Hide, Quit

### Mcp

MCP, Let an AI agent build and test your actions. Add Streamerz to an agent here, then chat with it: what it changes applies at once., Not added: Streamerz isn't in this agent's MCP servers yet., MCP connection, Address, Port, Route, Agents run /tmp/streamerz-gpui-reference/squashfs-root/usr/bin/Streamerz --mcp, which connects to ws://127.0.0.1:8097/, Add Streamerz, Remove, Open chat, Open file, Open folder, Copy config, Refresh, Navigation, Save, Nothing set up, Hide, Quit

### Plugins

Plugins, On/Off, Settings, Open folder, Refresh, Navigation, Save, Nothing set up, Hide, Quit

### Polls

Polls, Polls on Twitch and YouTube, predictions on Twitch. Kick and TikTok have no way to start them from an app. On Twitch they need the channel owner's login., Log in through Accounts to manage polls and predictions.,   Poll, New poll, End poll, Refresh,   Prediction, New prediction, Lock, Resolve..., Cancel prediction, Navigation, Save, Nothing set up, Hide, Quit

### Queues

Queues, Add, Edit, Delete, Blocking, Pause/Resume, Clear, Pause all, Resume all,   Pending, Navigation, Save, Nothing set up, Hide, Quit

### Servers

Servers,   Streamer.bot's servers (changes apply at once), Each server starts with Streamerz when it's on. Edit one to turn it on or off, or to change its address., WebSocket server: on, ws://127.0.0.1:8097/, Edit, HTTP server: off, http://127.0.0.1:7474/, UDP server: off, port 4242, Stream Deck server: off, ws://127.0.0.1:8059/,   WebSocket clients, WebSocket client, Add, Delete,   WebSocket servers, WebSocket server, On/Off, Navigation, Save, Nothing set up, Hide, Quit

### Settings

Settings, About, Version v1.6.0, A light stream automation app, made in Rust., Open on GitHub, Appearance, Theme, Follow the system (dark or light, and its accent colour), or pick one., Language, The language of the app: the system's by default, English when the system's isn't here. Applies the next time Streamerz\nstarts., Animations, Dialogs, menus and panels slide and fade in and out. Off: they appear at once., Memory, Aggressive memory saver, Applies after restart. Designed for background use.\nNo chat history, Community tracking, credits, avatars or emote catalogs.\nFirst Words stays enabled using temporary disk storage.\nExisting Community files stay untouched. Commands and connections stay enabled., Chat, Chat settings, How many lines the chat keeps, for how long, and how they fade. They're on the Chat page too, under Options., Notifications, Choose which desktop notifications to show., Manage, Startup, Start hidden, Open in the tray; click its icon for this window., Start automatically, Run Streamerz when you log in., Dates and times, Dates, How lists, history and the chat show a day., Times, 12 or 24 hours, for chat lines and history., Updates, New versions, This is Streamerz v1.6.0., Check now, Log, Debug lines, Also log details for tracking down a problem., Files, Settings file, /tmp/streamerz-ui-review-jzqdl8_f/streamerz/streamerz.json, Open file, Copy path, Open folder, Streamer.bot tools, Instance name, What tools connected to the WebSocket server see this as., Navigation, Save, Nothing set up, Hide, Quit

### Stream Apps

Stream Apps,   OBS Studio, In OBS Studio, open Tools > WebSocket Server Settings and enable the server (port 4455). Add a connection below with the same port and password, then choose Connect all., OBS connection, Add, Edit, On/Off, Delete, No connection yet: add one below., Connect, Connect all, Disconnect all, Refresh,   Streamlabs Desktop, In Streamlabs Desktop, open Settings > API / Remote Control and copy your API token. Add a connection below on port 59650 with that token, then choose Connect all., Streamlabs Desktop connection, Navigation, Save, Nothing set up, Hide, Quit

### Timers

Timers, Timer, Add, Edit, On/Off, Delete, Navigation, Save, Nothing set up, Hide, Quit

## Review pass: Stream Apps, Integrations, Donations, Globals, History, Settings, MCP, Log, Crashes, Backups, shell (2026-11-10)

Method: GPUI fixture with two OBS and one Streamlabs connection, donation tokens, persisted globals, four
backups, two crash reports and four history runs (`dev/gpui-shot.py --config --seed --ask`), compared with
the 1.6.0 captures, dark and light, 1100x800 and 540x700, with each page's buttons and dialogs exercised.

Fixed:
- Stream Apps: each section title shows its brand logo (OBS, Streamlabs).
- Globals and Backups: the page title no longer repeats above the guide (1.6.0 shows only the guide).
- Integrations: the service's logo, name, state and Configure button sit on one line, as in 1.6.0.
- Donations: rows are as tight as 1.6.0. Real bug: the three Edit buttons shared one element id, so only the
  last one worked (the others did nothing). Rows without a key now get unique ids. Smoke step added.
- MCP: connection fields (address, port, route) are inline and save when left or on Enter (core setting
  domain `mcp`, validated, MCP reachable through `core_setting`); the first agent is selected at start; the
  selected agent's state is a health-coloured line instead of a monospace box. Unit test added.
- Shell: Save, Hide and Quit use the normal button size; sidebar text 14 px.

Still differs: bold text does not render with the variable system font here (Adwaita Sans), so titles look
regular; the MCP Status column shows the state as text, 1.6.0 shows an icon; crash report text appears only
after selecting a row (1.6.0 too); History has no extra detail panel beyond its Variables dialogs; plugin
settings forms and live Windows runs are not covered. Shell tray and start-hidden need a tray host (the
Xvfb display has none, so a missing tray opens the window).

### Memory, hidden window (release build, Twitch channel set, OBS connection configured, Xvfb)

| Process | Before | After |
|---|---|---|
| Core, hidden | 34.8 MB RSS (5.6 anon, 29.0 file), 0 CPU ticks in 30 s | 13.1 MB RSS (2.5 anon, 10.6 file), 0 CPU ticks in 30 s |
| Frontend, hidden | process exits (0) | unchanged (0) |
| Frontend, shown (software Vulkan here) | 232 MB | unchanged |

The core's RSS was 23 MB of its own 45 MB binary's pages, touched during startup. `release_code_pages`
(`src/main.rs`) asks the kernel to reclaim the read-only file-backed mappings with `MADV_PAGEOUT` when the
window closes and once after a hidden start; they come back from the page cache when used. The `fast` dev
build (492 MB with debug info) measured 39.9 MB core before this change.

## Legacy window removed (2026-11-10)

GPUI is the only window. The FLTK UI is gone: `src/ui.rs` and `src/ui/*` (about 29,000 lines), the `fltk`
dependency (Linux and Windows) and its CMake, Pango and Xft build steps in CI and `dev/windows*`, the
`--gpui` opt-in and the `gpui` cargo feature (`gpui-dev` stays for the dev instrumentation), the FLTK key
and button tables in `hotkeys.rs`, the legacy smoke drivers (`dev/smoke.py`, `dev/automation-smoke.py`,
`dev/emote-check.py`; their shared WebSocket client is `dev/harness.py` now), the unused `image-webp` and
`gif` dependencies, the two window icon rasters, and 290 locale texts only the old window used.
`--gpui` is still accepted: the core ignores unknown arguments.

Moved, not lost: the emoji table (`src/emoji.txt`) beside `emotes.rs`; the window's crash trail (the page the
frontend opened, noted by `core/view.rs`); the system colour scheme and accent on Windows (read once at start by
`tray::run`); chatter 7TV personal emotes (`emotes::want_personal`, asked from `core/chat.rs` once per
chatter); and the viewer-count polling while the Chat page is the open page (`host::set_viewers_visible`,
set from `core/view.rs`, with a unit test). The last two had no caller in the GPUI build before this change.

Removed because only the old window called them, so GPUI has no equivalent yet (the code is in git history,
`git show HEAD:src/ui/...`):

- The chat box's `/` command list (`slash::suggestions` and its tables). `/` commands themselves still run.
- Per-field help text and trigger argument names in the form dialogs (`docs::field_help`, `argument_names`).
- The TikTok profile lookup in the Accounts dialogs (`webcast::profile`).
- Linux input access from the app (the pkexec udev rule for Mouse Click and Key Press triggers:
  `devinput::grant`, `script`, `rule`). Reading the devices still works once the user adds the rule by hand.
- Overlay "local file" copies for OBS (`overlays::standalone_at`).
- Retained-button identity by label (`i18n::button_id`) and the button-words test.

Windows-only note: `cargo xwin clippy` on the frontend crate reports `chunks_exact_mut` there; Linux clippy
(the CI one) is clean.

## Pixel pass (2026-11-10)

Every page was measured against the 1.6.0 captures (dark and light, 1100x800 and 540x700) and brought to about
1 to 3 px. Shared causes fixed once: the toolkit's rem was the 13px text size (all rem sizes were 19% small), now
16px in `frontend/src/app.rs`; the sans font is bundled (`assets/fonts/StreamerzSans-*.ttf`, Adwaita Sans static
Regular and Bold, Latin subset, OFL) because GPUI cannot make a variable system font bold; dialogs are centred.
Known small differences left: disabled buttons are drawn at half opacity by the toolkit (1.6.0 keeps the fill),
dialog title weight and backdrop blur, blue rounded checkboxes, button label widths 3 to 5% wider, text is
subpixel smoothed, narrow and light captures of Log, Crashes, Backups and MCP were not compared one by one.
