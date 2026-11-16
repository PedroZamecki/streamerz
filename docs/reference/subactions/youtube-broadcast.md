# YouTube / Broadcast (sub-actions)

The broadcast itself: its tags, category, privacy and end, and what it says about itself.

## Set Tags

- Kind: sub-action
- Code: 99654
- Type: YouTubeSetTags
- Category: YouTube/Broadcast
- Needs: the broadcaster's YouTube login (an owner or moderator of the chat)

Sets the broadcast's tags. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | Which broadcast: empty for the live one, or a video id. |
| Tags (one per line) | `tags` | The tags, one per line. Variables are filled in first. |

### Arguments

None set.

## Add Tag

- Kind: sub-action
- Code: 99655
- Type: YouTubeAddTag
- Category: YouTube/Broadcast
- Needs: the broadcaster's YouTube login (an owner or moderator of the chat)

Adds a tag to the broadcast. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | Which broadcast: empty for the live one, or a video id. |
| Tag | `tag` | The tag. Variables are filled in first. |

### Arguments

None set.

## Remove Tag

- Kind: sub-action
- Code: 99656
- Type: YouTubeRemoveTag
- Category: YouTube/Broadcast
- Needs: the broadcaster's YouTube login (an owner or moderator of the chat)

Removes a tag from the broadcast. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | Which broadcast: empty for the live one, or a video id. |
| Tag | `tag` | The tag. Variables are filled in first. |

### Arguments

None set.

## Clear Tags

- Kind: sub-action
- Code: 99657
- Type: YouTubeClearTags
- Category: YouTube/Broadcast
- Needs: the broadcaster's YouTube login (an owner or moderator of the chat)

Removes every tag from the broadcast. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | Which broadcast: empty for the live one, or a video id. |

### Arguments

None set.

## Set Category

- Kind: sub-action
- Code: 99658
- Type: YouTubeSetCategory
- Category: YouTube/Broadcast
- Needs: the broadcaster's YouTube login (an owner or moderator of the chat)

Sets the broadcast's category (Gaming, Music...). This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | Which broadcast: empty for the live one, or a video id. |
| Category | `category` | One of YouTube's categories. |

### Arguments

None set.

## Set Privacy

- Kind: sub-action
- Code: 99659
- Type: YouTubeSetPrivacy
- Category: YouTube/Broadcast
- Needs: the broadcaster's YouTube login (an owner or moderator of the chat)

Sets who can watch the broadcast: everyone, anyone with the link, or only you. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | Which broadcast: empty for the live one, or a video id. |
| Who can watch | `privacy` | Public, unlisted or private. |

### Arguments

None set.

## End Stream

- Kind: sub-action
- Code: 99660
- Type: YouTubeEndStream
- Category: YouTube/Broadcast
- Needs: the broadcaster's YouTube login (an owner or moderator of the chat)

Ends the YouTube broadcast. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | Which broadcast: empty for the live one, or a video id. |

### Arguments

None set.

### Notes

- Dev mode keeps it instead of ending the stream.

## Get Broadcast Info

- Kind: sub-action
- Code: 99661
- Type: YouTubeGetBroadcastInfo
- Category: YouTube/Broadcast
- Needs: the broadcaster's YouTube login (an owner or moderator of the chat)

Says whether the YouTube broadcast is live, with its title, category, viewers, likes and uptime. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | Which broadcast: empty for the live one, or a video id. |

### Arguments

- `isLive`: true while the stream is live.
- `streamTitle`, `streamCategory`: its title and category.
- `viewerCount`: how many are watching.
- `startedAt`, `uptime`, `uptimeSeconds`: when it started, and for how long it's been live (`1:02:03`).
- `streamPlatform`: the platform (`twitch`, `kick`).
- `broadcastId`, `likeCount`, `viewCount`, `scheduledStart`: the rest YouTube gives.

## Go Live

- Kind: sub-action
- Code: 99664
- Type: YouTubeGoLive
- Category: YouTube/Broadcast
- Needs: the broadcaster's YouTube login (an owner or moderator of the chat)

Moves the YouTube broadcast to testing (the preview) or live. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | Which broadcast: empty for the live one, or a video id. |
| Move to | `stage` | Testing, or Live. |

### Arguments

None set.

### Notes

- The stream must already be coming in, as YouTube requires. Dev mode keeps it instead of going live.
