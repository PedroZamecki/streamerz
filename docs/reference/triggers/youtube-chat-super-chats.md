# YouTube / Chat & Super Chats (triggers)

Messages in the YouTube live chat, and the paid messages, stickers and gifts sent in it.

## Message

- Kind: trigger
- Code: 4003
- Type: YouTubeMessage
- Category: YouTube/Chat & Super Chats
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires on every message in the YouTube live chat.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `message` | The text of the message. Emotes are in the text as their names. |
| `parts` | The message as a list of text and emoji parts, as JSON. |
| `emotes` | The YouTube emotes in the message, as JSON (name, position, image). |
| `rawInput`, `input0`... | The words of the message, the same way chat commands split them. |
| `user`, `userName`, `userId`, `userType`, `userProfileUrl` | The author. `user` has no `@`. |
| `color`, `badges` | The name colour and the author's badges. |
| `isSubscribed`, `isSponsor`, `userIsSponsor` | On when the author is a member of the channel. |
| `isModerator`, `isOwner`, `isVerified` | The author's role in the chat. |
| `messageId`, `id` | The id of the message. |
| `publishedAt` | When the message was sent, if YouTube gave the time. |
| `broadcastUsername`, `broadcastUserId` | The channel's name and id, when set. |
| `broadcastId`, `broadcast.*` | The live broadcast, as Broadcast Started sets them. |

### When

For each chat message that Streamerz reads from the live chat.

### Notes

- For a user's first message, Streamerz fires First Words too, just before this trigger.

## Message Deleted

- Kind: trigger
- Code: 4004
- Type: YouTubeMessageDeleted
- Category: YouTube/Chat & Super Chats
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when a message is removed from the YouTube live chat.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `messageId` | The id of the removed message. |
| `broadcastId`, `broadcast.*` | The live broadcast, as Broadcast Started sets them. |

### When

When the chat page says a message was removed. It fires for the message removal, whoever removed it.

### Notes

- Streamerz doesn't know the author of a removed message from this event alone, only its id.

## Super Chat

- Kind: trigger
- Code: 4006
- Type: YouTubeSuperChat
- Category: YouTube/Chat & Super Chats
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when a Super Chat comes in on YouTube.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `amount` | The amount as YouTube shows it, for example `$5.00`. |
| `microAmount` | The amount in millionths of the currency. |
| `currencyCode` | The currency code, for example `USD`. |
| `tier` | The background colour of the Super Chat, from YouTube. |
| `message` | The text of the Super Chat. |
| The author arguments | `user`, `userName`, `userId`, and the others the chat messages have. |

### When

For each Super Chat that Streamerz reads in the chat.

### Notes

- The amount and currency come from the text YouTube shows, so an unusual format may be read wrong.

## Super Sticker

- Kind: trigger
- Code: 4007
- Type: YouTubeSuperSticker
- Category: YouTube/Chat & Super Chats
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when a Super Sticker comes in on YouTube.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `amount`, `microAmount`, `currencyCode` | The amount, as Super Chat. |
| `tier` | The background colour of the sticker, from YouTube. |
| `stickerAltText` | The label of the sticker. |
| `stickerImageUrl` | The image of the sticker. |
| `stickerLanguage` | Always `en`. |
| The author arguments | `user`, `userName`, `userId`, and the others the chat messages have. |

### When

For each Super Sticker that Streamerz reads in the chat.

## Jewels Gifted

- Kind: trigger
- Code: 4030
- Type: YouTubeJewelsGifted
- Category: YouTube/Chat & Super Chats
- Needs: a YouTube channel or video set, and a live broadcast (no login).

Fires when a viewer gifts jewels in the YouTube chat.

### Fields

None.

### Arguments

| Argument | What it holds |
| --- | --- |
| `jewelCount` | The number of jewels. |
| `gift.jewelsAmount` | The same number. |
| The author arguments | `user`, `userName`, `userId`, and the others the chat messages have. |

### When

For each jewels gift that Streamerz reads in the chat.
