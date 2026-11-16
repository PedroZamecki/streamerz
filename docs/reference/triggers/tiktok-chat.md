# TikTok / Chat (triggers)

Chat messages from a TikTok Live. They need a TikTok channel set: Streamerz connects to its live by itself.

## Chat Message

- Kind: trigger
- Code: 41001
- Type: TikTokChatMessage
- Category: TikTok/Chat
- Needs: a TikTok channel set (with none, nothing is opened)

Fires on every standard chat message posted in TikTok Live chat.

### Fields

None.

### Arguments

- `user`: the viewer's nickname, or their unique id when there is no nickname.
- `message`: the text of the comment.
- `color`: the colour of the name in TikTok's dark mode.
- `badges`: a list of badges, each with `key`, `name` and `imageUrl`.
- `channel`: the channel as set.
- The rest of the chat event's fields, such as `userId`, `uniqueId`, `msgId`, `createTime`, `isModerator`, `isSubscriber`, `followRole` and `emotes`.

### When

Fires for each chat comment as it arrives. Comments posted before Streamerz joined the live are not fired: when they arrive in the first few seconds after connecting, they only show in chat.

### Notes

None.

## Room Pin

- Kind: trigger
- Code: 41013
- Type: TikTokRoomPin
- Category: TikTok/Chat
- Needs: a TikTok channel set and connected

Fires when a comment is pinned or unpinned in TikTok LIVE chat.

### Fields

None.

### Arguments

- `user`: the user whose comment was pinned.
- `message`: the content of the pinned comment.
- `channel`: the TikTok channel name.
- `action`: the pin action (pin or unpin).

### When

When a comment is pinned or unpinned during the TikTok LIVE.

## Message Deleted

- Kind: trigger
- Code: 41014
- Type: TikTokMessageDeleted
- Category: TikTok/Chat
- Needs: a TikTok channel set and connected

Fires when TikTok deletes messages during a LIVE.

### Fields

None.

### Arguments

- `channel`: the TikTok channel name.
- `messageIds`, `userIds`: deleted message IDs and user IDs reported by TikTok.
- `messageId`, `userId`: first deleted message ID and user ID, when present.

### When

When TikTok delivers a `messageDelete` moderation event during a LIVE.

## Message Pinned

- Kind: trigger
- Code: 41015
- Type: TikTokMessagePinned
- Category: TikTok/Chat
- Needs: a TikTok channel set and connected

Fires when a comment is pinned in TikTok LIVE chat.

### Fields

None.

### Arguments

- `user`, `userId`, `uniqueId`: the user whose comment was pinned.
- `message`, `messageId`, `pinMsgId`: the pinned comment text and message IDs.
- `pinnedBy`: the operator who pinned it, if provided.
- `channel`: the TikTok channel name.

### When

When a comment is pinned during the TikTok LIVE.

## Message Unpinned

- Kind: trigger
- Code: 41016
- Type: TikTokMessageUnpinned
- Category: TikTok/Chat
- Needs: a TikTok channel set and connected

Fires when a pinned comment is unpinned in TikTok LIVE chat.

### Fields

None.

### Arguments

- `user`, `userId`, `uniqueId`: the user whose comment was unpinned.
- `message`, `messageId`, `pinMsgId`: the unpinned comment text and IDs, if provided.
- `pinnedBy`: the operator who unpinned it, if provided.
- `channel`: the TikTok channel name.

### When

When a comment is unpinned during the TikTok LIVE.
