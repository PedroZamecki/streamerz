# Core / User Groups (triggers)

Triggers for user groups: a group is added or removed in the app, or an action clears it or changes who is in it.

## Group Added

- Kind: trigger
- Code: 32001
- Type: GroupAdded
- Category: Core/User Groups
- Needs: nothing

Fires when a user group is created in the app's user groups list.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group (none: any) | `groupId` | The id of the group to watch. Empty watches every group. |

### Arguments

- `group.id`: the group's id.
- `group.name`: the group's name.
- `group.bots`: whether the group is for bots.

### When

When you add a group in the app's user groups list.

## Group Removed

- Kind: trigger
- Code: 32002
- Type: GroupRemoved
- Category: Core/User Groups
- Needs: nothing

Fires when a user group is removed in the app's user groups list.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group (none: any) | `groupId` | The id of the group to watch. Empty watches every group. |

### Arguments

- `group.id`: the group's id.
- `group.name`: the group's name.
- `group.bots`: whether the group is for bots.

### When

When you remove a group in the app's user groups list.

## Group Cleared

- Kind: trigger
- Code: 32003
- Type: GroupCleared
- Category: Core/User Groups
- Needs: nothing

Fires when a Clear Users From Group sub-action runs on a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group (none: any) | `groupId` | The id of the group to watch. Empty watches every group. |

### Arguments

- `group.id`: the group's id.
- `group.name`: the group's name.
- `group.bots`: whether the group is for bots.

### When

Each time the Clear Users From Group sub-action runs on an existing group, even if the group was already empty.

## Group Users Added

- Kind: trigger
- Code: 32004
- Type: GroupUsersAdded
- Category: Core/User Groups
- Needs: nothing

Fires when the Add User to Group sub-action puts a user into a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group (none: any) | `groupId` | The id of the group to watch. Empty watches every group. |

### Arguments

- `group.id`, `group.name`, `group.bots`: the group, as for Group Added.
- `group.user.count`: how many users were added.
- `group.user0.userId`, `group.user0.user`, `group.user0.userName`, `group.user0.userType`, `group.user0.isSubscribed`: the first user. The next users use `group.user1.`, `group.user2.` and so on.

### When

When the Add User to Group sub-action (on Twitch, YouTube or Kick) adds a user who was not in the group yet. Users already in the group do not count.

### Notes

- `userType` and `isSubscribed` are empty for these users, because the sub-action does not pass them.

## Group Users Removed

- Kind: trigger
- Code: 32005
- Type: GroupUsersRemoved
- Category: Core/User Groups
- Needs: nothing

Fires when the Remove User from Group sub-action takes a user out of a group.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Group (none: any) | `groupId` | The id of the group to watch. Empty watches every group. |

### Arguments

- `group.id`, `group.name`, `group.bots`: the group, as for Group Added.
- `group.user.count`: how many users were removed.
- `group.user0.userId`, `group.user0.user`, `group.user0.userName`, `group.user0.userType`, `group.user0.isSubscribed`: the first user. The next users use `group.user1.`, `group.user2.` and so on.

### When

When the Remove User from Group sub-action (on Twitch, YouTube or Kick) takes out a user who was in the group. Users not in the group do not count.

### Notes

- `userType` and `isSubscribed` are empty for these users, as for Group Users Added.
