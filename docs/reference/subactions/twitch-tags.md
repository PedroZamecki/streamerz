# Twitch / Tags (sub-actions)

The tags of your Twitch channel. Changing them needs the channel's owner to be logged in, so a moderator can't do it on someone else's channel.

## Clear Channel Tags

- Kind: sub-action
- Code: 251
- Type: Clear Channel Tags
- Category: Twitch/Tags
- Needs: a Twitch login as the channel's owner (the broadcaster)

Removes every tag from the channel.

### Fields

None.

### Arguments

None set.

## Set Channel Tags

- Kind: sub-action
- Code: 252
- Type: Set Channel Tags
- Category: Twitch/Tags
- Needs: a Twitch login as the channel's owner (the broadcaster)

Replaces the channel's tags with the ones in the list.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Tags | `tags` | A list of texts, one per line in the editor. |

### Arguments

None set.

### Notes


## Add Channel Tag

- Kind: sub-action
- Code: 253
- Type: Add Channel Tag
- Category: Twitch/Tags
- Needs: a Twitch login as the channel's owner (the broadcaster)

Adds one tag to the channel. The current tags are read first, so the others stay.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Tag | `tag` | The tag to add. Variables are filled in first. |

### Arguments

None set.

### Notes

- A tag that's already there (ignoring case) is taken out first and added again at the end.

## Remove Channel Tag

- Kind: sub-action
- Code: 254
- Type: Remove Channel Tag
- Category: Twitch/Tags
- Needs: a Twitch login as the channel's owner (the broadcaster)

Removes one tag from the channel. The other tags stay.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Tag | `tag` | The tag to remove. Variables are filled in first. |

### Arguments

None set.

### Notes

- The tag is matched ignoring case.
