# YouTube / Polls (sub-actions)

Sub-actions that create a poll in a YouTube live chat and end it again.

## Create Poll

- Kind: sub-action
- Code: 4011
- Type: YouTubeCreatePoll
- Category: YouTube/Polls
- Needs: a YouTube login (the broadcaster)

Creates a poll in the live chat of the stream. Each line of the options is one answer. Blank lines are skipped.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the live stream Streamerz is reading. |
| Question | `question` | The question of the poll. |
| Options (one per line) | `options` | The answers, one per line. |

### Arguments

- `success`: true when YouTube created the poll.
- `pollId`: the id of the new poll, when it was created.

### Notes

- Streamerz remembers the poll it made, so End Poll can end it. Only one poll is remembered: a new one replaces the last.

## End Poll

- Kind: sub-action
- Code: 4012
- Type: YouTubeEndPoll
- Category: YouTube/Polls
- Needs: a YouTube login (the broadcaster)

Ends the poll that Streamerz created last.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | Not used. The poll is the one Streamerz made last. |

### Arguments

None set.

### Notes

- Only polls made by Streamerz can be ended.
- The Broadcast field is saved but not used.
