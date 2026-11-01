# TikTok / Chat (sub-actions)

Posting messages to the TikTok Live chat.

## Send Message to Live Chat

- Kind: sub-action
- Code: 43001
- Type: TikTokSendMessage
- Category: TikTok/Chat
- Needs: a TikTok login (the bot or the broadcaster) and a TikTok channel set

Sends a message to the TikTok Live chat, as the bot or as the broadcaster.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `text` | The text to send. Variables like `%user%` are filled in first. |
| Use Bot Account | `useBot` | On: the bot account sends it. Off: the broadcaster sends it. |
| Fallback to Broadcaster | `fallback` | On: when the chosen account is not logged in, the other one sends it. |

### Arguments

None set.

### Notes

- The message is posted with the session of the logged-in account.
- If the keyring is locked, or no TikTok channel is set, nothing is sent and the error is logged.
- Without the fallback, a message with no logged-in account for the chosen side is an error and is logged.
