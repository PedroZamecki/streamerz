# Overlay editor, sounds and voices (1.9.0)

The design for the overlay's own page, live preview and drafts, the media of an overlay, sounds played by
Streamerz, the alert box, new setting kinds, live controls and voices (text to speech). The phases are in
`PLAN.md` ("Version 1.9.0: Overlay editor, sounds and voices"). The owner agreed to the whole plan with the
recommended answers (2026-11-16); they are written in below as decisions.

## Goals

1. See every change the moment it is made, and test anything: any event, any value, any page.
2. See and hear an overlay's files: pictures, GIFs, videos, sounds and fonts.
3. Streamerz plays sounds itself, with the Streamer.bot sound sub-actions, its own extras and triggers.
4. An alert box set up entirely with forms, with HTML, CSS and JS one tab away.
5. More setting kinds and live controls for whoever writes an overlay.
6. Voices that cost nothing to anyone who does not install them.

## What it costs

Nothing here loads until it is used. The hidden window budget (under 25 MB RSS, about 0% idle CPU, with
Twitch and OBS connected) must hold with and without these features in use, measured before and after.

| Part | Not used | While used |
|---|---|---|
| Overlay page | Nothing: it is drawn by the window, a separate process that is gone while hidden | The window's own memory |
| Live preview | Nothing | A headless Chromium (the one `overlay_shot` already starts) while the Preview is on screen; closed when it is not |
| Sound player | Its code in the binary; no thread, no device open | One thread per device while a sound plays and, on Linux, the audio process; both end with the last sound, and the decoders' freed memory goes back to the system |
| System voices | Nothing | A few MB while speaking |
| Neural voices | A catalog and the Voices page in the binary (under 100 KB) | A separate process, downloaded on request: tens to a few hundred MB while a voice is loaded, one core for a fraction of the speech's length while speaking, 0% idle |

A neural voice can't run in 5 MB, but anyone who never presses Download pays nothing for it. That is the
rule for this whole plan.

## Part 1: The overlay page

Today an overlay's settings, pages and permissions are dialogs: change a value, press OK, the dialog
closes, then look at the page. The overlay gets a page of its own instead.

### Navigation

- The Overlays page stays the list. Opening a row (double-click, Enter, the Open button and its menu item)
  shows that overlay's page in the same place, with Back (and Escape, and the mouse's back button) to the
  list. The sidebar still says Overlays: no new navigation system.
- The overlay's page remembers its tab and selection, by stable ids, across Back and window restarts.
- The page is a core view (`core_view` domain `overlay:<group>`), so MCP sees and drives the same thing.

### Layout

- Wide (content of 1100px or more): the preview on the left (about 60%), the tabs on the right.
- Narrow: the preview on top (about 40% of the height, resizable), the tabs under it.
- Tabs: **Settings**, **Test**, **Media**, **Live**, **Alerts** (only for an alert box) and **Code**.
- The toolbar above keeps today's buttons: Add to OBS, Copy URL, Open in browser, Open folder, Export,
  and Save and Revert while there is a draft.

### Live preview

- A headless Chromium, started when the page is shown and closed when it is left, the window is hidden
  or minimised. Its picture comes over the DevTools protocol (`Page.startScreencast`): a frame is sent
  only when the page repaints, at most 30 a second, each one acknowledged, and drawn as a GPUI image. A
  still page sends nothing.
- The page opens at its size (Page and permissions) and is scaled to fit, with 100% and Fit buttons.
- Background: see-through (a checkerboard), dark, light, a picture of the user's choice, or **Your
  scene**: one picture of the program scene from OBS (`GetSourceScreenshot`, taken when asked, not on a
  timer).
- The preview takes clicks, the wheel and keys (`Input.dispatchMouseEvent`, `Input.dispatchKeyEvent`),
  so a dock page and a page's buttons work in it.
- Sound: the preview browser is started without `--mute-audio` and its sound goes to the monitor device
  (see Part 4). **(verify)** that headless Chromium plays sound on Windows and on Linux with PipeWire;
  if it does not, the page's sounds play through the Streamerz player for the preview.
- **Open preview in browser** opens the same preview page (with its preview token) in the user's
  browser: real motion, real sound, DevTools.
