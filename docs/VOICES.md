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
