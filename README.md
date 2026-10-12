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
(the same fields as Streamer.bot's settings).
