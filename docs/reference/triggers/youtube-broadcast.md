# YouTube / Broadcast (triggers)

Events about the YouTube live broadcast: when it starts and ends, and when Streamerz starts or stops watching the channel.

## Broadcast Started

- Kind: trigger
- Code: 4001
- Type: YouTubeBroadcastStarted
- Category: YouTube/Broadcast
- Needs: a YouTube channel or video set. A YouTube login adds the title and the other details.

Fires when Streamerz finds a live broadcast on the YouTube channel and starts reading its chat.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `broadcastId` | The video id of the broadcast. |
| `broadcast.id` | The same video id. |
| `broadcast.title`, `broadcast.description`, `broadcast.publishedAt`, `broadcast.defaultLanguage`, `broadcast.defaultAudioLanguage`, `broadcast.privacy`, `broadcast.status` | The video's details from the YouTube Data API. |
| `broadcast.tags` | The tags, separated by commas. |
| `broadcastUserProfileImage` | The channel's thumbnail. |

### When

Streamerz looks for a live broadcast every minute while the channel is offline, or every 5 seconds while a stream is warming up. It fires once when the chat of a live broadcast is found. After that broadcast ends, it can fire again for the next one.

### Notes

- The `broadcast.*` details and `broadcastUserProfileImage` need a YouTube login. Without one, only `broadcastId` and `broadcast.id` are set.

## Broadcast Ended

- Kind: trigger
- Code: 4002
- Type: YouTubeBroadcastEnded
- Category: YouTube/Broadcast
- Needs: a YouTube channel or video set.

Fires when Streamerz sees that the live broadcast has ended and goes back to waiting for the next one.

### Fields

None.

### Arguments

The same `broadcast.*` and `broadcastId` arguments that Broadcast Started had.

### When

Once, when the live stream is seen to end. It only fires for a broadcast that Streamerz started reading in this session.

### Notes

- Nothing is fired if no broadcast was started in this session.

## Broadcast Updated

- Kind: trigger
- Code: 4013
- Type: YouTubeBroadcastUpdated
- Category: YouTube/Broadcast
- Needs: nothing (not wired up yet).

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

### Notes

- Streamerz doesn't fire this yet.

## Broadcast Monitoring Started

- Kind: trigger
- Code: 4019
- Type: YouTubeBroadcastMonitoringStarted
- Category: YouTube/Broadcast
- Needs: a YouTube channel or video set.

Fires when Streamerz starts watching the YouTube channel for live broadcasts.

### Fields

None.

### Arguments

None.

### When

When the YouTube connection starts watching the channel. This happens before any broadcast is live.

### Notes

- Nothing is sent with it. Use Broadcast Started to act on a live stream.

## Broadcast Monitoring Ended

- Kind: trigger
- Code: 4020
- Type: YouTubeBroadcastMonitoringEnded
- Category: YouTube/Broadcast
- Needs: a YouTube channel or video set.

Fires when Streamerz stops watching the YouTube channel.

### Fields

None.

### Arguments

None.

### When

When the YouTube connection is turned off while it was watching the channel.

### Notes

- Nothing is sent with it.

## Broadcast Added

- Kind: trigger
- Code: 4021
- Type: YouTubeBroadcastAdded
- Category: YouTube/Broadcast
- Needs: nothing (not wired up yet).

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

### Notes

- Streamerz doesn't fire this yet.

## Broadcast Removed

- Kind: trigger
- Code: 4022
- Type: YouTubeBroadcastRemoved
- Category: YouTube/Broadcast
- Needs: nothing (not wired up yet).

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

### Notes

- Streamerz doesn't fire this yet.
