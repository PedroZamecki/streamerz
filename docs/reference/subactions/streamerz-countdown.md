# Streamerz / Countdown (sub-actions)

## Countdown (Streamerz)

- Kind: sub-action
- Code: 99803
- Type: StreamerzCountdown
- Category: Core/Timers
- Needs: nothing; a reset makes the countdown

Adds time to a countdown or resets it. This Streamerz-specific sub-action is not implemented by
Streamer.bot. The countdown never exceeds its maximum.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Countdown ID | `clockId` | The countdown's ID. |
| Operation | `operation` | `add` or `reset`. |
| Starting time | `startMs` | The reset value in milliseconds, or a variable such as `%group.start%`. |
| Maximum time | `maxMs` | The maximum remaining time in milliseconds, or a variable such as `%group.max%`. |
| Time per unit | `perUnitMs` | The time added per unit, in milliseconds, or a variable. |
| Units | `units` | A number or variable, such as `%donationAmount%`. |
| Currency | `expectedCurrency` | If set, only matching donation currency is counted. |

### Arguments

No new action values. The step updates the countdown identified by Countdown ID. A matching currency
is required when Currency is set; otherwise it leaves the countdown unchanged.

### Example

Reset with Starting time `600000` and Maximum time `3600000`, then add `30000` milliseconds per unit
with Units `%donationAmount%`. The countdown stays at or below one hour.
