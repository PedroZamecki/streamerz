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
      onStartup: { addListener() {} }, onInstalled: { addListener() {} } },
    offscreen: { createDocument: async () => { state.creates++; state.contexts = [{}]; },
      closeDocument: async () => { state.closes++; state.contexts = []; } },
    declarativeNetRequest: { updateSessionRules: async r => { state.rules.push(r); } },
    tabs: { create: async t => { state.tabs.push(t); } },
    cookies: { getAll: async () => state.cookies,
      onChanged: { addListener(f) { state.cookieChanged = f; } } },
    alarms: { create() {}, onAlarm: { addListener() {} } },
  };
  const context = vm.createContext({ chrome, self: { STREAMERZ: {} }, importScripts() {}, setInterval() {}, WebSocket: { OPEN: 1 }, console });
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
  assert.equal(headerRule.condition.urlFilter, "|https://www.tiktok.com/live/history/center|");
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

test("login redirects report sign-in immediately, without an idle loading loop", () => {
  const messages = [];
  const parent = { postMessage(m, origin) { messages.push({ m, origin }); } };
  vm.runInNewContext(readFileSync(new URL("../assets/tiktok-extension/frame.js", import.meta.url), "utf8"), {
    window: { parent }, parent, location: { pathname: "/login" },
  });
  assert.equal(messages.length, 1);
  assert.equal(messages[0].origin, "chrome-extension://lhijoeeghjcohapolkfkacipklhbnecc");
  assert.equal(messages[0].m.ready, true);
  assert.match(messages[0].m.error, /Log in/);
});


test("logged-out accounts do not load an expensive context or open a window", async () => {
  const { state, call } = worker();
  state.cookies = [];
  await assert.rejects(call({ op: "page" }), /Log in/);
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
