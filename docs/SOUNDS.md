# Sounds

Streamerz plays sounds itself: an alert sound, a sound board, music between scenes. No browser source is
needed in OBS. Capture the output in OBS (Desktop Audio, or the output you pick) to put the sounds on
stream.

Nothing is opened until a sound plays. The first sound opens the output, and the output is closed again
after the last sound ends, so a Streamerz that plays no sound costs nothing for them.

## Play a sound from an action

1. In **Actions**, add the sub-action **Play Sound** (Core > Sounds).
2. Pick the file: MP3, WAV, OGG, M4A or FLAC.
3. Set its volume (0 to 100) and, if you want to stop it later, give it a **Sound name**.

**Finish playing before continuing** is on by default: the action's next sub-action waits for the sound to
end. Turn it off to show something while the sound plays.

**Play Sound From Folder** picks a sound at random from a folder (and its folders, with **Include its
folders**). It doesn't pick the same sound twice in a row while the folder has another.

## Control what plays

| Sub-action | Does |
| --- | --- |
| Stop Sound Playback | Stops the sounds of a name |
| Stop All Sound Playback | Stops every sound |
| Set Sound Volume | Changes the volume of the sounds of a name (or all), with a fade |
| Pause Sound, Resume Sound | Pauses the sounds of a name (or all) where they are, and plays them on |
| Set Master Volume | Changes the volume of every sound, now and later |
| Get Sound Info | Reads a file's length and format into arguments, without playing it |

The triggers **Sound Started** and **Sound Ended** (Core > Sounds) run an action when a sound starts and
when it ends, with its name, file and why it ended (`finished`, `stopped` or `failed`). A sound played by
an action these triggers ran fires neither of them again.

## Outputs

**Audio device** in Play Sound picks the output. Leave it empty for the system's default. On Windows it is
the name Windows shows for the output (`Speakers (Realtek(R) Audio)`); on Linux it is the PulseAudio or
PipeWire sink name (`pactl list short sinks`). An output that isn't connected plays on the default instead.

On Linux, sounds need PulseAudio or PipeWire (with its PulseAudio service, as every desktop has it).

## Streamer.bot actions

Play Sound, Play Sound From Folder, Stop Sound Playback and Stop All Sound Playback work as in Streamer.bot,
with the same fields, so imported actions play their sounds. A Windows path (`C:\Sounds\boom.mp3`) in an
action imported on Linux needs the file picked again.
