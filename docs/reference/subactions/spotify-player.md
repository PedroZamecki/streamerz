# Integrations / Spotify / Player (sub-actions)

## Get current track

- Kind: sub-action
- Code: 99810
- Type: SpotifyGetCurrent
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Reads the current Spotify track. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Track details as an object, or null when nothing is playing.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Get devices

- Kind: sub-action
- Code: 99811
- Type: SpotifyGetDevices
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Lists available Spotify devices. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Object containing the available devices, their IDs and active state.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Get queue

- Kind: sub-action
- Code: 99812
- Type: SpotifyGetQueue
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Reads the Spotify playback queue. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Current track and queued tracks as an object.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Play

- Kind: sub-action
- Code: 99813
- Type: SpotifyPlay
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Resumes Spotify playback. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Pause

- Kind: sub-action
- Code: 99814
- Type: SpotifyPause
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Pauses Spotify playback. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Next track

- Kind: sub-action
- Code: 99815
- Type: SpotifyNext
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Skips to the next Spotify track. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Previous track

- Kind: sub-action
- Code: 99816
- Type: SpotifyPrevious
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Goes back to the previous Spotify track. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Seek

- Kind: sub-action
- Code: 99817
- Type: SpotifySeek
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Moves to a position, in milliseconds, in the current track. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Position | `value` | Position from the start, in milliseconds. For example, `30000` seeks to 30 seconds. Variables such as `%group.position%` are expanded. |

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Enter `30000` to jump to 0:30.

## Set volume

- Kind: sub-action
- Code: 99818
- Type: SpotifySetVolume
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Sets the Spotify volume, 0 to 100. Fires Volume changed when it differs from the last one seen. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Volume | `value` | Whole number from 0 to 100. For example, `25` sets 25 percent. Variables are expanded. |

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Enter `25` to lower the player to 25 percent.

## Add to queue

- Kind: sub-action
- Code: 99819
- Type: SpotifyAddToQueue
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Adds a track, found by link or words, to the Spotify queue. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Song | `value` | Full Spotify track link, track URI or words to search. A search uses the best matching track. Variables are expanded. |

### Arguments

- `spotifyResult`: URI of the track added to the queue.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Use `%rawInput%` after a chat command to search for the viewer's song.

## Transfer playback

- Kind: sub-action
- Code: 99820
- Type: SpotifyTransferPlayback
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Moves playback to the device with the given ID. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Device ID | `value` | ID from Get devices. Use its ID, not the visible device name. Variables are expanded. |

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Run Get devices, inspect `spotifyResult`, then enter the chosen device's ID here.

## Set shuffle

- Kind: sub-action
- Code: 99821
- Type: SpotifySetShuffle
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Turns shuffle on (`true`) or off (`false`). Fires Shuffle changed when it differs. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Shuffle | `value` | `true` to turn shuffle on or `false` to turn it off. Variables are expanded. |

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Enter `false` to disable shuffle before playing a playlist.

## Set repeat

- Kind: sub-action
- Code: 99822
- Type: SpotifySetRepeat
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Sets repeat to `off`, `track` or `context`. Fires Repeat changed when it differs. This is a Streamerz extension.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Repeat | `value` | `off` disables repeat, `track` repeats the song, `context` repeats the album or playlist. Variables are expanded. |

### Arguments

- `spotifyResult`: Null on success when Spotify returns no response body.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

### Example

Enter `track` to repeat the current song.

## Get playback state

- Kind: sub-action
- Code: 99823
- Type: SpotifyGetPlayback
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Reads what the player does. Also sets `spotifyPlaying`, `spotifyVolume`, `spotifyShuffle`, `spotifyRepeat` and `spotifyDevice`, and fires the change triggers for them. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Playback state, including the device and current item, when Spotify provides it.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.
- `spotifyPlaying`, `spotifyVolume`, `spotifyShuffle`, `spotifyRepeat`, `spotifyDevice`: the state Spotify reported, as text.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.

## Get recently played

- Kind: sub-action
- Code: 99824
- Type: SpotifyGetRecentlyPlayed
- Category: Integrations/Spotify/Player
- Needs: Spotify API login

Reads the last songs played. This is a Streamerz extension.

### Fields

No settings. Uses the connected Spotify account.

### Arguments

- `spotifyResult`: Object containing up to 50 recently played items.
- `spotifyText`: up to ten readable names joined into one line when the result contains names.
- `spotifyCount`: number of result items, which may exceed the names shown in the summary.
- `spotifyError`: empty on success, or a readable failure reason. On failure the result is null, text is empty and count is zero.

Reading state does not start a background poll. If Spotify requires permissions added since your login, reconnect the account in Integrations.
