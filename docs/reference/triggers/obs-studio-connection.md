# OBS Studio / Connection (triggers)

Triggers for the obs-websocket connection to OBS Studio: when Streamerz gets in and when it drops.

## Connected

- Kind: trigger
- Code: 14001
- Type: ObsConnected
- Category: OBS Studio/Connection
- Needs: an OBS Studio connection (obs-websocket) set up in Streamerz

Fires when Streamerz connects to OBS Studio and the connection is identified.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (empty: any) | `connectionId` | The id of the OBS connection. Empty: any connection. |

### Arguments

- `obs.id`: the connection's id.
- `obs.name`: the connection's name.
- `obs.host`: the host and port of the connection.
- `obs.studioVersion`: the OBS Studio version, as OBS reported it.
- `obs.websocketVersion`: the obs-websocket version, as OBS reported it.
- `connectionId`: the same as `obs.id`.
- `connectionName`: the same as `obs.name`.

### When

Each time the connection is identified, including after a reconnect.

### Notes

- A connection that never gets in (a wrong password, for example) doesn't fire this trigger.

## Disconnected

- Kind: trigger
- Code: 14002
- Type: ObsDisconnected
- Category: OBS Studio/Connection
- Needs: an OBS Studio connection (obs-websocket) set up in Streamerz

Fires when a connection that was identified goes down, or when it is disconnected on purpose.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (empty: any) | `connectionId` | The id of the OBS connection. Empty: any connection. |

### Arguments

Same as Connected.

### When

When an identified connection closes: the user disconnects it, or OBS or the network drops it.

### Notes

- A connection that failed before it was identified doesn't fire this trigger.
