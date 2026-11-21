# TikTok / User (sub-actions)

Picks and looks up TikTok viewers among those who chatted this session.

## Add Random Users

- Kind: sub-action
- Code: 99453
- Type: TikTokAddRandomUsers
- Category: TikTok/User
- Needs: nothing: it uses what the TikTok chat saw this session

Picks people at random among those who chatted on TikTok this session. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| How many | `count` | How many, at least 1. |

### Arguments

- `randomUsersCount`: how many were picked.
- `randomUser0`, `randomUserName0`, `randomUserId0`, `randomUserType0`, and so on for each one.

## Get User Info for Target

- Kind: sub-action
- Code: 99454
- Type: TikTokGetUserInfoForTarget
- Category: TikTok/User
- Needs: nothing: it uses what the TikTok chat saw this session

Gives what the TikTok chat knows of someone, as the target values. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | Who: the person who started the action (`%userName%`, the default), the name typed after a command (`%input0%`), or any name typed. Variables are filled in first. |

### Arguments

- `addTargetResult`: true when they chatted this session.
- `targetUser`, `targetUserName`, `targetUserId`, `targetUserType`: who they are.
- `targetUserProfileImageUrl`, `targetIsSubscribed`, `targetIsModerator`, `targetMessageCount`: what the chat saw.
