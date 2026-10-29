# Twitch / Polls & Predictions (triggers)

Events for the channel's polls and predictions, as Twitch's EventSub sends them.

## Poll Created

- Kind: trigger
- Code: 125
- Type: TwitchPollCreated
- Category: Twitch/Polls & Predictions
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

Starts when a poll begins on the channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `pollId`, `pollTitle`, `status`: the poll's id, title and status (`active` when Twitch sends none).
- `active`: true.
- `poll.Id`, `poll.Title`, `poll.StartedAt`, `poll.EndsAt`, `poll.Duration` (seconds), `poll.DurationRemaining` (seconds).
- `poll.votes`, `poll.rewardVotes`, `poll.totalVotes`, `poll.choices.count`: the totals.
- `poll.choice0.id`, `poll.choice0.title`, `poll.choice0.votes`, `poll.choice0.rewardVotes`, `poll.choice0.totalVotes`, and the same with `choice1`, `choice2`... for each choice.
- `poll._json`: all of the above as one JSON text.

### When

When a poll starts on the channel (Twitch's poll begin event).

## Poll Updated

- Kind: trigger
- Code: 126
- Type: TwitchPollUpdated
- Category: Twitch/Polls & Predictions
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A running poll has new numbers.

### Fields

None.

### Arguments

The same as Poll Created.

### When

Each progress event Twitch sends for a running poll (Twitch's poll progress event).

### Notes

- Every progress event fires it, so one poll can fire it several times.

## Poll Completed

- Kind: trigger
- Code: 127
- Type: TwitchPollCompleted
- Category: Twitch/Polls & Predictions
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A poll has ended with its final results.

### Fields

None.

### Arguments

The same as Poll Created, with `active` set to false, and:

- `poll.endedAt`: when the poll ended.
- `poll.winningIndex`: the position of the winning choice.
- `poll.winningChoice.id`, `poll.winningChoice.title`: the winning choice.
- `poll.winningChoice.votes`: its votes, without the channel points votes.
- `poll.winningChoice.rewardVotes`: its channel points votes.
- `poll.winningChoice.totalVotes`: all its votes.

### When

When the poll end event comes with a status other than `terminated` or `archived`.

### Notes

- The winner is the choice with the most votes, channel points votes included.

## Poll Terminated

- Kind: trigger
- Code: 161
- Type: TwitchPollTerminated
- Category: Twitch/Polls & Predictions
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A poll was ended before its time ran out.

### Fields

None.

### Arguments

The same as Poll Completed.

### When

When the poll end event has the status `terminated`. Poll Completed does not fire for it.

## Poll Archived

- Kind: trigger
- Code: 473
- Type: TwitchPollArchived
- Category: Twitch/Polls & Predictions
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A poll was archived.

### Fields

None.

### Arguments

The same as Poll Completed.

### When

When the poll end event has the status `archived`. Poll Completed does not fire for it.

## Prediction Created

- Kind: trigger
- Code: 128
- Type: TwitchPredictionCreated
- Category: Twitch/Polls & Predictions
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A prediction has started and viewers can bet on it.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `predictionId`, `predictionTitle`, `status`: the prediction's id, title and status.
- `winningOutcomeId`: empty until the prediction is resolved.
- `active`: true while the prediction is active.
- `prediction.Id`, `prediction.Title`, `prediction.Status`, `prediction.CreatedAt`: the prediction's details.
- `prediction.PredictionWindow` (seconds): from its start to its lock.
- `prediction.LockedAt`, `prediction.EndedAt`: when it was locked and ended.
- `prediction.outcome0.id`, `prediction.outcome0.title`, `prediction.outcome0.color`, `prediction.outcome0.users`, `prediction.outcome0.points`, and the same with `outcome1`, `outcome2`... for each outcome.
- `prediction._json`: all of the above as one JSON text.

### When

When a prediction begins (Twitch's prediction begin event).

## Prediction Updated

- Kind: trigger
- Code: 129
- Type: TwitchPredictionUpdated
- Category: Twitch/Polls & Predictions
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A running prediction has new numbers.

### Fields

None.

### Arguments

The same as Prediction Created.

### When

Each progress event Twitch sends for a running prediction (Twitch's prediction progress event).

## Prediction Locked

- Kind: trigger
- Code: 132
- Type: TwitchPredictionLocked
- Category: Twitch/Polls & Predictions
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

Betting on a prediction has closed.

### Fields

None.

### Arguments

The same as Prediction Created.

### When

When the prediction lock event comes (Twitch's prediction lock event).

## Prediction Completed

- Kind: trigger
- Code: 130
- Type: TwitchPredictionCompleted
- Category: Twitch/Polls & Predictions
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A prediction has ended with a result.

### Fields

None.

### Arguments

The same as Prediction Created, with:

- `prediction.winningIndex`: the position of the winning outcome.
- `prediction.winningOutcome.id`, `prediction.winningOutcome.title`, `prediction.winningOutcome.color`, `prediction.winningOutcome.users`, `prediction.winningOutcome.points`: the winning outcome.

### When

When the prediction end event comes with a status other than `canceled`.

### Notes

- The `prediction.winningOutcome.*` arguments are only set when the winning outcome is in the event.

## Prediction Canceled

- Kind: trigger
- Code: 131
- Type: TwitchPredictionCanceled
- Category: Twitch/Polls & Predictions
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A prediction was canceled before it was resolved.

### Fields

None.

### Arguments

The same as Prediction Created.

### When

When the prediction end event has the status `canceled`. Prediction Completed does not fire for it.
