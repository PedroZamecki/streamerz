# Kick / User Groups (sub-actions)

Adds users to, removes them from and checks the groups kept in Streamerz.

## Add User To Group

- Kind: sub-action
- Code: 35020
- Type: KickAddUserToGroup
- Category: Kick/User Groups
- Needs: nothing

Adds a Kick user to a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. A leading `@` is removed. |
| Group name | `groupName` | The name of the group. The ID of the group works too. |

### Arguments

- `addResult`: true when the group exists.

### Notes

- Groups are the ones Streamerz keeps, shared by all platforms. They are not Kick's own.
- A user is stored by Kick ID when the ID can be looked up, else by the login.

## Remove User from Group

- Kind: sub-action
- Code: 35021
- Type: KickRemoveUserFromGroup
- Category: Kick/User Groups
- Needs: nothing

Removes a Kick user from a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. A leading `@` is removed. |
| Group name | `groupName` | The name of the group. The ID of the group works too. |

### Arguments

- `removeResult`: true when the group exists.

### Notes

- Groups are the ones Streamerz keeps, shared by all platforms. They are not Kick's own.
- The user is found the same way as in Add User To Group: by Kick ID when it can be looked up, else by the login.

## User In Group

- Kind: sub-action
- Code: 35022
- Type: KickUserInGroup
- Category: Kick/User Groups
- Needs: nothing

Checks whether a Kick user is in a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. A leading `@` is removed. |
| Group name | `groupName` | The name of the group. The ID of the group works too. |

### Arguments

- `inGroup`: true when the user is in the group.
- `foundUser` and `foundUserName`: the login, set when the user is in the group.
- `foundUserId`: the ID of the user, set when the user is in the group.
- `foundUserType`: `kick`, set when the user is in the group.

### Notes

- Groups are the ones Streamerz keeps, shared by all platforms. They are not Kick's own.
