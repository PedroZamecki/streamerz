# Twitch / Clips (sub-actions)

Twitch clips: look up a clip by its id.

## Get Clip Download Urls

- Kind: sub-action
- Code: 573
- Type: TwitchGetClipDownloadUrls
- Category: Twitch/Clips
- Needs: a Twitch login (any role)

Looks up a clip on Twitch by its id.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Clip ID | `clipId` | The clip's id. Variables are filled in first. |

### Arguments

When the clip is found:

- `clipUrl`: the clip's Twitch page link.
- `clipThumbnailUrl`: the clip's thumbnail image link.

### Notes

- The name says download URLs, but the sub-action only gives the clip's page link and its thumbnail. It doesn't give a direct video file.
- When the clip isn't found, no arguments are added.

## Get Clips for Target

- Kind: sub-action
- Code: 99514
- Type: TwitchGetClipsForTarget
- Category: Twitch/Clips
- Needs: a Twitch login

Picks one of a channel's clips: at random, the newest or the most viewed. Made for shoutouts that play a clip. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Channel | `userLogin` | The channel: by default the target (`%targetUserName%`, as Get User Info for Target sets it), or any name. |
| Which clip | `pick` | Which one: a random one, the newest or the most viewed. |
| Featured clips only | `featured` | On: only the clips the channel features. |
| From the last days | `days` | Only clips made in the last days. 0: any time. |

### Arguments

- `clipFound`: true when the channel has a clip that fits.
- `clipCount`: how many clips were looked at (up to 100).
- `clipId`, `clipUrl`, `clipEmbedUrl`, `clipTitle`, `clipViewCount`, `clipCreatedAt`, `clipDuration`, `clipCreatorName`, `clipGameId`, `clipThumbnailUrl`: the clip picked.
