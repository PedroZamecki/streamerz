# Overlays

An overlay is one or more `.html` pages, with the files they need beside them (pictures, sounds, fonts) and settings anyone can change without touching the HTML. Streamerz serves it on the HTTP server's port and sends it live data. Nothing is hard coded: you write the pages (or start from an example), and they travel in one `.streamerz` file when you share them.

An overlay needs no actions. When it does (a poll started by `!poll`, a wheel that spins on `!spin`, a dock of buttons), a group of actions of the same name is **connected** to it: see "Overlays and actions".

## Make one

1. Turn the HTTP server on (Servers). The default port is 7474.
2. Open **Overlays** and press **New overlay**. It makes an overlay with a starting page, `index.html`,
   one setting, and `streamerz.d.ts` (the types of `window.streamerz`, for an editor to offer names as
   you type). **Open folder** shows the files: edit them with any editor, and the open page loads again
   when you save.
3. Select the page and press **Add to OBS** (it makes the browser source in the scene on air), or
   **Copy URL** and paste it in a browser source yourself:
   `http://127.0.0.1:7474/overlays/<overlay>/<file>`
4. `http://127.0.0.1:7474/overlays` lists every overlay.

**Page and permissions** sets a page's name, the size Add to OBS gives it, whether it is made for a dock,
and what the overlay's pages may ask Streamerz to do. For a dock page **Add to OBS** shows the steps in
OBS (Docks, Custom browser docks) with the address already copied.

**Test** sends made-up data to the open pages of the selected overlay, and to nothing else: no chat, no
action and no other overlay sees it. Each press sends the next ones: a chat message from another
platform, another kind of event (a sub, a Super Chat, a gift, a raid, a follow, a cheer), a TikTok gift
whose combo grows, and a song with a cover. They are built as the real ones are, so a page that works
with Test works live. A template adds its own in `template.test` (see "Share an overlay").

## Overlays and actions

An overlay and a group of actions are two things. The Overlays page lists overlays; the Actions page
lists groups of actions. When one of each has the same name they are **connected**: the overlay's pages
can run the group's actions (`streamerz.run`, with the permission `actions.run`), the actions read the
overlay's settings as `%group.name%`, and they are shared together in one file.

- On the Overlays page the column **Connected actions** shows a chain and what the group holds.
  **Connect to actions** gives the overlay a new, empty group of its name, or takes a group you already
  have (its actions, commands, timers and settings move under the overlay's name, so the page's address
  stays the same). **Disconnect** takes the group out of Actions: its actions stay there without a group,
  and the overlay keeps its pages and settings.
- On the Actions page a connected group has a chain at its right. Deleting it says that an overlay is
  connected, and only the group goes: the overlay stays on the Overlays page.
- Deleting an overlay's last page deletes the overlay; actions connected to it are never deleted.

Of the examples, Subathon, Top gifters, Poll, Prize wheel and Buttons dock come with actions; the others are overlays alone.

## Work on a page

**Open folder** shows the overlay's files. Edit them with any editor: when a file is saved, the open
pages of that overlay load again by themselves, in OBS and in a browser, so there is nothing to refresh.
The folder is watched by the system only while one of its pages is open.

A mistake in the page's script shows in **Log** as `Overlay <name>: <what went wrong> (line N)`, with
the line of your file (OBS has no console to look at). `streamerz.log(text)` writes a line there too.

**Inspect** shows what the overlay is doing now: its open pages and what each listens to, what they
logged and their errors, and the last messages sent to them. It is the first place to look when a
page shows nothing.

A file you put in the folder yourself (a picture, a sound, a second page) is the overlay's at once: it
is served, and it is in the list of files (and in what you share) from the next save or **Refresh**.

**Guide** (the book, above the list) opens this guide from Streamerz itself, at
`/overlays/-/guide`, so it is the one for the version you run.

## Examples

**Examples** (top of the Overlays page) lists overlays that come with Streamerz. **Add** puts one
here (with its actions, for the ones that have them); from there it is yours: change its settings, or press **Open folder** and edit its
page. Each one is short and commented, made to be read:

