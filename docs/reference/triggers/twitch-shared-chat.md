# Twitch / Shared Chat (triggers)

Twitch's Shared Chat: the sessions where several channels share one chat, and the notices and moderation that come from the other channels in it.

## Session Begin

- Kind: trigger
- Code: 474
- Type: TwitchSharedChatSessionBegin
- Category: Twitch/Shared Chat/Sessions
- Needs: a Twitch login as the broadcaster

Starts its actions when a Shared Chat session starts with this channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `broadcastIsAffiliate`, `broadcastIsPartner`: always false.
- `sessionId`: the id of the session.
- `host.userId`, `host.userLogin`, `host.userName`: the channel hosting the shared chat.
- `host.isSubscribed`, `host.isModerator`, `host.isVip`: always false.

### When

When Twitch sends the session begin event (`channel.shared_chat.begin`).

### Notes

- Only the broadcaster's own login gets these session events.

## Session Update

- Kind: trigger
- Code: 475
- Type: TwitchSharedChatSessionUpdate
- Category: Twitch/Shared Chat/Sessions
- Needs: a Twitch login as the broadcaster

Starts its actions when a Shared Chat session of this channel changes.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `broadcastIsAffiliate`, `broadcastIsPartner`: always false.
- `sessionId`: the id of the session.
- `host.userId`, `host.userLogin`, `host.userName`: the channel hosting the shared chat.
- `host.isSubscribed`, `host.isModerator`, `host.isVip`: always false.

### When

When Twitch sends the session update event (`channel.shared_chat.update`).

### Notes

- Only the broadcaster's own login gets these session events.

## Session End

- Kind: trigger
- Code: 476
- Type: TwitchSharedChatSessionEnd
- Category: Twitch/Shared Chat/Sessions
- Needs: a Twitch login as the broadcaster

Starts its actions when a Shared Chat session with this channel ends.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `broadcastIsAffiliate`, `broadcastIsPartner`: always false.
- `sessionId`: the id of the session.
- `host.userId`, `host.userLogin`, `host.userName`: the channel hosting the shared chat.
- `host.isSubscribed`, `host.isModerator`, `host.isVip`: always false.

### When

When Twitch sends the session end event (`channel.shared_chat.end`).

### Notes

- Only the broadcaster's own login gets these session events.

## Announcement

- Kind: trigger
- Code: 481
- Type: TwitchSharedChatAnnouncement
- Category: Twitch/Shared Chat/Messages & Announcements
- Needs: a Twitch login that reads the chat

Starts its actions when another channel in the shared chat makes an announcement.

### Fields

None.

### Arguments

The same as Announcement on this channel, plus:

- `fromSharedChat`: true.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the notice came from.

### When

When a shared chat notice of the announcement kind comes from another channel in the session.

### Notes

- An announcement from this channel itself fires Announcement, not this trigger.

## Raid

- Kind: trigger
- Code: 482
- Type: TwitchSharedChatRaid
- Category: Twitch/Shared Chat/Messages & Announcements
- Needs: a Twitch login that reads the chat

Starts its actions when another channel in the shared chat raids someone.

### Fields

None.

### Arguments

The same as Raid on this channel, plus:

- `fromSharedChat`: true.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the notice came from.

### When

When a shared chat notice of the raid kind comes from another channel in the session.

### Notes

- A raid from this channel itself fires Raid, not this trigger.

## Prime Paid Upgrade

- Kind: trigger
- Code: 483
- Type: TwitchSharedChatPrimePaidUpgrade
- Category: Twitch/Shared Chat/Subscriptions
- Needs: a Twitch login that reads the chat

Starts its actions when a viewer upgrades a Prime sub to a paid sub on another channel in the shared chat.

### Fields

None.

### Arguments

The same as Prime Paid Upgrade on this channel, plus:

- `fromSharedChat`: true.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the notice came from.

### When

When a shared chat notice of the Prime paid upgrade kind comes from another channel in the session.

