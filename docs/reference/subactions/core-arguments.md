# Core / Arguments (sub-actions)

Arguments: named values that an action passes on to the sub-actions after it. Use them as `%name%` in any text field.

## Set Argument

- Kind: sub-action
- Code: 123
- Type: Set Argument
- Category: Core/Arguments
- Needs: nothing

Sets an argument. The sub-actions after this one can use it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Variable name | `variableName` | The name of the argument. |
| Value | `value` | Text, several lines. Variables are filled in first. |
| Auto type | `autoType` | On: a value that looks like a whole number, a decimal, `true` or `false` is stored as that type. |

### Arguments

Sets the argument named in Variable name to the value.

### Notes

- Without Auto type the value is always text.
