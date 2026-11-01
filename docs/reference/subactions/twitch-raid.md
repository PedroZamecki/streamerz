# Twitch / Raid (sub-actions)

Raids from your Twitch channel to another one. Both need the channel's owner to be logged in.

## Start Raid

- Kind: sub-action
- Code: 500
- Type: Start Raid
- Category: Twitch/Raid
- Needs: a Twitch login as the channel's owner (the broadcaster)

Starts a raid to another channel.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the channel to raid. Variables are filled in first. |

### Arguments

Adds `startRaidSuccess`: true when Twitch accepted the raid, false when it didn't.

## Cancel Raid

- Kind: sub-action
- Code: 501
- Type: Cancel Raid
- Category: Twitch/Raid
- Needs: a Twitch login as the channel's owner (the broadcaster)

Cancels the raid that's started.

### Fields

None.

### Arguments

None set.

### Notes

- If Twitch refuses the call, the error is logged.
