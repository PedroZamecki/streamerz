# Streamerz / Spotify (sub-actions)

## Spotify Song Request (Streamerz)

- Kind: sub-action
- Code: 99802
- Type: StreamerzSpotifyQueue
- Category: Integrations/Spotify
- Needs: a connected Spotify account

Adds a track to the Spotify queue. This Streamerz-specific sub-action is not implemented by
Streamer.bot. In a development instance, Spotify must be connected explicitly.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Operation | `operation` | `request` (the default), `remove`, `current`, `next`, `queue`, `pos`, `skip`, `voteskip`, `play`, `pause`, `volume`, `bansong` or `like`. |
| Song | `query` | A track link, Spotify URI, or search phrase. For `remove`, a moderator's `@name` or number; for `volume`, 0 to 100. |
| Who | `who` | The lowest role: `everyone`, `subscribers`, `vips`, `moderators` or `broadcaster`. |
| Reply | `reply`, `replyDone` | What `spotifyReply` says when it worked (and when a vote or a volume finished). |
| Limits | `maxPerUser`, `maxPerSubscriber`, `unlimitedWho`, `maxLengthMs` | Songs per viewer (0 is no limit), for subscribers, the roles with no limit, the longest song. |
| Blocks | `blockExplicit`, `explicitWho`, `blockedUsers`, `blockedArtists`, `blockedSongs` | Explicit songs and who may still ask, then lists separated by commas. |
| Skipping | `skipOwn`, `skipCooldownMs`, `votesNeeded` | A viewer skips their own song, the wait between skips, the votes. |
| Replies to failures | `msgNotAllowed`, `msgNoQuery`, `msgNotFound`, `msgUnsupported`, `msgBlockedArtist`, `msgBlockedSong`, `msgExplicit`, `msgUnavailable`, `msgTooLong`, `msgDuplicate`, `msgMaxRequests`, `msgSkipCooldown`, `msgError`, `msgEmpty`, `msgNoPos`, `msgNoRemove`, `msgNoVolume` | What `spotifyReply` says for each; empty uses the plain text. |

Each field can be a variable such as `%group.maxPerUser%`, so a group's settings drive it. A request is
given to Spotify's queue at once (on the active device, or the device Spotify last had).

### Arguments

- `spotifyQueued`: `true` or `false`, as text, so an If compares it with `true`.
- `spotifyReply`: what to say; empty when nothing should be said (a blocked viewer).
- `spotifyError`: why it failed, in words for the viewer.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`, `spotifyRequester`, `spotifyRequestedBy`: the song.
- `spotifyPosition`, `spotifyTimeToPlay`: where a request is in the queue (1 is next), and when it plays.
- `spotifyList`: for `queue` and `pos`, "#1 Artist - Title (@user) | #2 ...".
- `spotifyVotes`, `spotifyVolume`, `spotifyMaxLength`, `spotifyMaxRequests`, `spotifyPlaying`.
