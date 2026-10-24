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

Unlike Streamer.bot, websites can't reach them: a browser page from another site (which could otherwise
run your actions or chat as your bot) is refused. Apps, local overlays (OBS browser sources with local
files, `localhost`) and the websites you list in Servers > WebSocket > **Websites allowed** get in.

## Your logins

Logins (tokens, cookies) and passwords/secrets you type are never stored as plain text in
`streamerz.json`: on Windows they're encrypted for your Windows user (DPAPI); on Linux with a key kept in
your keyring (GNOME Keyring, KWallet, KeePassXC), which may ask to unlock the first time. Without a
keyring they stay in the file, which only you can read. **Logout** also revokes the login at Twitch, Kick
and Google. Copying `streamerz.json` to another computer or user doesn't carry the logins: log in again.
Importing actions that run programs shows them and asks first.

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

- **Chat**: every connected platform's chat **all together** (tagged TW/YT/KI/TT), **side by side** (one
  column each), or one platform alone. Emoji show as they are, and platform emotes (Twitch, Kick, YouTube's
  own) are highlighted by name. Send at the bottom: **To** a platform or **All connected**, **as bot** (the
  broadcaster when no bot is logged in) or **as broadcaster**, Enter or **Send**. **Emotes** offers emoji
  and the emotes seen in chat; each platform gets its own format (on Kick an emote name becomes Kick's
  emote tag, a Kick emote sent elsewhere becomes its name). A send that fails says why, in grey.
- **Commands** and **Timers**: add, edit (a form), turn on/off and delete them.
  A command can just answer: put the text in **Reply text** (variables like `%user%` and `%input0%`
  work); it answers where it was typed, or everywhere with **Reply on every connected platform**. The
  platform boxes choose where a command works; new ones work everywhere.

## Built-in chat commands

Added once on first run, in the Built-in group: edit, turn off or re-alias them like any command
(deleted ones stay deleted). They work on every platform, answering where they were typed.

- `!commands`: the commands that work here and that the asker may use.
- `!command add !name text`, `!command edit !name text`, `!command remove !name`,
  `!command platforms !name twitch,kick,youtube,tiktok` (or `all`), `!command show !name` (moderators).
- `!so user` (moderators): "Go check out user at …" with that platform's link; on Twitch also Twitch's
  own shoutout.
- `!uptime`: how long this platform's stream has been live (not on TikTok).
- `!title`: this platform's stream title; moderators set it with `!title new title`.
- **Queues**: your queues and Default, with what's running and waiting in each; add, edit, delete,
  Blocking, pause/resume (one or all) and clear. **Pending** lists the waiting runs.
- **History**: the last 100 runs (actions set to Exclude from History are left out), with their
  variables when queued and after the run; **Run again** queues one with the same arguments.
- **Globals**: every global and user global, persisted or not; add, edit and delete globals (these fire
  the same triggers as Global (Set)). User globals are shown only.
- **Groups**: add and delete user groups (their Added/Removed triggers fire). Users join them from chat.
- **Servers**: the WebSocket, HTTP and UDP servers, WebSocket clients and custom WebSocket servers.
- **Accounts**: one row per platform (Twitch, YouTube, Kick, TikTok): connected or not (and why), the
  channel (a URL or @handle works), **Connect**, **Connect on start**, and **Login** for the broadcaster and
  the bot. Chat is sent as the bot when one is logged in. **Advanced…** has the rest:
  - Kick: sending needs your own Kick app (kick.com > Settings > Developer > Create app, redirect URL
    `http://127.0.0.1:45563/callback`, every scope; the bot account is only asked for chat). Paste its client ID and secret, then Login. Reading
    chat needs no login.
  - YouTube: the video ID (empty: the channel's live) and your Google OAuth client, needed to send:
    console.cloud.google.com > New project > enable **YouTube Data API v3** > OAuth consent screen
    (External, then **Publish app**; unpublished apps log you out every 7 days) > Credentials > Create
    OAuth client ID, type **Desktop app**. Paste its ID and secret, then Login. Google warns that the app
    is unverified: it's yours, so continue. Reading chat needs no login.
  - TikTok: the tikstream/TikFinity bridge URL, and Start tikstream. Login uses tikstream-login.
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

## File I/O, file tail, watcher and process triggers

- **File Operations**: Write to File (append or overwrite), Read Lines, Read Specific Line, and Read Random Line sub-actions with variable expansion and auto-typing.
- **File and Folder Checks**: File Exists and Folder Exists sub-actions for testing path presence and setting boolean arguments.
- **File Tail**: Set File Tail State sub-action and File Tail Changed trigger for monitoring log and text file appends.
- **File Watcher**: File Changed, File Created, File Deleted, and File Renamed triggers for monitoring filesystem directory modifications.
- **Process Triggers**: Process Started and Process Stopped triggers to react to application launches and terminations.
