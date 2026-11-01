# Twitch / Moderation (triggers)

Moderation events on the channel: bans, timeouts, moderator and VIP changes, shoutouts, chat modes, AutoMod and
suspicious users.

## User Timed Out

- Kind: trigger
- Code: 136
- Type: TwitchUserTimedOut
- Category: Twitch/Moderation/Bans & Timeouts
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A viewer was timed out in the channel's chat.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the timed out viewer.
- `isSubscribed`, `isModerator`, `isVip`, `badges`, `badgeCount`: the viewer's other flags, false or empty here.
- `createdById`, `createdByUsername`, `createdByDisplayName`: the moderator who did it.
- `reason`: the reason given.
- `isPermanent`: false.
- `endsAt`: when the timeout ends.
- `duration`: the length of the timeout, in seconds.

### When

When the channel's ban event comes with `is_permanent` false.

### Notes

- The viewer's messages are marked as removed in Streamerz's chat view.

## User Banned

- Kind: trigger
- Code: 137
- Type: TwitchUserBanned
- Category: Twitch/Moderation/Bans & Timeouts
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A viewer was banned from the channel's chat.

### Fields

None.

### Arguments

The same as User Timed Out, with `isPermanent` set to true and no `endsAt` or `duration`.

### When

When the channel's ban event comes with `is_permanent` true.

### Notes

- The viewer's messages are marked as removed in Streamerz's chat view.

## User Untimed Out

- Kind: trigger
- Code: 145
- Type: TwitchUserUntimedOut
- Category: Twitch/Moderation/Bans & Timeouts
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A timeout was lifted.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the viewer.
- `createdById`, `createdByUsername`, `createdByDisplayName`: the moderator who lifted it.

### When

When the channel's unban event comes.

### Notes

- Twitch sends one event both for a lifted ban and for a lifted timeout, so User Unbanned and User Untimed Out both fire for either one.

## User Unbanned

- Kind: trigger
- Code: 191
- Type: TwitchUserUnbanned
- Category: Twitch/Moderation/Bans & Timeouts
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A ban was lifted.

### Fields

None.

### Arguments

The same as User Untimed Out.

### When

When the channel's unban event comes.

### Notes

- The same event starts User Untimed Out. See that section.

## Shoutout Created

- Kind: trigger
- Code: 144
- Type: TwitchShoutoutCreated
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: a Twitch login that moderates the channel (the owner counts too)

The channel gave a shoutout to another channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `targetUser`, `targetUserDisplayName`: the shouted out channel's display name.
- `targetUserName`, `targetUserLogin`: its login.
- `targetUserId`: its id.
- `cooldownEndsAt`, `targetCooldownEndsAt`: when the shoutout cooldowns end.
- `viewerCount`: the viewers the event reports.
- `moderator.userName`, `moderator.login`, `moderator.id`: the moderator who gave it.

### When

When the channel's shoutout create event comes.

## Shoutout Received

- Kind: trigger
- Code: 156
- Type: TwitchShoutoutReceived
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: a Twitch login that moderates the channel (the owner counts too)

Another channel gave this channel a shoutout.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `fromUser`: the display name of the channel that gave the shoutout.
- `fromUserName`: its login.
- `fromUserId`: its id.
- `viewerCount`: the viewers the event reports.

### When

When the channel's shoutout receive event comes.

## Shield Mode Begin

- Kind: trigger
- Code: 151
- Type: TwitchShieldModeBegin
- Category: Twitch/Moderation/Automod & Safety
- Needs: a Twitch login that moderates the channel (the owner counts too)

Shield Mode was turned on.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.

### When

When the channel's shield mode begin event comes.

## Shield Mode End

- Kind: trigger
- Code: 152
- Type: TwitchShieldModeEnd
- Category: Twitch/Moderation/Automod & Safety
- Needs: a Twitch login that moderates the channel (the owner counts too)

Shield Mode was turned off.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.

### When

When the channel's shield mode end event comes.

## Moderator Added

- Kind: trigger
- Code: 187
- Type: TwitchModeratorAdded
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A viewer was made a moderator of the channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the new moderator.
- `isModerator`: true.
- `isVip`, `isSubscribed`, `badges`, `badgeCount`: the viewer's other flags, false or empty here.

### When

When the channel's moderator add event comes.

## Moderator Removed

