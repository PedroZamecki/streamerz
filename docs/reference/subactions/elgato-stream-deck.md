# Elgato / Stream Deck (sub-actions)

Button title, background, state and value for Elgato Stream Deck buttons.

## Set Title

- Kind: sub-action
- Code: 11001
- Type: StreamDeckSetTitle
- Category: Elgato/Stream Deck
- Needs: nothing

Sets the title of a Stream Deck button. The button ID and title are filled in with variables first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Button ID | `buttonId` | Text. The button to change. |
| Title | `title` | Text. |
| State | `state` | One of Enable, Disable or Toggle. |

### Arguments

None set.

### Notes

- The title is kept in Streamerz's memory only. Nothing sends it to a Stream Deck yet.
- State sets the button's state: Enable turns it on, Disable turns it off, Toggle flips it.

## Set Background

- Kind: sub-action
- Code: 11002
- Type: StreamDeckSetBackground
- Category: Elgato/Stream Deck
- Needs: nothing

Sets the background image and color of a Stream Deck button. The button ID, image and color are filled in with variables first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Button ID | `buttonId` | Text. The button to change. |
| Image | `image` | Text. |
| Color | `color` | Text. |
| State | `state` | One of Enable, Disable or Toggle. |

### Arguments

None set.

### Notes

- The background is kept in Streamerz's memory only. Nothing sends it to a Stream Deck yet.
- State works the same as for Set Title.

## Set State

- Kind: sub-action
- Code: 11004
- Type: StreamDeckSetState
- Category: Elgato/Stream Deck
- Needs: nothing

Sets the state of a Stream Deck button.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Button ID | `buttonId` | Text. The button to change. |
| State | `state` | One of Enable, Disable or Toggle. |

### Arguments

None set.

### Notes

- The state is kept in Streamerz's memory only. Nothing sends it to a Stream Deck yet.

## Show Alert

- Kind: sub-action
- Code: 11005
- Type: StreamDeckShowAlert
- Category: Elgato/Stream Deck
- Needs: nothing

Shows an alert on a Stream Deck button.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Button ID | `buttonId` | Text. The button to show the alert on. |

### Arguments

None set.

### Notes

- Streamerz only writes a debug line. Nothing shows on the device yet.

## Show Ok

- Kind: sub-action
- Code: 11006
- Type: StreamDeckShowOk
- Category: Elgato/Stream Deck
- Needs: nothing

Shows an OK mark on a Stream Deck button.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Button ID | `buttonId` | Text. The button to show the mark on. |

### Arguments

None set.

### Notes

- Streamerz only writes a debug line. Nothing shows on the device yet.

## Toggle State

- Kind: sub-action
- Code: 11007
- Type: StreamDeckToggleState
- Category: Elgato/Stream Deck
- Needs: nothing

Flips the state of a Stream Deck button, on to off or off to on.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Button ID | `buttonId` | Text. The button to change. |

### Arguments

None set.

### Notes

- The state is kept in Streamerz's memory only. Nothing sends it to a Stream Deck yet.

## Set Value

- Kind: sub-action
- Code: 11008
- Type: StreamDeckSetStatus
- Category: Elgato/Stream Deck
- Needs: nothing

Sets the value text of a Stream Deck button. The button ID and value are filled in with variables first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Button ID | `buttonId` | Text. The button to change. |
| Value | `value` | Text. |

### Arguments

None set.

### Notes

- The value is kept in Streamerz's memory only. Nothing sends it to a Stream Deck yet.

## Execute MCP Action

- Kind: sub-action
- Code: 14701
- Type: ElgatoStreamDeckMcpExecuteAction
- Category: Elgato/Stream Deck
- Needs: the Stream Deck app (not used yet)

Runs a Stream Deck MCP action.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- When it reaches the engine, Streamerz logs it once and skips it.
