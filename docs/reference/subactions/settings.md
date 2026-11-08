# Settings (sub-actions)

Sub-actions that change or reset values kept in Streamerz: the voice control input, the first words list and the credits.

## Pick Color

- Kind: sub-action
- Code: 1008
- Type: PickColor
- Category: Settings
- Needs: nothing

Not done in Streamerz yet: the type is known and kept, but nothing runs it.

### Fields

None.

### Arguments

None set.

### Notes

- The saved keys are `alpha`, `red`, `green`, `blue`, `random` and `variableName`.

## Reset First Words

- Kind: sub-action
- Code: 1026
- Type: ResetFirstWords
- Category: Settings
- Needs: nothing

Forgets who has already spoken, on every platform. The next first message from each user counts again.

### Fields

None.

### Arguments

None set.

## Reset Credits

- Kind: sub-action
- Code: 1027
- Type: ResetCredits
- Category: Settings
- Needs: nothing

Clears the credits: the event lists and the user lists kept for them.

### Fields

None.

### Arguments

None set.
