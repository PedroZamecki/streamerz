# Core / Voice Control (sub-actions)

Sub-actions that keep voice control commands in Streamerz and turn them on or off.

## Set Voice Control Command

- Kind: sub-action
- Code: 1018
- Type: SetVoiceControlCommand
- Category: Core/Voice Control
- Needs: nothing

Sets the phrase of a voice command, by its id. A new command starts enabled.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command ID | `commandId` | The id of the command. |
| Command Phrase | `command` | The phrase to say. |

### Arguments

None set.

### Notes

- No speech recognition runs in Streamerz yet, so nothing reads these commands, and the Speech to Text Command trigger doesn't fire.

## Set Voice Control Command State

- Kind: sub-action
- Code: 1019
- Type: SetVoiceControlCommandState
- Category: Core/Voice Control
- Needs: nothing

Enables, disables or toggles a voice command, by its id. A command that isn't known yet is created with no phrase, then the state is applied.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command ID | `commandId` | The id of the command. |
| State | `state` | Enable (0), Disable (1) or Toggle (2). |

### Arguments

None set.

### Notes

- No speech recognition runs in Streamerz yet, so nothing reads the state.
