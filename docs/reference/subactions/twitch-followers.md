# Twitch / Followers (sub-actions)

Sub-actions that read follower information from Twitch for the channel or for one user.

## Get Follow Age Info for Target

- Kind: sub-action
- Code: 51
- Type: AddFollowAgeInfo
- Category: Twitch/Followers
- Needs: a Twitch broadcaster login

Looks up when a user started following the channel, and adds the age to the arguments.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User login | `userLogin` | The user to look up. A leading `@` is removed. Variables are filled in first. |

### Arguments

| Key | What it holds |
| --- | --- |
| `isFollowing` | True when the user follows the channel. False otherwise. |
| `followUser`, `followUserName`, `followUserId` | The user's display name, login and id. |
| `followDate` | The date the user followed, as text. |
| `followAgeLong`, `followAgeShort` | The age of the follow, as long and short text. |
| `followAgeDays`, `followAgeMinutes`, `followAgeSeconds` | The age of the follow in days, minutes and seconds. |

### Notes

- If the user cannot be found, or Twitch does not answer, the arguments are left as they are, with `isFollowing` set to false.
- Nothing is looked up until Twitch is set up.

## Get Follower Count

- Kind: sub-action
- Code: 506
- Type: TwitchGetFollowerCount
- Category: Twitch/Followers
- Needs: a Twitch broadcaster login

Reads the total number of followers of the channel.

### Fields

None.

### Arguments

| Key | What it holds |
| --- | --- |
| `totalFollowers` | The number of followers. Set only when Twitch answers. |

## Get Latest Follower

- Kind: sub-action
- Code: 507
- Type: TwitchGetLatestFollower
- Category: Twitch/Followers
- Needs: a Twitch broadcaster login

Reads the channel's follower data from Twitch.

### Fields

None.

### Arguments

| Key | What it holds |
| --- | --- |
| `totalFollowers` | The number of followers. Set only when Twitch answers. |

### Notes

- This action makes the same request as Get Follower Count. It sets only `totalFollowers`. The name and time of the latest follower are not set yet.
