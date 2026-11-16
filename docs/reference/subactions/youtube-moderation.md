# YouTube / Moderation (sub-actions)

Sub-actions that ban or time out a user in a YouTube live chat.

## Ban User

- Kind: sub-action
- Code: 4008
- Type: YouTubeBanUser
- Category: YouTube/Moderation
- Needs: a YouTube login (the broadcaster)

Bans a user from the live chat of the stream.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the live stream Streamerz is reading. |
| User ID | `userId` | The YouTube channel id of the user to ban. |

### Arguments

- `banResult`: true when YouTube accepted the ban.
- `banId`: tracked YouTube ban resource ID returned by YouTube (use with Unban User).
- `bannedUser`, `bannedUserName` and `bannedUserId`: the user id as given.
- `bannedUserType`: `youtube`.

### Notes

- Always uses the broadcaster's login, never the bot's.
- Streamerz doesn't look up YouTube names, so the name arguments hold the id too.

## Timeout User

- Kind: sub-action
- Code: 4009
- Type: YoUTubeTimeoutUser
- Category: YouTube/Moderation
- Needs: a YouTube login (the broadcaster)

Times out a user in the live chat of the stream, for a number of seconds.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the live stream Streamerz is reading. |
| User ID | `userId` | The YouTube channel id of the user to time out. |
| Duration (seconds) | `duration` | How long the timeout lasts, in seconds. |

### Arguments

- `timeoutResult`: true when YouTube accepted the timeout.
- `banId`: tracked YouTube ban resource ID returned by YouTube.
- `bannedUser`, `bannedUserName` and `bannedUserId`: the user id as given.
- `bannedUserType`: `youtube`.

### Notes

- Always uses the broadcaster's login, never the bot's.
- Streamerz doesn't look up YouTube names, so the name arguments hold the id too.

## Unban User

- Kind: sub-action
- Code: 4015
- Type: YouTubeUnbanUser
- Category: YouTube/Moderation
- Needs: a YouTube login (the broadcaster)

Removes an active YouTube live chat ban using its ban ID or a single tracked ban for the user channel ID.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Ban ID | `banId` | YouTube ban resource ID returned by Ban User or Timeout User. |
| User ID | `userId` | Optional YouTube channel ID used to look up a single tracked ban when Ban ID is empty. |

### Arguments

- `unbanResult`: true when YouTube removed the ban.
- `banId`: the ban ID that was removed.

## Delete Message

- Kind: sub-action
- Code: 4016
- Type: YouTubeDeleteMessage
- Category: YouTube/Moderation
- Needs: a YouTube login (the broadcaster)

Deletes a live chat message on YouTube by message ID.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message ID | `messageId` | ID of the YouTube live chat message to delete. |

### Arguments

- `deleteMessageResult`: true when YouTube deleted the message.

## Add Moderator

- Kind: sub-action
- Code: 4017
- Type: YouTubeAddModerator
- Category: YouTube/Moderation
- Needs: a YouTube login (the broadcaster)

Adds a user as a moderator for the YouTube live chat.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the active live stream. |
| User Channel ID | `userId` | The YouTube channel ID of the user to promote. |

### Arguments

- `addModeratorResult`: true when YouTube added the moderator.
- `moderatorId`: YouTube moderator resource ID.

## Remove Moderator

- Kind: sub-action
- Code: 4018
- Type: YouTubeRemoveModerator
- Category: YouTube/Moderation
- Needs: a YouTube login (the broadcaster)

Removes a moderator from the YouTube live chat.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the active live stream. |
| Moderator ID | `moderatorId` | YouTube moderator resource ID (optional if User Channel ID is provided). |
| User Channel ID | `userId` | YouTube channel ID used to look up the moderator resource ID when Moderator ID is empty. |

### Arguments

- `removeModeratorResult`: true when YouTube removed the moderator.
