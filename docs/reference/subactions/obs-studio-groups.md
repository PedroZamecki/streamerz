# OBS Studio / Groups (sub-actions)

Sub-actions that work on OBS groups: a group is a set of sources that OBS treats as one item.

## Set Random Group Source Visible

- Kind: sub-action
- Code: 31
- Type: ObsRandomGroupSourceVisible
- Category: OBS Studio/Groups
- Needs: an OBS connection (the one picked, else the first connected one)

Shows one source of a group picked at random, and hides the others in the group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Connection | `connectionId` | The OBS connection, by id or name. Empty uses the first connected one. |
| Scene | `sceneName` | Stored with the action, but not read. |
| Group | `groupName` | The group name. Variables are filled in first. |

### Arguments

None set.

### Notes

- If the group is empty or OBS does not answer, nothing changes.
- The Scene field is not used. The group name is used as the name to look up in OBS.
