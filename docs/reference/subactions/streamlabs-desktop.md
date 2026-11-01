# Streamlabs Desktop (sub-actions)

Streaming and recording in Streamlabs Desktop. These run on a Streamlabs Desktop connection. Empty Connection uses the default connection, or the first one when none is the default.

## Streaming

- Kind: sub-action
- Code: 404
- Type: Streaming
- Category: Streamlabs Desktop
- Needs: a Streamlabs Desktop connection

Starts or stops the stream.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| State | `state` | 0 Stop, 1 Start. |

### Arguments

None set.

### Notes

- Nothing happens when the stream is already in that state.

## Recording

- Kind: sub-action
- Code: 415
- Type: Recording
- Category: Streamlabs Desktop
- Needs: a Streamlabs Desktop connection

Starts or stops the recording.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The Streamlabs Desktop connection id. Empty: the default connection. |
| State | `state` | 0 Start, 1 Stop, 2 Pause, 3 Resume. |

### Arguments

None set.

### Notes

- Pause and Resume do nothing. Streamlabs Desktop can't pause a recording.
- Nothing happens when the recording is already in the state asked for.
