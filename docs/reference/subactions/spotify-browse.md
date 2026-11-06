# Integrations / Spotify / Browse (sub-actions)

## Get profile

- Kind: sub-action
- Code: 99870
- Type: SpotifyGetProfile
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Reads the streamer's Spotify profile: name, country, plan and followers. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Profile object, including the fields Spotify makes available for the account.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Get top songs

- Kind: sub-action
- Code: 99871
- Type: SpotifyGetTopTracks
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Reads the 20 songs the streamer listens to most. The period is `short` (4 weeks), `medium` (6 months, the default) or `long` (all time). This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Period | `value` | `short` for about four weeks, `medium` for about six months or `long` for all time. Blank uses `medium`. Variables are expanded. |

### Arguments

- `spotifyResult`: Object containing up to 20 top songs for the chosen period.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Enter `short` for the recent top songs.

## Get top artists

- Kind: sub-action
- Code: 99872
- Type: SpotifyGetTopArtists
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Reads the 20 artists the streamer listens to most. The period is `short`, `medium` or `long`. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Period | `value` | `short`, `medium` or `long`. Blank uses `medium`. Variables are expanded. |

### Arguments

- `spotifyResult`: Object containing up to 20 top artists for the chosen period.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Enter `long` for top artists over all time.

## Search

- Kind: sub-action
- Code: 99873
- Type: SpotifySearch
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Searches Spotify and reads the first 10 results. The type is track (the default), artist, album or playlist. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Search | `value` | Words to search, such as an artist and song title. Variables are expanded. |
| Type | `extra` | `track`, `artist`, `album` or `playlist`. Blank searches tracks. Variables are expanded. |

### Arguments

- `spotifyResult`: Object containing up to 10 results of the chosen type.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Search `jazz` with Type `playlist`.

## Get current artist

- Kind: sub-action
- Code: 99874
- Type: SpotifyGetCurrentArtist
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Reads the artist of the song that plays: genres, followers and popularity. This is a Streamerz extension.

### Fields

No settings. Uses the current song on the Spotify account.

### Arguments

- `spotifyResult`: Details of the current song's first artist.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Get current album

- Kind: sub-action
- Code: 99875
- Type: SpotifyGetCurrentAlbum
- Category: Integrations/Spotify/Browse
- Needs: Spotify API login

Reads the album of the song that plays: its songs, year and label. This is a Streamerz extension.

### Fields

No settings. Uses the current song on the Spotify account.

### Arguments

- `spotifyResult`: Details of the current song's album.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.
