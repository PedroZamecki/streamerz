# Stream page: what each platform allows

A study of the Stream page (`src/core/stream.rs`), platform by platform: what the platform lets an app
read and change, how fresh it can be and what it costs, what the page did before 1.9.0, and what changed.
The platform answers below were read on 2026-11-15 from the owner's own channels (all off air) with
`platform_api` (MCP) and `dev/stream-check.mjs`; what needs a live is marked "to verify live".

## What was wrong (before 1.9.0)

- No card said whether the platform was live, since when, or how many were watching.
- Off air, Twitch's ad line read "Ready. 0 snoozes left." (Twitch keeps the schedule at zero then), and
  Run ad, Snooze, Add marker and Create clip were all enabled: each fails off air. YouTube's and Kick's
  Run ad were enabled off air too.
- The page refreshed only when something else redrew it, and asked again at most every 30 seconds.
  A title or category changed in Twitch's or Kick's dashboard didn't show until then, though both
  platforms already push that change to Streamerz (EventSub `channel.update`, Pusher `LivestreamUpdated`).
- Platforms were read one after another, and a multi-platform change ran one platform after another.
- YouTube's title came from downloading the whole watch page (around 1 MB of HTML) on every read. Its
  description and tags were never read back ("it costs quota": a read costs 1 unit of 10,000 a day), so
  the form started empty and the category and privacy weren't shown. The ready broadcast on the test
  channel was **private**, which nothing on the page said.
- Kick's tags were read from a field Kick's channel answer doesn't have (`custom_tags` is on the live).
  Kick's ad status answered 401 (a login from before the ads permission) and the page hid it silently.
- TikTok's card said "TikTok doesn't share its title", but the room lookup the chat makes says it.
- Saving the all-platforms form sent every prefilled value again to every platform, including
  YouTube's 50-unit `videos.update`, even when nothing changed there.

## Twitch (Helix and EventSub)

| | How | Cost, freshness |
|---|---|---|
| Title, category, tags, language, content labels, branded content, delay | `GET channels` | One request; pushed by EventSub `channel.update` (title, category, language, labels) |
| Live, started at, viewers, thumbnail | `GET streams` (empty off air) | One request; `stream.online`/`offline` pushed; viewers polled (no push) |
| Ads: next, last, length, snoozes, pre-roll free time | `GET channels/ads` (`channel:read:ads`) | Owner only (Affiliates and Partners); `channel.ad_break.begin` pushed immediately; all zeros off air |
| Followers, subscribers, goals, hype train | `channels/followers`, `subscriptions`, `goals` | Not on this page (Chat and overlays) |
| Change | One `PATCH channels` (title, game, tags, language, delay, labels, branded), commercial, snooze, marker, clip | Owner only; ads, snooze, marker and clip need a live |

Limits: title 140 characters, 10 tags of up to 25 letters and digits, delay 0 to 900 seconds (Partners). 800 requests a minute.

## Kick (official API and Pusher)

| | How | Cost, freshness |
|---|---|---|
| Title, category, live, start, viewers, mature flag, language | `GET /channels` (the `stream` object) | Title and category come back **empty off air** (seen); pushed by Pusher `LivestreamUpdated`, `StreamerIsLive`, `StopStreamBroadcast` |
| Tags | `GET /livestreams?broadcaster_user_id=` (`custom_tags`) | Only while live (to verify live) |
| Ads | `GET /ads/ad-break-status` (`ads:read`) | A login from before 1.x lacks the scope: 401; `opted_in: false` disables ad controls |
| Category lookup | `GET public/v2/categories?name=` with fallback `GET /categories?q=` | Resolves category id before one batched `PATCH /channels` |
| Change | One `PATCH /channels` (title, category id, custom tags), `POST /ads/ad-break` (7 to 90 s) | Owner only |

Kick's webhooks (`livestream.status.updated`, `livestream.metadata.updated`) need a public HTTPS
address, which a desktop app doesn't have; Pusher covers the same.

## YouTube (Data API v3)

10,000 units a day per project. A list costs 1 unit, `videos.update` and `thumbnails.set` 50.

| | How | Cost |
|---|---|---|
| The live, testing or next broadcast; state, privacy, scheduled start, latency, DVR, auto-start/stop | `liveBroadcasts.list part=snippet,status,contentDetails mine=true` | 1 |
| Title, description, tags, category, privacy, made for kids, viewers, likes | `videos.list part=snippet,status,liveStreamingDetails,statistics` | 1 |
| Stream health (good, ok, bad, no data), issues, and CDN resolution/frame rate | `liveStreams.list part=status,cdn` on the bound stream | 1, during `live`, `liveStarting`, `testing`, and `testStarting` |
| Change title, description, tags, category, privacy | `videos.update` (with cached broadcast id) | 50 |
| Thumbnail | `thumbnails.set` (with cached broadcast id) | 50 |
| Latency, DVR, auto start and stop | Read from `contentDetails` | Displayed on card; `liveBroadcasts.update` stays separate |
| Ad | `liveBroadcasts.cuepoint` (with cached broadcast id) | Monetized channels, while live |

