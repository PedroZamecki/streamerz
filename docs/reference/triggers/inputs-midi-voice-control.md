# Inputs & MIDI / Voice Control (triggers)

Speech recognition for the voice control commands, and the text it hears.

## Speech To Text Dictation

- Kind: trigger
- Code: 301
- Type: SpeechToTextDictation
- Category: Inputs & MIDI/Voice Control
- Needs: nothing

Meant to start its actions for each phrase the speech recognizer dictates.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Culture | `culture` | Only phrases in this culture (for example `en-US`). Empty: any. |

### Arguments

None set.

### When

It doesn't fire yet (see Notes).

### Notes

- Streamerz doesn't fire this yet.

## Speech To Text Command

- Kind: trigger
- Code: 302
- Type: SpeechToTextCommand
- Category: Inputs & MIDI/Voice Control
- Needs: nothing

Meant to start its actions when a voice control command is recognized.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command ID | `commandId` | Only this voice control command. Empty: any. |

### Arguments

None set.

### When

It doesn't fire yet (see Notes).

### Notes

- Streamerz doesn't fire this yet.
