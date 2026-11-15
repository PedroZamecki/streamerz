# Every Platform / User (sub-actions)

Looks people up on each platform picked, or on the one the trigger came from.

## Get User Info for Target

- Kind: sub-action
- Code: 99713
- Type: AllGetUserInfoForTarget
- Category: Every Platform/User
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Gives the target values (`targetUser`...) for someone, on each platform picked. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platforms | `platforms` | The platforms to act on, ticked. None ticked: the platform the trigger came from. |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |

### Arguments

- Those of each platform's own Get User Info for Target (`targetUser`, `targetUserId`...).
