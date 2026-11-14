// The browser owns the account. TikTok runs in an offscreen frame, never a visible helper tab.
// Firefox's `browser` answers with promises in every manifest version; Chromium has only `chrome`.
const ext = globalThis.browser ?? chrome;
const PAGE = "https://www.tiktok.com/live/history/center";
const PROTOCOL = 2;
let ws = null;
let creating = null;
let generation = 0;
let connecting = false;

let idleTimer = null;
function touch() {
  if (idleTimer) clearTimeout(idleTimer);
  idleTimer = setTimeout(() => { closeContext().catch(() => {}); }, 90000);
}

const hasOffscreen = typeof ext.offscreen !== "undefined";
let ffFrame = null;
let ffPending = new Map();
let ffNext = 0;
let ffReadyResolve = null;
let ffReady = new Promise(resolve => { ffReadyResolve = resolve; });

function setupFirefoxListener() {
  window.addEventListener("message", e => {
    if (e.source !== ffFrame?.contentWindow || e.data?.streamerz !== true) return;
    if (e.data.ready) { ffReadyResolve?.({ error: e.data.error, code: e.data.code }); return; }
    const p = ffPending.get(e.data.id);
    if (p) { clearTimeout(p.timer); ffPending.delete(e.data.id); p.resolve(e.data.result); }
  });
}

async function closeContext() {
  if (idleTimer) { clearTimeout(idleTimer); idleTimer = null; }
  if (creating) await creating.catch(() => {});
  if (hasOffscreen) {
    if ((await ext.runtime.getContexts({ contextTypes: ["OFFSCREEN_DOCUMENT"] })).length) {
      await ext.offscreen.closeDocument();
    }
  } else if (ffFrame) {
    ffFrame.remove();
    ffFrame = null;
    ffReadyResolve = null;
    ffPending.clear();
  }
  await ext.declarativeNetRequest.updateSessionRules({ removeRuleIds: [1, 2, 3, 4, 5, 6] });
}

async function context() {
  if (creating) return creating;
  creating = (async () => {
    if (hasOffscreen) {
      if ((await ext.runtime.getContexts({ contextTypes: ["OFFSCREEN_DOCUMENT"] })).length) return;
    } else if (ffFrame) {
      return;
    }
    const cookies = await ext.cookies.getAll({ url: "https://www.tiktok.com/" });
    if (!cookies.some(c => c.name === "sessionid" && c.value)) throw Object.assign(new Error("Log in to TikTok from Accounts > TikTok > Set up."), { code: "login_required" });
    const block = { type: "block" };
    await ext.declarativeNetRequest.updateSessionRules({ removeRuleIds: [1, 2, 3, 4, 5, 6], addRules: [
      { id: 1, priority: 1, action: block, condition: { tabIds: [-1],
        requestDomains: ["tiktok.com", "tiktokcdn.com", "tiktokcdn-us.com", "tiktokcdn-eu.com", "tiktokv.com", "ibyteimg.com", "ibytedtos.com", "ttwstatic.com"],
        resourceTypes: ["image", "media", "font", "stylesheet"] } },
      { id: 2, priority: 1, action: block, condition: { tabIds: [-1],
        requestDomains: ["mon.tiktokv.com", "mon-sg.tiktokv.com", "mcs-sg.tiktokv.com", "libraweb-sg.tiktok.com", "analytics.tiktok.com"] } },
      { id: 3, priority: 1, action: block, condition: { tabIds: [-1], urlFilter: "||webcast.tiktok.com/webcast/feed/" } },
      // Only our offscreen frame / background iframe may embed this one page. Normal browser tabs keep their headers.
      { id: 4, priority: 1, action: { type: "modifyHeaders", responseHeaders: [{ header: "x-frame-options", operation: "remove" }] },
        condition: { urlFilter: "|" + PAGE + "^", resourceTypes: ["sub_frame"],
          initiatorDomains: [new URL(ext.runtime.getURL("")).hostname] } },
      // Block video players and sentry error tracking to slash background RAM
      { id: 5, priority: 1, action: block, condition: { tabIds: [-1], urlFilter: "||tiktokcdn.com/*xgplayer*" } },
      { id: 6, priority: 1, action: block, condition: { tabIds: [-1], urlFilter: "*sentry*" } },
    ] });
    if (hasOffscreen) {
      await ext.offscreen.createDocument({ url: "offscreen.html", reasons: ["IFRAME_SCRIPTING"],
        justification: "Use TikTok's signed requests without opening a tab or window." });
    } else {
      ffReady = new Promise(resolve => { ffReadyResolve = resolve; });
      ffFrame = document.createElement("iframe");
      ffFrame.src = PAGE + "#streamerz-parent=" + encodeURIComponent(location.origin);
      ffFrame.style.display = "none";
      if (!window.__ffListener) {
        window.__ffListener = true;
        setupFirefoxListener();
      }
      document.body.appendChild(ffFrame);
    }
  })();
  try { await creating; } finally { creating = null; }
}

