# Integrations / Spotify (triggers)

## Connected

- Kind: trigger
- Code: 99804
- Type: SpotifyConnected
- Category: Integrations/Spotify
- Needs: Spotify login

Fires after a Spotify API login succeeds.

### When

Fires after a Spotify API login succeeds. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

No event-specific values. These events report the account connection, not song playback.

## Disconnected

- Kind: trigger
- Code: 99805
- Type: SpotifyDisconnected
- Category: Integrations/Spotify
- Needs: Spotify login

Fires when the Spotify API account is disconnected in Integrations.

### When

Fires when the Spotify API account is disconnected in Integrations. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

No event-specific values. These events report the account connection, not song playback.

## Error

- Kind: trigger
- Code: 99806
- Type: SpotifyError
- Category: Integrations/Spotify
- Needs: a Spotify operation

Fires when a Spotify operation or local monitor reports an error.

### When

Fires when a Spotify operation or local monitor reports an error. The `spotifyError` argument describes it.
This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyError`: readable failure reason.

Use `%spotifyError%` in a log or notification. Reconnect in Integrations when the login has expired.

## Local session connected

- Kind: trigger
- Code: 99807
- Type: SpotifyLocalConnected
- Category: Integrations/Spotify
- Needs: Windows local Spotify monitoring

Fires when the local Spotify media session appears.

### When

Fires when the local Spotify media session appears. The `spotifyTitle`, `spotifyArtist` and `spotifyPlaying` arguments describe the session. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyTitle`: local media title.
- `spotifyArtist`: local media artist.
- `spotifyPlaying`: whether the local session is playing, as a boolean.

These values are present when a local track is known. Local monitoring uses the Windows media session; it does not require an API login. For example, use `%spotifyArtist% - %spotifyTitle%` in an overlay.

## Local session disconnected

- Kind: trigger
- Code: 99808
- Type: SpotifyLocalDisconnected
- Category: Integrations/Spotify
- Needs: Windows local Spotify monitoring

Fires when the local Spotify media session disappears.

### When

Fires when the local Spotify media session disappears. The `spotifyTitle`, `spotifyArtist` and `spotifyPlaying` arguments describe the session. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyTitle`: local media title.
- `spotifyArtist`: local media artist.
- `spotifyPlaying`: whether the local session is playing, as a boolean.

These values are present when a local track is known. Local monitoring uses the Windows media session; it does not require an API login. For example, use `%spotifyArtist% - %spotifyTitle%` in an overlay.

## Track changed

- Kind: trigger
- Code: 99809
- Type: SpotifyTrackChanged
- Category: Integrations/Spotify
- Needs: Windows local Spotify monitoring

Fires when the local title or artist changes.

### When

Fires when the local title or artist changes. The `spotifyTitle`, `spotifyArtist` and `spotifyPlaying` arguments describe the session. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyTitle`: local media title.
- `spotifyArtist`: local media artist.
- `spotifyPlaying`: whether the local session is playing, as a boolean.

These values are present when a local track is known. Local monitoring uses the Windows media session; it does not require an API login. For example, use `%spotifyArtist% - %spotifyTitle%` in an overlay.

## Playback started

- Kind: trigger
- Code: 99825
- Type: SpotifyPlaybackStarted
- Category: Integrations/Spotify
- Needs: Windows local Spotify monitoring

Fires when the local session starts playing.

### When

Fires when the local session starts playing. The `spotifyTitle`, `spotifyArtist` and `spotifyPlaying` arguments describe the session. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyTitle`: local media title.
- `spotifyArtist`: local media artist.
- `spotifyPlaying`: whether the local session is playing, as a boolean.

These values are present when a local track is known. Local monitoring uses the Windows media session; it does not require an API login. For example, use `%spotifyArtist% - %spotifyTitle%` in an overlay.

## Playback paused

- Kind: trigger
- Code: 99826
- Type: SpotifyPlaybackPaused
- Category: Integrations/Spotify
- Needs: Windows local Spotify monitoring

Fires when the local session stops or pauses.

### When

Fires when the local session stops or pauses. The `spotifyTitle`, `spotifyArtist` and `spotifyPlaying` arguments describe the session. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyTitle`: local media title.
- `spotifyArtist`: local media artist.
- `spotifyPlaying`: whether the local session is playing, as a boolean.

