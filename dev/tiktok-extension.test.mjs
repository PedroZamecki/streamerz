// Run with node --test dev/tiktok-extension.test.mjs. No browser or live account needed.
import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFileSync } from "node:fs";

function worker() {
  const state = { cookies: [{ name: "sessionid", value: "session" }, { name: "unrelated", value: "private" }], contexts: [], creates: 0, closes: 0, tabs: [], messages: [], rules: [], reply: () => ({ params: { device_id: "42" } }) };
  const chrome = {
    runtime: { id: "lhijoeeghjcohapolkfkacipklhbnecc", getContexts: async () => state.contexts,
      getManifest: () => ({ version: "1.1.0" }), sendMessage: async m => { state.messages.push(m); return state.reply(m); },
      getURL: path => "chrome-extension://lhijoeeghjcohapolkfkacipklhbnecc/" + path, onStartup: { addListener() {} }, onInstalled: { addListener() {} } },
    offscreen: { createDocument: async () => { state.creates++; state.contexts = [{}]; },
      closeDocument: async () => { state.closes++; state.contexts = []; } },
    declarativeNetRequest: { updateSessionRules: async r => { state.rules.push(r); } },
    tabs: { create: async t => { state.tabs.push({ ...t, id: (state.nextTabId = (state.nextTabId || 0) + 1), windowId: t.windowId ?? 1 }); },
      query: async q => state.tabs.filter(t => (q?.windowId == null || t.windowId === q.windowId) && (!q?.url || (t.url || "").startsWith(q.url.replace(/\*$/, "")))),
      update: async (id, changes) => { Object.assign(state.tabs.find(t => t.id === id), changes); },
      remove: async id => { state.tabs = state.tabs.filter(t => t.id !== id); },
      onUpdated: { addListener(f) { state.tabUpdated = f; } } },
    windows: { update: async () => {} },
    cookies: { getAll: async () => state.cookies,
      onChanged: { addListener(f) { state.cookieChanged = f; } } },
    alarms: { create() {}, onAlarm: { addListener() {} } },
    storage: { local: { get: async () => ({ ...state.settings }), set: async value => Object.assign(state.settings, value) } },
  };
  state.settings = {};
  state.timers = new Map();
  state.intervals = new Map();
  let nextTimer = 0;
  let nextInterval = 0;
  state.sockets = [];
  class Socket { static OPEN = 1; constructor(url) { this.url = url; this.readyState = 1; this.sent = []; state.sockets.push(this); } send(d) { this.sent.push(d); } close() { this.closed = true; } }
  const context = vm.createContext({ chrome, URL, URLSearchParams, fetch: async () => ({ ok: true, json: async () => ({ ...(state.fileSettings || state.settings) }) }), self: { STREAMERZ: {} }, importScripts() {},
    setInterval(callback, ms) { const id = ++nextInterval; state.intervals.set(id, { callback, ms }); return id; },
    clearInterval(id) { state.intervals.delete(id); },
    setTimeout(callback, ms) { const id = ++nextTimer; state.timers.set(id, { callback, ms }); return id; },
    clearTimeout(id) { state.timers.delete(id); }, WebSocket: Socket, console });
  vm.runInContext(readFileSync(new URL("../assets/tiktok-extension/bridge.js", import.meta.url), "utf8"), context);
  const call = request => vm.runInContext(`handle(${JSON.stringify(request)})`, context);
  return { state, context, call };
}

test("concurrent requests share a hidden context and never open helper tabs", async () => {
  const { state, call } = worker();
  await Promise.all([call({ op: "page" }), call({ op: "search", query: "creator" })]);
  assert.equal(state.creates, 1);
  assert.equal(state.tabs.length, 0);
  const headerRule = state.rules.flatMap(r => r.addRules || []).find(r => r.id === 4);
  assert.equal(headerRule.condition.resourceTypes.join(), "sub_frame");
  assert.equal(headerRule.condition.initiatorDomains.join(), "lhijoeeghjcohapolkfkacipklhbnecc");
  // "^": the address ends there, or (Firefox matches the fragment too) a "#" follows.
  assert.equal(headerRule.condition.urlFilter, "|https://www.tiktok.com/live/history/center^");
});

