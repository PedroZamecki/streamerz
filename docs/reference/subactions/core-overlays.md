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
