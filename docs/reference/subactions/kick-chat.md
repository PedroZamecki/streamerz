# Kick / Chat (sub-actions)

Sends messages to the Kick chat, and replies to a chat message.

## Send Message to Channel

- Kind: sub-action
- Code: 35001
- Type: KickSendMessage
- Category: Kick/Chat
- Needs: a Kick login: the bot account when Use Bot Account is on, else the broadcaster account

Sends a message to the Kick chat, as the broadcaster or as the bot account. Variables like `%user%` in the text are filled in first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `text` | The text to send. |
| Use Bot Account | `useBot` | On: the bot account sends it. Off: the broadcaster does. |
| Fallback to Broadcaster | `fallback` | On: if the chosen account isn't logged in, the other one sends it. |

### Arguments

None set.

### Notes

- Off by default, so the broadcaster account sends the message unless Use Bot Account is on.

## Reply To Message

- Kind: sub-action
- Code: 35002
- Type: KickReplyToMessage
- Category: Kick/Chat
- Needs: a Kick login: the bot account when Use Bot Account is on, else the broadcaster account

Replies to a message in the Kick chat, by the ID of that message. Variables are filled in first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `text` | The text of the reply. |
| Use Bot Account | `useBot` | On: the bot account sends the reply. Off: the broadcaster does. |
| Fallback to Broadcaster | `fallback` | On: if the chosen account isn't logged in, the other one sends the reply. |
| Reply Message ID | `replyId` | The ID of the chat message to reply to. |

### Arguments

None set.

### Notes

- The other account is used when the chosen one isn't logged in, as with Send Message to Channel.
