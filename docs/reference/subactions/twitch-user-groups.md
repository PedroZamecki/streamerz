# Twitch / User Groups (sub-actions)

User groups: named lists of users kept by Streamerz. A group can hold Twitch, YouTube and Kick users.

## Add User To Group

- Kind: sub-action
- Code: 553
- Type: TwitchAddUserToGroup
- Category: Twitch/User Groups
- Needs: a Twitch login, to look up the user's id (a number skips the lookup)

Adds a user to a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | A Twitch login name (a leading `@` is removed). |
| Group | `groupName` | The name or id of a group in Streamerz. |

### Arguments

- `addResult`: true when the group was found and the user was added, false otherwise.

### Notes

- Groups are kept in Streamerz, not on Twitch.
- When the group doesn't exist, nothing changes.

## Remove User from Group

- Kind: sub-action
- Code: 554
- Type: TwitchRemoveUserFromGroup
- Category: Twitch/User Groups
- Needs: a Twitch login, to look up the user's id (a number skips the lookup)

Removes a user from a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | A Twitch login name (a leading `@` is removed). |
| Group | `groupName` | The name or id of a group in Streamerz. |

### Arguments

- `removeResult`: true when the group was found, false otherwise.

### Notes

- Groups are kept in Streamerz, not on Twitch.
- When the group doesn't exist, nothing changes.

## User In Group

- Kind: sub-action
- Code: 558
- Type: TwitchUserInGroup
- Category: Twitch/User Groups
- Needs: a Twitch login, to look up the user's id (a number skips the lookup)

Checks whether a user is in a group. It doesn't change the group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | A Twitch login name (a leading `@` is removed). |
| Group | `groupName` | The name or id of a group in Streamerz. |

### Arguments

- `inGroup`: true when the user is in the group, false otherwise.
- When the user is in the group, also `foundUser`, `foundUserName` (the login), `foundUserId` and `foundUserType` (`twitch`).

### Notes

- Groups are kept in Streamerz, not on Twitch.