- No Chromium, Chrome, Brave or Edge on the computer: the preview says so, with Open preview in browser.

### Drafts: every change shows at once

- Every change on the Settings tab is a draft. It is sent at once to the **preview pages** only: the
  preview pane and pages opened with Open preview in browser. A page that listens with `onSettings`
  updates in place. A page that reads `streamerz.settings` only when it loads is reloaded (preview pages
  only), so even an overlay that was never written for it shows the change.
- **Save** stores the draft and sends it to every page, OBS included. **Revert** drops it. Right-click on
  a field: Revert this field, Reset to default.
- Leaving the page, closing the window or opening another overlay with a draft asks: Save, Discard or
  Cancel.
- **Show on OBS while I edit** (a switch on the toolbar, off by default) sends the draft to every page,
  OBS included, for someone setting up off air. Decided: off by default.
- The draft lives in the core, in memory, not on disk, so it survives a window restart and MCP sees it.
- How: a preview page's address carries a token the core gives (`?preview=<token>`), its WebSocket says
  so, and `onSettings` for those connections carries the draft. OBS pages never see a draft unless the
  switch is on.

### Settings tab

- Today's settings form at full width, with an index of its `section` headings on the side (wide) or as a
  list at the top (narrow), and a search box when there are more than 12 settings.
- Each setting keeps its label tip, `showIf` and kind. Reset to default per field.

### Test tab

Test anything, from one place, with what Test sends today kept as a quick button.

- **Event**: platform chips with their logos, then the kinds pages count with (`follow`, `sub`, `resub`,
  `giftsub`, `giftbomb`, `cheer`, `raid`, `superchat`, `member`, `gift`, `reward` and the rest), and
  **Any trigger** to search every trigger of the catalog.
- **Values**: a form built from that trigger's arguments, already filled with made-up values (a name, an
  avatar, an amount, a message with emotes). Random fills it again. Every field can be changed.
- **Send** once, or **Send several**: a count and the time between them (a burst; a task that ends with
  the burst, not a timer).
- **Send to**: this overlay (the default), every overlay, or **as a real event**: the trigger runs
  through actions too, with `isTest` true. As a real event asks first when one of the actions it would
  run sends chat or changes OBS.
- **Saved tests**: named sequences of steps (an event, a topic, a pause), kept in the overlay's
  `template.test.sequences` and shared with it. Built in: One of each, Gift bomb of 20, Raid of 500, TikTok
  rose combo, Chat flood (50 lines).
- **Replay**: the last 50 real events of this stream (kind, trigger and arguments), to see the overlay
  with what really happened. **(verify)** the chat log keeps these with the alert lines; if not, its
  bounded history grows the three fields (no new store).
- **Topics**: set any topic, as a form when its shape is known, as JSON one button away. Countdowns:
  start, pause, add time, reset.
- **Song**: a made-up song, playing or paused, with a cover.
- **What happened**: a list of what was sent and what the page logged or got wrong in return (what
  Inspect shows, inline), newest first, with Clear.

### Media tab

An overlay's files, seen and heard. The same block (`controls/media.rs`) draws the sound library of the
Sounds page.

- A grid (wide) or rows (narrow) with a toggle. Each file: a picture, its name, kind, size, and for
  sounds and videos its length.
- Pictures: a thumbnail. GIFs and animated WebP: animated while the pointer is on them. **(verify)** GPUI
  draws animated GIF; if not, the first frame, animated in the preview.
- Videos: a picture from the video (taken once by the preview browser, kept in `cache.rs`) and its length.
- Sounds: Play and Stop on the card, a bar with the position, its length, and a waveform made once when
  the file is added (kept in the cache, a few hundred bytes).
- Fonts: a line of sample text in that font.
- Buttons and the right-click menu: Add (Browse, or drop files on the window), Play on (monitor or
  default device), Rename (the settings that point at the file follow it), Replace, Copy URL, Open folder,
  Delete (says which settings use it first).
- **Used by**: which settings and pages use each file. A file nothing uses says so.
- A file over 25 MB, or a video in a format OBS's browser can't play, gets a notice with what to do.

### Live tab

The overlay's live controls (Part 3) and, for an alert box, its queue: what is showing, how many are
waiting, Pause, Skip, Replay last, Clear, Mute.

