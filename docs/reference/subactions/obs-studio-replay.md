# OBS Studio / Replay (sub-actions)

The replay buffer of OBS Studio.

## Set Replay Buffer State

- Kind: sub-action
- Code: 321
- Type: Set Replay Buffer State
- Category: OBS Studio/Replay
- Needs: an OBS Studio connection (empty: whichever one is up)

Starts, stops, saves or toggles the replay buffer in OBS.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection id. Empty: any OBS connection that is up. |
| State | `state` | One of Start, Stop, Save, Toggle, stored as 0 to 3. |

### Arguments

None set.

### Notes

