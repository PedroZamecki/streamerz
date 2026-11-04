# Integrations / Spotify / Player controls (sub-actions)

## Play song or playlist

- Kind: sub-action
- Code: 99840
- Type: SpotifyPlayFrom
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Plays a song, an album, a playlist or an artist. The value is a Spotify link, a URI, or words to search a song for. Starts the device that was used last when none is active. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Toggle playback

- Kind: sub-action
- Code: 99841
- Type: SpotifyTogglePlayback
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Pauses when it plays and plays when it is paused. The result is true when it now plays. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Restart track

- Kind: sub-action
- Code: 99842
- Type: SpotifyRestartTrack
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Goes back to the start of the song that plays. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Change volume

- Kind: sub-action
- Code: 99843
- Type: SpotifyChangeVolume
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Raises or lowers the volume by an amount, for example `10` or `-10`, between 0 and 100. The result is the new volume. Fires Volume changed when it changed. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.

## Skip forward or back

- Kind: sub-action
- Code: 99844
- Type: SpotifySeekRelative
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Moves inside the song by some milliseconds, negative to go back. The result is the new position. The `spotifyResult` argument holds the answer, `spotifyText` the names in it as one line, `spotifyCount` how many, and `spotifyError` the reason when it fails. Needs the Spotify login, and a login made before this needs to be connected again once for the new permissions. This is a Streamerz extension.
