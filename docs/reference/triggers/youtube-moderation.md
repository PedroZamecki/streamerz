# YouTube / Moderation (triggers)

Moderation actions in the YouTube live chat.

## User Banned

- Kind: trigger
- Code: 4005
- Type: YouTubeUserBanned
- Category: YouTube/Moderation
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when the messages of a viewer are removed from the YouTube live chat, which is how a ban shows in the chat.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `userId`, `userType` | The id of the viewer, and `youtube`. |
| `banType` | Always `unknown`. |
| `user`, `userName` | The viewer's name, when they chatted in this session. |
| `broadcastId`, `broadcast.*` | The live broadcast, as Broadcast Started sets them. |

### When

When the chat page says all the messages of a viewer were removed.

### Notes

- The chat page doesn't say whether the viewer was banned or timed out, so `banType` is always `unknown`, and the removal can also come from a timeout.
- `user` and `userName` are only set if Streamerz saw a message from that viewer in this session.
