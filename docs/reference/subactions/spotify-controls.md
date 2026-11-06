# Integrations / Spotify / Player controls (sub-actions)

## Play song or playlist

- Kind: sub-action
- Code: 99840
- Type: SpotifyPlayFrom
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Plays a song, an album, a playlist or an artist. The value is a Spotify link, a URI, or words to search a song for. Starts the device that was used last when none is active. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Song or playlist | `value` | Full Spotify track, album, artist or playlist link or URI. Words search for a track. Variables are expanded. |

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Paste a full playlist link to start that playlist.

## Toggle playback

- Kind: sub-action
- Code: 99841
- Type: SpotifyTogglePlayback
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Pauses when it plays and plays when it is paused. The result is true when it now plays. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: `true` or `false` as text, indicating whether playback is now running.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Restart track

- Kind: sub-action
- Code: 99842
- Type: SpotifyRestartTrack
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Goes back to the start of the song that plays. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Change volume

- Kind: sub-action
- Code: 99843
- Type: SpotifyChangeVolume
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Raises or lowers the volume by an amount, for example `10` or `-10`, between 0 and 100. The result is the new volume. Fires Volume changed when it changed. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Volume change | `value` | Signed whole number. `10` raises the volume by 10 percentage points; `-10` lowers it. The result stays between 0 and 100. |

### Arguments

- `spotifyResult`: The resulting volume, from 0 to 100.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Enter `-10` to reduce volume by 10 percentage points.

## Skip forward or back

- Kind: sub-action
- Code: 99844
- Type: SpotifySeekRelative
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Moves inside the song by some milliseconds, negative to go back. The result is the new position. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Position change | `value` | Signed whole number of milliseconds. `10000` moves forward 10 seconds; `-10000` moves back. The result cannot go below zero. Variables are expanded. |

### Arguments

- `spotifyResult`: The resulting position in milliseconds.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Enter `-10000` to move back 10 seconds.
