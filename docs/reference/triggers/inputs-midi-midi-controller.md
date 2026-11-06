# Inputs & MIDI / MIDI Controller (triggers)

Triggers for MIDI controllers: notes, control changes and other messages from a device.

## Midi Message

- Kind: trigger
- Code: 15001
- Type: MidiMessage
- Category: Inputs & MIDI/MIDI Controller
- Needs: a MIDI input device

Starts its actions for each message from a MIDI device (a key, a pad, a knob). Every MIDI input is
open while an enabled action uses this trigger, and none otherwise: Linux reads the system's raw MIDI
devices (`/dev/snd/midiC*D*`), Windows uses winmm.

### When

Runs for each incoming MIDI message that matches all configured filters. Leave the filters empty
(or Channel at zero) to accept every message from an open input.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Device | `device` | The device's name. Empty: any device. |
| Command | `commandCode` | `NoteOn`, `NoteOff`, `ControlChange`... Empty: any command. |
| Channel | `channel` | 1 to 16. `0`: any channel. |
| Note or controller | `data1` | The note or controller number. Empty: any. |

### Arguments

Streamer.bot's names (it uses NAudio's):

- `name`, `deviceName`: the device; `timestamp`: milliseconds since it was opened.
- `commandCode` (`NoteOn`, `ControlChange`, `PitchWheelChange`...), `commandCodeNum` (144...), `channel`
  (1 to 16), `rawMessage` (the message as 8 hex digits).
- Notes: `noteName` (`C4` for 48: the octave is the number over 12), `noteNumber`, `velocity`,
  `velocityNormal` (0 to 1).
- Controllers: `controllerName` (`Pan`, `MainVolume`... else the number), `controllerNumber`, `value`,
  `valueNormal`.
- Program change, aftertouch and pitch wheel: `value` and `valueNormal`.

### Notes

- Streamer.bot keeps devices and "learned" event listeners in its MIDI settings, and its trigger names
  them by id. Streamerz opens every input and the trigger filters by name, command, channel and note,
  so a trigger imported from Streamer.bot needs its fields set again.
- A device plugged in later is opened when the settings next change (or at the next start).
