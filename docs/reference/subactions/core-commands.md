# Core / Commands (sub-actions)

Sub-actions that turn chat commands on and off, and read their state and their aliases.

## Set Command State

- Kind: sub-action
- Code: 9
- Type: SetCommandState
- Category: Core/Commands
- Needs: nothing

Turns a command on, off, or toggles it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command | `commandId` | The command to change. |
| State | `state` | Enable, Disable or Toggle. |

### Arguments

None set.

## Set Command Group State

- Kind: sub-action
- Code: 1010
- Type: SetCommandGroupState
- Category: Core/Commands
- Needs: nothing

Turns every command in a group on, off, or toggles each of them.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group | `groupName` | The name of the command group. |
| State | `state` | Enable, Disable or Toggle. |

### Arguments

None set.

## Get Commands

- Kind: sub-action
- Code: 1013
- Type: GetCommands
- Category: Core/Commands
- Needs: nothing

Gets the names of the commands in a group, as one text.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group | `groupName` | The group to list. Empty lists every command. |
| Variable name | `variableName` | The variable that gets the names. |
| Include all commands | `includeAll` | On: include the commands that are left out of lists. |
| First alias only | `ignoreAliases` | On: list only the first name of each command. |
| Only permitted ones | `hasPermission` | On: list only the commands the user in the arguments may use. |

### Arguments

- The variable named by `variableName` holds the names, sorted and joined with ", ".

### Notes

- Commands that are turned off are never listed.
- Only permitted ones uses the user, moderator, VIP and subscriber arguments of the action to decide.

## Get Command State

- Kind: sub-action
- Code: 1014
- Type: GetCommandState
- Category: Core/Commands
- Needs: nothing

Gets whether a command is on.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command | `commandId` | The command to check. |

### Arguments

- `commandState`: true when the command is on.

## Get Command Group State

- Kind: sub-action
- Code: 1015
- Type: GetCommandGroupState
- Category: Core/Commands
- Needs: nothing

Gets whether the commands in a group are on, off, or mixed.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group | `groupName` | The command group. Empty or All covers every command. |

### Arguments

- `commandGroupState`: 0 when all the commands are off, 1 when all are on, 2 when they are mixed.
- `commandsEnabled`: the ids of the commands that are on.
- `commandsDisabled`: the ids of the commands that are off.
