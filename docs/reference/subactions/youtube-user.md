# YouTube / User (sub-actions)

Sub-actions about the YouTube account that Streamerz is set up with.

## Add Broadcaster Information

- Kind: sub-action
- Code: 4010
- Type: YouTubeAddBroadcasterInformation
- Category: YouTube/User
- Needs: nothing (it reads what is set up in Streamerz)

Adds the broadcaster's channel details to the arguments. It calls nothing on YouTube.

### Fields

None.

### Arguments

- `broadcasterUserId`: the YouTube channel id set up in Streamerz.
- `broadcasterUser` and `broadcasterUserName`: the broadcaster's name. Set only when a broadcaster account is logged in.

### Notes

- The name arguments are left out when no broadcaster account is logged in.

## Get User Info for Target

- Kind: sub-action
- Code: 99662
- Type: YouTubeGetUserInfoForTarget
- Category: YouTube/User
- Needs: the broadcaster's YouTube login (an owner or moderator of the chat)

Gives the target values (`targetUser`...) for a YouTube channel. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |

### Arguments

- `addTargetResult`: true when the channel was found.
- `targetUser`, `targetUserName`, `targetUserId`, `targetUserType`: who they are.
- `targetDescription`, `targetUserProfileImageUrl`, `targetCreatedAt`, `targetSubscriberCount`, `targetVideoCount`: their channel.

## Add Random Users

- Kind: sub-action
- Code: 99663
- Type: YouTubeAddRandomUsers
- Category: YouTube/User
- Needs: nothing: it uses what the YouTube chat saw this session

Picks people at random among those who chatted on YouTube this session. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| How many | `count` | How many, at least 1. |

### Arguments

- `randomUsersCount`: how many were picked.
- `randomUser0`, `randomUserName0`, `randomUserId0`, `randomUserType0`, and so on for each one.
