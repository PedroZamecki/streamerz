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
