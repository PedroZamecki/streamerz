# Inputs & MIDI / Hotkeys & Mouse (triggers)

Triggers for keyboard shortcuts and mouse clicks, whichever program has focus.

## Input Key Press

- Kind: trigger
- Code: 16001
- Type: InputKeyPress
- Category: Inputs & MIDI/Hotkeys & Mouse
- Needs: on Windows and X11, nothing. On Wayland, the desktop's Global Shortcuts portal (KDE and GNOME ask you to allow the shortcut; on Hyprland Streamerz binds it); without the portal, allowing Streamerz to read the keyboards

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

- On X11, Streamerz takes the combination from the X server. Nothing to set up.
- On Wayland, Streamerz asks the desktop's Global Shortcuts portal for the combination. KDE and GNOME ask you to allow it.
- On Hyprland, Streamerz adds the bind through Hyprland's socket while it runs and removes it when it quits. Other programs still get the keys.
- On a Wayland desktop without the portal (sway, older GNOME), the trigger's form offers to let Streamerz read the keyboards. That lets any program you run read them too, so allow it only if you need hotkeys there.

## Input Mouse Click

- Kind: trigger
- Code: 16002
- Type: InputMouseClick
- Category: Inputs & MIDI/Hotkeys & Mouse
- Needs: on Windows and X11, nothing. On Wayland, allowing Streamerz to read the mice (the trigger's Allow button, once)

Fires on a mouse click anywhere on the screen.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Button | `button` | Streamerz's own: the button that fires it, set by clicking with it (0 or missing: any). |

### Arguments

- `mouse.button`: the button. `1` left, `2` right, `3` middle, `4` back, `5` forward.
- `mouse.clicks`: the number of clicks in a row (2 for a double click).
- `mouse.x`: the x position on the screen.
- `mouse.y`: the y position on the screen.

### When

When a mouse button is clicked. Streamerz listens only while an enabled action uses a Mouse Click trigger.

### Notes

- On X11, the clicks come from the X server. Nothing to set up.
- Wayland gives no program the clicks made in other windows, so Streamerz reads the mice. Allow (in the trigger's form) asks for your password once and adds `/etc/udev/rules.d/70-streamerz-mice.rules`, which lets the person at the computer read mice and touchpads. Keyboards stay closed. Remove access deletes it.
- On Wayland, the position comes from Hyprland. Other Wayland desktops don't share it, so `mouse.x` and `mouse.y` are 0. A touchpad's taps aren't seen, only its physical clicks.
- On Windows, programs run as administrator keep their clicks unless Streamerz runs as administrator too.