| Example | Shows | Uses |
|---|---|---|
| Clock | The time of day | settings only, the simplest one |
| Subathon | A countdown that grows with Twitch subs, community gifts and matching donations, even with its page closed | countdown actions, `streamerz.countdown`, duration, color and font settings |
| Viewer counter | The viewers of each platform and their total, counting up and down | the topic `viewers`, a `platforms` setting |
| Ad break | A badge with the time left while Twitch plays an ad | the topic `ad`, counting from an end time |
| Now playing | The song of the Spotify app on this computer: cover, title, artist, a bar that follows it. Three layouts, the accent taken from the cover | `streamerz.song`, `streamerz.position` (needs no Spotify login) |
| Chat overlay | The chat of every platform as the Chat page draws it: badges, emotes of every provider, replies, events | `streamerz.chat`, `streamerz.alerts`, `streamerz.line` |
| Flying chat | Messages fly across the screen, right to left (danmaku) | `streamerz.chat`, `streamerz.line` |
| Emote wall | Chat emotes float up the screen, capped per message and on screen; moderation removes them | `streamerz.chat`, resolved emote pictures, CSS transforms |
| Top gifters | Twitch gifted subscriptions, counted by actions with the page closed; change Period to start fresh | persistent group values, `streamerz.onSettings`, no page writes |
| Credits roll | Selected names from the stream scroll once or repeat | one HTTP read of `/GetCredits`, a `list` of sections, a browser animation |
| Song request queue | Upcoming requests from the Song requests (Spotify) action template, or another template | `streamerz.on`, the topic `songrequests.queue` or your own published array |
| Alerts | A card for each follow, sub, gift, cheer, Super Chat, raid and reward, in its platform's color, with who did it | `streamerz.alerts`, `streamerz.avatar`, `streamerz.logo` |
| TikTok gifts | Each gift on your TikTok Live, with a count that grows through a combo | `streamerz.event("TikTok.Gift")` |
| Poll | One poll for every platform: `!poll Question \| One \| Two`, viewers vote by typing a number. Comes with its commands, actions and a dock to run it from | topics set by an action, `streamerz.chat`, `streamerz.say`, a second page |
| Prize wheel | A wheel with your own slices, each with its chances. `!spin` turns it and the result goes to the chat | a topic set by an action, a `list` setting |
| Chat dock | The chat with a box to write in, for an OBS dock | `streamerz.chat`, `streamerz.say`, a dock page |
| Goal bar | A bar that fills with follows, subs, cheers or TikTok coins, and keeps its count across restarts | `streamerz.alerts` with `kind` and `args`, `streamerz.set`, `streamerz.testing` |
| Starting soon | A full screen with your picture, a countdown and music | `file` settings (a picture and a sound) |
| Buttons dock | A dock with a button for each action connected to it, and a banner the buttons show | `streamerz.actions`, `streamerz.run`, an `action` setting, two pages |

Their source is in `assets/examples/<name>`: a `template.json` and a `files` folder, exactly what a
`.streamerz` file holds when unpacked. `dev/test-examples.mjs` opens each one in a browser.

Subathon starts on installation. Run **Subathon: reset** in Actions to apply a new starting time.
The maximum is applied by the next reset or addition. Its Clock ID setting connects the page and its
actions; give independent clocks different IDs. Community gifts count their quantity once, and
donations must match the configured currency.

Top gifters counts Twitch single and community gifts once per gift. Totals are keyed by viewer ID,
so two people with the same name remain separate. Actions save them even when the page is closed.
Period selects a saved board; increase it for a fresh board, or return to an earlier number. Test
previews made-up totals on the page without storing them.

Song request queue is a display. Add **Song requests (Spotify)** in Actions and set up Spotify to
accept requests. The built-in topic `songrequests.queue` is an array of `{title, artist, user, uri}`.
Request actions, removals, clearing and the existing local media watcher update it. The current song
and requests taken back are omitted. A page opening late receives the current snapshot. Spotify API
reads occur only with Spotify enabled, pending requests and a queue listener, on a media track change;
there is no queue polling timer. If the local Spotify app is unavailable, the next request command
refreshes the known queue. Another template can use `streamerz.publish("my.queue", rows)` and set
the display's Topic to `my.queue`.

Credits roll reads `/GetCredits` once on opening. Its section rows name fields such as
`events.follows` or `users.moderators`. Reopen the page to read fresh credits. Test supplies made-up
credits to the page without changing the stream's records.

## Share an overlay

In Overlays, select a page and press **Export**. It writes one `.streamerz` file with everything the
overlay holds: its pages and files, its settings (the defaults, unless you tick the values too), and the
connected actions, commands and timers. An overlay with no actions is shared the same way. If something is worth a
look first (no description, a link to a file that is not there, a permission a page uses but the overlay
does not ask for, text that looks like a login), it says so.

