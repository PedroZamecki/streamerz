# Core / File Tail (sub-actions)

Sub-actions for file tails, the watches on new lines added to a file that the File Tail Changed trigger is about.

## Set File Tail State

- Kind: sub-action
- Code: 1030
- Type: SetFileTailState
- Category: Core/File Tail
- Needs: nothing

Enables, disables or toggles a file tail (Files page), by its id. The change is saved and the tail starts or stops at once.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| File Tail ID | `fileTailId` | The id of the file tail. |
| State | `state` | Enable (0), Disable (1) or Toggle (2). |

### Arguments

None set.

### Notes

- An id that matches no file tail does nothing.
