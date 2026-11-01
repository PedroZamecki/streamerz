# TikTok / Stream (triggers)

The state of a TikTok Live: its viewers, and when it starts and ends. They need a TikTok channel set: Streamerz connects to its live by itself.

## Viewer Count Update

- Kind: trigger
- Code: 41010
- Type: TikTokViewerCountUpdate
- Category: TikTok/Stream
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when the TikTok live's viewer count changes, with the top viewers by coins.

### Fields

None.

### Arguments

- `viewerCount`: the number of viewers now.
- `topViewers`: a list of the top viewers by coins sent, each with `coinCount` and the viewer's user fields.
- `channel`: the channel as set.

### When

Fires when TikTok's viewer count differs from the last count seen in this live. An unchanged count doesn't fire.

### Notes

None.

## Stream Online

- Kind: trigger
- Code: 41011
- Type: TikTokStreamOnline
- Category: TikTok/Stream
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when a new TikTok live is found.

### Fields

None.

### Arguments

- `channel`: the channel as set.
- `roomId`: the id of the new live room.

### When

Fires when Streamerz connects to a live room it hasn't seen before. The live's tallies (likes, coins and top gifters) start over then.

### Notes

- Coming back to the same live after a drop doesn't fire this again.

## Stream Offline

- Kind: trigger
- Code: 41012
- Type: TikTokStreamOffline
- Category: TikTok/Stream
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when the TikTok live ends, with its viewers, likes and coins.

### Fields

None.

### Arguments

- `isLive`: false.
- `roomId`: the id of the live room.
- `viewerCount`: the viewers when the live ended.
- `totalLikes`: the total likes of the live.
- `totalCoins`: the total coins sent in the live.
- `channel`: the channel as set.

### When

Fires when TikTok sends the message that the stream has ended, while Streamerz is connected to it.

### Notes

- Disconnected fires right after this one. Streamerz then waits for the next live.
