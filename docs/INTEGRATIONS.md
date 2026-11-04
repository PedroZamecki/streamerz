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

- **Inputs & Hotkeys**: On Windows, the Keyboard Press sub-action sends a key and optional modifiers.
  Keyboard simulation is not implemented on Linux. Key Press and Mouse Click triggers, stored as Streamer.bot stores them (SharpHook's `keyCode` and `modifierMask`).
  Set a trigger's keys by clicking **Keys** and pressing the combination. They work whichever program has
  focus, and listen only while an enabled action uses them:
  - Linux on X11: the X server gives both (key grabs and XInput2's clicks). Nothing to set up.
  - Linux on Wayland, keys: the desktop's Global Shortcuts portal (xdg-desktop-portal), with no root and no
    `input` group. KDE and GNOME ask you to allow each shortcut. On Hyprland, Streamerz binds them itself
    through Hyprland's socket while it runs (non-consuming, so other programs still get the keys) and
    removes them when it quits. The portal needs an app id, so a hidden `streamerz.desktop` is written to
    `~/.local/share/applications` the first time. A desktop without the portal (sway, older GNOME) can
    read the keyboards instead, once you allow it in the trigger.
  - Linux on Wayland, clicks: Wayland gives no program the clicks made in other windows, so Streamerz reads
    the mice themselves. The Mouse Click trigger has an **Allow** button: it asks for your password once
    (polkit, or sudo in a terminal) and adds `/etc/udev/rules.d/70-streamerz-mice.rules`, which lets the
    person at the computer read mice and touchpads (systemd's `uaccess`, as for sound cards). Keyboards
    stay closed. **Remove access** deletes the rule. Only button presses are read. The pointer's place
    comes from Hyprland; other Wayland desktops don't share it (`mouse.x` and `mouse.y` are 0), and a
    touchpad's taps aren't clicks the device sees.
  - Windows: RegisterHotKey (a combination another program already holds can't be taken), and a mouse hook
    only while a Mouse Click trigger is used.
  - Mouse Click's **Button** is caught like the keys: click the field, then click with the button you want
    (or **Any button**). `mouse.button` is 1 left, 2 right, 3 middle, 4 back, 5 forward.
- **MIDI**: Input and output are not implemented yet. MIDI output sub-actions only log their values.
- **Voice Control**: Speech input is not implemented yet. The command and input sub-actions only update in-memory state.

## Decks, Stream Deck, and Chat Window

- **Streamer.bot Decks**: Deck state is kept in memory. Integration with a real Deck needs verification.
- **Elgato Stream Deck**: State is kept in memory, but Stream Deck key presses and connection events are not read yet.
- **Chat Window**: multi-platform chat window action trigger (`ChatWindowAction`) with platform, user, message, and action arguments.

## File I/O, file tail, watcher and process triggers

- **File Operations**: Write to File (append or overwrite), Read Lines, Read Specific Line, and Read Random Line sub-actions with variable expansion and auto-typing.
- **File and Folder Checks**: File Exists and Folder Exists sub-actions for testing path presence and setting boolean arguments.
- **File Tail, File Watcher and Process Triggers**: The engine can dispatch these events, but no monitoring source is connected yet.

