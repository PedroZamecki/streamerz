// Run with node --test dev/overlays-client.test.mjs. No browser or running app needed.
import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFileSync } from "node:fs";

const CLIENT_SRC = readFileSync(new URL("../assets/overlays/client.js", import.meta.url), "utf8");

function makeElement(tag) {
  const classes = new Set();
  const styles = Object.create(null);
  const el = {
    tagName: tag.toUpperCase(),
    className: "",
    textContent: "",
    dataset: {},
    children: [],
    hidden: false,
    isConnected: true,
    _srcSet: 0,
    style: {
      setProperty(k, v) { styles[k] = v; },
      getPropertyValue(k) { return styles[k] || ""; },
      _styles: styles,
    },
    classList: {
      add(...cs) { cs.forEach(c => classes.add(c)); },
      remove(...cs) { cs.forEach(c => classes.delete(c)); },
      contains(c) { return classes.has(c); },
    },
    append(...nodes) {
      for (const n of nodes) {
        if (n && n._fragment) this.children.push(...n.children);
        else this.children.push(n);
      }
    },
    _animations: [],
    animate(frames, opts) {
      let resolveFinished;
      const finished = new Promise(r => { resolveFinished = r; });
      const anim = {
        frames,
        opts,
        cancelled: false,
        finished,
        finish() { resolveFinished(); },
        cancel() { this.cancelled = true; resolveFinished(); },
      };
      this._animations.push(anim);
      return anim;
    },
  };
  let srcVal;
  Object.defineProperty(el, "src", {
    get() { return srcVal; },
    set(v) { el._srcSet++; srcVal = v; },
    enumerable: true,
  });
  return el;
}

function loadClient({
  config = { server: "http://127.0.0.1:7474", settings: {}, controls: {}, group: "Demo", token: "tok" },
  search = "",
  pathname = "/overlays/Demo/index.html",
  fetchHandler = async () => ({ json: async () => ({ ok: true }) }),
} = {}) {
  const state = {
    sockets: [],
    timers: new Map(),
    calls: [],
    listeners: new Map(),
    audios: [],
    reloaded: 0,
    head: makeElement("head"),
    errors: [],
  };
  let nextTimer = 0;

  class MockWebSocket {
    constructor(url) {
      this.url = url;
      this.closed = false;
      state.sockets.push(this);
    }
    close() { this.closed = true; }
  }

  class MockAudio {
    constructor(url) {
      this.url = url;
      this.volume = 1;
      this.paused = false;
      this.played = false;
      state.audios.push(this);
    }
    play() { this.played = true; return Promise.resolve(); }
    pause() { this.paused = true; }
  }

  const document = {
    readyState: "complete",
    head: state.head,
    documentElement: state.head,
    currentScript: { textContent: "\n\n\n" },
    createElement: tag => makeElement(tag),
    createDocumentFragment: () => ({ _fragment: true, children: [], append(...nodes) { this.children.push(...nodes); } }),
  };

  const window = {};
  const location = {
    origin: "http://127.0.0.1:7474",
    href: "http://127.0.0.1:7474" + pathname + search,
    pathname,
    search,
    reload() { state.reloaded++; },
  };

  const ctx = vm.createContext({
    window,
    location,
    document,
    URLSearchParams,
    WebSocket: MockWebSocket,
    Audio: MockAudio,
    fetch: async (url, init) => {
      const body = init?.body ? JSON.parse(init.body) : null;
      state.calls.push({ url, body });
      return fetchHandler(body, url);
    },
    setTimeout(fn, ms) {
      const id = ++nextTimer;
      state.timers.set(id, { fn, ms });
      return id;
    },
    clearTimeout(id) { state.timers.delete(id); },
    addEventListener(type, fn) { (state.listeners.get(type) || state.listeners.set(type, []).get(type)).push(fn); },
    console: { error(...a) { state.errors.push(a); } },
  });

  const code = CLIENT_SRC.replace("__STREAMERZ__", JSON.stringify(config));
  vm.runInContext(code, ctx);

  function flushNextTimer() {
    const first = state.timers.entries().next();
    if (first.done) return null;
    const [id, entry] = first.value;
    state.timers.delete(id);
    entry.fn();
    return entry.ms;
  }

  return { streamerz: window.streamerz, window, state, flushNextTimer };
}

