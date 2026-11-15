# Every Platform / Stream (sub-actions)

The stream on each platform picked, or on the one the trigger came from.

## Set Stream Title

- Kind: sub-action
- Code: 99709
- Type: AllSetStreamTitle
- Category: Every Platform/Stream
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Sets the stream title on each platform picked. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platforms | `platforms` | The platforms to act on, ticked. None ticked: the platform the trigger came from. |
| Title | `title` | The title. Variables are filled in first. |

### Arguments

- Those of each platform's own Set Title.

## Set Category

- Kind: sub-action
- Code: 99710
- Type: AllSetCategory
- Category: Every Platform/Stream
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Sets the category on each platform picked (Twitch, Kick). This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platforms | `platforms` | The platforms to act on, ticked. None ticked: the platform the trigger came from. |
| Category | `category` | The category, by name (the platform's best match is used). |

### Arguments

- Those of each platform's own Set Category.

## Get Stream Info

- Kind: sub-action
- Code: 99711
- Type: AllGetStreamInfo
- Category: Every Platform/Stream
- Needs: the logins of the platforms it acts on (each one is skipped, with a log line, when it can't)

Says whether the stream is live on each platform picked, with its viewers and uptime. With several, each one's values also come with its name in front, and the viewers are added up. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Platforms | `platforms` | The platforms to act on, ticked. None ticked: the platform the trigger came from. |

### Arguments

- `isLive`: true while the stream is live.
- `streamTitle`, `streamCategory`: its title and category.
- `viewerCount`: how many are watching.
- `startedAt`, `uptime`, `uptimeSeconds`: when it started, and for how long it's been live (`1:02:03`).
- `streamPlatform`: the platform (`twitch`, `kick`).
- `twitchIsLive`, `kickViewerCount`, `tiktokUptime`... : each platform's own, when several are picked.
