# TikTok / Connection (triggers)

Streamerz connecting to and leaving a TikTok Live. They need a TikTok channel set: Streamerz connects by itself once it is set.

## Connected

- Kind: trigger
- Code: 41008
- Type: TikTokConnected
- Category: TikTok/Connection
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when connected to the TikTok channel's live.

### Fields

None.

### Arguments

- `channel`: the channel as set.
- `roomId`: the id of the live room.

### When

Fires each time Streamerz connects to the live room, including after a drop and reconnect to the same live. When the live is new, Stream Online fires first.

### Notes

None.

## Disconnected

- Kind: trigger
- Code: 41009
- Type: TikTokDisconnected
- Category: TikTok/Connection
- Needs: a TikTok channel set (with none, nothing is opened)

Fires when the TikTok live ends or the connection drops.

### Fields

None.

### Arguments

- `channel`: the channel as set.

### When

Fires when Streamerz was connected and the connection stops. That happens when the live ends, the connection fails, the channel isn't found, or the connection is closed by hand. It doesn't fire when Streamerz was not connected.

### Notes

- When the live ends, Stream Offline fires first, then this one.