test("a lost send response is never retried", async () => {
  const { state, call } = worker();
  state.reply = () => { throw new Error("reply lost"); };
  await assert.rejects(call({ op: "fetch", url: "https://webcast.tiktok.com/webcast/room/chat/", init: { method: "POST", body: ":)" } }), /reply lost/);
  assert.equal(state.messages.length, 1);
  assert.equal(state.contexts.length, 0);
});

test("discarded context recovers once for reads and cleans up when disconnected", async () => {
  const { state, call } = worker();
  let attempts = 0;
  state.reply = () => { if (++attempts === 1) throw new Error("no receiver"); return { params: {} }; };
  await call({ op: "page" });
  assert.equal(state.creates, 2);
  assert.equal(state.messages.length, 2);
  await call({ op: "close" });
  assert.equal(state.contexts.length, 0);
});

test("failed page load is discarded for the next attempt", async () => {
  const { state, call } = worker();
  state.reply = () => ({ error: "page not ready" });
  await call({ op: "page" });
  assert.equal(state.contexts.length, 0);
  state.reply = () => ({ params: { device_id: "42" } });
  await call({ op: "page" });
  assert.equal(state.creates, 2);
});

test("reader only receives session cookies, and login alone opens a visible tab", async () => {
  const { state, call } = worker();
  assert.equal((await call({ op: "session" })).cookie, "sessionid=session");
  assert.equal(state.creates, 0);
  await call({ op: "login" });
  assert.equal(state.tabs.length, 1);
  assert.equal(state.tabs[0].url, "https://www.tiktok.com/login");
});

test("the reader can reuse TikTok's device cookie without unrelated browser cookies", async () => {
  const { state, call } = worker();
  state.cookies.push({ name: "ttwid", value: "device" });
  assert.equal((await call({ op: "session" })).cookie, "sessionid=session; ttwid=device");
  assert.equal(state.creates, 0);
  assert.equal(state.tabs.length, 0);
});

function frame({ pathname, hash = "", search = "", storage = new Map(), referrer = "" }) {
  const messages = [], replaced = [];
  const parent = { postMessage(m, origin) { messages.push({ m, origin }); } };
  const sessionStorage = { getItem: k => storage.get(k) ?? null, setItem: (k, v) => storage.set(k, v) };
  const history = { state: null, replaceState(_, __, url) { replaced.push(url); } };
  vm.runInNewContext(readFileSync(new URL("../assets/tiktok-extension/frame.js", import.meta.url), "utf8"), {
    window: { parent, addEventListener() {}, fetch() {} }, parent, location: { pathname, hash, search }, document: { referrer },
    sessionStorage, history, URL, URLSearchParams, performance: { setResourceTimingBufferSize() {}, getEntriesByType: () => [] },
    setTimeout() {}, Function,
  });
  return { messages, replaced, storage };
}

test("login redirects report sign-in immediately, without an idle loading loop", () => {
  const { messages } = frame({ pathname: "/login" });
  assert.equal(messages.length, 1);
  assert.equal(messages[0].origin, "chrome-extension://lhijoeeghjcohapolkfkacipklhbnecc");
  assert.equal(messages[0].m.ready, true);
  assert.match(messages[0].m.error, /Log in/);
});

