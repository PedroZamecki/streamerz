# Integrations / Spotify / Library (sub-actions)

## Unlike song

- Kind: sub-action
- Code: 99850
- Type: SpotifyUnlikeSong
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Takes the song that plays out of the liked songs. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Is song liked

- Kind: sub-action
- Code: 99851
- Type: SpotifyIsSongLiked
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Says if the song that plays is in the liked songs. The result is true or false. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Get liked songs

- Kind: sub-action
- Code: 99852
- Type: SpotifyGetLikedSongs
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Reads the last 50 liked songs. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Follow artist

- Kind: sub-action
- Code: 99853
- Type: SpotifyFollowArtist
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Follows the artist of the song that plays. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Unfollow artist

- Kind: sub-action
- Code: 99854
- Type: SpotifyUnfollowArtist
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Stops following the artist of the song that plays. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Get followed artists

- Kind: sub-action
- Code: 99855
- Type: SpotifyGetFollowedArtists
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

Reads the first 50 artists the streamer follows. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.