### Code tab

What is in Page and permissions today, plus the developer's tools: files and pages with their sizes and
dock setting, permissions, Inspect, Open in editor (the system's editor for that file), Open folder,
`streamerz.d.ts`, Export.

## Part 2: New setting kinds

Each kind gets its control in `controls/form.rs`, its check in `src/automation/`, its type in
`streamerz.d.ts`, a row in the guide's table (the guide test fails without it) and an example that uses it.

| Kind | Holds | Drawn as | A page gets |
|---|---|---|---|
| `date` | `YYYY-MM-DD` | a date picker, with presets (Today, Tomorrow, Next week) | the text |
| `time` | `HH:MM` | a time box, with `presets` | the text |
| `datetime` | a moment | a date and a time, with presets (In 15 minutes, In 1 hour, Tonight at 20:00) | milliseconds since 1970 |
| `duration` (extended) | milliseconds | today's box plus `presets` as chips (30s, 1m, 5m, 10m, 1h) | milliseconds |
| `range` | `[from, to]` | a slider with two handles and both values | the pair |
| `percent` | 0 to 100 | a slider with % | the number |
| `position` | `{anchor, x, y}` | a small canvas of the page's size: nine anchors and a dot to drag | the object, and `streamerz.place(el, value)` |
| `size` | `{width, height}` | two boxes and a lock for the ratio | the object |
| `animation` | `{name, durationMs, delayMs, easing}` | a list of the bundled animations (none, fade, slide up, down, left, right, zoom, pop, bounce, flip, shake), the times, the easing; Play shows it in the preview | the object, and `streamerz.animate(el, value)` |
| `easing` | a CSS easing | a list | the text |
| `gradient` | stops and an angle | a bar with the stops, each a color picker | the CSS |
| `textStyle` | font, size, weight, color, outline, shadow | one compact group | the object, and `streamerz.css(value)` |
| `file` (extended) | as today | as today, with a thumbnail and Play for sounds | as today |
| `sound` | `{file, volume}` | a file of kind sound, a volume slider and Play | the address and the volume |
| `event` | a list of kinds | check boxes with icons, grouped by platform | the list |
| `reward` | a reward id | the rewards of the connected Twitch and Kick channels | `{platform, id, title}` |
| `gift` | a TikTok gift | the gift catalog, with pictures, narrowed by typing | `{id, name, coins, picture}` |
| `usergroup` | a user group | the user groups | the name |
| `scene`, `source` | an OBS name | the scenes or sources when OBS is connected, a box when not | the name |
| `code` | CSS, HTML or JS | a tall box in a fixed-width font | the text |
| `json` | any value | a box checked as JSON | the value |
| `button` | nothing | a button; pressing it publishes the topic `<overlay>.<name>` once (Reset the count) | a press, through `streamerz.on` |
| `hidden` | any value | not drawn, kept and shared | the value |

Every overlay also gets **Extra CSS** under Code, a `code` setting that `streamerz.js` puts on the page
by itself, so a look can be changed in any overlay without opening its files. Decided: yes.

## Part 3: Live controls

Settings are what is set before going live; controls are what is changed while live.

- An overlay declares them in `meta.template.controls`: `[{"name": "show", "label": "Show", "kind":
  "toggle", "default": true}, {"name": "speed", "kind": "slider", "min": 1, "max": 10}, {"name": "spin",
  "kind": "button", "icon": "rotate-cw"}]`. Kinds: `button`, `toggle`, `slider`, `choice`, `number`,
  `text`, `color`.
- Their values are kept across restarts in the overlay's values (as `streamerz.set` keeps them), not in
  its settings, so sharing the overlay shares the controls and their defaults, never the live values.
- A page: `streamerz.control(name, fn)`, `fn(value, old)` now and at each change; a page may set its own
  controls with `streamerz.control.set(name, value)` (to keep a toggle in step).
- Streamerz: the Live tab; a dock page made from them at `/overlays/<overlay>/-/controls` (Add to OBS
  shows the dock steps); the sub-action **Set Overlay Control** (99881) and the trigger **Overlay Control
  Changed** (99881) with `overlay`, `control`, `value` and `from` (`window`, `dock`, `action`, `page`,
  `mcp`); MCP `overlay_control`; the WebSocket op `control`.

## Part 4: Sounds

### The player

- In the core (`src/sound/`), so sounds play with the window hidden.
- Decoding: Symphonia (pure Rust), only the formats Streamer.bot lists and two more: WAV, MP3, OGG Vorbis,
  M4A/AAC and FLAC. A file is read from disk as it plays, never decoded whole into memory.
- Output: cpal (WASAPI on Windows; ALSA on Linux, which reaches PipeWire and PulseAudio). The device is
  opened on the first sound and closed when its last sound ends, told by the audio callback through a
  channel: no timer, no poll.
- One mixer per device: each sound with its volume, the device's volume and the master volume; a small
  linear resampler when the file's rate is not the device's.
- Named sounds (`soundName`, or the file's name with `useFileNameAsName`), stopped, paused or changed by
  name. At most 16 at once by default (Output setting); the next one waits or is refused, as set.
- `finishBeforeContinuing` waits for the sound's end through a oneshot, never a sleep.
- A device that goes away: the sound ends with `reason: failed`, the log says why, and the next sound on
  Default goes to the new default.
- Flatpak: the manifest asks for `--socket=pulseaudio` **(verify)** the ALSA plugin in the runtime.
- Decided and done: Symphonia decodes; the output is the system's own, with no audio crate: on Linux a
  small audio process (`Streamerz --sound-output`, started with the first sound, ended after the last)
  that loads PulseAudio's simple API (PipeWire answers it too), so its 2.4 MB of libraries never stay in
  Streamerz (owner's choice, 2026-11-16: a few tens of ms on the first sound after a quiet spell); on
  Windows WASAPI through the `windows` crate already in the build. cpal would have added ALSA bindings
  and a thread of its own.

### Sub-actions

Streamer.bot's four, with its fields (`types.rs`) and its behaviour (docs.streamer.bot, checked when the
phase starts, arguments included):

| Code | Sub-action | Fields |
|---|---|---|
| 1 | Play Sound | `device`, `soundFile`, `finishBeforeContinuing`, `volume`, `soundName`, `useFileNameAsName` |
| 7 | Play Sound From Folder | `device`, `folder`, `finishBeforeContinuing`, `volume`, `recursive`, `soundName`, `useFileNameAsName`, plus Streamerz's `avoidRepeats` (kept on export) |
| 30001 | Stop All Sound Playback | none |
| 30002 | Stop Sound Playback | `soundName` |

Streamerz's own, 99300 to 99319:

| Code | Sub-action | Does |
|---|---|---|
| 99300 | Set Sound Volume | a named sound or all of them, to a volume, over `fadeMs` (a fade in or out) |
| 99301 | Pause Sound | a named sound or all |
| 99302 | Resume Sound | a named sound or all |
| 99303 | Set Master Volume | the master volume, over `fadeMs` |
| 99304 | Get Sound Info | a file's `soundDurationMs`, `soundFormat`, `soundChannels` as arguments |
| 99305 | Play Sound In Overlay | plays a file on an overlay's open pages (the browser source's audio, so OBS mixes it with that source); `streamerz.js` plays it with no page code. Comes with live controls, since a page must be connected to hear it |
| 99306 | Set Sound Device | the device that Default means for Streamerz. Comes with the Output section of the Sounds page, which stores it |

Paths from a Streamer.bot import (`C:\...`) are kept; on Linux a path that is not there says so on the
sub-action, with Pick file.

### Triggers

Streamerz's own, 99300 to 99309:

| Code | Trigger | Arguments |
|---|---|---|
| 99300 | Sound Started | `soundName`, `soundFile`, `device`, `soundDurationMs`, `actionName` |
| 99301 | Sound Ended | the same, and `reason` (`finished`, `stopped`, `failed`) |
| 99302 | Sound Device Changed | `device`, `available` (a device came or went). Comes with the Sounds page: it needs the system's device notifications (IMMNotificationClient, PulseAudio's subscribe), never a poll |