- Kind: trigger
- Code: 188
- Type: TwitchModeratorRemoved
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A moderator was removed from the channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the former moderator.
- `isModerator`: false.
- `isVip`, `isSubscribed`, `badges`, `badgeCount`: the viewer's other flags, false or empty here.

### When

When the channel's moderator remove event comes.

## Vip Added

- Kind: trigger
- Code: 189
- Type: TwitchVipAdded
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A viewer became a VIP of the channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the new VIP.
- `isVip`: true.
- `isModerator`, `isSubscribed`, `badges`, `badgeCount`: the viewer's other flags, false or empty here.

### When

When the channel's VIP add event comes.

## Vip Removed

- Kind: trigger
- Code: 190
- Type: TwitchVipRemoved
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: the channel owner's Twitch login (a moderator's login gets nothing here)

A VIP was removed from the channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the former VIP.
- `isVip`: false.
- `isModerator`, `isSubscribed`, `badges`, `badgeCount`: the viewer's other flags, false or empty here.

### When

When the channel's VIP remove event comes.

## Unban Request Created

- Kind: trigger
- Code: 195
- Type: TwitchUnbanRequestCreated
- Category: Twitch/Moderation/Bans & Timeouts
- Needs: a Twitch login that moderates the channel (the owner counts too)

A banned viewer asked to be unbanned.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the viewer who asked.
- `requestId`: the id of the request.
- `text`: the viewer's message.
- `createdAt`: when the request was made.

### When

When the channel's unban request create event comes.

## Unban Request Approved

- Kind: trigger
- Code: 192
- Type: TwitchUnbanRequestApproved
- Category: Twitch/Moderation/Bans & Timeouts
- Needs: a Twitch login that moderates the channel (the owner counts too)

A moderator approved an unban request.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the viewer who asked.
- `moderator.userName`, `moderator.login`, `moderator.id`: the moderator who resolved it.
- `requestId`: the id of the request.
- `message`: the reply the moderator wrote.
- `createdAt`: when Streamerz got the event.

### When

When the unban request resolve event has the status `approved`.

## Unban Request Denied

- Kind: trigger
- Code: 193
- Type: TwitchUnbanRequestDenied
- Category: Twitch/Moderation/Bans & Timeouts
- Needs: a Twitch login that moderates the channel (the owner counts too)

A moderator denied an unban request.

### Fields

None.

### Arguments

The same as Unban Request Approved.

### When

When the unban request resolve event has any status other than `approved`.

### Notes

- Any other result than an approval counts as a denial.

## Chat Emote Mode On

- Kind: trigger
- Code: 451
- Type: TwitchChatEmoteModeOn
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: a Twitch login (any role in the channel)

Emote Mode was turned on in the chat.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `emoteMode`, `followerMode`, `followerModeDuration` (minutes), `slowMode`, `slowModeWaitTime` (seconds), `subscriberOnly`, `uniqueMode`: all the chat settings as they are now.

### When

When a chat settings update shows Emote Mode on, and it was off before.

### Notes

- Each settings update is compared with the one before it. The first one after connecting is compared with the settings Streamerz read when it connected.

## Chat Emote Mode Off

- Kind: trigger
- Code: 452
- Type: TwitchChatEmoteModeOff
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: a Twitch login (any role in the channel)

Emote Mode was turned off in the chat.

### Fields

None.

### Arguments

The same as Chat Emote Mode On.

### When

When a chat settings update shows Emote Mode off, and it was on before.

### Notes

- Each settings update is compared with the one before it, as in Chat Emote Mode On.

## Chat Follower Mode On

- Kind: trigger
- Code: 453
- Type: TwitchChatFollowerModeOn
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: a Twitch login (any role in the channel)

Follower Mode was turned on in the chat.

### Fields

None.

### Arguments

The same as Chat Emote Mode On.

### When

When a chat settings update shows Follower Mode on, and it was off before.

### Notes

- Chat Follower Mode Changed fires too, at the same time.

## Chat Follower Mode Off

- Kind: trigger
- Code: 454
- Type: TwitchChatFollowerModeOff
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: a Twitch login (any role in the channel)

Follower Mode was turned off in the chat.

### Fields

None.

### Arguments

The same as Chat Emote Mode On.

### When

When a chat settings update shows Follower Mode off, and it was on before.

### Notes

- Chat Follower Mode Changed fires too, at the same time.

## Chat Follower Mode Changed

