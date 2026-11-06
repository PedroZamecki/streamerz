# Core / File IO (sub-actions)

Sub-actions that write and read text files, and check whether a file or folder exists.

## Write To File

- Kind: sub-action
- Code: 3
- Type: WriteToFile
- Category: Core/File IO
- Needs: nothing

Writes text to a file. The file is replaced, or the text is added to its end.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| File | `file` | The path of the file. |
| Text | `text` | The text to write. Several lines are allowed. |
| Append | `append` | On: add the text to the end of the file instead of replacing it. |

### Arguments

None set.

### Notes

- The folders on the way to the file are created when they are missing.
- Append starts a new line first when the file is already there (even empty), as Streamer.bot does:
  `\r\n` on Windows, `\n` on Linux. A file it makes starts with the text.
- Variables in the path and the text are filled in first.
- A failed write is reported in the log and the action's execution result.

## Read Random Line From File

- Kind: sub-action
- Code: 5
- Type: ReadRandomLineFromFile
- Category: Core/File IO
- Needs: nothing

Puts a random line of a text file in a variable.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| File | `file` | The path of the file. |
| Parse Variables | `parseVariables` | On: variables in the line are filled in. |
| Auto Type | `autoType` | On: a line that looks like a number or a boolean is typed as one. |
| Variable Name | `variableName` | The variable that gets the line. |

### Arguments

- The variable named by `variableName` holds the line. It is not set when the file has no lines or can't be read.

### Notes

- Older imported Overwrite and Count values are preserved for export. Streamerz does not use them,
  so they are not offered as controls. JSON editing still exposes the preserved data.

## Read Lines From File

- Kind: sub-action
- Code: 1006
- Type: ReadLinesFromFile
- Category: Core/File IO
- Needs: nothing

Reads every line of a text file.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| File | `file` | The path of the file. |
| Parse Variables | `parseVariables` | On: variables in each line are filled in. |
| Auto Type | `autoType` | On: a line that looks like a number or a boolean is typed as one. |
| Variable Name | `variableName` | The variable that gets the list of lines. Can be left empty. |

### Arguments

- `fileFound`: true when the file could be read.
- `lineCount`: the number of lines.
- `line0`, `line1`... : each line, as read.
- With a variable name set: the variable holds the list of lines, `variable.count` holds the number, and `variable[0]`, `variable[1]`... hold each line.

### Notes

- The `line0` style values are the raw lines. Parse Variables and Auto Type only apply to the list in the variable.

## Read Specific Line From File

- Kind: sub-action
- Code: 1021
- Type: ReadSpecificLineFromFile
- Category: Core/File IO
- Needs: nothing

Reads one line of a text file by its number.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| File | `file` | The path of the file. |
| Parse Variables | `parseVariables` | On: variables in the line are filled in. |
| Auto Type | `autoType` | On: a line that looks like a number or a boolean is typed as one. |
| Line Number | `count` | The line to read. The first line is 1. |
| Variable Name | `variableName` | The variable that gets the line. Empty uses `line`. |

### Arguments

- `fileFound`: true when the file exists.
- The variable (`line` when none is set) holds the line. It is not set when the number is out of range or the file can't be read.

## File Exists

- Kind: sub-action
- Code: 1039
- Type: FileExists
- Category: Core/File IO
- Needs: nothing

Checks whether a file exists.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| File | `file` | The path to check. |
| Variable Name | `variableName` | The variable that gets true or false. Empty uses `fileExists`. |

### Arguments

- The variable (`fileExists` when none is set) is true when the path is a file.

## Folder Exists

- Kind: sub-action
- Code: 1040
- Type: FolderExists
- Category: Core/File IO
- Needs: nothing

Checks whether a folder exists.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Folder | `folder` | The path to check. |
| Variable Name | `variableName` | The variable that gets true or false. Empty uses `folderExists`. |

### Arguments

- The variable (`folderExists` when none is set) is true when the path is a folder.
