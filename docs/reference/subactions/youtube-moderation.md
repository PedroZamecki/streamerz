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
- `bannedUser`, `bannedUserName` and `bannedUserId`: the user id as given.
- `bannedUserType`: `youtube`.

### Notes

- Always uses the broadcaster's login, never the bot's.
- Streamerz doesn't look up YouTube names, so the name arguments hold the id too.
