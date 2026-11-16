# Twitch / Channel (sub-actions)

The stream itself on Twitch: title, game, stream markers, clips, viewer count and branded content.

## Set Channel Title

- Kind: sub-action
- Code: 15
- Type: SetChannelTitle
- Category: Twitch/Channel
- Needs: a Twitch login as the channel's owner (the broadcaster)

Changes the stream title.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Title | `title` | The new title. Variables like `%user%` are filled in first. |

### Arguments

- `titleSuccess`: true when Twitch accepted the new title, else false.

## Set Channel Game

- Kind: sub-action
- Code: 16
- Type: SetChannelGame
- Category: Twitch/Channel
- Needs: a Twitch login as the channel's owner (the broadcaster)

Changes the game or category of the stream. Streamerz looks the game up on Twitch first, then sets it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Source | `source` | Game name (0): the Game field is looked up by name. Game ID (1): the Game ID field is used as is. |
| Game | `game` | The game name. |
| Game ID | `gameId` | The Twitch game ID. |

### Arguments

- `gameSuccess`: true when Twitch accepted the new game, else false.
- `gameId`, `gameName`: the game Twitch found.
- `gameBoxArt`: the URL of the game's box art, at 285x380.

### Notes

- The game is looked up by `gameId` when Source is Game ID and `gameId` is filled in. Otherwise it is looked up by `game`, the name.
- When Twitch can't find the game, `gameSuccess` stays false.

## Create Stream Marker

- Kind: sub-action
- Code: 17
- Type: CreateStreamMarker
- Category: Twitch/Channel
- Needs: a Twitch login as the channel's owner (the broadcaster)

Adds a marker at the current point of the stream, so it's easy to find later.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Description | `description` | The text of the marker. |

### Arguments

- `streamMarkerSuccess`: true when Twitch made the marker, else false.
- `streamMarkerId`, `streamMarkerCreatedAt`, `streamMarkerDescription`, `streamMarkerPosition`: what Twitch returned for the marker. The position is in seconds from the start of the stream.

## Create Clip

- Kind: sub-action
- Code: 18
- Type: CreateClip
- Category: Twitch/Channel
- Needs: a Twitch login

Makes a clip of the live stream and adds its link to the arguments.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Allow a delayed clip | `hasDelay` | Lets Twitch include a delay when creating the clip. Off when omitted. |

### Arguments

- `createClipSuccess`: true when Twitch made the clip, else false.
- `clipId`, `createClipId`: the clip ID.
- `createClipUrl`: the link to the clip.
- `createClipCreatedAt`: when Streamerz made the clip, as an ISO date and time.

### Notes

- Streamerz sends only the delay setting to Twitch (`hasDelay`, off when missing). It doesn't send `title` or `duration` yet, so the clip's title and length are not set by these fields.

## Add Viewer Count

- Kind: sub-action
- Code: 24
- Type: TwitchAddViewerCount
- Category: Twitch/Channel
- Needs: a Twitch login

Reads how many viewers the stream has right now.

### Fields

None.

### Arguments

- `viewerCount`: the number of viewers. It is 0 when the stream is offline.

## Set Channel Branded Content

- Kind: sub-action
- Code: 564
- Type: SetChannelBrandedContent
- Category: Twitch/Channel
- Needs: a Twitch login as the channel's owner (the broadcaster)

Turns the branded content flag of the channel on or off.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Branded content | `status` | On turns the flag on, off turns it off. |

### Arguments

- `success`: true when Twitch accepted the change, else false.

## Get Stream Info

- Kind: sub-action
- Code: 99515
- Type: TwitchGetStreamInfo
- Category: Twitch/Channel
- Needs: a Twitch login

Says whether a Twitch channel is live, with its title, game, viewers and uptime. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | The channel. Empty: yours. |

### Arguments

- `isLive`: true while the stream is live.
- `streamTitle`, `streamCategory`: its title and category.
- `viewerCount`: how many are watching.
- `startedAt`, `uptime`, `uptimeSeconds`: when it started, and for how long it's been live (`1:02:03`).
- `streamPlatform`: the platform (`twitch`, `kick`).
- `gameId`, `language`, `thumbnailUrl`: the rest Twitch gives.

## Get Hype Train

