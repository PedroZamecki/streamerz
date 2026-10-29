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
