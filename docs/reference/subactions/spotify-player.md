# Integrations / Spotify / Player (sub-actions)

## Get current track

- Kind: sub-action
- Code: 99810
- Type: SpotifyGetCurrent
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Reads the current Spotify track. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Get devices

- Kind: sub-action
- Code: 99811
- Type: SpotifyGetDevices
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Lists available Spotify devices. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Get queue

- Kind: sub-action
- Code: 99812
- Type: SpotifyGetQueue
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Reads the Spotify playback queue. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Play

- Kind: sub-action
- Code: 99813
- Type: SpotifyPlay
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Resumes Spotify playback. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Pause

- Kind: sub-action
- Code: 99814
- Type: SpotifyPause
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Pauses Spotify playback. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Next track

- Kind: sub-action
- Code: 99815
- Type: SpotifyNext
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Skips to the next Spotify track. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Previous track

- Kind: sub-action
- Code: 99816
- Type: SpotifyPrevious
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Goes back to the previous Spotify track. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Seek

- Kind: sub-action
- Code: 99817
- Type: SpotifySeek
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Moves to a position, in milliseconds, in the current track. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Set volume

- Kind: sub-action
- Code: 99818
- Type: SpotifySetVolume
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Sets the Spotify volume, 0 to 100. Fires Volume changed when it differs from the last one seen. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Add to queue

- Kind: sub-action
- Code: 99819
- Type: SpotifyAddToQueue
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Adds a track, found by link or words, to the Spotify queue. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Transfer playback

- Kind: sub-action
- Code: 99820
- Type: SpotifyTransferPlayback
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Moves playback to the device with the given ID. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Set shuffle

- Kind: sub-action
- Code: 99821
- Type: SpotifySetShuffle
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Turns shuffle on (`true`) or off (`false`). Fires Shuffle changed when it differs. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Set repeat

- Kind: sub-action
- Code: 99822
- Type: SpotifySetRepeat
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Sets repeat to `off`, `track` or `context`. Fires Repeat changed when it differs. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Get playback state

- Kind: sub-action
- Code: 99823
- Type: SpotifyGetPlayback
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Reads what the player does. Also sets `spotifyPlaying`, `spotifyVolume`, `spotifyShuffle`, `spotifyRepeat` and `spotifyDevice`, and fires the change triggers for them. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.

## Get recently played

- Kind: sub-action
- Code: 99824
- Type: SpotifyGetRecentlyPlayed
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Reads the last songs played. The `spotifyResult` argument holds the API response, and `spotifyError` holds an error when the request fails. This is a Streamerz extension.
