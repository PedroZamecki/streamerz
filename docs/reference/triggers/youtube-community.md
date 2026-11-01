# YouTube / Community (triggers)

Emotes added to or removed from the channel by BetterTTV and 7TV, and the polls shown in the YouTube live chat.

## Seven TVEmote Added

- Kind: trigger
- Code: 4023
- Type: YouTubeSevenTVEmoteAdded
- Category: YouTube/Community
- Needs: a YouTube channel set, and a live broadcast (no login).

Fires when an emote is added to the channel's 7TV set.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `emote` | The name of the emote. |
| `imageUrl` | The image of the emote. |

### When

When the channel's 7TV emote set gains an emote, while the broadcast is live and an action uses this trigger.

### Notes

- Streamerz gets the changes from the 7TV service over its own connection, and reconnects after a minute if it drops.

## Seven TVEmote Removed

- Kind: trigger
- Code: 4024
- Type: YouTubeSevenTVEmoteRemoved
- Category: YouTube/Community
- Needs: a YouTube channel set, and a live broadcast (no login).

Fires when an emote is removed from the channel's 7TV set.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `emote` | The name of the emote. |
| `imageUrl` | The image of the emote. |

### When

When the channel's 7TV emote set loses an emote, while the broadcast is live and an action uses this trigger.

## Better TTVEmote Added

- Kind: trigger
- Code: 4025
- Type: YouTubeBetterTTVEmoteAdded
- Category: YouTube/Community
- Needs: a YouTube channel set, and a live broadcast (no login).

Fires when an emote is added to the channel's BetterTTV set.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `emote` | The name of the emote. |
| `imageUrl` | The image of the emote. |

### When

When the channel's BetterTTV emote set gains an emote, while the broadcast is live and an action uses this trigger.

### Notes

- Streamerz gets the changes from the BetterTTV service over its own connection, and reconnects after a minute if it drops.

## Better TTVEmote Removed

- Kind: trigger
- Code: 4026
- Type: YouTubeBetterTTVEmoteRemoved
- Category: YouTube/Community
- Needs: a YouTube channel set, and a live broadcast (no login).

Fires when an emote is removed from the channel's BetterTTV set.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `emote` | The name of the emote. |
| `imageUrl` | The image of the emote. |

### When

When the channel's BetterTTV emote set loses an emote, while the broadcast is live and an action uses this trigger.

## Poll Closed

- Kind: trigger
- Code: 4027
- Type: YouTubePollClosed
- Category: YouTube/Community
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when a poll in the chat is closed.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `panelId` | The id of the closed poll panel. |
| `poll.*`, `question` | The last state of the poll Streamerz saw (see Poll Started). |
| `broadcastId`, `broadcast.*` | The live broadcast, as Broadcast Started sets them. |

### When

When YouTube closes the poll panel. It fires only when Streamerz saw a poll in this session.

## Poll Started

- Kind: trigger
- Code: 4028
- Type: YouTubePollStarted
- Category: YouTube/Community
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when a poll is shown in the chat.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `poll.id` | The id of the poll. |
| `poll.question`, `question` | The question. |
| `poll.options.count` | The number of options. |
| `poll.option0.text`, `poll.option1.text`... | The text of each option, counting from 0. |
| `poll.option0.votePercentage`, `poll.option0.voteRatio`, `poll.option0.votes`... | The share of the votes of each option, and its count. |
| `poll.totalVotes` | The total number of votes. |
| `poll.winningOption.text`, `poll.winningOption.votes` | The option with the most votes. |
| `broadcastId`, `broadcast.*` | The live broadcast, as Broadcast Started sets them. |

### When

When YouTube shows a new poll panel in the chat.

### Notes

- YouTube gives the share of each option, not the vote count. Streamerz works out each `votes` value from the share and the total, so these counts are estimates.

## Poll Updated

- Kind: trigger
- Code: 4029
- Type: YouTubePollUpdated
- Category: YouTube/Community
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when the votes of the poll on screen change.

### Fields

None.

### Arguments

The same `poll.*` arguments as Poll Started, for the poll as it is now.

### When

Each time YouTube updates the poll in the chat.

### Notes

- The vote counts are worked out from the shares, as described for Poll Started.
