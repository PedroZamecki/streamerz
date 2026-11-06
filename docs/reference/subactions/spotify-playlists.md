# Integrations / Spotify / Playlists (sub-actions)

## Get my playlists

- Kind: sub-action
- Code: 99860
- Type: SpotifyGetPlaylists
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Reads the first 50 playlists of the streamer. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Object containing up to 50 playlists.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Create playlist

- Kind: sub-action
- Code: 99861
- Type: SpotifyCreatePlaylist
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Creates a private playlist with a name and a description. Fires Playlist changed. The result has its id and link. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Name | `value` | Required playlist name. Variables are expanded. |
| Description | `extra` | Optional playlist description. The playlist is created private. Variables are expanded. |

### Arguments

- `spotifyResult`: Created playlist object, including its ID and link.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Set Name to `Songs from chat` and Description to `Viewer requests`.

## Add current song to playlist

- Kind: sub-action
- Code: 99862
- Type: SpotifyAddCurrentToPlaylist
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Adds the song that plays to a playlist (link, URI or id). Fires Playlist changed. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Playlist | `value` | Full Spotify playlist link, playlist URI or playlist ID. Variables are expanded. |

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Add song to playlist

- Kind: sub-action
- Code: 99863
- Type: SpotifyAddSongToPlaylist
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Adds a song, found by link or words, to a playlist. Fires Playlist changed. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Playlist | `value` | Full Spotify playlist link, playlist URI or playlist ID. Variables are expanded. |
| Song | `extra` | Full Spotify track link, track URI or words to search. Variables are expanded. |

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Choose the playlist link and set Song to `%rawInput%`.

## Remove current song from playlist

- Kind: sub-action
- Code: 99864
- Type: SpotifyRemoveCurrentFromPlaylist
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Takes the song that plays out of a playlist. Fires Playlist changed. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Playlist | `value` | Full Spotify playlist link, playlist URI or playlist ID. Variables are expanded. |

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Get playlist songs

- Kind: sub-action
- Code: 99865
- Type: SpotifyGetPlaylistTracks
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Reads the first 50 songs of a playlist. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Playlist | `value` | Full Spotify playlist link, playlist URI or playlist ID. Variables are expanded. |

### Arguments

- `spotifyResult`: Object containing up to 50 playlist items.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Rename playlist

- Kind: sub-action
- Code: 99866
- Type: SpotifyRenamePlaylist
- Category: Integrations/Spotify/Playlists
- Needs: Spotify API login

Gives a playlist a new name. Fires Playlist changed. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Playlist | `value` | Full Spotify playlist link, playlist URI or playlist ID. Variables are expanded. |
| New name | `extra` | Required new playlist name. Variables are expanded. |

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Choose a playlist and enter its new name.
