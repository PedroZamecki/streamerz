# Kick / Chat (triggers)

Triggers for the Kick chat: the chat connecting and disconnecting, and each message.

## Broadcaster Chat Connected

- Kind: trigger
- Code: 35002
- Type: KickBroadcasterChatConnected
- Category: Kick/Chat
- Needs: a Kick channel set

Fires when Streamerz connects to the Kick chat of the channel.

### Fields

None.

### Arguments

- `channel`: the channel name.
- `channelId`: the Kick channel id.
- `chatroomId`: the Kick chatroom id.

### When

Each time the Kick chat connects.

## Broadcaster Chat Disconnected

- Kind: trigger
- Code: 35003
- Type: KickBroadcasterChatDisconnected
- Category: Kick/Chat
- Needs: a Kick channel set

Fires when the Kick chat disconnects.

### Fields

None.

### Arguments

None set.

### When

When a connected Kick chat drops, when the server closes it, or when Streamerz disconnects it. It does not fire while the chat is not connected.

## First Words

- Kind: trigger
- Code: 35008
- Type: KickFirstWords
- Category: Kick/Chat
- Needs: a Kick channel set

Fires on the first message a user sends in the Kick chat.

### Fields

None.

### Arguments

The same as Chat Message, plus:

- `firstMessage`: `true`.

### When

When a user chats on Kick for the first time since Streamerz started tracking who has spoken. It fires before Chat Message for the same message.

### Notes

- Streamerz forgets who has spoken when the Kick channel is changed.

## Chat Message

- Kind: trigger
- Code: 35010
- Type: KickChatMessage
- Category: Kick/Chat
- Needs: a Kick channel set

Fires on every message in the Kick chat.

### Fields

None.

### Arguments

- `user`: the sender's name (as Kick shows it).
- `userName`: the sender's Kick slug, or the name in lowercase.
- `userId`: the sender's Kick user id.
- `userType`: `kick`.
- `userSlug`: the sender's Kick slug.
- `color`: the name colour, as `#RRGGBB`.
- `colorR`, `colorG`, `colorB`: the colour's red, green and blue values.
- `message`: the message text.
- `messageStripped`: the message without its emote names, with single spaces.
- `messageId`: the Kick message id.
- `emoteCount`: how many emotes the message has.
- `emotes`: a JSON list of the emotes, each with `name` and `imageUrl`.
- `badgeCount`, `badge`: how many badges the sender has, and their types.
- `badges`: a list of the badges, each with `key`, `name` and `imageUrl`.
- `isBroadcaster`, `isModerator`, `isSubscribed`, `isSubscriber`, `isVip`: the sender's roles. `isSubscribed` and `isSubscriber` are the same.
- `isInternal`: `false`.
- `rawInput`, `rawInputEscaped`, `rawInputUrlEncoded`, `input0`, `inputEscaped0`, `inputUrlEncoded0` and so on: the message, and each of its words.
- `isReply`, `replyTo`, `replyToMessage`: only for a reply, with the name and text of the message replied to.
- `broadcastUser`, `broadcastUsername`, `broadcastUserId`: the broadcaster.

### When

Each time a message arrives in the Kick chat, including the broadcaster's own messages.
