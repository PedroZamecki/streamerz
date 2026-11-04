# Inputs & MIDI / Voice Control (triggers)

Speech recognition for the voice control commands, and the text it hears.

## Speech To Text Dictation

- Kind: trigger
- Code: 301
- Type: SpeechToTextDictation
- Category: Inputs & MIDI/Voice Control
- Needs: nothing

Starts its actions for each phrase the speech recognizer hears.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Culture | `culture` | Only phrases in this culture (for example `en-US`). Empty: any. |

### Arguments

| Argument | What it holds |
| --- | --- |
| `spokenText` | What was heard. |
| `spokenTextConfidence` | How sure the recognizer was, 0 to 1. |
| `spokenTextConfidencePercent` | The same, 0 to 100. |

### When

Each phrase heard, once it ends.

### Notes

- Windows only: it uses Windows' own speech recognizer (SAPI dictation) and the default microphone. Linux has no system recognizer, so there it logs a warning and nothing listens.
- The recognizer runs only while an enabled action uses Speech to Text Dictation or Command.
- The culture field is kept but not used: the recognizer listens in Windows' language.
- Built from SAPI's documentation; not checked on a real Windows yet (Wine has no recognizer). The engine side is covered by the speech scenario.

## Speech To Text Command

- Kind: trigger
- Code: 302
- Type: SpeechToTextCommand
- Category: Inputs & MIDI/Voice Control
- Needs: nothing

Starts its actions when a phrase heard says one of the voice commands (Commands page), with enough confidence.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command ID | `commandId` | Only this voice control command. Empty: any. |

### Arguments

| Argument | What it holds |
| --- | --- |
| `spokenText` | What was heard. |
| `spokenTextConfidence` | How sure the recognizer was, 0 to 1. |
| `spokenTextConfidencePercent` | The same, 0 to 100. |
| `spokenCommand` | The phrase of the command that matched (lowercase). |
| `spokenTextInput` | What was said after the phrase (commands said at the start). |
| `commandId` | The id of the voice command. |
| `commandName` | Its name. |

### When

A phrase heard is the command's phrase (Said: the whole phrase), starts with it (at the start) or holds it (anywhere), and the recognizer was at least as sure as the global confidence (Advanced), or the command's own when it overrides it.

### Notes

- Windows only: it uses Windows' own speech recognizer (SAPI dictation) and the default microphone. Linux has no system recognizer, so there it logs a warning and nothing listens.
- The recognizer runs only while an enabled action uses Speech to Text Dictation or Command.
- Built from SAPI's documentation; not checked on a real Windows yet (Wine has no recognizer). The engine side is covered by the speech scenario.