The file is a zip: `template.json` (what it is), `actions.json` and `files/`. Anyone can open it and keep
the folder in git.

A setting in `template.json` (`meta.variables`) has a `name`, a `label`, a `default` and a `kind`, and the
settings form draws each kind with its own control:

| Kind | Holds | Drawn as |
| --- | --- | --- |
| `text` | text | a box |
| `lines` | text of several lines (a list, one item per line) | a tall box |
| `integer`, `number` | a number | a box |
| `slider` | a number from `min` to `max`, in steps of `step` | a slider, with its value |
| `boolean` | true or false | a check box |
| `choice` | one of `options` | a list |
| `color` | `#rrggbb`, or `#rrggbbaa` when see-through | a color picker: a palette, or hue, saturation, lightness and opacity |
| `font` | the name of a font, empty for the system's | a list of the fonts installed here, narrowed by typing |
| `duration` | milliseconds | a box, typed as `h:mm:ss` |
| `file` | one of the overlay's files, or a web address (empty: none); `accept` is `image`, `sound`, `video` or `font` | its files of that kind to pick, or one typed or found with Browse (a file of this computer is copied into the overlay) |
| `platforms` | a list of `twitch`, `youtube`, `kick`, `tiktok` | a check box with the logo of each |
| `action` | the id of one of the actions connected to the overlay (empty: none) | a list of those actions |
| `list` | rows: a list of objects with one value for each of its `columns` | a box with a row a line, the cells between bars: `Pizza \| 2` |

A `file` setting reaches a page as the address it is served at, so it goes straight into an `<img>`, an
`<audio>` or a CSS `url()`. A `list` declares its `columns`, each a setting of a simple kind (`text`,
`integer`, `number`, `slider`, `boolean`, `choice`, `color`, `duration`) whose `default` fills a cell
left out:

```json
{"name": "slices", "label": "Slices", "kind": "list",
 "columns": [{"name": "label", "label": "Slice", "kind": "text", "default": ""},
             {"name": "weight", "label": "Chances", "kind": "integer", "default": 1}],
 "default": [{"label": "100 points", "weight": 2}, {"label": "Nothing", "weight": 1}]}
```

`description` is shown when the pointer rests on the label, and `section` puts a heading above the
setting, for the ones from there on. `showIf` shows a setting only while another one (a choice or a
check box) has a value: `"showIf": {"setting": "look", "is": "Custom color"}`.

`meta.template.test` is what Test adds for this overlay: `{"topics": {"name": value}, "events":
[["Chat.Line", {...}]]}`. The Poll example starts a made-up poll and votes in it that way.

To take one in, press **Import** and give the file's path or its `https` address. Before anything
changes you read what it holds, what its pages may do, and whether it is new, updates one you have
(what you set there stays) or replaces one of the same name.

## In the page

Streamerz adds `window.streamerz` to the top of the page:

```html
<div id="n"></div>
<script>
  streamerz.on("viewers", v => n.textContent = v.total);
</script>
```

