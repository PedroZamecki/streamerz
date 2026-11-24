# Core / Overlays (sub-actions)

Sub-actions that send data to overlay pages. An overlay is an `.html` file kept in an action group and served by Streamerz. See [Overlays](../../OVERLAYS.md).

## Overlay

- Kind: sub-action
- Code: 99880
- Type: StreamerzOverlayPublish
- Category: Core/Overlays
- Needs: the HTTP server on, for pages to listen (the sub-action itself always works)

Sets an overlay topic. The overlay pages listening to that topic receive the value at once, and a page that opens later starts with the last value. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Topic | `topic` | The name the page listens to, such as `score`. Variables like `%user%` are filled in. An empty topic does nothing. |
| Value | `value` | Text, a number or JSON. Variables are filled in first. Text that is valid JSON (`5`, `true`, `{"a": 1}`) arrives as that value, anything else as text. |

### Arguments

None set.

### Notes

- Streamerz keeps the last value of at most 64 topics.
- `viewers` and `ad` are built in; publish to other names.

## Set Overlay Control

- Kind: sub-action
- Code: 99881
- Type: StreamerzSetOverlayControl
- Category: Core/Overlays
- Needs: an overlay with live controls (`meta.template.controls`)

Sets a live control of an overlay, as its Live tab and its dock do: the overlay's pages hear it at once (`streamerz.control`), the value is kept across restarts, and Overlay Control Changed fires with `from` set to `action`. A button is pressed. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Overlay | `overlay` | The overlay's name, picked from the overlays or typed. Variables are filled in. |
| Control | `control` | The control's name, such as `show`. Variables are filled in. |
| Value | `value` | What it is set to: `true` or `false` for a toggle, a number for a slider or a number, one of its options for a choice, `#rrggbb` for a color, text for text. A button ignores it. Variables are filled in first; text that is valid JSON arrives as that value. |

### Arguments

- `controlValue`: the value it was set to, as the control keeps it (a slider's within its ends).

### Notes

- A value the control doesn't take (a word for a slider, an option a choice doesn't have) is refused and said in the log; the control keeps its value.
- An action run by Overlay Control Changed that sets a control doesn't fire the trigger again, so two actions can't set each other forever.