test("websocket reconnect backs off exponentially and resets on open", () => {
  const { streamerz, state, flushNextTimer } = loadClient();
  streamerz.on("viewers", () => {});
  assert.equal(flushNextTimer(), 0);
  assert.equal(state.sockets.length, 1);

  const delays = [];
  for (let i = 0; i < 6; i++) {
    state.sockets.at(-1).onclose();
    delays.push(flushNextTimer());
  }
  assert.deepEqual(delays, [2000, 4000, 8000, 16000, 30000, 30000]);

  // A successful connection resets backoff to 2000ms.
  state.sockets.at(-1).onopen();
  state.sockets.at(-1).onclose();
  assert.equal(flushNextTimer(), 2000);
});

test("stale websocket messages and malformed JSON are ignored", () => {
  const { streamerz, state, flushNextTimer } = loadClient();
  const seen = [];
  streamerz.on("score", v => seen.push(v));
  flushNextTimer();
  const first = state.sockets[0];

  // Trigger a reconnect by adding another listener.
  streamerz.on("ad", () => {});
  flushNextTimer();
  const second = state.sockets[1];
  assert.equal(first.closed, true);

  // Stale socket message must be ignored.
  first.onmessage?.({ data: JSON.stringify({ event: { source: "Overlay", type: "score" }, data: "stale" }) });
  assert.deepEqual(seen, []);

  // Malformed JSON and missing event object must not throw.
  second.onmessage({ data: "{not json" });
  second.onmessage({ data: "null" });
  second.onmessage({ data: "{}" });
  second.onmessage({ data: JSON.stringify({ event: { source: "Overlay", type: "score" }, data: "live" }) });
  assert.deepEqual(seen, ["live"]);
});

test("avatars cache evicts old entries and retries after transient errors", async () => {
  let failOnce = true;
  const { streamerz, state } = loadClient({
    fetchHandler: async body => {
      if (body.userId === "flaky" && failOnce) {
        failOnce = false;
        throw new Error("offline");
      }
      return { json: async () => ({ ok: true, url: "https://cdn.example/" + body.userId + ".png" }) };
    },
  });

  // Transient error returns "" without permanently poisoning the cache.
  assert.equal(await streamerz.avatar({ platform: "twitch", userId: "flaky" }), "");
  assert.equal(await streamerz.avatar({ platform: "twitch", userId: "flaky" }), "https://cdn.example/flaky.png");

  // Fill past 500 entries and verify the oldest key is evicted and re-fetched.
  for (let i = 0; i < 501; i++) {
    await streamerz.avatar({ platform: "twitch", userId: "u" + i });
  }
  const before = state.calls.length;
  await streamerz.avatar({ platform: "twitch", userId: "u0" });
  assert.equal(state.calls.length, before + 1);
});

test("animate cancels prior out animation, prunes detached elements, and guards prototype keys", async () => {
  const enter = { name: "fade", durationMs: 0, delayMs: 0, easing: "ease-out" };
  const leave = { name: "fade", durationMs: 200, delayMs: 0, easing: "ease-in" };
  const { streamerz, state, flushNextTimer } = loadClient({
    config: { server: "http://127.0.0.1:7474", settings: { enter, leave }, controls: {}, group: "Demo", token: "tok" },
  });

  const el = makeElement("div");
  const outPromise = streamerz.animate(el, streamerz.settings.leave, { out: true });
  assert.equal(el._animations.length, 1);
  assert.equal(el._animations[0].opts.fill, "forwards");
  el._animations[0].finish();
  await outPromise;

  // Next enter animation must cancel the lingering forwards-filled leave animation.
  const inPromise = streamerz.animate(el, streamerz.settings.enter);
  assert.equal(el._animations[0].cancelled, true);
  assert.equal(el._animations[1].opts.duration, 0);
  el._animations[1].finish();
  await inPromise;

  // Prototype names like "constructor" must not throw.
  await streamerz.animate(el, { name: "constructor" });

  // Detached elements are pruned on Play replay.
  flushNextTimer();
  const ws = state.sockets.at(-1);
  const removed = makeElement("div");
  const live = makeElement("div");
  streamerz.animate(removed, streamerz.settings.enter);
  removed.isConnected = false;
  streamerz.animate(live, streamerz.settings.enter);
  const removedCount = removed._animations.length;
  const liveCount = live._animations.length;
  ws.onmessage({ data: JSON.stringify({ event: { source: "Play", type: "Demo" }, data: { name: "enter" } }) });
  assert.equal(removed._animations.length, removedCount);
  assert.equal(live._animations.length, liveCount + 1);
});

