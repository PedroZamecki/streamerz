# OBS Studio / Scenes & Sources (triggers)

Triggers for scene changes in OBS Studio.

## Scene Changed

- Kind: trigger
- Code: 14004
- Type: ObsSceneChanged
- Category: OBS Studio/Scenes & Sources
- Needs: an OBS Studio connection (obs-websocket) set up in Streamerz

Fires when the active program scene in OBS Studio changes.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (empty: any) | `connectionId` | The id of the OBS connection. Empty: any connection. |
| Scene (empty: any) | `sceneName` | The scene's name. Compared without case. Empty: any scene. |

### Arguments

- `obs.id`, `obs.name`, `obs.host`, `obs.studioVersion`, `obs.websocketVersion`, `connectionId`, `connectionName`: as in Connected.
- `eventType` and `obsEvent.eventType`: `CurrentProgramSceneChanged`.
- `sceneName` and `obs.sceneName`: the new scene's name.
- `obs.oldSceneName`: the scene that was active before, as Streamerz last saw it. Empty when Streamerz hasn't seen one yet.
- The event's data keys, as in Event.

### When

When OBS reports that the current program scene changed, and the connection and scene match the filters.

### Notes

- The filter compares the new scene's name, so set it to the scene you want to react to.
