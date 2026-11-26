(() => {
  const C = __STREAMERZ__;
  // Opened by the overlay editor as a preview (`?preview` in the address): it shows the settings being
  // changed before they are saved, and listens for them even when the page asks for nothing else.
  const preview = new URLSearchParams(location.search).has("preview");
  const topics = new Set(), events = new Set(), handlers = Object.create(null), last = Object.create(null);
  const controls = C.controls && typeof C.controls === "object" ? C.controls : (C.controls = {});
  const BASE_DELAY = 2000, MAX_DELAY = 30000;
  let source, timer, delay = BASE_DELAY;
  const safeDecode = s => { try { return decodeURIComponent(s); } catch { return s; } };
  const fire = (key, value, name) => (handlers[key] || []).forEach(f => {
    try { f(value, name); } catch (e) { console.error(e); }
  });
  const connect = () => { clearTimeout(timer); timer = setTimeout(open, 0); };
  // The live feed: one WebSocket to Streamerz for everything this page listens to. (Not server-sent
  // events: a browser keeps only six of those open to one address, and OBS has more sources than that.)
  function open() {
    if (source) {
      source.onclose = null;
      source.onmessage = null;
      source.close();
    }
    // Saved settings reach every page that listens at all: its Extra CSS follows them.
    const listened = new Set([...events, "Settings." + C.group]);
    const q = new URLSearchParams({
      topics: [...topics].join(","), events: [...listened].join(","), group: C.group, token: C.token,
      page: safeDecode(location.pathname),
    });
    if (preview) q.set("preview", "1");
    const ws = (source = new WebSocket((C.server || location.origin).replace(/^http/, "ws") + "/overlays/events?" + q));
    ws.onopen = () => { if (source === ws) delay = BASE_DELAY; };
    // Streamerz closed or restarted: back off while it is away, so idle sources stay quiet in OBS.
    ws.onclose = () => {
      if (source !== ws) return;
      clearTimeout(timer);
      const wait = delay;
      delay = Math.min(delay * 2, MAX_DELAY);
      timer = setTimeout(open, wait);
    };
    ws.onmessage = m => {
      if (source !== ws) return;
      let d;
      try { d = JSON.parse(m.data); } catch { return; }
      const e = d && d.event;
      if (!e) return;
      if (e.source === "Overlay") {
        last[e.type] = d.data;
        fire(e.type, d.data, !!d.replay);
      } else if (e.source === "Test") {
        // Test on the Overlays page: made-up topics and events, handed to whatever this page listens to.
        // While it runs streamerz.testing is true, so a page that keeps what it counts can leave it out.
        streamerz.testing = true;
        try {
          for (const [topic, value] of Object.entries((d.data && d.data.topics) || {})) fire(topic, value);
          for (const [name, data] of (d.data && d.data.events) || []) {
            fire("event:" + name, data, name);
            fire("event:" + name.split(".")[0] + ".*", data, name);
          }
        } finally {
          streamerz.testing = false;
        }
      } else if (e.source === "Control") {
        // A live control was set (the Live tab, a dock, an action, a page).
        const { name, value, old } = d.data || {};
        if (name !== undefined && value !== undefined) controls[name] = value;
        fire("control:" + name, value, old);
      } else if (e.source === "Sound") {
        // Play Sound In Overlay: the page plays it itself, so OBS mixes it with this source.
        const a = new Audio(whole(d.data.url));
        a.volume = Math.max(0, Math.min(1, +d.data.volume || 0));
        sounds.add(a);
        a.onended = a.onerror = () => sounds.delete(a);
        a.szName = d.data.name || "";
        a.play().catch(() => sounds.delete(a));
      } else if (e.source === "SoundStop") {
        for (const a of sounds) if (!d.data.name || a.szName.toLowerCase() === d.data.name.toLowerCase()) (a.pause(), sounds.delete(a));
      } else if (e.source === "Play") {
        // Play on a settings form: the animation of that setting again, where streamerz.animate used it.
        replay(d.data.name);
      } else if (e.source === "Reload") {
        // A file of this overlay was saved (in an editor, by an assistant): the page loads again.
        location.reload();
      } else if (e.source === "Draft") {
        // The editor changed a setting (not saved yet): a page that follows its settings shows it at
        // once, any other page loads again with it.
        const follows = (handlers["event:Settings." + C.group] || []).length;
        if (!follows && !onlyCss(d.data)) return location.reload();
        Object.keys(streamerz.settings).forEach(k => delete streamerz.settings[k]);
        Object.assign(streamerz.settings, d.data);
        extraCss();
        fire("event:Settings." + C.group, streamerz.settings);
      } else if (e.source === "Settings") {
        // A preview page shows the draft: the saved settings come back through Draft.
        if (preview) return;
        Object.keys(streamerz.settings).forEach(k => delete streamerz.settings[k]);
        Object.assign(streamerz.settings, d.data);
        extraCss();
        fire("event:Settings." + e.type, streamerz.settings);
      } else {
        const name = e.source + "." + e.type;
        fire("event:" + name, d.data, name);
        fire("event:" + e.source + ".*", d.data, name);
      }
    };
  }
  // Extra CSS (a setting every overlay has): put after the page's own styles, so it wins.
  let extraStyle;
  const extraCss = () => {
    const css = (C.settings && C.settings.extraCss) || "";
    if (!extraStyle) {
      if (!css) return;
      extraStyle = document.createElement("style");
      extraStyle.id = "sz-extra-css";
    }
    extraStyle.textContent = css;
    (document.head || document.documentElement).append(extraStyle);
  };
  // A draft that changed only the Extra CSS needs no reload, whatever the page does with settings.
  const onlyCss = data =>
    Object.keys({ ...data, ...C.settings }).every(k => k === "extraCss" || JSON.stringify(data[k]) === JSON.stringify(C.settings[k]));
  if (document.readyState === "loading") addEventListener("DOMContentLoaded", extraCss);
  else extraCss();
  // The animations of streamerz.animate: keyframes over whatever transform the element has.
  const moves = Object.assign(Object.create(null), {
    fade: [{ opacity: 0 }, { opacity: 1 }],
    "slide-up": [{ opacity: 0, t: "translateY(40px)" }, { opacity: 1, t: "none" }],
    "slide-down": [{ opacity: 0, t: "translateY(-40px)" }, { opacity: 1, t: "none" }],
    "slide-left": [{ opacity: 0, t: "translateX(40px)" }, { opacity: 1, t: "none" }],
    "slide-right": [{ opacity: 0, t: "translateX(-40px)" }, { opacity: 1, t: "none" }],
    zoom: [{ opacity: 0, t: "scale(0.5)" }, { opacity: 1, t: "none" }],
    pop: [{ opacity: 0, t: "scale(0.3)" }, { opacity: 1, t: "scale(1.1)", offset: 0.7 }, { t: "none" }],
    bounce: [{ t: "translateY(-60px)", opacity: 0 }, { t: "none", opacity: 1, offset: 0.5 }, { t: "translateY(-15px)", offset: 0.75 }, { t: "none" }],
    flip: [{ opacity: 0, t: "perspective(600px) rotateY(90deg)" }, { opacity: 1, t: "none" }],
    shake: [{ t: "none" }, { t: "translateX(-10px)" }, { t: "translateX(10px)" }, { t: "translateX(-10px)" }, { t: "translateX(10px)" }, { t: "none" }],
  });
  const played = Object.create(null);
  const running = new WeakMap();
  // Sounds an action plays on this page (Play Sound In Overlay), until they end or are stopped.
  const sounds = new Set();
  const prune = set => { for (const el of set) if (!el.isConnected) set.delete(el); };
  const replay = name => {
    const set = played[name];
    if (!set) return;
    prune(set);
    for (const el of set) streamerz.animate(el, streamerz.settings[name]);
  };
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
    const line = here && e.lineno > lines ? e.lineno - lines : e.lineno;
    const at = e.lineno ? " (" + (here || !e.filename ? "line " + line : e.filename.split("/").pop() + ":" + e.lineno) + ")" : "";
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
    if (src) img.src = src;
    img.alt = alt || "";
    return img;
  };
  const avatars = new Map();
  const MAX_AVATARS = 500;
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
      if (!avatars.has(key)) {
        if (avatars.size >= MAX_AVATARS) avatars.delete(avatars.keys().next().value);
        const pending = call("avatar", { platform: line.platform, userId: line.userId }).then(
          r => r.url || "",
          () => {
            if (avatars.get(key) === pending) avatars.delete(key);
            return "";
          },
        );
        avatars.set(key, pending);
      }
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
    // streamerz.place(el, s.where): puts an element where a `position` setting says ({anchor, x, y}:
    // one of nine places of the page, and pixels from it). The element is placed absolutely.
    place(el, where) {
      const w = where || {}, a = String(w.anchor || "center");
      const h = a.endsWith("left") ? "left" : a.endsWith("right") ? "right" : "";
      const v = a.startsWith("top") ? "top" : a.startsWith("bottom") ? "bottom" : "";
      // Pinned to its sides (centered with auto margins), so it keeps the width of its content.
      Object.assign(el.style, {
        position: "absolute", width: "fit-content", height: "fit-content",
        left: h === "right" ? "auto" : "0", right: h === "left" ? "auto" : "0",
        top: v === "bottom" ? "auto" : "0", bottom: v === "top" ? "auto" : "0",
        marginLeft: h ? "0" : "auto", marginRight: h ? "0" : "auto",
        marginTop: v ? "0" : "auto", marginBottom: v ? "0" : "auto",
        transform: `translate(${+w.x || 0}px, ${+w.y || 0}px)`,
      });
      return el;
    },
    // streamerz.animate(el, s.enter, {out: true}): plays an `animation` setting ({name, durationMs,
    // delayMs, easing}) on an element, backwards with out (to leave). Resolves when it ends. Given a
    // setting itself, Play on its form plays it again on this element.
    animate(el, anim, options = {}) {
      const a = anim || {}, steps = moves[a.name];
      const name = Object.keys(streamerz.settings).find(k => streamerz.settings[k] === anim);
      if (name) {
        const set = (played[name] ||= new Set());
        prune(set);
        set.add(el);
        // Play comes through the page's feed: open it if the page listens to nothing else.
        if (!source) connect();
      }
      const prev = running.get(el);
      if (prev) {
        if (prev.cancel) prev.cancel();
        running.delete(el);
      }
      if (!steps || !el.animate) return Promise.resolve();
      const base = el.style.transform && el.style.transform !== "none" ? el.style.transform + " " : "";
      const frames = steps.map(({ t, ...f }) => (t ? { ...f, transform: base + (t === "none" ? "" : t) || "none" } : f));
      const duration = a.durationMs == null ? 400 : Math.max(0, +a.durationMs || 0);
      const run = el.animate(frames, {
        duration, delay: Math.max(0, +a.delayMs || 0),
        easing: a.easing || "ease-out", fill: options.out ? "forwards" : "backwards", direction: options.out ? "reverse" : "normal",
      });
      running.set(el, run);
      return run.finished.then(
        () => { if (!options.out && running.get(el) === run) running.delete(el); },
        () => { if (running.get(el) === run) running.delete(el); },
      );
    },
    // streamerz.css(s.title): a `textStyle` setting as CSS for an element's style:
    // el.style.cssText += streamerz.css(s.title).
    css(style) {
      const s = style || {}, out = [], clean = v => String(v).replace(/[;\r\n]/g, "");
      if (s.font) out.push(`font-family: "${String(s.font).replace(/["\\\r\n;]/g, "")}"`);
      if (+s.size) out.push(`font-size: ${+s.size}px`);
      if (+s.weight) out.push(`font-weight: ${+s.weight}`);
      if (s.color) out.push(`color: ${clean(s.color)}`);
      if (+s.outline) out.push(`-webkit-text-stroke: ${+s.outline}px ${clean(s.outlineColor || "#000")}`, "paint-order: stroke fill");
      if (s.shadow) out.push("text-shadow: 0 2px 6px rgba(0, 0, 0, 0.6)");
      return out.join("; ") + (out.length ? ";" : "");
    },
    // streamerz.control("show", (value, old) => ...): a live control of this overlay
    // (meta.template.controls), now and at every change, wherever it is set: the Live tab, its dock,
    // an action, another page. A button gives the moment it was pressed. streamerz.control.set("show",
    // false) sets one (to keep a toggle in step with the page); streamerz.controls holds them all.
    control: Object.assign(
      (name, fn) => {
        (handlers["control:" + name] ||= []).push(fn);
        if (Object.hasOwn(controls, name)) fn(controls[name], undefined);
        if (!source) connect();
        return streamerz;
      },
      { set: (name, value) => call("control", { name, value, from: window.streamerzDock ? "dock" : "page" }) },
    ),
    controls,
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
        call("chat", { count: options.history, alerts: false }).then(
          r => (r.lines || []).forEach(l => { try { fn(l); } catch (e) { console.error(e); } }),
          console.error,
        );
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
        call("chat", { count: options.history, lines: false }).then(
          r => (r.lines || []).forEach(l => { try { fn(l); } catch (e) { console.error(e); } }),
          console.error,
        );
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
      return streamerz.event("Misc.StreamerzCountdownChanged", d => d && d.clockId === id && d.snapshot && fn(d.snapshot));
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
    // streamerz.speak("Thanks for the raid!", {voice: "pt-BR"}).then(audio => audio.play()): the text
    // spoken by a voice of this computer, as an <audio> to play in step with the page (the browser
    // source's sound). Needs voice.speak; speed and pitch around 1 as well.
    speak: (text, options = {}) =>
      call("speak", { text: String(text), ...options }).then(r => Object.assign(new Audio(whole(r.url)), { durationMs: r.durationMs })),
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
  streamerz.preview = preview;
  window.streamerz = streamerz;
  // A preview listens for the editor's draft even when the page asks for nothing else.
  if (preview) events.add("Settings." + C.group) && connect();
})();
