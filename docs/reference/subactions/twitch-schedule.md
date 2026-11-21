# Twitch / Schedule (sub-actions)

Reads and changes the channel's stream schedule.

## Get Schedule

- Kind: sub-action
- Code: 99536
- Type: TwitchGetSchedule
- Category: Twitch/Schedule
- Needs: the broadcaster's Twitch login with more permissions (Accounts, Twitch, Allow more)

Reads the channel's stream schedule. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

None.

### Arguments

- `segmentCount`: how many segments were found.
- `segment0Id`, `segment0Start`, `segment0End`, `segment0Title`, `segment0Category`, `segment0Recurring`, and so on for each one (`segment1Id`...).
- `onVacation`: true while the schedule is on vacation.

## Add Schedule Segment

- Kind: sub-action
- Code: 99537
- Type: TwitchAddScheduleSegment
- Category: Twitch/Schedule
- Needs: the broadcaster's Twitch login with more permissions (Accounts, Twitch, Allow more)

Adds a stream to the channel's schedule. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Starts (2026-12-01 20:00) | `start` | When it starts: `2026-12-01 20:00` in this computer's time, or ISO 8601 with its zone. |
| Duration | `duration` | How long (30 minutes to 23 hours). |
| Title | `title` | The stream's title. Optional. |
| Category | `category` | The category, by name. Optional. |
| Every week | `recurring` | On: every week at that time. |

### Arguments

- `segmentId`: the new segment.

## Cancel Next Schedule Segment

- Kind: sub-action
- Code: 99538
- Type: TwitchCancelScheduleSegment
- Category: Twitch/Schedule
- Needs: the broadcaster's Twitch login with more permissions (Accounts, Twitch, Allow more)

Cancels the next stream on the schedule. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

None.

### Arguments

- `segmentId`: the segment cancelled.

## Set Vacation

- Kind: sub-action
- Code: 99539
- Type: TwitchSetVacation
- Category: Twitch/Schedule
- Needs: the broadcaster's Twitch login with more permissions (Accounts, Twitch, Allow more)

Puts the schedule on vacation between two dates, or ends the vacation. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Turn | `state` | On or Off. |
| Starts (2026-12-01 20:00) | `start` | From: `2026-12-20 00:00` in this computer's time. |
| Until (2027-01-05 00:00) | `end` | Until: `2027-01-05 00:00` in this computer's time. |

### Arguments

None set.
