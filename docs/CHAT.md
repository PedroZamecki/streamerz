# Chat

How the chat page behaves. For what each trigger and sub-action does, see the reference.

## The chat box

- One box per platform. It is off, with a plain user icon, when no account is logged in for that platform:
  the chat is then read only. TikTok uses the browser account described below instead. "Message all"
  leaves a platform without sending set up out, and its lines only offer Copy.
- The hover actions follow the platform's rules and our role there (viewer, VIP, moderator, lead
  moderator, broadcaster): nobody can ban or time out the broadcaster or themselves, moderators leave
  each other to the owner, and a viewer only gets Reply and Copy.
- A message from Twitch that starts with `/me` is shown in italics, in the sender's colour.
- What you send to Kick, YouTube or TikTok shows at once and is replaced by the platform's own copy when it
  comes back (a failed send takes the line away and says why). The log says how long each send took.
- An `@name` that names one of our logged-in accounts sits on a chip, so it stands out. The chip follows
  the theme (light and dark).

## TikTok account and search

In Accounts > TikTok > Browser account, choose Set up. Open your Chromium browser's extensions page,
turn on Developer mode, choose Load unpacked, and select the folder shown by Streamerz. When updating
an existing installation, reload the extension after Streamerz writes its new files.

Log in opens TikTok in that browser. This is the only TikTok login Streamerz uses: the browser account
supplies search, authenticated reading and sending. Old separate account and bot logins are no longer
used. An old account's own channel is preserved when loading its settings; its saved cookies are not
carried into the new settings. Ordinary public LIVE reading still works without the extension.

The browser account picture appears in Accounts, the composer and its single-account popup. Channel
pictures use the same cache, with signed search when the extension is ready and a public lookup otherwise.

The channel picker searches TikTok through the extension, with names, pictures, follower counts and
live state. A pasted link or @handle still works when search is unavailable. Use my own channel selects
the browser account. A new setup with no chosen channel uses that account's channel automatically.

The extension loads TikTok inside a hidden browser document, with pictures, video, fonts, telemetry
and feed refreshes blocked. No helper tab or window opens. Browser background mode lets it work after
all browser windows close; explicitly quitting the browser disconnects it. Log in may require a visible
browser window, but ordinary use never requires navigating to a helper page. A discarded context is
recreated on the next request. A send with a lost reply is never retried automatically.

Accounts distinguishes an extension connection, a loading context, a ready account and a required
login. Sending stays disabled until the browser account is ready. Session changes refresh the identity
and reconnect an active reader with the new session. Reader cookies stay in memory and are never saved.
Closing Streamerz or removing the integration releases the hidden context. Remove the extension from
the browser too if you no longer use it.

To measure usage while other tabs are open, use the browser's Task Manager. Include Streamerz for TikTok
and its hidden TikTok frame. Shared browser, network and GPU costs are separate. Developer tools and an
open Task Manager can affect CPU readings; allow the context to settle before measuring it.

TikTok replies add `@handle` to the message, using the chatter's handle rather than their display name.
The same handle is used in mention suggestions and profile links. Message all includes TikTok and uses
its browser account regardless of the account chosen for other platforms.

Unicode emoji can be sent. Native TikTok stickers are shown when received, with a preview instead of
internal IDs in their tooltip, but their outgoing encoding is not implemented. The picker hides these
unsendable entries and a pasted sticker token produces an explanation before anything is posted.

TikTok also follows incoming message and user-history deletions, pinned comments and explicit pin
cancellations. Pause and resume updates appear in chat without disconnecting it. The 150-character
limit includes a reply's @mention; an oversized draft stays in the composer so it can be shortened.
See [TikTok capabilities](TIKTOK.md) for the supported paths and verification limits.

## Twitch chat commands

Twitch's chat API sends text as text, so Streamerz turns a command typed in the box into the call it
stands for. Type `/` in Twitch's chat box and a list opens above it with each command, what it takes and
what it does (arrows or Tab to pick, Enter to finish it, Escape to close). A command with common values
lists them after its name: `/commercial ` offers 30, 60, 90, 120, 150 and 180. Streamerz does not guess
what your role may do: every command is listed, it is tried, and the chat shows Twitch's own answer if
it refuses. The one thing known beforehand is that a channel that is not a Twitch affiliate or partner
cannot run commercials (read from the channel when connecting), so `/commercial` is not offered there.
Twitch documents `/commercial` as the broadcaster's only (Helix, "Start Commercial"); twitch.tv's own
site may let others do it, but through a call Streamerz's login cannot make.

