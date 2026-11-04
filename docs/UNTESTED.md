# What isn't tested yet

Everything here is built, but nobody has checked it on the real thing. If you have the setup, you can
help: try it and open an issue saying what happened (it works, or what went wrong, with the log from
`logs/latest.log`). Run Streamerz with `STREAMERZ_DEV=1` to test without sending anything to your chats.

How the rest is tested is in [TESTING.md](TESTING.md).

## On a real Windows machine

The Windows build is tested under Wine on Linux (unit tests, scenarios, the UI and the Windows paths:
see TESTING.md). Wine can't do these:

- **Speech to Text** (triggers Speech to Text and Voice Command): Windows' speech recognizer (SAPI
  dictation), from the default microphone. Wine has no recognizer.
- **Spotify local** (Now Playing from the Spotify app): Windows' media session. Wine doesn't have it.
- **Toast Notification**: whether the toast shows, looks right, and a click on it fires Toast Clicked.
- **File Watcher on subfolders**: a change in a subfolder that already existed when the watcher started.
  Under Wine only subfolders made later are watched.
- **Hotkeys and Mouse Click while another program has focus** (a game, OBS). Under Wine they were only
  tested with Streamerz's own window focused.
- **Start with Windows**: that Streamerz really starts at login. The registry value is tested.
- **Saving on logout or shutdown**: that nothing is lost when Windows ends the session.
- **Saved logins** (DPAPI): that they survive a restart, and that another Windows user can't read them.
- **Opening the browser** to log in (Twitch, Kick, YouTube, Spotify), and opening folders.
- **TikTok**: it drives your browser (Chrome, Edge, Brave...); not tried on Windows.
- **AI agents** (MCP page): finding the installed CLIs (npm's `.cmd` shims) and opening them in a terminal.
- **The tray icon** on the real taskbar, light and dark, and the window following Windows' accent colour.

## On Linux

- **Mouse Click on Wayland with real clicks**: after **Allow** in the trigger, clicks in other programs fire
  it (tested with a stand-in device and the permission flow, not with clicks from a real mouse). On
  GNOME, KDE and others: whether `mouse.x` and `mouse.y` stay 0 as expected, and that touchpads work.
- **Key Press on Hyprland with a real key press**: Streamerz binds it through Hyprland's socket and the bind
  fires the trigger (tested by dispatching the bind; keys injected by ydotool don't reach Hyprland's binds).
- **Key Press on KDE and GNOME 48+**: the desktop's prompt to allow the shortcut.
- **Key Press on a Wayland desktop without the portal** (sway, older GNOME): reading the keyboards once
  allowed.
- **Allow without a polkit agent**: the fallback that opens a terminal with sudo.

## With hardware

- **MIDI devices**, on Windows and Linux: a real controller sending (MIDI Message) and a real synth or
  device receiving (MIDI Out sub-actions). Tested with a software loopback only.
- **Elgato Stream Deck**: the real Stream Deck app with Streamer.bot's plugin (`bot.streamer.streamdeck`)
  connected to Streamerz on port 8059 (Stream Deck Connected, Disconnected, Action, and the Stream Deck
  sub-actions that change a button). Tested against the plugin's protocol, not the plugin itself.
- **Microphones** other than the default one for Speech to Text (Set Voice Control Input keeps the choice,
  but the recognizer always uses the default microphone).

## Triggers only fired by a connection or by polling

These need a live account or service to fire, and no automated test fires them yet. Tested so far by
hand at most:

- **Twitch**: Present Viewers, Raid Start, Raid Cancelled, Viewer Count Update, Broadcaster
  Authenticated, Broadcaster Chat Connected and Disconnected, Broadcaster and Bot EventSub Connected and
  Disconnected, 7TV and BetterTTV Emote Added and Removed, Upcoming Ad.
- **YouTube**: Broadcast Ended, Statistics Updated, Present Viewers, New Subscriber, Broadcast Monitoring
  Started and Ended, 7TV and BetterTTV Emote Added and Removed.
- **Kick**: Present Viewers, 7TV Emote Added and Removed, Viewer Count Update.
- **TikTok**: Connected, Disconnected, Stream Online, Stream Offline.
- **Streamlabs Desktop**: Connected, Disconnected.
- **TreatStream**: Authenticated, Connected, Disconnected.
- **Spotify**: Disconnected, Local Connected and Disconnected, Track Changed, Playback Started, Playback
  Paused, Login Expired.

The up-to-date list is `todo` in `tests/excluded.json` (`python3 -P dev/scenario-todo.py` prints it).

## On a live channel

Sub-actions that change a channel (ads, raids, bans, timeouts, polls, predictions, title and game,
rewards) are tested against fakes of each platform's API, not on a live channel.
