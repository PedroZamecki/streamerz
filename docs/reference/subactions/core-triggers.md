# Core / Triggers (sub-actions)

Sub-actions that turn other triggers on or off, and that fire the Custom trigger.

## Set Trigger State

- Kind: sub-action
- Code: 1033
- Type: SetTriggerState
- Category: Core/Triggers
- Needs: nothing

Enables, disables or toggles one trigger of an action. The trigger is found by its id inside the action given.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Action | `actionId` | The id of the action that has the trigger. |
| Trigger | `triggerId` | The id of the trigger inside that action. |
| State | `state` | Enable (0), Disable (1) or Toggle (2). |

### Arguments

None set.

### Notes

- Does nothing when the action or the trigger isn't found.

## Custom Event Trigger

- Kind: sub-action
- Code: 1034
- Type: CustomEventTrigger
- Category: Core/Triggers
- Needs: nothing

Fires the Custom trigger with an event name. Actions with a Custom trigger run when their event name is empty or the same as this one.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Event name | `eventName` | The name of the event. Variables in it are filled in. |
| Send the arguments | `useArgs` | On: the arguments of this run are sent with the event. |

### Arguments

None set.
