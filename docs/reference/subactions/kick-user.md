# Kick / User (sub-actions)

Looks up Kick channels and users, and picks random people from the chat.

## Add Broadcaster Information

- Kind: sub-action
- Code: 35010
- Type: KickAddBroadcasterInformation
- Category: Kick/User
- Needs: nothing

Adds the name, profile link and ID of the broadcaster's Kick channel for the sub-actions after it.

### Fields

None.

### Arguments

- `broadcastUser`: the channel name, as set in Streamerz.
- `broadcastUserName`: the channel name in lowercase.
- `broadcastUserType`: `kick`.
- `broadcasterProfileUrl`: the link to the channel on Kick.
- `broadcastUserId`: the ID of the channel. When it can't be looked up, the channel name is used.

### Notes

- The ID is looked up on Kick only when Streamerz is connected to Kick.

## Get User Info for Target

- Kind: sub-action
- Code: 35011
- Type: KickAddTargetInfo
- Category: Kick/User
- Needs: a Kick broadcaster login (the lookup runs as the broadcaster)

Looks up a Kick channel by its login and adds its details for the sub-actions after it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the channel. A leading `@` is removed. |

### Arguments

- `addTargetResult`: true when the channel was found.
- `targetUser` and `targetUserName`: the channel's slug.
- `targetUserId`: the ID of the channel's broadcaster.
- `targetUserType`: `kick`.
- `targetDescription` and `targetDescriptionEscaped`: the channel description, plain and escaped.
- `targetChannelTitle`: the stream title.
- `game` and `gameId`: the category name and ID.
- `tagCount`, `tag0`, `tag1`...: the stream tags, one argument each.
- `tagsDelimited`: the tags joined with commas.
- `tags`: the list of tags.

### Notes

- The login is lowercased. When the channel isn't found, or the lookup fails, `addTargetResult` is false and an error is logged.

## Add Random Users

- Kind: sub-action
- Code: 35012
- Type: KickAddRandomUsers
- Category: Kick/User
- Needs: nothing

Picks random users for the sub-actions after it. Kick doesn't give a list of who is in the chat, so the pick comes from the people who chatted in this session.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Present users only | `present` | On or off. |
| Count | `count` | How many users to pick, a whole number. At least 1. |
| Include from | `includeFrom` | A whole number. |
| Groups (one per line) | `groups` | A list of group names, one per line. |

### Arguments

- `randomUser0`, `randomUser1`...: the names of the picked users.
- `randomUserName0`...: the names in lowercase.
- `randomUserId0`...: the IDs of the users.
- `randomUserType0`...: `kick`.
- `randomUsersCount`: how many users were picked. It can be less than Count when fewer people chatted.

### Notes

- Only Count is used for now. Present users only, Include from and Groups are saved but not read yet.
