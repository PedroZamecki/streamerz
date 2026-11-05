// Streamerz for TikTok: keeps one light TikTok tab open while Streamerz runs, and makes the requests
// Streamerz asks for inside it. TikTok's own scripts in that tab sign each request like one of their own.
// Streamerz writes config.js next to this file: self.STREAMERZ = {url, token}.
importScripts("config.js");

const PAGE = "https://www.tiktok.com/live/history/center";
const PROTOCOL = 1;
let ws = null;
let tabId = null;

// The tab only needs TikTok's scripts: no pictures, video, fonts, telemetry or feed refreshes.
function rules(tab) {
  const block = { type: "block" };
  return [
    { id: 1, priority: 1, action: block, condition: { tabIds: [tab], resourceTypes: ["image", "media", "font"] } },
    { id: 2, priority: 1, action: block, condition: { tabIds: [tab],
      requestDomains: ["mon.tiktokv.com", "mon-sg.tiktokv.com", "mcs-sg.tiktokv.com", "libraweb-sg.tiktok.com"] } },
    { id: 3, priority: 1, action: block, condition: { tabIds: [tab], urlFilter: "||webcast.tiktok.com/webcast/feed/" } },
  ];
}

const inPage = async (tab, func, args = []) => {
  const [r] = await chrome.scripting.executeScript({ target: { tabId: tab }, world: "MAIN", func, args });
  return r && r.result;
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function openTab() {
  if (tabId !== null) {
    const t = await chrome.tabs.get(tabId).catch(() => null);
    if (t && t.url && t.url.startsWith("https://www.tiktok.com/")) return tabId;
  }
  // Blank first, so the rules are in place before TikTok's page loads.
  let t;
  try {
    t = await chrome.tabs.create({ url: "about:blank", active: false, pinned: true });
  } catch {
    const w = await chrome.windows.create({ url: "about:blank", focused: false, state: "minimized" });
    t = w.tabs[0];
  }
  tabId = t.id;
  await chrome.storage.session.set({ tabId });
  await chrome.tabs.update(tabId, { muted: true, autoDiscardable: false });
  await chrome.declarativeNetRequest.updateSessionRules({ removeRuleIds: [1, 2, 3], addRules: rules(tabId) });
  await chrome.tabs.update(tabId, { url: PAGE });
  for (let i = 0; i < 60; i++) {
    await sleep(500);
    const ok = await inPage(tabId, () => !!(window.byted_acrawler && window.secsdk)).catch(() => false);
    if (ok) return tabId;
  }
  throw new Error("TikTok's page didn't load");
}

async function closeTab() {
  const id = tabId ?? (await chrome.storage.session.get("tabId")).tabId;
  tabId = null;
  await chrome.storage.session.remove("tabId");
  if (id != null) await chrome.tabs.remove(id).catch(() => {});
}

// The query TikTok's page sends with its own webcast requests (browser, device, region...).
async function params(tab) {
  for (let i = 0; i < 20; i++) {
    const p = await inPage(tab, () => {
      const urls = performance.getEntriesByType("resource").map((e) => e.name)
        .filter((u) => u.startsWith("https://webcast.tiktok.com/webcast/") && u.includes("device_id="));
      return urls.length ? Object.fromEntries(new URL(urls[urls.length - 1]).searchParams) : null;
    });
    if (p) return p;
    await sleep(500);
  }
  throw new Error("TikTok's page made no request to learn from");
}

async function fetchInPage(tab, url, init) {
  return inPage(tab, async (url, init) => {
    try {
      const res = await fetch(url, { credentials: "include", ...init });
      if (!res) return { error: "TikTok's page refused the request" };
      return { status: res.status, body: await res.text() };
    } catch (e) {
      return { error: String(e) };
    }
  }, [url, init || {}]);
}

async function handle(m) {
  switch (m.op) {
    case "page": {
      const tab = await openTab();
      return { params: await params(tab) };
    }
    case "fetch":
      return fetchInPage(await openTab(), m.url, m.init);
    case "close":
      await closeTab();
      return {};
    default:
      return { error: "unknown request " + m.op };
  }
}

function connect() {
  if (ws) return;
  const s = (self.STREAMERZ || {});
  if (!s.url || !s.token) return;
  ws = new WebSocket(s.url);
  ws.onopen = () => ws.send(JSON.stringify({ op: "hello", token: s.token, protocol: PROTOCOL,
    version: chrome.runtime.getManifest().version }));
  ws.onmessage = async (e) => {
    let m;
    try { m = JSON.parse(e.data); } catch { return; }
    if (m.id == null) return;
    let reply;
    try { reply = await handle(m); } catch (err) { reply = { error: String(err && err.message || err) }; }
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ id: m.id, ...reply }));
  };
  ws.onerror = () => {};
  // Streamerz closed: the tab goes too, and nothing runs until it's back (checked once a minute).
  ws.onclose = () => { ws = null; closeTab(); };
}

// Traffic every 20 s keeps the worker alive while Streamerz is connected (Chrome 116+).
setInterval(() => { if (ws && ws.readyState === WebSocket.OPEN) ws.send('{"op":"ping"}'); }, 20000);
chrome.alarms.create("streamerz", { periodInMinutes: 1 });
chrome.alarms.onAlarm.addListener(connect);
// A browser restart brings pinned tabs back, without the rules above: close ours.
chrome.runtime.onStartup.addListener(async () => {
  for (const t of await chrome.tabs.query({ pinned: true, url: PAGE })) await chrome.tabs.remove(t.id).catch(() => {});
  connect();
});
chrome.runtime.onInstalled.addListener(connect);
chrome.storage.session.get("tabId").then((s) => { if (s.tabId != null && tabId === null) tabId = s.tabId; });
connect();
