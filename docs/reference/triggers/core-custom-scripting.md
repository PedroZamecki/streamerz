# Core / Custom & Scripting (triggers)

Triggers for custom events, test runs, the raw events of actions, and requests sent to Streamerz's WebSocket server.

## Custom

- Kind: trigger
- Code: 1
- Type: Custom
- Category: Core/Custom & Scripting
- Needs: nothing

Meant for custom events. Streamerz doesn't fire it yet.

### Fields

None.

### Arguments

None set.

### When

Never, for now.

### Notes

- Streamerz doesn't fire this yet.
- Custom events use the Custom Event trigger (code 18001) instead.

## Test

- Kind: trigger
- Code: 702
- Type: Test
- Category: Core/Custom & Scripting
- Needs: nothing

Runs an action as a test.

### Fields

None.

### Arguments

- `isTest`: true.
- `triggerName`: `Test`.
- `triggerCategory`: `Core`.
- `eventSource`: `streamerbot`.

### When

When you press Test on the action in the Actions list.

### Notes

- The trigger list also has a test that runs the action with the arguments you type, as that trigger would pass them.

## Raw Action

- Kind: trigger
- Code: 801
- Type: RawAction
- Category: Core/Custom & Scripting
- Needs: nothing

The raw event for an action starting. WebSocket clients get it; no action is started by it.

### Fields

None.

### Arguments

None set.

### When

Never, for now.

### Notes

- Streamerz doesn't fire this yet.
- WebSocket clients that subscribe to the `Raw` events get an `Action` event when an action starts.

## Raw Sub Action

- Kind: trigger
- Code: 802
- Type: RawSubAction
- Category: Core/Custom & Scripting
- Needs: nothing

The raw event for a sub-action about to run. WebSocket clients get it; no action is started by it.

### Fields

None.

### Arguments

None set.

### When

Never, for now.

### Notes

- Streamerz doesn't fire this yet.
- WebSocket clients that subscribe to the `Raw` events get a `SubAction` event before each sub-action runs. Groups and the If/Else and Switch containers are left out.

## Raw Action Completed

- Kind: trigger
- Code: 806
- Type: RawActionCompleted
- Category: Core/Custom & Scripting
- Needs: nothing

The raw event for an action that has finished. WebSocket clients get it; no action is started by it.

### Fields

None.

### Arguments

None set.

### When

Never, for now.

### Notes

- Streamerz doesn't fire this yet.
- WebSocket clients that subscribe to the `Raw` events get an `ActionCompleted` event when an action ends, with its duration, its start and end times, and its arguments.

## Custom Event

- Kind: trigger
- Code: 18001
- Type: CustomEvent
- Category: Core/Custom & Scripting
- Needs: nothing

Fires when a Custom Event Trigger sub-action runs with this event name.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Event name | `eventName` | The event name to match. Empty matches every event name. |

### Arguments

The arguments of the action that ran the Custom Event Trigger sub-action, when that sub-action has `useArgs` on. Otherwise none.

### When

Each time an action runs a Custom Event Trigger sub-action whose event name is this trigger's `eventName`, or any name when `eventName` is empty. The sub-action's own event name is filled in with variables first.

### Notes

- The names must match exactly, with the same letter case.

## Custom Code Event

- Kind: trigger
- Code: 18002
- Type: CustomCodeEvent
- Category: Core/Custom & Scripting
- Needs: a WebSocket client connected to Streamerz

Fires when a WebSocket client sends an `ExecuteCodeTrigger` request with this trigger's name.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Trigger name | `triggerName` | The name to match, as the request sends it. |

### Arguments

The `args` object sent with the request, as it is.

### When

Each time an `ExecuteCodeTrigger` request arrives with a `triggerName` that matches this trigger's name.

### Notes

- Only the actions triggered by name run. C# code triggers aren't run by Streamerz.
