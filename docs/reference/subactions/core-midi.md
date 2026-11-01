# Core / MIDI (sub-actions)

Sends MIDI messages to a MIDI device.

## Note On

- Kind: sub-action
- Code: 9001
- Type: MidiOutNoteOn
- Category: Core/MIDI
- Needs: nothing

Builds a MIDI Note On message with a channel, note, velocity and duration. The device name is filled in with variables first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Device | `device` | Text. The MIDI device name. |
| Name | `name` | Text. A label. |
| Send Note Off | `sendNoteOff` | On or off. Whether a Note Off follows. |
| Channel | `channel` | Whole number. |
| Note | `note` | Whole number. |
| Velocity | `velocity` | Whole number. |
| Duration (ms) | `duration` | Whole number. How long the note lasts. |

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the message is only logged, no MIDI device gets it.
- With Send Note Off on, the log says the note would be turned off after the duration. No Note Off is sent.

## Control Change

- Kind: sub-action
- Code: 9002
- Type: MidiOutControlChange
- Category: Core/MIDI
- Needs: nothing

Builds a MIDI Control Change message with a channel, controller and value. The device name is filled in with variables first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Device | `device` | Text. The MIDI device name. |
| Name | `name` | Text. A label. |
| Channel | `channel` | Whole number. |
| Controller | `controller` | Whole number. |
| Value | `value` | Whole number. |

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the message is only logged, no MIDI device gets it.

## Generic Event

- Kind: sub-action
- Code: 9003
- Type: MidiOutGeneric
- Category: Core/MIDI
- Needs: nothing

Builds a MIDI message from a command name, a channel and two data values. The device name and command are filled in with variables first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Device | `device` | Text. The MIDI device name. |
| Name | `name` | Text. A label. |
| Command | `command` | Text. For example `PitchBend`. |
| Channel | `channel` | Whole number. |
| Data 1 | `data1` | Whole number. |
| Data 2 | `data2` | Whole number. |

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the message is only logged, no MIDI device gets it.
