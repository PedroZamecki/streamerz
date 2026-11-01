# YouTube / User Groups (sub-actions)

Sub-actions that add YouTube users to a group, remove them, or check whether they are in one. Groups are kept in Streamerz, not on YouTube.

## Add User To Group

- Kind: sub-action
- Code: 4005
- Type: YouTubeAddUserToGroup
- Category: YouTube/User Groups
- Needs: nothing

Adds a user to a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The user's YouTube channel id (starts with `UC`). A leading `@` is removed. |
| Group name | `groupName` | The name of the group, or its id. The name is matched in any case. |

### Arguments

- `addResult`: true when the group was found.

### Notes

- Streamerz doesn't look up YouTube names. The value is stored as typed, so use the channel id for the group to match the user later.

## Remove User from Group

- Kind: sub-action
- Code: 4006
- Type: YouTubeRemoveUserFromGroup
- Category: YouTube/User Groups
- Needs: nothing

Removes a user from a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The user's YouTube channel id (starts with `UC`). A leading `@` is removed. |
| Group name | `groupName` | The name of the group, or its id. The name is matched in any case. |

### Arguments

- `removeResult`: true when the group was found.

### Notes

- Streamerz doesn't look up YouTube names. The value is stored as typed, so use the channel id for the group to match the user later.

## User In Group

- Kind: sub-action
- Code: 4007
- Type: YouTubeUserInGroup
- Category: YouTube/User Groups
- Needs: nothing

Checks whether a user is in a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The user's YouTube channel id (starts with `UC`). A leading `@` is removed. |
| Group name | `groupName` | The name of the group, or its id. The name is matched in any case. |

### Arguments

- `inGroup`: true or false.
- When the user is in the group: `foundUser`, `foundUserName` and `foundUserId` (the value as typed), and `foundUserType` set to `youtube`.

### Notes

- Streamerz doesn't look up YouTube names. The value is compared as typed, so use the channel id.
