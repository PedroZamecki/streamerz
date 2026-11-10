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
- The message goes to the live Streamerz is reading (Accounts > TikTok > Channel). TikTok takes up to 150 characters. Longer messages are rejected with a reason, without sending a shortened copy.
- When the extension isn't connected, TikTok isn't live, or TikTok refuses the message (too fast, muted, comments off), nothing is sent and the reason is logged.
- In development mode (`STREAMERZ_DEV=1`) the message is kept for `GetChatOut` instead of sent.

## Pin Comment

- Kind: sub-action
- Code: 43011
- Type: TikTokPinComment
- Category: TikTok/Chat
- Needs: TikTok sending set up (browser extension connected) and an active live room

Pins a comment to the top of the TikTok LIVE chat.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Comment ID | `commentId` | The ID of the comment to pin. |

### Arguments

- `pinSuccess`: true when the comment was pinned, false otherwise.

## Unpin Comment

- Kind: sub-action
- Code: 43012
- Type: TikTokUnpinComment
- Category: TikTok/Chat
- Needs: TikTok sending set up (browser extension connected) and an active live room

Unpins the current pinned comment in TikTok LIVE chat.

### Fields

None.

### Arguments

- `unpinSuccess`: true when the pinned comment was removed, false otherwise.
