// Run with node --test dev/overlays-examples.test.mjs. No browser or running app needed.
import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFileSync } from "node:fs";

function readExample(rel) {
  const html = readFileSync(new URL("../assets/examples/" + rel, import.meta.url), "utf8");
  const scripts = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)];
  return { html, script: scripts.at(-1)[1] };
}

function makeEl(tag, id = "") {
  const classes = new Set();
  const styles = Object.create(null);
  const listeners = new Map();
  const el = {
    tagName: tag.toUpperCase(),
    id,
    className: "",
    textContent: "",
    innerHTML: "",
    value: "",
    hidden: false,
    disabled: false,
    offsetWidth: 100,
    offsetHeight: 40,
    clientWidth: 400,
    clientHeight: 300,
    scrollHeight: 300,
    scrollTop: 0,
    dataset: {},
    children: [],
    parentNode: null,
    get isConnected() { return this._connected !== false; },
    get firstChild() { return this.children[0] || null; },
    get firstElementChild() { return this.children.find(c => typeof c !== "string") || null; },
    get lastElementChild() { return [...this.children].reverse().find(c => typeof c !== "string") || null; },
    style: {
      cssText: "",
      setProperty(k, v) { styles[k] = v; },
      getPropertyValue(k) { return styles[k] || ""; },
      _styles: styles,
    },
    classList: {
      add(...cs) { cs.forEach(c => classes.add(c)); },
      remove(...cs) { cs.forEach(c => classes.delete(c)); },
      contains(c) { return classes.has(c); },
      toggle(c, force) {
        if (force === undefined) return classes.has(c) ? (classes.delete(c), false) : (classes.add(c), true);
        if (force) classes.add(c); else classes.delete(c);
        return !!force;
      },
    },
    append(...nodes) {
      for (const n of nodes) {
        if (n && typeof n === "object") {
          if (n._fragment) { this.append(...n.children); continue; }
          n.parentNode = this;
          n._connected = true;
        }
        this.children.push(n);
      }
    },
    prepend(...nodes) {
      for (const n of [...nodes].reverse()) {
        if (n && typeof n === "object") { n.parentNode = this; n._connected = true; }
        this.children.unshift(n);
      }
    },
    replaceChildren(...nodes) {
      for (const c of this.children) if (c && typeof c === "object") { c.parentNode = null; c._connected = false; }
      this.children = [];
      this.append(...nodes);
    },
    remove() {
      this._connected = false;
      if (this.parentNode) {
        const idx = this.parentNode.children.indexOf(this);
        if (idx >= 0) this.parentNode.children.splice(idx, 1);
        this.parentNode = null;
      }
    },
    addEventListener(type, fn, opts) {
      const list = listeners.get(type) || [];
      list.push({ fn, once: !!opts?.once });
      listeners.set(type, list);
    },
    dispatch(type) {
      const list = [...(listeners.get(type) || [])];
      for (const item of list) {
        if (item.once) {
          const cur = listeners.get(type) || [];
          const i = cur.indexOf(item);
          if (i >= 0) cur.splice(i, 1);
        }
        item.fn({ target: this });
      }
    },
    querySelector(sel) { return this.querySelectorAll(sel)[0] || null; },
    querySelectorAll(sel) {
      const out = [];
      const match = node => {
        if (!node || typeof node !== "object") return false;
        if (sel.startsWith(".")) {
          const cls = sel.slice(1);
          return node.classList.contains(cls) || String(node.className || "").split(/\s+/).includes(cls);
        }
        return node.tagName === sel.toUpperCase();
      };
      const walk = node => {
        for (const c of node.children || []) {
          if (c && typeof c === "object") {
            if (match(c)) out.push(c);
            walk(c);
          }
        }
      };
      walk(this);
      return out;
    },
    setAttribute(k, v) { this[k] = v; },
    getAttribute(k) { return this[k] ?? null; },
    removeAttribute(k) { delete this[k]; },
  };
  return el;
}

