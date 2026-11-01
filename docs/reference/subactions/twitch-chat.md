# Twitch / Chat (sub-actions)

Sub-actions that write to a Twitch channel's chat, and the ones that clear or pin chat messages.

## Send Message to Channel

- Kind: sub-action
- Code: 10
- Type: Message
- Category: Twitch/Chat
- Needs: a Twitch login: the broadcaster account, and the bot account too when Send from bot account is on

Sends a message to the Twitch chat, as the bot or as the broadcaster. Variables, such as `%user%`, are filled in first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `text` | The text to send. It can have several lines. |
| Pin message | `pin` | On or off. Stored, but not used yet. |
| Pin to stream end | `pinToStreamEnd` | On or off. Stored, but not used yet. |
| Send from bot account | `useBot` | On: the bot account sends it. Off: the broadcaster account does. |
| Fallback to broadcaster | `fallback` | On: the broadcaster account sends it when the bot account can't. |

### Arguments

None set.

### Notes

- Pin message and Pin to stream end are kept with the action, but nothing pins the message yet.
- The fallback applies when the bot account isn't set up or its login has expired. Without it, the message isn't sent and an error is logged.
- In dev mode the message is recorded for testing instead of being sent.
- This sub-action sends the text as it is. The chat box on the Chat page also understands Twitch's chat
  commands (`/commercial`, `/timeout`...), see the Chat guide.

## Send Announcement to Channel

- Kind: sub-action
- Code: 23
- Type: TwitchAnnounce
- Category: Twitch/Chat
- Needs: a Twitch login. From the broadcaster account, it must be a moderator of the channel (the owner counts).

Posts an announcement in the chat, shown in one of Twitch's highlight colors. Variables are filled in first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Announcement | `text` | The announcement. It can have several lines. |
| Color (Primary/Blue/Green/Orange/Purple) | `color` | Text: Primary, Blue, Green, Orange or Purple. Any other value, or an empty one, is Primary. |
| Send from bot account | `useBot` | On: the bot account sends it. Off: the broadcaster account does. |
| Fallback to broadcaster | `fallback` | On: the broadcaster account sends it when the bot account can't. |

### Arguments

None set.

### Notes

- The color name doesn't matter in upper or lower case.
- The fallback works the same as in Send Message to Channel.

## Clear Chat Messages

- Kind: sub-action
- Code: 510
- Type: TwitchClearChatMessages
- Category: Twitch/Chat
- Needs: a Twitch login that moderates the channel (the owner counts)

Clears the chat of the channel.

### Fields

None.

### Arguments

- `clearSuccess`: true when the chat was cleared, false when it wasn't.

### Notes

- It always runs as the broadcaster account. The bot account isn't used.
- Streamerz tries the call and logs what Twitch answers if it refuses.

## Reply To Message

- Kind: sub-action
- Code: 560
- Type: ReplyToMessage
- Category: Twitch/Chat
- Needs: a Twitch login: the broadcaster account, and the bot account too when Send from bot account is on

Sends a message as a reply to another message in the chat. Variables are filled in first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Reply | `text` | The reply text. It can have several lines. |
| Reply message ID | `replyId` | Text: the ID of the message to reply to. |
| Send from bot account | `useBot` | On: the bot account sends it. Off: the broadcaster account does. |
| Fallback to broadcaster | `fallback` | On: the broadcaster account sends it when the bot account can't. |

### Arguments

None set.

### Notes

- When `replyId` is empty, the text is sent as a normal message.
- The fallback works the same as in Send Message to Channel.
- In dev mode the reply is recorded for testing instead of being sent.

## Get Pinned Chat Message

- Kind: sub-action
- Code: 575
- Type: TwitchGetPinnedChatMessage
- Category: Twitch/Chat
- Needs: a Twitch login that moderates the channel (the owner counts)

Reads the message that is pinned in the chat now, and puts its details in the arguments.

### Fields

None.

### Arguments

- `pinnedMessageId`: the ID of the pinned message.
- `pinnedMessage`: the text of the pinned message.
- `pinnedUserId`: the ID of the user who wrote it.
- `pinnedUserName`: the name of the user who wrote it.
- `pinnedByUserName`: the name of the moderator who pinned it.

All of them are empty when nothing is pinned.

### Notes

- The pin sub-actions (this one, Pin Chat Message, Updated Pinned Chat Message Duration and Unpin Chat Message) each read the pinned message first. The update and unpin ones act on the message pinned at that moment.

## Pin Chat Message

- Kind: sub-action
- Code: 576
- Type: TwitchPinChatMessage
- Category: Twitch/Chat
- Needs: a Twitch login that moderates the channel (the owner counts)

Pins a chat message, by its ID.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message ID | `messageId` | Text: the ID of the message to pin. Variables are filled in first. |
| Duration (seconds) | `duration` | A whole number: how long the message stays pinned. |

### Arguments

None set.

### Notes

- A duration of 0 (or none) leaves the message pinned until the stream ends.
- Twitch accepts durations from 30 seconds to 30 minutes.

## Updated Pinned Chat Message Duration

- Kind: sub-action
- Code: 577
- Type: TwitchUpdatePinnedChatMessageDuration
- Category: Twitch/Chat
- Needs: a Twitch login that moderates the channel (the owner counts)

Changes how long the message pinned now stays pinned.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Duration (seconds) | `duration` | A whole number: the new duration. |

### Arguments

None set.

### Notes

- If nothing is pinned, nothing changes and an error is logged.
- A duration of 0 (or none) is not sent, so the same rule as Pin Chat Message applies.

## Unpin Chat Message

- Kind: sub-action
- Code: 578
- Type: TwitchUnpinChatMessage
- Category: Twitch/Chat
- Needs: a Twitch login that moderates the channel (the owner counts)

Unpins the message that is pinned now.

### Fields

None.

### Arguments

None set.

### Notes

- If nothing is pinned, an error is logged.
