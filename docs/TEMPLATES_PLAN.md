# Shareable templates with overlays: plan

The decisions in section 12 are agreed. `PLAN.md` remains the spec: the phases of section 15 go there as a
checklist under the version the owner picks.

## 1. Goal

Anyone can build a template (actions, commands, timers, settings and web pages), test it, pack it into one
file and hand it to someone else, who imports it, fills in a form and adds it to OBS in a few clicks.
Streamerz ships the tools, not a catalogue: the bundled examples are made with the same tools and exist to
prove them.

Not in scope: a hosted gallery or store, a visual page designer, running template code inside Streamerz
(pages run in OBS or a browser, never in our process).

## 2. Existing tools

| Piece | State | Where |
| --- | --- | --- |
| Group: actions, commands, timers, typed settings, files, icon | Done | `src/groups.rs` |
| Export and import of a group, with files and settings | `.streamerz` packages and pasted exports | `src/package.rs`, `src/edit.rs`, `src/core/overlays.rs` |
| Overlay: an `.html` file of a group served at `/overlays/<group>/<file>` | Done | `src/overlays.rs` |
| `window.streamerz`: live settings, feeds and calls to actions, chat and stored values | Done, one WebSocket per page | `assets/overlays/client.js` |
| Topics `viewers`, `ad`, `songrequests.queue`, any name by the Overlay sub-action (99880) | Done | `src/engine/overlays.rs`, `src/engine/songs.rs` |
| Countdown (99803) and the Subathon example | Bundled files, page uses the countdown feed | `assets/examples/subathon` |
| Bundled overlay examples | 19 examples as editable files | `assets/examples/`, `src/examples.rs` |
| Other built-in action templates | Rust code; song request builder file migration remains open | `src/templates/` |
| File watcher from the OS, no polling | Done | `src/fswatch.rs` |
| Test events and fake services | Done | `src/scenario/fakes.rs`, MCP `fire_trigger`, `inject_chat` |

## 3. Original gaps addressed by this plan

This was the initial audit. Implementation progress and remaining checks are in `PLAN.md`.

1. **A page can't talk back.** No way to run an action, send a chat message or store a value from a page.
   Blocks the chat dock, the roulette and any dock with buttons.
2. **No ready feeds for the common data.** Chat lines with emotes and badges, alerts across platforms, the
   current song and countdowns each need hand-written code per platform or a raw WebSocket client.
3. **Settings are fixed at page load.** Changing a colour needs a refresh of the OBS source.
4. **Settings are too poor.** Text, integer, number, boolean, duration, choice. No colour, picture, sound,
   slider, long text, or list of rows (the roulette's results).
5. **Files live in `streamerz.json` as base64.** They sit in RAM and are rewritten at every save. A sound or
   a GIF breaks the 25 MB budget. File names are flat (no folders).
6. **Sharing is pasted text.** Fine for actions, wrong for megabytes of media. No version, no author page,
   no picture, no update of an installed template.
7. **Nothing tells the importer what a page can do**, and once pages can send chat that matters.
8. **OBS is set up by hand**: copy the URL, create the source, type the size.
9. **Overlays are hidden** inside Groups, the group's settings, Files.
10. **No developer loop**: edit in a dialog, no live reload, no way to see a page's errors (OBS has no
    console), no test buttons, no starting point.

## 4. Design in one page

- The unit stays the **group**. No new concept in the engine. What the user sees are two things:
  - an **overlay**: pages only (a chat overlay, a clock). Made, shared and imported by itself.
  - a **template**: actions, commands and timers, with or without overlays (a subathon, a roulette).
  Both are a group underneath and the same file; one with no actions is shown as an overlay. A page for an
  OBS dock is an overlay like any other, marked "for a dock" only so the app shows the right steps.
- A group gains a small **manifest**: id, version, author, link, picture, its pages (kind, size), what the
  pages may do, which integrations it needs.
- Pages get a **two-way API** on the port already open: SSE down (exists), `POST /overlays/call` up. No new
  port, socket, thread or timer. A page that asks for nothing still connects to nothing.
