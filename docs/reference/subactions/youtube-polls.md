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

## Get Poll Results

- Kind: sub-action
- Code: 99665
- Type: YouTubeGetPollResults
- Category: YouTube/Polls
- Needs: a YouTube channel or video set, and a live broadcast (no login)

Gives the poll on the live now, with its votes so far, as the chat shows it. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

None.

### Arguments

- `pollFound`: true when a poll is on the live now.
- `poll.id`, `poll.question`, `question`, `poll.options.count`.
- `poll.option0.text`, `poll.option0.votePercentage`, `poll.option0.voteRatio`, `poll.option0.votes`, and the same for each option, counting from 0.
- `poll.totalVotes`, `poll.winningOption.text`, `poll.winningOption.votes`.

### Notes

- YouTube gives the share of each option, not the vote count, so each `votes` is worked out from the share and the total.

