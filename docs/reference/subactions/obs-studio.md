# OBS Studio (sub-actions)

Sub-actions that control OBS as a whole: the stream, the recording, the program scene, screenshots, raw requests and status.

## Streaming

- Kind: sub-action
- Code: 33
- Type: ObsStreaming
- Category: OBS Studio
- Needs: an OBS connection (the one picked, else the first connected one)

Starts, stops or toggles the stream.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| State | `state` | One of Start, Stop, Toggle, stored as 0, 1, 2. |

### Arguments

None set.

### Notes


## Set Active Scene

- Kind: sub-action
- Code: 34
- Type: ObsSetActiveScene
- Category: OBS Studio
- Needs: an OBS connection (the one picked, else the first connected one)

Switches OBS to the given scene.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | The scene to switch to. Variables are filled in first. |

### Arguments

None set.

## Get Current Scene

- Kind: sub-action
- Code: 43
- Type: ObsGetCurrentScene
- Category: OBS Studio
- Needs: an OBS connection (the one picked, else the first connected one)

Reads the scene that is on the program output in OBS.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |

### Arguments

| Key | What it holds |
| --- | --- |
| `currentScene` | The name of the current scene. Set only when OBS answers. |

## Recording

- Kind: sub-action
- Code: 45
- Type: ObsRecording
- Category: OBS Studio
- Needs: an OBS connection (the one picked, else the first connected one)

Starts, stops, pauses, resumes or toggles the recording.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| State | `state` | One of Start, Stop, Pause, Resume, Toggle, stored as 0 to 4. |

### Arguments

None set.

### Notes


## Raw

- Kind: sub-action
- Code: 46
- Type: ObsRaw
- Category: OBS Studio
- Needs: an OBS connection (the one picked, else the first connected one)

Sends any OBS WebSocket request and keeps the answer.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Request type | `name` | The request name, such as `GetVersion`. Used when Raw JSON has no `requestType`. |
| Prefix | `prefix` | The start of the argument names for the answer. Default `obsRaw`. |
| Raw JSON | `raw` | A JSON object with `requestType` and `requestData`. Variables are filled in first. |
| Results to args | `resultsToArgs` | On by default. Adds the answer to the arguments. |

### Arguments

When Results to args is on, every key of the answer is added under the prefix, with dots between the names. For example, `obsRaw.currentProgramSceneName`. Lists add `[0]`, `[1]` and so on.

### Notes

- If Raw JSON has a `requestType`, that name is used and Request type is ignored. An older form of Raw JSON holds only the request data, and then Request type gives the name.
- If Raw JSON is not valid JSON, the request is sent with empty data.

## Take Screenshot

- Kind: sub-action
- Code: 48
- Type: ObsTakeScreenshot
- Category: OBS Studio
- Needs: an OBS connection (the one picked, else the first connected one)

Saves a screenshot of a source, or of the scene when no source is set.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | The scene. Used when Source is empty. Variables are filled in first. |
| Source | `sourceName` | The source to capture. Variables are filled in first. |
| File path | `path` | Where to save the file. The extension sets the image format, and PNG is used when there is none. |
| Quality | `quality` | A whole number, passed to OBS as the compression quality. |

### Arguments

None set.

## Get Status

- Kind: sub-action
- Code: 328
- Type: ObsGetStatus
- Category: OBS Studio
- Needs: an OBS connection (the one picked, else the first connected one)

Reads whether OBS is connected, streaming and recording.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |

### Arguments

| Key | What it holds |
| --- | --- |
| `obs.isConnected` | True when OBS answered the stream status request. |
| `obs.isStreaming` | True when the stream is running. |
| `obs.isRecording` | True when the recording is running. |

## Create Record Chapter

- Kind: sub-action
- Code: 329
- Type: ObsCreateRecordChapter
- Category: OBS Studio
- Needs: an OBS connection (the one picked, else the first connected one)

Adds a chapter marker to the current recording.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Chapter name | `chapterName` | The name of the chapter. Variables are filled in first. |

### Arguments

| Key | What it holds |
| --- | --- |
| `success` | True when OBS accepted the chapter. |
