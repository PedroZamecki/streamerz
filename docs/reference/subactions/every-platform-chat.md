# Every Platform / Chat (sub-actions)

Chat sub-actions that act on each platform picked, or on the one the trigger came from.

## Send Message

- Kind: sub-action
- Code: 99700
- Type: AllSendMessage
- Category: Every Platform/Chat
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Sends a message to the chat of each platform picked, or of the one the trigger came from. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platforms | `platforms` | The platforms to act on, ticked. None ticked: the platform the trigger came from. |
| Message | `text` | The text to send. Variables like `%user%` are filled in first. |
| Send as the bot | `useBot` | On: the bot account sends it. Off: the broadcaster does. |
| Use the broadcaster if the bot can't | `fallback` | On: when the chosen account can't send, the other one does. |

### Arguments

None set.

### Notes

- YouTube and TikTok send as the account they have; Send as the bot applies to Twitch and Kick.

## Reply to Message

- Kind: sub-action
- Code: 99701
- Type: AllReplyToMessage
- Category: Every Platform/Chat
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Replies to the message that started the action, on its platform. Twitch and Kick reply to it; YouTube and TikTok put the name in front. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `messageId` | Which message: the one that started the action (`%msgId%`, the default), or a message's ID. |
| Message | `text` | The text to send. Variables like `%user%` are filled in first. |
| Send as the bot | `useBot` | On: the bot account sends it. Off: the broadcaster does. |
| Use the broadcaster if the bot can't | `fallback` | On: when the chosen account can't send, the other one does. |

### Arguments

None set.

## Delete Message

- Kind: sub-action
- Code: 99702
- Type: AllDeleteMessage
- Category: Every Platform/Chat
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Deletes the message that started the action, on its platform (Twitch, Kick, YouTube). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `messageId` | Which message: the one that started the action (`%msgId%`, the default), or a message's ID. |

### Arguments

- Those of the platform's own sub-action (`deleteResult`).

### Notes

- TikTok is skipped with a log line for now.

## Pin Message

- Kind: sub-action
- Code: 99703
- Type: AllPinMessage
- Category: Every Platform/Chat
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Pins the message that started the action, on its platform (Twitch, TikTok). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `messageId` | Which message: the one that started the action (`%msgId%`, the default), or a message's ID. |
| Duration | `duration` | How long it stays pinned on Twitch (30 seconds to 30 minutes). 0: until the stream ends. |

### Arguments

None set.

## Unpin Message

- Kind: sub-action
- Code: 99704
- Type: AllUnpinMessage
- Category: Every Platform/Chat
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Unpins the pinned message, on the platform the trigger came from (Twitch, TikTok). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

None.

### Arguments

None set.

## Shoutout

- Kind: sub-action
- Code: 99712
- Type: AllShoutout
- Category: Every Platform/Chat
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Gives someone a shoutout: Twitch's own shoutout, and a message on the other platforms picked. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platforms | `platforms` | The platforms to act on, ticked. None ticked: the platform the trigger came from. |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |
| Message | `text` | The message for the platforms without shoutouts, such as `Go follow @%targetUser%!`. Empty: only Twitch. |

### Arguments

None set.
