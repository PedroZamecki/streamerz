# Twitch / Polls (sub-actions)

Twitch polls on the channel: start one, end it, archive it, or read the one that is running. Polls are the channel owner's, so the logged-in account must be the owner.

## Create Poll

- Kind: sub-action
- Code: 561
- Type: TwitchCreatePoll
- Category: Twitch/Polls
- Needs: a Twitch login. Twitch decides who may do it on a channel (its guide says moderators and editors can manage polls; the call's reference says the broadcaster); if it refuses, its answer is logged

Starts a poll on the channel.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Title | `title` | The poll's question. |
| Duration (seconds) | `duration` | A whole number. |
| Points per vote | `channelPointsPerVote` | A whole number. |
| Choices (one per line) | `choices` | One choice per line. |

### Arguments

- `success`: true when Twitch created the poll, false otherwise.
- `pollId`: the new poll's id (only when it was created).

### Notes

- The duration is kept between 15 and 1800 seconds.
- Channel points voting is turned on when points per vote is above 0.
- Variables like `%user%` in the title and choices are filled in first.

## Terminate Active Poll

- Kind: sub-action
- Code: 562
- Type: TwitchTerminateActivePoll
- Category: Twitch/Polls
- Needs: a Twitch login. Twitch decides who may do it on a channel (its guide says moderators and editors can manage polls; the call's reference says the broadcaster); if it refuses, its answer is logged

Ends the active poll early.

### Fields

None.

### Arguments

None set.

### Notes

- It acts on the newest poll that is still active. When there is none, nothing happens and an error goes to the log.

## Archive Active Poll

- Kind: sub-action
- Code: 563
- Type: TwitchArchiveActivePoll
- Category: Twitch/Polls
- Needs: a Twitch login. Twitch decides who may do it on a channel (its guide says moderators and editors can manage polls; the call's reference says the broadcaster); if it refuses, its answer is logged

Sets the active poll to archived on Twitch.

### Fields

None.

### Arguments

None set.

### Notes

- It acts on the newest poll that is still active. When there is none, nothing happens and an error goes to the log.

## Get Active Poll

- Kind: sub-action
- Code: 566
- Type: TwitchGetActivePoll
- Category: Twitch/Polls
- Needs: a Twitch login. Twitch decides who may do it on a channel (its guide says moderators and editors can manage polls; the call's reference says the broadcaster); if it refuses, its answer is logged

Reads the poll that is running, if there is one.

### Fields

None.

### Arguments

- `active`: true when a poll is running, false otherwise.

When a poll is running, also:

- `poll.Id`, `poll.Title`, `poll.StartedAt`, `poll.EndsAt`, `poll.Duration` (seconds).
- `poll.DurationRemaining`: seconds left (never below 0).
- `poll.choices.count`: the number of choices.
- `poll.votes`: votes cast with votes, not channel points.
- `poll.rewardVotes`: votes cast with channel points.
- `poll.totalVotes`: both added together.
- For each choice, counted from 0: `poll.choice0.id`, `poll.choice0.title`, `poll.choice0.votes`, `poll.choice0.rewardVotes` and `poll.choice0.totalVotes` (then `poll.choice1.` and so on).

### Notes

- A poll that has ended is not returned. Only an active one is.