function harness({ settings = {}, ids = [], speak, say, actions } = {}) {
  const byId = Object.create(null);
  for (const id of ids) byId[id] = makeEl("div", id);
  const timers = new Map();
  const intervals = new Map();
  const rafs = new Map();
  let nextTimer = 0, nextInterval = 0, nextRaf = 0;
  let now = 1_000_000;
  const listeners = { settings: [], chat: [], removed: [], alerts: [], topics: new Map(), events: new Map(), controls: new Map() };
  const published = [], said = [], logged = [], audios = [];

  class MockAudio {
    constructor(src) {
      this.src = src;
      this.volume = 1;
      this.muted = false;
      this.paused = false;
      this.ended = false;
      this.duration = 2;
      this._listeners = new Map();
      audios.push(this);
    }
    play() { this.played = true; return Promise.resolve(); }
    pause() { this.paused = true; }
    addEventListener(type, fn) { (this._listeners.get(type) || this._listeners.set(type, []).get(type)).push(fn); }
  }

  class MockImage {
    constructor() { this.tagName = "IMG"; this.className = ""; this.style = {}; this.classList = makeEl("img").classList; }
  }

  const streamerz = {
    settings: structuredClone(settings),
    group: "Demo",
    testing: false,
    state: Object.create(null),
    platforms: {
      twitch: { name: "Twitch", color: "#9146ff" },
      youtube: { name: "YouTube", color: "#ff0033" },
      kick: { name: "Kick", color: "#53fc18" },
      tiktok: { name: "TikTok", color: "#fe2c55" },
    },
    asset: p => "http://127.0.0.1:7474/overlays/-/" + p,
    logo: p => "http://127.0.0.1:7474/overlays/-/brands/" + p + ".svg",
    icon: n => "http://127.0.0.1:7474/overlays/-/icons/" + n + ".svg",
    css: () => "",
    place: el => el,
    animate: () => Promise.resolve(),
    words: line => { const f = { _fragment: true, children: [line.text || ""] }; return f; },
    avatar: () => Promise.resolve(""),
    line: line => {
      const el = makeEl("div");
      el.className = "sz-line sz-" + line.platform;
      el.dataset.msg = line.msgId || "";
      el.dataset.user = line.platform + ":" + (line.userId || "");
      el.textContent = (line.user || "") + ": " + (line.text || "");
      return el;
    },
    onSettings(fn) { listeners.settings.push(fn); fn(this.settings); return this; },
    chat(fn, opts = {}) {
      listeners.chat.push(fn);
      if (opts.removed) listeners.removed.push(opts.removed);
      return this;
    },
    alerts(fn) { listeners.alerts.push(fn); return this; },
    on(topic, fn) {
      const list = listeners.topics.get(topic) || [];
      list.push(fn);
      listeners.topics.set(topic, list);
      return this;
    },
    song(fn) { return this.on("song", fn); },
    position(s) {
      if (!s) return 0;
      const played = s.playing && s.at ? now - s.at : 0;
      return (s.positionMs || 0) + Math.max(0, played);
    },
    event(name, fn) {
      const list = listeners.events.get(name) || [];
      list.push(fn);
      listeners.events.set(name, list);
      return this;
    },
    control(name, fn) {
      const list = listeners.controls.get(name) || [];
      list.push(fn);
      listeners.controls.set(name, list);
      return this;
    },
    publish(topic, value) { published.push({ topic, value }); return Promise.resolve(); },
    say(text, opts) {
      said.push({ text, opts });
      return say ? say(text, opts) : Promise.resolve();
    },
    log(text) { logged.push(text); return Promise.resolve(); },
    speak(text, opts) { return speak ? speak(text, opts) : Promise.resolve(new MockAudio("/speech/1")); },
    actions() { return actions ? actions() : Promise.resolve([]); },
    set() { return Promise.resolve(); },
  };

  const document = {
    body: makeEl("body"),
    documentElement: makeEl("html"),
    getElementById: id => byId[id] || null,
    createElement: tag => makeEl(tag),
    createElementNS: (_, tag) => makeEl(tag),
  };

  const window = {};
  const globals = {
    window,
    document,
    streamerz,
    Audio: MockAudio,
    Image: MockImage,
    URL,
    location: { href: "http://127.0.0.1:7474/overlays/Demo/index.html" },
    innerWidth: 1280,
    innerHeight: 720,
    Date: Object.assign(function (...a) { return new Date(...a); }, { now: () => now }),
    performance: { now: () => now },
    getComputedStyle: () => ({ fontSize: "24px" }),
    setTimeout(fn, ms) { const id = ++nextTimer; timers.set(id, { fn, ms, at: now + (ms || 0) }); return id; },
    clearTimeout(id) { timers.delete(id); },
    setInterval(fn, ms) { const id = ++nextInterval; intervals.set(id, { fn, ms }); return id; },
    clearInterval(id) { intervals.delete(id); },
    requestAnimationFrame(fn) { const id = ++nextRaf; rafs.set(id, fn); return id; },
    cancelAnimationFrame(id) { rafs.delete(id); },
    addEventListener() {},
    console,
    ...byId,
  };

  const ctx = vm.createContext(globals);
  return {
    ctx, window, document, streamerz, byId, timers, intervals, rafs, listeners,
    published, said, logged, audios, MockAudio,
    advance(ms) { now += ms; },
    setNow(t) { now = t; },
    runDueTimers() {
      for (const [id, t] of [...timers.entries()]) {
        if (t.at <= now) { timers.delete(id); t.fn(); }
      }
    },
    emitSettings(next) {
      Object.assign(streamerz.settings, next);
      for (const fn of listeners.settings) fn(streamerz.settings);
    },
    emitTopic(topic, val, old = false) {
      for (const fn of listeners.topics.get(topic) || []) fn(val, old);
    },
    emitEvent(name, val) {
      for (const fn of listeners.events.get(name) || []) fn(val, name);
    },
    emitControl(name, val, old) {
      for (const fn of listeners.controls.get(name) || []) fn(val, old);
    },
    emitChat(line) { for (const fn of listeners.chat) fn(line); },
    emitRemoved(r) { for (const fn of listeners.removed) fn(r); },
    emitAlert(a) { for (const fn of listeners.alerts) fn(a); },
  };
}

