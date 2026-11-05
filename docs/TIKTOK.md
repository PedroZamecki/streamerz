# TikTok capabilities

Streamerz reads LIVE events natively. The optional browser extension supplies one account for signed
search, authenticated reading and chat sends. Public reading works without it. No new polling task is
needed for incoming chat updates.

| Feature | Support |
| --- | --- |
| LIVE chat and commands | Read comments, badges, avatars and received emotes; run TikTok commands and triggers. |
| Sending | Text and Unicode emoji through the browser account; replies use @handle. Message all and sub-action 43001 use the same sender. |
| Channel search | Signed search with names, pictures, follower counts and live state. Pasted profile, video and LIVE links resolve to their handle. |
| Room lookup | Native lookup first; a transient public failure falls back to the connected browser's signed room endpoint. Offline responses stay offline. Reuse the browser device cookie when available instead of downloading an anonymous page. |
| Community | Gifts, likes, follows, shares, subscriptions and joins, with existing triggers and WebSocket events. |
| Stream data | Viewer count, likes, coins, top gifters and observed pause state. Sub-actions 43002 and 43003 read the same state. |
| Deleted messages | Apply incoming message and user-history deletions; remove an affected pin. |
| Pinned comments | Show incoming chat pins and explicit cancellations. A stale cancellation cannot remove a newer pin. |
| Pause and resume | Keep chat connected, show a notice and update isPaused. |
| Background operation | One offscreen browser context; no helper tab. Recreate discarded read contexts. Never automatically replay a chat send. |
| Recovery | Reopening account setup after a failed request rechecks identity and signing state once. |

Messages over 150 characters, including an @mention, are rejected before sending. The composer retains
the draft. It never posts a silently shortened message. Automation uses the same validation.

## Automation and inspection

Use the MCP `request` tool with `{"request":"StreamerzTikTokState","count":10}`. It returns `stream`,
`topGifters` and current `pins` without making a network request. `count` is limited to 1 through 100.
Authentication follows the other Streamerz requests.

Subscribe to `TikTok` events `MessageDeleted`, `MessagePinned`, `StreamPaused` and `StreamResumed`.
`MessagePinned` carries `action` 1 (pin) or 2 (cancel), the original `chatMessage`, `pinMsgId` and
`operator`. These extend the WebSocket catalog without assigning unofficial Streamer.bot trigger IDs.
Existing TikTok triggers keep their IDs and behavior.

In development mode, `InjectTikTokEvent` accepts an `event` and an object `data`, and runs the real
handler. Supported names are `chat`, `gift`, `like`, `follow`, `share`, `subscribe`, `member`, `roomUser`,
`messageDelete`, `roomPin`, `streamPaused` and `streamResumed`. For example:

```json
{"request":"InjectTikTokEvent","event":"messageDelete","data":{"deleteMsgIds":["123"]}}
```

Explicit bridge recovery is `{"request":"StreamerzTikTokBridge","action":"retry"}`. It reloads
account and signing state; it sends no chat message and opens no login window.

## Boundaries and verification

Native stickers can be displayed but their send encoding is not implemented. Moderation writes, gifts,
likes, follow/unfollow, direct messages, publishing videos and starting or editing a LIVE are not
implemented. A valid signature alone does not establish the endpoint, payload or permission needed for
those operations. They are not shown as working controls.

Pins follow explicit cancellation, deletion, disconnect and stream-end events. Automatic duration
expiry is not inferred: the schema does not establish its time units. Non-chat pin payloads are ignored.

The wire additions use the field numbers and enum values in
[tiktok-live-proto 0.2.4](https://www.npmjs.com/package/tiktok-live-proto/v/0.2.4), documented in the
[connector's schema](https://ttlc.docs.isaackogan.com/schema/). Regression tests cover packed and unpacked
64-bit deletion IDs, pin and control decoding, the real event handler, request authentication and signed
room replies. These additions have fixture coverage; a live moderator deletion, timed pin, pause and
public-challenge fallback have not been observed in this audit. Earlier real chat sending and signed
search results are recorded in [TESTING.md](TESTING.md).