### Sounds page

A sidebar page, **Sounds** (Lucide `volume-2`), with a Show selector: Playing, Library, Board, Voices,
Output. Decided: one page with the Show selector.

- **Playing**: what plays now: name, file, device, place and length, a volume slider; Stop, Pause, Stop
  all; the right-click menu has the same.
- **Library**: every sound file Streamerz knows: those the actions use (sub-action fields), those of the
  overlays, and a `sounds` folder beside `streamerz.json`. The media block, with Play (monitor or
  default), Copy path, Used by (the actions and overlays), New action with this sound (an action with a
  Play Sound ready), Add to Board.
- **Board**: pads of chosen sounds, pressed to play, each with its key, volume and device; also a dock page
  (`/sounds/board`) for OBS, and reachable from Stream Deck through actions.
- **Output**: the default device, the **monitor** device (what previews play on: the streamer hears it,
  OBS does not capture it when it is a different device), the master volume, at most at once, and Play a
  test sound.
- **Voices**: Part 5.

## Part 5: Voices

### Engines

1. **System voices**, in Streamerz, nothing to download:
   - Windows: `Windows.Media.SpeechSynthesis` (WinRT, a feature of the `windows` crate already used). It
     gives a WAV in memory, played by the Streamerz player, so it goes to any device.
   - Linux: `espeak-ng --stdout` when it is installed (on the host in Flatpak, through `sandbox::on_host`);
     when it is not, the Voices page shows the command to install it.
