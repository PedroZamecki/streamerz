# Core / Timers (sub-actions)

Sub-actions that turn timers on and off.

## Set Timer State

- Kind: sub-action
- Code: 8
- Type: SetTimerState
- Category: Core/Timers
- Needs: nothing

Turns a timer on, off, or toggles it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Timer | `timerId` | The timer to change. |
| State | `state` | Enable, Disable or Toggle. |

### Arguments

None set.

### Notes

- The timers are updated as soon as the sub-action runs.
