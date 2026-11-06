# Integrations / Spotify / Library (sub-actions)

## Unlike song

- Kind: sub-action
- Code: 99850
- Type: SpotifyUnlikeSong
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Takes the song that plays out of the liked songs. This is a Streamerz extension.

### Fields

No settings. Uses the current song on the Spotify account.

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Is song liked

- Kind: sub-action
- Code: 99851
- Type: SpotifyIsSongLiked
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Says if the song that plays is in the liked songs. The result is true or false. This is a Streamerz extension.

### Fields

No settings. Uses the current song on the Spotify account.

### Arguments

- `spotifyResult`: `true` or `false` as text, indicating whether the current song is liked.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Get liked songs

- Kind: sub-action
- Code: 99852
- Type: SpotifyGetLikedSongs
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Reads the last 50 liked songs. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Object containing up to 50 liked songs.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Follow artist

- Kind: sub-action
- Code: 99853
- Type: SpotifyFollowArtist
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Follows the artist of the song that plays. This is a Streamerz extension.

### Fields

No settings. Uses the current song on the Spotify account.

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Unfollow artist

- Kind: sub-action
- Code: 99854
- Type: SpotifyUnfollowArtist
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Stops following the artist of the song that plays. This is a Streamerz extension.

### Fields

No settings. Uses the current song on the Spotify account.

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Get followed artists

- Kind: sub-action
- Code: 99855
- Type: SpotifyGetFollowedArtists
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Reads the first 50 artists the streamer follows. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Object containing up to 50 followed artists.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.