- **Ready feeds** as topics: `chat`, `alerts`, `song`, `countdown:<id>`, `settings`. Each costs nothing
  until a page listens, and none opens a connection by itself.
- Files move **to disk**, with folders, served straight from there.
- One **`.streamerz` file** to share. Import shows what is inside and what it may do, then a setup screen.
- An **Overlays page** in the app: every page with its address, Add to OBS, Test, Develop, Export.
- **Develop mode**: the group's files are a folder you edit with your own editor, pages reload on save, page
  errors appear in the Log, test buttons send fake events.
- A **guide** in the app and in MCP, with the reference generated from the code so it can't go stale.

## 5. The page API (`window.streamerz`, version 2)

Everything that exists keeps working. New calls return a Promise.

| Call | What it does | Needs |
| --- | --- | --- |
| `settings`, `on(topic, fn)`, `event(name, fn)` | As today | nothing |
| `onSettings(fn)` | `fn(settings)` now and whenever a setting changes in Streamerz | nothing |
| `chat(fn, {history: 50, platforms})` | Every chat line of every connected platform, same shape as the Chat page: platform, user, colour, badges, text, emotes with picture URLs, and `deleted`/`cleared` | read chat |
| `alerts(fn)` | One shape for follows, subs, gifts, cheers, raids, donations, rewards, on every platform: `{kind, platform, user, picture, amount, currency, months, tier, message}` | read alerts |
| `song(fn)` | Spotify: `{title, artist, album, art, durationMs, positionMs, playing, requestedBy}` | nothing |
| `countdown(id, fn)` | `{running, remainingMs, deadlineMs}`; the page counts from the deadline, Streamerz sends no ticks | nothing |
| `run(action, args)` | Runs an action **of this group**, resolves when it was queued | run actions |
| `say(text, {platform, bot})` | Sends to chat | send chat |
| `set(name, value)` / `get(name)` | A value of this group (survives restarts), seen by actions as `%group.name%` | store values |
| `publish(topic, value)` | Sets a topic for every page listening with `on()` | nothing |
| `log(text)` | A line in Streamerz's Log | nothing |
| `test` | True when opened from the Test button | nothing |

Rules:

- **Transport.** Down: the SSE stream that exists. Up: `POST /overlays/call` with `{op, ...}` and the page's
  token. Both on the HTTP server's port.
- **Token.** Streamerz adds a token to each page it serves (and to "Save as page" copies): a hash of a secret
  of this installation and the group's name. A call without a valid token is refused. This is how a call is
  tied to a group and its permissions, and why another website can't call.
- **`run` only reaches the group's own actions.** A template that needs more ships an action that does it,
  which the importer can read. A page never gets "run anything".
- **Topics are shared**: `publish` from a page sets the same topics as the Overlay sub-action, so `on()`
  reads both and one template can show another's data (the song request queue). Authors prefix their
  topic names (`roulette.spin`).
- **Feeds reuse what the app already computes**: `chat` and `alerts` are the Chat page's lines
  (`engine/chatlog.rs`), serialized only when a page listens (`emit` already takes a closure). `song` reads
  Spotify only if Spotify is set up and only while a page listens, like `viewers`.
- **Errors**: in develop mode the client forwards `window.onerror` and failed calls to the Log.
- `streamerz.d.ts` (TypeScript types) is generated and placed in every new template, for autocompletion.

## 6. Permissions and safety

A template's pages are code from a stranger. They run in OBS's browser, not in Streamerz, so the risk is
what they can ask Streamerz to do.

- The manifest lists what the pages may do: `chat.read`, `alerts.read`, `chat.send`,
  `actions.run`, `values.write`.