async function handle(m) {
  const epoch = generation;
  if (m.op === "close") { await closeContext(); return {}; }
  if (m.op === "login") {
    const existing = (await ext.tabs.query({ url: "https://www.tiktok.com/login*" })).find(t => !t.incognito);
    if (existing) {
      await ext.tabs.update(existing.id, { active: true });
      await ext.windows.update(existing.windowId, { focused: true });
      return {};
    }
    await ext.tabs.create({ url: "https://www.tiktok.com/login", active: true });
    return {};
  }
  if (m.op === "session") {
    const keep = new Set(["ttwid", "sessionid", "sessionid_ss", "sid_tt", "sid_guard", "uid_tt", "tt-target-idc"]);
    const cookies = await ext.cookies.getAll({ url: "https://www.tiktok.com/" });
    return { cookie: cookies.filter(c => keep.has(c.name)).map(c => c.name + "=" + c.value).join("; ") };
  }
  if (!["page", "fetch", "search"].includes(m.op)) throw new Error("unknown request " + m.op);
  await context();
  if (epoch !== generation) throw new Error("Streamerz closed");
  let result;
  if (hasOffscreen) {
    try { result = await ext.runtime.sendMessage({ target: "offscreen", request: m }); }
    catch (error) {
      if (epoch !== generation) throw error;
      await closeContext();
      if (m.op === "fetch" && (m.init?.method || "GET").toUpperCase() !== "GET") throw error;
      await context();
      if (epoch !== generation) throw new Error("Streamerz closed");
      result = await ext.runtime.sendMessage({ target: "offscreen", request: m });
    }
  } else {
    const state = await Promise.race([ffReady, new Promise((_, reject) => {
      const timer = setTimeout(() => reject(new Error("TikTok didn't load. Try again, or log in from Accounts.")), 30000);
      ffReady.then(() => clearTimeout(timer));
    })]);
    if (state.error) throw Object.assign(new Error(state.error), { code: state.code });
    result = await new Promise(resolve => {
      const id = ++ffNext;
      const timer = setTimeout(() => { ffPending.delete(id); resolve({ error: "TikTok didn't answer. Try again." }); }, 30000);
      ffPending.set(id, { resolve, timer });
      ffFrame.contentWindow.postMessage({ streamerz: true, id, request: m }, "https://www.tiktok.com");
    });
  }
  if (result?.error) await closeContext();
  else touch();
  return result;
}

