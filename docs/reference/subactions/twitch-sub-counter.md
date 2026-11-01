# Twitch / Sub Counter (sub-actions)

The sub counter: a running count of subs for the channel, kept by Streamerz. Twitch subs, resubs and gifted subs add to it.

## Get Twitch Sub Counter Count

- Kind: sub-action
- Code: 551
- Type: TwitchGetSubCounterCount
- Category: Twitch/Sub Counter
- Needs: nothing

Reads the sub counter and its rollover settings. It doesn't call Twitch.

### Fields

None.

### Arguments

- `subCounter.count`: the current count.
- `subCounter.rollover`: the count the counter starts again at (0 means it never does).
- `subCounter.rolloverCount`: how many times the counter has started again.

### Notes

- The count is kept in Streamerz, so it works without a Twitch login.

## Set Twitch Sub Counter Count

- Kind: sub-action
- Code: 552
- Type: TwitchSetSubCounterCount
- Category: Twitch/Sub Counter
- Needs: nothing

Sets the sub counter to a whole number. It doesn't call Twitch.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Value | `value` | A whole number. |

### Arguments

- `setResult`: true when the value was a whole number and the count was set, false otherwise.

### Notes

- A value that isn't a whole number leaves the count as it was.
