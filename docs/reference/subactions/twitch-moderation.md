# Twitch / Moderation (sub-actions)

Moderation tools on Twitch: chat modes, timeouts, bans, VIPs, moderators, shield mode and shoutouts.

## Subscriber Only

- Kind: sub-action
- Code: 12
- Type: SubscriberOnly
- Category: Twitch/Moderation/Chat Modes
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Turns subscriber only mode on or off in the Twitch chat.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Subscriber only | `status` | On turns the mode on, off turns it off. |

### Arguments

None set.

## Emote Only

- Kind: sub-action
- Code: 13
- Type: EmoteOnly
- Category: Twitch/Moderation/Chat Modes
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Turns emote only mode on or off in the Twitch chat.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Emote only | `status` | On turns the mode on, off turns it off. |

### Arguments

None set.

## Slow Mode

- Kind: sub-action
- Code: 14
- Type: SlowMode
- Category: Twitch/Moderation/Chat Modes
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Turns slow mode on or off, and sets how many seconds a viewer waits between messages.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Slow mode | `status` | On turns the mode on, off turns it off. |
| Duration (seconds) | `duration` | The wait between messages, in seconds. |

### Arguments

None set.

## Timeout User

- Kind: sub-action
- Code: 20
- Type: TwitchTimeoutUser
- Category: Twitch/Moderation
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Times a user out in the Twitch chat for some seconds. Streamerz looks the login up first, then sends the timeout.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |
| Duration (seconds) | `duration` | How long the timeout lasts. Less than 1 is sent as 1. |
| Reason | `reason` | The reason Twitch shows to the moderators. |

### Arguments

- `timeoutResult`: true when Twitch accepted the timeout, else false.
- `timedOutUser`, `timedOutUserName`: the login as typed.
- `timedOutUserType`: `twitch`.
- `timedOutUserId`: the Twitch user ID.

### Notes

- Nothing is sent when the login can't be found, and `timeoutResult` stays false.

## Get Shield Mode Status

- Kind: sub-action
- Code: 26
- Type: TwitchGetShieldModeStatus
- Category: Twitch/Moderation/Shield
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Reads whether shield mode is on for the channel.

### Fields

None.

### Arguments

- `shieldMode`: true when shield mode is on, else false. Not set when Twitch doesn't answer.

## Update Shield Mode Status

- Kind: sub-action
- Code: 27
- Type: TwitchUpdateShieldModeStatus
- Category: Twitch/Moderation/Shield
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Turns shield mode on or off.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Shield mode | `status` | On turns shield mode on, off turns it off. |

### Arguments

None set.

## Follow Mode

- Kind: sub-action
- Code: 28
- Type: TwitchFollowMode
- Category: Twitch/Moderation/Chat Modes
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Turns follower mode on or off. With it on, a viewer has to follow the channel for a while before they can chat.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Follower mode | `status` | On turns the mode on, off turns it off. |
| Duration (minutes) | `duration` | How many minutes a viewer must follow before chatting. |

### Arguments

None set.

## Send Shoutout

- Kind: sub-action
- Code: 29
- Type: TwitchSendShoutout
- Category: Twitch/Moderation
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Gives a shoutout to another channel in the Twitch chat.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the channel to shout out. |

### Arguments

- `shoutoutSuccess`: true when Twitch accepted the shoutout, else false.

## Add VIP

- Kind: sub-action
- Code: 502
- Type: TwitchAddVip
- Category: Twitch/Moderation
- Needs: a Twitch login as the channel's owner (the broadcaster)

Makes a user a VIP of the channel.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

None set.

### Notes

- Only the channel's owner can do this. A moderator's login is refused by Twitch's rules.

## Remove VIP

- Kind: sub-action
- Code: 503
- Type: TwitchRemoveVip
- Category: Twitch/Moderation
- Needs: a Twitch login as the channel's owner (the broadcaster)

Removes the VIP badge from a user.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

None set.

### Notes

- Only the channel's owner can do this.

## Add Moderator

- Kind: sub-action
- Code: 504
- Type: TwitchAddModerator
- Category: Twitch/Moderation
- Needs: a Twitch login as the channel's owner (the broadcaster)

Makes a user a moderator of the channel.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

None set.

### Notes

- Only the channel's owner can do this.

## Remove Moderator

- Kind: sub-action
- Code: 505
- Type: TwitchRemoveModerator
- Category: Twitch/Moderation
- Needs: a Twitch login as the channel's owner (the broadcaster)

Removes the moderator role from a user.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

None set.

### Notes

- Only the channel's owner can do this.

## Ban User

- Kind: sub-action
- Code: 511
- Type: TwitchBanUser
- Category: Twitch/Moderation
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Bans a user from the channel. Streamerz looks the login up first, then sends the ban.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |
| Reason | `reason` | The reason for the ban. |

### Arguments

- `banResult`: true when Twitch accepted the ban, else false.
- `bannedUser`, `bannedUserName`: the login as typed.
- `bannedUserType`: `twitch`.
- `bannedUserId`: the Twitch user ID.

## Unban User

- Kind: sub-action
- Code: 512
- Type: TwitchUnbanUser
- Category: Twitch/Moderation
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Lifts a ban from a user.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

- `unbanResult`: true when Twitch accepted the change, else false.
- `unbannedUser`, `unbannedUserName`: the login.
- `unbannedUserType`: `twitch`.
- `unbannedUserId`: the Twitch user ID.

### Notes

- A leading `@` is removed from the login.

## UnTimeout User

- Kind: sub-action
- Code: 513
- Type: TwitchUnTimeoutUser
- Category: Twitch/Moderation
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Ends a timeout early, for a user who is timed out in the chat.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

- `unTimedOutResult`: true when Twitch accepted the change, else false.
- `unTimedOutUser`, `unTimedOutUserName`: the login.
- `unTimedOutUserType`: `twitch`.
- `unTimedOutUserId`: the Twitch user ID.

### Notes

- A leading `@` is removed from the login.

## Warn User

- Kind: sub-action
- Code: 565
- Type: TwitchWarnUser
- Category: Twitch/Moderation
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Sends a warning to a user, with a reason.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |
| Reason | `reason` | The reason for the warning. |

### Arguments

- `warnedResult`: true when Twitch accepted the warning, else false.
- `warnedUser`, `warnedUserName`: the login as typed.
- `warnedUserType`: `twitch`.
- `warnedUserId`: the Twitch user ID.

## Add Suspicious Status

- Kind: sub-action
- Code: 579
- Type: TwitchAddSuspiciousStatus
- Category: Twitch/Moderation
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Marks a user as suspicious, so Twitch treats their messages with care.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |
| Status | `status` | Monitored (0): their messages show, marked for the moderators. Restricted (1): their messages wait for a moderator's approval. |

### Arguments

None set.

## Remove Suspicious Status

- Kind: sub-action
- Code: 580
- Type: TwitchRemoveSuspiciousStatus
- Category: Twitch/Moderation
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Removes the suspicious mark from a user.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

None set.
