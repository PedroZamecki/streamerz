# Core / Globals (triggers)

Triggers for global variables being created, changed or deleted. Global variables are kept for everyone, and user global variables are kept per user.

## Global Variable Updated

- Kind: trigger
- Code: 709
- Type: GlobalVariableUpdated
- Category: Core/Globals
- Needs: nothing

Starts an action when a global variable that already has a value is written.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Variable (none: any) | `variableName` | The variable's name. Empty matches any variable. |
| Persisted | `persisted` | Any, Persisted or Temporary. Any matches both kinds. |

### Arguments

- `global.name`: the variable's name.
- `global.persisted`: true for a persisted variable, false for a temporary one.
- `global.lastWrite`: when the variable was written.
- `global.value`: the new value.
- `global.newValue`: the new value.
- `global.oldValue`: the value before the write.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time a global variable that already had a value is written. The value doesn't have to change.

## User Global Variable Updated

- Kind: trigger
- Code: 710
- Type: UserGlobalVariableUpdated
- Category: Core/Globals
- Needs: nothing

Starts an action when a user global variable that already has a value is written for a user.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Variable (none: any) | `variableName` | The variable's name. Empty matches any variable. |
| Persisted | `persisted` | Any, Persisted or Temporary. Any matches both kinds. |
| User login (none: any) | `userLogon` | The user's login. Empty matches any user. |
| Platform (none: any) | `platform` | The user's platform. Empty matches any platform. |

### Arguments

- `global.name`: the variable's name.
- `global.persisted`: true for a persisted variable, false for a temporary one.
- `global.lastWrite`: when the variable was written.
- `global.newValue`: the new value.
- `global.oldValue`: the value before the write.
- `user`, `userName`, `userId` and `userType`: the user, when the action that wrote the value had them.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time a user global variable that already had a value is written for a user that matches the filters.

### Notes

- The user arguments come from the action that wrote the value. They are missing when that action had no user.

## Global Variable Created

- Kind: trigger
- Code: 711
- Type: GlobalVariableCreated
- Category: Core/Globals
- Needs: nothing

Starts an action when a global variable gets a value for the first time.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Variable (none: any) | `variableName` | The variable's name. Empty matches any variable. |
| Persisted | `persisted` | Any, Persisted or Temporary. Any matches both kinds. |

### Arguments

- `global.name`: the variable's name.
- `global.persisted`: true for a persisted variable, false for a temporary one.
- `global.lastWrite`: when the variable was written.
- `global.value`: the new value.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time a global variable that had no value gets one.

### Notes

- User global variables don't fire this trigger. They fire User Global Variable Updated.

## Global Variable Deleted

- Kind: trigger
- Code: 712
- Type: GlobalVariableDeleted
- Category: Core/Globals
- Needs: nothing

Starts an action when a global variable is deleted.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Variable (none: any) | `variableName` | The variable's name. Empty matches any variable. |
| Persisted | `persisted` | Any, Persisted or Temporary. Any matches both kinds. |

### Arguments

- `global.name`: the variable's name.
- `global.value`: the value it had before it was deleted.
- `global.persisted`: true for a persisted variable, false for a temporary one.
- `global.lastWrite`: when the variable was deleted.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each time a variable that had a value is deleted from the Globals page.
