# Chat effects

Everything the platforms' own chats show, and what Streamerz's chat (and the user card) does with it.

- **Shown**: in the chat, as the platform shows it (or close).
- **Marked**: the message is there, set apart (a tint, a bar and its name above).
- **Event**: a line of its own: icon, who, what happened, and what they wrote with it.
- **Not yet**: possible, not done.
- **No**: the platform doesn't give it to apps (or its own chat doesn't show it).

Events come from the triggers (`engine/chatlog.rs`, `event_of`), so every trigger that fires one also
puts it in the chat, whatever reads it. Try them without a live channel: `FireTrigger` (dev mode).

## Messages, every platform

| Effect | Twitch | Kick | YouTube | TikTok |
|---|---|---|---|---|
| Text, name colour | Shown | Shown | Shown | Shown |
| Badges (as pictures, named in the user card) | Shown | Shown | Shown | Shown |
| Emotes (platform, 7TV, BTTV, FFZ) | Shown | Shown | Shown | Shown |
| Animated emotes (Options > Animated emotes) | Shown | Shown | Shown | Shown |
| Replies ("Replying to") | Shown | Shown | No | No |
| Deleted message (struck through, or removed elsewhere) | Shown | Shown | Shown | No |
| User timed out / banned (their lines go) | Shown + Event | Shown + Event | Shown | No |
| Pinned message, who pinned, time left | Shown | Shown | Shown (no end) | No |
| Chat cleared | Not yet | Not yet | No | No |

## Twitch

| Effect | State |
|---|---|
| Cheer (bits) | Event (the message is a chat line already) |
| Cheermotes as pictures ("Cheer100") | Not yet: shown as words |
| Sub, resub with its message, gift sub, gift bomb | Event |
| Raid | Event |
| Channel points redemption, with its input | Event |
| Highlighted message (channel points) | Marked |
| First message ever in the channel | Marked |
| Power-ups: gigantified emote, message effect | Marked (drawn at the usual size, no animation) |
| Announcement, in its colour | Event |
| Hype Chat (paid pinned message) | Not yet: shown as a message |
| Charity donation, Hype Train, goals | No (Twitch's chat shows them outside the messages) |
| Shared chat (messages from the other channels) | Shown (their channel isn't named yet) |

## Kick

| Effect | State |
|---|---|
| Subscription, resub, gifted subs | Event |
| Kicks (with their message) | Event |
| Channel reward redemption | Event |
| Host / raid | Not yet |

## YouTube

| Effect | State |
|---|---|
| Super Chat (amount and message) | Event (one colour, not YouTube's per amount) |
| Super Sticker | Event (the sticker isn't drawn yet) |
| New member, member milestone (with message), gifted memberships | Event |
| Poll | On the Polls page |

## TikTok

| Effect | State |
|---|---|
| Gift (a combo once, when it ends) | Event |
| Follow, share, subscribe | Event |
| Likes, joins | No (TikTok shows them as passing notices, too many to list) |
