# Core / Voices (triggers)

Triggers for the speeches of Speak (see [Voices](../../VOICES.md)). Streamerz only.

## Speech Started

- Kind: trigger
- Code: 99320
- Type: SpeechStarted
- Category: Core/Voices
- Needs: nothing

Fires when a speech starts being said.

### Fields

None.

### Arguments

- `text`: what is said, filtered.
- `voice`: the voice.
- `user`: the viewer whose message it is, when there is one.
- `speechDurationMs`: its length.
- `actionName`: the action that asked for it.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time Speak's turn comes and the voice has made the speech.

## Speech Ended

- Kind: trigger
- Code: 99321
- Type: SpeechEnded
- Category: Core/Voices
- Needs: nothing

Fires when a speech ends, for whatever reason.

### Fields

None.

### Arguments

- The same as Speech Started, and `reason`: `finished`, `stopped`, `skipped` or `failed`.

### When

When the speech has been said, stopped, dropped from the queue, or couldn't be made.

### Notes

- A speech asked by an action that one of these triggers ran fires neither, so speech can't start itself over and over.