2. **Neural voices**, downloaded on request: `streamerz-voice`, a small program of ours over sherpa-onnx
   (Apache-2.0, with ONNX Runtime, MIT).
   - Built by our CI for Windows x64 and Linux x64 with each release, published as its own file. Streamerz
     is built with its SHA-256 and downloads only the copy made for its version.
   - It runs Piper voices (many languages, Brazilian Portuguese included), Kokoro (many voices in one
     model, which can be mixed) and, for Make a voice, Pocket TTS (a voice from a short recording).
   - It talks over stdin and stdout (framed JSON, then the PCM), loads a model the first time it is used,
     unpacks downloads (the models come as `.tar.bz2`, so the main binary needs no bzip2), and quits
     when voices are turned off, on Unload, or when Streamerz closes (stdin closed).
   - **Keep the voice loaded** (on by default while voices are on) answers at once; off, the program quits
     when the speech queue is empty and starts again for the next one (a second or two later, no memory
     between). Decided: on by default.

### Catalog and downloads

- `assets/voices.json` in Streamerz: each voice's id, engine, language, style, quality, size on disk and
  in memory, address (the sherpa-onnx releases on GitHub and Hugging Face), SHA-256, license with its
  attribution, and a short sample to hear before downloading.
- New voices come with Streamerz updates; there is no catalog fetched in the background. Decided.
- A download can be cancelled and picks up where it stopped, is checked against its SHA-256, and lands in
  `voices/` beside `streamerz.json` (not in backups, not in exports).
- The license is shown before the download, and voices that are not free for commercial use say so.
- **(verify)** at phase start: sizes (Piper medium voices are about 60 MB, Kokoro int8 about 130 MB,
  Pocket TTS to be measured), memory while loaded, speed on a 4-core CPU, and the licenses of the Kokoro
  and Pocket TTS weights and of each Piper voice we list.

### Voices (on the Sounds page)

- **Engine**: system voices and their state; neural voices: Download (with its size), version, memory in
  use now, Unload, Remove (frees the disk).
- **Find voices**: the catalog, the UI's language first, with language, quality, size and license
  filters; Play sample, Download with progress, Cancel.
- **Installed**: a box to type in and Speak (on the monitor or the default device); speed, pitch and
  volume; Set as default; Delete.
- **Make a voice**:
  - **Mix voices** (Kokoro): two to four voices, each with a slider for its share, and the speed; Speak
    to hear it, Save with a name. A mix is a weighted average of the voices' style vectors: instant, a few
    KB on disk.
  - **From a recording** (Pocket TTS): a WAV or MP3 of 5 to 10 seconds of one person talking, or one
    recorded from a microphone here; the user ticks that it is their own voice or one they have
    permission to use; made on this computer, nothing is uploaded; Save with a name. **(verify)** the
    languages (reported: English, French, German, Spanish, Portuguese, Italian) and the weights' terms.
  - **Effects**: pitch, speed, and a few made by the player (echo, radio, robot), saved with the voice.
  - Training a voice from scratch is not offered: it takes a GPU, hours and a large set of recordings.