- Kind: sub-action
- Code: 99516
- Type: TwitchGetHypeTrain
- Category: Twitch/Channel
- Needs: a Twitch login as the channel's owner (the broadcaster)

Reads the channel's hype train, and its record. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

None.

### Arguments

- `hypeTrainActive`: true while one runs.
- `hypeTrainLevel`, `hypeTrainProgress`, `hypeTrainGoal`, `hypeTrainTotal`: where it stands.
- `hypeTrainStartedAt`, `hypeTrainEndsAt`: when it started and ends.
- `hypeTrainRecordLevel`, `hypeTrainRecordTotal`: the channel's best.

## Get Creator Goals

- Kind: sub-action
- Code: 99517
- Type: TwitchGetCreatorGoals
- Category: Twitch/Channel
- Needs: a Twitch login as the channel's owner (the broadcaster)

Lists the channel's creator goals (followers, subscriptions, bits). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

None.

### Arguments

- `goalCount`: how many goals were found.
- `goal0Type`, `goal0Description`, `goal0Current`, `goal0Target`, and so on for each one (`goal1Type`...).

## Get Bits Leaderboard

- Kind: sub-action
- Code: 99518
- Type: TwitchGetBitsLeaderboard
- Category: Twitch/Channel
- Needs: a Twitch login as the channel's owner (the broadcaster)

Lists who cheered the most bits. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Period | `period` | All time, today, this week, this month or this year. |
| How many | `count` | How many, 1 to 100. |

### Arguments

- `bitsLeaderCount`: how many people were found.
- `bitsLeader0User`, `bitsLeader0UserName`, `bitsLeader0UserId`, `bitsLeader0Bits`, `bitsLeader0Rank`, and so on for each one (`bitsLeader1User`...).

## Set Content Classification

- Kind: sub-action
- Code: 99519
- Type: TwitchSetContentClassification
- Category: Twitch/Channel
- Needs: a Twitch login as the channel's owner (the broadcaster)

Sets the stream's content classification labels: those ticked on, the others off. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Labels | `labels` | The labels that apply to the stream. Twitch sets Mature-rated game itself, from the category. |

### Arguments

None set.

## Set Stream Language

- Kind: sub-action
- Code: 99520
- Type: TwitchSetStreamLanguage
- Category: Twitch/Channel
- Needs: a Twitch login as the channel's owner (the broadcaster)

Sets the stream's language. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| language | `language` | A two-letter code such as `en` or `pt`, or `other`. |

### Arguments

None set.

## Set Stream Delay

- Kind: sub-action
- Code: 99521
- Type: TwitchSetStreamDelay
- Category: Twitch/Channel
- Needs: a Twitch login as the channel's owner (the broadcaster)

Sets the stream's delay (partners only). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Delay | `delay` | How long, up to 15 minutes. 0: none. |

### Arguments

None set.

## Get Shared Chat

- Kind: sub-action
- Code: 99541
- Type: TwitchGetSharedChat
- Category: Twitch/Channel
- Needs: a Twitch login

Says whether the channel is in a Shared Chat, and with which channels. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

None.

### Arguments

- `sharedChatActive`: true while it is in one.
- `sharedChatSessionId`, `sharedChatHostId`: the session and who started it.
- `sharedChatParticipantCount`, `sharedChatParticipant0Id`...: the channels in it.

## Get Channel Info

- Kind: sub-action
- Code: 99542
- Type: TwitchGetChannelInfo
- Category: Twitch/Channel
- Needs: a Twitch login

Reads a channel's title, game, language and tags: yours, or any channel's. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | The channel. Empty: yours. |

### Arguments

- `channelUser`, `channelUserName`, `channelUserId`: the channel.
- `channelTitle`, `channelGame`, `channelGameId`, `channelLanguage`, `channelTags` (separated by commas).

## Get Latest Video

- Kind: sub-action
- Code: 99543
- Type: TwitchGetLatestVideo
- Category: Twitch/Channel
- Needs: a Twitch login

Finds a channel's latest video: the last stream, upload or highlight. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | The channel. Empty: yours. |
| kind | `kind` | The last stream (a past broadcast), the last upload or the last highlight. |

### Arguments

- `videoFound`: true when there is one.
- `videoId`, `videoTitle`, `videoUrl`, `videoDuration` (as Twitch writes it, `1h2m3s`), `videoCreatedAt`, `videoViews`, `videoThumbnail`.
