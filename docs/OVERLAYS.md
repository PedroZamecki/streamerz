# Overlays

An overlay is a single `.html` file kept in an action group. Streamerz serves it on the HTTP server's port and sends it live data. Nothing is hard coded: you write the page (or start from a template), and it travels with the group when you export or import it, together with the actions that feed it.

## Make one

1. Turn the HTTP server on (Settings, Servers). The default port is 7474.
2. Add an `.html` file to an action group (Actions, the group's settings, Files, Add file), or add a template: **Ad break overlay** or **Viewer counter overlay**.
3. In the Files window select the file and press **Copy URL**. Paste it in an OBS browser source:
   `http://127.0.0.1:7474/overlays/<group>/<file>`
4. `http://127.0.0.1:7474/overlays` lists every overlay.

**Save as page** writes a copy that has Streamerz's script and address inside, to use as a local file in OBS. It works while Streamerz runs.

## Share an overlay

In Actions, press **Export** and tick the group. Groups appear even when they have no actions.
The export carries the original files, setting definitions and values, and the group's actions,
commands and timers. Import that export on the other installation, then copy its overlay URL.
The receiving installation uses its own HTTP address and port.

**Save files** extracts the original files. **Save as page** adds the client and the current server
address for a local OBS source. Use the group export when sharing an editable overlay.

## In the page

Streamerz adds `window.streamerz` to the top of the page:

| Call | What it does |
| --- | --- |
| `streamerz.settings` | The group's settings (`%group.name%` in actions), so colors, texts and sizes are edited in Streamerz and not in the HTML. |
| `streamerz.on(topic, fn)` | `fn(value)` runs with the topic's current value, then at every change. |
| `streamerz.event("Source.Type", fn)` | `fn(data)` runs for any Streamerz event, such as `Spotify.TrackChanged` or `Twitch.Follow`. `Twitch.*` is every event of a source. |

```html
<div id="n"></div>
<script>
  streamerz.on("viewers", v => n.textContent = v.total);
</script>
```

The page connects only if it asks for something, over server-sent events on the same port, and reconnects by itself.

## Topics

| Topic | Value |
| --- | --- |
| `viewers` | `{twitch, youtube, kick, tiktok, total}`. A platform that is not connected is `null`. The counts are read once a minute while a page listens. |
| `ad` | While Twitch plays an ad: `{running: true, duration, endsAt, isAutomatic, startedAt}`; when it ends `{running: false}`. `endsAt` is in milliseconds. |
| any name | Set by the **Overlay** sub-action (topic and value) or the MCP tool `overlay_publish`. |

To show a game score, a goal or the current song, make an action that runs on the right trigger and ends with an Overlay sub-action. For example, "Spotify Track Changed" then Overlay with topic `song` and value `{"title": "%spotifyTitle%", "artist": "%spotifyArtist%"}`.

## For assistants (MCP)

`overlays` lists the overlays with their URLs and the topics that have a value. `overlay_publish` sets a topic. `set_group_meta` writes the group's files (base64) and `add_template` installs a template.

## Development checks

`node dev/test-overlays.mjs` checks a scratch dev instance with WebSocket port 8095 and HTTP port 7495.
It needs Node with built-in WebSocket support and Chromium. It adds test groups and an action, so use
an isolated development configuration. It checks live topics and events, viewer totals, ad expiry,
settings, group export/import, local HTML, and refused foreign origins. Screenshots go to
`/tmp/szoverlay`.
