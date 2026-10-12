# streamerz plan

Streamer.bot rebuilt in Rust, built like songrequestz and tikstream: one small exe, FLTK window + tray,
~nothing idle. Same model as Streamer.bot (actions made of sub-actions, fired by triggers, with
variables, commands, timers and queues), same data format (imports Streamer.bot exports, reads its
`actions.json`), and the same WebSocket/HTTP/UDP servers, so songrequestz, tikstream and existing overlays
connect to it without changes.

Platforms: **Twitch, OBS Studio, Kick, YouTube, Streamlabs Desktop**, then the chosen integrations.
Every documented trigger and sub-action for those is in scope. `COVERAGE.md` lists each one, and each
phase ticks its own items there.

Development starts only after this plan is agreed. Work one phase at a time, one branch and one PR per
phase, following `CLAUDE.md`.

## Decisions (agreed)

| topic | decision |
|---|---|
| Name / repo | `streamerz`, public GitHub `PedroZamecki/streamerz`, MIT. Same rules and CI as songrequestz (`CLAUDE.md`). |
| Compatibility | Drop-in for Streamer.bot's servers: WebSocket (127.0.0.1:8080, `/`), HTTP (7474), UDP (4242). Same requests, events, auth and JSON. songrequestz stays a separate app with no changes; it connects to streamerz exactly as it does to Streamer.bot. |
| Data model | Streamer.bot's own model and field names (`actions.json` v24: actions, `subActions` with numeric `type`, triggers, queues, groups, commands, timers, globals). Import Streamer.bot export strings from day one. Export works too, so actions can go back to Streamer.bot. |
| Type codes | Built from Streamer.bot.exe's .NET metadata (sub-action/trigger enums and their property names), read with a decompiler, for interoperability only. Only the code↔name/field mapping goes into the repo; no Streamer.bot code or data. |
| Unsupported on import | Kept untouched (so a re-export loses nothing), shown disabled, logged once, skipped at run. This applies to C# code, out-of-scope integrations and the later extras. |
| Scripting | No C#. The "Run a Program" sub-action is extended: it runs any exe/script, passes the arguments as JSON on stdin, and reads `key=value` lines or a JSON object from stdout back into arguments. No embedded interpreter. |
| Variables | Full Streamer.bot parity: `%arg%`, `~global~`, persisted and temporary globals, user globals, inline functions (`$math()$` style, same names and syntax as the docs), same argument names per trigger. `$math()$` covers a common subset of mXparser: numbers, `+ - * / ^ %`, parentheses, comparisons, `& | ~`, abs round floor ceil min max sqrt exp ln log log10 sin cos tan asin acos atan sign rand/rnd, pi and e; anything else gives NaN with a log line, as Streamer.bot does. `%x:format%` follows .NET format strings in the OS locale (glibc locale data on Linux, Windows' own on Windows). |
| Editor | Native FLTK window + tray (starts hidden), with the same tabs and panes as Streamer.bot: Actions (groups tree, sub-actions list, triggers list), Commands, Platforms, Stream Apps, Integrations, Servers/Clients, Settings, Log. Plain FLTK look with the dark theme. Tools: action history (last N runs with their args, re-run), pending queue view (running/queued per queue, clear), test trigger (fire any trigger with sample args). |
| Storage | One `streamerz.json` next to the exe: settings, actions, commands, timers, queues, globals, user data and tokens. Gitignored. Written atomically (temp file + rename) with a `.bak` of the previous version. |
| Twitch | EventSub WebSocket for chat and every event, Helix REST for sub-actions, no IRC. Broadcaster account plus an optional bot account (chat is sent as the bot when set, as in Streamer.bot). Device-code OAuth with our public client ID, so there's no secret in the exe. |
| Kick | Read through Kick's public Pusher websocket (what kick.com uses): chat, follows, subs, gifts and more, with no login. The official API with OAuth is used to send chat and moderate. That needs the **user's own Kick app** (client ID + secret pasted in Settings), because Kick requires a secret even for refresh. Official webhooks are optional, through a user-run tunnel to the local HTTP server, with signatures verified (RSA-SHA256, Kick's public key). |
| YouTube | Read chat and events through InnerTube (what the youtube.com page uses): no quota, no login. The official Data API with Google OAuth (our project; the installed-app secret is embedded, which Google allows) is used to send messages, moderate and set the title/description. If InnerTube breaks, the fallback is the official gRPC `liveChatMessages.streamList`, which Streamer.bot itself uses. |
| OAuth apps | The repo owner registers the Twitch and Google apps (steps under "Setup"). Users only click Login. Kick is the exception (see above). |
| Stream apps | OBS Studio (obs-websocket v5) and Streamlabs Desktop (its websocket API). Meld Studio and PolyPop are not supported. |
| Integrations | Opt-in and off by default; a disabled one opens nothing. In scope: donations/shops (Ko-fi, StreamElements, Streamlabs, Patreon, Fourthwall, Throne, Tipeee, Treatstream, DonorDrive, Pally, Shopify, Streamloots) and webhooks/remote (Discord, IFTTT, pronouns, Streamer.bot remote instances). |
| OS | Windows + Linux (Wayland, ksni tray). CI releases a Windows exe and a Linux binary on every release. |
| Budget | Hidden window, Twitch + OBS connected: < 25 MB RSS and ~0% idle CPU, measured and written in every PR. Streamer.bot uses ~300 MB. |
| Dependencies | The songrequestz set (tokio single-thread runtime, tokio-tungstenite, futures-util, serde, serde_json, sha2, base64, fltk, ksni/zbus, tray-icon/windows-sys, embed-resource), plus **native-tls** (SChannel/OpenSSL, through tokio-tungstenite's feature), **flate2** with miniz_oxide (import/export), serde_json's **preserve_order** (indexmap, so Streamer.bot's JSON keeps its key order), and **rsa** (Kick webhooks only). HTTP is a small HTTP/1.1 client written here over tokio + TLS, with keep-alive per host. **tokio-native-tls** (the crate tokio-tungstenite's native-tls feature uses, for HTTPS), **regex-lite** (If/Else, While, Switch and regex commands; .NET's backreferences and lookaround aren't supported). Anything else needs a question first. |
| Not in scope | TikTok (that's tikstream), Meld, PolyPop, Elgato, VTube Studio/Voicemod/Speaker.bot/TITS/Crowd Control/HypeRate/Pulsoid/Lumia/Streamfog/mtion, the Streamer.bot website integration/cloud, C# and plugins. |
| Later (own phases, after phase 14) | Quotes, credits, points, users/first words; sounds and TTS; file I/O, file tail, folder watcher, process triggers; hotkeys/inputs, MIDI, voice control; Decks, HTML overlay, multi-platform chat window. |

## Architecture

- `main.rs`: config, shared `Status`, and the single-thread tokio runtime on its own thread. FLTK runs on
  the main thread (as in songrequestz).
- `model.rs`: Streamer.bot data types (serde, unknown fields kept as raw JSON so nothing is lost) and
  `types.rs`, the code↔name table.
- `engine.rs`: trigger dispatch, action runner, queues (blocking/concurrent, pause), random/weights,
  always-run, nested actions, signals, delays (timers on tokio, never polled), variable/inline function
  expansion, history and pending.
- `servers/`: WebSocket, HTTP and UDP servers, plus WebSocket clients and custom servers (from triggers).
- `http.rs`: the small HTTPS client. `oauth.rs`: device code, PKCE with a loopback listener, refresh.
- One file or folder per platform/app/integration (`twitch/`, `obs.rs`, `kick/`, `youtube/`,
  `slobs.rs`, `integrations/<name>.rs`). Each is a task started only when enabled and stopped on disable
  (`watch` config like songrequestz).
- `ui/`: editor tabs and one form per sub-action family.

Event flow: a platform task parses an event, turns it into a trigger and its arguments, and passes it to
the engine; the engine runs the matching actions and emits WebSocket events. Sub-actions call back into
platform handles through channels, never through shared locks held across awaits.

## Protocols (references, verify each when its phase starts)

### Streamer.bot servers
- WebSocket: `Hello` on connect, with `authentication {salt, challenge}` when auth is on.
  `Authenticate`: `base64(sha256(base64(sha256(password + salt)) + challenge))`. Auth is enforced only on
  privileged requests unless set otherwise.
- Requests: Subscribe, UnSubscribe, GetEvents, GetActions, DoAction, GetBroadcaster, GetCredits,
  TestCredits, ClearCredits, GetInfo, GetActiveViewers, GetCodeTriggers, ExecuteCodeTrigger, GetCommands,
  TwitchGetEmotes, YouTubeGetEmotes, GetGlobals, GetGlobal, TwitchGetUserGlobals, TwitchGetUserGlobal,
  SendMessage, GetUserPronouns.
- Event categories and names come from the real Streamer.bot's `GetEvents`.
- songrequestz's `src/streamerbot.rs` is a working client to test against.
- HTTP: `POST /DoAction {action:{id|name}, args}`, GetActions, GetCredits, TestCredits, ClearCredits,
  ClearFirstWordsCache, and optional static file serving.
- UDP: `{"request":"DoAction","action":{...},"args":{...}}`.

### Export string
- `base64("SBAE" + gzip(JSON))`. The JSON has `meta {name, author, version, description, autoRunAction,
  minimumVersion}` and `data {actions, queues, commands, timers, websocketClients, websocketServers}`.
- Older exports name the sub-action list `actions` instead of `subActions`; accept both.

### Twitch
- Device code: `POST https://id.twitch.tv/oauth2/device`, then poll `/token` at the given interval until
  approved (login only), and refresh after that.
- EventSub WS `wss://eventsub.wss.twitch.tv/ws`: after `session_welcome`, create the subscriptions through
  Helix; handle `session_keepalive`, `session_reconnect` and revocation.
- Chat comes from `channel.chat.message` and other `channel.chat.*` subscriptions; it's sent with Helix
  `POST /chat/messages`.
- Third-party emotes for the emote-added triggers: BTTV/FFZ through REST at connect, 7TV through its
  event API websocket. Each is only on when a trigger uses it.

### OBS Studio
- obs-websocket v5 `ws://127.0.0.1:4455`: `Hello` → `Identify` (sha256 auth like above, with event
  subscriptions), `Request`/`RequestResponse`, `Event`, and `RequestBatch` for raw requests.

### Kick
- Pusher `wss://ws-us2.pusher.com/app/<key>?protocol=7`: subscribe to `chatrooms.<id>.v2` and
  `channel.<id>`. Look up the chatroom/channel ids from the slug at connect.
- Official API `https://api.kick.com/public/v1/` (chat, moderation, channel). OAuth 2.1 with PKCE and the
  secret at `https://id.kick.com/oauth/token`. The redirect is on loopback; Kick's docs warn about a
  `127.0.0.1` rewrite bug, so use the workaround they give.
- Webhooks: the HTTP server gets a `/kick` path, with RSA-SHA256 signature checked against Kick's public
  key.

### YouTube
- InnerTube: find the live video from the channel's `/live` page, then call
  `youtubei/v1/live_chat/get_live_chat` with a continuation, waiting the `timeoutMs` the server returns
  (the server sets the pace; this is the one poll that can't be avoided).
- Official Data API v3 to send, moderate and set the broadcast title/description.

### Streamlabs Desktop
- Websocket API `ws://127.0.0.1:59650/api/websocket` (SockJS framing), token auth, JSON-RPC to the
  services (scenes, sources, streaming, recording, replay buffer).

## Setup (repo owner, once, before the phases that need it)

- Twitch (phase 6): dev.twitch.tv → Register application. Client type **Public**, redirect
  `http://localhost`, category "Application Integration". Enable Device Code Grant. Put the client ID in
  the code (it's public).
- Google (phase 11): a Cloud project, enable YouTube Data API v3. OAuth consent screen External, testing
  then production. Create an OAuth client of type **Desktop app**, and put the ID and secret in the code
  (Google treats installed-app secrets as non-confidential).
- Kick: nothing for the owner. Each user creates their own app at kick.com → Settings → Developer, and
  README documents the steps.

## Definition of done (every phase)

- It works against the real service, live where possible, and side by side with the real Streamer.bot
  for behaviour and JSON shapes.
- The phase's `COVERAGE.md` items are ticked, or struck through with the reason.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` and the Windows check
  pass.
- One test for each piece of non-trivial parsing or logic (Streamer.bot JSON, event parsing, variable
  expansion), with no frameworks.
- Hidden RSS and idle CPU are measured, written in the PR description, and within budget.
- README (user-facing) and DEVELOPING.md (dev notes) are updated, and the phase is ticked here.

## Phases

- [x] 0. Plan, rules (`CLAUDE.md`/`AGENTS.md`), commit hooks, the `commits` CI check, coverage list.
  Public repo with the songrequestz settings (squash only, delete branch on merge, "protect main" ruleset).
- [x] 1. Scaffold:
  - Cargo, single-thread runtime, `streamerz.json` (atomic save + `.bak`), shared status, log.
  - Window and tray skeleton from songrequestz (starts hidden, autostart, dark theme), with the icon.
  - CI: `lint` (rust + commits); `build` with the Windows exe and Linux binary on PRs and releases.
  - Add `rust`, `windows` and `linux` to the ruleset's required checks.
  - Measure empty baseline RSS: 10.1 MB hidden, 0 CPU ticks in 30 s idle (Linux, release build).
- [x] 2. Data model and import/export:
  - The type table from Streamer.bot.exe's metadata (a local extraction step, documented in
    DEVELOPING.md; throwaway scripts outside the repo, Python dnfile since there's no .NET SDK).
  - Model types that keep unknown fields.
  - Read Streamer.bot's `actions.json` and import/export strings, both round-trip byte-for-byte on the
    JSON.
  - Tests: hand-made fixtures of each export version (v3, v10, v23) and of actions.json, committed; the
    docs' sample exports (all 10) and the real Streamer.bot's actions.json through `STREAMERZ_SAMPLES`,
    not committed (the docs have no license; actions.json is user data).
- [x] 3a. Engine core:
  - Sub-action field table (code → name, category, JSON fields) from the metadata, next to the type codes.
  - Triggers, actions, queues, history, pending and test trigger (no UI yet).
  - Variables, formatting (OS locale) and inline functions.
  - Sub-actions: action queues, actions, arguments, delay, globals, logic, misc, signals, triggers; C# kept
    and skipped. Triggers: Custom, Test, Streamer.bot Started, global variables.
- [x] 3b. Engine, the rest:
  - Commands (permissions, cooldowns, aliases, regex) and their sub-actions and triggers.
  - Timers (cron deferred: see open points), Fetch URL, UDP Broadcast, user groups, and the extended
    Run a Program.
  - Tick the rest of COVERAGE "Phase 3".
- [ ] 4. Servers:
  - WebSocket, HTTP and UDP drop-ins.
  - WebSocket clients and custom servers, with their triggers.
  - On 8090, songrequestz and the real Streamer.bot's own clients must work unchanged; compare responses
    with the real Streamer.bot.
  - Tick COVERAGE "Phase 4".
- [ ] 5. Editor UI:
  - All tabs, the actions tree with drag-reorder, the sub-action list (enable/disable, nesting for logic).
  - Trigger and command editors, timers, queues, globals viewer.
  - Server settings, import/export dialogs, history, pending, test trigger.
  - Forms for the core sub-actions.
- [ ] 6. Twitch A:
  - Login (broadcaster + bot) and the EventSub session.
  - Chat in and out: messages, whispers, announcements.
  - Follows, subs, gifts, cheers, raids, first words, stream online/offline.
  - Their sub-actions and forms.
- [ ] 7. Twitch B:
  - Channel points rewards (create/update/redemptions), power-ups.
  - Polls, predictions, goals, hype train, community goals.
  - Channel info (title, category, tags), clips, markers, ads.
- [ ] 8. Twitch C:
  - Moderation (bans, timeouts, unban requests, AutoMod, shield mode, warnings, chat settings).
  - Shared chat, Guest Star, user groups.
  - Third-party emotes and the rest of the Twitch list. COVERAGE Twitch complete.
- [ ] 9. OBS Studio: connection settings, every sub-action (including raw/batch) and trigger.
- [ ] 10. Kick:
  - Pusher read, and user-app OAuth for send/moderation.
  - Optional webhooks with signature check.
  - Every trigger and sub-action.
- [ ] 11. YouTube:
  - InnerTube chat and events (messages, Super Chats/Stickers, memberships, gifts, polls…).
  - Google OAuth to send/moderate and set the title/description; viewer counts.
  - Every trigger and sub-action.
- [ ] 12. Streamlabs Desktop: every sub-action and trigger.
- [ ] 13. Donations and shops: one integration per commit, each opt-in. Check each one's transport first
  (socket, REST or webhook through the same tunnel as Kick) and ask if one needs Streamer.bot's cloud.
- [ ] 14. Webhooks and remote: Discord webhook, IFTTT, pronouns (alejo.io), Streamer.bot remote instances
  (signals/triggers between instances, also with a real Streamer.bot).
- [ ] 15+. Later extras, one phase each, order chosen then (see Decisions).

## Open points (decide when the phase starts, ask first)

- Cron trigger: new in Streamer.bot 1.1.0. When the real Streamer.bot is updated, read its type code and
  fields from the metadata, then add it (and a small cron parser).
- OpenSSL costs ~1.9 MB of hidden RSS on Linux (mapped at start, even unused). If that matters, ask
  about rustls or loading TLS lazily.
- Phase 5: imports of old export versions (v3/v10: sub-actions under `actions`, random groups in
  `actionGroups`) need upgrading to the v24 shape. Check how Streamer.bot upgrades them, and ask.
- Phase 3: user data growth (users seen, user globals) inside the one JSON file. If it gets big, ask
  before splitting.
- Phase 10: Kick Pusher app key and chatroom id lookup (kick.com's v2 channel endpoint may sit behind
  Cloudflare). Check, and fall back to the official API ids.
- Phase 13: integrations whose only transport is Streamer.bot's website relay.
