# streamerz

A small, light Streamer.bot replacement in Rust: Twitch, Kick, YouTube, OBS Studio and Streamlabs Desktop,
with Streamer.bot's actions, triggers and servers. Not usable yet; see [PLAN.md](PLAN.md).

## Install

Download `streamerz.exe` (Windows) or `streamerz` (Linux) from the latest
[release](https://github.com/PedroZamecki/streamerz/releases) and run it; on Linux, make it executable
first (`chmod +x streamerz`). It starts hidden in the tray: click the icon to open the window. Settings
are kept in `streamerz.json` next to it (and the previous version in `streamerz.json.bak`).

Settings → Window: dark theme, start hidden in the tray, and start with Windows / when you log in.

## Streamer.bot's servers

streamerz answers on Streamer.bot's WebSocket server (`ws://127.0.0.1:8080/`, on by default), its HTTP
server (port 7474) and its UDP server (port 4242), both off by default, with the same requests, events
and authentication, so tools made for Streamer.bot (songrequestz, overlays, Stream Deck) connect without
changes. Set them in the Servers tab (changes apply at once), along with WebSocket clients and custom
servers.

## The editor

Click the tray icon to open the window. The Actions tab lists your actions by group; select one to see
its triggers and sub-actions. Add, edit, enable/disable and delete them with the buttons; drag an action
onto a group to move it there, and drag sub-actions to reorder them or into a group, If/Else branch or
loop. Double-click anything to edit it. Core sub-actions and triggers open in a form (actions, queues,
timers and commands picked by name; "Edit JSON" shows the whole thing); the others open as JSON.
**Import** takes a Streamer.bot import string (pasted or from a file); **Export** makes one from the
actions you tick, with the commands, timers and queues they use. Sub-actions that don't run in
streamerz are kept, shown greyed out with "(not supported)", and skipped.

The other tabs, as in Streamer.bot:

- **Commands** and **Timers**: add, edit (a form), turn on/off and delete them.
- **Queues**: your queues and Default, with what's running and waiting in each; add, edit, delete,
  Blocking, pause/resume (one or all) and clear. **Pending** lists the waiting runs.
- **History**: the last 100 runs (actions set to Exclude from History are left out), with their
  variables when queued and after the run; **Run again** queues one with the same arguments.
- **Globals**: every global and user global, persisted or not; add, edit and delete globals (these fire
  the same triggers as Global (Set)). User globals are shown only.
- **Groups**: add and delete user groups (their Added/Removed triggers fire). Users join them from chat.
- **Servers**: the WebSocket, HTTP and UDP servers, WebSocket clients and custom WebSocket servers.
- **Twitch**: Broadcaster and Bot account login (Device Code Grant OAuth via browser), auto-connect on startup, and EventSub connection controls.
- **Stream Apps**: OBS Studio and Streamlabs Desktop connections.
- **Settings**: window options and this instance's name (what GetInfo reports).

## Streamlabs Desktop

In Streamlabs Desktop, open Settings → Remote Control, click the QR code to show the details, and copy
the token (the port is 59650 unless you changed it). In streamerz, add a connection in the Stream Apps
tab with that token. "Default connection" is the one sub-actions use when they don't name one. Every
Streamlabs Desktop trigger and sub-action of Streamer.bot works, with the same arguments. Differences:
Set Browser Source URL and Set GDI Text work (in Streamer.bot 1.0.7 they do nothing), and in a
dual-output scene a source's horizontal and vertical copies both change unless "Display" picks one.

## Webhooks and remote integrations

- **Discord Webhook**: post basic messages, embeds, custom username/avatar and TTS directly to Discord webhooks.
- **IFTTT**: send basic and advanced notification triggers directly to IFTTT webhooks.
- **Pronouns**: fetch and cache user pronouns using the alejo.io Twitch Chat Pronouns API, populating standard pronoun variables.
- **Remote Instances**: connect multiple streamerz or Streamer.bot instances via WebSocket clients or servers, sending and waiting for remote triggers and signals.

## Hotkeys, inputs, MIDI and voice control

- **Inputs & Hotkeys**: Key Press sub-actions (keyboard press simulation with modifiers) and Input Key Press / Mouse Click triggers with key, button, and modifier filters.
- **MIDI**: MIDI Message triggers (note on/off, CC, pitch bend, sysex) with device, channel, and status filters, and Note On, Control Change, and Generic MIDI Out sub-actions.
- **Voice Control**: Speech to Text Dictation and Speech to Text Command triggers, plus Set Voice Control Command, Set Command State, and Set Voice Input sub-actions.

## Decks, Stream Deck, and Chat Window

- **Streamer.bot Decks**: set item state, toggle state, title, background, icon, value, deck notifications, and page navigation (set, next, previous).
- **Elgato Stream Deck**: set title, background, state, toggle state, value, show alert, and show OK sub-actions; Action, Connected, Disconnected, and Info triggers.
- **Chat Window**: multi-platform chat window action trigger (`ChatWindowAction`) with platform, user, message, and action arguments.
