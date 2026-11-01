# OBS Studio / Scenes (sub-actions)

Sub-actions that work on the sources and filters of one scene.

## Set Scene Filter State

- Kind: sub-action
- Code: 47
- Type: ObsSceneFilterState
- Category: OBS Studio/Scenes
- Needs: an OBS connection (the one picked, else the first connected one)

Enables, disables or toggles a filter on a scene.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | The scene that has the filter. Variables are filled in first. |
| Filter | `filterName` | The filter name. Variables are filled in first. |
| State | `state` | One of Enable, Disable, Toggle, stored as 0, 1, 2. |

### Arguments

None set.

### Notes

- The filter is looked up on the scene itself, so the scene name is used as the source name.

## Get Scene Item Properties

- Kind: sub-action
- Code: 324
- Type: ObsGetSceneItemProperties
- Category: OBS Studio/Scenes
- Needs: an OBS connection (the one picked, else the first connected one)

Reads the position, size, rotation, crop, visibility, mute and lock state of a source in a scene.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | The scene that holds the source. Variables are filled in first. |
| Source | `sourceName` | The source to read. Variables are filled in first. |

### Arguments

| Key | What it holds |
| --- | --- |
| `props._json` | All the properties as one JSON text. |
| `props.name`, `props.itemId` | The source name and its id in the scene. |
| `props.position.x`, `props.position.y`, `props.position.alignment` | The position. |
| `props.rotation`, `props.scale.x`, `props.scale.y` | The rotation and scale. |
| `props.crop.top`, `props.crop.right`, `props.crop.bottom`, `props.crop.left` | The crop. |
| `props.visible`, `props.muted`, `props.locked` | The visibility, mute and lock state. |
| `props.bounds.*`, `props.width`, `props.height`, `props.sourceWidth`, `props.sourceHeight` | The bounds and sizes. |

### Notes

- If the source is not in the scene, no arguments are set.

## Hide Scene's Sources

- Kind: sub-action
- Code: 325
- Type: ObsHideSceneSources
- Category: OBS Studio/Scenes
- Needs: an OBS connection (the one picked, else the first connected one)

Hides every source in a scene.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | The scene. Variables are filled in first. |

### Arguments

None set.

## Set Random Scene Source Visible

- Kind: sub-action
- Code: 326
- Type: ObsSetRandomSceneSourceVisible
- Category: OBS Studio/Scenes
- Needs: an OBS connection (the one picked, else the first connected one)

Shows one source of a scene picked at random.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | The scene. Variables are filled in first. |

### Arguments

None set.

### Notes

- Only the picked source is shown. The other sources keep their state.