- Kind: trigger
- Code: 455
- Type: TwitchChatFollowerModeChanged
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: a Twitch login (any role in the channel)

Follower Mode was switched on or off, or its time limit changed.

### Fields

None.

### Arguments

The same as Chat Emote Mode On.

### When

When Follower Mode is switched on or off, or when its duration changes while it is on.

### Notes

- It also fires with Chat Follower Mode On or Off, when Follower Mode is switched.

## Chat Unique Mode On

- Kind: trigger
- Code: 461
- Type: TwitchChatUniqueModeOn
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: a Twitch login (any role in the channel)

Unique Chat Mode was turned on.

### Fields

None.

### Arguments

The same as Chat Emote Mode On.

### When

When a chat settings update shows Unique Chat Mode on, and it was off before.

### Notes

- Each settings update is compared with the one before it, as in Chat Emote Mode On.

## Chat Unique Mode Off

- Kind: trigger
- Code: 462
- Type: TwitchChatUniqueModeOff
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: a Twitch login (any role in the channel)

Unique Chat Mode was turned off.

### Fields

None.

### Arguments

The same as Chat Emote Mode On.

### When

When a chat settings update shows Unique Chat Mode off, and it was on before.

### Notes

- Each settings update is compared with the one before it, as in Chat Emote Mode On.

## Auto Mod Message Held

- Kind: trigger
- Code: 463
- Type: TwitchAutoModMessageHeld
- Category: Twitch/Moderation/Automod & Safety
- Needs: a Twitch login that moderates the channel (the owner counts too)

AutoMod held a chat message for a moderator to check.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the author of the message.
- `rawInput`, `rawInputEscaped`, `rawInputUrlEncoded`: the message text.
- `input0`, `input1`... and their escaped and URL encoded forms: the words of the message.
- `messageId`: the id of the message.
- `category`, `level`: the AutoMod category and level.
- `reason`: why it was held.
- `heldAt`: when it was held.

### When

When the AutoMod message hold event comes.

## Auto Mod Message Update

- Kind: trigger
- Code: 464
- Type: TwitchAutoModMessageUpdate
- Category: Twitch/Moderation/Automod & Safety
- Needs: a Twitch login that moderates the channel (the owner counts too)

A message held by AutoMod was allowed or denied.

### Fields

None.

### Arguments

The same as Auto Mod Message Held, with:

- `status`: the new status of the message.
- `moderator.userName`, `moderator.login`, `moderator.id`: the moderator who acted on it.

### When

When the AutoMod message update event comes.

## Warned User

- Kind: trigger
- Code: 467
- Type: TwitchWarnedUser
- Category: Twitch/Moderation/Automod & Safety
- Needs: a Twitch login that moderates the channel (the owner counts too)

A moderator warned a user in the channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the warned user.
- `moderator.userName`, `moderator.login`, `moderator.id`: the moderator who warned them.
- `reason`: the reason given.

### When

When the channel's warning send event comes.

## Suspicious User Update

- Kind: trigger
- Code: 468
- Type: TwitchSuspiciousUserUpdate
- Category: Twitch/Moderation/Automod & Safety
- Needs: a Twitch login that moderates the channel (the owner counts too)

A user was marked as suspicious, or the mark was changed.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the user.
- `moderator.userName`, `moderator.login`, `moderator.id`: the moderator in the event.
- `lowTrustStatus`: the user's low trust status.

### When

When the channel's suspicious user update event comes.

## Suspicious User Message

- Kind: trigger
- Code: 495
- Type: TwitchSuspiciousUserMessage
- Category: Twitch/Moderation/Automod & Safety
- Needs: a Twitch login that moderates the channel (the owner counts too)

A suspicious user sent a chat message.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType` (`twitch`): the user.
- `message`, `msgId`, `rawInput`, `input0`...: the message, as the chat message arguments give it.
- `lowTrustStatus`: the user's low trust status.
- `banEvasionEvaluation`: Twitch's ban evasion evaluation.
- `sharedBanChannelIds`: the channels that share the ban.

### When

When the channel's suspicious user message event comes.

## Modiversary

- Kind: trigger
- Code: 497
- Type: TwitchModiversary
- Category: Twitch/Moderation/Roles & Shoutouts
- Needs: nothing, Streamerz never subscribes to it

A moderator's anniversary in the channel.

### Fields

None.

### Arguments

None set.

### When

Streamerz doesn't fire this yet.
