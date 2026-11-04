# Core / Voice Control (sub-actions)

Sub-actions that keep voice control commands in Streamerz and turn them on or off.

## Set Voice Control Command

- Kind: sub-action
- Code: 1018
- Type: SetVoiceControlCommand
- Category: Core/Voice Control
- Needs: nothing

Sets the phrases of a voice command from the Voice commands list (Commands page), by its id.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command ID | `commandId` | The id of the command. |
| Command Phrase | `command` | The phrase to say (one per line for several). |

### Arguments

None set.

### Notes

- An id that isn't in the list changes nothing.

## Set Voice Control Command State

- Kind: sub-action
- Code: 1019
- Type: SetVoiceControlCommandState
- Category: Core/Voice Control
- Needs: nothing

Enables, disables or toggles a voice command from the Voice commands list, by its id. A disabled command doesn't fire Speech to Text Command.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command ID | `commandId` | The id of the command. |
| State | `state` | Enable (0), Disable (1) or Toggle (2). |

### Arguments

None set.

### Notes

- An id that isn't in the list changes nothing.
