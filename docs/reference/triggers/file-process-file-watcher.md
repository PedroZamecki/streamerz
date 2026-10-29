# File & Process / File Watcher (triggers)

Changes to the files and folders a file watcher is set up for.

## Changed

- Kind: trigger
- Code: 501
- Type: FileWatcherChanged
- Category: File & Process/File Watcher
- Needs: nothing

Meant to start its actions when a watched file is modified.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Watcher ID | `watcherId` | Only changes from this watcher. Empty: any watcher. |
| Path | `path` | Only changes to this path. Empty: any path. |

### Arguments

None set.

### When

It doesn't fire yet (see Notes).

### Notes

- Streamerz doesn't fire this yet.

## Created

- Kind: trigger
- Code: 502
- Type: FileWatcherCreated
- Category: File & Process/File Watcher
- Needs: nothing

Meant to start its actions when a new file appears in a watched folder.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Watcher ID | `watcherId` | Only changes from this watcher. Empty: any watcher. |
| Path | `path` | Only changes to this path. Empty: any path. |

### Arguments

None set.

### When

It doesn't fire yet (see Notes).

### Notes

- Streamerz doesn't fire this yet.

## Deleted

- Kind: trigger
- Code: 503
- Type: FileWatcherDeleted
- Category: File & Process/File Watcher
- Needs: nothing

Meant to start its actions when a file is deleted from a watched folder.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Watcher ID | `watcherId` | Only changes from this watcher. Empty: any watcher. |
| Path | `path` | Only changes to this path. Empty: any path. |

### Arguments

None set.

### When

It doesn't fire yet (see Notes).

### Notes

- Streamerz doesn't fire this yet.

## Renamed

- Kind: trigger
- Code: 504
- Type: FileWatcherRenamed
- Category: File & Process/File Watcher
- Needs: nothing

Meant to start its actions when a file in a watched folder is renamed.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Watcher ID | `watcherId` | Only changes from this watcher. Empty: any watcher. |
| Path | `path` | Only changes to this path. Empty: any path. |

### Arguments

None set.

### When

It doesn't fire yet (see Notes).

### Notes

- Streamerz doesn't fire this yet.
