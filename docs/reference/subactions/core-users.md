# Core / Users (sub-actions)

Changes the members of Streamerz user groups.

## Clear Users From Group

- Kind: sub-action
- Code: 10001
- Type: ClearUsersFromGroup
- Category: Core/Users
- Needs: nothing

Removes every user from a user group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group | `groupId` | The user group. |

### Arguments

None set.

### Notes

- Only the members are removed. The group itself stays.
- If the group does not exist, nothing happens.
- Streamerz fires the group's Cleared event after the members are removed.
