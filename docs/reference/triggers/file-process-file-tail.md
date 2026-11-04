# File & Process / File Tail (triggers)

New lines added to a file that a file tail follows (Files page), such as a game's log. Each enabled tail
waits on the system for the file to change; none runs when there are none. The arguments are
Streamer.bot 1.0.7's (checked against it with `dev/sb-probe.py`).

## Changed

- Kind: trigger
- Code: 511
- Type: FileTailChanged
- Category: File & Process/File Tail
- Needs: a file tail on the Files page

Starts its actions for each new line of the file.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| File tail | `tailId` | Only lines from this tail. Empty: any tail. |

### Arguments

- `filePath`: the file's path.
- `fileName`: its name.
- `line`: the new line, without its line break.
- Plus `triggerId`, `triggerName`, `triggerCategory` (`Core/File IO/File Tail`) and `eventSource` (`filetail`).

### When

Once a line is whole (its line break is written): a line written in parts fires once.

### Notes

- Lines already in the file when the tail starts don't fire. A file that doesn't exist yet is read from
  its start once it appears.
- A file that gets shorter (rewritten, rotated) is followed from its new end, as Streamer.bot does.
