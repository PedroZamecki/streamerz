# Core / Action Queues (sub-actions)

Sub-actions that change action queues: pause or resume them, drop the runs waiting in them, and set whether they block.

## Set Action Queue Pause State

- Kind: sub-action
- Code: 1001
- Type: SetActionQueuePauseState
- Category: Core/Action Queues
- Needs: nothing

Pauses or resumes an action queue. While a queue is paused, its runs wait and don't start.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Queue (none: all) | `actionQueueId` | The queue id. Leave it empty to pause or resume every queue. |
| State | `state` | Pause (0) or Resume (1). |
| Clear the queue | `clear` | On: drop the runs waiting in the queue when it is paused. |

### Arguments

None set.

### Notes

- Empty queue id means every queue, the default one included.

## Clear Action Queue

- Kind: sub-action
- Code: 1005
- Type: ClearActionQueue
- Category: Core/Action Queues
- Needs: nothing

Drops the runs waiting in an action queue. Runs that already started are not touched.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Queue | `actionQueueId` | The queue id. |

### Arguments

None set.

### Notes

- The default queue has the id `00000000-0000-0000-0000-000000000000`.

## Set Action Queue Blocking State

- Kind: sub-action
- Code: 1022
- Type: SetActionQueueBlockingState
- Category: Core/Action Queues
- Needs: nothing

Sets whether an action queue blocks. On a blocking queue, a new run waits until the run that is going has finished.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Queue | `actionQueueId` | The queue id. |
| State | `state` | Blocking (0) or Non-blocking (1). |

### Arguments

None set.

### Notes

- The queue id has to match a queue that exists. An unknown id does nothing.
