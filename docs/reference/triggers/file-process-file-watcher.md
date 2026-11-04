# File & Process / File Watcher (triggers)

Changes to the files a file watcher is set up for (Files page: a folder or one file, a filter such as
`*.txt`, and whether folders inside count). Each enabled watcher waits on the system (inotify on Linux,
ReadDirectoryChangesW on Windows); none runs when there are none. The arguments are Streamer.bot 1.0.7's
(checked against it with `dev/sb-probe.py`).

## Changed

- Kind: trigger
- Code: 501
- Type: FileWatcherChanged
- Category: File & Process/File Watcher
- Needs: a file watcher on the Files page

Starts its actions when a watched file is written.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Watcher | `watcherId` | Only changes from this watcher. Empty: any watcher. |

### Arguments

- `watcherFolder`: the watcher's folder (or file).
- `watcherFilter`: its filter.
- `fullPath`: the file's full path.
- `fileName`: its name, with the extension.
- `changeType`: `Changed`.
- `fileWasDeleted`: true when the file was gone by the time it was read.
- `fileSize`: its size in bytes; `empty`: whether it is empty.
- With "Add the file's lines": `lineCount`, `line0`.. and `lineEscaped0`.. (URL-encoded, `+` for spaces).
- With "Read the file as JSON": its values, flattened (`a.b`, `list[0]`), in place of the lines.
- Plus `triggerId`, `triggerName`, `triggerCategory` (`Core/File/Folder Watcher`) and `eventSource` (`filewatcher`).

### When

After writes to the file settle (writes a tenth of a second apart are one change).

### Notes

- A file made empty (`touch`) fires only Created; a folder fires nothing.

## Created

- Kind: trigger
- Code: 502
- Type: FileWatcherCreated
- Category: File & Process/File Watcher
- Needs: a file watcher on the Files page

Starts its actions when a file appears in a watched folder.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Watcher | `watcherId` | Only changes from this watcher. Empty: any watcher. |

### Arguments

- `watcherFolder`: the watcher's folder (or file).
- `watcherFilter`: its filter.
- `fullPath`: the file's full path.
- `fileName`: its name, with the extension.
- `changeType`: `Created`.
- Plus `triggerId`, `triggerName`, `triggerCategory` (`Core/File/Folder Watcher`) and `eventSource` (`filewatcher`).

### When

When the file is made or moved in.

## Deleted

- Kind: trigger
- Code: 503
- Type: FileWatcherDeleted
- Category: File & Process/File Watcher
- Needs: a file watcher on the Files page

Starts its actions when a file is deleted from a watched folder.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Watcher | `watcherId` | Only changes from this watcher. Empty: any watcher. |

### Arguments

- `watcherFolder`: the watcher's folder (or file).
- `watcherFilter`: its filter.
- `fullPath`: the file's full path.
- `fileName`: its name, with the extension.
- `changeType`: `Deleted`.
- Plus `triggerId`, `triggerName`, `triggerCategory` (`Core/File/Folder Watcher`) and `eventSource` (`filewatcher`).

### When

When the file is deleted or moved out.

## Renamed

- Kind: trigger
- Code: 504
- Type: FileWatcherRenamed
- Category: File & Process/File Watcher
- Needs: a file watcher on the Files page

Starts its actions when a file in a watched folder is renamed.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Watcher | `watcherId` | Only changes from this watcher. Empty: any watcher. |

### Arguments

- `watcherFolder`, `watcherFilter`.
- `oldFullPath`, `oldFilename`: the path and name before.
- `fullPath`, `fileName`: after.
- `changeType`: `Renamed`.
- Plus `triggerId`, `triggerName`, `triggerCategory` (`Core/File/Folder Watcher`) and `eventSource` (`filewatcher`).

### When

When the file is renamed (or moved within the folder).
