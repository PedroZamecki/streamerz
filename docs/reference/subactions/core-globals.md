# Core / Globals (sub-actions)

Global variables: values that stay set for every action, either saved with the settings or kept only until Streamerz closes.

## Global (Get)

- Kind: sub-action
- Code: 121
- Type: Global (Get)
- Category: Core/Globals
- Needs: nothing

Reads a global variable into an argument. Source picks whose variable it is: the global one, the one of the user who ran the action, or the one of the target user.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Source | `source` | 0 Global, 1 User, 2 Target user. |
| Variable name | `variableName` | The name of the variable to read. |
| Persisted | `persisted` | On: the saved variable. Off: the temporary one. |
| Destination variable | `destinationVariable` | The name of the argument that gets the value. |
| Default value | `defaultValue` | Text. Used when the variable isn't set. |

### Arguments

Adds the argument named in `destinationVariable`, with the value read. It is null when the variable isn't set (and there's no default) or when the user isn't known.

### Notes

- A default value is saved as the variable's value when it's used, so the next read finds it. A number, `true` or `false` is stored as that type.
- For a user or the target user, the user has to be in the action's arguments (the trigger gives it). When it isn't, the destination argument is null.

## Global (Set)

- Kind: sub-action
- Code: 122
- Type: Global (Set)
- Category: Core/Globals
- Needs: nothing

Sets a global variable to a value, adds a number to it or subtracts one, or removes it. Destination picks whose variable it is, the same way as Global (Get).

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Destination | `destination` | 0 Global, 1 User, 2 Target user. |
| Variable name | `variableName` | The name of the variable to change. |
| Persisted | `persisted` | On: saved with the settings. Off: kept until Streamerz closes. |
| Set | `source` | 0 Value, 1 Increment, 2 Decrement, 3 Unset. |
| Value | `value` | The text to set, or the number to add or subtract. Variables are filled in first. |
| Auto type | `autoType` | On: a value that looks like a number, `true` or `false` is stored as that type. |

### Arguments

None set.

### Notes

- Increment and Decrement need a number in Value. Otherwise nothing changes.
- Unset removes the variable.
- Every change fires the variable's updated trigger, even when the value stays the same. Unsetting a variable that isn't set does nothing.
- When the user or the target user isn't known, nothing is set.

## Clear Non-Persisted Globals

- Kind: sub-action
- Code: 125
- Type: Clear Non-Persisted Globals
- Category: Core/Globals
- Needs: nothing

Forgets every temporary global variable, the ones that aren't persisted.

### Fields

None.

### Arguments

None set.

### Notes

- Only the global ones are cleared. Use Clear Non-Persisted User Globals for the user ones.

## Clear Non-Persisted User Globals

- Kind: sub-action
- Code: 126
- Type: Clear Non-Persisted User Globals
- Category: Core/Globals
- Needs: nothing

Forgets every temporary user variable, the ones that aren't persisted, for all users.

### Fields

None.

### Arguments

None set.

### Notes

- Saved user variables are kept. Global variables are not touched (see Clear Non-Persisted Globals).