const SETUP_RE = /^file:\/\/.*\/tiktok-setup\.html([?#]|$)/;

async function closeSetupTabs() {
  const tabs = (await ext.tabs.query({})).filter(t => SETUP_RE.test(t.url || ""));
  for (const tab of tabs) {
    const sameWindow = await ext.tabs.query({ windowId: tab.windowId });
    if (sameWindow.length <= 1) await ext.tabs.create({ windowId: tab.windowId });
    await ext.tabs.remove(tab.id).catch(() => {});
  }
}

// Where Streamerz is and its pairing token. A copy Streamerz wrote to disk has them in config.json; a
// copy from a store learns them from the setup page Streamerz opens (#pair=... in its address).
function pairingFrom(address) {
  if (!SETUP_RE.test(address || "")) return null;
  const hash = (address.split("#")[1] || "");
  const params = new URLSearchParams(hash);
  const url = params.get("url") || "", token = params.get("token") || "";
  if (!/^ws:\/\/(127\.0\.0\.1|localhost|\[::1\]):\d+\/streamerz\/tiktok$/.test(url) || !/^[0-9a-f]{64}$/.test(token)) return null;
  return { url, token };
}

async function remember(pairing) {
  const known = await ext.storage.local.get(["url", "token"]);
  if (known.url === pairing.url && known.token === pairing.token) return false;
  await ext.storage.local.set(pairing);
  return true;
}

// A setup page already open (the extension was installed from it, or the browser restarted).
async function pairFromOpenTabs() {
  for (const tab of await ext.tabs.query({})) {
    const pairing = pairingFrom(tab.url);
    if (pairing && await remember(pairing)) { ws?.close(); ws = null; }
  }
}

// The files this copy was started with (config.json's "build"): when Streamerz writes new ones, the
// next connection attempt restarts the extension once, so the browser runs them.
let loadedBuild;
async function settings() {
  try {
    const response = await fetch(ext.runtime.getURL("config.json"), { cache: "no-store" });
    if (response.ok) {
      const s = await response.json();
      const build = s.build || "";
      if (loadedBuild === undefined) loadedBuild = build;
      else if (build !== loadedBuild) { ext.runtime.reload(); return null; }
      if (s.url && s.token) return s;
    }
  } catch {}
  const stored = await ext.storage.local.get(["url", "token"]);
  return stored.url && stored.token ? stored : null;
}

async function connect() {
  if (ws || connecting) return;
  connecting = true;
  let s;
  try { s = await settings(); }
  finally { connecting = false; }
  if (!s) return;
  const socket = new WebSocket(s.url);
  ws = socket;
  let paired = false;
  socket.onopen = () => socket.send(JSON.stringify({ op: "hello", token: s.token, protocol: PROTOCOL,
    version: ext.runtime.getManifest().version }));
  socket.onmessage = async e => {
    let m;
    try { m = JSON.parse(e.data); } catch { return; }
    if (m.id == null) return;
    if (!paired) { paired = true; closeSetupTabs().catch(() => {}); }
    let reply;
    try { reply = await handle(m); } catch (err) { reply = { error: String(err.message || err), code: err.code }; }
    if (ws === socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ id: m.id, ...reply }));
  };
  socket.onerror = () => {};
  socket.onclose = () => { if (ws === socket) { generation++; ws = null; closeContext().catch(() => {}); } };
}

// Session changes invalidate identity and signing state, without polling for a login.
let resetting = null;
ext.cookies.onChanged.addListener(({ cookie }) => {
  if (cookie.name !== "sessionid" || !/(^|\.)tiktok\.com$/.test(cookie.domain.replace(/^\./, "")) || ws?.readyState !== WebSocket.OPEN || resetting) return;
  resetting = closeContext().then(() => {
    if (ws?.readyState === WebSocket.OPEN) ws.send('{"op":"session_changed"}');
  }).catch(() => {}).finally(() => { resetting = null; });
});
setInterval(() => { if (ws?.readyState === WebSocket.OPEN) ws.send('{"op":"ping"}'); }, 20000);
ext.alarms.create("streamerz", { periodInMinutes: 1 });
ext.alarms.onAlarm.addListener(connect);
ext.runtime.onStartup.addListener(connect);
// Set up opens its tab in the user's existing profile. Retry there immediately, not a minute later.
ext.tabs.onUpdated.addListener(async (_id, change) => {
  const pairing = pairingFrom(change.url);
  // A new pairing (Streamerz was set up again, or moved to another port) replaces the old connection.
  if (pairing && await remember(pairing)) { ws?.close(); ws = null; }
  if (/^(chrome|edge|brave|vivaldi|opera):\/\/extensions\/?/.test(change.url || "") || SETUP_RE.test(change.url || "")) connect();
});
ext.runtime.onInstalled.addListener(async () => {
  // Upgrade from the visible-tab version: close only the exact tab it recorded as its own.
  // Firefox's persistent background has no session storage (and never had the visible tab).
  const { tabId } = (await ext.storage.session?.get("tabId")) ?? {};
  if (tabId != null) {
    const tab = await ext.tabs.get(tabId).catch(() => null);
    if (tab?.pinned && tab.url === PAGE) await ext.tabs.remove(tabId).catch(() => {});
    await ext.storage.session?.remove("tabId");
  }
  await pairFromOpenTabs().catch(() => {});
  connect();
});
pairFromOpenTabs().catch(() => {}).finally(connect);
