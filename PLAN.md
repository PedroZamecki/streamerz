# Streamerz plan

Streamerz is a light stream automation app in Rust, built like songrequestz and tikstream: one small exe, FLTK window + tray,
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
| Name / repo | `Streamerz`, repo `streamerz`, public GitHub `PedroZamecki/streamerz`, MIT. Same rules and CI as songrequestz (`CLAUDE.md`). |
| Compatibility | Drop-in for Streamer.bot's servers: WebSocket (127.0.0.1:8080, `/`), HTTP (7474), UDP (4242). The WebSocket server is on by default (127.0.0.1:8080, no auth); HTTP and UDP are off by default. GetInfo/Hello report version 1.0.7 (the Streamer.bot API level copied), the instance name (default "streamerz"), and an extra `streamerz` field with its own version. Same requests, events, auth and JSON. songrequestz stays a separate app with no changes; it connects to Streamerz exactly as it does to Streamer.bot. |
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
| OS | Windows + Linux (Wayland, ksni tray). CI releases a Windows exe and the Linux AppImage on every release. |
| Budget | Hidden window, Twitch + OBS connected: < 25 MB RSS and ~0% idle CPU, measured and written in every PR. Streamer.bot uses ~300 MB. |
| Dependencies | The songrequestz set (tokio single-thread runtime, tokio-tungstenite, futures-util, serde, serde_json, sha2, base64, fltk, ksni/zbus, tray-icon/windows-sys, embed-resource), plus **native-tls** (SChannel/OpenSSL, through tokio-tungstenite's feature), **flate2** with miniz_oxide (import/export), serde_json's **preserve_order** (indexmap, so Streamer.bot's JSON keeps its key order), and **rsa** (Kick webhooks only). No webview (wry/tao were removed in 1.1.0: TikTok uses the user's own browser). **image-webp** (WebP emotes and avatars; FLTK decodes the other formats itself). HTTP is a small HTTP/1.1 client written here over tokio + TLS, with keep-alive per host. **tokio-native-tls** (the crate tokio-tungstenite's native-tls feature uses, for HTTPS), **regex-lite** (If/Else, While, Switch and regex commands; .NET's backreferences and lookaround aren't supported). Anything else needs a question first. |
| TikTok | Public LIVE reading is native through `webcast.rs`. The optional extension is the single account for signed search, authenticated reading and sending (`tiktok_bridge.rs`). It uses TikTok's own scripts in an offscreen browser frame, with no visible helper tab, and recovers discarded contexts on demand. Chromium background mode permits use with all browser windows closed. Separate broadcaster and bot logins are retired; reader cookies are requested from the browser only when connecting and remain in memory. |
| Chat commands | Commands keep Streamer.bot's `sources` bitmask (Twitch 1/2/8, YouTube 1024, Kick 2097152, TikTok 4194304), so each command chooses its platforms. Built-in commands (`!commands`, `!command add/edit/remove`, `!command platforms`, `!so`, `!uptime`, `!title`) are normal commands created on first run, editable and disable-able. Replies go to the platform the command came from; a command can opt in to reply on all connected platforms. |
| Not in scope | Meld, PolyPop, Elgato, VTube Studio/Voicemod/Speaker.bot/TITS/Crowd Control/HypeRate/Pulsoid/Lumia/Streamfog/mtion, the Streamer.bot website integration/cloud, C# and plugins. |
| Later (own phases, after phase 14) | Quotes, credits, points, users/first words; sounds and TTS; file I/O, file tail, folder watcher, process triggers; hotkeys/inputs and MIDI; Decks, HTML overlay, multi-platform chat window. |

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
  Public repo with the songrequestz settings (merge commits only, never squash; delete branch on merge, "protect main" ruleset).
- [x] 1. Scaffold:
  - Cargo, single-thread runtime, `streamerz.json` (atomic save + `.bak`), shared status, log.
  - Window and tray skeleton from songrequestz (starts hidden, autostart, dark theme), with the icon.
  - CI: `lint` (rust + commits); `build` with the Windows exe and Linux AppImage on PRs and releases.
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
- [ ] 15. Hotkeys, inputs and MIDI: keyboard simulation, mouse click trigger and MIDI In/Out. Speech recognition and voice control were retired for 1.6.0.
- [ ] 16. Streamer.bot Decks, Stream Deck, HTML overlay and chat window: Stream Deck sub-actions and triggers, Decks item/page controls, and multi-platform chat window action triggers.
- [ ] 17. File I/O, file tail, watcher and process triggers: File sub-actions (write, read lines, read specific/random line, file/folder exists, set file tail state), file watcher triggers, file tail trigger, process started/stopped triggers.
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
  as ordinary editable commands with Streamerz-only `builtin`/`response`/`replyAll` fields; text replies
  for any command; new commands work on every platform.
- [x] 23. TikTok native read (`webcast.rs`, ported from piratetok-live-rs): chat, gifts, likes, follows,
  shares, subscriptions and joins without tikstream or TikFinity; the bridge URL and Start tikstream removed.
- [x] 23b. TikTok window (`tiktok_web.rs`, wry/tao): TikTok's login page, and sending by typing into the
  live page's chat in a hidden window (video paused) that opens on first send and closes after 2 idle
  minutes. Its own `Streamerz-tiktok` binary on Linux, `Streamerz.exe --tiktok` on Windows; tikstream-login
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
  subscriber emotes stay text until a live with them can be checked. Native YouTube emotes are shown in
  received chat but omitted from the picker because the sending API accepts text only.
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
- [x] 27c. StreamElements and Streamlabs alerts over their socket.io APIs (only a webhook nothing sends was
  there); DonorDrive polling.
- [x] 28. MCP: `streamerz --mcp`, a Model Context Protocol server (stdio, no new dependency) driving a running
  instance through its WebSocket: status, log, actions, run an action, list/fire triggers, inject chat and
  read what would have been sent. New dev-only requests (`STREAMERZ_DEV=1`): FireTrigger, GetLog, GetStatus.
  The repo's `.mcp.json` points at a dev instance on 8090. Phase 27b moves after it.
- [x] 29. Chat: animated Twitch emotes (GIF, `gif` crate); click a name for a card with everything known about
  that user on its platform; sending goes to the platform shown; only platforms that are set up show in the
  views, the side-by-side panes and the send list.
- [x] 30. Accounts and pictures: TikTok's profile picture, sharper pictures everywhere, another channel than the
  broadcaster's only under Advanced, and the window coming back as it was left (tab, chat view, size).
- [x] 31. Connections: the status bar counts every connection that's set up (platforms, OBS, Streamlabs
  Desktop, donations...); clicking it shows each one's state (connecting, connected, failed and why) with a
  retry button and a link to its settings.
- [x] 32. Hotkeys: Hotkey Press triggers through the desktop's Global Shortcuts portal on Linux (no sudo, no
  input group: the desktop asks the user) and RegisterHotKey on Windows; "press the keys" to set a
  trigger's shortcut. Changed in 1.4.0: X11 uses the X server for keys and clicks (no setup); Hyprland
  gets its binds from Streamerz through its socket; Wayland clicks (and keys where there's no portal) read
  the devices once the user allows it in the trigger (a udev `uaccess` rule for mice only, or keyboards
  only, added through polkit and removed by the same button). Mouse Click gets a Button filter, set by
  clicking with it.
- [x] Chat emotes follow the channel read (not the logged-in account's) and each chatter's personal 7TV set,
  asked once per chatter (400 kept); 7TV sets come from its GraphQL API (60 KB, not 2.4 MB). BTTV personal
  emotes and FFZ user sets are not covered (no public API for BTTV's).
- [x] 33. Cache: the BTTV/FFZ/7TV emote lists and the accounts' picture URLs kept in `cache/` (used at once,
  fetched again in the background after a day), user card lookups reused for 10 minutes, and the pictures
  kept under 50 MB (least recently shown go first). Faster start was dropped: it's fast enough.
- [x] 34. Fixes: TikTok says when an account has never gone live (TikTok's live service answers "not
  found" for those) and logs a reason once, not every 30 s; chat shows only the platforms connected or with
  chat in the log, and follows connections as they come and go; the emote picker fills its grid after it's
  shown (it was one emote a row on first open).
- [x] 35. Real badges: Twitch's own badge pictures (sharp) instead of drawn role badges, the ones the
  chatter wears, with spacing; chat settings: timestamps on/off, badges on/off. Active emote-only and slow modes
  appear above the composer, with the platform named in combined chat.
- [x] 36. Message actions on hover (Twitch-like): reply, copy, pin (Twitch), delete (Twitch, Kick), timeout and
  ban (Twitch, Kick, YouTube); lines deleted or banned anywhere leave the chat.
- [x] 37. Emote picker like 7TV's: per-platform and per-account provider tabs (Favorites, platform native
  via Twitch Helix `chat/emotes/user` & Kick `/emotes/{slug}`, 7TV, BTTV, FFZ, Emoji), search across tabs,
  collapsible section headers, empty-state notes, and right-click `★` favorites.
- [x] 38. Recent chat on connect: persisted `cache/chat-history.json` plus pre-join platform history
  (Twitch recent-messages, Kick v2 messages, YouTube initial/pre-join live chat, TikTok history comments)
  merged before `"Welcome to the chat room!"`; platform-authentic badges and colours everywhere.
- [x] 39. Per-pane chat boxes & connection self-healing: individual `[Avatar] [Input] [Emotes] [Send]` bar
  under each chat pane with an account selector popup, discrete `"Message all"` popup in the top bar,
  D-Bus keyring retry + background/interactive unlock, Twitch EventSub supervisor, and OBS/Streamlabs-woken
  standby mode for offline YouTube & TikTok channels.
- [x] 40. Quotes, credits, users and first words: a Community tab (users seen in chat, quotes with
  add/edit/delete, the stream's credits); quote sub-actions and Quote Added; GetCredits/TestCredits/
  ClearCredits filled; one First Words cache for every platform (Reset First Words, ClearFirstWordsCache).
- [x] 41. TikTok triggers and sub-actions beyond chat, gifts, likes and follows: Viewer Count Update (with
  the top viewers), Stream Online/Offline; Get Top Gifters and Get Stream Info from per-live tallies (combo
  gifts counted once, when they end); TikTok in GetEvents.
- [x] 42. Visual polish across every tab: flat rounded look in zinc colours (like shadcn/ui) for every
  button, field, list and tab, drawn by two box functions; the open tab joins its page; light theme too.
- [x] 43. More visual work: column titles over the lists, a hint in empty lists and trees, placeholders in
  search and message fields, roomier menus, thinner scrollbars.
- [x] 44. Sidebar and drawn lists: the 16 tabs become a sidebar with five sections (Live, Automation,
  Connections, Data, App) and a Lucide icon per page; lists drawn by hand with roomy rows, a hover highlight
  and a thin scrollbar; the chat scrollbar is flat, without arrows.
- [ ] 45+. Later extras, one phase each, order chosen then (see Decisions).

## Version 1.1.0

- [x] Release notes list every commit since the last tag, grouped by type (not only the pull request).
- [x] Bugs: the chat froze under a user card or any dialog; Community, Accounts and Donations didn't fill
  in; Kick connected twice; the pin and timeout menus always used the first time; a logout, shutdown or
  `kill` looked like a crash (and saved nothing); the Settings file buttons weren't aligned.
- [x] Logs: DEBUG/INFO/WARN/ERROR/FATAL levels in the file and the Log page (colours, level filter, search,
  Debug lines in Settings). A panic logs FATAL and saves the settings and Community before closing.
- [x] Crashes page (read, report on GitHub, delete) and Backups page (a backup at a new version's first
  start, the last 10 kept; back up now, restore, delete).
- [x] User card: banner, account age and picture without a login (Twitch's public web API, kick.com's
  card); badges as pictures, named when pointed at; the chatter's messages live in the chat's own view,
  with its actions (no box to write in); animated emotes (and an option to keep them still). Banners
  without a login for Twitch, Kick (kick.com's channel page) and YouTube (the channel's public page).
- [x] Hover details: pointing at an emote or a badge in any chat shows its picture big, its name, where it
  comes from (the source's icon: 7TV, BTTV, FFZ, the platform) and its id (`ui/tip.rs`).
- [x] Pinned messages over the chat, with who pinned them and the time left (Twitch, Kick, YouTube).
- [x] TikTok without a webview: the user's Chromium browser supplies the account through the extension.
  The older DevTools login driver is retired; wry, tao, WebKitGTK and the helper binary remain removed.
- [x] Accounts: the channel to read is on the page, for every platform; Twitch reads someone else's
  channel too, as a moderator (the subscriptions a moderator can't have are left out).
- [x] Accounts, simpler: a platform connects at start by itself once set up (Advanced turns that off);
  the channel box is a button that opens a search (`search.rs`, `ui/find.rs`: Twitch, Kick and YouTube by
  name with picture, followers and live, no login; a pasted link works too) and is empty while reading
  your own channel; community data belongs to the channel it was seen in.
- [x] Community: every column of the users table sorts (click a title, again to turn it around).
- [x] Crash reports carry their context (steps, facts, the system, a trimmed GitHub issue).
- [x] Security: secrets scan on every commit and PR (`dev/scan_secrets.py`), `cargo audit`, least-privilege
  workflow tokens; see SECURITY.md.
- [x] Fixes: switching the Twitch channel while its subscriptions were being made no longer logs a wall of
  "websocket session has already disconnected" errors (the setup stops with its session).
- [x] Faster dev builds: `debug = "line-tables-only"` in `[profile.dev]` (a 114 MB binary, not 150 MB).
- [x] Polls page: Twitch and YouTube polls, Twitch predictions (Kick and TikTok have no API for them).
- [x] Chat effects listed in [CHAT.md](CHAT.md): subs, gifts, raids, rewards, announcements, Kicks,
  Super Chats, memberships and TikTok gifts as event lines; highlighted and first messages marked.

## Version 1.2.0

- [ ] Chat viewer counts: per platform in separate panes, combined with a platform breakdown on hover.

- [x] Releases are titled with their version only (`v1.2.3`), not the merge commit's message.
- [x] Crash reports: only `crash-*.txt` is offered (the trail file `context.txt` once was, again and again);
  a second window starting doesn't take the first, still running, for a crash.
- [x] Everything printed to stderr (by Streamerz, a library, or a program it starts: the file manager, the
  browser) goes to the log (`stderr:` lines, levelled by what they say) and `logs/stderr.log`, not the
  terminal. Opened folders and links don't inherit a forced `GDK_BACKEND`.
- [x] Roles (`roles.rs`): our role in each chat (anonymous, viewer, VIP, moderator, lead moderator,
  broadcaster). Twitch asks at connect (only a moderator may list the chatters), the others know the owner,
  and our own messages' badges update it. EventSub subscriptions, Helix calls (a sub-action that needs the
  owner or a moderator fails with a clear reason instead of a 403) and the chat's hover actions follow it:
  nobody touches the broadcaster or themselves, moderators leave each other to the owner, viewers only
  reply and copy. Accounts says what we are on someone else's channel, and how to get more.
- [x] Chat without a login: a channel set and no account reads it (Twitch over anonymous IRC, `irc.rs`;
  Kick and YouTube already could); the chat box is off, "Message all" leaves it out, no hover actions.
- [x] Community: the platform column sorts and filters (a funnel lists the platforms in the list); both
  only show with more than one platform. The sort header no longer leaves ghost arrows.
- [x] Sub-action fixes found while documenting: OBS state choices in the order the engine reads them, Set
  Channel Tags (it cleared every tag), Update Redemption Status, Kick reward states, Set Channel Game's
  source, Suspicious Status; Ko-fi and Patreon webhooks are checked once their token or secret is set.
- [x] Reference (`docs/reference`): a page for every trigger and sub-action, from what Streamerz does, built
  in and served by the MCP server (`docs`, `docs_search`, `guide`, and in `describe`); a test fails when
  one is missing.
- [x] Scenarios (`scenario.rs`, `tests/scenarios`, [docs/SCENARIOS.md](docs/SCENARIOS.md)): actions tested
  on the engine alone, no window or login, from `cargo test`, `Streamerz --scenario` and the MCP
  `run_scenario` tool.
- [x] Connections that survive a bad network: YouTube retries a failed read on the same page (growing
  waits, a session is given up only after eight in a row) instead of calling the stream ended and joining
  again; Kick pings after a silent minute and reconnects when nothing answers; Twitch EventSub notices a
  socket that stopped sending keepalives; the anonymous IRC reader pings too. Reconnects wait longer each
  time, and an outage logs one line, not one per try.
- [x] YouTube finds the live by itself from a handle (`@name`) as well as a channel id (it only knew ids),
  looks every 30 s, and reads again right after a message is sent from here. TikTok's sending tab opens
  when its chat box gets the focus. TikTok's "stream started" line no longer repeats.
- [x] Twitch chat commands (`slash.rs`, [docs/CHAT.md](docs/CHAT.md)): `/commercial`, `/timeout`, `/slow`,
  `/followers`, `/raid`, `/announce`... typed in the chat box are the Helix calls they stand for, with a
  menu of presets next to the emote button (Twitch's chat only).
- [x] Chat: `/me` lines in italics in the sender's colour, and an `@name` of one of our accounts on a chip
  (light and dark).

- [x] Chat page: no drawn stand-in badges on Twitch (the real pictures come from Twitch's public badge
  service, so a chat read without a login has them too), tooltips that say what a badge or emote is, our
  role known at connect (Kick asks, history says the rest), commands listed in the box as `/` is typed
  (the button is gone), poll and prediction cards and a redone pin card, channel reward lines with their
  price, sent messages shown at once, a desktop notification when a stream ends, and a compact Reconnect
  and Hide chat message when chat is disconnected (Options puts a chat back). Stream end and chat
  connectivity are independent.
- [x] Detected lists (`catalog.rs`): a channel's rewards (Twitch, Kick) and OBS's scenes and sources are
  offered in the editors, and a name can be typed when nothing was found. Reward triggers can name their
  reward. Sub-actions take a reward by name or id.

- [x] No assumed permissions: the role tables that decided which Twitch calls and events were tried are gone;
  Twitch answers, and its answer is shown (refused events are told once). Our role is asked at connect
  (Twitch's moderator list, Kick's user endpoint). Commercials are only offered to affiliates and partners
  (read from the channel). Kick's chat box lists the commands its API can do. Twitch badge pictures come
  from Twitch's public web API and nothing is drawn in their place.

- [x] Rewards and power-ups in the chat ([CHAT.md](docs/CHAT.md)): a message sent with a custom reward has
  it over it (picture, name, price in the channel's own points picture), without a login too (the reward
  list comes from Twitch's public web API, the reward id from the message); automatic rewards and Bits
  power-ups (custom ones fire now) are lines with their picture and price, a gigantified emote is drawn
  big, watch streaks fire and show (EventSub and the anonymous reader's notice), Kick rewards show their
  price (kick.com's reward list) and colour, TikTok gifts their picture and coins. The anonymous reader
  marks highlighted, sub-only, gigantified and animated messages.

## Version 1.4.0

- [ ] Introduce testable boundaries for the features in this version, with Linux and Windows
  platform tests. Migrate existing code incrementally.
- [x] Provide reusable groups containing actions, commands, timers and files, with named variables and
  editable configuration shared by their items. Users can create, edit, export, import and configure
  groups in the UI. Use group variables for shared configuration instead of template-specific configuration packages.
  Keep Streamer.bot exports compatible. Groups can be created, renamed and deleted, and have optional
  embedded PNG or JPEG icons addressed by SHA-256. Include icons and group variables in exports; merge
  imported variables by name. Expose group management, icons and import/export through MCP.
- [ ] Introduce testable service boundaries for OBS and Spotify so their connectors can be replaced
  without changing action behavior.
- [x] Create a configurable subathon example using the general group tools, with an HTML overlay receiving timer updates from
  Streamerz. Twitch subscriptions and Streamlabs donations add configurable time per subscription and currency
  unit in one configured currency. Cap remaining time at the configured maximum; start on installation,
  provide a manual reset, and pause the countdown while Streamerz is closed.
- [x] Add an opt-in Spotify connection to read what is playing and add tracks to the Spotify queue.
  Song requests accept song names or Spotify links. Streamerz does not play Spotify audio. Use an
  explicitly marked Streamerz-specific action for the song request template; it does not run in
  Streamer.bot and must be preserved on export.
- [x] Improve Spotify connection reliability and make song requests easy to build with Streamerz's
  actions and templates. Use existing commands and triggers for permissions and cooldowns, and expose
  Spotify operations through actions and MCP. Keep the integration opt-in without unnecessary polling.
  Song requests follow Songify (https://github.com/songify-rocks/Songify): a group with settings
  (who can request, limits, blocks, skipping, replies), commands for `!sr`, `!song`, `!next`, `!queue`,
  `!pos`, `!remove`, `!skip`, `!voteskip`, `!play`, `!pause`, `!vol`, `!songlike` and `!bansong`, and the
  Spotify icon. A request goes to Spotify's queue at once; a request taken back is skipped when it plays.
- [ ] Expand Spotify triggers and sub-actions, with Spotify under Integrations and countdown actions
  under Core/Timers. On Windows, read local Spotify media sessions without an API login and make the
  local-only limitations clear. API-dependent operations require an authorized account.
- [x] An MCP page (App section) listing the AI agents installed here (Claude Code, Codex, OpenCode, Gemini
  CLI, Antigravity, Cursor, Windsurf, VS Code, Copilot CLI, Claude Desktop, Qwen Code, Kiro, Crush, Amp,
  LM Studio), found by their command or folder. Adding Streamerz writes its MCP server into the agent's own
  config file (nothing else in it changes; a file it can't read is left alone); a development build adds
  `streamerz-dev` with its address. Open chat starts a terminal agent in the user's terminal (no terminal
  inside Streamerz) in a folder with notes for it. Read only when the page shows; MCP tools `agents`
  and `agent_setup`.
- [x] Overlays: an overlay is an `.html` file in an action group (so it exports and imports with the group),
  served on the HTTP server's port at `/overlays/<group>/<file>` (index at `/overlays`), with Streamerz's
  script added to the page: `streamerz.settings` (the group's settings), `streamerz.on(topic)` and
  `streamerz.event("Source.Type")`, fed by server-sent events on the same port (no new port, thread or
  timer; a page that asks for nothing connects to nothing). Built-in topics `viewers` (per platform and
  total; polled only while a page listens) and `ad` (Twitch ad breaks); any other topic by the Streamerz-only
  Overlay sub-action (99880) or the MCP tools `overlays` and `overlay_publish`. Files dialog: Copy URL and
  Save as page (a copy with the script inside, for a local file in OBS). Templates: Ad break overlay and
  Viewer counter overlay. See `docs/OVERLAYS.md`.

## Development isolation

- [x] Production always uses the global configuration and permits only one running production instance.
  Use an OS-held lock released on exit or crash, independent of network ports. Development instances do
  not take this lock. Migrate existing chosen or portable configurations without losing data.
- [x] Each development instance clones the global configuration into its own private OS temporary
  directory, including the related data needed to use it. Never write production files or keychain items.
  Keep credentials usable without changing production secrets; do not log or commit them. Clean up on
  normal exit and remove stale directories safely after crashes. Fail closed if isolation cannot be set up.
- [x] Development builds identify themselves with the Git commit hash and a dirty marker, use the DEV
  logo, and report their temporary configuration path. Releases retain their release version and logo.
- [x] Disable automatic live connections and actions in the development clone. Allow multiple development
  instances with separate local ports and identifiers, without changing production settings. Make deliberate
  live testing possible from the development instance.
- [x] Cover isolation, concurrent instances, cleanup, production locking, and version identification
  with tests. Check the DEV UI at narrow and wide sizes and run the smoke test.
- [ ] Verify migrated and development logins with the real keyring, and test the workflow on Windows.

## Feature audit

- [ ] Review [the feature audit and proposed sequence](docs/FEATURE-AUDIT.md), verify its findings against
  the implementation and real services, and agree priorities before adding implementation phases.

## Memory investigation and storage

- [x] Add an opt-in aggressive Memory Saver policy, applied after restart. Disable Community tracking
  and loading, credits, retained chat, recent chatter facts in RAM, downloaded images and emote catalogs.
  Preserve existing Community files untouched. Keep commands, connections and triggers enabled.
- [x] Keep Kick random users exact for the session with chat history off or Memory Saver on. Store
  Kick identities in session-only disk buckets and clear them on exit and the next start.
- [x] Keep exact First Words behavior using session-only disk buckets in Memory Saver. Do not evict
  speakers and accidentally repeat First Words. Reset by platform or globally, and clear on clean exit.
- [ ] Measure this mode with First Words under sustained 1,000 messages/second and a 25,000-message
  burst, verify restart and file preservation, and document the remaining obstacles to a 100 MB ceiling.
  [Current traffic measurements](docs/MEMORY-SAVER.md); long interactive image and card testing remains.

- [x] Compare memory strategies individually and together using measured RSS, CPU time, latency,
  write volume and restart integrity. Prefer lower RAM when extra processing helps. Target sustained
  1,000 messages/second and test an unthrottled 25,000-message burst; report whether 1 to 2% of one CPU
  core is met, rather than assuming it. Keep all user records and statistics.
  [Results and applied changes](docs/MEMORY-EXPERIMENTS.md): compact records and borrowed streaming
  saves selected; record-level compression rejected. The one-core CPU target remains unmet.

- [x] Measure synthetic chat growth and reclamation with native process metrics and allocation traces,
  separate repeated users from distinct users, and document cache ownership and cleanup in
  [docs/MEMORY.md](docs/MEMORY.md). Add a reproducible isolated development harness.
- [x] Retention decision: keep all Community records and statistics across restarts. Reduce the resident
  working set instead of automatically deleting inactive users.
- [x] Remove full Community save copies and linear identity lookup. Index credits without duplicating
  names, avoid cloning unmatched commands, and retain the JSON format, save frequency and unknown fields.
- [x] Transfer only displayed Community columns to the UI, release rows on page leave and hide, and
  check refill, cards, sorting, filtering, narrow/wide screenshots and the smoke test.
- [ ] Reduce the remaining processing and full-snapshot write volume. The sustained 60,000-message
  test delivers 1,000 messages/second with stable RAM, but uses 37.08% of one core and issues about
  6 GB of logical writes. Do not weaken save guarantees or drop events to hide this cost.
- [ ] Agree indexed disk storage and migration, then load historical Community records on demand.
  Compare compression, CPU, allocated disk space and RAM before selecting the format.
- [x] Prevent queued refreshes and late image results from refilling hidden presentation storage.
  Test stale image rejection and ready-pixel cleanup; repeat the full smoke regression.
- [ ] Bound image pipeline bytes and pending work, review other cache lifetimes, and verify reclamation
  after expiration, hide, disconnect and cancellation with realistic image and action workloads.
- [ ] Complete Windows and real Twitch + OBS profiling. Linux release-profile synthetic, restart,
  sustained SSD, allocation, and UI reclamation comparisons are recorded in the experiment report.

## Version 1.5.0

- [x] Twitch standalone GIF messages: show the animated picture below the sender in live chat and history,
  with the title as fallback.
- [x] Active emote-only and slow modes above the composer, with the platform named in combined chat.
- [x] TikTok sending through the user's own browser. The Streamerz for TikTok extension
  (`assets/tiktok-extension`, loaded unpacked from the folder Streamerz writes next to `streamerz.json`)
  connects to the WebSocket server on `/streamerz/tiktok`: only its fixed extension ID and the pairing
  token are accepted. The route does not exist before setup. Chat, SendMessage and sub-action 43001 use
  the browser account. MCP tool `tiktok_bridge` manages setup and login.
- [x] TikTok convenience: replace separate logins with the extension's account, preserve the chosen
  channel during migration, and use browser session cookies only in memory for authenticated reading.
  Search channels through the extension from the picker and MCP `search_channels`. Use an offscreen
  frame instead of a pinned tab, recover discarded contexts, distinguish loading, ready and login states,
  and never automatically retry a send with an ambiguous outcome. Keep public reading independent.


- [x] TikTok account audit: one account in the chat popup, profile pictures across Accounts and chat,
  signed channel-picture lookup, handle-based mention replies and suggestions, and TikTok in Message all.
  Retire obsolete account-selection form fields and profile migration; preserve imported action data.
  Hide internal sticker IDs in tips and fallback text, and prevent unsupported native sticker sends.
  Refresh account pictures on change without continuously redrawing an idle popup.

- [x] TikTok capability audit: receive message deletions, user-history deletions, pins and pin
  cancellations; report pause/resume without dropping chat. Expose observed state and development
  event injection through WebSocket requests. Recover room lookup through the browser after a public
  lookup failure, accept pasted LIVE links, and reject oversized messages before clearing the draft.
  Reuse the browser device cookie, recheck signing state when reopening setup after a transient error,
  and never replay a send. Keep validation notices readable when the window is resized.

- [x] Repair TikTok browser setup: reuse the existing browser profile and window, keep the
  dialog open, preserve development pairing in a stable cache, and open login when required.
  Verify running-browser reuse, cold start, pairing, login and narrow/wide layouts with
  `dev/tiktok-setup.py`. Browser-controlled extension installation remains a manual step.

## UI and UX review

The agreed scope is to fix the issues cited in the UI/UX reviews and verify the affected screens.
There is no per-page score threshold or repeated scoring requirement. Track the finite checklist in
[docs/UI-AUDIT.md](docs/UI-AUDIT.md); scores from earlier reviews are historical context only.

- [x] Inventory pages and major dialogs using independent code and visual reviews.
- [x] Complete the cited shared-control, page-layout, state, accessibility and workflow fixes.
- [x] Verify affected pages at narrow and wide sizes, including relevant populated, unavailable,
  error, context-menu, cancellation and validation cases. Cover fixed bugs with regression checks.
- [x] Run the smoke suite, formatting, clippy and relevant tests. Record comparable hidden memory
  and idle CPU measurements, and state any live-service or Windows verification limits plainly.
- [x] Commit locally and report the completed checklist. Keep the version open until asked to close it.

## MCP verification and usability

- [x] Check every advertised tool's schema and dispatch, and repair missing routes.
- [x] Validate arguments, bound connection waits and report protocol errors.
- [x] Exercise all 51 tools through stdio on an isolated dev instance, including
  authoring round trips, exports, group settings and unavailable integrations.
- [x] Document setup, offline testing, replacement semantics and service limits in docs/MCP.md.
- [ ] Before closing the version, test configured live-service success paths and record
  the required hidden RSS and idle CPU measurements.

## Trigger configuration and documentation

- [x] Inspect the misleading trigger editor hint, built-in reference access and TikTok gift
  configuration/catalog pipeline. Findings and proposed sequence are in
  [docs/TRIGGER-DOCUMENTATION-AUDIT.md](docs/TRIGGER-DOCUMENTATION-AUDIT.md).
- [x] Inventory every registered trigger and sub-action, review shared authoring and execution
  paths, and organize simple user-facing ideas by family. The proposed packages and acceptance
  workflows are in [docs/AUTOMATION-IDEAS.md](docs/AUTOMATION-IDEAS.md), with a reproducible
  [per-type inventory](docs/AUTOMATION-INVENTORY.csv).
- [x] Deliver honest picker availability and shared in-app Help with explicit form states.
  Every reference has Fields and Arguments, plus When for triggers, checked by a regression test.
- [x] Add reusable identity-preserving selectors, starting with dynamic TikTok gifts.
  Discovery uses the current room and paired browser; channel/account changes invalidate the list.
- [x] Simplify supported forms with task-specific fields, human units and value insertion.
  Keep imported values and unknown fields intact; hide options with no runtime effect.
- [x] Share live-event matching with previews and report actual step outcomes.
  Gift previews use the live matcher; history distinguishes reported failures and skipped steps.
- [x] Build the selected workflows as editable recipes using the general group/template tools.
  Thank TikTok gifts and Reward shows an OBS source start disabled until configured.
- [x] Use completion delivery for new gift triggers and retain legacy update delivery when absent.
  Select one exact gift ID or any gift. Inclusive coin bounds use cumulative quantity times unit
  value; zero means unbounded. Separate triggers remain alternatives and can queue separate runs.
- [ ] Before closing the version, verify gift discovery and matching on a real live, native Windows
  behavior, and hidden Twitch + OBS RSS and idle CPU. Fixture and isolated UI checks are recorded in
  [docs/TESTING.md](docs/TESTING.md); disconnected measurements do not establish the connected budget.

## Open points (decide when the phase starts, ask first)

- Points: its sub-actions are new in Streamer.bot 1.1.0, unreleased (1.0.7 is the latest). When it's out,
  read the type codes and the point definitions' format from it, then add them.

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

## Windows and notification improvements

- [ ] Bring the existing production window forward on a second launch, on Linux and Windows.
- [ ] Add a notification dialog for login expiry, updates, stream endings and automation toasts, enabled by default.
- [ ] Investigate free trusted signing and SmartScreen reputation for Windows releases.

- [ ] Prepare SignPath Foundation onboarding, Windows metadata and a verified signed artifact flow that preserves build reuse.

## Version 1.6.0: review and cleanup

- [x] Carry pending work onto one version branch, without pushing or opening a PR.
- [x] Require local CI/CD verification before pushing, with a report of unavailable checks.
- [x] Review pending changes and remove only material proven unused.
- [x] Share repeated logic without changing behavior or adding runtime work.
- [x] Run local CI checks, scenarios, packaging and hidden-display smoke (limits in docs/REVIEW_1.6.0.md).
- [x] Record removal evidence, validation results and remaining platform/service limits.
- [ ] Keep the version open for further requested changes.

Version 1.6.0 decisions: speech and voice control are retired. Browser discovery and extension-store
publishing stay in scope. Unmapped sub-actions keep the JSON editor until their form fields have
explicit types; never guess widget types from field names.

Protected-folder updates retain elevation, with private installation staging and encoded Windows
commands. Packed Windows file notifications are parsed without assuming four-byte record alignment.
The version remains open; release blockers are tracked in `docs/REVIEW_1.6.0.md`.