test("Firefox's frame answers its own moz-extension page after TikTok's login redirect", () => {
  // Firefox sends no referrer from an extension: the background page names itself in the address.
  const parent = "moz-extension://9f64fc19-5a5d-4538-8b47-4acd59127c5a";
  const first = frame({ pathname: "/live/history/center", hash: "#streamerz-parent=" + encodeURIComponent(parent) });
  // The address loses it before TikTok's scripts run, so TikTok never sees it.
  assert.deepEqual(first.replaced, ["/live/history/center"]);
  const login = frame({ pathname: "/login", referrer: "https://www.tiktok.com/live/history/center", storage: first.storage });
  assert.equal(login.messages[0].origin, parent);
  // Only an extension's origin is ever taken.
  const forged = frame({ pathname: "/login", hash: "#streamerz-parent=https%3A%2F%2Fevil.example" });
  assert.equal(forged.messages[0].origin, "chrome-extension://lhijoeeghjcohapolkfkacipklhbnecc");
});


test("logged-out accounts do not load an expensive context or open a window", async () => {
  const { state, call } = worker();
  state.cookies = [];
  await assert.rejects(call({ op: "page" }), error => error.code === "login_required");
  assert.equal(state.creates, 0);
  assert.equal(state.tabs.length, 0);
});


test("disconnect during creation cannot reopen an orphaned context", async () => {
  const { state, context, call } = worker();
  let finish;
  context.chrome.offscreen.createDocument = () => new Promise(resolve => {
    finish = () => { state.creates++; state.contexts = [{}]; resolve(); };
  });
  const request = call({ op: "page" });
  await new Promise(resolve => setImmediate(resolve));
  assert.ok(finish);
  const closing = vm.runInContext("generation++; closeContext()", context);
  finish();
  await assert.rejects(request, /Streamerz closed/);
  await closing;
  assert.equal(state.contexts.length, 0);
  assert.equal(state.messages.length, 0);
});


test("repeated login requests reuse the existing TikTok sign-in tab", async () => {
  const { state, call } = worker();
  await call({ op: "login" });
  await call({ op: "login" });
  assert.equal(state.tabs.length, 1);
  assert.equal(state.tabs[0].active, true);
});

test("reconnecting rereads pairing settings, wakes on setup tab, and cleans up setup tabs on pair", async () => {
  const { state, context } = worker();
  await new Promise(resolve => setImmediate(resolve));
  state.settings = { url: "ws://127.0.0.1:8090/streamerz/tiktok", token: "first" };
  await vm.runInContext("connect()", context);
  assert.equal(state.sockets.length, 1);
  state.sockets[0].onclose();

  // Opening tiktok-setup.html wakes connect() immediately, and pairing closes the setup tab while keeping the window alive.
  state.tabs = [{ id: 10, windowId: 1, url: "file:///tmp/tiktok-setup.html" }];
  state.settings = { url: "ws://127.0.0.1:8091/streamerz/tiktok", token: "second" };
  state.tabUpdated(10, { url: "file:///tmp/tiktok-setup.html" });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(state.sockets.length, 2);
  assert.equal(state.sockets[1].url, state.settings.url);

  await state.sockets[1].onmessage({ data: JSON.stringify({ id: 1, op: "session" }) });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(state.tabs.some(t => (t.url || "").includes("tiktok-setup.html")), false);
  assert.equal(state.tabs.length, 1);
});

test("explicit pairing overrides stale unpacked settings without losing bundled fallback", async () => {
  const { state, context } = worker();
  await new Promise(resolve => setImmediate(resolve));
  state.fileSettings = { url: "ws://127.0.0.1:8080/streamerz/tiktok", token: "old", build: "" };
  state.settings = { url: "ws://127.0.0.1:8090/streamerz/tiktok", token: "new" };
  await vm.runInContext("connect()", context);
  assert.equal(state.sockets.at(-1).url, state.settings.url);
  assert.equal((await vm.runInContext("settings()", context)).token, "new");
  state.settings = {};
  assert.equal((await vm.runInContext("settings()", context)).url, state.fileSettings.url);
  context.chrome.storage.local.get = async () => { throw new Error("storage unavailable"); };
  assert.equal((await vm.runInContext("settings()", context)).token, "old");
});


