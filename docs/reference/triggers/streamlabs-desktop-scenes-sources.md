# Streamlabs Desktop / Scenes & Sources (triggers)

Triggers for scenes in Streamlabs Desktop, such as a scene switch.

## Scene Changed

- Kind: trigger
- Code: 22003
- Type: StreamlabsDesktopSceneChanged
- Category: Streamlabs Desktop/Scenes & Sources
- Needs: a Streamlabs Desktop connection set up in Streamerz (its host, port and token)

Fires when Streamlabs Desktop switches to another scene.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (none: any) | `sdId` | The connection id to match. Empty (or the all-zero id) matches any connection. |
| Scene (empty: any) | `sceneName` | The scene's name, exactly as Streamlabs Desktop has it. Empty matches any scene. |

### Arguments

- `sd.id`: the connection id.
- `sd.name`: the connection name.
- `sd.host`: the host the connection uses.
- `sd.sceneName`: the scene Streamlabs Desktop switched to.

### When

When Streamlabs Desktop reports a scene switch on a connection that is up.

### Notes

- The scene name in the filter has to match the scene name exactly.