| Command | Does | Needs |
| --- | --- | --- |
| `/commercial 30` | Runs a commercial of 30, 60, 90, 120, 150 or 180 seconds. | the owner |
| `/clear` | Clears the chat. | a moderator |
| `/delete ID` | Deletes one message. | a moderator |
| `/timeout NAME [length] [reason]` | Times out; the length is `90`, `10m`, `2h`, `1d`, `1w` (10 minutes when left out). | a moderator |
| `/ban NAME [reason]`, `/unban NAME` | Bans or lifts a ban or timeout. | a moderator |
| `/slow [seconds]`, `/slowoff` | Slow mode, 3 to 120 seconds (30 when left out). | a moderator |
| `/followers [length]`, `/followersoff` | Followers-only; a bare number is minutes. | a moderator |
| `/subscribers`, `/emoteonly`, `/uniquechat` (and the same with `off`) | Turns a chat mode on or off. | a moderator |
| `/raid NAME`, `/unraid` | Starts or cancels a raid. | the owner |
| `/shoutout NAME` | Shouts a channel out. | a moderator |
| `/mod NAME`, `/unmod NAME`, `/vip NAME`, `/unvip NAME` | Changes who moderates or is a VIP. | the owner |
| `/announce TEXT` (`blue`, `green`, `orange`, `purple` after it for a color) | Posts an announcement. | a moderator |
| `/marker [description]` | Adds a stream marker. | the owner |
| `/me TEXT` | An action message: sent as is. | anyone |

Kick's box lists the commands Kick's API can do (from its help page "KICK chat commands"): `/ban`,
`/unban`, `/timeout <user> [seconds]` and `/title`. The others (slow mode, followers-only, `/clear`...) have
no call in Kick's API, so they are not offered.

A command Streamerz doesn't know isn't sent (it would show up as plain text in the chat). In dev mode
nothing is sent: the chat shows what would have been.

## At the top of a chat

- A message pinned in a chat shows as a card with who pinned it and the time left.
- A poll or prediction going on shows as a card with each choice's share (a prediction: the points on it),
  the time left, and for a short while after it ends, the result.
- Rewards and power-ups (see "Rewards, power-ups and gifts" below).
- A badge's tooltip says what it means; an emote's, where it comes from (Twitch, 7TV, BetterTTV...).

## Rewards, power-ups and gifts

Each shows in the chat as TikTok's gifts do: an icon, who, what, a picture when there is one, and the price.

| What | Shown as | Needs |
| --- | --- | --- |
| A custom reward with words (Twitch) | A header over the message: the reward's picture, name and price in the channel's own points picture | Nothing: the chat message says which reward it came with (the name, price and picture come from Twitch's public web API) |
| A custom reward with no words (Twitch) | A line with the same | A login with the redemptions scope (the owner's; Twitch decides if a moderator's may). Without one nothing tells of it: Twitch's public socket refuses the redemptions topic |
| An automatic reward (Highlight My Message, unlock an emote, Message Effects, Gigantify an Emote, On-Screen Celebration) | A line with its name, price and for an emote its picture (gigantified: big) | Same. The message of a highlight, a sub-only bypass, an effect or a gigantified emote is also marked, with or without a login |
| A Bits power-up, custom ones too | A line with what was used and the Bits | A login with the `bits:read` scope (Twitch decides who may) |
| A watch streak (Twitch) | A line with the streak | Nothing |
| A gigantified message | The last emote drawn big | Nothing |
| A reward (Kick) | A line with the reward's name, price and colour | Nothing: kick.com's own reward list gives the price |
| A gift (TikTok) | A line with its picture and coins | Nothing |

Twitch decides who gets which event: a moderator's login may be refused some of them, and Streamerz says so
once in the log.

## When a stream stops

Each chat tells when its platform is not live: "the stream ended" (Twitch, Kick, YouTube and TikTok tell
when it does), "waiting for the stream to start" (YouTube and TikTok, which watch for it), or "not
connected". The notice sits above the chat box with Reconnect (not for an ended stream: the chat stays
connected and can still take messages) and Hide chat. A hidden chat leaves every view and its pins and
polls go with it; Options > "Show ... chat" puts it back.

## Our role

Streamerz asks for what the logged-in account is in each channel as it connects: Twitch's list of the
channel's moderators, Kick's own user endpoint (moderator or owner, no login needed); for YouTube it comes
from our own earlier messages in the chat's history, and from the next one we send. Nothing a role "may"
do is assumed beyond what the platforms document: calls are made and the platform's answer is shown. The
events Twitch won't give an account (a moderator asking for the owner's) are refused by Twitch and told
once in the log.

## Connections that drop

Each platform's reader watches for silence and reconnects by itself:

- YouTube: a read that fails is retried on the same page, 1, 2, 4, 8 then 15 seconds apart. After eight
  failures in a row the stream is looked up again, and the chat page is opened anew if it is still live.
- Kick: after a silent minute the server is pinged; with no answer in 20 seconds the socket is replaced.
- Twitch: the socket is replaced when no keepalive comes for its keepalive time plus 20 seconds.
- Reads are never given a longer pause than 15 seconds between tries, and an outage is logged once.

YouTube finds the live stream of a channel by itself. The channel can be an `@handle` or a channel id
(`UC...`); a video id is only needed to read a stream that isn't the channel's current live.
