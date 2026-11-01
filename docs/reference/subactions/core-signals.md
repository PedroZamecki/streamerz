# Core / Signals (sub-actions)

Local signals pass a name and a set of arguments from one action to another inside Streamerz.

## Send Local Signal

- Kind: sub-action
- Code: 1031
- Type: SendLocalSignal
- Category: Core/Signals
- Needs: nothing

Sends a signal by name. Each run waiting for it (Wait For Local Signal) gets the signal. If nothing is waiting and Queue if nobody waits is on, the signal is kept for the next wait.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Signal | `signalName` | The name of the signal. |
| Send the arguments | `useArge` | On: send the arguments of this run with the signal. |
| Arguments (name=value) | `customArgs` | Extra values sent with the signal. Variables in them are filled in. |
| Queue if nobody waits | `queueSignal` | On: keep the signal until a wait takes it. |

### Arguments

None set.

### Notes

- The key for Send the arguments is `useArge`, as it is saved.
- Kept signals are taken in the order they were sent. Each wait takes one.

## Wait For Local Signal

- Kind: sub-action
- Code: 1032
- Type: WaitForLocalSignal
- Category: Core/Signals
- Needs: nothing

Waits for a signal sent with Send Local Signal. If a kept signal is waiting, it is taken at once. Otherwise the run waits up to the timeout.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Signal | `signalName` | The name of the signal to wait for. |
| Overwrite arguments | `overwrite` | On: the signal's arguments replace the ones already set. Off: they only add names that aren't set yet. |
| Timeout (ms) | `timeout` | How long to wait, in milliseconds. Blank or over 30000 means 30 seconds. |

### Arguments

- `signalName`: the name of the signal waited for.
- `signalTimeout`: true when the wait ran out before a signal came.
- `hasResult`: true when a signal was received.
- Every argument the signal carried, added when it arrives. With Overwrite arguments off, the ones already set are kept.

### Notes

- When the wait runs out, a warning goes to the log.
- A timeout of 0 means 30 seconds, and a longer one is cut to 30 seconds.