These values are present when a local track is known. Local monitoring uses the Windows media session; it does not require an API login. For example, use `%spotifyArtist% - %spotifyTitle%` in an overlay.

## Song requested

- Kind: trigger
- Code: 99830
- Type: SpotifySongRequested
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

A viewer's song request was queued.

### When

A viewer's song request was queued. Sets `spotifyRequester`, `spotifyTitle`, `spotifyArtist`, `spotifyUri` and `spotifyPosition`. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyRequester`: viewer who ran the operation.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when known.
- `spotifyDurationMs`: duration passed with the song. Request-operation events currently pass zero, so do not use this to measure song length.
- `spotifyPosition`: request position, starting at one, when known.

Song values can be absent, for example when a search finds nothing. Values from the originating action are not automatically copied to this trigger.

## Song request refused

- Kind: trigger
- Code: 99831
- Type: SpotifySongRequestRefused
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

A song request was refused.

### When

A song request was refused. `spotifyReason` is notfound, blocked, explicit, unavailable, toolong, duplicate, limit, notallowed or error. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyRequester`: viewer who ran the operation.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when known.
- `spotifyDurationMs`: duration passed with the song. Request-operation events currently pass zero, so do not use this to measure song length.
- `spotifyReason`: refusal code listed above.

Song values can be absent, for example when a search finds nothing. Values from the originating action are not automatically copied to this trigger.

## Song request taken back

- Kind: trigger
- Code: 99832
- Type: SpotifySongRequestTakenBack
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

A request was taken back.

### When

A request was taken back. Sets `spotifyRequester` and the song. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyRequester`: viewer who ran the operation.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when known.
- `spotifyDurationMs`: duration passed with the song. Request-operation events currently pass zero, so do not use this to measure song length.

Song values can be absent, for example when a search finds nothing. Values from the originating action are not automatically copied to this trigger.

## Song skipped

- Kind: trigger
- Code: 99833
- Type: SpotifySongSkipped
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

The song was skipped by a command or a vote (`spotifyReason` is command or vote).

### When

The song was skipped by a command or a vote (`spotifyReason` is command or vote). This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyRequester`: viewer who ran the operation.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when known.
- `spotifyDurationMs`: duration passed with the song. Request-operation events currently pass zero, so do not use this to measure song length.
- `spotifyReason`: `command` or `vote`.

Song values can be absent, for example when a search finds nothing. Values from the originating action are not automatically copied to this trigger.

## Vote to skip

- Kind: trigger
- Code: 99834
- Type: SpotifyVoteSkip
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

A viewer voted to skip.

### When

A viewer voted to skip. Sets `spotifyRequester`. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyRequester`: viewer who ran the operation.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when known.
- `spotifyDurationMs`: duration passed with the song. Request-operation events currently pass zero, so do not use this to measure song length.

Song values can be absent, for example when a search finds nothing. Values from the originating action are not automatically copied to this trigger.

## Song blocked

- Kind: trigger
- Code: 99835
- Type: SpotifySongBlocked
- Category: Integrations/Spotify/Song Requests
- Needs: Spotify API login

A song was blocked with Block Song or `!bansong`.

### When

