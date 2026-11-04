# Servers & Remote / Remote Instances (triggers)

Triggers for remote instances. In Streamerz an instance is another Streamerz (or Streamer.bot) whose
WebSocket server one of your WebSocket clients (Servers page) connects to: once its server says who it is,
it is a remote instance, and Send Remote Trigger and Send Remote Signal reach it. (Streamer.bot links its
instances through its website, which Streamerz doesn't use.)

## Connected

- Kind: trigger
- Code: 29001
- Type: StreamerBotRemoteInstanceConnected
- Category: Servers & Remote/Remote Instances
- Needs: a WebSocket client (Servers page) connected to another instance's WebSocket server

Fires when a remote instance connects.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Instance | `instanceId` | The instance id to match. Empty or `Any` matches every instance. |

### Arguments

- `instanceId`: the instance's id.
- `instanceName`: the instance's name.

### When

When a WebSocket client connection is made, or when a client connects to the WebSocket server.

### Notes

- A client connected to the WebSocket server gets the id `remote-N` and the name `Client N`, where N counts the connections.

## Disconnected

- Kind: trigger
- Code: 29002
- Type: StreamerBotRemoteInstanceDisconnected
- Category: Servers & Remote/Remote Instances
- Needs: a WebSocket client (Servers page) connected to another instance's WebSocket server

Fires when a remote instance disconnects.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Instance | `instanceId` | The instance id to match. Empty or `Any` matches every instance. |

### Arguments

- `instanceId`: the instance's id.
- `instanceName`: the instance's name.

### When

When a WebSocket client connection closes, or when a client of the WebSocket server disconnects.

### Notes

None.

## Trigger

- Kind: trigger
- Code: 29003
- Type: StreamerBotRemoteInstanceTrigger
- Category: Servers & Remote/Remote Instances
- Needs: a WebSocket client (Servers page) connected to another instance's WebSocket server

Fires when a remote instance sends a trigger.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Instance | `instanceId` | The instance id to match. Empty or `Any` matches every instance. |
| Event Name | `eventName` | The event name to match. Empty matches every event name. |

### Arguments

- `instanceId`: the instance's id, or `Any` when the sender gave none.
- `eventName`: the event name the sender used.
- The arguments the sender sent with the trigger.

### When

When a trigger arrives from a remote instance: over a WebSocket client connection, through the WebSocket server (`ExecuteRemoteTrigger`, `SendRemoteTrigger` or `RemoteTrigger` requests), or when the Send Remote Trigger sub-action runs in Streamerz.

### Notes

- The Send Remote Trigger sub-action also fires this trigger in the Streamerz that sent it.

## Signal

- Kind: trigger
- Code: 29004
- Type: StreamerBotRemoteInstanceSignal
- Category: Servers & Remote/Remote Instances
- Needs: a WebSocket client (Servers page) connected to another instance's WebSocket server

Fires when a remote instance sends a signal.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Instance | `instanceId` | The instance id to match. Empty or `Any` matches every instance. |
| Signal Name | `signalName` | The signal name to match. Empty matches every signal name. |

### Arguments

- `instanceId`: the instance's id, or `Any` when the sender gave none.
- `signalName`: the signal name the sender used.
- The arguments the sender sent with the signal.

### When

When a signal arrives from a remote instance: over a WebSocket client connection, through the WebSocket server (`SendRemoteSignal` or `RemoteSignal` requests), or when the Send Remote Signal sub-action runs in Streamerz.

### Notes

- A signal that nobody is waiting for is kept, and the next Wait For Remote Signal sub-action picks it up.
- The Send Remote Signal sub-action also fires this trigger in the Streamerz that sent it.
