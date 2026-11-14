# YouTube (sub-actions)

Sub-actions that post to a YouTube live stream and change its title and description. A blank Broadcast field means the live stream Streamerz is reading.

## Send Message to Channel

- Kind: sub-action
- Code: 4001
- Type: YouTubeMessage
- Category: YouTube
- Needs: a YouTube login (the broadcaster, or the bot when Use Bot Account is on)

Posts a message in the live chat of the stream. Variables like `%user%` in the text are filled in first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the live stream Streamerz is reading. |
| Message | `text` | The text to post. |
| Use Bot Account | `useBot` | On: the bot account posts the message. Off: the broadcaster does. |
| Fallback to Broadcaster | `fallback` | On: when the account picked isn't logged in, the other one posts. |

### Arguments

None set.

### Notes

- With no Broadcast, Streamerz uses the live stream it is reading. Nothing is sent while no stream is live.

## Set Title

- Kind: sub-action
- Code: 4002
- Type: YouTubeSetTitle
- Category: YouTube
- Needs: a YouTube login (the broadcaster)

Changes the title of the stream's video.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the live stream Streamerz is reading. |
| Title | `title` | The new title. |

### Arguments

None set.

### Notes

- YouTube replaces the whole video details on an update, so Streamerz reads them first and sends back the rest unchanged.

## Set Description

- Kind: sub-action
- Code: 4003
- Type: YouTubeSetDescription
- Category: YouTube
- Needs: a YouTube login (the broadcaster)

Changes the description of the stream's video.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the live stream Streamerz is reading. |
| Description | `description` | The new description. |

### Arguments

None set.

### Notes

- YouTube replaces the whole video details on an update, so Streamerz reads them first and sends back the rest unchanged.

## Set Title and Description

- Kind: sub-action
- Code: 4004
- Type: YouTubeSetTitleAndDescription
- Category: YouTube
- Needs: a YouTube login (the broadcaster)

Changes the title and the description of the stream's video in one update.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the live stream Streamerz is reading. |
| Title | `title` | The new title. |
| Description | `description` | The new description. |

### Arguments

None set.

### Notes

- YouTube replaces the whole video details on an update, so Streamerz reads them first and sends back the rest unchanged.

## Set Thumbnail

- Kind: sub-action
- Code: 4013
- Type: YouTubeSetThumbnail
- Category: YouTube
- Needs: a YouTube broadcaster login

Uploads a custom thumbnail image for the stream broadcast video.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the live stream Streamerz is reading. |
| File Path | `file` | Absolute path to the thumbnail image (JPG or PNG, up to 2MB). |

### Arguments

- `thumbnailSuccess`: true when the thumbnail was uploaded, false on error.

### Notes

- The image file must be smaller than 2MB.

## Run Commercial

- Kind: sub-action
- Code: 4014
- Type: YouTubeRunCommercial
- Category: YouTube
- Needs: a YouTube broadcaster login

Inserts an ad break cuepoint into the YouTube live broadcast.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Broadcast | `broadcast` | The video id of the stream. Leave it empty for the live stream Streamerz is reading. |
| Duration (seconds) | `duration` | Length of the ad break cuepoint in seconds. Defaults to 30 seconds if omitted. |

### Arguments

- `commercialSuccess`: true when the commercial cuepoint was accepted, false otherwise.
