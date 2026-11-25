# Every Platform / Moderation (sub-actions)

Moderation that acts on each platform picked, or on the one the trigger came from.

## Remove Timeout

- Kind: sub-action
- Code: 99726
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
