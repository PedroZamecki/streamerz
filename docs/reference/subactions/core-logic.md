# Core / Logic (sub-actions)

Sub-actions that decide what runs next: branches, switches, loops, random numbers and input from the user.

## If/Else

- Kind: sub-action
- Code: 120
- Type: LogicIf
- Category: Core/Logic
- Needs: nothing

Runs the first branch when the comparison is true, and the second branch when it is false.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Input | `input` | The text to test. Variables are filled in first. |
| Operation | `operation` | One of Equals, Not Equals, Contains, Regex Match, Less Than, Greater Than, Does Not Exist, Equals (Ignore Case), Not Equals (Ignore Case), Is Null or Empty, stored as 0 to 9. |
| Value | `value` | The text to compare with. Variables are filled in first. |
| Auto type | `autoType` | On: numbers are compared as numbers. |

### Arguments

For Regex Match, the groups of the match are added: `match[0]` for the whole match, then one entry for each group. Named groups are also added under their name.

### Notes

- Contains and the Ignore Case operations do not care about upper or lower case. Equals and Not Equals do, unless you use the Ignore Case ones.
- Without Auto type, Less Than and Greater Than compare the text, so `10` is less than `9`.
- Does Not Exist checks whether an argument with the name in Input exists. Write the argument name with or without `%`. It does not look at the text.
- Is Null or Empty checks the Input text only.

## Break

- Kind: sub-action
- Code: 124
- Type: Break
- Category: Core/Logic
- Needs: nothing

Stops the whole action. This also stops Run Action sub-actions that the action started inline.

### Fields

None.

### Arguments

None set.

## Switch

- Kind: sub-action
- Code: 127
- Type: LogicSwitch
- Category: Core/Logic
- Needs: nothing

Runs the first case whose values match the input. If none matches, the default case runs.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Input | `input` | The text to test. Variables are filled in first. |
| Auto type | `autoType` | On: a case value also matches when it is a number equal to the input. |

Each case holds its own values and a Case sensitive setting.

### Arguments

None set.

### Notes

- Cases are checked from top to bottom. Only the first match runs.
- Without Case sensitive, the match ignores upper and lower case.

## While

- Kind: sub-action
- Code: 128
- Type: LogicWhile
- Category: Core/Logic
- Needs: nothing

Runs its sub-actions again and again while the comparison is true.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Input | `input` | The text to test. Variables are filled in again on every pass. |
| Operation | `operation` | One of Equals, Not Equals, Less Than, Greater Than, stored as 0 to 3. |
| Value | `value` | The text to compare with. Variables are filled in again on every pass. |
| Auto type | `autoType` | On: numbers are compared as numbers. |
| Ignore case | `ignoreCase` | On: both sides are lowered before the comparison. |

### Arguments

None set.

### Notes

- The comparison is checked before each pass, so the loop can end with no passes at all.
- Streamerz lets other work run between passes. A loop that never ends still keeps the action busy, so add a Loop Break or make sure the comparison changes.

## Loop Break

- Kind: sub-action
- Code: 129
- Type: LoopBreak
- Category: Core/Logic
- Needs: nothing

Ends the While loop it is in.

### Fields

None.

### Arguments

None set.

### Notes

- Outside a While, it ends the list of sub-actions it is in. Use Break to stop the whole action.

## Loop Continue

- Kind: sub-action
- Code: 130
- Type: LoopContinue
- Category: Core/Logic
- Needs: nothing

Skips the rest of the While loop's sub-actions for this pass, and starts the next pass.

### Fields

None.

### Arguments

None set.

### Notes

- Outside a While, it ends the list of sub-actions it is in, the same as Loop Break.

## Get Random Number

- Kind: sub-action
- Code: 1003
- Type: GetRandomNumber
- Category: Core/Logic
- Needs: nothing

Picks a random whole number between two values, or a random fraction between 0 and 1.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Type | `randomType` | Between or Next Float, stored as 0 or 1. |
| Min | `min` | The lowest whole number. |
| Max | `max` | The highest whole number. |

### Arguments

| Key | What it holds |
| --- | --- |
| `randomNumber` | With Between: a whole number from Min to Max, both included. |
| `randomFloat` | With Next Float: a number from 0 up to 1. |
| `randomPercent` | With Next Float: the fraction as a whole percent, from 0 to 99. |

### Notes

- If Min is larger than Max, the two are swapped.

## Get Input

- Kind: sub-action
- Code: 1023
- Type: GetInput
- Category: Core/Logic
- Needs: nothing

Asks the user for text in a dialog of the Streamerz window, and keeps the answer in a variable.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Title | `title` | The title of the dialog. Variables are filled in first. |
| Prompt | `prompt` | The text above the box. Variables are filled in first. |
| Starting text | `originalText` | The text that is in the box when it opens. Variables are filled in first. |
| Output variable | `outputVariable` | The argument that gets the answer. Default `inputResult`. |
| Auto type | `autoTypeOutput` | On: the answer is stored as a number when it looks like one. |

### Arguments

| Key | What it holds |
| --- | --- |
| The output variable (`inputResult` by default) | The text the user typed. Set only when the user confirms. |
| `success` | True when the user confirmed the dialog. False when they cancelled. |

### Notes

- The action waits until the dialog is closed.
