# OBS Studio / Extras (sub-actions)

What OBS can do beyond Streamer.bot's sub-actions: moving sources, fading volume, Studio Mode, hotkeys and more.

## Move or Size Source

- Kind: sub-action
- Code: 99750
- Type: ObsMoveSource
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Moves, sizes, rotates or crops a source in a scene. Only what is filled in changes. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| sceneName | `sceneName` | The scene. |
| sourceName | `sourceName` | The source in it. |
| Position X | `x` | Where its left side goes, in pixels. |
| Position Y | `y` | Where its top goes, in pixels. |
| Size in % | `scale` | Its size as a percent of the original (100 is normal), both ways. |
| Rotation in degrees | `rotation` | Degrees. |
| Crop left | `cropLeft` | Pixels cut off the left. |
| Crop right | `cropRight` | Pixels cut off the right. |
| Crop top | `cropTop` | Pixels cut off the top. |
| Crop bottom | `cropBottom` | Pixels cut off the bottom. |

### Arguments

None set.

### Notes

- Variables are filled in first, so a position can come from the action.

## Set Volume

- Kind: sub-action
- Code: 99751
- Type: ObsSetVolume
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Sets an input's volume, at once or fading over a time. Good for lowering the music while someone talks. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| inputName | `inputName` | The audio source. |
| Level | `level` | The volume to reach. |
| In | `unit` | Percent (100 is normal) or decibels (0 is normal). |
| Fade over | `fade` | How long the change takes. 0: at once. It goes in steps of a tenth of a second, up to a hundred. |

### Arguments

None set.

### Notes

- The action waits while it fades.

## Set Audio Monitor

- Kind: sub-action
- Code: 99752
- Type: ObsSetMonitor
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Chooses whether OBS plays an input on your headphones, and whether it also sends it out. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| inputName | `inputName` | The audio source. |
| monitor | `monitor` | Off, monitor only, or monitor and output. |

### Arguments

None set.

## Studio Mode

- Kind: sub-action
- Code: 99753
- Type: ObsStudioMode
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Turns Studio Mode on or off, or switches it. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| Turn | `state` | On, Off or Toggle. |

### Arguments

- `obsStudioMode`: true when it is on now.

## Set Preview Scene

- Kind: sub-action
- Code: 99754
- Type: ObsSetPreviewScene
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Picks the scene in the Studio Mode preview. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| sceneName | `sceneName` | The scene. |

### Arguments

None set.

## Trigger Studio Transition

- Kind: sub-action
- Code: 99755
- Type: ObsTriggerStudioTransition
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Sends the preview to the program, as Studio Mode's Transition button does. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |

### Arguments

None set.

## Set Transition

- Kind: sub-action
- Code: 99756
- Type: ObsSetTransition
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Chooses the scene transition and how long it takes. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| Transition | `transitionName` | The transition, by its name. Empty: keep the current one. |
| Duration | `duration` | In milliseconds. Empty: keep the current one. |

### Arguments

None set.

## Virtual Camera

- Kind: sub-action
- Code: 99757
- Type: ObsVirtualCamera
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Starts or stops OBS's virtual camera, or switches it. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| Turn | `state` | Start, Stop or Toggle. |

### Arguments

- `obsVirtualCamera`: true when it is on now.

## Split Recording

- Kind: sub-action
- Code: 99758
- Type: ObsSplitRecording
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Closes the recording file and starts a new one without stopping the recording. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |

### Arguments

None set.

### Notes

- OBS must record with file splitting available (OBS 30.2 or newer).

## Trigger Hotkey

- Kind: sub-action
- Code: 99759
- Type: ObsTriggerHotkey
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Presses one of OBS's hotkeys by its name. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| Hotkey name | `hotkeyName` | The hotkey's name inside OBS, such as `OBSBasic.StartStreaming`. |

### Arguments

None set.

## Switch Profile

- Kind: sub-action
- Code: 99760
- Type: ObsSwitchProfile
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Switches OBS's profile. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| Profile | `profileName` | The profile. |

### Arguments

None set.

## Switch Scene Collection

- Kind: sub-action
- Code: 99761
- Type: ObsSwitchSceneCollection
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Switches OBS's scene collection. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| Scene collection | `collectionName` | The scene collection. |

### Arguments

None set.

## Get Last Replay File

- Kind: sub-action
- Code: 99762
- Type: ObsGetLastReplay
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Says where the replay buffer's last saved replay is, to send it somewhere (to Discord with Upload File). This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |

### Arguments

- `obsReplayPath`: the file's path.

## Set Filter Settings

- Kind: sub-action
- Code: 99763
- Type: ObsSetFilterSettings
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Changes some settings of a filter, such as a blur's size or a colour correction's hue. The other settings stay. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| sourceName | `sourceName` | The source the filter is on. |
| filterName | `filterName` | The filter. |
| Settings (Name=value) | `settings` | Name=value lines. A value that is `true`, `false` or a number goes as one; anything else as text. |

### Arguments

None set.

## Set Input Settings

- Kind: sub-action
- Code: 99764
- Type: ObsSetInputSettings
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Changes some settings of an input, such as a text source's text or a browser source's address. The other settings stay. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| inputName | `inputName` | The input. |
| Settings (Name=value) | `settings` | Name=value lines, typed as in Set Filter Settings. |

### Arguments

None set.

## Show Source For a While

- Kind: sub-action
- Code: 99765
- Type: ObsShowSourceFor
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Shows a source for a while and hides it again: one step in place of show, wait and hide. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| sceneName | `sceneName` | The scene. |
| sourceName | `sourceName` | The source in it. |
| Duration | `duration` | How long it stays shown. |
| Wait until it's hidden again | `wait` | On: the action waits until it is hidden. Off: the action goes on and the source hides itself. |

### Arguments

None set.

## Output

- Kind: sub-action
- Code: 99766
- Type: ObsOutput
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Starts, stops or toggles one of OBS's outputs by name, such as a multistream target or a vertical canvas a plugin adds. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| Output | `outputName` | The output's name, as OBS lists it. The list offers the ones OBS has. |
| State | `state` | Start, stop or toggle. |

### Arguments

- `obsOutputActive`: true when the output is running after the step.

## Lock Source

- Kind: sub-action
- Code: 99767
- Type: ObsLockSource
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Locks or unlocks a source in a scene, so it can't be moved by accident. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| sceneName | `sceneName` | The scene. |
| sourceName | `sourceName` | The source in it. |
| State | `state` | Lock, unlock or toggle. |

### Arguments

- `obsSourceLocked`: true when the source is locked after the step.

## Set Audio Sync Offset

- Kind: sub-action
- Code: 99768
- Type: ObsSetAudioSyncOffset
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Sets how much a source's audio is moved in time, to match it to the video. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| Source | `inputName` | The audio source. |
| Offset (ms, -950 to 20000) | `offset` | Milliseconds. Outside OBS's range it's kept at the nearest end. |

### Arguments

None set.

## Set Audio Balance

- Kind: sub-action
- Code: 99769
- Type: ObsSetAudioBalance
- Category: OBS Studio/Extras
- Needs: an OBS connection (Stream Apps)

Pans a source's audio left or right. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| connectionId | `connectionId` | The OBS connection to use. Empty: any that is connected. |
| Source | `inputName` | The audio source. |
| Balance (0 left, 50 middle, 100 right) | `balance` | Where the sound sits. Empty: the middle. |

### Arguments

None set.