- The import dialog shows them in plain words ("Can send chat messages as you", "Can run its own 4
  actions"). An update that asks for more shows what is new and asks again.
- The user can turn each one off afterwards, on the Overlays page.
- Reading stays open to a page with no declared permissions only for what is public on screen today
  (`viewers`, `ad`, published topics). Whispers and donor e-mails are never sent to pages.
- The import dialog also says that pages can load things from the internet. We can't prevent that in OBS;
  we say it.
- Actions in a template are shown before import as today (`import_question`), with a warning on the
  sub-actions that touch the computer (run a program, write a file, key presses).
- The origin check that exists stays in front of everything.

## 7. Files on disk

- Files move from `streamerz.json` to `<config folder>/groups/<group id>/` on first start (one-time
  migration, the base64 entries are removed after the files are verified on disk). `Meta.files` keeps name
  and size only.
- Folders are allowed (`img/wheel.png`); names stay portable (the rules of `valid_file_name` per segment).
  Relative links in a page just work because `/overlays/<group>/` is the base.
- Served by reading the file when asked, in chunks, with `ETag` and range requests (video and long sounds).
- No size limits. Files are read from disk in chunks, so a large video costs no RAM. The import dialog and
  the Overlays page show the size, and an import from a URL shows it before anything is kept.
- Backups and the development clone (`Development isolation` in `PLAN.md`) include the folder.
- Result: less RAM than today, and saving the configuration stops rewriting file contents.

## 8. Settings a template can ask for

New kinds next to the six that exist, each with an explicit type (never guessed from the name):

| Kind | Form control | Value |
| --- | --- | --- |
| `color` | Colour swatch and hex field | `#rrggbbaa` |
| `slider` | Number with `min`, `max`, `step` | number |
| `lines` | Multi-line text | text |
| `file` | Pick or add a file of the group, filtered by `accept` (image, sound, video, font) | file name |
| `font` | Installed font names, or a `file` | text |
| `platforms` | Tick boxes of the four platforms | list |
| `action` | Pick one of the group's actions | action id |
| `list` | A table of rows with typed columns (any kind above), add, edit, delete, reorder, right-click menu | array of objects |

Plus `section` (a heading to group settings) and `showIf` (a setting shown only when another has a value).
`list` is what makes the roulette, per-event alert variations and goal steps configurable without touching
HTML. Values reach pages through `settings` and `onSettings`; file values arrive as URLs.

## 9. The template file and its life

**Manifest** (in the group's `Meta`, edited in a form, JSON one button away):

```json
{
  "id": "uuid", "name": "Roulette", "version": "1.2.0", "author": "name", "link": "https://...",
  "about": "One line.", "picture": "preview.png", "streamerz": "1.7.0",
  "pages": [
    {"file": "wheel.html", "title": "Wheel", "width": 800, "height": 800},
    {"file": "dock.html", "title": "Roulette controls", "dock": true}
  ],
  "permissions": ["alerts.read", "actions.run"],
  "requires": ["twitch", "obs"]
}
```

**File**: `Name.streamerz`, a zip written with `flate2` (already a dependency; no new crate):

```
template.json      manifest, settings definitions and defaults
actions.json       actions, commands, timers (the Streamer.bot export format, so the actions still import there)
files/...          the pages and media, as they are on disk
README.md          optional, shown in the import dialog
```

Being a zip, anyone can open it, read it and keep the folder in git. Pasted text export stays for
action-only sharing and Streamer.bot.

**Import**: file picker, drag and drop on the window, paste, or a URL. The dialog shows picture, name,
author, version, what is inside (counts and size), what it may do, what it needs and whether that is set up
(a missing integration links to its page). Then:

0. An overlay alone imports the same way and lands on the Overlays page; an overlay can later be moved into
   a template (and taken out), since both are groups.
1. Same `id` installed: offer Update (files and actions replaced, the user's values kept, new settings get
   their default) or Keep both.
2. Setup screen: the settings form, then each page with **Add to OBS** (section 10), ticked off as done.

**Export**: Overlays page or Groups, Export, As file. Checks before writing: every file a page links exists,
every setting has a label, unused permissions, total size, no secrets (the same scan as `dev/scan_secrets.py`
patterns on text files), user values are not exported unless ticked.

## 10. OBS

- **Add to OBS** (when OBS is connected): creates a browser source in the current scene with the page's URL,
  width, height and "shutdown when not visible" off, through the OBS connection that exists
  (`CreateInput`, kind `browser_source`). If a source with that URL exists it is selected, not duplicated.
- **Docks**: obs-websocket can't add a custom dock. The app shows the three steps (Docks, Custom browser
  docks, paste) with Copy name and Copy URL. Writing OBS's dock list ourselves is possible only while OBS is
  closed and changes another program's file: left for later, behind an explicit question.
- **Streamlabs Desktop**: same button through `slobs.rs` if its API allows; otherwise the steps.
- **Not HTML**: a text or image source fed by the OBS sub-actions that exist already works in a template.
  A clock in a text source would need a tick every second, so the page is the recommended way.
- Save as page (a local file) stays for people who don't want the HTTP server; it gets the token too.

## 11. In the app

**A new page, Overlays** (Automation section, after Groups). `Rows` list, one row per page, grouped by
template:

- Columns: name, where it belongs (a template's name, or nothing for an overlay by itself), `Health` (how many pages are connected now), address.
- Buttons and the same right-click menu: Copy URL, Add to OBS, Open in browser, Settings, Test, Develop,
  Export, Remove.
- Top: Import, New, Examples, Guide.
- **Examples**: the bundled ones (section 14), each with Add (installs it), **View files** (opens its
  folder, read only, to learn from) and **Copy to edit** (a copy of your own in develop mode).
- **New**: name, by itself or inside a template, starting point (blank, or a copy of an example). Creates the group, the folder, a
  commented starter page and `streamerz.d.ts`.
- **Test**: buttons that send a fake chat message, follow, sub, gift, donation, raid, song or a topic value
  to this template's pages only (the fakes of `scenario/fakes.rs`), without firing real actions elsewhere.
- **Develop** (a switch per template, off by default):
  - Open folder: the files are edited with any editor.
  - The folder is watched with `fswatch.rs` only while the switch is on; a change sends `reload` to the
    template's open pages.
  - Inspector: connected pages, the topics and their values, the last events sent, the page's errors.
- **Guide**: section 13, inside the app.

Groups keeps its Files dialog; both show the same data.

**MCP**, so an assistant can build and test a template end to end, each with a test: `template_new`,
`template_manifest` (read, write), `template_export`, `template_import` (file or URL, returns the question
before it changes anything), `overlay_test` (fake events), `overlay_inspect` (pages, topics, errors),
`overlay_add_to_obs`. The existing `overlays`, `overlay_publish`, `save_group_files` stay.

## 12. Decisions (agreed)

| # | Decision |
| --- | --- |
| 1 | The shared thing is a **template**. An **overlay** is its own thing: it lives inside a template or by itself. There is no separate "dock" concept: a dock page is an overlay. |
| 2 | A new **Overlays** page in the app. |
| 3 | The file is a zip named `.streamerz`. Pasted text stays for action-only sharing. |
| 4 | Pages may send chat and run actions: declared in the manifest, shown at import, own actions only. |
| 5 | Files move to disk with a one-time migration. |
| 6 | No size limits; sizes are shown. |
| 7 | Built-in templates and overlays are bundled files made with the same tools, readable and copyable so people learn from them. |
| 8 | Examples: as many as prove something (section 14). |
| 9 | Import from a URL: yes. A `streamerz://` link handler: later. |
| 10 | The version it ships in is the owner's call. |

## 13. The guide ("Make an overlay")

One source, `docs/OVERLAYS.md` rewritten, shown in the app's Guide and returned by MCP `guide`/`docs`:

1. Your first overlay in five minutes: New, edit, see it reload, Add to OBS.
2. How it works: group, files, settings, pages, the two directions.
3. The API, call by call, each with a ten-line example that can be copied.
4. Feeds reference: the shape of `chat`, `alerts`, `song`, `countdown`, `viewers`, `ad`, with real sample
   payloads. Generated from the code by a test that fails when the doc and the code differ.
5. Every event name for `event()`, from `events.json`.
6. Settings: each kind with a picture of its control.
7. Recipes: animate in and out, queue alerts, play a sound, a transparent background, fonts, a dock that
   follows OBS's theme, testing without going live.
8. Sharing: manifest, permissions, export checks, versions and updates.
9. For assistants: the MCP tools in the order to use them.

## 14. Examples (built with the tools, each proves a capability)

Each is a folder in `assets/examples/` with commented, readable source, embedded as files, checked by
Chromium in `dev/test-examples.mjs`, and listed under Examples with View files and Copy to edit.

| # | Name | Kind | Proves |
| --- | --- | --- | --- |
| 1 | Subathon clock (the current one, rewritten) | template | `countdown`, live settings, `color`, `font` |
| 2 | Now playing (Spotify) | overlay | `song`, picture from a URL, in and out animation |
| 3 | Roulette | template | `list` settings, `alerts` as the trigger, an action per slice, `run` from a dock page |
| 4 | Chat | overlay | `chat` with emotes, badges, platform logos, deletions, animation |
| 5 | Chat dock | overlay | `chat` with history, `say`, the send permission |
| 6 | Multistream alerts and event list | template | `alerts` on every platform, a queue, sound and video files, per-event variations |
| 7 | Viewer counter (existing, converted) | overlay | `viewers` |
| 8 | Ad break badge (existing, converted) | overlay | `ad` |
| 9 | Goal bar (followers, subs, donations) | template | `set`/`get`, values that survive restarts |
| 10 | Activity dock with Replay | overlay | `run` with arguments |
| 11 | Starting soon countdown | template | `countdown` started by an action, `file` settings (background, music) |
| 12 | Poll results | overlay | `event()` with the poll events of every platform |
| 13 | Emote wall | overlay | `chat` emote pictures, many moving elements at 60 fps |
| 14 | Top gifters leaderboard | template | Twitch gifts kept by actions with the page closed, periods, `onSettings` for live updates |
| 15 | Song request queue | overlay | the song request actions publish `songrequests.queue`, or another template supplies a custom topic |
| 16 | Control panel dock (buttons that run actions) | template | `list` of `action` settings, `run` |
| 17 | Credits roll | overlay | a one-off HTTP read (`GetCredits`), long animation |
| 18 | Blank starter | overlay | the commented minimum; what New copies |

## 15. Phases

Each phase ends with its tests, `cargo fmt`, `clippy`, and a tick here.

1. **Files on disk**: storage, folders, migration, streaming with ranges, backup and dev clone.
   Tests: migration keeps every byte, traversal refused, range requests.
2. **Calls up and the token**: `POST /overlays/call`, `run`, `say`, `set`, `get`, `publish`, `log`,
   permissions enforced. Tests: no token, wrong group, missing permission, another origin.
3. **Feeds**: `settings` live, `countdown`, `song`, `chat`, `alerts`. Tests: nothing is computed with no
   listener, Spotify is not read when not set up.
4. **Setting kinds**: the eight kinds, `section`, `showIf`, in the shared form control and `docs/UI.md`.
5. **Manifest and file**: zip writer and reader, export checks, import dialog, update, URL import.
   Tests: round trip, a damaged file, a zip with `../` names, an oversized file.
6. **OBS**: Add to OBS, the steps for a dock page. Tested against real OBS.
7. **Overlays page**: list, menus, New, Test. Smoke step in `dev/ui-smoke.py`, screenshots narrow and wide.
8. **Develop mode**: folder, reload, error forwarding, inspector, `streamerz.d.ts`.
9. **Examples**: 1 to 8 and 18 first (they replace the Rust templates for ad, viewers and subathon), then
   9 to 17, each one added only with its Chromium check. Song requests moves to a bundled file last.
10. **Guide and MCP tools**, with the doc and code comparison test.
11. **Close**: full smoke, `dev/test-overlays.mjs`, hidden RSS and idle CPU before and after (expected lower
    RSS for anyone with overlay files; unchanged otherwise), Windows check.

## 16. Performance

- No new port, thread or timer. One more route on the HTTP server.
- Nothing runs until a page listens; a feed with no listener is not serialized.
- File watching only for a template in develop mode.
- File contents leave the configuration and RAM.
- Countdown and song progress are computed in the page from a deadline and a position, not ticked.
- The zip code is used only at import and export.
