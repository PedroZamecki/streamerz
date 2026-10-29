# Streamlabs Desktop / Streaming & Recording (triggers)

Triggers for streaming and recording in Streamlabs Desktop: when they start and when they stop.

## Streaming Started

- Kind: trigger
- Code: 22004
- Type: StreamlabsDesktopStreamingStarted
- Category: Streamlabs Desktop/Streaming & Recording
- Needs: a Streamlabs Desktop connection set up in Streamerz (its host, port and token)

Fires when Streamlabs Desktop starts streaming.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (none: any) | `sdId` | The connection id to match. Empty (or the all-zero id) matches any connection. |

### Arguments

- `sd.id`: the connection id.
- `sd.name`: the connection name.
- `sd.host`: the host the connection uses.

### When

When Streamlabs Desktop reports that the stream is starting.

### Notes

- Streamerz also notes the start time, and checks YouTube and TikTok faster for about 90 seconds after it.

## Streaming Stopped

- Kind: trigger
- Code: 22005
- Type: StreamlabsDesktopStreamingStopped
- Category: Streamlabs Desktop/Streaming & Recording
- Needs: a Streamlabs Desktop connection set up in Streamerz (its host, port and token)

Fires when Streamlabs Desktop stops streaming.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (none: any) | `sdId` | The connection id to match. Empty (or the all-zero id) matches any connection. |

### Arguments

- `sd.id`: the connection id.
- `sd.name`: the connection name.
- `sd.host`: the host the connection uses.

### When

When Streamlabs Desktop reports that the stream is ending.

### Notes

None.

## Recording Started

- Kind: trigger
- Code: 22006
- Type: StreamlabsDesktopRecordingStarted
- Category: Streamlabs Desktop/Streaming & Recording
- Needs: a Streamlabs Desktop connection set up in Streamerz (its host, port and token)

Fires when Streamlabs Desktop starts recording.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (none: any) | `sdId` | The connection id to match. Empty (or the all-zero id) matches any connection. |

### Arguments

- `sd.id`: the connection id.
- `sd.name`: the connection name.
- `sd.host`: the host the connection uses.

### When

When Streamlabs Desktop reports that the recording has started.

### Notes

None.

## Recording Stopped

- Kind: trigger
- Code: 22007
- Type: StreamlabsDesktopRecordingStopped
- Category: Streamlabs Desktop/Streaming & Recording
- Needs: a Streamlabs Desktop connection set up in Streamerz (its host, port and token)

Fires when Streamlabs Desktop stops recording.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (none: any) | `sdId` | The connection id to match. Empty (or the all-zero id) matches any connection. |

### Arguments

- `sd.id`: the connection id.
- `sd.name`: the connection name.
- `sd.host`: the host the connection uses.

### When

When Streamlabs Desktop reports that the recording is stopping.

### Notes

None.
