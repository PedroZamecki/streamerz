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

- **Chat**: every set-up platform's chat **all together** (tagged TW/YT/KI/TT), **side by side** (one
  column each), or one platform alone (then what you send goes to that platform). Click a name for that
  user's card: roles, messages this session, their variables (%userName%...), and what Twitch or Kick says
  (account age, following since, subscription, their channel). Emoji show as they are, and platform emotes (Twitch, Kick, YouTube's
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
  channel read (the broadcaster's own), **Connect**, **Connect on start**, and **Login** for the broadcaster
  and the bot, with their profile pictures. Chat is sent as the bot when one is logged in. **Advanced…**
  has the rest, and for YouTube, Kick and TikTok an **Other channel** to read instead of the broadcaster's
  (a URL or @handle works; empty: the broadcaster's again):
  - Kick: sending needs your own Kick app (kick.com > Settings > Developer > Create app, redirect URL
    `http://127.0.0.1:45563/callback`, every scope; the bot account is only asked for chat). Paste its client ID and secret, then Login. Reading
    chat needs no login.
  - YouTube: the video ID (empty: the channel's live) and your Google OAuth client, needed to send:
    console.cloud.google.com > New project > enable **YouTube Data API v3** > OAuth consent screen
    (External, then **Publish app**; unpublished apps log you out every 7 days) > Credentials > Create
    OAuth client ID, type **Desktop app**. Paste its ID and secret, then Login. Google warns that the app
    is unverified: it's yours, so continue. Reading chat needs no login.
  - TikTok: streamerz reads the channel's live itself, with no login (it waits while the
    channel is offline). Login opens TikTok's own login page; sending types into the live page's chat in a
    hidden window (`streamerz-tiktok` next to streamerz on Linux), which closes after 2 idle minutes.
- **Stream Apps**: OBS Studio and Streamlabs Desktop connections.
- **Settings**: window options and this instance's name (what GetInfo reports).

The window comes back as it was left: its size, its tab and the chat's view.

Downloads are kept in `cache/` next to the settings file: pictures (at most 50 MB; the least recently
shown go first), the channels' BTTV/FFZ/7TV emote lists and the accounts' picture addresses (used at
once, fetched again in the background after a day). Deleting the folder is safe.

The bottom bar counts the connections that are set up (platforms, OBS and Streamlabs Desktop connections,
StreamElements, Streamlabs, DonorDrive), e.g. "● 3/5 connected". Click it for each one's state (connected,
connecting, or not and why), with **Retry** and **Settings** (its tab).

## Streamlabs Desktop

In Streamlabs Desktop, open Settings → Remote Control, click the QR code to show the details, and copy
the token (the port is 59650 unless you changed it). In streamerz, add a connection in the Stream Apps
tab with that token. "Default connection" is the one sub-actions use when they don't name one. Every
Streamlabs Desktop trigger and sub-action of Streamer.bot works, with the same arguments. Differences:
Set Browser Source URL and Set GDI Text work (in Streamer.bot 1.0.7 they do nothing), and in a
dual-output scene a source's horizontal and vertical copies both change unless "Display" picks one.

## Donations: StreamElements, Streamlabs, DonorDrive

In the Donations tab:

- **StreamElements**: paste the JWT token (StreamElements dashboard → your account → Channels → Show
  secrets). Streamer.bot logs in with OAuth instead; the token does the same. Tips and merch arrive
  live, including the dashboard's Emulate buttons.
- **Streamlabs**: paste Your Socket API Token (Dashboard → Settings → API Settings → API Tokens).
  Donations, charity donations and merch arrive live, including test alerts.
- **DonorDrive** (Extra Life and other charities on DonorDrive): the site and your participant ID.
  streamerz asks every 15 s whether your page changed (what DonorDrive asks for), so a donation
  shows up within 15 s.

Each one connects only when it's filled in, and reconnects a minute after it drops. Tip, Donation and
Charity Donation triggers have Streamer.bot's Range (Min and Max amount).

## Webhooks and remote integrations

- **Discord Webhook**: post basic messages, embeds, custom username/avatar and TTS directly to Discord webhooks.
- **IFTTT**: send basic and advanced notification triggers directly to IFTTT webhooks.
- **Pronouns**: fetch and cache user pronouns using the alejo.io Twitch Chat Pronouns API, populating standard pronoun variables.
- **Remote Instances**: connect multiple streamerz or Streamer.bot instances via WebSocket clients or servers, sending and waiting for remote triggers and signals.

## Hotkeys, inputs, MIDI and voice control

- **Inputs & Hotkeys**: Key Press sub-actions (keyboard press simulation with modifiers), and the Key Press
  and Mouse Click triggers, stored as Streamer.bot stores them (SharpHook's `keyCode` and `modifierMask`).
  Set a trigger's keys by clicking **Keys** and pressing the combination. They work whichever program has
  focus, and listen only while an enabled action uses them:
  - Linux: through the desktop's Global Shortcuts portal (xdg-desktop-portal), with no root and no `input`
    group. KDE and GNOME ask you to allow each shortcut; Hyprland binds them in its config, so the capture
    window shows the line to add to hyprland.lua or hyprland.conf (**Copy line**). The portal needs an app
    id, so a hidden `streamerz.desktop` is written to `~/.local/share/applications` the first time.
    Mouse Click doesn't work on Linux: only reading every input device could see clicks, which also sees
    every key typed.
  - Windows: RegisterHotKey (a combination another program already holds can't be taken), and a mouse hook
    only while a Mouse Click trigger is used (`mouse.button` 1 left, 2 right, 3 middle).
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

## Chat

- Twitch badges as Twitch's own pictures (the ones the chatter wears, as Twitch's chat shows them); other platforms' roles drawn. **Options** hides the time or the badges.
- One view for every set-up platform's chat (together, side by side, or one platform), drawn with the platforms' logos, role badges and name colours. Click a name for what's known about that user.
- Emotes as pictures: Twitch (animated ones too), Kick and YouTube's own, the channel's BTTV, FFZ and 7TV emotes (7TV on Kick too), animated ones moving while the chat is on screen; Unicode emoji as Twemoji pictures.
- An emote picker with the emotes seen in chat, the channel's third-party emotes and every emoji by group. Right click a message to copy it.

## Credits

- Platform logos: [Simple Icons](https://simpleicons.org) (CC0 1.0).
- Emoji pictures: [Twemoji](https://github.com/jdecked/twemoji) by Twitter and contributors (CC-BY 4.0), loaded from jsDelivr when shown.
- Emoji list: Unicode's [emoji-test.txt](https://unicode.org/Public/emoji/latest/emoji-test.txt) (Unicode License v3).

## MCP (AI assistants)

`streamerz --mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server on stdin/stdout that
drives a running streamerz through its WebSocket server: status, log, actions, run an action, list and
fire triggers, inject a chat message and read what would have been sent. Start streamerz with
`STREAMERZ_DEV=1` for the testing tools (it then keeps chat replies instead of sending them). It connects to
the address in `streamerz.json` next to the exe, or `STREAMERZ_URL`.

- Claude Code: `claude mcp add streamerz -- /path/to/streamerz --mcp` (this repo's `.mcp.json` points at
  `target/release/streamerz` and a dev instance on port 8090).
- Other clients: command `/path/to/streamerz`, argument `--mcp`.
