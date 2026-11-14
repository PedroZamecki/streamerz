(() => {
  const C = __STREAMERZ__;
  const topics = new Set(), events = new Set(), handlers = Object.create(null), last = Object.create(null);
  let source, timer;
  const fire = (key, value, name) => (handlers[key] || []).forEach(f => {
    try { f(value, name); } catch (e) { console.error(e); }
  });
  const connect = () => { clearTimeout(timer); timer = setTimeout(open, 0); };
  // The live feed: one WebSocket to Streamerz for everything this page listens to. (Not server-sent
  // events: a browser keeps only six of those open to one address, and OBS has more sources than that.)
  function open() {
    if (source) {
      source.onclose = null;
      source.close();
    }
    const q = new URLSearchParams({
      topics: [...topics].join(","), events: [...events].join(","), group: C.group, token: C.token,
      page: decodeURIComponent(location.pathname),
    });
    const ws = (source = new WebSocket((C.server || location.origin).replace(/^http/, "ws") + "/overlays/events?" + q));
    // Streamerz closed or restarted: try again in a moment (a timer only while it is away).
    ws.onclose = () => {
      if (source === ws) (clearTimeout(timer), (timer = setTimeout(open, 2000)));
    };
    source.onmessage = m => {
      const d = JSON.parse(m.data), e = d.event;
      if (e.source === "Overlay") {
        last[e.type] = d.data;
        fire(e.type, d.data, !!d.replay);
      } else if (e.source === "Test") {
        // Test on the Overlays page: made-up topics and events, handed to whatever this page listens to.
        // While it runs streamerz.testing is true, so a page that keeps what it counts can leave it out.
        streamerz.testing = true;
        try {
          for (const [topic, value] of Object.entries(d.data.topics || {})) fire(topic, value);
          for (const [name, data] of d.data.events || []) {
            fire("event:" + name, data, name);
            fire("event:" + name.split(".")[0] + ".*", data, name);
          }
        } finally {
          streamerz.testing = false;
        }
      } else if (e.source === "Reload") {
        // A file of this overlay was saved (in an editor, by an assistant): the page loads again.
        location.reload();
      } else if (e.source === "Settings") {
        Object.keys(streamerz.settings).forEach(k => delete streamerz.settings[k]);
        Object.assign(streamerz.settings, d.data);
        fire("event:Settings." + e.type, streamerz.settings);
      } else {
        const name = e.source + "." + e.type;
        fire("event:" + name, d.data, name);
        fire("event:" + e.source + ".*", d.data, name);
      }
    };
  }
  // Ask Streamerz to do something. Resolves with its answer, rejects with why it didn't.
  const call = (op, data) =>
    fetch(C.server + "/overlays/call", {
      method: "POST",
      body: JSON.stringify({ ...data, op, group: C.group, token: C.token }),
    })
      .then(r => r.json())
      .then(r => {
        if (!r.ok) throw new Error(r.error);
        return r;
      });
  // The page's own errors go to Streamerz's log (OBS has no console to look at): what went wrong and
  // the line of the file it is on. At most twenty, and each only once.
  const lines = (document.currentScript ? document.currentScript.textContent : "").split("\n").length - 1;
  const told = new Set();
  const report = text => {
    if (told.size >= 20 || told.has(text)) return;
    told.add(text);
    call("log", { text, error: true }).catch(() => {});
  };
  addEventListener("error", e => {
    // This script sits above the page's first line: its lines are taken off, to give the file's.
    const here = e.filename === location.href.split("#")[0];
    const at = e.lineno ? " (" + (here ? "line " + (e.lineno - lines) : e.filename.split("/").pop() + ":" + e.lineno) + ")" : "";
    report((e.message || "error") + at);
  });
  addEventListener("unhandledrejection", e => report(String((e.reason && e.reason.message) || e.reason)));
  // A picture Streamerz has (see streamerz.asset), and an address of its own made whole.
  const asset = path => C.server + "/overlays/-/" + path;
  const whole = url => (typeof url === "string" && url[0] === "/" ? C.server + url : url);
  const make = (tag, cls, text) => {
    const el = document.createElement(tag);
    if (cls) el.className = cls;
    if (text != null) el.textContent = text;
    return el;
  };
  const picture = (cls, src, alt) => {
    const img = make("img", cls);
    img.src = src;
    img.alt = alt || "";
    return img;
  };
  const avatars = new Map();
  const streamerz = {
    settings: C.settings,
    group: C.group,
    // True while the page handles what Test sent (made up): a page that keeps a count skips keeping it.
    testing: false,
    state: last,
    // The platforms' names and colours, for a border, a tile or a dot.
    platforms: {
      twitch: { name: "Twitch", color: "#9146ff" },
      youtube: { name: "YouTube", color: "#ff0033" },
      kick: { name: "Kick", color: "#53fc18" },
      tiktok: { name: "TikTok", color: "#fe2c55" },
    },
    // Pictures Streamerz has, by address (for an <img> or a CSS url()):
    // streamerz.logo("twitch"): a platform's or an app's logo (twitch, youtube, kick, tiktok, spotify...).
    // streamerz.icon("heart", "#ff5c8a"): a Lucide icon (lucide.dev) in that colour, white without one.
    // streamerz.asset("badges/kick-vip.svg"): any of them by its path.
    asset,
    logo: name => asset("brands/" + name + ".svg"),
    icon: (name, color) => asset("icons/" + (color ? color.replace("#", "") + "/" : "") + name + ".svg"),
    // streamerz.words(line): the message of a chat line as nodes, its emotes as pictures (class
    // sz-emote, and sz-big for one drawn big). What a chatter typed is put in as text, never as HTML.
    words(line) {
      const out = document.createDocumentFragment();
      for (const part of line.parts || [{ t: line.text || "" }]) {
        if (part.u) out.append(picture("sz-emote" + (part.big ? " sz-big" : ""), part.u, part.e));
        else out.append(part.t || "");
      }
      return out;
    },
    // streamerz.avatar(line).then(url => ...): the picture of who wrote a line or caused an event ("" when
    // there is none). YouTube and TikTok send it along; Twitch and Kick are asked once per person.
    avatar(line) {
      if (line.avatar) return Promise.resolve(line.avatar);
      const key = line.platform + ":" + line.userId;
      if (!line.userId || String(line.userId).startsWith("test-")) return Promise.resolve("");
      if (!avatars.has(key))
        avatars.set(key, call("avatar", { platform: line.platform, userId: line.userId }).then(r => r.url || "", () => ""));
      return avatars.get(key);
    },
    // streamerz.line(line, {platform, time, avatar, badges}): a chat line as an element, drawn as the
    // Chat page draws it, for the page's CSS to dress (see the Chat overlay example):
    //   .sz-line (and .sz-twitch..., .sz-event, .sz-action, .sz-deleted), with data-msg and data-user
    //     .sz-reply      who and what it answers
    //     .sz-what       an event's line: .sz-icon, .sz-name, the words, .sz-cost
    //     .sz-head       .sz-platform, .sz-time, .sz-avatar, .sz-badge..., .sz-name
    //     .sz-text       the message, with .sz-emote pictures
    //     .sz-picture    the picture that came with it (a reward, a gift, a GIF)
    // Every option is on unless set to false, except time and avatar, which are off unless true.
    line(line, options = {}) {
      const el = make("div", "sz-line sz-" + line.platform);
      el.dataset.msg = line.msgId || "";
      el.dataset.user = line.platform + ":" + (line.userId || "");
      el.style.setProperty("--sz-platform", (streamerz.platforms[line.platform] || {}).color || "#a1a1aa");
      el.style.setProperty("--sz-accent", line.accent || line.color || "#a1a1aa");
      if (line.action) el.classList.add("sz-action");
      if (line.deleted) el.classList.add("sz-deleted");
      if (line.reply) el.append(make("div", "sz-reply", "@" + line.reply.user + ": " + line.reply.text));
      const name = () => {
        const who = make("span", "sz-name", line.user);
        who.style.color = line.color || "";
        return who;
      };
      const head = make("span", "sz-head");
      if (options.platform !== false) head.append(picture("sz-platform", streamerz.logo(line.platform), line.platform));
      if (options.time && line.time) head.append(make("span", "sz-time", line.time));
      if (options.avatar) {
        const face = picture("sz-avatar", "");
        face.hidden = true;
        streamerz.avatar(line).then(url => {
          if (url) (face.src = url), (face.hidden = false);
        });
        head.append(face);
      }
      if (line.event) {
        el.classList.add("sz-event");
        const what = make("div", "sz-what");
        what.append(head);
        if (line.icon) what.append(picture("sz-icon", streamerz.icon(line.icon, line.accent)));
        what.append(name(), " " + line.event);
        if (line.cost) what.append(make("span", "sz-cost", String(line.cost)));
        el.append(what);
      } else {
        if (options.badges !== false) {
          for (const b of line.badges || []) {
            const src = b.url || (b.art ? asset("badges/" + b.art + ".svg") : streamerz.icon(b.icon || "award"));
            const badge = picture("sz-badge", src, b.label);
            badge.title = b.label || "";
            head.append(badge);
          }
        }
        head.append(name());
        el.append(head);
      }
      if (line.text || (line.parts || []).length) {
        const text = make("span", "sz-text");
        text.append(streamerz.words(line));
        el.append(text);
      }
      const shown = line.gif || line.picture;
      if (shown) el.append(picture("sz-picture" + (line.huge ? " sz-big" : ""), shown));
      return el;
    },
    // streamerz.onSettings(s => ...): the group's settings now, and again whenever one changes in
    // Streamerz (no reload of the page). streamerz.settings always holds the current ones.
    onSettings(fn) {
      fn(streamerz.settings);
      return streamerz.event("Settings." + C.group, fn);
    },
    // streamerz.chat(line => ..., {history: 50, removed: r => ...}): every chat message of every
    // connected platform, the moment it arrives, with the newest `history` ones first. A line is what the
    // Chat page shows: {platform, user, userId, color (the name's, already readable), badges: [{label,
    // url, art, icon}], text, parts: [{t: text} or {e: name, u: picture, big}] (the emotes of every
    // provider already found), time, msgId, avatar, reply: {user, text}, broadcaster, moderator, vip,
    // subscriber, action, gif, picture, tag}. streamerz.line(line) draws one. `removed` gets {platform,
    // msgId, userId} when a moderator deletes a message or a user's messages.
    chat(fn, options = {}) {
      if (options.history)
        call("chat", { count: options.history, alerts: false }).then(r => r.lines.forEach(l => fn(l)), console.error);
      if (options.removed) streamerz.event("Chat.Removed", options.removed);
      return streamerz.event("Chat.Line", fn);
    },
    // streamerz.alerts(a => ..., {history: 10}): what happens in the channels (a follow, a sub, a
    // gift, a raid, a cheer, a reward), on every platform: a chat line with {event (what happened, in
    // words), icon (a Lucide name: star, gift, swords, user-plus, gem...), accent (its colour), cost,
    // picture (a reward's, a gift's), text (what they wrote with it)}. To count with, a live one also
    // has kind (follow, sub, resub, giftsub, giftbomb, cheer, raid, superchat, member, gift, reward...),
    // trigger (the trigger's name) and args (its arguments: bits, viewers, coins, repeatCount...).
    alerts(fn, options = {}) {
      if (options.history)
        call("chat", { count: options.history, lines: false }).then(r => r.lines.forEach(l => fn(l)), console.error);
      return streamerz.event("Chat.Alert", fn);
    },
    // streamerz.song(s => ...): the song on the Spotify app of this computer, at every change (a new
    // song, a pause, a seek): {title, artist, album, cover (a picture's address), playing, durationMs,
    // positionMs, at}. It is the topic "song", which an action can also set with more (who asked for
    // it). streamerz.position(s) is where the song is now, in ms: count on with it, nothing is sent
    // while a song just plays.
    song: fn => streamerz.on("song", s => fn(s && s.cover ? { ...s, cover: whole(s.cover) } : s)),
    position(song) {
      if (!song) return 0;
      const played = song.playing && song.at ? Date.now() - song.at : 0;
      const now = (song.positionMs || 0) + Math.max(0, played);
      return song.durationMs ? Math.min(now, song.durationMs) : now;
    },
    // streamerz.countdown(id, c => ...): a countdown's {running, remainingMs, deadlineMs, maxMs} now
    // and at every change. Count down from deadlineMs yourself: Streamerz sends no ticks.
    countdown(id, fn) {
      call("countdown", { id }).then(r => r.snapshot && fn(r.snapshot), console.error);
      return streamerz.event("Misc.StreamerzCountdownChanged", d => d.clockId === id && d.snapshot && fn(d.snapshot));
    },
    // streamerz.run("Spin", {user: "ana"}): queue an action of this overlay's group.
    run: (action, args) => call("run", { action, args }),
    // streamerz.actions().then(list => ...): the actions connected to this overlay, [{id, name, enabled}],
    // for a page that offers them as buttons. Needs the same permission as run.
    actions: () => call("actions").then(r => r.actions),
    // streamerz.say("hi", {platform: "twitch", bot: true}): send to chat (every platform by default).
    say: (text, options) => call("say", { text, ...options }),
    // streamerz.set("wins", 3) and streamerz.get("wins"): values of the group, kept across restarts.
    set: (name, value) => call("set", { name, value }),
    get: name => call("get", { name }).then(r => r.value),
    // streamerz.publish("score", {home: 1}): set a topic for every page listening with on().
    publish: (topic, value) => call("publish", { topic, value }),
    // streamerz.log("text"): a line in Streamerz's log (OBS has no console to look at).
    log: text => call("log", { text: String(text) }),
    // streamerz.on("viewers", (v, old) => ...): a topic. Gets the current value first, then every
    // change. `old` is true for the value from before the page opened: a page that acts on a change
    // (spin the wheel) skips that one, a page that shows a state (a score) uses it.
    on(topic, fn) {
      (handlers[topic] ||= []).push(fn);
      topics.add(topic);
      if (topic in last) fn(last[topic], true);
      connect();
      return streamerz;
    },
    // streamerz.event("Spotify.TrackChanged", data => ...): any Streamerz event ("Twitch.*" for all of one source).
    event(name, fn) {
      (handlers["event:" + name] ||= []).push(fn);
      events.add(name);
      connect();
      return streamerz;
    },
  };
  window.streamerz = streamerz;
})();