### Speaking

- One speech queue per device. Filters, as group settings of the template and as Speak fields: skip
  links, say emote names or skip them, cap repeated letters, a maximum length, words never read (the
  moderation lists when they are set), skip commands and bots, `%user% says` before the message.
- A voice per viewer: the user variable `ttsVoice`, set by a command or an action.

Sub-actions, 99320 to 99339:

| Code | Sub-action | Does |
|---|---|---|
| 99320 | Speak | `text`, `voice`, `volume`, `speed`, `pitch`, a device or an overlay, `finishBeforeContinuing`, the filters; sets `speechDurationMs` |
| 99321 | Speak To File | the same into a WAV; sets `speechFile` (as Speaker.bot's process-only Speak) |
| 99322 | Stop Speaking | the current speech |
| 99323 | Clear Speech Queue | what is waiting |
| 99324 | Set Speech Paused | `state` |
| 99325 | Set Speech Enabled | `state` |
| 99326 | Set User Voice | a user and a voice (the `ttsVoice` user variable) |
| 99327 | Get Voices | `voices` (the installed ones) and `voiceCount` |

Triggers, 99320 to 99329:

| Code | Trigger | Arguments |
|---|---|---|
| 99320 | Speech Started | `text`, `voice`, `user`, `speechDurationMs` |
| 99321 | Speech Ended | the same, and `reason` (`finished`, `stopped`, `skipped`, `failed`) |

- A page: `streamerz.speak(text, {voice})` (permission `voice.speak`) gives the address of the speech as a
  WAV, served once at `/overlays/-/speech/<id>`, so an alert's voice plays in the browser source in step
  with the alert.
- Action template **Read chat aloud**: `!tts`, a channel point reward, `!voice <name>` per viewer,
  `!skip` for moderators, the filters as its settings.

## Part 6: The alert box

A built-in example, **Alert box**, plain DOM and CSS animations driven by its settings. Nothing in it needs
HTML; HTML, CSS and JS are there for whoever wants them. Decided while building it: the whole runtime is
the example's own `index.html` (commented, so a power user reads and changes it, and a `.streamerz` file
or a saved page carries it), and the rules are a `list` setting (When, From, At least, At most, Text,
Picture, Sound, Volume, Seconds, Weight) that the Alerts tab will edit as a form. The controls are a
topic for now (`alertbox`, `{"do": "pause"}`...), and become live controls in Part 3.

### Queue

- One at a time, at most N at once, or the newest replaces the one showing; a gap between alerts; the
  longest an alert waits before it is dropped.
- Merge: a gift bomb is one card with its count; a TikTok combo grows on the card showing.
- Pause, Skip, Replay last, Clear and Mute are its live controls, on the Live tab and its dock.
- The queue runs in the page. Streamerz adds no timer for it.

### Rules (variations)

Each rule:

- **When**: kinds, platforms, and conditions: amount from and to (bits, coins, money in the stream's
  currency), tier, months from and to, gift names, user groups, first time, message contains.
- **Which one plays**: the first matching rule in order, or one of the matching rules by weight, chosen
  per alert box. Decided: both.
- **Show**: a picture, GIF or video (fit, loop); the layout (picture above, at the left, at the right,
  behind the text, text only, picture only); its `position` and `size`; the text from a template
  (`%user%`, `%amount%`, `%message%`, `%gift%`, `%months%`, `%platform%`) with a `textStyle` and a color
  for the names in it; the viewer's message with its own style; animations in, out and for the text; how
  long it stays (or as long as its sound); a delay before the text.
- **Sound and voice**: a sound with its volume and delay; read the message (or a template) with a voice,
  at most N characters, after the sound or over it.
- **Code**: HTML, CSS and JS for this rule, which take over the parts they define; CSS for the whole box.

### Editor

The **Alerts** tab of the overlay page: the rules as rows (drag to reorder, a switch to turn one off,
Duplicate, Delete, the same in the right-click menu), and the selected rule's form in four tabs: When,
Show, Sound and voice, Code. **Play this rule** sends a matching made-up event to the preview. Hints when
a rule can never play ("Subs of every tier above it catches all of these").

### Next to the others

