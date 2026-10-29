# Built-in chat commands

Added once on first run, in the Built-in group: edit, turn off or re-alias them like any command
(deleted ones stay deleted). They work on every platform, answering where they were typed.

- `!commands`: the commands that work here and that the asker may use.
- `!command add !name text`, `!command edit !name text`, `!command remove !name`,
  `!command platforms !name twitch,kick,youtube,tiktok` (or `all`), `!command show !name` (moderators).
- `!so user` (moderators): "Go check out user at …" with that platform's link; on Twitch also Twitch's
  own shoutout.
- `!uptime`: how long this platform's stream has been live (not on TikTok).
- `!title`: this platform's stream title; moderators set it with `!title new title`.
- **Queues**: your queues and Default, with what's running and waiting in each; add, edit, delete,
  Blocking, pause/resume (one or all) and clear. **Pending** lists the waiting runs.
- **History**: the last 100 runs (actions set to Exclude from History are left out), with their
  variables when queued and after the run; **Run again** queues one with the same arguments.
- **Globals**: every global and user global, persisted or not; add, edit and delete globals (these fire
  the same triggers as Global (Set)). User globals are shown only.
- **Groups**: add and delete user groups (their Added/Removed triggers fire). Users join them from chat.
- **Servers**: the WebSocket, HTTP and UDP servers, WebSocket clients and custom WebSocket servers.
- **Accounts**: one row per platform (Twitch, YouTube, Kick, TikTok): connected or not (and why), the
  **Channel** to read and moderate (empty: your own; a URL or @name works, so a moderator can use their
  own account on someone else's channel, with nothing of their own to stream), **Connect**,
  **Connect on start**, and **Login** for your account and an optional bot, with their pictures. Chat is
  sent as the bot when one is logged in. **Advanced…** has the rest:
  - Kick: sending needs your own Kick app (kick.com > Settings > Developer > Create app, redirect URL
    `http://127.0.0.1:45563/callback`, every scope; the bot account is only asked for chat). Paste its client ID and secret, then Login. Reading
    chat needs no login.
  - YouTube: the video ID (empty: the channel's live) and your Google OAuth client, needed to send:
    console.cloud.google.com > New project > enable **YouTube Data API v3** > OAuth consent screen
    (External, then **Publish app**; unpublished apps log you out every 7 days) > Credentials > Create
    OAuth client ID, type **Desktop app**. Paste its ID and secret, then Login. Google warns that the app
    is unverified: it's yours, so continue. Reading chat needs no login.
  - TikTok: Streamerz reads the channel's live itself, with no login (it waits while the
    channel is offline). Login opens TikTok's own login page in your browser (Chrome, Chromium, Edge, Brave or Vivaldi,
    with a profile of Streamerz's own); sending types into the live page's chat in a hidden tab of it,
    which closes after 2 idle minutes. Nothing else to install: on Windows, Edge is always there.
- **Polls**: start and end polls on Twitch and YouTube and follow their votes; start, lock, resolve and
  cancel Twitch predictions.
- **Stream Apps**: OBS Studio and Streamlabs Desktop connections.
- **Settings**: window options and this instance's name (what GetInfo reports).
- **Log**: every line with its level (debug, info, warning, error, fatal) in colour, a level filter and a
  search.
- **Crashes**: the crash reports, to read, send as a GitHub issue, or delete.
- **Backups**: a backup of the settings and the Community data is made when a new version first starts
  (the last 10 are kept); back up now, restore (Streamerz restarts) or delete.

The window comes back as it was left: its size, its tab and the chat's view.

Downloads are kept in `cache/` next to the settings file: pictures (at most 50 MB; the least recently
shown go first), the channels' BTTV/FFZ/7TV emote lists and the accounts' picture addresses (used at
once, fetched again in the background after a day). Deleting the folder is safe.

The bottom bar counts the connections that are set up (platforms, OBS and Streamlabs Desktop connections,
StreamElements, Streamlabs, DonorDrive), e.g. "● 3/5 connected". Click it for each one's state (connected,
connecting, or not and why), with **Retry** and **Settings** (its tab).

