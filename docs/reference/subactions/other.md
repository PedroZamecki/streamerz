# Other (sub-actions)

Sub-actions that don't belong to a category: placeholders, types Streamerz doesn't run yet, and the groups that If/Else and Switch use for their branches.

## Nothing

- Kind: sub-action
- Code: 0
- Type: Nothing
- Category: Other
- Needs: nothing

Does nothing. Streamerz doesn't run it yet.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## SlobsRaw

- Kind: sub-action
- Code: 416
- Type: SlobsRaw
- Category: Other
- Needs: nothing

Streamlabs Desktop raw request. Streamerz doesn't run it yet.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Get Sub-Action State

- Kind: sub-action
- Code: 1029
- Type: GetSubActionState
- Category: Other
- Needs: nothing

Gets the state of a sub-action. Streamerz doesn't run it yet.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Lumia Stream Set Brightness

- Kind: sub-action
- Code: 6003
- Type: LumiaStreamSetBrightness
- Category: Other
- Needs: nothing

Lumia Stream brightness. Streamerz doesn't run it yet.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Switch Profile

- Kind: sub-action
- Code: 11003
- Type: StreamDeckSwitchProfile
- Category: Other
- Needs: nothing

Stream Deck profile switch. Streamerz doesn't run it yet.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Elgato Camera Hub Prompter Set Contrast

- Kind: sub-action
- Code: 14613
- Type: ElgatoCameraHubPrompterSetContrast
- Category: Other
- Needs: nothing

Elgato Camera Hub prompter contrast. Streamerz doesn't run it yet.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Meld Studio Set Layer Rotation

- Kind: sub-action
- Code: 41015
- Type: MeldStudioSetLayerRotation
- Category: Other
- Needs: nothing

Meld Studio layer rotation. Streamerz doesn't run it yet.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Group

- Kind: sub-action
- Code: 99900
- Type: Group
- Category: Other
- Needs: nothing

A group of sub-actions. It runs its sub-actions in order, or one of them picked by weight when Random is on.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Name | `name` | A name for the group. |
| Random | `random` | On: run one of the sub-actions inside, picked by weight. |
| Color (#RRGGBB) | `color` | A colour for the group. |

### Arguments

None set.

### Notes

- Disabled sub-actions inside the group are skipped.
- With Random on, the chance of each sub-action follows its weight (see Set Sub-Action Weight). When every weight is 0, each one has the same chance.
- If a sub-action inside ends the run early, the rest of the group is skipped too.
- The engine doesn't read `name` or `color` when it runs the group.

## If True Group

- Kind: sub-action
- Code: 99901
- Type: IfTrueGroup
- Category: Other
- Needs: nothing

The branch of an If/Else that runs when its condition is true.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Random | `random` | On: run one of the sub-actions inside, picked by weight. |

### Arguments

None set.

### Notes

- Streamerz runs it only as the true branch of an If/Else sub-action.
- Disabled sub-actions inside are skipped.

## If False Group

- Kind: sub-action
- Code: 99902
- Type: IfFalseGroup
- Category: Other
- Needs: nothing

The branch of an If/Else that runs when its condition is false.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Random | `random` | On: run one of the sub-actions inside, picked by weight. |

### Arguments

None set.

### Notes

- Streamerz runs it only as the false branch of an If/Else sub-action.
- Disabled sub-actions inside are skipped.

## Switch Case

- Kind: sub-action
- Code: 99903
- Type: SwitchCaseGroup
- Category: Other
- Needs: nothing

One case of a Switch. The first case whose values match the Switch's input runs, and the other cases are skipped.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Values (one per line) | `values` | The values to match, one per line. |
| Case sensitive | `caseSensitive` | On: upper and lower case must match. Off by default. |
| Random | `random` | On: run one of the sub-actions inside, picked by weight. |

### Arguments

None set.

### Notes

- Without Case sensitive, the match ignores upper and lower case.
- When the Switch has Auto Type on, numbers are compared as numbers, so the same value written two ways can match.
- A Switch Case only runs inside a Switch sub-action.

## Switch Default

- Kind: sub-action
- Code: 99904
- Type: SwitchDefaultGroup
- Category: Other
- Needs: nothing

The default branch of a Switch. It runs when no Switch Case matched.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Random | `random` | On: run one of the sub-actions inside, picked by weight. |

### Arguments

None set.

### Notes

- Streamerz runs it only inside a Switch sub-action, and only when no case matched.
- Disabled sub-actions inside are skipped.
