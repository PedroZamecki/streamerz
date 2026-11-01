# Kick / Moderation (sub-actions)

Bans, unbans, times out and un-times out Kick users.

## Ban User

- Kind: sub-action
- Code: 35040
- Type: KickBanUser
- Category: Kick/Moderation
- Needs: a Kick broadcaster login

Bans a user from the Kick channel. Variables are filled in first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |
| Reason | `reason` | The reason sent to Kick with the ban. |

### Arguments

- `banResult`: true when Kick accepted the ban.
- `bannedUser` and `bannedUserName`: the login as it was typed.
- `bannedUserType`: `kick`.

### Notes

- The ban is made with the broadcaster account. The bot account is never used here.
- The reason is cut to 100 characters.
- Streamerz doesn't check the role of the account here. Kick decides what the broadcaster account may do.

## Unban User

- Kind: sub-action
- Code: 35041
- Type: KickUnbanUser
- Category: Kick/Moderation
- Needs: a Kick broadcaster login

Lifts the ban of a user on the Kick channel.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

- `unbanResult`: true when Kick accepted the unban.
- `unbannedUser` and `unbannedUserName`: the login as it was typed.
- `unbannedUserType`: `kick`.

### Notes

- Kick uses the same call to lift a ban and a timeout, so this also ends a timeout.
- The change is made with the broadcaster account. The bot account is never used here.

## Timeout User

- Kind: sub-action
- Code: 35042
- Type: KickTimeoutUser
- Category: Kick/Moderation
- Needs: a Kick broadcaster login

Times out a user on the Kick channel for a number of seconds.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |
| Duration (seconds) | `duration` | How long the timeout lasts, in seconds. |
| Reason | `reason` | The reason sent to Kick with the timeout. |

### Arguments

- `timeoutResult`: true when Kick accepted the timeout.
- `timedOutUser` and `timedOutUserName`: the login as it was typed.
- `timedOutUserType`: `kick`.

### Notes

- Kick counts in whole minutes. The seconds are rounded up, and the time is kept between 1 minute and 7 days.
- The reason is cut to 100 characters.
- The change is made with the broadcaster account. The bot account is never used here.

## UnTimeout User

- Kind: sub-action
- Code: 35043
- Type: KickUnTimeoutUser
- Category: Kick/Moderation
- Needs: a Kick broadcaster login

Ends the timeout of a user on the Kick channel.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

- `unTimedOutResult`: true when Kick accepted the change.
- `unTimedOutUser` and `unTimedOutUserName`: the login as it was typed.
- `unTimedOutUserType`: `kick`.

### Notes

- Kick has no separate call for this, so the unban call is used.
- The change is made with the broadcaster account. The bot account is never used here.