### Notes

- An upgrade on this channel itself fires Prime Paid Upgrade, not this trigger.

## Gift Paid Upgrade

- Kind: trigger
- Code: 484
- Type: TwitchSharedChatGiftPaidUpgrade
- Category: Twitch/Shared Chat/Subscriptions
- Needs: a Twitch login that reads the chat

Starts its actions when a gifted sub is upgraded to a paid sub on another channel in the shared chat.

### Fields

None.

### Arguments

The same as Gift Paid Upgrade on this channel, plus:

- `fromSharedChat`: true.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the notice came from.

### When

When a shared chat notice of the gift paid upgrade kind comes from another channel in the session.

### Notes

- An upgrade on this channel itself fires Gift Paid Upgrade, not this trigger.

## Pay It Forward

- Kind: trigger
- Code: 485
- Type: TwitchSharedChatPayItForward
- Category: Twitch/Shared Chat/Subscriptions
- Needs: a Twitch login that reads the chat

Starts its actions when a viewer pays a sub forward on another channel in the shared chat.

### Fields

None.

### Arguments

The same as Pay It Forward on this channel, plus:

- `fromSharedChat`: true.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the notice came from.

### When

When a shared chat notice of the pay it forward kind comes from another channel in the session.

### Notes

- A pay it forward on this channel itself fires Pay It Forward, not this trigger.

## Sub

- Kind: trigger
- Code: 486
- Type: TwitchSharedChatSub
- Category: Twitch/Shared Chat/Subscriptions
- Needs: a Twitch login that reads the chat

Starts its actions when someone subscribes on another channel in the shared chat.

### Fields

None.

### Arguments

The same as Sub on this channel (`tier`, `isMultiMonth`, `multiMonthDuration`, `multiMonthTenure`...), plus:

- `fromSharedChat`: true.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the notice came from.

### When

When a shared chat notice of the sub kind comes from another channel in the session.

### Notes

- A sub on this channel itself fires Sub, not this trigger.

## Resub

- Kind: trigger
- Code: 487
- Type: TwitchSharedChatResub
- Category: Twitch/Shared Chat/Messages & Announcements
- Needs: a Twitch login that reads the chat

Starts its actions when someone resubscribes on another channel in the shared chat.

### Fields

None.

### Arguments

The same as Resub on this channel, plus:

- `fromSharedChat`: true.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the notice came from.

### When

When a shared chat notice of the resub kind comes from another channel in the session.

### Notes

- A resub on this channel itself fires Resub, not this trigger.

## Sub Gift

- Kind: trigger
- Code: 488
- Type: TwitchSharedChatSubGift
- Category: Twitch/Shared Chat/Subscriptions
- Needs: a Twitch login that reads the chat

Starts its actions when someone gifts a sub on another channel in the shared chat.

### Fields

None.

### Arguments

The same as Sub Gift on this channel, plus:

- `fromSharedChat`: true.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the notice came from.

### When

When a shared chat notice of the sub gift kind comes from another channel in the session.

### Notes

- A gift on this channel itself fires Sub Gift, not this trigger.

## Community Sub Gift

- Kind: trigger
- Code: 489
- Type: TwitchSharedChatCommunitySubGift
- Category: Twitch/Shared Chat/Subscriptions
- Needs: a Twitch login that reads the chat

Starts its actions when someone gifts subs to the community on another channel in the shared chat.

### Fields

None.

### Arguments

The same as Community Sub Gift on this channel, plus:

- `fromSharedChat`: true.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the notice came from.

### When

When a shared chat notice of the community sub gift kind comes from another channel in the session.

### Notes

- A community gift on this channel itself fires Community Sub Gift, not this trigger.

## User Banned

- Kind: trigger
- Code: 490
- Type: TwitchSharedChatUserBanned
- Category: Twitch/Shared Chat/Moderation
- Needs: a Twitch login that moderates the channel (the broadcaster or a moderator)

Starts its actions when another channel in the shared chat bans a user.

