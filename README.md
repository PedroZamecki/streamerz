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
changes. Until the editor exists, change them in `streamerz.json` under `websockets`, `http` and `udp`
(the same fields as Streamer.bot's settings). WebSocket clients and custom servers go in
`websockets.connections` and `websockets.servers`.

## The editor

Click the tray icon to open the window. The Actions tab lists your actions by group; select one to see
its triggers and sub-actions. Add, edit, enable/disable and delete them with the buttons; drag an action
onto a group to move it there, and drag sub-actions to reorder them or into a group, If/Else branch or
loop. Double-click anything to edit it. Sub-actions and triggers without their own form yet open as
JSON. **Import** takes a Streamer.bot import string (pasted or from a file); **Export** makes one from
the actions you tick, with the commands, timers and queues they use. Sub-actions that don't run in
streamerz are kept, shown greyed out with "(not supported)", and skipped.
