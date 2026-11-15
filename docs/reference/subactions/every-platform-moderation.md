# Every Platform / Moderation (sub-actions)

Moderation that acts on each platform picked, or on the one the trigger came from.

## Timeout User

- Kind: sub-action
- Code: 99705
- Type: AllTimeoutUser
- Category: Every Platform/Moderation
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Times someone out on each platform picked (Twitch, Kick, YouTube). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platforms | `platforms` | The platforms to act on, ticked. None ticked: the platform the trigger came from. |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |
| Duration | `duration` | How long, in human units such as `10m` or `1h`. |
| Reason | `reason` | Why, for the moderators. Optional. |

### Arguments

- Those of each platform's own Timeout User (`timeoutResult`...).

### Notes

- YouTube finds the person by the name its chat saw them with.

## Remove Timeout

- Kind: sub-action
- Code: 99706
- Type: AllRemoveTimeout
- Category: Every Platform/Moderation
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Ends someone's timeout on each platform picked (Twitch, Kick, YouTube when Streamerz timed them out). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platforms | `platforms` | The platforms to act on, ticked. None ticked: the platform the trigger came from. |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |

### Arguments

- Those of each platform's own UnTimeout User.

## Ban User

- Kind: sub-action
- Code: 99707
- Type: AllBanUser
- Category: Every Platform/Moderation
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Bans someone on each platform picked (Twitch, Kick, YouTube). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platforms | `platforms` | The platforms to act on, ticked. None ticked: the platform the trigger came from. |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |
| Reason | `reason` | Why, for the moderators. Optional. |

### Arguments

- Those of each platform's own Ban User (`banResult`...).

### Notes

- YouTube finds the person by the name its chat saw them with.

## Unban User

- Kind: sub-action
- Code: 99708
- Type: AllUnbanUser
- Category: Every Platform/Moderation
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Unbans someone on each platform picked (Twitch, Kick, YouTube when Streamerz banned them). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platforms | `platforms` | The platforms to act on, ticked. None ticked: the platform the trigger came from. |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |

### Arguments

- Those of each platform's own Unban User.
