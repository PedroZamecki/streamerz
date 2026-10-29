# Core / System (sub-actions)

Sub-actions that run programs and press keys on the computer, and show notifications.

## Run a Program

- Kind: sub-action
- Code: 6
- Type: RunExecutable
- Category: Core/System
- Needs: nothing

Runs a program with arguments.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command | `command` | The program to run. |
| Arguments | `arguments` | The arguments. On Linux, spaces separate them and double quotes group words. |
| Working directory | `workingDir` | The folder to run in. Empty uses the folder of the program. |
| Wait for exit (s) | `waitForExit` | How many seconds to wait for the program to end. 0 runs it and moves on. |
| Environment (NAME=value) | `envVars` | Extra environment variables for the program. |

### Arguments

- The program gets the arguments of the action as JSON on its standard input.
- With Wait for exit above 0, the program's output is read after it ends:
  - `outputCount`: the number of output lines.
  - `output0`, `output1`... : each output line.
  - A JSON object in the output adds its keys as arguments. Lines like `key=value` add their keys as arguments too.

### Notes

- On Linux the arguments are split as described above. On Windows the text is passed to the program as it is.
- With Wait for exit above 0, a program that doesn't end in time is logged as an error.

## Keyboard Press

- Kind: sub-action
- Code: 1012
- Type: KeyboardPress
- Category: Core/System
- Needs: nothing

Presses a key, with optional modifier keys.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Key | `key` | The key to press. |
| Modifiers | `modifiers` | A number of flags added together: 1 Alt, 2 Control, 4 Shift, 8 Windows. |

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the sub-action is read and logged, but no key is sent to the computer.

## Toast Notification

- Kind: sub-action
- Code: 1024
- Type: ToastNotification
- Category: Core/System
- Needs: nothing

Shows a notification. Streamerz doesn't show it yet.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
