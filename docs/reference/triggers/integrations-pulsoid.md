# Integrations / Pulsoid (triggers)

Heart rate readings from Pulsoid, the app that shares your heart rate from a watch or band.

## Heart Rate Pulse

- Kind: trigger
- Code: 5001
- Type: PulsoidHeartRatePulse
- Category: Integrations/Pulsoid
- Needs: nothing (not wired up yet).

Meant to fire on each new heart rate reading from Pulsoid. Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `heartRate` | The last heart rate in beats per minute (BPM). |

### When

Never. Streamerz doesn't fire this yet. In Streamer.bot it fires about once a second while Pulsoid broadcasts.

### Notes

- Streamerz doesn't fire this yet.
- It can fire every second, so keep the action fast or put it in a blocking queue.
