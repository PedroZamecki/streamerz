# Integrations / Spotify / Song Requests (sub-actions)

## Take back request

- Kind: sub-action
- Code: 99827
- Type: SpotifyTakeBackRequest
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

Takes back the viewer's last song request: it is skipped when it plays. A moderator can name `@user` or a number in Query. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Request | `query` | Leave blank for the viewer's latest request. A moderator can enter `@name` or a request number from Get song requests. Variables are expanded. |

### Arguments

- `spotifyQueued`: `true` as text when the operation succeeds, `false` when refused or failed.
- `spotifyError`: reason for a refusal or failure; empty on success.
- `spotifyReply`: configured reply, or failure text. Add Send Message to Live Chat to send it.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when one is known.
- `spotifyRequester`, `spotifyRequestedBy`: original requester when known.

These steps read `user`, `userId` and viewer role flags from the trigger. The full Spotify Song Request step exposes advanced permission and reply settings in JSON.

### Example

Leave Request blank in a viewer command to take back that viewer's latest request.

## Skip song

- Kind: sub-action
- Code: 99828
- Type: SpotifySkipSong
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

Skips the song if the viewer is allowed (Who, or their own request) and the wait since the last skip is over. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `who` | Lowest role allowed: `everyone`, `subscribers`, `vips`, `moderators` or `broadcaster`. Blank allows everyone. |
| Skip own song | `skipOwn` | `true` allows a viewer to skip their own requested song even below the selected role; `false` requires that role. Default: `true`. |
| Wait between skips | `skipCooldownMs` | Minimum wait after a successful skip, in milliseconds. `10000` means 10 seconds; zero means no wait. Variables are expanded. |

### Arguments

- `spotifyQueued`: `true` as text when the operation succeeds, `false` when refused or failed.
- `spotifyError`: reason for a refusal or failure; empty on success.
- `spotifyReply`: configured reply, or failure text. Add Send Message to Live Chat to send it.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when one is known.
- `spotifyRequester`, `spotifyRequestedBy`: original requester when known.

These steps read `user`, `userId` and viewer role flags from the trigger. The full Spotify Song Request step exposes advanced permission and reply settings in JSON.

### Example

Set Who to `moderators`, Skip own song to `true` and Wait between skips to `10000`.

## Vote to skip

- Kind: sub-action
- Code: 99829
- Type: SpotifyVoteSkip
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

Counts one vote to skip; the song is skipped when Votes needed is reached. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Votes needed | `votesNeeded` | Number of different viewers needed to skip. Blank or zero means one. Votes reset when the song changes or the vote skips it. |
| Wait between skips | `skipCooldownMs` | Minimum wait after a successful skip, in milliseconds. Zero means no wait. Variables are expanded. |

### Arguments

- `spotifyQueued`: `true` as text when the operation succeeds, `false` when refused or failed.
- `spotifyError`: reason for a refusal or failure; empty on success.
- `spotifyReply`: configured reply, or failure text. Add Send Message to Live Chat to send it.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when one is known.
- `spotifyRequester`, `spotifyRequestedBy`: original requester when known.
- `spotifyVotes`: current votes and required votes, such as `2/3`. Repeated votes from one viewer are ignored.

These steps read `user`, `userId` and viewer role flags from the trigger. The full Spotify Song Request step exposes advanced permission and reply settings in JSON.

### Example

Set Votes needed to `3`; three different viewers must vote for the same song.

## Block song

- Kind: sub-action
- Code: 99830
- Type: SpotifyBlockSong
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

