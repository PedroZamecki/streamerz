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
| Compatibility | Drop-in for Streamer.bot's servers: WebSocket (127.0.0.1:8080, `/`), HTTP (7474), UDP (4242). The WebSocket server is on by default (127.0.0.1:8080, no auth); HTTP and UDP are off by default. GetInfo/Hello report version 1.0.7 (the Streamer.bot API level copied), the instance name (default "streamerz"), and an extra `streamerz` field with its own version. Same requests, events, auth and JSON. songrequestz stays a separate app with no changes; it connects to streamerz exactly as it does to Streamer.bot. |
| Data model | Streamer.bot's own model and field names (`actions.json` v24: actions, `subActions` with numeric `type`, triggers, queues, groups, commands, timers, globals). Import Streamer.bot export strings from day one. Export works too, so actions can go back to Streamer.bot. |
| Type codes | Built from Streamer.bot.exe's .NET metadata (sub-action/trigger enums and their property names), read with a decompiler, for interoperability only. Only the code↔name/field mapping goes into the repo; no Streamer.bot code or data. |
| Unsupported on import | Kept untouched (so a re-export loses nothing), shown disabled, logged once, skipped at run. This applies to C# code, out-of-scope integrations and the later extras. |
| Scripting | No C#. The "Run a Program" sub-action is extended: it runs any exe/script, passes the arguments as JSON on stdin, and reads `key=value` lines or a JSON object from stdout back into arguments. No embedded interpreter. |
| Variables | Full Streamer.bot parity: `%arg%`, `~global~`, persisted and temporary globals, user globals, inline functions (`$math()$` style, same names and syntax as the docs), same argument names per trigger. `$math()$` covers a common subset of mXparser: numbers, `+ - * / ^ %`, parentheses, comparisons, `& | ~`, abs round floor ceil min max sqrt exp ln log log10 sin cos tan asin acos atan sign rand/rnd, pi and e; anything else gives NaN with a log line, as Streamer.bot does. `%x:format%` follows .NET format strings in the OS locale (glibc locale data on Linux, Windows' own on Windows). |
| Editor | Native FLTK window + tray (starts hidden), with the same tabs and panes as Streamer.bot: Actions (groups tree, sub-actions list, triggers list), Commands, Platforms, Stream Apps, Integrations, Servers/Clients, Settings, Log. Plain FLTK look with the dark theme. Tools: action history (last N runs with their args, re-run), pending queue view (running/queued per queue, clear), test trigger (fire any trigger with sample args). |
| Storage | One `streamerz.json` next to the exe: settings, actions, commands, timers, queues, globals, user data and tokens. Gitignored. Written atomically (temp file + rename) with a `.bak` of the previous version. |
| Twitch | EventSub WebSocket for chat and every event, Helix REST for sub-actions, no IRC. Broadcaster account plus an optional bot account (chat is sent as the bot when set, as in Streamer.bot). Device-code OAuth with our public client ID, so there's no secret in the exe. |
| Kick | Read through Kick's public Pusher websocket (what kick.com uses): chat, follows, subs, gifts and more, with no login. The official API with OAuth is used to send chat and moderate. That needs the **user's own Kick app** (client ID + secret pasted in Settings), because Kick requires a secret even for refresh. Official webhooks are optional, through a user-run tunnel to the local HTTP server, with signatures verified (RSA-SHA256, Kick's public key). |
| YouTube | Read chat and events through InnerTube (what the youtube.com page uses): no quota, no login. The official Data API with Google OAuth, through **the user's own Google client** (Desktop app, pasted in Accounts > YouTube > Advanced, like Kick: nothing shared is shipped) is used to send messages, moderate and set the title/description. If InnerTube breaks, the fallback is the official gRPC `liveChatMessages.streamList`, which Streamer.bot itself uses. |
| OAuth apps | Only Twitch has a shared app: a public client ID with no secret (device code). Kick and YouTube use the user's own app, so no secret is ever in the repo or the exe. |
| Security | Tokens and cookies live in the OS keychain (Windows DPAPI, Linux Secret Service; `streamerz.json` with 0600 when there is none), are revoked on logout and never logged. The WebSocket and HTTP servers refuse browser origins other than local ones (`null`, `file://`, localhost) and an allowlist in Servers; clients without an Origin (apps) are unchanged. Importing actions that run programs asks first. Minimum scopes per account. |
| Stream apps | OBS Studio (obs-websocket v5) and Streamlabs Desktop (its websocket API). Meld Studio and PolyPop are not supported. |
| Integrations | Opt-in and off by default; a disabled one opens nothing. In scope: donations/shops (Ko-fi, StreamElements, Streamlabs, Patreon, Fourthwall, Throne, Tipeee, Treatstream, DonorDrive, Pally, Shopify, Streamloots) and webhooks/remote (Discord, IFTTT, pronouns, Streamer.bot remote instances). |
| OS | Windows + Linux (Wayland, ksni tray). CI releases a Windows exe and a Linux binary on every release. |
| Budget | Hidden window, Twitch + OBS connected: < 25 MB RSS and ~0% idle CPU, measured and written in every PR. Streamer.bot uses ~300 MB. |
| Dependencies | The songrequestz set (tokio single-thread runtime, tokio-tungstenite, futures-util, serde, serde_json, sha2, base64, fltk, ksni/zbus, tray-icon/windows-sys, embed-resource), plus **native-tls** (SChannel/OpenSSL, through tokio-tungstenite's feature), **flate2** with miniz_oxide (import/export), serde_json's **preserve_order** (indexmap, so Streamer.bot's JSON keeps its key order), and **rsa** (Kick webhooks only), **wry** + **tao** (the TikTok window, child process only). **image-webp** (WebP emotes and avatars; FLTK decodes the other formats itself). HTTP is a small HTTP/1.1 client written here over tokio + TLS, with keep-alive per host. **tokio-native-tls** (the crate tokio-tungstenite's native-tls feature uses, for HTTPS), **regex-lite** (If/Else, While, Switch and regex commands; .NET's backreferences and lookaround aren't supported). Anything else needs a question first. |
| TikTok | Read natively, no bridge app: `webcast.rs` ports the part of piratetok-live-rs (0BSD) streamerz uses (ttwid cookie, room lookup, WebSocket, ~10 protobuf messages) onto the HTTP/WebSocket code and flate2 already here, instead of vendoring it with reqwest, prost and tracing. Send through streamerz's own TikTok window (OS webview via wry/tao, a child process: `streamerz-tiktok` on Linux so streamerz never loads WebKitGTK, `streamerz.exe --tiktok` on Windows) typing into the logged-in live page's chat, since TikTok signs its chat requests; it runs only around sends (~1 GB while open, closes after 2 idle minutes). Its login replaces tikstream-login. Broadcaster + bot accounts like the other platforms. |
| Chat commands | Commands keep Streamer.bot's `sources` bitmask (Twitch 1/2/8, YouTube 1024, Kick 2097152, TikTok 4194304), so each command chooses its platforms. Built-in commands (`!commands`, `!command add/edit/remove`, `!command platforms`, `!so`, `!uptime`, `!title`) are normal commands created on first run, editable and disable-able. Replies go to the platform the command came from; a command can opt in to reply on all connected platforms. |
| Not in scope | Meld, PolyPop, Elgato, VTube Studio/Voicemod/Speaker.bot/TITS/Crowd Control/HypeRate/Pulsoid/Lumia/Streamfog/mtion, the Streamer.bot website integration/cloud, C# and plugins. |
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
- Websocket API `ws://127.0.0.1:59650/api/websocket` (plain JSON-RPC text frames, no SockJS framing), token auth, JSON-RPC to the
  services (scenes, sources, streaming, recording, replay buffer).

## Setup (repo owner, once, before the phases that need it)

- Twitch (phase 6): dev.twitch.tv → Register application. Client type **Public**, redirect
  `http://localhost`, category "Application Integration". Enable Device Code Grant. Put the client ID in
  the code (it's public).
- Google: nothing for the owner (each user makes a Desktop OAuth client; README has the steps).
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
- [x] 4a. Servers:
  - WebSocket (requests, auth, events), HTTP and UDP drop-ins. Events are emitted as the engine fires
    them; the platforms' own events come with their phases.
  - Answers match the real Streamer.bot's byte-for-byte where it can be compared (captured from it).
  - On 8090, songrequestz and real Streamer.bot clients must work unchanged.
- [x] 4b. WebSocket clients and custom servers, with their triggers. Tick the rest of COVERAGE "Phase 4".
- [x] 5a. Editor, actions: the Actions tab (groups tree, drag-reorder, sub-action list with enable/disable
  and nesting for logic, triggers list), import/export dialogs, test trigger. Sub-actions and triggers
  without their own form open as editable JSON (validated on save).
- [x] 5b. Editor, forms: a form for every core sub-action and trigger.
- [x] 5c. Editor, the rest: Commands, timers, queues, globals viewer, server settings, history and
  pending views.
- [x] 6. Twitch A:
  - Login (broadcaster + bot) and the EventSub session.
  - Chat in and out: messages, whispers, announcements.
  - Follows, subs, gifts, cheers, raids, first words, stream online/offline.
  - Their sub-actions and forms.
- [x] 7. Twitch B:
  - Channel points rewards (create/update/redemptions), power-ups.
  - Polls, predictions, goals, hype train, community goals.
  - Channel info (title, category, tags), clips, markers, ads.
- [x] 8. Twitch C:
  - Moderation (bans, timeouts, unban requests, AutoMod, shield mode, warnings, chat settings).
  - Shared chat, Guest Star, user groups.
  - Third-party emotes and the rest of the Twitch list. COVERAGE Twitch complete.
- [x] 9. OBS Studio: connection settings, every sub-action (including raw/batch) and trigger.
- [x] 10. Kick:
  - Pusher read, and user-app OAuth for send/moderation.
  - Optional webhooks with signature check.
  - Every trigger and sub-action.
- [x] 11. YouTube:
  - InnerTube chat and events (messages, Super Chats/Stickers, memberships, gifts, polls…).
  - Google OAuth to send/moderate and set the title/description; viewer counts.
  - Every trigger and sub-action.
- [x] 12. Streamlabs Desktop: every sub-action and trigger.
- [x] 13. Donations and shops: one integration per commit, each opt-in. Check each one's transport first
  (socket, REST or webhook through the same tunnel as Kick) and ask if one needs Streamer.bot's cloud.
- [x] 14. Webhooks and remote: Discord webhook, IFTTT, pronouns (alejo.io), Streamer.bot remote instances
  (signals/triggers between instances, also with a real Streamer.bot).
- [x] 15. Hotkeys, inputs, MIDI and voice control: keyboard simulation, mouse click trigger, MIDI In/Out, Speech to Text triggers and voice control sub-actions.
- [x] 16. Streamer.bot Decks, Stream Deck, HTML overlay and chat window: Stream Deck sub-actions and triggers, Decks item/page controls, and multi-platform chat window action triggers.
- [x] 17. File I/O, file tail, watcher and process triggers: File sub-actions (write, read lines, read specific/random line, file/folder exists, set file tail state), file watcher triggers, file tail trigger, process started/stopped triggers.
- [x] 18. Command sources and test hooks: fix Kick's (2097152) and TikTok's (4194304) command source bits
  and the TikTok checkbox, and run TikTok Send Message (43001, never routed). `InjectChat` and `GetChatOut`
  WebSocket requests (only with `STREAMERZ_DEV=1`) test every platform's commands without live channels.
- [x] 19. Accounts tab: one row per platform (status, channel, Connect, Broadcaster/Bot Login), details
  under "Advanced". Google loopback OAuth for YouTube, Kick PKCE with the user's app, tikstream-login for
  TikTok, tokens refreshed before use. Replaces the four platform tabs. Kick's rotated Pusher key fixed.
  YouTube logs in with the user's own Google client.
- [x] 20. Security (see Decisions): keychain storage, revoke on logout, origin check and allowlist, token
  redaction, import warning for Run a Program, minimum scopes. Linux keeps one random key in the Secret
  Service and encrypts with HMAC-SHA256 (counter mode, encrypt-then-MAC; sha2 only, no new crate).
- [x] 21. Chat tab: every platform's chat together, side by side or alone; one send box (a platform or
  All connected, as bot or broadcaster) with emoji/emotes converted per platform. WebSocket SendMessage
  sends to any platform. Fixed on the way: YouTube read (InnerTube needs the chat page's continuation and
  the channel's live video), YouTube send (live chat id, not video id), YouTube auto-connect, Kick channel
  change keeping the old chatroom, one shared TLS setup instead of one per request.
- [x] 22. Built-in commands (`!commands`, `!command add/edit/remove/platforms/show`, `!so`, `!uptime`, `!title`)
  as ordinary editable commands with streamerz-only `builtin`/`response`/`replyAll` fields; text replies
  for any command; new commands work on every platform.
- [x] 23. TikTok native read (`webcast.rs`, ported from piratetok-live-rs): chat, gifts, likes, follows,
  shares, subscriptions and joins without tikstream or TikFinity; the bridge URL and Start tikstream removed.
- [x] 23b. TikTok window (`tiktok_web.rs`, wry/tao): TikTok's login page, and sending by typing into the
  live page's chat in a hidden window (video paused) that opens on first send and closes after 2 idle
  minutes. Its own `streamerz-tiktok` binary on Linux, `streamerz.exe --tiktok` on Windows; tikstream-login
  no longer needed.
- [x] 24. Images and icons: FLTK's own PNG/JPEG/GIF/SVG plus `image-webp`; web pictures fetched on the engine's
  thread into `cache/` next to `streamerz.json`, decoded only while the window shows them and freed when it hides.
  Platform and app logos (Simple Icons, CC0) on the section titles; profile pictures in Accounts (Twitch, Kick
  and YouTube from their APIs, TikTok seen at login, or its live lookup). The status bar shows "2/4 connected"
  (each platform, and OBS/Streamlabs when set up, in its tooltip; a click opens Accounts).
- [x] 25. Emotes in chat: a chat view that draws text, emote pictures, platform logos, role badges and name
  colours. Twitch, Kick and YouTube emotes, the channel's BTTV, FFZ and 7TV ones (7TV on Kick too), Unicode
  emoji as Twemoji pictures; animated WebP moves only while it's on screen (one 50 ms timeout, re-armed by
  drawing). The picker shows pictures: emotes seen, third-party ones, every emoji by group. GIFs show their
  first frame (FLTK's animated GIF doesn't give out frames; BTTV/FFZ/7TV are asked for WebP); TikTok's
  subscriber emotes stay text until a live with them can be checked.
- [x] 26. Ready-made actions: Actions > Templates (nothing created by itself), in `templates.rs`: welcome first
  chatters, thank follows, subscriptions, gifted subs, cheers/Super Chats, TikTok gifts, raid welcome and
  shoutout, OBS scene on raid, socials and hydrate timers, `!lurk` and `!discord`. Ordinary actions (one per
  platform where they send differently), commands and timers in a "Templates" group.
- [x] 27a. Twitch triggers: about 45 triggers that never fired now do (Hype Train Level Up, Pyramids, AutoMod,
  blocked/permitted terms, unban requests, warnings, suspicious users, automatic rewards, power-ups, Prime/gift
  upgrades, Pay It Forward, Bits badge tier, shared chat notices/bans/deletes, Viewer Count, Present Viewers,
  Upcoming Ad, Sub Counter Rollover, BTTV/7TV emote changes), with every documented variable. Subs, resubs,
  gift bombs, raids and cheers fired twice (chat notifications and their own subscriptions): now once. Chat
  mode triggers fire only on a change. Polled ones (viewers, present viewers, next ad) poll only while live
  and an action uses them; BTTV/7TV sockets open only for their triggers. New scopes: blocked terms, moderators,
  VIPs (log in again). Not deliverable without IRC/PubSub: Watch Streak, Modiversary, community goals.
- [x] 27b. YouTube, Kick, OBS: YouTube's Broadcast Started/Ended, Statistics Updated, Present Viewers, New
  Subscriber, Sponsor Only, Poll Updated and BTTV/7TV emote triggers fire, and every item has the documented
  YouTubeUser/Broadcaster/Broadcast variables (publishedAt, profile picture, input#, emotes/parts, Super Chat
  amount in micros and currency, member months and level, poll options). Kick follows the payloads recorded
  from kick.com's Pusher (bans came with an object user, so names were blank), with KickChat/KickUser
  variables, recipients, Kicks, stream and category changes; Viewer Count and Present Viewers poll only when
  used. OBS: obs.* variables, old scene, and Streaming/Recording Stopped no longer fire when an output starts.
- [ ] 27c. StreamElements and Streamlabs alerts over their socket.io APIs (only a webhook nothing sends was
  there); DonorDrive polling.
- [x] 28. MCP: `streamerz --mcp`, a Model Context Protocol server (stdio, no new dependency) driving a running
  instance through its WebSocket: status, log, actions, run an action, list/fire triggers, inject chat and
  read what would have been sent. New dev-only requests (`STREAMERZ_DEV=1`): FireTrigger, GetLog, GetStatus.
  The repo's `.mcp.json` points at a dev instance on 8090. Phase 27b moves after it.
- [ ] 29. Points: Streamer.bot's Points sub-actions and triggers.
- [ ] 30. Quotes, credits, users and first words.
- [ ] 31. TikTok triggers and sub-actions beyond chat, gifts, likes and follows (viewers, top gifters, live
  start/end and more).
- [ ] 32. Visual polish across every tab: layout, spacing, status colours, icons.
- [ ] 33+. Later extras, one phase each, order chosen then (see Decisions).

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
