# Integrations / HypeRate (triggers)

Heart rate readings and events from HypeRate.io, the service that shares your heart rate on stream.

## Heart Rate Pulse

- Kind: trigger
- Code: 6001
- Type: HypeRateHeartRatePulse
- Category: Integrations/HypeRate
- Needs: nothing (not wired up yet).

Meant to fire on each new heart rate reading from HypeRate. Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `heartRate` | The last heart rate in beats per minute (BPM). |

### When

Never. Streamerz doesn't fire this yet. In Streamer.bot it fires about once a second while HypeRate broadcasts.

### Notes

- Streamerz doesn't fire this yet.
- It can fire every second, so keep the action fast or put it in a blocking queue.

## Twitch Clip Created

- Kind: trigger
- Code: 6002
- Type: HypeRateTwitchClipCreated
- Category: Integrations/HypeRate
- Needs: nothing (not wired up yet).

Meant to fire when HypeRate itself creates a clip on your Twitch channel (its own clip feature). It is not a clip you made, and it is not a Twitch trigger. Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `clipUrl` | The address of the clip HypeRate created on Twitch. |

### When

Never. Streamerz doesn't fire this yet.

### Notes

- Streamerz doesn't fire this yet.
- To make a clip from an action, use the Twitch Create Clip sub-action instead.

## Connected

- Kind: trigger
- Code: 6003
- Type: HypeRateConnected
- Category: Integrations/HypeRate
- Needs: nothing (not wired up yet).

Meant to fire when the connection to HypeRate is made. Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

### Notes

- Streamerz doesn't fire this yet.

## Disconnected

- Kind: trigger
- Code: 6004
- Type: HypeRateDisconnected
- Category: Integrations/HypeRate
- Needs: nothing (not wired up yet).

Meant to fire when the connection to HypeRate is lost or closed. Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

### Notes

- Streamerz doesn't fire this yet.
