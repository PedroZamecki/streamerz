# TikTok / User Groups (sub-actions)

Puts TikTok viewers in user groups, by the name the chat saw them with.

## Add User to Group

- Kind: sub-action
- Code: 99450
- Type: TikTokAddUserToGroup
- Category: TikTok/User Groups
- Needs: nothing: it uses what the TikTok chat saw this session

Adds a TikTok user to a user group, by the name the chat saw them with. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |
| User group | `groupName` | The user group, picked by name. |

### Arguments

- `addResult`: true when the group exists.

## Remove User from Group

- Kind: sub-action
- Code: 99451
- Type: TikTokRemoveUserFromGroup
- Category: TikTok/User Groups
- Needs: nothing: it uses what the TikTok chat saw this session

Takes a TikTok user out of a user group. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |
| User group | `groupName` | The user group, picked by name. |

### Arguments

- `removeResult`: true when the group exists.

## User in Group

- Kind: sub-action
- Code: 99452
- Type: TikTokUserInGroup
- Category: TikTok/User Groups
- Needs: nothing: it uses what the TikTok chat saw this session

Says whether a TikTok user is in a user group. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |
| User group | `groupName` | The user group, picked by name. |

### Arguments

- `inGroup`: true when they are.
- `foundUser`, `foundUserName`, `foundUserId`, `foundUserType`: who was found.