### Fields

None.

### Arguments

- `createdById`, `createdByUsername`, `createdByDisplayName`: the moderator who banned the user.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the ban came from.
- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `user`, `userName`, `userId`...: the banned user.
- `reason`: the reason given.
- `fromSharedChat`: true.

### When

When another channel in the session bans a user in the shared chat. A ban made on this channel fires the usual trigger instead, not this one.

## User Unbanned

- Kind: trigger
- Code: 491
- Type: TwitchSharedChatUserUnbanned
- Category: Twitch/Shared Chat/Messages & Announcements
- Needs: a Twitch login that moderates the channel (the broadcaster or a moderator)

Starts its actions when another channel in the shared chat unbans a user.

### Fields

None.

### Arguments

- `createdById`, `createdByUsername`, `createdByDisplayName`: the moderator who unbanned the user.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the unban came from.
- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `user`, `userName`, `userId`...: the user.
- `reason`: the reason given.
- `fromSharedChat`: true.

### When

When another channel in the session unbans a user in the shared chat. An unban made on this channel fires the usual trigger instead, not this one.

## User Timedout

- Kind: trigger
- Code: 492
- Type: TwitchSharedChatUserTimedout
- Category: Twitch/Shared Chat/Moderation
- Needs: a Twitch login that moderates the channel (the broadcaster or a moderator)

Starts its actions when another channel in the shared chat times a user out.

### Fields

None.

### Arguments

- `createdById`, `createdByUsername`, `createdByDisplayName`: the moderator who timed the user out.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the timeout came from.
- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `user`, `userName`, `userId`...: the user.
- `reason`: the reason given.
- `endsAt`: when the timeout ends, when Twitch says.
- `fromSharedChat`: true.

### When

When another channel in the session times a user out in the shared chat. A timeout made on this channel fires the usual trigger instead, not this one.

## User Untimedout

- Kind: trigger
- Code: 493
- Type: TwitchSharedChatUserUntimedout
- Category: Twitch/Shared Chat/Messages & Announcements
- Needs: a Twitch login that moderates the channel (the broadcaster or a moderator)

Starts its actions when another channel in the shared chat ends a user's timeout early.

### Fields

None.

### Arguments

- `createdById`, `createdByUsername`, `createdByDisplayName`: the moderator who removed the timeout.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the action came from.
- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `user`, `userName`, `userId`...: the user.
- `reason`: the reason given.
- `fromSharedChat`: true.

### When

When another channel in the session removes a user's timeout in the shared chat. A timeout removed on this channel fires the usual trigger instead, not this one.

## Message Deleted

- Kind: trigger
- Code: 494
- Type: TwitchSharedChatMessageDeleted
- Category: Twitch/Shared Chat/Moderation
- Needs: a Twitch login that moderates the channel (the broadcaster or a moderator)

Starts its actions when another channel in the shared chat deletes a message.

### Fields

None.

### Arguments

- `createdById`, `createdByUsername`, `createdByDisplayName`: the moderator who deleted the message.
- `sourceBroadcasterId`, `sourceBroadcasterUsername`, `sourceBroadcasterDisplayName`: the channel the message was deleted in.
- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `user`, `userName`, `userId`...: the author of the message.
- `targetMessageId`: the id of the deleted message.
- `message`: the text of the deleted message.
- `fromSharedChat`: true.

### When

When another channel in the session deletes a message in the shared chat.

### Notes

- Deleting a message on this channel fires Chat Message Deleted, not this trigger.

## Shared Modiversary

- Kind: trigger
- Code: 498
- Type: TwitchSharedModiversary
- Category: Twitch/Shared Chat/Messages & Announcements
- Needs: a Twitch login that reads the chat

Meant to start its actions when a viewer's modiversary is shared in the shared chat.

### Fields

None.

### Arguments

None set.

### When

It doesn't fire yet (see Notes).

### Notes

- Streamerz doesn't fire this yet.
