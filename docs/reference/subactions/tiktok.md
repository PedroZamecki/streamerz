# TikTok (sub-actions)

Sub-actions for the TikTok live.

## Get Stream Info

- Kind: sub-action
- Code: 43003
- Type: TikTokGetStreamInfo
- Category: TikTok
- Needs: nothing to set up. The values come from the TikTok live Streamerz is connected to.

Reads what Streamerz knows about the TikTok live and adds it to the arguments: whether the live is on, its room ID, the viewer count, and the total likes and coins. It doesn't start a TikTok connection.

### Fields

None.

### Arguments

- `isLive`: true while Streamerz is connected to the TikTok live.
- `isPaused`: true while connected and TikTok reports the LIVE is paused. A pause keeps chat connected.
- `roomId`: the room ID of the live.
- `viewerCount`: the viewer count the live reported last.
- `totalLikes`: the likes counted for this live.
- `totalCoins`: the coins counted for this live.

### Notes

- Without a TikTok connection, `roomId` is empty and the counts are zero.
- The values stay as they were when the connection closed. They reset to zero when a different live starts.

## Set Stream Title

- Kind: sub-action
- Code: 43010
- Type: TikTokSetStreamTitle
- Category: TikTok
- Needs: TikTok sending set up (browser extension connected) and an active live room

Sets the title of the TikTok LIVE broadcast.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Title | `title` | The new stream title. |

### Arguments

- `titleSuccess`: true when the title was updated on TikTok LIVE, false otherwise.
