# Core / Overlays (triggers)

Triggers for overlays. See [Overlays](../../OVERLAYS.md). Streamerz only: Streamer.bot does not have them.

## Overlay Control Changed

- Kind: trigger
- Code: 99881
- Type: OverlayControlChanged
- Category: Core/Overlays
- Needs: an overlay with live controls (`meta.template.controls`)

Fires when a live control of an overlay is set: from its Live tab, its dock, an action, one of its pages or an assistant.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Overlay | `overlay` | Only this overlay (empty: any). |
| Control | `control` | Only this control (empty: any). |

### Arguments

- `overlay`: the overlay's name.
- `control`: the control's name.
- `value`: what it holds now (a button: when it was pressed, in milliseconds since 1970).
- `old`: what it held before (empty for a button).
- `from`: who set it: `window`, `dock`, `action`, `page` or `mcp`.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time a control is set, even to the value it had.

### Notes

- Set Overlay Control in an action this trigger runs doesn't fire it again.