Blocks the song that plays (it is added to the group's `blockedSongs` setting) and skips it. The action must belong to a group; otherwise the operation fails. This is a Streamerz extension.

### Fields

No settings. Uses the current song on the Spotify account.

### Arguments

- `spotifyQueued`: `true` as text when the operation succeeds, `false` when refused or failed.
- `spotifyError`: reason for a refusal or failure; empty on success.
- `spotifyReply`: configured reply, or failure text. Add Send Message to Live Chat to send it.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when one is known.
- `spotifyRequester`, `spotifyRequestedBy`: original requester when known.

These steps read `user`, `userId` and viewer role flags from the trigger. The full Spotify Song Request step exposes advanced permission and reply settings in JSON.

## Like song

- Kind: sub-action
- Code: 99831
- Type: SpotifyLikeSong
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Saves the song that plays to the liked songs. This is a Streamerz extension.

### Fields

No settings. Uses the current song on the Spotify account.

### Arguments

- `spotifyQueued`: `true` as text when the operation succeeds, `false` when refused or failed.
- `spotifyError`: reason for a refusal or failure; empty on success.
- `spotifyReply`: configured reply, or failure text. Add Send Message to Live Chat to send it.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when one is known.
- `spotifyRequester`, `spotifyRequestedBy`: original requester when known.

These steps read `user`, `userId` and viewer role flags from the trigger. The full Spotify Song Request step exposes advanced permission and reply settings in JSON.

## Clear song requests

- Kind: sub-action
- Code: 99832
- Type: SpotifyClearRequests
- Category: Integrations/Spotify/Song Requests
- Needs: nothing

Forgets the song requests Streamerz keeps. Spotify's own queue is not touched. This is a Streamerz extension.

### Fields

No settings. Uses the song requests kept by Streamerz.

### Arguments

- `spotifyOk`: `true` as text after the local request list is cleared.

## Get song requests

- Kind: sub-action
- Code: 99833
- Type: SpotifyGetRequests
- Category: Integrations/Spotify/Song Requests
- Needs: nothing

Lists the pending song requests that Streamerz keeps, including who requested each song. This is a Streamerz extension.

### Fields

No settings. Uses the song requests kept by Streamerz.

### Arguments

- `spotifyRequestCount`: number of pending requests, as text.
- `spotifyList`: readable list, such as `#1 Artist - Title (viewer) | #2 ...`.

## Open or close song requests

- Kind: sub-action
- Code: 99834
- Type: SpotifySetRequestsState
- Category: Integrations/Spotify/Song Requests
- Needs: the song requests template (its group holds the settings)

Opens or closes the song requests: while closed, !sr answers that they are closed. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Song requests | `state` | Open, close, or the other way from now. |

### Arguments

- `spotifyRequestsOpen`: `true` or `false`, as text.

### Notes

- It changes the Requests open setting of the song requests group: the action's own group when it is that one, else the template's.

## Take back a viewer's requests

- Kind: sub-action
- Code: 99835
- Type: SpotifyTakeBackUserRequests
- Category: Integrations/Spotify/Song Requests
- Needs: a Spotify login

Takes back every song a viewer has waiting: each is skipped when it comes up. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Who | `userLogin` | The viewer, by name. |

### Arguments

- `spotifyTakenBack`: how many were taken back, as text.

### Notes

- Spotify can't take a song out of its queue, so Streamerz skips each one when it starts, as !remove does.

## Block a viewer or artist

- Kind: sub-action
- Code: 99836
- Type: SpotifyBlockViewerOrArtist
- Category: Integrations/Spotify/Song Requests
- Needs: the song requests template (its group holds the settings)

Adds a viewer to the blocked viewers, or an artist to the blocked artists. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Block | `what` | A viewer or an artist. |
| Name (blank artist: the one playing) | `value` | Who to block. For an artist, empty is the artist of the song playing now. |

### Arguments

- `spotifyBlocked`: the name that was blocked.

## Change a song request rule

- Kind: sub-action
- Code: 99837
- Type: SpotifyChangeRequestRule
- Category: Integrations/Spotify/Song Requests
- Needs: the song requests template (its group holds the settings)

Changes one of the song request settings, such as the songs per viewer or who can request. This sub-action is Streamerz only: Streamer.bot does not have it, and skips it in an export.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Rule | `rule` | Which setting: who can request, songs per viewer or per subscriber, no limit for, longest song, block explicit songs, explicit songs allowed for, who can skip, a viewer can skip their own song, votes to skip, wait between skips, or who controls playback. |
| New value | `value` | The value as you would type it in the group's settings: a number, `true` or `false`, a time such as `5:00`, or a role such as `subscribers`. |

### Arguments

- `spotifyRule`: the setting's name.
- `spotifyRuleValue`: what it holds now.

### Notes

- A value the setting doesn't take is refused and logged; the setting stays as it was.