test("activity replaces the idle timer and expiry releases the hidden context", async () => {
  const { state, call } = worker();
  await call({ op: "page" });
  await call({ op: "page" });
  assert.equal(state.timers.size, 1);
  const timer = [...state.timers.values()][0];
  assert.equal(timer.ms, 90000);
  timer.callback();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(state.contexts.length, 0);
});

test("ping timer runs only while connected and malformed WS messages are ignored", async () => {
  const { state, context } = worker();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(state.intervals.size, 0, "no ping interval while disconnected");

  state.settings = { url: "ws://127.0.0.1:8090/streamerz/tiktok", token: "a".repeat(64) };
  await vm.runInContext("connect()", context);
  const sock = state.sockets.at(-1);
  assert.equal(state.intervals.size, 0, "no ping interval before onopen");
  sock.onopen();
  assert.equal(state.intervals.size, 1);
  [...state.intervals.values()][0].callback();
  assert.equal(sock.sent.includes('{"op":"ping"}'), true);

  // Primitive or malformed JSON messages must not throw.
  await sock.onmessage({ data: "null" });
  await sock.onmessage({ data: "42" });
  await sock.onmessage({ data: "{bad" });

  sock.onclose();
  assert.equal(state.intervals.size, 0, "ping interval cleared on close");
});

test("re-pairing disconnects cleanly and closes active context", async () => {
  const { state, context, call } = worker();
  await new Promise(resolve => setImmediate(resolve));
  state.settings = { url: "ws://127.0.0.1:8090/streamerz/tiktok", token: "a".repeat(64) };
  await vm.runInContext("connect()", context);
  state.sockets[0].onopen();
  await call({ op: "page" });
  assert.equal(state.contexts.length, 1);
  assert.equal(state.intervals.size, 1);

  const newToken = "b".repeat(64);
  state.tabUpdated(5, { url: "file:///tmp/tiktok-setup.html#url=" + encodeURIComponent("ws://127.0.0.1:8091/streamerz/tiktok") + "&token=" + newToken });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(state.sockets[0].closed, true);
  assert.equal(state.contexts.length, 0);
  assert.equal(state.intervals.size, 0);
});

test("failed offscreen creation cleans up session rules", async () => {
  const { state, context, call } = worker();
  context.chrome.offscreen.createDocument = async () => { throw new Error("offscreen failed"); };
  await assert.rejects(call({ op: "page" }), /offscreen failed/);
  assert.deepEqual(structuredClone(state.rules.at(-1)), { removeRuleIds: [1, 2, 3, 4, 5, 6] });
});

test("Firefox bridge validates origin and settles pending requests on closeContext", async () => {
  const listeners = [];
  const posted = [];
  const timers = new Map();
  let nextTimer = 0;
  let removed = false;
  const frameEl = {
    style: {},
    contentWindow: { postMessage(m, target) { posted.push({ m, target }); } },
    remove() { removed = true; },
  };
  const browser = {
    runtime: { id: "ff", getManifest: () => ({ version: "1.4.0" }), getURL: p => "moz-extension://ff/" + p, onStartup: { addListener() {} }, onInstalled: { addListener() {} } },
    declarativeNetRequest: { updateSessionRules: async () => {} },
    tabs: { query: async () => [], onUpdated: { addListener() {} } },
    windows: { update: async () => {} },
    cookies: { getAll: async () => [{ name: "sessionid", value: "s" }], onChanged: { addListener() {} } },
    alarms: { get: async () => null, create() {}, onAlarm: { addListener() {} } },
    storage: { local: { get: async () => ({}), set: async () => {} } },
  };
  const win = { addEventListener(type, fn) { if (type === "message") listeners.push(fn); } };
  const ctx = vm.createContext({
    browser, window: win, location: { origin: "moz-extension://ff" },
    document: { createElement: () => frameEl, body: { appendChild() {} } },
    URL, URLSearchParams, fetch: async () => ({ ok: false }),
    setTimeout(fn, ms) { const id = ++nextTimer; timers.set(id, { fn, ms }); return id; },
    clearTimeout(id) { timers.delete(id); },
    setInterval() { return 1; }, clearInterval() {}, WebSocket: class {}, console,
  });
  vm.runInContext(readFileSync(new URL("../assets/tiktok-extension/bridge.js", import.meta.url), "utf8"), ctx);

  const req = vm.runInContext('handle({ op: "page" })', ctx);
  await new Promise(r => setImmediate(r));
  // Wrong origin from the same contentWindow must be ignored.
  listeners[0]({ source: frameEl.contentWindow, origin: "https://evil.example", data: { streamerz: true, ready: true } });
  await new Promise(r => setImmediate(r));
  assert.equal(posted.length, 0);

  // Valid TikTok origin readies the frame and posts the request.
  listeners[0]({ source: frameEl.contentWindow, origin: "https://www.tiktok.com", data: { streamerz: true, ready: true } });
  await new Promise(r => setImmediate(r));
  assert.equal(posted.length, 1);

  // Closing the context while a request is in flight resolves it cleanly and clears its 30s timer.
  await vm.runInContext("closeContext()", ctx);
  const res = await req;
  assert.equal(removed, true);
  assert.equal(res.error, "Streamerz closed");
  assert.equal(timers.size, 0);
});

