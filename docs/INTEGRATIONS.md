# Integrations

Everything Streamerz connects to besides the four chats and OBS. Each is off until you set it up.

## Streamlabs Desktop

In Streamlabs Desktop, open Settings → Remote Control, click the QR code to show the details, and copy
the token (the port is 59650 unless you changed it). In Streamerz, add a connection in the Stream Apps
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
  Streamerz asks every 15 s whether your page changed (what DonorDrive asks for), so a donation
  shows up within 15 s.

Each one connects only when it's filled in, and reconnects a minute after it drops. Tip, Donation and
Charity Donation triggers have Streamer.bot's Range (Min and Max amount).

## Webhooks and remote integrations

- **Discord Webhook**: post basic messages, embeds, custom username/avatar and TTS directly to Discord webhooks.
- **IFTTT**: send basic and advanced notification triggers directly to IFTTT webhooks.
- **Pronouns**: fetch and cache user pronouns using the alejo.io Twitch Chat Pronouns API, populating standard pronoun variables.
- **Remote Instances**: connect multiple Streamerz or Streamer.bot instances via WebSocket clients or servers, sending and waiting for remote triggers and signals.

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

