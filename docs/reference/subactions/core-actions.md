# Core / Actions (sub-actions)

Sub-actions that run other actions, and turn actions and their sub-actions on or off.

## Run Action

- Kind: sub-action
- Code: 4
- Type: Action
- Category: Core/Actions
- Needs: nothing

Runs another action.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Action | `actionId` | The action to run. |
| Run immediately | `runImmedately` | On: run the action's sub-actions now. Off: put the action in its queue. |

### Arguments

None set.

### Notes

- With Run immediately off, the action waits in its queue and runs with the arguments of this action.

## Set Action State

- Kind: sub-action
- Code: 1004
- Type: SetActionState
- Category: Core/Actions
- Needs: nothing

Turns an action on, off, or toggles it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Action | `actionId` | The action to change. |
| State | `state` | Enable, Disable or Toggle. |

### Arguments

None set.

## Set Action Group State

- Kind: sub-action
- Code: 1011
- Type: SetActionGroupState
- Category: Core/Actions
- Needs: nothing

Turns every action in a group on, off, or toggles each of them.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group | `groupName` | The name of the action group. |
| State | `state` | Enable, Disable or Toggle. |

### Arguments

None set.

## Get Action State

- Kind: sub-action
- Code: 1016
- Type: GetActionState
- Category: Core/Actions
- Needs: nothing

Gets whether an action is on.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Action | `actionId` | The action to check. |

### Arguments

- `actionState`: true when the action is on. False when it is off or not found.

## Get Action Group State

- Kind: sub-action
- Code: 1017
- Type: GetActionGroupState
- Category: Core/Actions
- Needs: nothing

Gets whether the actions in a group are on, off, or mixed.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group | `groupName` | The name of the action group. |

### Arguments

- `actionGroupState`: 0 when all the actions are off, 1 when all are on, 2 when they are mixed.
- `actionsEnabled`: the ids of the actions that are on.
- `actionsDisabled`: the ids of the actions that are off.

## Set Sub-Action State

- Kind: sub-action
- Code: 1028
- Type: SetSubActionState
- Category: Core/Actions
- Needs: nothing

Turns a sub-action of an action on, off, or toggles it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Action | `actionId` | The action that holds the sub-action. |
| Sub-action | `subActionId` | The sub-action to change, inside that action. |
| State | `state` | Enable, Disable or Toggle. |

### Arguments

None set.

## Set Sub-Action Weight

- Kind: sub-action
- Code: 1038
- Type: SetSubActionWeight
- Category: Core/Actions
- Needs: nothing

Sets the weight of a sub-action. A weight decides how often the sub-action is picked when its group or action runs at random.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Action | `actionId` | The action that holds the sub-action. |
| Sub-action | `subActionId` | The sub-action to change, inside that action. |
| Weight | `weight` | The new weight, as a number. |

### Arguments

None set.
