# YouTube / Channel & Statistics (triggers)

Numbers and changes for the channel and the live broadcast: statistics, viewers, new subscribers and the first messages of viewers.

## Statistics Updated

- Kind: trigger
- Code: 4012
- Type: YouTubeStatisticsUpdated
- Category: YouTube/Channel & Statistics
- Needs: a YouTube login (the broadcaster, else the bot), and a live broadcast.

Fires with the numbers of the live broadcast: likes, views and the other counts.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `likeCount`, `dislikeCount` | The likes and dislikes. |
| `viewCount` | The views. |
| `favoriteCount` | The times it was saved as a favourite. |
| `commentCount` | The number of comments. |
| `concurrentViewers` | The viewers watching now. |
| `broadcastId`, `broadcast.*` | The live broadcast, as Broadcast Started sets them. |

### When

Once a minute while the broadcast is live, but only when an action uses this trigger.

### Notes

- Streamerz reads the numbers from the YouTube Data API, so it needs a YouTube login. Without one, nothing fires.

## First Words

- Kind: trigger
- Code: 4016
- Type: YouTubeFirstWords
- Category: YouTube/Channel & Statistics
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires on the first message a viewer writes in the channel's chat.

### Fields

None.

### Arguments

The same arguments as Message (see YouTube / Chat & Super Chats), for that message.

### When

On a viewer's first message that Streamerz sees since it started counting. It fires just before Message for that same message.

### Notes

- Streamerz keeps the list of viewers who have written in the chat. The list is cleared when the YouTube channel is changed, or when the first words cache is cleared through the API.

## Present Viewers

- Kind: trigger
- Code: 4017
- Type: YouTubePresentViewers
- Category: YouTube/Channel & Statistics
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires every five minutes with the viewers who wrote in the chat recently.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `users` | The viewers who wrote in the last 10 minutes, as a list. Each has `userId`, `userName` and `user`. |
| `isLive` | Always on. |
| `isTest` | Always off. |
| `broadcastId`, `broadcast.*` | The live broadcast, as Broadcast Started sets them. |

### When

Every five minutes while the broadcast is live, but only when an action uses this trigger. It fires even when the list is empty.

## New Subscriber

- Kind: trigger
- Code: 4018
- Type: YouTubeNewSubscriber
- Category: YouTube/Channel & Statistics
- Needs: a YouTube broadcaster login, and a live broadcast.

Fires for each new subscriber to the channel.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `user`, `userName` | The subscriber's name. |
| `userId` | The subscriber's channel id. |
| `userType` | Always `youtube`. |
| `userProfileUrl` | The subscriber's thumbnail. |
| `broadcastId`, `broadcast.*` | The live broadcast, as Broadcast Started sets them. |

### When

Every two minutes while the broadcast is live, but only when an action uses this trigger. It compares the list of recent subscribers with the one from the last check.

### Notes

- The first check only sets the list to compare with, so nothing fires for the subscribers that were there before.
- Only the 50 most recent subscribers are checked each time.
- The YouTube Data API needs a login from the broadcaster. The bot account is not used here.

## User Timedout

- Kind: trigger
- Code: 4031
- Type: YouTubeUserTimedout
- Category: YouTube/Channel & Statistics
- Needs: nothing (not wired up yet).

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

### Notes

- Streamerz doesn't fire this yet. A timeout can show up as User Banned, since the chat only shows that the viewer's messages were removed.
