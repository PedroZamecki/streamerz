# Integrations / Discord (sub-actions)

Posts messages to Discord channels through a webhook.

## Basic Webhook

- Kind: sub-action
- Code: 7001
- Type: DiscordBasicWebhook
- Category: Integrations/Discord
- Needs: a Discord webhook URL (set on the action)

Posts a message to a Discord channel through a webhook URL. The content, username, avatar and image URL are filled in with variables first (like `%user%`). An image URL is sent as an embed.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Name | `name` | Text. |
| Username | `username` | Text. The name the message shows as. |
| Webhook URL | `webhookUrl` | Text. The Discord webhook to post to. |
| Content | `content` | Text, several lines. The message text. |
| Image URL | `image` | Text. Sent as an image in the message. |
| Avatar URL | `avatar` | Text. The picture the message uses. |
| Text-to-Speech | `textToSpeech` | On or off. Discord reads the message aloud. |

### Arguments

- `messageId`: the ID of the Discord message that was posted. Only set when the post works.

### Notes

- The `name` field is kept in the action but is not sent to Discord.
- Without a webhook URL, nothing is posted, and the error goes to the log.
- A failed post is logged as an error. The sub-actions after it still run.
