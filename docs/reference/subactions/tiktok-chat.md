# TikTok / Chat (sub-actions)

Posting messages to the TikTok Live chat.

## Send Message to Live Chat

- Kind: sub-action
- Code: 43001
- Type: TikTokSendMessage
- Category: TikTok/Chat
- Needs: TikTok sending set up (Accounts > TikTok > Browser account), the Streamerz for TikTok extension connected, and the channel's live being read

Sends a message to the TikTok Live chat, as the TikTok account logged in to the browser with the extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `text` | The text to send. Variables like `%user%` are filled in first. |

Older imported account-selection fields are preserved but ignored. The browser account always sends.

### Arguments

None set.

### Notes

- TikTok has no official way to send LIVE chat. The extension uses an offscreen TikTok frame while Streamerz runs; no helper tab is visible. TikTok's own scripts sign the message.
- The message goes to the live Streamerz is reading (Accounts > TikTok > Channel). TikTok takes up to 150 characters; the rest is cut.
- When the extension isn't connected, TikTok isn't live, or TikTok refuses the message (too fast, muted, comments off), nothing is sent and the reason is logged.
- In development mode (`STREAMERZ_DEV=1`) the message is kept for `GetChatOut` instead of sent.
