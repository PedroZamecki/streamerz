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

## Rich Embed Webhook

- Kind: sub-action
- Code: 99740
- Type: DiscordEmbedWebhook
- Category: Integrations/Discord
- Needs: nothing: the webhook's address is the key

Posts a message to a Discord channel with a rich embed: a title, text, colour, picture, fields and footer. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Webhook URL | `webhookUrl` | The Discord webhook to post to. Variables are filled in first. |
| Name it shows as | `username` | The name the message shows as. Optional. |
| Picture URL | `avatar` | The picture the message uses. Optional. |
| Message | `content` | The text above the embed. Optional. |
| Title | `title` | The embed's title (up to 256 characters). |
| Description | `description` | The embed's text (up to 4096 characters). |
| Colour | `color` | The colour of the embed's side bar, picked. |
| Link of the title | `url` | Where the title links to. |
| Author | `author` | A line above the title. |
| Footer | `footer` | A line at the bottom. |
| Big picture URL | `image` | A big picture, by its address. |
| Small picture URL | `thumbnail` | A small picture at the side, by its address. |
| Fields (Name=value) | `fields` | Name=value lines: up to 25 small headed boxes. A line with an empty value is left out. |
| Fields side by side | `inlineFields` | On: the fields sit side by side. |
| Show the time | `timestamp` | On: the embed shows the time it was posted. |
| Thread id | `threadId` | A thread to post in (its id). Empty: the channel. |
| Post name (a forum channel) | `threadName` | A forum channel: the name of the post this starts. |

### Arguments

- `messageId`: the message posted.
- `discordChannelId`: its channel.

### Notes

- Discord's limits are applied: longer text is cut, and no more than 25 fields go.
- Variables like `%user%` are filled in everywhere first.

## Edit Message

- Kind: sub-action
- Code: 99741
- Type: DiscordEditMessage
- Category: Integrations/Discord
- Needs: nothing: the webhook's address is the key

Changes a message a webhook posted: its text, its embed, or both. Only what is given changes. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Webhook URL | `webhookUrl` | The Discord webhook to post to. Variables are filled in first. |
| Message | `messageId` | The message: the one `Rich Embed Webhook` or a Basic Webhook just posted (`%messageId%`), or its id. |
| Thread id | `threadId` | A thread to post in (its id). Empty: the channel. |
| Message | `content` | The text above the embed. Optional. |
| Title | `title` | The embed's title (up to 256 characters). |
| Description | `description` | The embed's text (up to 4096 characters). |
| Colour | `color` | The colour of the embed's side bar, picked. |
| Link of the title | `url` | Where the title links to. |
| Author | `author` | A line above the title. |
| Footer | `footer` | A line at the bottom. |
| Big picture URL | `image` | A big picture, by its address. |
| Small picture URL | `thumbnail` | A small picture at the side, by its address. |
| Fields (Name=value) | `fields` | Name=value lines: up to 25 small headed boxes. A line with an empty value is left out. |
| Fields side by side | `inlineFields` | On: the fields sit side by side. |

### Arguments

None set.

### Notes

- Use it to keep one now-playing or status message up to date instead of posting a new one each time.

## Delete Message

- Kind: sub-action
- Code: 99742
- Type: DiscordDeleteMessage
- Category: Integrations/Discord
- Needs: nothing: the webhook's address is the key

Deletes a message a webhook posted. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Webhook URL | `webhookUrl` | The Discord webhook to post to. Variables are filled in first. |
| Message | `messageId` | The message: the one `Rich Embed Webhook` or a Basic Webhook just posted (`%messageId%`), or its id. |
| Thread id | `threadId` | A thread to post in (its id). Empty: the channel. |

### Arguments

None set.

## Upload File

- Kind: sub-action
- Code: 99743
- Type: DiscordUploadFile
- Category: Integrations/Discord
- Needs: nothing: the webhook's address is the key

Posts a file (a screenshot, a replay) to a Discord channel, with a message. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Webhook URL | `webhookUrl` | The Discord webhook to post to. Variables are filled in first. |
| File | `file` | The path of the file to send (up to 25 MB). |
| Name to show | `fileName` | The name shown for it. Empty: the file's own. |
| Message | `content` | The text above the embed. Optional. |
| Name it shows as | `username` | The name the message shows as. Optional. |
| Picture URL | `avatar` | The picture the message uses. Optional. |
| Thread id | `threadId` | A thread to post in (its id). Empty: the channel. |

### Arguments

- `messageId`: the message posted.

### Notes

- Discord takes up to 25 MB here; a bigger file is refused, with a log line.
