# Integrations / Spotify / Browse (sub-actions)

## Get profile

- Kind: sub-action
- Code: 99870
- Type: SpotifyGetProfile
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Reads the streamer's Spotify profile: name, country, plan and followers. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Get top songs

- Kind: sub-action
- Code: 99871
- Type: SpotifyGetTopTracks
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Reads the 20 songs the streamer listens to most. The period is `short` (4 weeks), `medium` (6 months, the default) or `long` (all time). The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Get top artists

- Kind: sub-action
- Code: 99872
- Type: SpotifyGetTopArtists
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Reads the 20 artists the streamer listens to most. The period is `short`, `medium` or `long`. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Search

- Kind: sub-action
- Code: 99873
- Type: SpotifySearch
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Searches Spotify and reads the first 10 results. The type is track (the default), artist, album or playlist. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Get current artist

- Kind: sub-action
- Code: 99874
- Type: SpotifyGetCurrentArtist
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Reads the artist of the song that plays: genres, followers and popularity. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Get current album

- Kind: sub-action
- Code: 99875
- Type: SpotifyGetCurrentAlbum
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Reads the album of the song that plays: its songs, year and label. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.
