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
| Title, category, tags, language, content labels, branded content, delay | `GET channels` | One request; pushed by EventSub `channel.update` (already subscribed) |
| Live, started at, viewers, thumbnail | `GET streams` (empty off air) | One request; `stream.online`/`offline` pushed; viewers polled (no push) |
| Ads: next, last, length, snoozes, pre-roll free time | `GET channels/ads` (`channel:read:ads`) | Owner only; all zeros off air |
| Followers, subscribers, goals, hype train | `channels/followers`, `subscriptions`, `goals` | Not on this page (Chat and overlays) |
| Change | `PATCH channels` (title, game, tags, language, labels, branded), commercial, snooze, marker, clip | Owner only; ads, snooze, marker and clip need a live |

Limits: title 140 characters, 10 tags of up to 25 letters and digits. 800 requests a minute.

## Kick (official API and Pusher)

| | How | Cost, freshness |
|---|---|---|
| Title, category, live, start, viewers | `GET /channels` (the `stream` object) | Title and category come back **empty off air** (seen); pushed by Pusher `LivestreamUpdated`, `StreamerIsLive`, `StopStreamBroadcast` |
| Tags | `GET /livestreams?broadcaster_user_id=` (`custom_tags`) | Only while live (to verify live) |
| Ads | `GET /ads/ad-break-status` (`ads:read`) | A login from before 1.x lacks the scope: 401 |
| Category search | `GET /categories?q=` | Returns ids, names and thumbnails |
| Change | `PATCH /channels` (title, category id, custom tags), `POST /ads/ad-break` (7 to 90 s) | Owner only |

Kick's webhooks (`livestream.status.updated`, `livestream.metadata.updated`) need a public HTTPS
address, which a desktop app doesn't have; Pusher covers the same.

## YouTube (Data API v3)

10,000 units a day per project. A list costs 1 unit, `videos.update` and `thumbnails.set` 50.

| | How | Cost |
|---|---|---|
| The live, else the next broadcast; its state, privacy, start | `liveBroadcasts.list mine=true` | 1 |
| Title, description, tags, category, privacy, made for kids, viewers | `videos.list part=snippet,status,liveStreamingDetails` | 1 |
| Stream health (good, ok, bad, no data) and what's wrong (bitrate, resolution...) | `liveStreams.list part=status` on the bound stream | 1, only while live |
| Change title, description, tags, category, privacy | `videos.update` | 50 |
| Thumbnail | `thumbnails.set` | 50 |
| Latency, DVR, auto start and stop | `liveBroadcasts.update` | 50 (not on this page) |
| Ad | `liveBroadcasts.cuepoint` | Monetized channels, while live |

There is no push: the chat reader notices a live starting and ending. Someone else's channel has no
API access: its title and viewers come from the watch page, as before.

## TikTok (no public API)

| | How | Notes |
|---|---|---|
| Title, start, viewers | `api-live/user/room` (the chat's room lookup), `roomUser` events | Only while live (to verify live) |
| Change the title | Browser extension, `webcast/room/update_room` | Only while live, as the account logged in there |

Category and tags have no API at all.

## What changed in 1.9.0

- Each card says Live (since when, how many watch: the chat's count, else the platform's), Not live, or
  why it can't change anything. Viewer counts are polled while the Stream page is open, as on Chat.
- Ads, snooze, marker and clip are off with "Only while live." when a platform is off air (buttons, the
  top Run ad, the ad form's platform boxes and the requests behind them). The ad line says "Ads run only
  while live." off air, and shows Twitch's pre-roll free time.
- Twitch `channel.update` and Kick `LivestreamUpdated` update the card at once, with no request; a live
  starting or ending does too, and the read that follows confirms it. Twitch and Kick are read every
  minute while the page is open (was 30 seconds), YouTube every 5 minutes unless the page is opened,
  Refresh is pressed, something changes or a live starts or ends.
- Platforms are read side by side, and a change on several platforms runs on each at once.
- YouTube is read through the Data API (2 units, 3 while live) instead of the watch page: title,
  description, tags, category, privacy (Private and Unlisted are said on the card), viewers and stream
  health with YouTube's own complaint. The edit form starts with the description and tags.
- Kick reads its tags from the live and says when its login lacks the ads permission.
- TikTok shows the live's title and start from the room lookup.
- A value the platform already shows isn't sent again; a form that changes nothing says so.

## Testing

- Unit tests parse real (trimmed) answers from each platform, check the off-air guards, pushes, quota
  pacing and that unchanged values aren't sent (`core::stream::tests`).
- `platform_api` (MCP, dev mode) and the `StreamerzPlatformApi` request make one raw platform call
  through Streamerz's own login, so a test sees what the platform says without copying a token.
- `node dev/stream-check.mjs` reads the page and each platform directly (GETs only) and fails where
  they disagree: title, category, tags, live, the off-air buttons and YouTube's privacy. Run it with the
  platforms connected on a dev instance, off air and again while live.

## Open questions

- Editing more from the page: YouTube's category and privacy (`videos.update`), Twitch's language,
  content labels and branded content (`PATCH channels`). All are readable now.
- A combined line on top (live on 2 of 4, total viewers).
- Re-login prompt for Kick's ads permission from the card.