test("alert-box: queue never deadlocks without animationend, mutes voice, and guards prototype keys", async () => {
  const { script } = readExample("alert-box/files/index.html");
  let resolveSpeak;
  const h = harness({
    ids: ["stage", "extra"],
    settings: {
      platforms: ["twitch", "tiktok"],
      mode: "One at a time",
      gap: 0,
      animMs: 200,
      animOut: "Fade",
      volume: 100,
      untilSound: true,
      cardHtml: "<b>%user%</b> <i>%constructor%</i>",
      rules: [
        { when: "Follow", from: "Any", min: 0, max: 0, text: "%user% followed", sound: "chime.wav", say: "Hi %user%", volume: 80, seconds: 2, weight: 1 },
        { when: "TikTok gift", from: "Any", min: 0, max: 0, text: "%user% sent %gift%", sound: "", say: "", volume: 80, seconds: 2, weight: 1 },
      ],
    },
    speak: () => new Promise(r => { resolveSpeak = r; }),
  });
  vm.runInContext(script, h.ctx);

  // Take two alerts; first shows, second waits.
  h.emitAlert({ kind: "follow", platform: "twitch", user: "<img onerror=x>", args: {} });
  h.emitAlert({ kind: "follow", platform: "twitch", user: "Bob", args: {} });
  assert.equal(h.window.alertBox.showing.length, 1);
  assert.equal(h.window.alertBox.waiting.length, 1);
  // Card HTML escapes user input and leaves prototype names untouched.
  const card = h.byId.stage.children[0];
  assert.equal(card.innerHTML.includes("<img"), false);
  assert.match(card.innerHTML, /&#60;img onerror=x&#62;/);
  assert.match(card.innerHTML, /%constructor%/);

  // Non-finite audio duration must not shorten the alert timer to 0ms.
  const audio = h.audios[0];
  audio.duration = Infinity;
  audio.onloadedmetadata();
  assert.equal(h.window.alertBox.showing.length, 1);

  // Voice arrives and mute control mutes both audio and voice.
  const voice = new h.MockAudio("/speech/1");
  voice.durationMs = 1500;
  resolveSpeak(voice);
  await new Promise(r => setImmediate(r));
  h.window.alertBox.control({ do: "mute" });
  assert.equal(audio.muted, true);
  assert.equal(voice.muted, true);
  h.window.alertBox.control({ do: "unmute" });
  assert.equal(audio.muted, false);
  assert.equal(voice.muted, false);

  // Skip without firing animationend: fallback timer must advance the queue to Bob.
  h.window.alertBox.control({ do: "skip" });
  h.advance(500);
  h.runDueTimers();
  assert.equal(h.window.alertBox.showing.length, 1);
  assert.equal(h.window.alertBox.showing[0].a.user, "Bob");
  assert.equal(h.window.alertBox.waiting.length, 0);

  // A TikTok combo arriving while the prior combo card is already leaving starts a new alert.
  h.window.alertBox.control({ do: "clear" });
  h.advance(500);
  h.runDueTimers();
  h.emitAlert({ kind: "gift", platform: "tiktok", user: "Dee", userId: "d1", args: { giftId: 1, giftName: "Rose", repeatCount: 1 } });
  h.window.alertBox.control({ do: "skip" }); // now leaving, not yet gone
  h.emitAlert({ kind: "gift", platform: "tiktok", user: "Dee", userId: "d1", args: { giftId: 1, giftName: "Rose", repeatCount: 2 } });
  assert.equal(h.window.alertBox.waiting.length, 1);
});
