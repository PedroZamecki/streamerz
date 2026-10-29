# Streamlabs Desktop / Connection (triggers)

Triggers for the connection between Streamerz and Streamlabs Desktop: when it comes up and when it goes down.

## Connected

- Kind: trigger
- Code: 22001
- Type: StreamlabsDesktopConnected
- Category: Streamlabs Desktop/Connection
- Needs: a Streamlabs Desktop connection set up in Streamerz (its host, port and token)

Fires when Streamerz is connected to Streamlabs Desktop and Streamlabs has accepted the token.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (none: any) | `sdId` | The connection id to match. Empty (or the all-zero id) matches any connection. |

### Arguments

- `sd.id`: the connection id.
- `sd.name`: the connection name.
- `sd.host`: the host the connection uses.

### When

When a Streamlabs Desktop connection is up, right after Streamlabs accepts its token.

### Notes

- A connection starts when it is set to connect on start, or when you connect it by hand.

## Disconnected

- Kind: trigger
- Code: 22002
- Type: StreamlabsDesktopDisconnected
- Category: Streamlabs Desktop/Connection
- Needs: a Streamlabs Desktop connection set up in Streamerz (its host, port and token)

Fires when a Streamlabs Desktop connection that was up goes down.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection (none: any) | `sdId` | The connection id to match. Empty (or the all-zero id) matches any connection. |

### Arguments

- `sd.id`: the connection id.
- `sd.name`: the connection name.
- `sd.host`: the host the connection uses.

### When

When a connection that was up drops, or when you disconnect it in Streamerz. A connection that never came up doesn't fire it.

### Notes

- With reconnect on, Streamerz tries again after the connection's retry interval. Each drop fires this trigger once.
