# Inputs & MIDI / MIDI Controller (triggers)

Triggers for MIDI controllers: notes, control changes and other messages from a device.

## Midi Message

- Kind: trigger
- Code: 15001
- Type: MidiMessage
- Category: Inputs & MIDI/MIDI Controller
- Needs: a MIDI input device (not wired up yet)

Meant for an incoming MIDI message (a note or a control change) from a device.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Device | `device` | The MIDI device's name. Empty: any device. |
| Channel | `channel` | The MIDI channel. `0`: any channel. |
| Status / Type | `status` | The message type, such as `NoteOn`, `NoteOff` or `ControlChange`. Empty: any type. |

### Arguments

None set.

### When

Never, for now. Streamerz doesn't fire this yet.

### Notes

- Streamerz can send MIDI (the MIDI actions), but it doesn't read MIDI input yet, so this trigger never fires.
- Streamerz doesn't fire this yet.
