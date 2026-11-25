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

## Purge User

- Kind: sub-action
- Code: 99501
- Type: TwitchPurgeUser
- Category: Twitch/Moderation
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Takes away someone's messages from the Twitch chat, with a timeout of one second. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |
| Reason | `reason` | Why, for the moderators. Optional. |

### Arguments

- `purgeResult`: true when it worked.
- `purgedUserName`, `purgedUserId`: who it was.

## Block User

- Kind: sub-action
- Code: 99534
- Type: TwitchBlockUser
- Category: Twitch/Moderation
- Needs: the broadcaster's Twitch login with more permissions (Accounts, Twitch, Allow more)

Blocks someone on Twitch, as the broadcaster. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |

### Arguments

None set.

## Unblock User

- Kind: sub-action
- Code: 99535
- Type: TwitchUnblockUser
- Category: Twitch/Moderation
- Needs: the broadcaster's Twitch login with more permissions (Accounts, Twitch, Allow more)

Unblocks someone on Twitch. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |

### Arguments

None set.

## Chat Mode for a While

- Kind: sub-action
- Code: 99544
- Type: TwitchChatModeFor
- Category: Twitch/Moderation/Chat Modes
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Turns a chat mode on and puts it back as it was after a while: slow mode, followers only, subscribers only, emote only or unique chat. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| mode | `mode` | Which mode. |
| Seconds (slow) or minutes (followers), blank: 30 or any follower | `value` | Slow mode: the seconds between messages (empty: 30). Followers only: how many minutes they must have followed (empty: any follower). |
| Duration | `duration` | How long it stays on. It needs a time, or it would never end. |

### Arguments

None set.

### Notes

- The chat goes back to what it was when this started: a mode that was already on stays on, with its old value.
- If Streamerz closes first, the chat isn't put back: turn it off by hand.

## Delete Message

- Kind: sub-action
- Code: 581
- Type: TwitchDeleteMessage
- Category: Twitch/Moderation
- Needs: a Twitch login as a moderator of the channel

Deletes a single chat message on Twitch by message ID.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message ID | `messageId` | The UUID of the chat message to delete. |

### Arguments

- `deleteMessageResult`: true when Twitch deleted the message, else false.

## Resolve AutoMod Message

- Kind: sub-action
- Code: 582
- Type: TwitchResolveAutoModMessage
- Category: Twitch/Moderation/AutoMod
- Needs: a Twitch login as a moderator of the channel

Approves or denies a message held by Twitch AutoMod.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message ID | `messageId` | The held AutoMod message ID. |
| Action | `action` | Allow (0) or Deny (1). |

### Arguments

- `resolveAutoModResult`: true when Twitch accepted the decision, else false.

## Add Blocked Term

- Kind: sub-action
- Code: 583
- Type: TwitchAddBlockedTerm
- Category: Twitch/Moderation/Terms
- Needs: a Twitch login as a moderator of the channel

Adds a blocked term or phrase to the channel's AutoMod blocked terms list.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Term | `text` | The word or phrase to block. |

### Arguments

- `blockedTermResult`: true when added, else false.
- `blockedTermId`: the created term's ID.
- `blockedTermText`: the blocked term text.

## Remove Blocked Term

- Kind: sub-action
- Code: 584
- Type: TwitchRemoveBlockedTerm
- Category: Twitch/Moderation/Terms
- Needs: a Twitch login as a moderator of the channel

Removes a blocked term from the channel by term ID.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Term ID | `termId` | The ID of the blocked term to remove. |

### Arguments

- `removeBlockedTermResult`: true when removed, else false.

## Get Blocked Terms

- Kind: sub-action
- Code: 585
- Type: TwitchGetBlockedTerms
- Category: Twitch/Moderation/Terms
- Needs: a Twitch login as a moderator of the channel

Reads up to 100 blocked terms configured for the channel.

### Fields

None.

### Arguments

- `blockedTermCount`: number of blocked terms returned.
- `blockedTerm0.id`, `blockedTerm0.text`, and so on: individual blocked terms.
- `blockedTerms`: JSON array of the returned blocked term objects.

## Resolve Unban Request

- Kind: sub-action
- Code: 586
- Type: TwitchResolveUnbanRequest
- Category: Twitch/Moderation/Unban Requests
- Needs: a Twitch login as a moderator of the channel

Approves or denies a Twitch unban request.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Unban request ID | `unbanRequestId` | The ID of the unban request. |
| Decision | `status` | Approve (0) or Deny (1). |
| Resolution text | `resolutionText` | Optional note sent with the decision. |

### Arguments

- `resolveUnbanRequestResult`: true when Twitch accepted the decision, else false.
- `unbanRequestStatus`: resulting status string (`approved` or `denied`).

## Get Unban Requests

- Kind: sub-action
- Code: 587
- Type: TwitchGetUnbanRequests
- Category: Twitch/Moderation/Unban Requests
- Needs: a Twitch login as a moderator of the channel

Lists Twitch unban requests filtered by status.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Status | `status` | Pending (0), Approved (1), Denied (2), Acknowledged (3), or Canceled (4). |

### Arguments

- `unbanRequestCount`: number of unban requests returned.
- `unbanRequest0.id`, `unbanRequest0.userId`, `unbanRequest0.userLogin`, `unbanRequest0.userName`, `unbanRequest0.status`, `unbanRequest0.text`, and so on.
- `unbanRequests`: JSON array of returned unban request objects.

## Unique Chat Mode

- Kind: sub-action
- Code: 588
- Type: TwitchUniqueChatMode
- Category: Twitch/Moderation/Chat Modes
- Needs: a Twitch login as a moderator of the channel

