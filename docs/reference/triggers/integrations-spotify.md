# Integrations / Spotify (triggers)

## Connected

- Kind: trigger
- Code: 99804
- Type: SpotifyConnected
- Category: Integrations/Spotify
- Needs: Spotify login

Fires after a Spotify API login succeeds. This is a Streamerz extension.

## Disconnected

- Kind: trigger
- Code: 99805
- Type: SpotifyDisconnected
- Category: Integrations/Spotify
- Needs: Spotify login

Fires when the Spotify API account is disconnected in Integrations. This is a Streamerz extension.

## Error

- Kind: trigger
- Code: 99806
- Type: SpotifyError
- Category: Integrations/Spotify
- Needs: Spotify song request action

Fires when a song request operation reports an error. The `spotifyError` argument describes it.
This is a Streamerz extension.

## Local session connected

- Kind: trigger
- Code: 99807
- Type: SpotifyLocalConnected
- Category: Integrations/Spotify
- Needs: Windows local Spotify monitoring

Fires when the local Spotify media session appears. The `spotifyTitle`, `spotifyArtist` and `spotifyPlaying` arguments describe the session. This is a Streamerz extension.

## Local session disconnected

- Kind: trigger
- Code: 99808
- Type: SpotifyLocalDisconnected
- Category: Integrations/Spotify
- Needs: Windows local Spotify monitoring

Fires when the local Spotify media session disappears. The `spotifyTitle`, `spotifyArtist` and `spotifyPlaying` arguments describe the session. This is a Streamerz extension.

## Track changed

- Kind: trigger
- Code: 99809
- Type: SpotifyTrackChanged
- Category: Integrations/Spotify
- Needs: Windows local Spotify monitoring

Fires when the local title or artist changes. The `spotifyTitle`, `spotifyArtist` and `spotifyPlaying` arguments describe the session. This is a Streamerz extension.

## Playback started

- Kind: trigger
- Code: 99825
- Type: SpotifyPlaybackStarted
- Category: Integrations/Spotify
- Needs: Windows local Spotify monitoring

Fires when the local session starts playing. The `spotifyTitle`, `spotifyArtist` and `spotifyPlaying` arguments describe the session. This is a Streamerz extension.

## Playback paused

- Kind: trigger
- Code: 99826
- Type: SpotifyPlaybackPaused
- Category: Integrations/Spotify
- Needs: Windows local Spotify monitoring

Fires when the local session stops or pauses. The `spotifyTitle`, `spotifyArtist` and `spotifyPlaying` arguments describe the session. This is a Streamerz extension.

## Song requested

- Kind: trigger
- Code: 99830
- Type: SpotifySongRequested
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

A viewer's song request was queued. Sets `spotifyRequester`, `spotifyTitle`, `spotifyArtist`, `spotifyUri` and `spotifyPosition`. This is a Streamerz extension.

## Song request refused

- Kind: trigger
- Code: 99831
- Type: SpotifySongRequestRefused
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

A song request was refused. `spotifyReason` is notfound, blocked, explicit, unavailable, toolong, duplicate, limit, notallowed or error; `spotifyError` says it in words. This is a Streamerz extension.

## Song request taken back

- Kind: trigger
- Code: 99832
- Type: SpotifySongRequestTakenBack
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

A request was taken back. Sets `spotifyRequester` and the song. This is a Streamerz extension.

## Song skipped

- Kind: trigger
- Code: 99833
- Type: SpotifySongSkipped
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

The song was skipped by a command or a vote (`spotifyReason` is command or vote). This is a Streamerz extension.

## Vote to skip

- Kind: trigger
- Code: 99834
- Type: SpotifyVoteSkip
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

A viewer voted to skip. Sets `spotifyRequester`. This is a Streamerz extension.

## Song blocked

- Kind: trigger
- Code: 99835
- Type: SpotifySongBlocked
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

A song was blocked with Block Song or `!bansong`. Sets the song. This is a Streamerz extension.

## Volume changed

- Kind: trigger
- Code: 99836
- Type: SpotifyVolumeChanged
- Category: Integrations/Spotify/Playback
- Needs: Spotify API login

The volume of the player changed. `spotifyValue` is the new volume and `spotifyPrevious` the old one. Volume, shuffle, repeat and device changes are noticed when Streamerz reads or changes the player, not by polling. This is a Streamerz extension.

## Shuffle changed

- Kind: trigger
- Code: 99837
- Type: SpotifyShuffleChanged
- Category: Integrations/Spotify/Playback
- Needs: Spotify API login

Shuffle was turned on or off. `spotifyValue` is true or false. Volume, shuffle, repeat and device changes are noticed when Streamerz reads or changes the player, not by polling. This is a Streamerz extension.

## Repeat changed

- Kind: trigger
- Code: 99838
- Type: SpotifyRepeatChanged
- Category: Integrations/Spotify/Playback
- Needs: Spotify API login

Repeat changed to off, track or context. `spotifyValue` is the new mode. Volume, shuffle, repeat and device changes are noticed when Streamerz reads or changes the player, not by polling. This is a Streamerz extension.

## Device changed

- Kind: trigger
- Code: 99839
- Type: SpotifyDeviceChanged
- Category: Integrations/Spotify/Playback
- Needs: Spotify API login

The device that plays changed. `spotifyValue` is its name. Volume, shuffle, repeat and device changes are noticed when Streamerz reads or changes the player, not by polling. This is a Streamerz extension.

## Login expired

- Kind: trigger
- Code: 99840
- Type: SpotifyLoginExpired
- Category: Integrations/Spotify/Connection
- Needs: Spotify API login

Spotify refused to renew the login (it expired or was revoked), so it has to be connected again. `spotifyError` has the reason. This is a Streamerz extension.

## Song liked

- Kind: trigger
- Code: 99841
- Type: SpotifySongLiked
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

A song was saved to the liked songs by Streamerz. `spotifyTitle`, `spotifyArtist` and `spotifyUri` describe it. This is a Streamerz extension.

## Playlist changed

- Kind: trigger
- Code: 99842
- Type: SpotifyPlaylistChanged
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

A playlist was created, renamed or had a song added or removed by a Streamerz sub-action. `spotifyPlaylist` is what was given. This is a Streamerz extension.
