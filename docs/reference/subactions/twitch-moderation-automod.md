# Twitch / Moderation / AutoMod (sub-actions)

Decides on what AutoMod held, and sets what it holds back.

## Allow AutoMod Message

- Kind: sub-action
- Code: 99507
- Type: TwitchAllowAutoModMessage
- Category: Twitch/Moderation/AutoMod
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Lets a message AutoMod held through to the chat. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `messageId` | The held message: the one that started the action (`%messageId%`, from AutoMod Message Held), or its ID. |

### Arguments

- `autoModResult`: true when Twitch took the decision.

## Deny AutoMod Message

- Kind: sub-action
- Code: 99508
- Type: TwitchDenyAutoModMessage
- Category: Twitch/Moderation/AutoMod
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Keeps a message AutoMod held out of the chat. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Message | `messageId` | The held message: the one that started the action (`%messageId%`, from AutoMod Message Held), or its ID. |

### Arguments

- `autoModResult`: true when Twitch took the decision.

## Add Blocked Term

- Kind: sub-action
- Code: 99530
- Type: TwitchAddBlockedTerm
- Category: Twitch/Moderation/AutoMod
- Needs: the broadcaster's Twitch login with more permissions (Accounts, Twitch, Allow more)

Adds a word or phrase AutoMod blocks. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Word or phrase | `term` | The word or phrase (2 to 500 characters). |

### Arguments

None set.

## Remove Blocked Term

- Kind: sub-action
- Code: 99531
- Type: TwitchRemoveBlockedTerm
- Category: Twitch/Moderation/AutoMod
- Needs: the broadcaster's Twitch login with more permissions (Accounts, Twitch, Allow more)

Removes a word or phrase from the blocked terms. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Word or phrase | `term` | The word or phrase, as it was added (case doesn't matter). |

### Arguments

None set.

## Set AutoMod Level

- Kind: sub-action
- Code: 99532
- Type: TwitchSetAutoModLevel
- Category: Twitch/Moderation/AutoMod
- Needs: the broadcaster's Twitch login with more permissions (Accounts, Twitch, Allow more)

Sets how much AutoMod holds back: off, or 1 (the least) to 4 (the most). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| level | `level` | Off, or 1 to 4. |

### Arguments

None set.
