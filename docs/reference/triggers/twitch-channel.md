# Twitch / Channel (triggers)

Triggers for the Twitch channel: follows, stream state, viewers, emotes, chatters and connections.

## Follow

- Kind: trigger
- Code: 101
- Type: TwitchFollow
- Category: Twitch/Channel/Follow & Stream
- Needs: a Twitch login (the broadcaster)

Fires when someone follows the channel on Twitch.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `user`, `userName`, `userId`: the user who followed.
- The other user fields (`isSubscribed`, `isModerator`, `isVip`, `badges`...) are set to their defaults.

### When

When Twitch reports a new follow for the channel.

## Stream Update

- Kind: trigger
- Code: 118
- Type: TwitchStreamUpdate
- Category: Twitch/Channel/Follow & Stream
- Needs: a Twitch login (the broadcaster)

Fires when the channel's title or category is updated.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `status`, `game`, `gameName`, `gameId`, `gameBoxArt`: the new title and category.
- `oldStatus`, `oldGameId`, `oldGameName`, `oldGameBoxArt`: the title and category before the update.
- `statusUpdate`: true when the title changed.
- `gameUpdate`: true when the category changed.

### When

Each time Twitch reports a channel update.

## Stream Update Game On Connect

- Kind: trigger
- Code: 123
- Type: TwitchStreamUpdateGameOnConnect
- Category: Twitch/Channel/Follow & Stream
- Needs: nothing

Listed for the stream update sent when a connection starts.

### Fields

None.

### Arguments

None set.

### When

Never, for now.

### Notes

- Streamerz doesn't fire this yet.

## Present Viewers

- Kind: trigger
- Code: 124
- Type: TwitchPresentViewers
- Category: Twitch/Channel/Follow & Stream
- Needs: a Twitch login (the broadcaster). Twitch lists the chatters only to the broadcaster or a moderator.

Fires with the chatters in the channel while the stream is live.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `users`: the chatters, each with `id`, `login` and `display`.
- `isLive`: true.
- `isTest`: false.

### When

Every 5 minutes while the stream is live and an action uses this trigger. Up to 1000 chatters are listed. If Twitch doesn't return the list, nothing fires.

## Stream Online

- Kind: trigger
- Code: 154
- Type: TwitchStreamOnline
- Category: Twitch/Channel/Follow & Stream
- Needs: a Twitch login (the broadcaster)

Fires when the channel goes live.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `startedAt`: when the stream started, as Twitch gives it.
- `game`, `gameId`, `tagCount`, `tags`: empty, zero, or an empty list. Streamerz doesn't fill them here.

### When

When Twitch reports that the stream started.

### Notes

- Use Stream Update for the category. The game fields here are empty.
- If the Twitch first words are set to reset when the stream starts, Streamerz resets them here.

## Stream Offline

- Kind: trigger
- Code: 155
- Type: TwitchStreamOffline
- Category: Twitch/Channel/Follow & Stream
- Needs: a Twitch login (the broadcaster)

Fires when the channel goes offline.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `endedAt`: the time Streamerz got the event, in ISO 8601 UTC.

### When

When Twitch reports that the stream ended.

### Notes

- Streamerz removes the Twitch chat pins when the stream ends.

## Viewer Count Update

- Kind: trigger
- Code: 164
- Type: TwitchViewerCountUpdate
- Category: Twitch/Channel/Follow & Stream
- Needs: a Twitch login (the broadcaster)

Fires when the live viewer count changes.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `viewerCount`: the new viewer count.

### When

Once a minute while the stream is live and an action uses this trigger. It fires when the count differs from the last check.

## Broadcaster Authenticated

- Kind: trigger
- Code: 173
- Type: TwitchBroadcasterAuthenticated
- Category: Twitch/Channel/Follow & Stream
- Needs: a Twitch login (the broadcaster)

Fires when Streamerz connects to Twitch for the broadcaster account.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.

### When

When the broadcaster's Twitch connection comes up. It doesn't fire on automatic reconnects.

## Broadcaster Chat Connected

- Kind: trigger
- Code: 174
- Type: TwitchBroadcasterChatConnected
- Category: Twitch/Channel/Follow & Stream
- Needs: a Twitch login (the broadcaster)

Fires when the broadcaster's Twitch chat connects.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.

### When

When the broadcaster's Twitch connection comes up, at the same time as Broadcaster Authenticated. It doesn't fire on automatic reconnects.

## Broadcaster Chat Disconnected

- Kind: trigger
- Code: 175
- Type: TwitchBroadcasterChatDisconnected
- Category: Twitch/Channel/Follow & Stream
- Needs: a Twitch login (the broadcaster)

Fires when the broadcaster's Twitch chat disconnects.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster, when the connection was lost. None when Streamerz disconnects from Twitch.

### When

When the connection is lost, or when Streamerz disconnects from Twitch.

## SevenTV Emote Removed

- Kind: trigger
- Code: 181
- Type: TwitchSevenTVEmoteRemoved
- Category: Twitch/Channel/Emotes & Badges
- Needs: a Twitch login (the broadcaster)

Fires when a 7TV emote is removed from the channel.

### Fields

None.

### Arguments

- `emote`: the name of the emote.
- `imageUrl`: the URL of the emote's image.

### When

While the Twitch channel is connected and an action uses one of the 7TV or BetterTTV emote triggers, each time 7TV reports a removed emote.

## BetterTTV Emote Removed

- Kind: trigger
- Code: 183
- Type: TwitchBetterTTVEmoteRemoved
- Category: Twitch/Channel/Emotes & Badges
- Needs: a Twitch login (the broadcaster)

Fires when a BetterTTV emote is removed from the channel.

### Fields

None.

### Arguments

- `emote`: the name of the emote.
- `imageUrl`: the URL of the emote's image.

### When

While the Twitch channel is connected and an action uses one of the 7TV or BetterTTV emote triggers, each time BetterTTV reports a removed emote.

## Warning Acknowledged

- Kind: trigger
- Code: 471
- Type: TwitchWarningAcknowledged
- Category: Twitch/Channel/Follow & Stream
- Needs: a Twitch login (the broadcaster)

Fires when a user acknowledges a warning from the channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the broadcaster.
- `user`, `userName`, `userId`: the user who acknowledged the warning.

### When

When Twitch reports that a user acknowledged a warning.
