# Streamer.bot / Remote (sub-actions)

Sends triggers and signals between instances, and waits for a signal to arrive.

## Send Remote Trigger

- Kind: sub-action
- Code: 20001
- Type: StreamerBotRemoteInstanceSendTrigger
- Category: Streamer.bot/Remote
- Needs: nothing

Sends a trigger to an instance. Streamerz fires the Remote Instance Trigger here, so the triggers with a matching instance and event name run. It also sends the trigger to the clients that subscribe to Streamerz events.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Instance | `instanceId` | The instance ID. Empty means any instance. |
| Event Name | `eventName` | The event name. Triggers with the same name match. |
| Use current arguments | `useArge` | On: the arguments of the running action go along with the trigger. |
| Custom arguments | `customArgs` | Name and text pairs added to the sent arguments. Variables are filled in. |

### Arguments

None set.

### Notes

- The sent arguments also carry `instanceId`, `instanceName` (this instance's name) and `instanceOs` (`windows` or `linux`).
- The key is spelled `useArge` in the saved settings.

## Send Remote Signal

- Kind: sub-action
- Code: 20002
- Type: StreamerBotRemoteInstanceSendSignal
- Category: Streamer.bot/Remote
- Needs: nothing

Sends a signal to an instance. If a Wait For Remote Signal is waiting for it, that action gets the signal. If nothing waits, the signal is kept until one does.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Instance | `instanceId` | The instance ID. Empty means any instance. |
| Signal Name | `signalName` | The signal name. Waits with the same name match. |
| Use current arguments | `useArge` | On: the arguments of the running action go along with the signal. |
| Custom arguments | `customArgs` | Name and text pairs added to the sent arguments. Variables are filled in. |

### Arguments

None set.

### Notes

- The signal carries the same `instanceId`, `instanceName` and `instanceOs` values as a remote trigger.
- A kept signal stays in memory until a wait takes it. Streamerz doesn't save it to disk.

## Wait For Remote Signal

- Kind: sub-action
- Code: 20003
- Type: StreamerBotRemoteInstanceWaitForSignal
- Category: Streamer.bot/Remote
- Needs: nothing

Waits for a signal with the given name. A signal that was already sent and kept is taken at once. The actions after this one wait until a signal comes or the time runs out.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Instance | `instanceId` | The instance ID. Empty or `Any` takes the signal from any instance. |
| Signal Name | `signalName` | The signal name to wait for. |
| Overwrite | `overwrite` | On: values from the signal replace the arguments that are already set. Off: they only fill in the missing ones. |
| Timeout (ms) | `timeout` | How long to wait, in milliseconds. 0 means 30 seconds. The most is 60 seconds. |

### Arguments

- The values the signal carried, merged in as set by Overwrite.
- `hasResult`: true when a signal came.
- `signalTimeout`: true when the time ran out.
- `instanceId` and `signalName`: set when a signal came.

### Notes

- A wait with an empty or `Any` instance also takes signals sent from any instance.
