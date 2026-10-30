# Chat

How the chat page behaves. For what each trigger and sub-action does, see the reference.

## The chat box

- One box per platform. It is off, with a plain user icon, when no account is logged in for that platform:
  the chat is then read only. "Message all" leaves such a platform out, and its lines only offer Copy.
- The hover actions follow the platform's rules and our role there (viewer, VIP, moderator, lead
  moderator, broadcaster): nobody can ban or time out the broadcaster or themselves, moderators leave
  each other to the owner, and a viewer only gets Reply and Copy.
- A message from Twitch that starts with `/me` is shown in italics, in the sender's colour.
- What you send to Kick, YouTube or TikTok shows at once and is replaced by the platform's own copy when it
  comes back (a failed send takes the line away and says why). The log says how long each send took.
- An `@name` that names one of our logged-in accounts sits on a chip, so it stands out. The chip follows
  the theme (light and dark).

## Twitch chat commands

Twitch's chat API sends text as text, so Streamerz turns a command typed in the box into the call it
stands for. Type `/` in Twitch's chat box and a list opens above it with each command, what it takes and
what it does (arrows or Tab to pick, Enter to finish it, Escape to close). A command with common values
lists them after its name: `/commercial ` offers 30, 60, 90, 120, 150 and 180. The list only shows what
your role in that channel can do (the owner's commands are hidden from a moderator), and a command that
needs more says so in the chat instead of failing with an error from Twitch.

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

A command Streamerz doesn't know isn't sent (it would show up as plain text in the chat). In dev mode
nothing is sent: the chat shows what would have been.

## At the top of a chat

- A message pinned in a chat shows as a card with who pinned it and the time left.
- A poll or prediction going on shows as a card with each choice's share (a prediction: the points on it),
  the time left, and for a short while after it ends, the result.
- A channel reward redeemed shows as a line with its price in channel points.
- A badge's tooltip says what it means; an emote's, where it comes from (Twitch, 7TV, BetterTTV...).

## When a stream stops

Each chat tells when its platform is not live: "the stream ended" (Twitch, Kick, YouTube and TikTok tell
when it does), "waiting for the stream to start" (YouTube and TikTok, which watch for it), or "not
connected". The notice sits above the chat box with Reconnect (not for an ended stream: the chat stays
connected and can still take messages) and Hide chat. A hidden chat leaves every view and its pins and
polls go with it; Options > "Show ... chat" puts it back.

## Our role

Streamerz knows what the logged-in account is in each channel as it connects: Twitch is asked, Kick says
it (moderator or owner) without any login, and for YouTube it comes from our own earlier messages in the
chat's history, and from the next one we send.

## Connections that drop

Each platform's reader watches for silence and reconnects by itself:

- YouTube: a read that fails is retried on the same page, 1, 2, 4, 8 then 15 seconds apart. After eight
  failures in a row the stream is looked up again, and the chat page is opened anew if it is still live.
- Kick: after a silent minute the server is pinged; with no answer in 20 seconds the socket is replaced.
- Twitch: the socket is replaced when no keepalive comes for its keepalive time plus 20 seconds.
- Reads are never given a longer pause than 15 seconds between tries, and an outage is logged once.

YouTube finds the live stream of a channel by itself. The channel can be an `@handle` or a channel id
(`UC...`); a video id is only needed to read a stream that isn't the channel's current live.
