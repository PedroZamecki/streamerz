# Twitch / Chat (triggers)

What happens in a Twitch channel's chat: messages, cheers, whispers, announcements, deleted messages and the chat modes.

## Cheer

- Kind: trigger
- Code: 102
- Type: TwitchCheer
- Category: Twitch/Chat/Bits & Cheers
- Needs: a Twitch login (any role in the channel)

Fires when a chat message comes with Bits cheered in it.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The chatter: `user`, `userName`, `userId`, `isSubscribed`, `isModerator`, `isVip`, `badges`, `badgeCount`.
- The message: `message`, `msgId`, `color`, `role`, `bits` (how many Bits it cheered), `messageType`, `isHighlight`, `isReply`.

### When

For each chat message with at least one Bit in it. Chat Message fires first for the same message.

### Notes

- The form has no field for it, but a stored trigger can hold `bitsMin` and `bitsMax` (0 means no limit). Then only cheers in that range fire it.

## Reward Deleted

- Kind: trigger
- Code: 115
- Type: TwitchRewardDeleted
- Category: Twitch/Chat/Messages & Modes
- Needs: the broadcaster's Twitch login (channel points belong to the owner)

Fires when a channel points custom reward is deleted.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reward (none: any) | `rewardId` | The reward this is for: picked from the channel's rewards, or typed, by name or id. Empty: any reward. |

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The reward, as Twitch sent it: `rewardId`, `rewardTitle`, `rewardCost`, `rewardPrompt`.

### When

When Twitch says a custom reward was removed.

## Whisper

- Kind: trigger
- Code: 119
- Type: TwitchWhisper
- Category: Twitch/Chat/Special Messages
- Needs: a Twitch login (the account the whisper was sent to)

Fires when a whisper arrives on Twitch for an account that isn't the bot.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The sender: `user`, `userName`, `userId`, `isSubscribed`, `isModerator`, `isVip`, `badges`, `badgeCount`.
- The text: `message`, `messageStripped`.

### When

When a whisper comes in for an account that isn't the bot account. Whispers to the bot fire Bot Whisper instead.

## First Words

- Kind: trigger
- Code: 120
- Type: TwitchFirstWord
- Category: Twitch/Chat/Special Messages
- Needs: a Twitch login (any role in the channel)

Fires the first time a user chats in the stream.

### Fields

None.

### Arguments

- The channel, the chatter and the message, as for Cheer.
- `firstMessage` (true) and `first_words` (true).

### When

On a user's first chat message after Streamerz last cleared its list of who has chatted. The list is cleared when the stream goes online, but only if the Twitch setting `first_words_auto_reset` is on.

### Notes

- A stored trigger can hold `userLogon` to fire only for one login. Empty means any user.

## Chat Message

- Kind: trigger
- Code: 133
- Type: TwitchChatMessage
- Category: Twitch/Chat/Messages & Modes
- Needs: a Twitch login (any role in the channel)

Fires on every standard chat message posted in Twitch chat.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The chatter: `user`, `userName`, `userId`, `isSubscribed`, `isModerator`, `isVip`, `badges`, `badgeCount`.
- The message: `message`, `messageStripped`, `messageCheermotesStripped`, `msgId`, `bits`, `role`, `color`, `colorR`, `colorG`, `colorB`, `emoteCount`, `emotes`, `messageType`, `isHighlight`, `isAction`, `internal`, `isReply`.
- When it replies to another message: `reply.msgId`, `reply.userId`, `reply.userLogin`, `reply.userName`, `reply.msgBody`.

### When

For each chat message the channel gets.

### Notes

- A stored trigger can hold `contains`. Then only messages with that text in them fire it (case does not matter). The form has no field for it.

## Chat Message Deleted

- Kind: trigger
- Code: 135
- Type: TwitchChatMessageDeleted
- Category: Twitch/Chat/Messages & Modes
- Needs: a Twitch login (any role in the channel)

