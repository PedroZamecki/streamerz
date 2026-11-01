# Kick / Channel & Stream (triggers)

Triggers for the Kick channel: the broadcaster's login, the stream going live or offline, the title and category, viewers, follows, renewals, timeouts and gifted Kicks.

## Broadcaster Authenticated

- Kind: trigger
- Code: 35001
- Type: KickBroadcasterAuthenticated
- Category: Kick/Channel & Stream
- Needs: a Kick login (the broadcaster account) and Kick connected

Fires once the Kick chat connects, when a Kick login is set.

### Fields

None.

### Arguments

- `user`: the broadcaster account's name.
- `userName`: the broadcaster account's name in lowercase.
- `userId`: the broadcaster account's Kick user id.
- `userType`: `kick`.

### When

Each time the Kick chat connects while a Kick login is set. Nothing fires when no login is set.

## Present Viewers

- Kind: trigger
- Code: 35009
- Type: KickPresentViewers
- Category: Kick/Channel & Stream
- Needs: a Kick channel and Kick connected, with the stream live

Fires with the list of people who chatted recently.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster, from the Kick account or the channel.
- `title`, `status`, `categoryId`, `categoryName`, `categoryThumbnail`, `category.id`, `category.name`, `category.thumbnail`, `startedAt`: the stream's title and category.
- `viewerCount`, `viewers`: the viewer count when the check ran.
- `users`: a list of the people who chatted in the last 10 minutes. Each has `userId`, `userName` and `user`.
- `isLive`: `true`.
- `isTest`: `false`.

### When

About a minute after Kick connects, then every 5 minutes while the stream is live. Streamerz checks Kick only when an action uses this trigger.

### Notes

- Only people who chatted while Streamerz was connected are in the list.

## Follow

- Kind: trigger
- Code: 35011
- Type: KickFollow
- Category: Kick/Channel & Stream
- Needs: a Kick channel and Kick connected

Fires when someone follows the channel on Kick.

### Fields

None.

### Arguments

- `user`: the follower's name.
- `userName`: the follower's Kick slug, or the name in lowercase.
- `userId`: the follower's Kick user id.
- `userType`: `kick`.
- `followersCount`: the channel's follower count.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

When Kick reports a new follow with a name. Unfollows do not fire it.

## Stream Online

- Kind: trigger
- Code: 35012
- Type: KickStreamOnline
- Category: Kick/Channel & Stream
- Needs: a Kick channel and Kick connected

Fires when the channel goes live on Kick.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.
- `title`, `status`, `categoryId`, `categoryName`, `categoryThumbnail`, `category.id`, `category.name`, `category.thumbnail`, `startedAt`: the stream's title and category, as Kick reports them when it goes live.
- `channel`: the channel name.

### When

When Kick reports that the stream started.

## Stream Offline

- Kind: trigger
- Code: 35013
- Type: KickStreamOffline
- Category: Kick/Channel & Stream
- Needs: a Kick channel and Kick connected

Fires when the channel goes offline on Kick.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.
- `title`, `status`, `categoryId`, `categoryName`, `categoryThumbnail`, `category.id`, `category.name`, `category.thumbnail`, `startedAt`: the title and category Streamerz last saw.
- `endedAt`: when the stream ended, as Streamerz saw it.
- `channel`: the channel name.

### When

When Kick reports that the stream ended.

## Resubscription

- Kind: trigger
- Code: 35015
- Type: KickResubscription
- Category: Kick/Channel & Stream
- Needs: a Kick channel and Kick connected

Fires when a subscriber renews for more than one month.

### Fields

None.

### Arguments

- `user`, `userName`, `userId`, `userType`: the subscriber.
- `months`, `monthsSubscribed`: the number of months.
- `duration`: `1`.
- `subscribedAt`: when Streamerz saw the event.
- `expiresAt`: empty.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

When Kick reports a subscription event with more than 1 month. A subscription for a single month fires Subscription instead.

## User Timed Out

- Kind: trigger
- Code: 35018
- Type: KickUserTimedOut
- Category: Kick/Channel & Stream
- Needs: a Kick channel and Kick connected

Fires when a user is timed out in the Kick chat.

### Fields

None.

### Arguments

- `user`, `userName`, `userId`, `userType`: the user who was timed out.
- `createdById`, `createdByUsername`, `createdByDisplayName`, `bannedBy`: the moderator who did it.
- `createdAt`: when Streamerz saw the event.
- `reason`: the reason given, if any.
- `expiresAt`: when the timeout ends, as Kick reports it.
- `duration`: the length of the timeout in seconds.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

When Kick reports a timeout (a ban that is not permanent). Permanent bans fire User Banned instead.

### Notes

- Kick reports the timeout in minutes. Streamerz passes `duration` in seconds.

## Channel Update

- Kind: trigger
- Code: 35020
- Type: KickChannelUpdate
- Category: Kick/Channel & Stream
- Needs: a Kick channel and Kick connected

Fires when the stream's title or category changes on Kick.

### Fields

None.

### Arguments

- `title`, `status`, `categoryId`, `categoryName`, `categoryThumbnail`, `category.id`, `category.name`, `category.thumbnail`, `startedAt`: the title and category now.
- `statusUpdated`: `true` when the title changed.
- `oldStatus`: the title before.
- `categoryUpdated`: `true` when the category changed.
- `oldCategoryId`, `oldCategoryName`, `oldCategoryThumbnail`: the category before.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

When Kick reports a change to the livestream or the channel. The title and category are compared with the last ones Streamerz saw.

## Viewer Count Update

- Kind: trigger
- Code: 35023
- Type: KickViewerCountUpdate
- Category: Kick/Channel & Stream
- Needs: a Kick channel and Kick connected, with the stream live

Fires when the viewer count changes.

### Fields

None.

### Arguments

- `viewerCount`, `viewers`: the new viewer count.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.
- `title`, `status`, `categoryId`, `categoryName`, `categoryThumbnail`, `category.id`, `category.name`, `category.thumbnail`, `startedAt`: the stream's title and category.

### When

Once a minute while the stream is live, and only if the count differs from the last check. Streamerz checks Kick only when an action uses this trigger.

## Kicks Gifted

- Kind: trigger
- Code: 35025
- Type: KickKicksGifted
- Category: Kick/Channel & Stream
- Needs: a Kick channel and Kick connected

Fires when someone gifts Kicks.

### Fields

None.

### Arguments

- `user`, `userName`, `userId`, `userType`: the sender.
- `kicks.amount`, `amount`: how many Kicks.
- `kicks.name`: the name of the gift.
- `kicks.type`, `kicks.tier`: the type and tier of the gift.
- `kicks.pinnedTimeSeconds`: how long the gift stays pinned.
- `kicks.createdAt`: when the gift was made.
- `message`: the message sent with the gift.
- `rawInput`, `rawInputEscaped`, `rawInputUrlEncoded`, `input0`, `inputEscaped0`, `inputUrlEncoded0` and so on: the message, and each of its words.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

When Kick reports a Kicks gift.
