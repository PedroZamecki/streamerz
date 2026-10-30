# Twitch / Predictions (sub-actions)

Twitch predictions on the channel: read the one that is running, start one, cancel or lock it, and resolve it with the winning outcome. Predictions are the channel owner's, so the logged-in account must be the owner.

## Get Active Prediction

- Kind: sub-action
- Code: 567
- Type: TwitchGetActivePrediction
- Category: Twitch/Predictions
- Needs: a Twitch login. Twitch decides who may do it on a channel (its guide says moderators and editors can manage polls; the call's reference says the broadcaster); if it refuses, its answer is logged

Reads the prediction that is running, if there is one.

### Fields

None.

### Arguments

- `active`: true when a prediction is running, false otherwise.

When a prediction is running, also:

- `prediction.Id`, `prediction.Title`, `prediction.Status`, `prediction.CreatedAt`, `prediction.PredictionWindow`.
- For each outcome, counted from 0: `prediction.outcome0.id`, `prediction.outcome0.title`, `prediction.outcome0.color`, `prediction.outcome0.users` and `prediction.outcome0.points` (then `prediction.outcome1.` and so on).

### Notes

- Only an active prediction is returned.

## Create Prediction

- Kind: sub-action
- Code: 568
- Type: TwitchCreatePrediction
- Category: Twitch/Predictions
- Needs: a Twitch login. Twitch decides who may do it on a channel (its guide says moderators and editors can manage polls; the call's reference says the broadcaster); if it refuses, its answer is logged

Starts a prediction on the channel.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Title | `title` | The prediction's question. |
| Prediction window (seconds) | `predictionWindow` | A whole number. |
| Outcomes (one per line) | `outcomes` | One outcome per line. |

### Arguments

- `success`: true when Twitch created the prediction, false otherwise.
- `prediction.Id`: the new prediction's id (only when it was created).

### Notes

- The window is kept between 30 and 1800 seconds.
- Variables like `%user%` in the title and outcomes are filled in first.

## Cancel Active Prediction

- Kind: sub-action
- Code: 569
- Type: TwitchCancelActivePrediction
- Category: Twitch/Predictions
- Needs: a Twitch login. Twitch decides who may do it on a channel (its guide says moderators and editors can manage polls; the call's reference says the broadcaster); if it refuses, its answer is logged

Cancels the prediction that is running.

### Fields

None.

### Arguments

- `active`: true when a prediction was found, false otherwise.
- `cancelled`: true once Twitch cancelled it.
- The `prediction.*` values of the prediction (the same as Get Active Prediction).

### Notes

- It acts on the first prediction that is active or locked. When there is none, an error goes to the log.

## Lock Active Prediction

- Kind: sub-action
- Code: 570
- Type: TwitchLockActivePrediction
- Category: Twitch/Predictions
- Needs: a Twitch login. Twitch decides who may do it on a channel (its guide says moderators and editors can manage polls; the call's reference says the broadcaster); if it refuses, its answer is logged

Locks the active prediction, so no more votes come in.

### Fields

None.

### Arguments

- `active`: true when an active prediction was found, false otherwise.
- `locked`: true once Twitch locked it.
- The `prediction.*` values of the prediction (the same as Get Active Prediction).

### Notes

- It acts only on an active prediction. A locked one is not locked again. When there is none, an error goes to the log.

## Resolve Last Prediction

- Kind: sub-action
- Code: 571
- Type: TwitchResolveLastPrediction
- Category: Twitch/Predictions
- Needs: a Twitch login. Twitch decides who may do it on a channel (its guide says moderators and editors can manage polls; the call's reference says the broadcaster); if it refuses, its answer is logged

Resolves the prediction with the winning outcome, picked by its position.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Winning outcome index | `winningIndex` | A whole number, counted from 0. |

### Arguments

- `success`: true once the prediction is resolved, false otherwise.
- `active` and the `prediction.*` values of the prediction.
- `prediction.winningIndex`: the winning outcome's position.
- `prediction.winningOutcome.id`, `.title`, `.color`, `.users` and `.points`: the winning outcome.

### Notes

- The index is counted from 0, so the first outcome is 0. An empty value means 0.
- It acts on the first prediction that is active or locked.
- When the index is not an outcome, nothing is resolved and `success` stays false.

## Resolve Last Prediction By Title

- Kind: sub-action
- Code: 572
- Type: TwitchResolveLastPredictionByTitle
- Category: Twitch/Predictions
- Needs: a Twitch login. Twitch decides who may do it on a channel (its guide says moderators and editors can manage polls; the call's reference says the broadcaster); if it refuses, its answer is logged

Resolves the prediction with the winning outcome, picked by its title.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Winning outcome title | `outcomeTitle` | The outcome's title. |

### Arguments

The same as Resolve Last Prediction.

### Notes

- The title is matched without regard to upper or lower case.
- It acts on the first prediction that is active or locked.
- When no outcome has that title, nothing is resolved and `success` stays false.
