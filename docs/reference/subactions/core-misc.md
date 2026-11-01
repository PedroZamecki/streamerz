# Core / Misc (sub-actions)

Small sub-actions: a note that does nothing when it runs, and a line for the log.

## Comment

- Kind: sub-action
- Code: 1009
- Type: Comment
- Category: Core/Misc
- Needs: nothing

A note kept in the action. It does nothing when it runs.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Comment | `value` | The note text. It can have several lines. |
| Color (#RRGGBB) | `color` | A colour in #RRGGBB form. Runs don't use it. |

### Arguments

None set.

## Log

- Kind: sub-action
- Code: 1020
- Type: LogEntry
- Category: Core/Misc
- Needs: nothing

Writes a line to the Streamerz log. Variables in the message are filled in first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Level | `logLevel` | Verbose, Debug, Information or Warning. Kept, but not used yet: every line is written at Information. |
| Message | `message` | The text to log. It can have several lines. |

### Arguments

None set.

### Notes

- The level is kept but not used yet. Every line is written at Information.
