# OBS Studio / Streaming & Recording (triggers)

Triggers for the stream and the recording in OBS Studio starting and stopping.

## Streaming Started

- Kind: trigger
- Code: 14005
- Type: ObsStreamingStarted
- Category: OBS Studio/Streaming & Recording
- Needs: an OBS Studio connection (obs-websocket) set up in Streamerz

Fires when OBS Studio starts the live stream.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (empty: any) | `connectionId` | The id of the OBS connection. Empty: any connection. |

### Arguments

- `obs.id`, `obs.name`, `obs.host`, `obs.studioVersion`, `obs.websocketVersion`, `connectionId`, `connectionName`: as in Connected.
- `eventType` and `obsEvent.eventType`: `StreamStateChanged`.
- `outputState` and `obsEvent.outputState`: `OBS_WEBSOCKET_OUTPUT_STARTED`.

### When

When OBS reports the stream output as started, and the connection matches the filter.

### Notes

- The "starting" state OBS sends first doesn't fire anything. Only the started and stopped states do.

## Streaming Stopped

- Kind: trigger
- Code: 14006
- Type: ObsStreamingStopped
- Category: OBS Studio/Streaming & Recording
- Needs: an OBS Studio connection (obs-websocket) set up in Streamerz

Fires when OBS Studio stops the live stream.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (empty: any) | `connectionId` | The id of the OBS connection. Empty: any connection. |

### Arguments

Same as Streaming Started, with `outputState` set to `OBS_WEBSOCKET_OUTPUT_STOPPED`.

### When

When OBS reports the stream output as stopped, and the connection matches the filter.

### Notes

- The "starting" and "stopping" states OBS sends first don't fire anything. Only the started and stopped states do.

## Recording Started

- Kind: trigger
- Code: 14007
- Type: ObsRecordingStarted
- Category: OBS Studio/Streaming & Recording
- Needs: an OBS Studio connection (obs-websocket) set up in Streamerz

Fires when OBS Studio starts a recording.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (empty: any) | `connectionId` | The id of the OBS connection. Empty: any connection. |

### Arguments

- `obs.id`, `obs.name`, `obs.host`, `obs.studioVersion`, `obs.websocketVersion`, `connectionId`, `connectionName`: as in Connected.
- `eventType` and `obsEvent.eventType`: `RecordStateChanged`.
- `outputState` and `obsEvent.outputState`: `OBS_WEBSOCKET_OUTPUT_STARTED`.

### When

When OBS reports the recording output as started, and the connection matches the filter.

### Notes

- The "starting" and "stopping" states OBS sends first don't fire anything. Only the started and stopped states do.

## Recording Stopped

- Kind: trigger
- Code: 14008
- Type: ObsRecordingStopped
- Category: OBS Studio/Streaming & Recording
- Needs: an OBS Studio connection (obs-websocket) set up in Streamerz

Fires when OBS Studio stops a recording.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (empty: any) | `connectionId` | The id of the OBS connection. Empty: any connection. |

### Arguments

Same as Recording Started, with `outputState` set to `OBS_WEBSOCKET_OUTPUT_STOPPED`.

### When

When OBS reports the recording output as stopped, and the connection matches the filter.

### Notes

- The "starting" and "stopping" states OBS sends first don't fire anything. Only the started and stopped states do.
