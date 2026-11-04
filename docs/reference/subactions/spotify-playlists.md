# Integrations / Spotify / Playlists (sub-actions)

## Get my playlists

- Kind: sub-action
- Code: 99860
- Type: SpotifyGetPlaylists
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Reads the first 50 playlists of the streamer. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Create playlist

- Kind: sub-action
- Code: 99861
- Type: SpotifyCreatePlaylist
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Creates a private playlist with a name and a description. Fires Playlist changed. The result has its id and link. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Add current song to playlist

- Kind: sub-action
- Code: 99862
- Type: SpotifyAddCurrentToPlaylist
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Adds the song that plays to a playlist (link, URI or id). Fires Playlist changed. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Add song to playlist

- Kind: sub-action
- Code: 99863
- Type: SpotifyAddSongToPlaylist
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Adds a song, found by link or words, to a playlist. Fires Playlist changed. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Remove current song from playlist

- Kind: sub-action
- Code: 99864
- Type: SpotifyRemoveCurrentFromPlaylist
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Takes the song that plays out of a playlist. Fires Playlist changed. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Get playlist songs

- Kind: sub-action
- Code: 99865
- Type: SpotifyGetPlaylistTracks
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Reads the first 50 songs of a playlist. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Rename playlist

- Kind: sub-action
- Code: 99866
- Type: SpotifyRenamePlaylist
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Gives a playlist a new name. Fires Playlist changed. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.
