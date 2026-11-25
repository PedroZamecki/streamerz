# Sounds

Streamerz plays sounds itself: an alert sound, a sound board, music between scenes. No browser source is
needed in OBS. Capture the output in OBS (Desktop Audio, or the output you pick) to put the sounds on
stream.

Nothing is opened until a sound plays. The first sound opens the output, and the output is closed again
after the last sound ends, so a Streamerz that plays no sound costs nothing for them. On Linux the output
is a small audio process (`Streamerz --sound-output`) that starts with the first sound and ends after the
last, so the system's audio library is never loaded in Streamerz itself. The first sound after a quiet
spell waits for it (a few tens of milliseconds); the ones that follow don't.

## Play a sound from an action

1. In **Actions**, add the sub-action **Play Sound** (Core > Sounds).
2. Pick the file: MP3, WAV, OGG, M4A or FLAC.
3. Set its volume (0 to 100) and, if you want to stop it later, give it a **Sound name**.

**Finish playing before continuing** is on by default: the action's next sub-action waits for the sound to
end. Turn it off to show something while the sound plays.

**At once or one after another.** Several sounds play together (up to 16, on every output together):
turn **Finish playing before continuing** off on each Play Sound and they all start at once. Leave it on
and each one starts when the one before ends. Any mix of the two works in one action.

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
| Play Sound In Overlay | Plays a file on an overlay's open pages: the browser source plays it, so OBS mixes it with that source |
| Set Sound Device | Chooses the output Default means for Streamerz (kept in Output) |

The triggers **Sound Started** and **Sound Ended** (Core > Sounds) run an action when a sound starts and
when it ends, with its name, file and why it ended (`finished`, `stopped` or `failed`). A sound played by
an action these triggers ran fires neither of them again. **Sound Device Changed** runs one when an output
comes or goes (headphones plugged in), with `device`, `deviceName` and `available`; Streamerz listens for
that only while an action has the trigger.

## The Sounds page

**Sounds** (Automation) shows one part at a time, picked with **Show**:

- **Playing**: what plays now, its output, where it is and its volume. Stop, Pause or Resume, Volume, and
  Stop all; the right-click menu has the same.
- **Library**: every sound Streamerz knows: those the actions play, those of the overlays, and those of the
  `sounds` folder beside `streamerz.json` (Add a sound copies one there, Open folder shows it). Play (on the
  monitor), Copy path, Used by, New action with this sound (an action with Play Sound ready, for a hotkey,
  a Stream Deck key or a command) and Add to Board. A file an action names that isn't there says so.
- **Board**: pads one press away, each with its sound, volume and output. Play, Edit, Move up, Delete, and
  New action with this sound. **Copy the dock's address** gives the Board as an OBS dock (Docks, Custom
  Browser Docks): a button for each pad, signed with a key of this computer, so only it plays them.
- **Voices**: the voices that speak (text to speech).
- **Output**: the Default device, the **monitor** device (where previews play: the streamer hears them,
  OBS doesn't capture them when it is another device), the master volume and how many sounds play at
  once, with a test sound for each device.

## Outputs

**Audio device** in Play Sound picks the output, from the list of this computer's outputs or typed. Leave
it empty (Default) for the one the Output settings chose, else the system's default. On Windows it is the
name Windows shows for the output (`Speakers (Realtek(R) Audio)`); on Linux it is the PulseAudio or
PipeWire sink name, listed by its description. An output that isn't connected plays on the default
instead. `Monitor` as a device plays on the monitor device.

On Linux, sounds need PulseAudio or PipeWire (with its PulseAudio service, as every desktop has it).

## Streamer.bot actions

Play Sound, Play Sound From Folder, Stop Sound Playback and Stop All Sound Playback work as in Streamer.bot,
with the same fields, so imported actions play their sounds. A Windows path (`C:\Sounds\boom.mp3`) in an
action imported on Linux needs the file picked again.

## Checking it here

`node dev/test-sounds.mjs <a sound file>` drives a dev instance (WebSocket 8095) on this computer's real
output: three copies of the file at once, one stopped by name, the rest faded out; two in a row through
an action, the second starting when the first ends; four at once through the request, each answer with
its own sound; and the output closed after each. With `STREAMERZ_PID` set it prints the memory, threads
and CPU while they play and after, and checks that the audio process ends with the last sound and that
PulseAudio is never loaded in Streamerz. Done with an 18 s MP3 on PipeWire, in a dev build: four at once
took 2.5% of a core, and the memory after sounds stays 0.8 MB above the start (the decoders' code, read
once), the same after every burst; with PulseAudio loaded in Streamerz it was 3.3 MB.

