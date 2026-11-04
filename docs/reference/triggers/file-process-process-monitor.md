# File & Process / Process Monitor (triggers)

Other programs starting and stopping on this computer. The list of processes is compared every 2 seconds,
and only while an enabled action uses one of these triggers: nothing runs otherwise. (No system gives an
unprivileged event for it; Streamer.bot's own watcher polls Windows too.) The arguments are those
Streamer.bot documents.

## Started

- Kind: trigger
- Code: 703
- Type: ProcessStarted
- Category: File & Process/Process Monitor
- Needs: nothing

Starts its actions when a program starts. The name is matched without caring about case, with or without `.exe`; an empty name matches
any program.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Process Name | `processName` | The program's name, such as `obs64` or `obs64.exe`. Empty matches any program. |

### Arguments

- `name`: the program's name without `.exe` (`obs64`).
- `executable`: its file name (`obs64.exe`).
- `path`: the folder it runs from; `fullPath`: the executable's full path (empty when the system won't say).
- `processId`, `parentProcessId`: its id and the id of the program that started it (as text).
- Plus `triggerId`, `triggerName`, `triggerCategory` (`Core/Processes`) and `eventSource` (`processes`).

### When

Within 2 seconds of it happening.

## Stopped

- Kind: trigger
- Code: 704
- Type: ProcessStopped
- Category: File & Process/Process Monitor
- Needs: nothing

Starts its actions when a program ends. The name is matched without caring about case, with or without `.exe`; an empty name matches
any program.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Process Name | `processName` | The program's name, such as `obs64` or `obs64.exe`. Empty matches any program. |

### Arguments

- `name`: the program's name without `.exe` (`obs64`).
- `executable`: its file name (`obs64.exe`).
- `path`: the folder it runs from; `fullPath`: the executable's full path (empty when the system won't say).
- `processId`, `parentProcessId`: its id and the id of the program that started it (as text).
- Plus `triggerId`, `triggerName`, `triggerCategory` (`Core/Processes`) and `eventSource` (`processes`).

### When

Within 2 seconds of it happening.