| Call | What it does |
| --- | --- |
| `streamerz.settings` | The overlay's settings (`%group.name%` in actions), so colors, texts and sizes are edited in Streamerz and not in the HTML. A `file` setting is the address to load. |
| `streamerz.on(topic, fn)` | `fn(value, old)` runs with the topic's current value, then at every change. `old` is true for the value from before the page opened: a page that acts on a change (spin the wheel) skips it. |
| `streamerz.event("Source.Type", fn)` | `fn(data)` runs for any Streamerz event, such as `Spotify.TrackChanged` or `Twitch.Follow`. `Twitch.*` is every event of a source. |
| `streamerz.onSettings(fn)` | `fn(settings)` now and whenever a setting changes in Streamerz, with no reload. |
| `streamerz.countdown(id, fn)` | A countdown's `{running, remainingMs, deadlineMs, maxMs}` now and at every change. Count from `deadlineMs` in the page: no ticks are sent. |
| `streamerz.chat(fn, {history, removed})` | Every chat message of every connected platform, the moment it arrives (also what you send from Streamerz), as the Chat page shows it: `{platform, user, userId, color, badges: [{label, url, art, icon}], text, parts: [{t}` or `{e, u, big}], time, msgId, avatar, reply, broadcaster, moderator, vip, subscriber, action, gif, picture, tag}`. The name's color is already readable and the emotes of every provider (the platform's, 7TV, BetterTTV, FrankerFaceZ) are already found. `history` gives the newest ones first; `removed` gets `{platform, msgId, userId}` when a moderator deletes a message or a user's messages. Needs `chat.read`. |
| `streamerz.alerts(fn, {history})` | What happens in the channels, on every platform: a follow, a sub, a gift, a cheer, a Super Chat, a raid, a reward. A chat line with `{event` (what happened, in words)`, icon` (a Lucide name: `star`, `gift`, `swords`, `user-plus`, `gem`...)`, accent, cost, picture, text}`. One that happens now also has what a page counts with: `kind` (`follow`, `sub`, `resub`, `giftsub`, `giftbomb`, `cheer`, `raid`, `superchat`, `supersticker`, `member`, `gift`, `share`, `reward`, `powerup`, `streak`, `announcement`, `timeout`, `ban`), `trigger` (the trigger's name) and `args` (its arguments: `bits`, `viewers`, `gifts`, `coins`, `amount`...). Needs `alerts.read`. |
| `streamerz.line(line, {platform, time, avatar, badges})` | A chat line or an alert as an element, drawn as the Chat page draws it, for the page's CSS to dress: `.sz-line` holding `.sz-reply`, `.sz-what` (an event), `.sz-head` (`.sz-platform`, `.sz-time`, `.sz-avatar`, `.sz-badge`, `.sz-name`), `.sz-text` (with `.sz-emote`) and `.sz-picture`. What a chatter typed is put in as text, never as HTML. |
| `streamerz.words(line)` | Only the message, as nodes, with its emotes as pictures. |
| `streamerz.avatar(line)` | A Promise of the picture of who wrote a line or caused an event (`""` when there is none). YouTube and TikTok send it along; Twitch and Kick are asked once per person. |
| `streamerz.song(fn)` | The song of the Spotify app on this computer, at every change (a new song, a pause, a seek): `{title, artist, album, cover, playing, durationMs, positionMs, at}`. It is the topic `song`. The player is watched only while a page listens, with no login and no timer. |
| `streamerz.position(song)` | Where the song is now, in milliseconds: count on with it, nothing is sent while a song just plays. |
| `streamerz.logo(name)`, `streamerz.icon(name, color)`, `streamerz.asset(path)` | The address of a picture Streamerz has: a platform's logo, a Lucide icon in a color, or any of them by path (see "Pictures"). |
| `streamerz.platforms` | Each platform's `{name, color}`. |
| `streamerz.run(action, args)` | Queues an action connected to the overlay, by name or id. Needs `actions.run`. |
| `streamerz.actions()` | A Promise of the actions connected to the overlay, `[{id, name, enabled}]`, for a page that offers them as buttons. Needs `actions.run`. |
| `streamerz.say(text, {platform, bot})` | Sends to chat, on every connected platform by default. Needs `chat.send`. |
| `streamerz.set(name, value)`, `streamerz.get(name)` | A value of the overlay, kept across restarts and read by actions as `%group.name%`. Writing needs `values.write`. |
| `streamerz.publish(topic, value)` | Sets a topic for every page listening with `on()`. |
| `streamerz.log(text)` | A line in Streamerz's log (OBS has no console to look at). |
| `streamerz.testing` | True while the page handles what **Test** sent: a page that keeps a count (the Goal bar) shows it but does not keep it. |

The calls that ask Streamerz for something return a Promise, rejected with the reason when refused.

```html
<div id="clock"></div>
<script>
  let c;
  streamerz.countdown("the-countdown-id", state => c = state);
  setInterval(() => {
    if (!c) return;
    const ms = c.running ? Math.max(0, c.deadlineMs - Date.now()) : c.remainingMs;
    clock.textContent = new Date(ms).toISOString().slice(11, 19);
  }, 250);
</script>
```

The page connects only if it asks for something, with one WebSocket on the same port
(`/overlays/events`), and connects again by itself when Streamerz restarts. A WebSocket and not
server-sent events, because a browser keeps only six of those open to one address: with more pages
than that in OBS, the rest would wait forever. A plain `GET /overlays/events` still answers as
server-sent events, for tools.

## What a page may do

A page reads what is on screen anyway (viewers, ads, topics, its own settings) with no permission.
Everything else is listed in its `permissions` (**Page and permissions**, or MCP `overlay_save`), and a
page of another overlay, or one Streamerz did not serve, is refused:

| Permission | Lets its pages |
| --- | --- |
| `chat.read` | Read the chat of your channels |
| `alerts.read` | See follows, subscriptions, gifts, donations and raids |
| `chat.send` | Send chat messages as you |
| `actions.run` | Run the actions connected to it |
| `values.write` | Store values of its own |

## Files

An overlay's files live in the `groups` folder beside `streamerz.json`, not inside it. A name may have
folders (`img/wheel.png`), so a page links its styles, scripts, pictures, sounds and videos with
relative addresses. Large files are sent in pieces and a video can be sought.

## Pictures

A page may use the pictures Streamerz is drawn with, at `/overlays/-/...`:

| Address | Is |
| --- | --- |
| `brands/<name>.svg` | a platform's or an app's logo: `twitch`, `youtube`, `kick`, `tiktok`, `spotify`... |
| `badges/<platform>-<role>.svg` | a chat badge the platform gives no picture for |
| `icons/<name>.svg` | a [Lucide](https://lucide.dev) icon in white; `icons/<rrggbb>/<name>.svg` in that color |
| `cover` | the cover of the song, where the player hands it over as a picture (Windows) |
| `guide` | this guide, as a page |
| `streamerz.d.ts` | the types of `window.streamerz`, for an editor (a new overlay gets a copy in its folder) |

## Topics

| Topic | Value |
| --- | --- |
| `viewers` | `{twitch, youtube, kick, tiktok, total}`. A platform that is not connected is `null`. The counts are read once a minute while a page listens. |
| `ad` | While Twitch plays an ad: `{running: true, duration, endsAt, isAutomatic, startedAt}`; when it ends `{running: false}`. `endsAt` is in milliseconds. |
| `song` | The song of the Spotify app on this computer (see `streamerz.song`), or `{playing: false}` when it is closed. |
| any name | Set by the **Overlay** sub-action (topic and value), a page (`streamerz.publish`) or the MCP tool `overlay_publish`. |

To show a game score or a goal, make an action that runs on the right trigger and ends with an Overlay
sub-action. To make a page do something once (start a poll, spin a wheel), set a topic from an action
and let the page act when it changes: the Poll and Prize wheel examples do that from a chat command.

## For assistants (MCP)

An assistant builds, tries and shares an overlay with no window at all:

1. `overlay_examples` lists the examples (each a good starting point); with `add` it installs one, and
   `overlay_read` with `file` gives the text of any of its files.
2. `overlay_save` makes an overlay or changes one in one call: its files as text (`files`) or base64
   (`filesBase64`), its `settings`, `values`, `permissions`, `pages`, `test`, and `connect` to give it
   a group of actions. Its answer has the pages' addresses and `checks`, what to fix before sharing.
3. `overlay_shot` opens a page in a browser of this computer with no window, sends it `test` rounds of
   made-up data, and gives a PNG with what its console said and its script's errors (with the line of
   the file). `overlay_test` sends made-up data (or exactly the events and topics given) to the pages
   already open; `overlay_inspect` says what the open pages listen to, got and logged.
4. `overlay_add_to_obs` makes the browser source in OBS; `overlay_connect` connects a group of actions
   (or disconnects it); `overlay_delete` deletes a file or the whole overlay.
5. Actions are built with `describe`, `new_action` and `save` (their `group` is the overlay's name), and
   tried with `fire_trigger` and `inject_chat`.
6. `template_check`, `template_export`, `template_inspect` and `template_import` share it as a
   `.streamerz` file. `overlay_publish` sets a topic.

The same is the WebSocket request `StreamerzOverlay` with an `op` (`list`, `read`, `save`, `delete`,
`test`, `inspect`, `obs`, `connect`, `disconnect`, `examples`, `add_example`, `shot`). The Overlays page
is `core_view` and `core_action` with the domain `overlays`, and its forms are `entity_form` kinds
`overlay:new`, `overlay:import`, `overlay:settings:<row>`, `overlay:page:<row>`,
`overlay:connect:<row>` and `overlay:export:<row>`.

## Development checks

`node dev/test-overlays.mjs` checks a scratch dev instance with WebSocket port 8095 and HTTP port 7495.
It needs Node with built-in WebSocket support and Chromium. It adds test groups and an action, so use
an isolated development configuration. It checks live topics and events, viewer totals, ad expiry,
settings, group export/import, local HTML, and refused foreign origins. Screenshots go to
`/tmp/szoverlay`.

`node dev/test-examples.mjs` adds every example to the same kind of instance, opens each in Chromium,
feeds it with Test and made-up chat, tries its settings and commands, and checks that more than six
pages open at once all stay live. With the Spotify app running it also checks the real song.
