# Core / Voices (sub-actions)

Sub-actions that speak text aloud with the voices of this computer (see [Voices](../../VOICES.md)). Streamerz only: Streamer.bot has no voices of its own.

Speeches wait for each other: there is one queue for each output (and one for each overlay), so two speeches never talk over each other there.

## Speak

- Kind: sub-action
- Code: 99320
- Type: Speak
- Category: Core/Voices
- Needs: a voice (espeak-ng on Linux, a speech voice on Windows)

Says a text aloud. Filters make a chat message fit to be read: they drop links, emote names, commands and the words you never want read, cap letters said over and over, and cut a long message.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Text | `text` | What to say. Variables are filled in (`%rawInput%`, `%message%`). |
| Voice | `voice` | A voice of this computer. Empty: the viewer's own (Set User Voice), else the default of the Voices page. |
| Volume | `volume` | 0 to 100. Empty: the default. |
| Speed | `speed` | 0.5 to 3, 1 as the voice speaks. Empty: the default. |
| Pitch | `pitch` | 0.5 to 1.5, 1 as the voice speaks. Empty: the default. |
| Audio device | `device` | The output. Empty: Default. `Monitor`: the monitor device. |
| Play in an overlay | `overlay` | An overlay whose open pages play it instead (the browser source's sound, so OBS mixes it there). |
| Finish playing before continuing | `finishBeforeContinuing` | On (the default): the action waits until it has been said. |
| Said before | `prefix` | Said before the text, such as `%user% says`. |
| Skip links | `skipLinks` | Drop web addresses. |
| Skip emote names | `skipEmotes` | Drop the emotes of the message that started the action. |
| Skip commands | `skipCommands` | Say nothing when the text starts with `!`. |
| A letter at most | `maxRepeat` | `nooooooo` becomes `nooo` with 3. 0: as written. |
| At most this many characters | `maxLength` | Cut at a word. 0: all. |
| Words never read | `blockedWords` | One word or phrase a line, in any case. |
| Users never read | `skipUsers` | One a line (bots): their messages are not said. |

### Arguments

- `speechDurationMs`: how long it spoke, when the action waited.
- `speechReason`: how it ended: `finished`, `stopped`, `skipped` (cleared, or speech is off) or `failed`.
- `speechSkipped`: `True` when the filters left nothing to say.

## Speak To File

- Kind: sub-action
- Code: 99321
- Type: SpeakToFile
- Category: Core/Voices
- Needs: a voice

Writes the speech to a WAV file instead of playing it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Text | `text` | What to say. Variables are filled in. |
| Voice | `voice` | As in Speak. |
| Speed | `speed`, `pitch` | As in Speak. |
| File | `file` | Where to write it. Empty: a new file in the `sounds` folder beside `streamerz.json`. |

### Arguments

- `speechFile`: the file written.
- `speechDurationMs`: its length.

## Stop Speaking

- Kind: sub-action
- Code: 99322
- Type: StopSpeaking
- Category: Core/Voices
- Needs: nothing

Stops what is being said now. Each queue goes on with its next speech.

### Fields

None.

### Arguments

None set.

## Clear Speech Queue

- Kind: sub-action
- Code: 99323
- Type: ClearSpeechQueue
- Category: Core/Voices
- Needs: nothing

Drops the speeches waiting their turn (each ends as `skipped`). What is being said goes on.

### Fields

None.

### Arguments

None set.

## Set Speech Paused

- Kind: sub-action
- Code: 99324
- Type: SetSpeechPaused
- Category: Core/Voices
- Needs: nothing

Pauses speech where it is, or resumes it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| State | `state` | 0 Resume, 1 Pause, 2 Toggle. |

### Arguments

None set.

## Set Speech Enabled

- Kind: sub-action
- Code: 99325
- Type: SetSpeechEnabled
- Category: Core/Voices
- Needs: nothing

Turns speech off (nothing is said, what waits is dropped) or on again.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| State | `state` | 0 Disable, 1 Enable, 2 Toggle. |

### Arguments

None set.

## Set User Voice

- Kind: sub-action
- Code: 99326
- Type: SetUserVoice
- Category: Core/Voices
- Needs: nothing

Gives a viewer a voice of their own: Speak uses it when it names no voice. It is kept in the user variable `ttsVoice`.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Whose voice | `source` | 0 the user who ran the action, 1 the target user. |
| Voice | `voice` | A voice of this computer. Empty: back to the default. |

### Arguments

None set.

## Get Voices

- Kind: sub-action
- Code: 99327
- Type: GetVoices
- Category: Core/Voices
- Needs: nothing

Reads the voices of this computer.

### Arguments

- `voices`: a JSON list of `{id, name, language, gender}`.
- `voiceCount`: how many.

### Fields

None.
