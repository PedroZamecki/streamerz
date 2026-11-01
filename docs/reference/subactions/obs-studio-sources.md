# OBS Studio / Sources (sub-actions)

Sub-actions that change one source in OBS: its visibility, filters, mute, audio track, text, colour, media file and browser URL.

## Set Source Visibility State

- Kind: sub-action
- Code: 30
- Type: ObsSourceVisbility
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Shows, hides or toggles a source in a scene. The source is looked up by name in that scene. If the source is not in the scene, nothing happens.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | The scene name. Variables are filled in first. |
| Source | `sourceName` | The source name in that scene. Variables are filled in first. |
| State | `state` | One of Show, Hide, Toggle, stored as 0, 1, 2. |

### Arguments

None set.

### Notes


## Hide Group's Sources

- Kind: sub-action
- Code: 32
- Type: ObsHideGroupSources
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Hides every source inside an OBS group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Group | `groupName` | The group name. Variables are filled in first. |

### Arguments

None set.

### Notes

- The group name is used as the name to look up in OBS. The Scene field is not used.

## Set Source Filter State

- Kind: sub-action
- Code: 35
- Type: ObsSourceFilterState
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Enables, disables or toggles one filter on a source.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The source that has the filter. Variables are filled in first. |
| Filter | `filterName` | The filter name. Variables are filled in first. |
| State | `state` | One of Enable, Disable, Toggle, stored as 0, 1, 2. |

### Arguments

None set.

### Notes


## Flip Source

- Kind: sub-action
- Code: 36
- Type: ObsFlipSource
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Flips a source horizontally, vertically or both. It flips the current scale, so flipping twice puts the source back the way it was.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Flip mode | `flipMode` | One of Horizontal, Vertical, Both, stored as 0, 1, 2. |
| Scene | `sceneName` | The scene that holds the source. Variables are filled in first. |
| Source | `sourceName` | The source to flip. Variables are filled in first. |

### Arguments

None set.

## Rotate Source

- Kind: sub-action
- Code: 37
- Type: ObsRotateSource
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Sets the rotation of a source to the given degrees. With Additive on, the degrees are added to the current rotation instead.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Degrees | `degrees` | A number of degrees. |
| Additive | `additive` | On: add to the current rotation. Off: set the rotation. |
| Scene | `sceneName` | The scene that holds the source. Variables are filled in first. |
| Source | `sourceName` | The source to rotate. Variables are filled in first. |

### Arguments

None set.

### Notes

- The result is taken modulo 360 degrees.

## Set Browser Source URL

- Kind: sub-action
- Code: 38
- Type: ObsSetBrowserSourceUrl
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Sets the `url` setting of a browser source.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The browser source. Variables are filled in first. |
| URL | `url` | The new address. Variables are filled in first. |

### Arguments

None set.

### Notes

- The Scene field is not used. The source is found by name in OBS.

## Set GDI Text

- Kind: sub-action
- Code: 39
- Type: ObsSetGdiText
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Sets the `text` setting of a text source.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The text source. Variables are filled in first. |
| Text | `text` | The new text. Variables are filled in first. |

### Arguments

None set.

### Notes

- The Scene field is not used. The source is found by name in OBS.

## Hide Source's Filters

- Kind: sub-action
- Code: 41
- Type: ObsHideSourceFilters
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Disables every filter on a source.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The source whose filters are turned off. Variables are filled in first. |

### Arguments

None set.

### Notes

- The Scene field is not used. The source is found by name in OBS.

## Set Source Mute State

- Kind: sub-action
- Code: 42
- Type: ObsSourceMuteState
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Mutes, unmutes or toggles the audio of a source.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The source. Variables are filled in first. |
| State | `state` | One of Mute, Unmute, Toggle, stored as 0, 1, 2. |

### Arguments

None set.

### Notes


## Set Source Audio Track State

- Kind: sub-action
- Code: 44
- Type: ObsSetAudioTrackState
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Turns one audio track of a source on or off, or toggles it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The source. Variables are filled in first. |
| Track | `track` | Track 1 to Track 6, stored as 0 to 5. |
| State | `state` | One of Enable, Disable, Toggle, stored as 0, 1, 2. |

### Arguments

None set.

### Notes


## Set Media State

- Kind: sub-action
- Code: 49
- Type: ObsSetMediaState
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Controls a media source: play, pause, stop, restart, next or previous.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The media source. Variables are filled in first. |
| State | `state` | One of Play, Pause, Restart, Stop, Next, Previous, stored as 0 to 5. |

### Arguments

None set.

### Notes

- Any value above 4 is Previous.

## Set Media Source File

- Kind: sub-action
- Code: 322
- Type: ObsSetMediaSourceFile
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Sets the file that a media source plays (its `local_file` setting).

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The media source. Variables are filled in first. |
| File name | `fileName` | The file path. Variables are filled in first. |

### Arguments

None set.

### Notes

- The Scene field is not used. The source is found by name in OBS.

## Set Image Source File

- Kind: sub-action
- Code: 323
- Type: ObsSetImageSourceFile
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Sets the file that an image source shows (its `file` setting).

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The image source. Variables are filled in first. |
| File name | `fileName` | The file path. Variables are filled in first. |

### Arguments

None set.

### Notes

- The Scene field is not used. The source is found by name in OBS.

## Set Color Source Color

- Kind: sub-action
- Code: 327
- Type: ObsSetColorSourceColor
- Category: OBS Studio/Sources
- Needs: an OBS connection (the one picked, else the first connected one)

Sets the colour of a colour source. With Random on, a random colour is picked each time.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Source | `sourceName` | The colour source. Variables are filled in first. |
| Color | `color` | A hex colour: `RRGGBB` or `#RRGGBB`, or `AARRGGBB` for a colour with alpha. |
| Random | `random` | On: pick a random colour, and ignore Color. |

### Arguments

None set.

### Notes

- Text that is not a hex colour becomes opaque black.
- The Scene field is not used. The source is found by name in OBS.
