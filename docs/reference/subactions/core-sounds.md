# Core / Sounds (sub-actions)

Sub-actions that play sounds through Streamerz itself, on this computer's speakers or another output, with no browser source in OBS. Nothing is opened until a sound plays, and the output is closed again after the last one. See [Sounds](../../SOUNDS.md).

## Play Sound

- Kind: sub-action
- Code: 1
- Type: PlaySound
- Category: Core/Sounds
- Needs: PulseAudio or PipeWire on Linux (nothing on Windows)

Plays a sound file: MP3, WAV, OGG, M4A or FLAC.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Sound file | `soundFile` | The file to play. Variables are filled in. A path that is not absolute is read beside `streamerz.json`. |
| Audio device | `device` | The output to play on. Empty, `Default` or `Application Default` is the system's default output. An output that isn't connected falls back to the default. |
| Volume | `volume` | 0 to 100 (Streamer.bot's slider). A number from 0 to 1 is read as a fraction. |
| Finish playing before continuing | `finishBeforeContinuing` | On: the next sub-action waits for the sound's end. Off: the sound plays on while the action goes on. |
| Sound name | `soundName` | A name for Stop Sound Playback, Set Sound Volume, Pause Sound and Resume Sound. |
| Use the file name as the name | `useFileNameAsName` | The file's name, without its extension, is the sound's name. |

### Arguments

- `soundLength`: the sound's length in seconds.
- `soundLengthMs`: the sound's length in milliseconds.

### Notes

- At most 16 sounds play at once; another is refused with a line in the log.
- A file that can't be played writes why in the log and the action goes on.
- It fires **Sound Started** when it begins and **Sound Ended** when it ends.

## Play Sound From Folder

- Kind: sub-action
- Code: 7
- Type: PlaySoundFromFolder
- Category: Core/Sounds
- Needs: PulseAudio or PipeWire on Linux (nothing on Windows)

Plays a sound picked at random from a folder.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Folder | `folder` | The folder of sound files (MP3, WAV, OGG, M4A, FLAC). Other files are left out. |
| Include its folders | `recursive` | Also pick from the folders inside it. |
| Not the same sound twice in a row | `avoidRepeats` | Streamerz only, on by default: the last sound of this folder isn't picked again while there is another. |
| Audio device | `device` | As in Play Sound. |
| Volume | `volume` | As in Play Sound. |
| Finish playing before continuing | `finishBeforeContinuing` | As in Play Sound. |
| Sound name | `soundName` | As in Play Sound. |
| Use the file name as the name | `useFileNameAsName` | As in Play Sound. |

### Arguments

- `randomSoundFile`: the full path of the sound picked.
- `randomSoundFileName`: its file name.
- `soundLength`: its length in seconds.
- `soundLengthMs`: its length in milliseconds.

### Notes

- An empty or missing folder writes why in the log and the action goes on.

## Stop All Sound Playback

- Kind: sub-action
- Code: 30001
- Type: StopAllSoundPlayback
- Category: Core/Sounds
- Needs: nothing

Stops every sound Streamerz is playing.

### Fields

None.

### Arguments

None set.

### Notes

- A Play Sound waiting for its sound's end goes on at once.

## Stop Sound Playback

- Kind: sub-action
- Code: 30002
- Type: StopSoundPlayback
- Category: Core/Sounds
- Needs: nothing

Stops the sounds of a name.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Sound name | `soundName` | The name given in Play Sound. Upper and lower case don't matter. Empty stops nothing. |

### Arguments

None set.

### Notes

- Every sound of that name stops, on every output.

## Set Sound Volume

- Kind: sub-action
- Code: 99300
- Type: SetSoundVolume
- Category: Core/Sounds
- Needs: nothing

Changes the volume of sounds that are playing, at once or with a fade. Streamerz only.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Sound name | `soundName` | The sounds to change. Empty changes every sound playing. |
| Volume | `volume` | 0 to 100. |
| Fade | `fadeMs` | How long the change takes, in milliseconds (0: at once). A fade to 0 leaves the sound playing silently. |

### Arguments

None set.

### Notes

- To fade a sound out and stop it, fade it to 0 and then use Stop Sound Playback after a Delay of the same length.

## Pause Sound

- Kind: sub-action
- Code: 99301
- Type: PauseSound
- Category: Core/Sounds
- Needs: nothing

Pauses sounds that are playing, where they are. Streamerz only.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Sound name | `soundName` | The sounds to pause. Empty pauses every sound. |

### Arguments

None set.

### Notes

- A paused sound keeps its output open until it is resumed or stopped.

## Resume Sound

- Kind: sub-action
- Code: 99302
- Type: ResumeSound
- Category: Core/Sounds
- Needs: nothing

Plays paused sounds on from where they were. Streamerz only.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Sound name | `soundName` | The sounds to resume. Empty resumes every sound. |

### Arguments

None set.

### Notes

- A sound that isn't paused is left as it is.

## Set Master Volume

- Kind: sub-action
- Code: 99303
- Type: SetMasterVolume
- Category: Core/Sounds
- Needs: nothing

Changes the volume of every sound Streamerz plays, now and later. Streamerz only.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Volume | `volume` | 0 to 100. |
| Fade | `fadeMs` | How long the change takes for the sounds playing, in milliseconds. |

### Arguments

None set.

### Notes

- The master volume is kept until Streamerz closes.

## Get Sound Info

- Kind: sub-action
- Code: 99304
- Type: GetSoundInfo
- Category: Core/Sounds
- Needs: nothing

Reads a sound file's length and format without playing it. Streamerz only.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Sound file | `soundFile` | The file to read. Variables are filled in. |

### Arguments

- `soundLength`: the length in seconds.
- `soundLengthMs`: the length in milliseconds.
- `soundFormat`: the file's extension, such as `mp3`.
- `soundChannels`: 1 for mono, 2 for stereo.
- `soundRate`: samples a second, such as `44100`.

### Notes

- A file that can't be read writes why in the log and sets nothing.

## Play Sound In Overlay

- Kind: sub-action
- Code: 99305
- Type: PlaySoundInOverlay
- Category: Core/Sounds
- Needs: the HTTP server on, and a page of the overlay open (in OBS, for OBS to hear it)

Plays a sound on an overlay's open pages instead of on this computer's output: the browser source plays it, so OBS mixes it with that source, with its own volume and filters. The page needs no code for it. Streamerz only.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Overlay | `overlay` | The overlay's name. Variables are filled in. |
| Sound | `soundFile` | One of the overlay's files (`sounds/ding.mp3`) or a web address. Variables are filled in. |
| Volume | `volume` | 0 to 100. |
| Sound name | `soundName` | A name, so Stop Sound Playback can stop it. |

### Arguments

- `overlayPages`: how many pages of the overlay were open to play it.

### Notes

- Stop All Sound Playback and Stop Sound Playback also stop the sounds overlays play.
- A file that isn't one of the overlay's writes why in the log.

## Set Sound Device

- Kind: sub-action
- Code: 99306
- Type: SetSoundDevice
- Category: Core/Sounds
- Needs: nothing

Chooses the output that Default means for Streamerz: every Play Sound left on Default plays there from now on. It is kept in the Sounds page's Output settings. Streamerz only.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Device | `device` | The output's name, picked from the list or typed. `Default` or empty: the system's default again. Variables are filled in. |

### Arguments

None set.

### Notes

- A sound already playing stays where it is; the next one goes to the new device.
- An output that isn't there when a sound plays: the sound plays on the system's default and the log says so.
