# File & Process / File Tail (triggers)

New lines added to the log files that a file tail follows.

## Changed

- Kind: trigger
- Code: 511
- Type: FileTailChanged
- Category: File & Process/File Tail
- Needs: nothing

Meant to start its actions when new lines are appended to a followed log file.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Tail ID | `tailId` | Only lines from this tail. Empty: any tail. |
| Path | `path` | Only lines from this file. Empty: any file. |

### Arguments

None set.

### When

It doesn't fire yet (see Notes).

### Notes

- Streamerz doesn't fire this yet.