Turns unique chat mode (formerly R9K) on or off in Twitch chat.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Enabled | `status` | On turns unique chat mode on, off turns it off. |

### Arguments

- `uniqueModeEnabled`: boolean reflecting the resulting unique chat mode state.

## Chat Delay

- Kind: sub-action
- Code: 589
- Type: TwitchChatDelay
- Category: Twitch/Moderation/Chat Modes
- Needs: a Twitch login as a moderator of the channel

Turns non-moderator chat delay on or off and sets the delay duration (2, 4, or 6 seconds).

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Enabled | `status` | On turns chat delay on, off turns it off. |
| Delay | `duration` | Delay in seconds (Twitch accepts 2, 4, or 6 seconds). |

### Arguments

- `chatDelayEnabled`: boolean reflecting whether chat delay is enabled.
- `chatDelayDuration`: active delay duration in seconds when enabled.

## Get Chat Settings

- Kind: sub-action
- Code: 590
- Type: TwitchGetChatSettings
- Category: Twitch/Moderation/Chat Modes
- Needs: a Twitch login as a moderator of the channel

Reads the current Twitch chat settings for the channel.

### Fields

None.

### Arguments

- `emoteMode`, `followerMode`, `followerModeDuration`, `slowMode`, `slowModeWaitTime`, `subscriberMode`, `uniqueChatMode`, `nonModeratorChatDelay`, `nonModeratorChatDelayDuration`.

## Get AutoMod Settings

- Kind: sub-action
- Code: 591
- Type: TwitchGetAutoModSettings
- Category: Twitch/Moderation/AutoMod
- Needs: a Twitch login as a moderator of the channel

Reads the current Twitch AutoMod filtering levels for the channel.

### Fields

None.

### Arguments

- `automod.overallLevel`, `automod.aggression`, `automod.bullying`, `automod.disability`, `automod.misogyny`, `automod.raceEthnicityOrReligion`, `automod.sexBasedTerms`, `automod.sexualitySexOrGender`, `automod.swearing`.

## Update AutoMod Settings

- Kind: sub-action
- Code: 592
- Type: TwitchUpdateAutoModSettings
- Category: Twitch/Moderation/AutoMod
- Needs: a Twitch login as a moderator of the channel

Updates one or more Twitch AutoMod category levels (0 to 4). Any field outside 0 to 4 (such as `-1` or empty) keeps its existing channel value.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Aggression | `aggression` | Filtering level 0 to 4 (or -1/empty to keep current). |
| Bullying | `bullying` | Filtering level 0 to 4 (or -1/empty to keep current). |
| Disability | `disability` | Filtering level 0 to 4 (or -1/empty to keep current). |
| Misogyny | `misogyny` | Filtering level 0 to 4 (or -1/empty to keep current). |
| Race/Ethnicity/Religion | `raceEthnicityOrReligion` | Filtering level 0 to 4 (or -1/empty to keep current). |
| Sex-based terms | `sexBasedTerms` | Filtering level 0 to 4 (or -1/empty to keep current). |
| Sexuality/Sex/Gender | `sexualitySexOrGender` | Filtering level 0 to 4 (or -1/empty to keep current). |
| Swearing | `swearing` | Filtering level 0 to 4 (or -1/empty to keep current). |

### Arguments

- `updateAutoModSettingsResult`: true when Twitch updated the settings, else false.
- `automod.overallLevel`, `automod.aggression`, `automod.bullying`, `automod.disability`, `automod.misogyny`, `automod.raceEthnicityOrReligion`, `automod.sexBasedTerms`, `automod.sexualitySexOrGender`, `automod.swearing`.

## Check AutoMod Status

- Kind: sub-action
- Code: 593
- Type: TwitchCheckAutoModStatus
- Category: Twitch/Moderation/AutoMod
- Needs: a Twitch login as the broadcaster

Tests whether a message string would be permitted by Twitch AutoMod in the channel.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message ID | `msgId` | Optional ID for the check (generated automatically if empty). |
| Message | `message` | Text to check against AutoMod. |

### Arguments

- `autoModStatusResult`: true when the check succeeded, else false.
- `isPermitted`: true if AutoMod would permit the message, false if held.

## Get Banned Users

- Kind: sub-action
- Code: 594
- Type: TwitchGetBannedUsers
- Category: Twitch/Moderation
- Needs: a Twitch login as the broadcaster

Reads banned and timed-out users for the channel, optionally filtered by a user login.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | Optional login to filter for; leave empty to list up to 100 banned users. |

### Arguments

- `bannedUserCount`: number of banned users returned.
- `bannedUser0.userId`, `bannedUser0.userLogin`, `bannedUser0.userName`, `bannedUser0.expiresAt`, `bannedUser0.reason`, `bannedUser0.moderatorName`, and so on.
- `bannedUsers`: JSON array of returned ban entries.

## Get Moderators

- Kind: sub-action
- Code: 595
- Type: TwitchGetModerators
- Category: Twitch/Moderation
- Needs: a Twitch login as the broadcaster

Reads up to 100 channel moderators.

### Fields

None.

### Arguments

- `moderatorCount`: number of moderators returned.
- `moderator0.userId`, `moderator0.userLogin`, `moderator0.userName`, and so on.
- `moderators`: JSON array of returned moderator entries.

## Get VIPs

- Kind: sub-action
- Code: 596
- Type: TwitchGetVips
- Category: Twitch/Moderation
- Needs: a Twitch login as the broadcaster

Reads up to 100 channel VIPs.

### Fields

None.

### Arguments

- `vipCount`: number of VIPs returned.
- `vip0.userId`, `vip0.userLogin`, `vip0.userName`, and so on.
- `vips`: JSON array of returned VIP entries.