There is no push: the chat reader notices a live starting and ending. YouTube cooldown (`300s`,
plus `30s` open cooldown) is recorded only on a successful read so transient errors retry on next view,
and cached `broadcast_id` avoids extra lookups during updates, thumbnails, and cuepoints. Someone
else's channel has no API access: its title and viewers come from the watch page, as before.

## TikTok (no public API)

| | How | Notes |
|---|---|---|
| Title, start, viewers, paused state, likes, gift coins | `api-live/user/room` (the chat's room lookup), live event counters | Paused state updates immediately from `streamPaused`/`streamResumed` |
| Change the title | Browser extension, `webcast/room/update_room` | Only while live, as the account logged in there |

Category and tags have no API at all.

## OBS Studio (zero-cost output status)

Already-connected OBS instances are queried with `GetStreamStatus` when the Stream page refreshes (no
extra connections or background timers) and woken by `StreamStateChanged` / `Disconnected`. The summary
shows standby, streaming, dropped frames, congestion warnings, reconnecting, or stopped while a platform
is live.

## What changed in 1.9.0

- Each card says Live (since when, how many watch: the chat's count, else the platform's), Testing
  (YouTube pre-live ingest check), Paused (TikTok), Not live, or why it can't change anything. Viewer
  counts are polled while the Stream page is open, as on Chat.
- Ads, snooze, marker and clip are off with "Checking accounts and stream..." before the initial read
  finishes, "Only while live." when a platform is off air, and channel ineligibility reasons when Twitch
  is not Affiliate/Partner or Kick has `opted_in: false`. The ad line shows pre-roll free time, last ad
  time and duration, and immediate EventSub `channel.ad_break.begin` status.
- Twitch `channel.update` (including language and content labels) and Kick `LivestreamUpdated` update the
  card at once, with no request; a live starting or ending and TikTok pause/resume do too. Twitch and
  Kick are read every minute while the page is open, YouTube every 5 minutes (with a 30-second tab-reopen
  cooldown; recorded only on success) unless Refresh is pressed, something changes, or a live starts or ends.
- Platforms are read side by side, and Twitch/Kick metadata + category + tags changes batch into a single
  `PATCH` per platform instead of three sequential calls.
- YouTube is read through the Data API (2 units off air, 3 while testing or live) instead of the watch
  page: title, description, tags, category, privacy, scheduled start, Made for Kids, latency, DVR,
  auto-start/stop, likes, viewers, stream health and CDN video format. The cached `broadcast_id` is
  reused for `videos.update`, `thumbnails.set`, and `liveBroadcasts.cuepoint`.
- Kick reads its tags from the live, surfaces mature-audience and language metadata, and says when its
  login lacks the ads permission.
- TikTok shows the live's title, start, paused state, likes, and coins from already-tracked room/stream state.
- OBS connected output health (`GetStreamStatus`) is summarized cleanly when OBS is already connected.
- A value the platform already shows isn't sent again; a form that changes nothing says so.

## Testing

- Unit tests parse real (trimmed) answers from each platform, check initial-read and off-air guards,
  batched Twitch/Kick updates, EventSub/Kick/TikTok/OBS hooks, YouTube quota pacing and cached broadcast
  IDs, and that unchanged values aren't sent (`core::stream::tests`).
- `platform_api` (MCP, dev mode) and the `StreamerzPlatformApi` request make one raw platform call
  through Streamerz's own login, so a test sees what the platform says without copying a token.
- `node dev/stream-check.mjs` reads the page and each platform directly (GETs only) and fails where
  they disagree: title, category, tags, live/testing state, delay, and YouTube's privacy. Run it with the
  platforms connected on a dev instance, off air and again while live.

## Expanded editors

The owner approved the additional supported fields. Twitch's card editor offers language, partner stream
delay (0 to 900 seconds, shown for partners or when non-zero), branded content and the six content
labels Helix can change. Twitch assigns MatureGame from the category; that label is read (and shown in
the card summary) but never sent in a change. Only changed labels are enabled or disabled. YouTube's
editor and the combined form offer category and privacy. These changes share one `videos.update` with
title, description and tags. Current writable snippet and status fields are preserved; read-only fields
are excluded. Switching to public or unlisted clears a publishing schedule, which is explained on the
privacy field. An unchanged save still sends no write. The page also summarizes live, not-live and
unknown platforms, adds viewer counts only when every live platform has one, shows connected OBS stream
status, and offers Accounts beside Kick's ads permission notice.

Sources checked for these additions on 2026-11-15:
[Twitch channel updates](https://dev.twitch.tv/docs/api/reference/channels),
[Twitch content labels](https://dev.twitch.tv/docs/api/reference/ccls),
[YouTube video updates](https://developers.google.com/youtube/v3/docs/videos/update).

## Live acceptance

Live testing is deferred at the owner's request. The implementation and fixture tests do not count as
live acceptance. The owner will fill in the platform checklist in [TESTING.md](TESTING.md) after testing.
Kick live tags, TikTok's room title/paused/title writes, YouTube's testing/live stream health and
live-only actions remain unverified against a live broadcast.
