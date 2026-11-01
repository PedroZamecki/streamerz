# Twitch / Ads (sub-actions)

Ads on Twitch: running a commercial, snoozing the next ad and reading the ad schedule.

## Run Commercial

- Kind: sub-action
- Code: 19
- Type: RunCommercial
- Category: Twitch/Ads
- Needs: a Twitch login as the channel's owner (the broadcaster)

Starts an ad break on the channel.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Duration (seconds) | `duration` | The length of the ad break, in seconds. |

### Arguments

None set.

### Notes

- A duration of 0 (or none) runs a 30 second ad break.

## Snooze Ad

- Kind: sub-action
- Code: 555
- Type: TwitchSnoozeAd
- Category: Twitch/Ads
- Needs: a Twitch login as the channel's owner (the broadcaster)

Snoozes the next ad, so it is pushed back.

### Fields

None.

### Arguments

- `snooze.success`: true when Twitch accepted the snooze, else false.

## Get Ad Schedule

- Kind: sub-action
- Code: 556
- Type: TwitchGetAdSchedule
- Category: Twitch/Ads
- Needs: a Twitch login as the channel's owner (the broadcaster)

Reads the ad schedule of the channel.

### Fields

None.

### Arguments

- `prerollFree`: true when the pre-roll free time is more than 0, else false.
- `prerollFreeSeconds`: the pre-roll free time, in seconds.
- `snoozeCount`: how many times the next ad has been snoozed.
- `adScheduleNextAdAt`: when the next ad runs, as Twitch gives it.

## Get Pre-roll Free Time

- Kind: sub-action
- Code: 557
- Type: TwitchGetPreRollFreeTime
- Category: Twitch/Ads
- Needs: a Twitch login as the channel's owner (the broadcaster)

Reads the pre-roll free time of the channel, the seconds without a pre-roll ad at the start of a viewer's stream.

### Fields

None.

### Arguments

- `prerollFree`: true when the pre-roll free time is more than 0, else false.
- `prerollFreeSeconds`: the pre-roll free time, in seconds.