| | Streamlabs | StreamElements | Twitch | Streamerz |
|---|---|---|---|---|
| Platforms in one queue | mostly one | mostly one | Twitch | Twitch, YouTube, Kick, TikTok |
| Variations and conditions | yes | yes | some | yes, with user groups and TikTok gifts |
| Picture, GIF, video, sound | yes | yes | yes | yes, seen and heard in Streamerz |
| Text to speech | cloud | cloud | no | on this computer, voices you choose or make |
| Custom HTML, CSS, JS | per variation | custom widget | no | per rule or the whole box |
| Test | buttons | emulate | basic | any event, any value, saved sequences, replay |
| Live controls and dock | some | some | no | pause, skip, replay, mute, from window, dock, actions, Stream Deck |
| Driven by actions and assistants | no | some | no | yes |
| Works offline, files local | no | no | no | yes |

## Part 7: Examples

New examples, each with its Test data, browser checks and narrow and wide screenshots:

- **Alert box** (Part 6), with a rule for each common event.
- **Countdown to** a moment (`datetime` with presets, `textStyle`, `position`).
- **Sound board dock** (live controls, sounds through the Streamerz player).
- **Read chat aloud** (the action template and a small page showing who is being read).
- **Scene banner**: a message that comes in and out (`animation`, `position`, `button`, controls).

The current examples get the new kinds where they are better: Starting soon (`datetime`, `sound`), Goal
bar (`textStyle`, `position`), Alerts (moves to the alert box, the old page kept as a code example).

## Part 8: MCP and WebSocket

Every new feature can be driven without the window:

- `overlay_draft` (read, set, save, revert), `overlay_preview` (a frame of the preview as a PNG, the
  background), `overlay_control` (read and set), `overlay_test` extended (any trigger, a burst, a saved
  sequence, as a real event), `overlay_media` (list files with their kind, size, length and users).
- `sound_play`, `sound_stop`, `sounds` (what plays, the devices, the library).
- `voices` (installed and catalog), `voice_install` and `voice_remove` (asked first, as other downloads),
  `speak`, `voice_mix`.
- The same as WebSocket requests (`StreamerzOverlay` gets `draft`, `preview`, `control`, `media`; new
  `StreamerzSound` and `StreamerzVoice`), each with a test.

## Part 9: Docs

- `docs/OVERLAYS.md`: the overlay page, drafts, the Test tab, the new kinds, controls, the alert box,
  `streamerz.control`, `place`, `animate`, `css`, `speak`; the guide test checks them.
- `docs/SOUNDS.md` and `docs/VOICES.md`, also opened from the Sounds page.
- `docs/UI.md`: the editor with a preview (list then item page with Back), the media block, a
  download with progress and license, the board of pads.
- `docs/reference`: Core / Sounds no longer "not done", and the new sub-actions and triggers.
- `docs/MCP.md`, `docs/TESTING.md`, the README's features.

## Part 10: Tests and checks

- Unit: decoding of tiny files of each format, the mixer and resampler, volume fades, named stops, a
  device that goes away, `finishBeforeContinuing`; drafts reach preview pages only and Save reaches the
  rest; controls are kept and published; each setting kind's check; the alert box's rule matching (JS,
  in `dev/test-examples.mjs`); the voice program's protocol against a fake one; catalog hashes; a
  download cut and picked up again.
- Scenarios: actions with Play Sound and Speak, run offline with `run_scenario` (a fake player).
- Smoke (`dev/ui-smoke.py`, also with `--locale pt_BR`): the overlay page and each tab, a draft and
  Revert, the Media grid, the Sounds page and each Show, Voices with no engine, right-click everywhere.
- Real: sounds on Windows and on Linux (PipeWire), OBS capturing a sound played in an overlay, a Piper
  voice and a Kokoro mix speaking, the alert box in OBS with every event.
- Hidden RSS and idle CPU before and after, and once more after a sound played and a voice spoke and
  was unloaded, to see the memory come back.
- Windows type-check (`cargo xwin check`).

## Build and release

- `build.yml` builds `streamerz-voice` in the same PR run as Streamerz, before it, and gives its SHA-256 to
  the Streamerz build as an environment variable: no step changes the tree, the merge publishes the same
  files.
- The voice program is a release file of its own, never in the packages, the AppImage or the Flatpak.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` for both.
