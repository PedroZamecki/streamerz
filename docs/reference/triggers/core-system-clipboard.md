# Core / System & Clipboard (triggers)

Triggers about the system itself, such as the clipboard.

## Clipboard Changed

- Kind: trigger
- Code: 42001
- Type: SystemClipboardChanged
- Category: Core/System
- Needs: nothing

Starts its actions when something is copied.

### Fields

None.

### Arguments

- `formats.count` and `formats.0`..: what the clipboard holds (`Text`, `UnicodeText`, `System.String`
  for text; `Bitmap` for an image).
- `containsText`, `containsImage`, `containsAudio`.
- `clipboard.text`: the text, when there is some.
- `error`: false.
- Plus `triggerId`, `triggerName`, `triggerCategory` (`Core/System`) and `eventSource` (`system`).

### When

When the clipboard changes, while an enabled action uses this trigger (nothing listens otherwise).

### Notes

- Windows and X11 always tell. On Wayland the system only tells the focused app, so it fires only while
  Streamerz's window has focus.
