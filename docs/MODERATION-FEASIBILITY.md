# Moderation feasibility study

Research date: 2026-11-14. This is a design and feasibility study, not a description of a completed feature. Public documentation and the Streamerz source were reviewed. Authenticated moderation dashboards and real moderation actions were not tested.

## Recommendation

Build a moderation workspace with Review, People, Activity and Protection views. Keep platform operations and identity links in the toolkit-independent core, expose each operation through MCP, and use the existing GPUI controls. Present a capability and its source per channel and account. A platform's dashboard can offer operations that its public API does not expose.

Do not represent an observed event log as a complete platform history. Show whether a list is authoritative, partial, stale, unavailable or still loading, when it was last refreshed, and why. Fetch pages on demand and reuse existing event connections; disabled integrations must not start tasks or connections. Persist only what is necessary and respect existing user data settings.

## Capability map

| Feature | Twitch | Kick | YouTube | TikTok |
| --- | --- | --- | --- | --- |
| Current restrictions | Documented paginated API, broadcaster authorization | Dashboard list; official API list not found | No list method in documented live chat bans API | LIVE mute and block lists documented; web access unverified |
| Restrict and lift | Documented ban, timeout and unban API | Documented ban, timeout and unban API | Insert and delete live chat ban; delete needs ban ID | LIVE mute and block exist; integration routes unverified |
| Review queue | AutoMod holds via EventSub, allow or deny via API; unban requests via API | Dashboard and API coverage need investigation | No equivalent established by this study | No equivalent established by this study |
| Protection | Documented AutoMod settings, blocked terms, chat settings, Shield Mode | Dashboard supports chat modes and blocked terms; public API access unverified | Platform-specific chat controls need separate study | LIVE comment settings need separate study |
| Activity | EventSub provides future events, subject to scopes and connection | Dashboard feed; documented ban webhook; historical access unverified | Live chat ban events; coverage depends on reader | Existing reader receives some moderation events; list coverage unverified |