A song was blocked with Block Song or `!bansong`. Sets the song. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyRequester`: viewer who ran the operation.
- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: affected song when known.
- `spotifyDurationMs`: duration passed with the song. Request-operation events currently pass zero, so do not use this to measure song length.

Song values can be absent, for example when a search finds nothing. Values from the originating action are not automatically copied to this trigger.

## Volume changed

- Kind: trigger
- Code: 99836
- Type: SpotifyVolumeChanged
- Category: Integrations/Spotify/Playback
- Needs: Spotify API login

The volume of the player changed.

### When

The volume of the player changed. `spotifyValue` is the new volume and `spotifyPrevious` the old one. Volume, shuffle, repeat and device changes are noticed when Streamerz reads or changes the player, not by polling. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyValue`: new volume from 0 to 100, as text.
- `spotifyPrevious`: previously observed value, as text.

The first observed value establishes the baseline and does not fire this event. A later read or change must see a different value; no background poll is added.

## Shuffle changed

- Kind: trigger
- Code: 99837
- Type: SpotifyShuffleChanged
- Category: Integrations/Spotify/Playback
- Needs: Spotify API login

Shuffle was turned on or off.

### When

Shuffle was turned on or off. `spotifyValue` is true or false. Volume, shuffle, repeat and device changes are noticed when Streamerz reads or changes the player, not by polling. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyValue`: new shuffle state (`true` or `false`), as text.
- `spotifyPrevious`: previously observed value, as text.

The first observed value establishes the baseline and does not fire this event. A later read or change must see a different value; no background poll is added.

## Repeat changed

- Kind: trigger
- Code: 99838
- Type: SpotifyRepeatChanged
- Category: Integrations/Spotify/Playback
- Needs: Spotify API login

Repeat changed to off, track or context.

### When

Repeat changed to off, track or context. `spotifyValue` is the new mode. Volume, shuffle, repeat and device changes are noticed when Streamerz reads or changes the player, not by polling. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyValue`: new repeat mode (`off`, `track` or `context`), as text.
- `spotifyPrevious`: previously observed value, as text.

The first observed value establishes the baseline and does not fire this event. A later read or change must see a different value; no background poll is added.

## Device changed

- Kind: trigger
- Code: 99839
- Type: SpotifyDeviceChanged
- Category: Integrations/Spotify/Playback
- Needs: Spotify API login

The device that plays changed.

### When

The device that plays changed. `spotifyValue` is its name. Volume, shuffle, repeat and device changes are noticed when Streamerz reads or changes the player, not by polling. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyValue`: new device name, as text.
- `spotifyPrevious`: previously observed value, as text.

The first observed value establishes the baseline and does not fire this event. A later read or change must see a different value; no background poll is added.

## Login expired

- Kind: trigger
- Code: 99840
- Type: SpotifyLoginExpired
- Category: Integrations/Spotify/Connection
- Needs: Spotify API login

Spotify refused to renew the login (it expired or was revoked), so it has to be connected again.

### When

Spotify refused to renew the login (it expired or was revoked), so it has to be connected again. `spotifyError` has the reason. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyError`: readable failure reason.

Use `%spotifyError%` in a log or notification. Reconnect in Integrations when the login has expired.

## Song liked

- Kind: trigger
- Code: 99841
- Type: SpotifySongLiked
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

A song was saved to the liked songs by Streamerz.

### When

A song was saved to the liked songs by Streamerz. `spotifyTitle`, `spotifyArtist` and `spotifyUri` describe it. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyTitle`, `spotifyArtist`, `spotifyUri`: song saved by Streamerz, when known.

Liking a song elsewhere does not fire this trigger.

## Playlist changed

- Kind: trigger
- Code: 99842
- Type: SpotifyPlaylistChanged
- Category: Integrations/Spotify/Library
- Needs: Spotify API login

A playlist was created, renamed or had a song added or removed by a Streamerz sub-action.

### When

A playlist was created, renamed or had a song added or removed by a Streamerz sub-action. `spotifyPlaylist` is what was given. This is a Streamerz extension.

### Fields

No settings. Enable the trigger on the action that should run.

### Arguments

- `spotifyPlaylist`: playlist link, URI, ID or creation name supplied to the operation.

This is the input value, not always a playlist ID. Edits made elsewhere do not fire this trigger.
