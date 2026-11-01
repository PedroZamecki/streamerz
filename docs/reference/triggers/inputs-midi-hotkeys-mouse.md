# Inputs & MIDI / Hotkeys & Mouse (triggers)

Triggers for keyboard shortcuts and mouse clicks, whichever program has focus.

## Input Key Press

- Kind: trigger
- Code: 16001
- Type: InputKeyPress
- Category: Inputs & MIDI/Hotkeys & Mouse
- Needs: on Windows, nothing. On Linux, the desktop's Global Shortcuts portal (KDE and GNOME ask you to allow the shortcut; Hyprland needs a bind line)

Fires when its key combination is pressed, whichever program has focus. Set the combination by pressing the keys in the trigger's form.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Keys | `keyCode` | The key, captured by pressing it. The modifiers (Ctrl, Alt, Shift, Meta) are kept in `modifierMask`. |

### Arguments

- `key`: the key code.
- `modifier`: the modifier mask.
- `hasCtrl`: true when Ctrl is held.
- `hasAlt`: true when Alt is held.
- `hasShift`: true when Shift is held.

### When

When the combination is pressed and the modifiers match exactly. Streamerz listens only while an enabled action uses a Key Press trigger.

### Notes

- On Linux, Streamerz asks the desktop's Global Shortcuts portal for the combination. KDE and GNOME ask you to allow it.
- On Hyprland, add the bind line Streamerz shows to your config. Hyprland doesn't bind the keys by itself.
- Streamerz doesn't read the keyboard directly on Linux, so no input group or root access is needed.

## Input Mouse Click

- Kind: trigger
- Code: 16002
- Type: InputMouseClick
- Category: Inputs & MIDI/Hotkeys & Mouse
- Needs: Windows. Not available on Linux.

Fires on a mouse click anywhere on the screen.

### Fields

None.

### Arguments

- `mouse.button`: the button. `1` left, `2` right, `3` middle, `4` and `5` the side buttons.
- `mouse.clicks`: the number of clicks.
- `mouse.x`: the x position on the screen.
- `mouse.y`: the y position on the screen.

### When

When a mouse button is clicked. Streamerz installs its mouse hook only while an enabled action uses a Mouse Click trigger.

### Notes

- Windows only. On Linux this trigger never fires: catching clicks would mean reading every input device, which also sees every key typed, and Streamerz doesn't do that.