Sources: [Twitch moderation API](https://dev.twitch.tv/docs/api/reference/moderation), [Kick moderation API](https://github.com/KickEngineering/KickDevDocs/blob/main/apis/moderation.md), [Kick dashboard](https://help.kick.com/en/articles/10162074-moderation-features-guide), [YouTube live chat bans](https://developers.google.com/youtube/v3/live/docs/liveChatBans), [TikTok LIVE moderator](https://seller-ie.tiktok.com/university/essay?knowledge_id=3810732642666242&lang=en-GB).

## Twitch routes and behavior

Use Helix `GET moderation/banned` for the paginated current list and `POST` / `DELETE moderation/bans` for bans, timeouts and removal. The list's broadcaster authorization differs from the authorization for moderation actions: a moderator able to ban someone may not be able to fetch the broadcaster's complete list. Preserve IDs, reason, moderator, creation and expiry where returned. A timeout can expire or change outside Streamerz, so refresh after actions and reconcile EventSub events.

AutoMod held messages arrive through `automod.message.hold`; `automod.message.update` reports changes, and `POST moderation/automod/message` allows or denies. This is an event-driven queue, not proof that holds missed while offline can be recovered. Mark the queue's coverage accordingly and handle another moderator resolving a message first. Use `GET` / `PUT moderation/automod/settings` for settings, subject to the exact authorization returned by Twitch.

Use `GET` / `PATCH moderation/unban_requests` for requests and resolution. Use the documented blocked-terms, chat-settings, Shield Mode, warnings and suspicious-user endpoints for their respective controls. EventSub provides corresponding live updates, including suspicious-user messages that may not appear in ordinary chat. See [Twitch's moderation guide](https://dev.twitch.tv/docs/chat/moderation/) and [EventSub reference](https://dev.twitch.tv/docs/eventsub/reference/channel-moderation).

The source already requests many moderation scopes in `src/twitch.rs`, receives several moderation events, and implements some action paths in `src/engine/rest.rs`. Audit exact scope and token requirements operation by operation. For example, blocked-term management and AutoMod settings changes require checking whether the existing scope set suffices. Do not claim dashboard parity for historical moderator comments, old messages, shared-ban settings or action history without a documented route and verification.

## Kick routes and behavior

Kick's public API documents `POST` and `DELETE /public/v1/moderation/bans`. The `moderation:ban` scope and a permitted account are required. Streamerz already implements these in `src/kick.rs`; its timeout conversion rounds seconds up to minutes and clamps the duration. Show the effective duration before sending.

Kick's dashboard documents a searchable, paginated list of current bans and timeouts, blocked terms, chat modes and a moderator action feed. Its published moderation API page does not document list or dashboard-management methods. The `moderation.banned` webhook can report new bans, but does not reconstruct the complete list or historical feed and requires the optional webhook setup. See [Kick events](https://github.com/KickEngineering/KickDevDocs/blob/main/events/event-types.md).

Research the authenticated dashboard in an account with permission: identify the list and settings requests, pagination, role requirements, stable identifiers and errors. A website endpoint would be unofficial, may require browser authentication and may change. Do not assume the existing OAuth token authorizes it. Evaluate whether a supported route exists before proposing any browser extension or storing browser credentials. Without a verified list route, label entries observed by Streamerz as partial and provide a link to Kick's dashboard.

An independent endpoint inventory lists `api/v1/channels/{channel}/banned-users` and `api/internal/v1/channels/{channel}/chatroom/banned-words`. These are leads for an authenticated dashboard investigation, not verified current contracts or evidence that Kick's public OAuth token can read them. Source: [kick.py endpoint inventory](https://github.com/cibere/kick.py/blob/main/endpoints.json).

## YouTube routes and behavior

The official Live Streaming API supports `POST /youtube/v3/liveChat/bans` for temporary or permanent bans and `DELETE /youtube/v3/liveChat/bans?id=...` to remove a ban. An authorized owner or moderator and the relevant live chat are required. The published resource lists insert and delete, but no list method. Deletion requires the ban resource ID, not merely the user's channel ID. [Google's reference](https://developers.google.com/youtube/v3/live/docs/liveChatBans) is the contract.

The current implementation in `src/youtube.rs` sends `bannedAuthorDetails`, whereas the documented resource specifies `bannedUserDetails`. This discrepancy needs an API fixture and real-service verification; do not assume either that the current request succeeds or that changing it alone fixes it. The current success path discards the response body, so it does not retain the returned ban ID. Capture and associate it with the live chat and target only after confirming the response. Existing bans made elsewhere cannot be assumed recoverable. Observed ban events and local records are partial and can become stale.

## TikTok routes and behavior

TikTok documents separate LIVE mute and block lists and moderator management. Its help distinguishes LIVE restrictions from other account restrictions. The precise current desktop page, list scope, fields, pagination, permissions and requests have not been inspected: the available preview profile reached TikTok's login page. Do not confuse LIVE mutes with notification or content mutes, or assume a block has the same consequences as a chat timeout.

Streamerz's existing browser extension executes requests in a TikTok page context using the browser session and TikTok's signing scripts. This makes it a potential route, not evidence that moderation calls work. The frame currently permits only specified TikTok hosts. An authenticated test must determine the exact page and request flow, whether a live room is required, how owner and moderator permissions differ, whether list and action requests work through the existing frame, and how errors and pagination behave. Record sanitized request shapes only; never save cookies, tokens or real user data. See `assets/tiktok-extension/frame.js` and `src/tiktok_bridge.rs`.

An independent implementation identifies candidate LIVE routes `webcast/room/silence/`, `unsilence/`, `kick/user/` and `unkick/user/`, with room and target IDs, and distinguishes JSON from form encoding. Its tests exercise mocked transport, so these are leads, not verified contracts for Streamerz's browser context. Inspect and verify each against a test LIVE before relying on it. Source: [live_management.py](https://github.com/Loukious/TikTokStreamKeyGenerator/blob/main/Libs/live_management.py).

If the platform requires a different browser extension permission or installation change, explain the change and obtain the user's agreement before modifying another program. If no reliable integration is verified, offer the appropriate TikTok page and clearly state the limitation.

## Cross-platform model and operations

An identity link is an explicit user-maintained group of platform account IDs, scoped to the relevant channel or owner as appropriate. Names are labels and search hints, never proof of identity. Allow reviewing, correcting and removing links. Preserve platform-specific restriction types, reason, duration, actor, scope and provenance rather than translating all restrictions into one generic ban. Distinguish a TikTok block from a chat-only action.

For an operation across linked accounts, present each selected target and effective action, check available account and permissions, then execute independently. Report success, failure or unknown outcome for each target. Never roll back successful platform actions silently. Avoid automatic retries when a timeout leaves the outcome unknown; refresh or reconcile first. No automatic propagation should be implied by linking accounts.

Model states separately: authoritative snapshot, observed event, local action and unknown state. Track source, channel, platform ID, event ID where available, timestamps and freshness. Deduplicate events and handle reconnect gaps, pagination, external changes, expired timeouts, revoked permissions and conflicts. Restrict persistence and retention to what the user needs; avoid leaking moderation data into logs or test fixtures.

## Verification and implementation sequence

1. Audit existing core, MCP and frontend controls and write an agreed scope to `PLAN.md`. Define capability and freshness states before building UI.
2. Implement and test Twitch snapshots, actions and event-driven review, including broadcaster versus moderator credentials, pagination, disconnects and competing moderator actions.
3. Audit and fix YouTube request and response handling against Google's API, then verify with a test live chat. Treat known bans as partial, including after restart.
4. Implement Kick's documented actions and explicitly partial event history. Inspect its authenticated dashboard before choosing any additional integration.
5. Inspect TikTok's authenticated LIVE and test list and action requests with consenting test accounts. Implement only verified capabilities; document any account, region or live-state restrictions.
6. Add explicit account linking and cross-platform action results. Test mixed outcomes, duplicate identities, durations, errors and stale data.
7. Expose all operations through MCP with meaningful tests. Exercise the UI on the hidden display at narrow and wide sizes, run the smoke test and required local checks, and test real services where available. Measure the version's hidden RSS and idle CPU as required by the project rules.

Open decisions include retention duration for local moderation records, whether to offer optional persistent history, which dashboard features to prioritize, and whether any unofficial browser route is acceptable after its behavior and maintenance costs are known. Platform restrictions and missing authenticated verification remain blockers to claiming full parity.
