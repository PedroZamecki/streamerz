# Elgato / Stream Deck (triggers)

Triggers for the Elgato Stream Deck: key presses, the device connecting and disconnecting, and the Stream Deck MCP server.

## Action

- Kind: trigger
- Code: 17001
- Type: StreamDeckAction
- Category: Elgato/Stream Deck
- Needs: an Elgato Stream Deck connection (not wired up yet)

Meant for a Stream Deck key or dial action.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Button ID | `buttonId` | The id of the button. Empty: any button. |

### Arguments

None set.

### When

Never: Streamer.bot 1.0.7 drops this trigger from actions. A button press runs the action the button
names, with `sdButtonId`, `sdButtonUuid` and the button's own arguments (`eventSource` is `streamdeck`).

## Connected

- Kind: trigger
- Code: 17002
- Type: StreamDeckConnected
- Category: Elgato/Stream Deck
- Needs: an Elgato Stream Deck connection (not wired up yet)

Meant for a Stream Deck connecting.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Device | `device` | The device's name. Empty: any device. |

### Arguments

None set.

### When

When the Stream Deck plugin connects to the Stream Deck server (Servers page). No other arguments.

## Disconnected

- Kind: trigger
- Code: 17003
- Type: StreamDeckDisconnected
- Category: Elgato/Stream Deck
- Needs: an Elgato Stream Deck connection (not wired up yet)

Meant for a Stream Deck disconnecting.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Device | `device` | The device's name. Empty: any device. |

### Arguments

None set.

### When

When the plugin disconnects. No other arguments.

## Info

- Kind: trigger
- Code: 17004
- Type: StreamDeckInfo
- Category: Elgato/Stream Deck
- Needs: an Elgato Stream Deck connection (not wired up yet)

Meant for the information a Stream Deck reports, such as its layout.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Device | `device` | The device's name. Empty: any device. |

### Arguments

None set.

### When

Never: Streamer.bot 1.0.7 doesn't fire it with its current plugin either, which sends no device
information.

## Mcp Connected

- Kind: trigger
- Code: 21501
- Type: ElgatoStreamDeckMcpConnected
- Category: Elgato/Stream Deck
- Needs: the Stream Deck MCP server (not wired up yet)

Meant for the Stream Deck MCP server connecting.

### Fields

None.

### Arguments

None set.

### When

Never, for now. Streamerz doesn't fire this yet.

### Notes

- Streamerz doesn't fire this yet.

## Mcp Disconnected

- Kind: trigger
- Code: 21502
- Type: ElgatoStreamDeckMcpDisconnected
- Category: Elgato/Stream Deck
- Needs: the Stream Deck MCP server (not wired up yet)

Meant for the Stream Deck MCP server disconnecting.

### Fields

None.

### Arguments

None set.

### When

Never, for now. Streamerz doesn't fire this yet.

### Notes

- Streamerz doesn't fire this yet.
