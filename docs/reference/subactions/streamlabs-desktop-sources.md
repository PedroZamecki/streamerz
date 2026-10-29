# Streamlabs Desktop / Sources (sub-actions)

Sources in a Streamlabs Desktop scene: their visibility, mute state, flip, rotation, text and URL, and their filters. These run on a Streamlabs Desktop connection.

Each of these sub-actions has a Connection field. Empty (or the all-zero id) uses the default connection, or the first one when none is the default. The name fields (scene, source, group, filter, URL, text) are filled in with variables first. Where a scene name is empty, the active scene is used. When a source name matches more than one copy in a scene (dual output), the Display field picks which copies change.

## Set Source Visibility State

- Kind: sub-action
- Code: 401
- Type: Set Source Visibility State
- Category: Streamlabs Desktop/Sources
- Needs: a Streamlabs Desktop connection

Shows, hides or toggles a source in a scene.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Source | `sourceName` | The source's name. |
| State | `state` | 0 Visible, 1 Hidden, 2 Toggle. |
| Display | `display` | 0 Both, 1 Horizontal, 2 Vertical. |

### Arguments

None set.

### Notes

- A scene name that doesn't exist is logged as an error and nothing changes. A source name that isn't in the scene does nothing.

## Set Source Filter State

- Kind: sub-action
- Code: 406
- Type: Set Source Filter State
- Category: Streamlabs Desktop/Sources
- Needs: a Streamlabs Desktop connection

Shows, hides or toggles one filter on a source. The filter is found by its name on each copy of the source in the scene.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Source | `sourceName` | The source's name. |
| Filter | `filterName` | The filter's name. |
| State | `state` | 0 Visible, 1 Hidden, 2 Toggle. |

### Arguments

None set.

### Notes

- Filters with another name are left alone.

## Flip Source

- Kind: sub-action
- Code: 407
- Type: Flip Source
- Category: Streamlabs Desktop/Sources
- Needs: a Streamlabs Desktop connection

Flips a source horizontally, vertically or both ways.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Flip | `flipMode` | 0 Horizontal, 1 Vertical, 2 Both. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Source | `sourceName` | The source's name. |
| Display | `display` | 0 Both, 1 Horizontal, 2 Vertical. |

### Arguments

None set.

## Rotate Source

- Kind: sub-action
- Code: 408
- Type: Rotate Source
- Category: Streamlabs Desktop/Sources
- Needs: a Streamlabs Desktop connection

Rotates a source to an angle, or by an angle when Additive is on.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Degrees | `degrees` | A number of degrees. |
| Additive (rotate by, not to) | `additive` | On: rotate by the degrees. Off: rotate to them. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Source | `sourceName` | The source's name. |
| Display | `display` | 0 Both, 1 Horizontal, 2 Vertical. |

### Arguments

None set.

## Set Browser Source URL

- Kind: sub-action
- Code: 409
- Type: Set Browser Source URL
- Category: Streamlabs Desktop/Sources
- Needs: a Streamlabs Desktop connection

Sets the URL of a browser source.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Source | `sourceName` | The source's name. |
| URL | `url` | The new address. |

### Arguments

None set.

### Notes

- Streamerz changes the source's `url` setting in Streamlabs, on each copy of the source.

## Set GDI Text

- Kind: sub-action
- Code: 410
- Type: Set GDI Text
- Category: Streamlabs Desktop/Sources
- Needs: a Streamlabs Desktop connection

Sets the text of a GDI text source.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Source | `sourceName` | The source's name. |
| Text | `text` | The new text, several lines. Variables are filled in first. |

### Arguments

None set.

### Notes

- Streamerz changes the source's `text` setting in Streamlabs, on each copy of the source.

## Hide Source's Filters

- Kind: sub-action
- Code: 412
- Type: Hide Source's Filters
- Category: Streamlabs Desktop/Sources
- Needs: a Streamlabs Desktop connection

Hides every filter on a source.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Source | `sourceName` | The source's name. |

### Arguments

None set.

### Notes

- All of the source's filters are hidden, whatever their state was.

## Set Source Mute State

- Kind: sub-action
- Code: 413
- Type: Set Source Mute State
- Category: Streamlabs Desktop/Sources
- Needs: a Streamlabs Desktop connection

Mutes, unmutes or toggles an audio source in a scene.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Source | `sourceName` | The source's name. |
| State | `state` | 0 Muted, 1 Not muted, 2 Toggle. |

### Arguments

None set.

### Notes

- Only audio sources are matched. A source with no audio does nothing.
