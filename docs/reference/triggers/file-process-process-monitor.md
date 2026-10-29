# File & Process / Process Monitor (triggers)

Triggers for other programs starting and stopping on this computer.

## Started

- Kind: trigger
- Code: 703
- Type: ProcessStarted
- Category: File & Process/Process Monitor
- Needs: nothing

Starts an action when a program starts. The name is matched without caring about case. An empty name matches any program.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Process Name | `processName` | The program's file name, such as `obs64.exe`. Empty matches any program. |

### Arguments

None yet (see Notes).

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.
- The code that will fire it sets `processName` and `processId`.

## Stopped

- Kind: trigger
- Code: 704
- Type: ProcessStopped
- Category: File & Process/Process Monitor
- Needs: nothing

Starts an action when a program exits. The name is matched without caring about case. An empty name matches any program.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Process Name | `processName` | The program's file name, such as `obs64.exe`. Empty matches any program. |

### Arguments

None yet (see Notes).

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.
- The code that will fire it sets `processName`, `processId` and `exitCode`.
