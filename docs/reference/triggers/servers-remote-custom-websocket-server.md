# Servers & Remote / Custom WebSocket Server (triggers)

Connections and messages on the custom WebSocket servers you set up on the Servers page.

## Server Open

- Kind: trigger
- Code: 2001
- Type: WebsocketCustomServerOpen
- Category: Servers & Remote/Custom WebSocket Server
- Needs: a custom WebSocket server on the Servers page, started

Starts an action when a client connects to the custom server.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Server | `serverId` | The server to watch. The trigger matches only that server. |

### Arguments

- `sessionId`: an id for this connection. Each client gets its own.
- `ip`: the address of the client.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time a client connects.

### Notes

- A custom server only starts by itself when it's set to start on its own.

## Server Close

- Kind: trigger
- Code: 2002
- Type: WebsocketCustomServerClose
- Category: Servers & Remote/Custom WebSocket Server
- Needs: a custom WebSocket server on the Servers page, started

Starts an action when a client disconnects.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Server | `serverId` | The server to watch. The trigger matches only that server. |

### Arguments

- `sessionId`: the id of the connection that closed.
- `ip`: the address of the client.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time a client disconnects.

## Server Message

- Kind: trigger
- Code: 2003
- Type: WebsocketCustomServerMessage
- Category: Servers & Remote/Custom WebSocket Server
- Needs: a custom WebSocket server on the Servers page, started

Starts an action for each message a client sends to the custom server.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Server | `serverId` | The server to watch. The trigger matches only that server. |

### Arguments

- `sessionId`: the id of the connection that sent the message.
- `ip`: the address of the client.
- `data`: the text of the message. Binary messages are read as text.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time a client sends a text or binary message.
