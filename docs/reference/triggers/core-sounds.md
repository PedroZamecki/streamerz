# Core / Sounds (triggers)

Triggers for the sounds Streamerz plays with Play Sound and Play Sound From Folder. Streamerz only: Streamer.bot does not have them.

## Sound Started

- Kind: trigger
- Code: 99300
- Type: SoundStarted
- Category: Core/Sounds
- Needs: nothing

Fires when a sound starts playing.

### Fields

None.

### Arguments

- `soundName`: the sound's name (empty when it has none).
- `soundFile`: the file's full path.
- `soundDevice`: the output it plays on (empty: the default).
- `soundLengthMs`: its length in milliseconds, when the file says.
- `actionName`: the action that played it.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time Play Sound or Play Sound From Folder starts a sound.

### Notes

- A sound played by an action that this trigger (or Sound Ended) runs fires neither trigger, so an action can't start itself over and over.

## Sound Ended

- Kind: trigger
- Code: 99301
- Type: SoundEnded
- Category: Core/Sounds
- Needs: nothing

Fires when a sound stops playing, for whatever reason.

### Fields

None.

### Arguments

- `soundName`, `soundFile`, `soundDevice`, `soundLengthMs` and `actionName`: as in Sound Started.
- `reason`: `finished` (it played to its end), `stopped` (a stop sub-action or a fade to stop) or `failed` (the output went away or the file couldn't be read on).
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Once for every sound that started, when it ends.

### Notes

- A failed sound also writes why in the log.
