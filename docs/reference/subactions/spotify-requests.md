# Integrations / Spotify / Song Requests (sub-actions)

## Take back request

- Kind: sub-action
- Code: 99827
- Type: SpotifyTakeBackRequest
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

Takes back the viewer's last song request: it is skipped when it plays. A moderator can name `@user` or a number in Query. The `spotifyQueued`, `spotifyReply` and `spotifyError` arguments tell how it went (see Spotify Song Request). This is a Streamerz extension.

## Skip song

- Kind: sub-action
- Code: 99828
- Type: SpotifySkipSong
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

Skips the song if the viewer is allowed (Who, or their own request) and the wait since the last skip is over. The `spotifyQueued`, `spotifyReply` and `spotifyError` arguments tell how it went (see Spotify Song Request). This is a Streamerz extension.

## Vote to skip

- Kind: sub-action
- Code: 99829
- Type: SpotifyVoteSkip
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

Counts one vote to skip; the song is skipped when Votes needed is reached. The `spotifyQueued`, `spotifyReply` and `spotifyError` arguments tell how it went (see Spotify Song Request). This is a Streamerz extension.

## Block song

- Kind: sub-action
- Code: 99830
- Type: SpotifyBlockSong
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

Blocks the song that plays (it is added to the group's `blockedSongs` setting) and skips it. The `spotifyQueued`, `spotifyReply` and `spotifyError` arguments tell how it went (see Spotify Song Request). This is a Streamerz extension.

## Like song

- Kind: sub-action
- Code: 99831
- Type: SpotifyLikeSong
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Saves the song that plays to the liked songs. The `spotifyQueued`, `spotifyReply` and `spotifyError` arguments tell how it went (see Spotify Song Request). This is a Streamerz extension.

## Clear song requests

- Kind: sub-action
- Code: 99832
- Type: SpotifyClearRequests
- Category: Integrations/Spotify/Song Requests
- Needs: nothing

Forgets the song requests Streamerz keeps. Spotify's own queue is not touched. The `spotifyQueued`, `spotifyReply` and `spotifyError` arguments tell how it went (see Spotify Song Request). This is a Streamerz extension.

## Get song requests

- Kind: sub-action
- Code: 99833
- Type: SpotifyGetRequests
- Category: Integrations/Spotify/Song Requests
- Needs: nothing

Sets `spotifyRequestCount` and `spotifyList` ("#1 Artist - Title (user) | ..."). The `spotifyQueued`, `spotifyReply` and `spotifyError` arguments tell how it went (see Spotify Song Request). This is a Streamerz extension.
