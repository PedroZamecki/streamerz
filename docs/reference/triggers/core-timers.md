# Core / Timers (triggers)

Triggers that run on a set interval. Timers are set up on the Timers page.

## Timed Action

- Kind: trigger
- Code: 701
- Type: TimedAction
- Category: Core/Timers
- Needs: nothing

Fires on an interval. A timer has an interval in seconds, an optional upper interval for a random wait, an optional number of chat lines it waits for, and a repeat switch.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Timer | `timerId` | The timer to fire on. Other timers don't start this trigger's actions. |

### Arguments

- `timerId`: the timer's id.
- `timerName`: the timer's name.
- `counter`: how many times the timer has fired, this time included.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time the timer's wait runs out. If the timer waits for chat lines, it fires only once enough lines have come in since its last run. Timers run only while they are enabled.

### Notes

- A timer that doesn't repeat fires once, then turns itself off.
- With a random interval, each wait is picked between the interval and the upper interval.
- Timers start with Streamerz. Changing the timers restarts all of them.
