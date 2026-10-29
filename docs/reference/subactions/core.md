# Core (sub-actions)

Basic sub-actions that shape the flow of an action, such as waiting before the next step.

## Delay

- Kind: sub-action
- Code: 1002
- Type: DelayAction
- Category: Core
- Needs: nothing

Waits before the next sub-action runs. The wait is in milliseconds. With Random on, the wait is picked at random between the delay and the "Up to" value, when that value is larger.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Delay (ms) | `value` | How long to wait, in milliseconds. |
| Random | `random` | On: pick a random wait between `value` and `maxValue`. |
| Up to (ms) | `maxValue` | The longest wait. Used only when Random is on and it is larger than `value`. |

### Arguments

None set.

### Notes

- The wait holds up the rest of the action, so the sub-actions after it wait too.
