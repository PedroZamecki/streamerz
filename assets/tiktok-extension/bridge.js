// The browser owns the account. TikTok runs in an offscreen frame, never a visible helper tab.
importScripts("config.js");
const PAGE = "https://www.tiktok.com/live/history/center";
const PROTOCOL = 2;
let ws = null;
let creating = null;
let generation = 0;

async function closeContext() {
  if (creating) await creating.catch(() => {});
  if ((await chrome.runtime.getContexts({ contextTypes: ["OFFSCREEN_DOCUMENT"] })).length) {
    await chrome.offscreen.closeDocument();
  }
  await chrome.declarativeNetRequest.updateSessionRules({ removeRuleIds: [1, 2, 3, 4] });
}

async function context() {
  if (creating) return creating;
  creating = (async () => {
    if ((await chrome.runtime.getContexts({ contextTypes: ["OFFSCREEN_DOCUMENT"] })).length) return;
    const cookies = await chrome.cookies.getAll({ url: "https://www.tiktok.com/" });
    if (!cookies.some(c => c.name === "sessionid" && c.value)) throw new Error("Log in to TikTok from Accounts > TikTok > Set up.");
    const block = { type: "block" };
    await chrome.declarativeNetRequest.updateSessionRules({ removeRuleIds: [1, 2, 3, 4], addRules: [
      { id: 1, priority: 1, action: block, condition: { tabIds: [-1],
        requestDomains: ["tiktok.com", "tiktokcdn.com", "tiktokcdn-us.com", "tiktokcdn-eu.com", "tiktokv.com", "ibyteimg.com", "ibytedtos.com", "ttwstatic.com"],
        resourceTypes: ["image", "media", "font"] } },
      { id: 2, priority: 1, action: block, condition: { tabIds: [-1],
        requestDomains: ["mon.tiktokv.com", "mon-sg.tiktokv.com", "mcs-sg.tiktokv.com", "libraweb-sg.tiktok.com"] } },
      { id: 3, priority: 1, action: block, condition: { tabIds: [-1], urlFilter: "||webcast.tiktok.com/webcast/feed/" } },
      // Only our offscreen frame may embed this one page. Normal browser tabs keep their headers.
      { id: 4, priority: 1, action: { type: "modifyHeaders", responseHeaders: [{ header: "x-frame-options", operation: "remove" }] },
        condition: { initiatorDomains: [chrome.runtime.id], urlFilter: "|" + PAGE + "|", resourceTypes: ["sub_frame"] } },
    ] });
    await chrome.offscreen.createDocument({ url: "offscreen.html", reasons: ["IFRAME_SCRIPTING"],
      justification: "Use TikTok's signed requests without opening a tab or window." });
  })();
  try { await creating; } finally { creating = null; }
}

async function handle(m) {
  const epoch = generation;
  if (m.op === "close") { await closeContext(); return {}; }
  if (m.op === "login") {
    await chrome.tabs.create({ url: "https://www.tiktok.com/login", active: true });
    return {};
  }
  if (m.op === "session") {
    const keep = new Set(["sessionid", "sessionid_ss", "sid_tt", "sid_guard", "uid_tt", "tt-target-idc"]);
    const cookies = await chrome.cookies.getAll({ url: "https://www.tiktok.com/" });
    return { cookie: cookies.filter(c => keep.has(c.name)).map(c => c.name + "=" + c.value).join("; ") };
  }
  if (!["page", "fetch", "search"].includes(m.op)) throw new Error("unknown request " + m.op);
  await context();
  if (epoch !== generation) throw new Error("Streamerz closed");
  // A browser may discard a context. Recreate it once, then give the actual error to Streamerz.
  let result;
  try { result = await chrome.runtime.sendMessage({ target: "offscreen", request: m }); }
  catch (error) {
    if (epoch !== generation) throw error;
    await closeContext();
    // A lost reply is ambiguous: a message may already have reached TikTok. Never send it twice.
    if (m.op === "fetch" && (m.init?.method || "GET").toUpperCase() !== "GET") throw error;
    await context();
    if (epoch !== generation) throw new Error("Streamerz closed");
    result = await chrome.runtime.sendMessage({ target: "offscreen", request: m });
  }
  if (result?.error) await closeContext();
  return result;

}

function connect() {
  if (ws) return;
  const s = self.STREAMERZ || {};
  if (!s.url || !s.token) return;
  const socket = new WebSocket(s.url);
  ws = socket;
  socket.onopen = () => socket.send(JSON.stringify({ op: "hello", token: s.token, protocol: PROTOCOL,
    version: chrome.runtime.getManifest().version }));
  socket.onmessage = async e => {
    let m;
    try { m = JSON.parse(e.data); } catch { return; }
    if (m.id == null) return;
    let reply;
    try { reply = await handle(m); } catch (err) { reply = { error: String(err.message || err) }; }
    if (ws === socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ id: m.id, ...reply }));
  };
  socket.onerror = () => {};
  socket.onclose = () => { if (ws === socket) { generation++; ws = null; closeContext().catch(() => {}); } };
}

// Session changes invalidate identity and signing state, without polling for a login.
let resetting = null;
chrome.cookies.onChanged.addListener(({ cookie }) => {
  if (cookie.name !== "sessionid" || !/(^|\.)tiktok\.com$/.test(cookie.domain.replace(/^\./, "")) || ws?.readyState !== WebSocket.OPEN || resetting) return;
  resetting = closeContext().then(() => {
    if (ws?.readyState === WebSocket.OPEN) ws.send('{"op":"session_changed"}');
  }).catch(() => {}).finally(() => { resetting = null; });
});
setInterval(() => { if (ws?.readyState === WebSocket.OPEN) ws.send('{"op":"ping"}'); }, 20000);
chrome.alarms.create("streamerz", { periodInMinutes: 1 });
chrome.alarms.onAlarm.addListener(connect);
chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(async () => {
  // Upgrade from the visible-tab version: close only the exact tab it recorded as its own.
  const { tabId } = await chrome.storage.session.get("tabId");
  if (tabId != null) {
    const tab = await chrome.tabs.get(tabId).catch(() => null);
    if (tab?.pinned && tab.url === PAGE) await chrome.tabs.remove(tabId).catch(() => {});
    await chrome.storage.session.remove("tabId");
  }
  connect();
});
connect();