test("line omits empty img.src for pending avatars and escapes chat text as text nodes", () => {
  const { streamerz } = loadClient({
    fetchHandler: async () => ({ json: async () => ({ ok: true, url: "" }) }),
  });
  const el = streamerz.line(
    { platform: "twitch", user: "<b>alice</b>", userId: "test-1", text: "<img src=x onerror=alert(1)>", parts: [{ t: "<script>x()</script>" }] },
    { avatar: true },
  );
  const head = el.children[0];
  const avatarImg = head.children.find(c => c.className === "sz-avatar");
  assert.equal(avatarImg._srcSet, 0);
  assert.equal(avatarImg.hidden, true);
  const textSpan = el.children[1];
  assert.deepEqual(textSpan.children, ["<script>x()</script>"]);
});

test("control ignores Object.prototype keys and keeps streamerz.controls in sync", () => {
  const { streamerz, state, flushNextTimer } = loadClient({
    config: { server: "http://127.0.0.1:7474", settings: {}, controls: { show: true }, group: "Demo", token: "tok" },
  });
  let protoFired = false;
  streamerz.control("toString", () => { protoFired = true; });
  assert.equal(protoFired, false);

  const got = [];
  streamerz.control("show", (v, old) => got.push([v, old]));
  assert.deepEqual(got, [[true, undefined]]);

  flushNextTimer();
  state.sockets[0].onmessage({
    data: JSON.stringify({ event: { source: "Control", type: "Demo" }, data: { name: "show", value: false, old: true } }),
  });
  assert.equal(streamerz.controls.show, false);
  assert.deepEqual(got, [[true, undefined], [false, true]]);
});

test("css sanitizes quotes, backslashes, semicolons and newlines", () => {
  const { streamerz } = loadClient();
  const out = streamerz.css({
    font: 'Inter"; background:url(x)\\\n',
    size: 24,
    weight: 700,
    color: "#fff; position:fixed",
    outline: 2,
    outlineColor: "#000\n;color:red",
    shadow: true,
  });
  const decls = out.split(";").map(s => s.trim()).filter(Boolean);
  assert.equal(decls.length, 7);
  assert.equal(decls.some(d => d.startsWith("background:") || d.startsWith("position:")), false);
  assert.equal(out.includes("\n"), false);
  assert.match(out, /font-size: 24px/);
  assert.match(out, /font-weight: 700/);
});

test("Test, Draft, Settings and Sound messages follow the overlay contract", () => {
  const { streamerz, state, flushNextTimer } = loadClient({
    config: { server: "http://127.0.0.1:7474", settings: { title: "Hi" }, controls: {}, group: "Demo", token: "tok" },
    search: "?preview=1",
  });
  assert.equal(streamerz.preview, true);
  flushNextTimer();
  const ws = state.sockets[0];

  // Draft with only extraCss does not reload even without onSettings.
  ws.onmessage({ data: JSON.stringify({ event: { source: "Draft", type: "Demo" }, data: { title: "Hi", extraCss: "body{color:red}" } }) });
  assert.equal(state.reloaded, 0);

  // Draft with other setting reloads when page has no onSettings listener.
  ws.onmessage({ data: JSON.stringify({ event: { source: "Draft", type: "Demo" }, data: { title: "Bye" } }) });
  assert.equal(state.reloaded, 1);

  // Preview ignores Settings events (drafts arrive via Draft).
  const settingsSeen = [];
  streamerz.onSettings(s => settingsSeen.push(s.title));
  flushNextTimer();
  const ws2 = state.sockets.at(-1);
  ws2.onmessage({ data: JSON.stringify({ event: { source: "Settings", type: "Demo" }, data: { title: "Saved" } }) });
  assert.deepEqual(settingsSeen, ["Hi"]);

  // Test sets streamerz.testing only while firing handlers.
  let wasTesting = false;
  streamerz.on("score", () => { wasTesting = streamerz.testing; });
  flushNextTimer();
  const ws3 = state.sockets.at(-1);
  ws3.onmessage({ data: JSON.stringify({ event: { source: "Test", type: "Demo" }, data: { topics: { score: 7 }, events: [] } }) });
  assert.equal(wasTesting, true);
  assert.equal(streamerz.testing, false);

  // Sound and SoundStop manage audio elements.
  ws3.onmessage({ data: JSON.stringify({ event: { source: "Sound", type: "Demo" }, data: { url: "/overlays/-/speech/1", volume: 0.5, name: "Alert" } }) });
  assert.equal(state.audios.length, 1);
  assert.equal(state.audios[0].url, "http://127.0.0.1:7474/overlays/-/speech/1");
  assert.equal(state.audios[0].volume, 0.5);
  ws3.onmessage({ data: JSON.stringify({ event: { source: "SoundStop", type: "Demo" }, data: { name: "alert" } }) });
  assert.equal(state.audios[0].paused, true);
});
