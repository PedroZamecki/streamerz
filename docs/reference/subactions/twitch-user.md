# Twitch / User (sub-actions)

Looking up Twitch users and teams, picking chatters and reading the broadcaster's own info.

## Add Broadcaster Information

- Kind: sub-action
- Code: 21
- Type: TwitchAddBroadcasterInformation
- Category: Twitch/User
- Needs: a Twitch login

Adds the info of the broadcaster, the logged-in channel, to the arguments.

### Fields

None.

### Arguments

- `broadcastUser`: the display name.
- `broadcastUserName`: the login.
- `broadcastUserId`: the Twitch user ID.
- `broadcastUserType`: `twitch`.
- `broadcastIsAffiliate`, `broadcastIsPartner`: true when the broadcaster is an affiliate or a partner.
- `broadcasterProfileUrl`, `broadcasterOfflineUrl`: the profile and offline image URLs.
- `broadcasterCreatedAt`: when the account was created.
- `broadcasterDescription`: the account's description.

### Notes

- Nothing is added when no broadcaster is logged in.

## Add Random Users

- Kind: sub-action
- Code: 22
- Type: TwitchAddRandomUsers
- Category: Twitch/User
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Picks random chatters from the chat and adds them to the arguments.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Present users only | `present` | On: only the users in the chat now. |
| Count | `count` | How many users to pick. Less than 1 picks one. |
| Include from | `includeFrom` | Where the users come from. |
| Groups | `groups` | The groups to pick from, one per line. |

### Arguments

For each user picked, counted from 0:

- `randomUser0`, `randomUser1`...: the display name.
- `randomUserName0`...: the login.
- `randomUserId0`...: the Twitch user ID.
- `randomUserType0`...: `twitch`.
- `randomUsersAdded`: how many users were picked.

### Notes

- Streamerz picks from the chatters that Twitch lists, up to 1000. `present`, `includeFrom` and `groups` are kept but not read yet.
- When the chat has fewer chatters than `count`, only the ones there are picked.

## Get User Info for Target

- Kind: sub-action
- Code: 50
- Type: AddTargetInfo
- Category: Twitch/User
- Needs: a Twitch login. The subscription, moderator and VIP checks need the channel owner's login (see Notes).

Looks up a Twitch user by login and adds their info to the arguments.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

- `addTargetResult`: true when the user was found, else false.
- `targetUser`, `targetUserName`, `targetUserId`, `targetUserType`: the display name, login, Twitch user ID and `twitch`.
- `targetDescription`, `targetDescriptionEscaped`: the account's description, plain and escaped for a URL or a chat message.
- `targetUserProfileImageUrl`, `targetUserProfileImageUrlEscaped`: the profile image URL, plain and URL encoded.
- `targetIsAffiliate`, `targetIsPartner`: true when the user is an affiliate or a partner.
- `createdAt`: when the account was created. `accountAge`: its age in days.
- `targetIsSubscribed`, `targetSubscriptionTier`: whether the user subscribes to the channel, and the tier.
- `targetIsModerator`, `targetIsVip`: whether the user is a moderator or a VIP of the channel.
- `targetChannelTitle`, `game`, `gameId`: the stream title and game of the user's channel.
- `tagCount`, `tag0`..., `tagsDelimited`, `tags`: the tags of the user's channel.
- `isFollowing`: whether the user follows the channel.
- `followUser`, `followUserName`, `followUserId`: the follower's name, login and ID, when they follow.
- `followAgeLong`, `followAgeShort`, `followAgeDays`, `followAgeMinutes`, `followAgeSeconds`: how long they have followed. `followAge.years`, `followAge.months`, `followAge.days`, `followAge.hours`, `followAge.minutes` and `followAge.seconds` hold the parts.

### Notes

- A leading `@` is removed from the login.
- The subscription, moderator and VIP checks need the login of the channel's owner. Without it, Streamerz doesn't ask Twitch, and those arguments come back false.

## Get Team Info for Target

- Kind: sub-action
- Code: 52
- Type: AddTeamInfo
- Category: Twitch/User
- Needs: a Twitch login

Looks up the Twitch teams that a user is in and adds them to the arguments.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The login of the user. |

### Arguments

- `_jsonTeamData`: the teams as raw JSON.
- For each team, counted from 0: `teamId0`..., `teamName0`..., `teamDisplayName0`..., `teamBackgroundUrl0`..., `teamBannerUrl0`...

### Notes

- A leading `@` is removed from the login.

## Add Present User

- Kind: sub-action
- Code: 550
- Type: TwitchAddPresentUser
- Category: Twitch/User
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Picks the chatter at a position in the list of chatters in the chat now, and adds that user to the arguments.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User index | `userIndex` | The position in the list, starting at 0. |

### Arguments

- `presentUsersCount`: how many chatters Twitch lists.
- `presentUser`, `presentUserName`, `presentUserId`: the display name, login and Twitch user ID of the chatter at that position.
- `presentUserType`: `twitch`.

### Notes

- The position starts at 0. When it isn't a number, the first chatter is picked.
- When there is no chatter at that position, only `presentUsersCount` is set.
