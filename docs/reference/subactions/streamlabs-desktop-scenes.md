# Streamlabs Desktop / Scenes (sub-actions)

Scenes in Streamlabs Desktop: switching to one, reading the active one, and the filters on a scene. These run on a Streamlabs Desktop connection.

## Set Active Scene

- Kind: sub-action
- Code: 405
- Type: Set Active Scene
- Category: Streamlabs Desktop/Scenes
- Needs: a Streamlabs Desktop connection

Switches Streamlabs Desktop to a scene.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene | `sceneName` | The scene to switch to. Variables are filled in first. |

### Arguments

None set.

## Get Current Scene

- Kind: sub-action
- Code: 414
- Type: Get Current Scene
- Category: Streamlabs Desktop/Scenes
- Needs: a Streamlabs Desktop connection

Reads the name of the active scene.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |

### Arguments

Adds `currentScene`, the name of the active scene.

## Set Scene Filter State

- Kind: sub-action
- Code: 417
- Type: Set Scene Filter State
- Category: Streamlabs Desktop/Scenes
- Needs: a Streamlabs Desktop connection

Shows, hides or toggles a filter on a scene itself, not on a source.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| Scene (empty: active) | `sceneName` | The scene's name. Empty: the active scene. |
| Filter | `filterName` | The filter's name. Variables are filled in first. |
| State | `state` | 0 Visible, 1 Hidden, 2 Toggle. |

### Arguments

None set.

### Notes

- It works the same way as Set Source Filter State, with the scene in place of the source.
