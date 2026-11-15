# Twitch / Moderation / Unban Requests (sub-actions)

Reads and answers the requests to be unbanned.

## Get Unban Requests

- Kind: sub-action
- Code: 99509
- Type: TwitchGetUnbanRequests
- Category: Twitch/Moderation/Unban Requests
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Lists the channel's unban requests. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Requests | `status` | Which ones: waiting, approved or denied. |

### Arguments

- `unbanRequestCount`: how many requests were found.
- `unbanRequest0Id`, `unbanRequest0User`, `unbanRequest0UserName`, `unbanRequest0UserId`, `unbanRequest0Text`, `unbanRequest0CreatedAt`, and so on for each one (`unbanRequest1Id`...).

## Resolve Unban Request

- Kind: sub-action
- Code: 99510
- Type: TwitchResolveUnbanRequest
- Category: Twitch/Moderation/Unban Requests
- Needs: a Twitch login as a moderator of the channel (the broadcaster, or a moderator there)

Approves or denies an unban request. This sub-action is Streamerz only: Streamer.bot does not have it.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Unban request | `requestId` | The request: the one that started the action (`%requestId%`, from Unban Request Created), or its ID. |
| Approve (off: deny) | `approve` | On: approve (they're unbanned). Off: deny. |
| Note for the viewer | `resolutionText` | A note the viewer sees. Optional. |

### Arguments

- `unbanRequestResult`: true when Twitch took the decision.
