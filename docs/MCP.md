# Use Streamerz through MCP

Register the Streamerz executable with the argument `--mcp` in your client's MCP
configuration, or use the app's MCP page. Set `STREAMERZ_URL` to the running
instance's WebSocket address when needed. Development instances normally use
`ws://127.0.0.1:8090/`; the shipped default is port 8080. The MCP process itself
opens no window and connects only when a tool needs the running app.

## Build and test an action

1. Call `list_templates` first. A template may already cover the task.
2. Find types with `list_triggers` and `list_subactions`, then call `describe`
   for their fields, `formState` (`form`, `no_settings` or `json`), `available`
   status and a ready item. `docs` gives examples and requirements; `docs_search`
   finds reference pages by words.
3. Call `new_action`, fill its triggers and subActions, and test it with
   `run_scenario` or `preview_gift_trigger`. Call `guide` with name `scenarios`
   for the scenario format. These tools work without a running app or account.
4. Use `save` to install the action. Use `get` before editing an existing item:
   `save` replaces the whole item with the same id. Keep the returned saved id.
5. On a dev instance, use `fire_trigger` or `inject_chat` to test the installed
   configuration, and `action_history` to inspect observed step outcomes. Their
   chat output is captured. They wait briefly for actions; delayed work may
   finish later. `chat_out` drains remaining captured messages.

## Inspect and change a running instance

`get` reads actions, commands, timers, user groups and queues, and
`action_history` returns recent action runs with their observed `executionStatus`.
Action groups have
separate tools for membership, metadata, variables, icons, files and export.
`export_actions` and `export_group` return both an export object and an import
string. Pass the object to `import_actions`. Imports containing programs require
review before setting `allowPrograms`. A whole group with its pages and files is shared as one
`.streamerz` file: `template_check`, `template_export`, `template_inspect` and `template_import`
(see `docs/OVERLAYS.md`).

`connectors` reports OBS and Spotify availability without secrets. Spotify tools
require an enabled, signed-in integration, except `spotify_local`, which reads a
Windows media session. OBS requests and Spotify playback and queue controls act
on real applications, including in dev mode. Disabled integrations remain off.

`agents` discovers clients. `agent_setup` changes the selected client's config;
ask the user before changing another program. `tiktok_bridge` status is read-only;
install, login and remove affect the user's browser setup. `tiktok_gifts` reads
the connected TikTok room's discovered gift catalog (and refreshes it through the
browser account when `refresh: true` is passed).

## Development and failures

Start the app with `STREAMERZ_DEV=1` for `status`, `log`, `fire_trigger`,
`inject_chat`, `chat_out`, `ui` and `obs_request`. Offline scenario and
gift-filter preview tests need no dev flag. A normal instance does not expose
these development requests.

The MCP bridge does not authenticate WebSocket sessions. Use a separate local dev
instance with authentication off. An unavailable service or bad tool argument
returns `isError: true` with a reason. A failed scenario returns `passed: false`
and its failures. Tool arguments are checked before requests reach the app.
Connection setup, sends and replies have timeouts. A channel search allows 60
seconds for its reply; other requests allow 10. After a timeout or dropped
connection, the next call reconnects. A timed-out mutation may have completed:
read its state before retrying it.

Run `python3 dev/mcp_smoke.py target/debug/Streamerz` to exercise every advertised
tool through stdio on a separate dev instance at `DISPLAY=:99`. It uses scratch
settings and no account credentials. Live OBS, Spotify and platform success paths
still need a configured service. Run `cargo test mcp::tests` for schema, dispatch
and protocol regressions.

## OBS stream outputs

`obs_outputs({connectionId?})` discovers stream destinations on an already connected OBS instance.
With several connections, pass `connectionId`. The reply has `outputs`, `canvases` and `capabilities`.
An output has an opaque `id`, `source`, `name`, nullable `active` and `reconnecting`, `controllable`,
optional `platform` and `platformSource`, and any `limitation`. A `name_hint` platform is a guess from
its name, not a verified destination. Recordings and virtual cameras are excluded.

`obs_outputs_control({connectionId?, op: "start" | "stop", outputs: [id, ...]})` acts on exactly those
IDs. It rediscovers first, rejects missing or ambiguous selections before changing anything and returns
one result per destination. `requested` means the plugin queued the command, not that the destination
is live. Read `obs_outputs` again to inspect actual state. `unchanged` means it already had the requested
state. A selected main stream is started and confirmed first, and stopped last. If it is reconnecting,
Streamerz waits for its existing reconnection instead of sending another start request. If starting it fails,
additional starts are skipped. A main stream that was not selected is never started implicitly.

The wire request is `StreamerzOutputs`, with `op: "list" | "start" | "stop"`, `connectionId` and
`outputs`. It uses the normal connection authentication and works outside dev mode. Discovery does
not enable an integration, open another socket, read plugin configuration files or keep a polling task.
Timeouts exist only for in-flight requests and waiting for a selected main stream's start event.

Supported adapters:

- Stock OBS: main stream and its service; canvas enumeration when supported by obs-websocket.
- Aitum Stream Suite: configured stream outputs, actual state and its canvases.
- Aitum Multistream: configured horizontal and vertical outputs; state is read from matching runtime
  outputs and confirmed start/stop events. A confirmed stopped state survives removal of its runtime
  object. Event state is discarded on disconnect and profile/scene collection changes. Names must be
  unique, including across canvases.
- Aitum Vertical: the first matching canvas's stream group. Starting it starts all enabled destinations
  on that canvas. Its API does not enumerate individual dormant destinations.
  This aggregate control is omitted when Multistream already lists its vertical destinations.
- Multi-RTMP websocket fork (`sorayuki.multi_rtmp`): targets by ID. The upstream plugin without that
  vendor API cannot enumerate dormant configured targets through obs-websocket.
- Other plugins: instantiated service outputs exposed by OBS. Potential duplicate runtime outputs
  alongside a supported plugin are read-only; use the plugin destination's control.

Plugin APIs do not consistently expose platform/server or encoder dependency metadata. Streamerz
reports this gap instead of inferring a confirmed platform or starting an unselected main stream.
Streamlabs Desktop output adapters are not included in these OBS tools.

Protocol references: [obs-websocket](https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md),
[Aitum Stream Suite](https://github.com/Aitum/obs-aitum-stream-suite/blob/main/src/utils/obs-websocket.cpp),
[Aitum Multistream](https://github.com/Aitum/obs-aitum-multistream/blob/main/multistream.cpp),
[Aitum Vertical](https://github.com/Aitum/obs-vertical-canvas/blob/master/vertical-canvas.cpp),
[Multi-RTMP websocket fork](https://github.com/davidcool/obs-multi-rtmp-websocket-support/tree/pr-obs-websocket).
