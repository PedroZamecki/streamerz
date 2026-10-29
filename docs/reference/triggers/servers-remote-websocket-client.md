# Servers & Remote / WebSocket Client (triggers)

Triggers for the WebSocket clients you set up on the Servers page: connecting, closing and messages.

## Open

- Kind: trigger
- Code: 803
- Type: WebsocketClientOpen
- Category: Servers & Remote/WebSocket Client
- Needs: a WebSocket client on the Servers page

Starts an action when a WebSocket client connects to its endpoint.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Client | `clientId` | The client to watch. The trigger matches only that client. |

### Arguments

None beyond the ones every trigger gets: `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time the client connects, including after a reconnect.

### Notes

- A client only connects by itself when Connect on start is on.

## Close

- Kind: trigger
- Code: 804
- Type: WebsocketClientClose
- Category: Servers & Remote/WebSocket Client
- Needs: a WebSocket client on the Servers page

Starts an action when a WebSocket client's connection closes.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Client | `clientId` | The client to watch. The trigger matches only that client. |

### Arguments

None beyond the ones every trigger gets: `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time the connection closes, and each time a try to connect fails.

### Notes

- With Reconnect on, the client tries again after the retry interval (5 seconds by default) once a connection drops.

## Message

- Kind: trigger
- Code: 805
- Type: WebsocketClientMessage
- Category: Servers & Remote/WebSocket Client
- Needs: a WebSocket client on the Servers page

Starts an action for each message a WebSocket client receives.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Client | `clientId` | The client to watch. The trigger matches only that client. |

### Arguments

- `message`: the text of the message. Binary messages are read as text.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time the client receives a text or binary message.

### Notes

- A message that carries a remote instance's event also runs the matching remote trigger.