test("offscreen.js ignores null runtime messages", () => {
  let onMsg;
  const frameEl = { contentWindow: null };
  const ctx = vm.createContext({
    document: { createElement: () => frameEl, body: { append() {} } },
    window: { addEventListener() {} },
    chrome: { runtime: { id: "ext", onMessage: { addListener(f) { onMsg = f; } } } },
    setTimeout, clearTimeout,
  });
  vm.runInContext(readFileSync(new URL("../assets/tiktok-extension/offscreen.js", import.meta.url), "utf8"), ctx);
  assert.equal(onMsg(null, { id: "ext" }, () => {}), undefined);
});

test("frame.js survives malformed rehydration JSON on search and rejects non-extension origins", async () => {
  const messages = [];
  const listeners = [];
  const fetched = [];
  const parent = { postMessage(m, origin) { messages.push({ m, origin }); } };
  const windowObj = {
    parent,
    byted_acrawler: {},
    addEventListener(type, fn) { if (type === "message") listeners.push(fn); },
    fetch: async url => { fetched.push(url); return { status: 200, text: async () => '{"status_code":0}' }; },
  };
  vm.runInNewContext(readFileSync(new URL("../assets/tiktok-extension/frame.js", import.meta.url), "utf8"), {
    window: windowObj,
    fetch: windowObj.fetch,
    parent,
    location: { pathname: "/live/history/center", hash: "", search: "" },
    document: {
      referrer: "",
      cookie: "",
      getElementById: () => ({ textContent: "{corrupt json" }),
    },
    sessionStorage: { getItem: () => null, setItem() {} },
    history: { state: null, replaceState() {} },
    URL, URLSearchParams,
    performance: {
      setResourceTimingBufferSize() {},
      getEntriesByType: () => [{ name: "https://webcast.tiktok.com/webcast/room/?device_id=42&aid=1988" }],
    },
    setTimeout() {}, Function,
  });

  // Forged origin with chrome-extension:// prefix but invalid characters must be rejected.
  await listeners[0]({ source: parent, origin: "chrome-extension://bad.origin/evil", data: { streamerz: true, id: 1, request: { op: "page" } } });
  assert.equal(messages.some(m => m.m.id === 1), false);

  // Search succeeds even when rehydration JSON is malformed.
  await listeners[0]({
    source: parent,
    origin: "chrome-extension://lhijoeeghjcohapolkfkacipklhbnecc",
    data: { streamerz: true, id: 2, request: { op: "search", query: "creator" } },
  });
  const reply = messages.find(m => m.m.id === 2);
  assert.equal(reply.m.result.status, 200);
  assert.match(fetched[0], /keyword=creator/);
});
