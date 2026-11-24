# Voices

Streamerz speaks text aloud (text to speech) and plays it with its own sound player, so a voice goes to
any output, the monitor included, at its own volume. Nothing runs until something speaks.

## System voices

The voices this computer already has, with nothing to download:

- **Linux**: espeak-ng, with more than a hundred languages and accents (Portuguese of Brazil and of
  Portugal included). Streamerz starts it for each speech and it ends with it. When it isn't installed,
  the Voices of the Sounds page says so with the command for your system, such as
  `sudo pacman -S espeak-ng`, `sudo apt install espeak-ng` or `sudo dnf install espeak-ng`. In the
  Flatpak, the one installed on the computer is used.
- **Windows**: the system's speech voices (Settings, Time and language, Speech), more of them by adding
  a language there.

## The Voices of the Sounds page

**Sounds**, Show **Voices** lists the voices with their language. Search narrows them by name or
language.

- **Say** opens a box to type something and hear it, on the monitor device or the default one.
- **Make default** makes it the voice used when an action names none.
- **Speed, pitch and volume** set how every voice speaks by default: speed from 0.5 to 3 (1 is the
  voice's own), pitch from 0.5 to 1.5, volume from 0 to 100.
- **Refresh** asks the system again, after a voice or espeak-ng was installed.

A speech is made as a WAV, played like any sound (it shows on Playing while it speaks, and Stop stops
it) and removed once it has played.

## Speaking from actions

**Speak** (Core > Voices) says a text aloud. Speeches wait their turn: one queue for each output, and
one for each overlay, so two speeches never talk over each other there. Its filters make chat fit to be
read: skip links, emote names and commands, cap a letter said over and over, cut a long message, never
read some words or some users (bots), and say something before (`%user% says`). With **Play in an
overlay**, the overlay's open pages play it instead (OBS mixes it with that browser source).

| Sub-action | Does |
| --- | --- |
| Speak | Says a text, in its turn; `speechDurationMs` and `speechReason` when the action waits |
| Speak To File | Writes the speech to a WAV (`speechFile`) |
| Stop Speaking | Stops what is said now; the next one goes on |
| Clear Speech Queue | Drops what waits |
| Set Speech Paused | Pauses speech where it is, or resumes it |
| Set Speech Enabled | Turns speech off (nothing is said) or on |
| Set User Voice | Gives a viewer a voice of their own (the user variable `ttsVoice`) |
| Get Voices | The voices (`voices`, `voiceCount`) |

The triggers **Speech Started** and **Speech Ended** run an action as a speech starts and ends, with
its text, voice, user and why it ended (`finished`, `stopped`, `skipped`, `failed`).

The Voices of the Sounds page also pauses or resumes speech, clears what waits and stops what is said.

**Read chat aloud** (Actions, New from template) sets it up: `!tts <text>`, `!voice <name>` for a
voice of one's own, `!skip` for moderators and a channel point reward (off until you pick the reward in
its trigger), with the filters as its settings.

A page with the permission `voice.speak` speaks too: `streamerz.speak(text, {voice})` gives an
`<audio>` of it, to play in step with what the page shows.