Fires when a chat message is deleted.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The author of the deleted message: `user`, `userName`, `userId`.
- `targetMessageId` (the message's id) and `message` (its text).

### When

When Twitch says a chat message was deleted.

### Notes

- `message` is empty for a message Streamerz didn't see. It keeps the last 200 messages it has seen since connecting.
- Streamerz also removes the message from its own chat view.

## Announcement

- Kind: trigger
- Code: 138
- Type: TwitchAnnouncement
- Category: Twitch/Chat/Special Messages
- Needs: a Twitch login (any role in the channel)

Fires when a chat announcement is sent in the channel.

### Fields

None.

### Arguments

- The channel, the sender and the message, as for Cheer.
- `announceColor` (the colour Twitch gave, `PRIMARY` if none).
- `systemMessage` (Twitch's text for the notice) and `anonymous`.

### When

When Twitch sends an announcement notice in the chat.

## Bot Whisper

- Kind: trigger
- Code: 140
- Type: TwitchBotWhisper
- Category: Twitch/Chat/Special Messages
- Needs: the bot account's Twitch login

Fires when a whisper is sent to the bot account.

### Fields

None.

### Arguments

The same as Whisper.

### When

When a whisper comes in for the bot account.

## Coin Cheer

- Kind: trigger
- Code: 143
- Type: TwitchCoinCheer
- Category: Twitch/Chat/Bits & Cheers
- Needs: nothing yet (it never fires)

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

## Chat Cleared

- Kind: trigger
- Code: 157
- Type: TwitchChatCleared
- Category: Twitch/Chat/Messages & Modes
- Needs: a Twitch login (any role in the channel)

Fires when a moderator clears the whole chat.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.

### When

When Twitch says the chat was cleared.

## Hype Chat

- Kind: trigger
- Code: 170
- Type: TwitchHypeChat
- Category: Twitch/Chat/Bits & Cheers
- Needs: nothing yet (it never fires)

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

## Hype Chat Level

- Kind: trigger
- Code: 172
- Type: TwitchHypeChatLevel
- Category: Twitch/Chat/Bits & Cheers
- Needs: nothing yet (it never fires)

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never. Streamerz doesn't fire this yet.

## Chat Slow Mode On

- Kind: trigger
- Code: 456
- Type: TwitchChatSlowModeOn
- Category: Twitch/Chat/Messages & Modes
- Needs: a Twitch login (any role in the channel)

Fires when slow mode is turned on in the chat.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The chat settings after the change: `emoteMode`, `followerMode`, `followerModeDuration` (minutes), `slowMode`, `slowModeWaitTime` (seconds), `subscriberOnly`, `uniqueMode`.

### When

When a chat settings update from Twitch turns slow mode on. Streamerz compares each update with the one before it. The first update after connecting is compared with all modes being off.

## Chat Slow Mode Off

- Kind: trigger
- Code: 457
- Type: TwitchChatSlowModeOff
- Category: Twitch/Chat/Messages & Modes
- Needs: a Twitch login (any role in the channel)

Fires when slow mode is turned off in the chat.

### Fields

None.

### Arguments

The same as Chat Slow Mode On.

### When

When a chat settings update turns slow mode off. The first update after connecting is compared with all modes being off, as for Chat Slow Mode On.

## Chat Slow Mode Changed

- Kind: trigger
- Code: 458
- Type: TwitchChatSlowModeChanged
- Category: Twitch/Chat/Messages & Modes
- Needs: a Twitch login (any role in the channel)

Fires when slow mode is switched on or off, or its wait time changes.

### Fields

None.

### Arguments

The same as Chat Slow Mode On.

### When

When a chat settings update switches slow mode on or off, or changes the wait time while slow mode is on.

### Notes

- Turning slow mode on fires Chat Slow Mode On and this one.

## Blocked Terms Deleted

- Kind: trigger
- Code: 466
- Type: TwitchBlockedTermsDeleted
- Category: Twitch/Chat/Messages & Modes
- Needs: a Twitch login that moderates the channel (the broadcaster, or a moderator)

Fires when terms are removed from the blocked terms list.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The moderator who did it: `moderatorId`, `moderatorUsername`, `moderatorDisplayName`.
- `termCount` (how many terms), `term.[i]` (each term, as Twitch sent it), `fromAutomod` and `createdAt`.

### When

When Twitch says terms were removed from the blocked list.

### Notes

- `moderatorUsername` holds the moderator's display name and `moderatorDisplayName` their login. This is the reverse of what the names say, and it's kept that way for Streamer.bot's names.

## Permitted Terms Deleted

- Kind: trigger
- Code: 470
- Type: TwitchPermittedTermsDeleted
- Category: Twitch/Chat/Messages & Modes
- Needs: a Twitch login that moderates the channel (the broadcaster, or a moderator)

Fires when terms are removed from the permitted terms list.

### Fields

None.

### Arguments

The same as Blocked Terms Deleted.

### When

When Twitch says terms were removed from the permitted list.

### Notes

- The names of `moderatorUsername` and `moderatorDisplayName` are swapped, as for Blocked Terms Deleted.

## Bits Badge Tier

- Kind: trigger
- Code: 480
- Type: TwitchBitsBadgeTier
- Category: Twitch/Chat/Bits & Cheers
- Needs: a Twitch login (any role in the channel)

Fires when a viewer earns a Bits badge tier.

### Fields

None.

### Arguments

- The channel: `broadcastUser`, `broadcastUserName`, `broadcastUserId`.
- The viewer: `user`, `userName`, `userId`, `isSubscribed`, `isModerator`, `isVip`, `badges`, `badgeCount`.
- The notice: `message`, `systemMessage`, `anonymous`.
- `tier`: the Bits badge tier, as Twitch sent it.

### When

When Twitch sends a Bits badge tier notice in the chat. Not for notices from another channel in a shared chat session.
