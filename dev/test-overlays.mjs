// Browser check of Streamerz overlays: a dev instance (WS on 8095, HTTP on 7495) and headless Chromium.
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";

mkdirSync("/tmp/szoverlay", {recursive: true});

const WS = process.env.STREAMERZ_TEST_WS || "ws://127.0.0.1:8095/";
const HTTP = "http://127.0.0.1:7495";
const sleep = ms => new Promise(r => setTimeout(r, ms));
const fails = [];
const check = (ok, what) => { console.log((ok ? "ok   " : "FAIL ") + what); if (!ok) fails.push(what); };

// Streamerz WebSocket requests.
const sz = new WebSocket(WS);
let n = 0; const waiting = {};
sz.onmessage = m => { const d = JSON.parse(m.data); if (d.id && waiting[d.id]) { waiting[d.id](d); delete waiting[d.id]; } };
await new Promise(r => sz.onopen = r);
const ask = req => new Promise(r => { const id = "r" + ++n; waiting[id] = r; sz.send(JSON.stringify({ ...req, id })); });

// Chromium over the DevTools protocol.
const chrome = spawn("chromium", ["--headless=new", "--remote-debugging-port=9333", "--no-first-run",
  "--user-data-dir=/tmp/szoverlay/profile", "--window-size=900,500", "about:blank"], { stdio: "ignore" });
let targets;
for (let i = 0; i < 50 && !targets; i++) {
  try { targets = await (await fetch("http://127.0.0.1:9333/json")).json(); } catch { await sleep(200); }
}
async function tab() {
  const t = await (await fetch("http://127.0.0.1:9333/json/new?about:blank", { method: "PUT" })).json();
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise(r => ws.onopen = r);
  let k = 0; const w = {}; const errors = [];
  ws.onmessage = m => {
    const d = JSON.parse(m.data);
    if (d.id && w[d.id]) { w[d.id](d); delete w[d.id]; }
    if (d.method === "Runtime.exceptionThrown") errors.push(d.params.exceptionDetails.exception?.description || d.params.exceptionDetails.text);
    if (d.method === "Runtime.consoleAPICalled" && d.params.type === "error") errors.push(d.params.args.map(a => a.value ?? a.description).join(" "));
  };
  const cmd = (method, params = {}) => new Promise(r => { const id = ++k; w[id] = r; ws.send(JSON.stringify({ id, method, params })); });
  await cmd("Runtime.enable");
  await cmd("Page.enable");
  const js = async expr => (await cmd("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
  const until = async (expr, ms = 5000) => {
    for (let t = 0; t < ms; t += 100) { if (await js(expr)) return true; await sleep(100); }
    return false;
  };
  const shot = async file => writeFileSync(file, Buffer.from((await cmd("Page.captureScreenshot")).result.data, "base64"));
  return { cmd, js, until, shot, errors, close: () => ws.close() };
}

try {
  for (const name of ["Ad break overlay", "Viewer counter overlay"])
    check((await ask({ request: "StreamerzTemplateAdd", name })).status === "ok", `template ${name} added`);
  // A custom overlay: a topic, an event and the group's settings, as a user would write one.
  const custom = `<!doctype html><html><head><title>t</title></head><body><div id=o></div><script>
    window.got = { song: [], chat: [] };
    streamerz.on("song", v => got.song.push(v));
    streamerz.event("Raw.ActionCompleted", (d, name) => got.chat.push([name, d.message ?? d.rawInput ?? ""]));
    document.getElementById("o").textContent = streamerz.settings.greeting;
  </script></body></html>`;
  await ask({ request: "StreamerzManageActionGroup", group: "Test overlay", operation: "create" });
  const meta = { description: "", variables: [{ name: "greeting", label: "Greeting", kind: "text", default: "Hello <b>&" }],
    files: [{ name: "test.html", content: Buffer.from(custom).toString("base64") }] };
  const set = await ask({ request: "StreamerzSetGroupMeta", group: "Test overlay", meta });
  check(set.status === "ok", "custom overlay group saved " + (set.error || ""));
  const savedMeta = (await ask({request: "StreamerzGroupMeta", group: "Test overlay"})).meta;
  const list = await ask({ request: "StreamerzOverlays" });
  check(list.overlays?.length >= 3, "three overlays listed: " + JSON.stringify(list.overlays?.map(o => o.url)));
  for (const p of ["twitch", "kick", "youtube"]) await ask({ request: "SimulateChatConnection", platform: p, connected: true });

  await ask({request: "SimulateChatViewers", platform: "youtube", count: 0});

  // The index page.
  const idx = await (await fetch(HTTP + "/overlays")).text();
  check(idx.includes("Ad%20overlay/ad.html") && idx.includes("Test%20overlay/test.html"), "index lists the pages");

  // Viewer counter: counts per platform, the total, live updates.
  const v = await tab();
  await ask({ request: "SimulateChatViewers", platform: "twitch", count: 120 });
  await ask({ request: "SimulateChatViewers", platform: "kick", count: 30 });
  await v.cmd("Page.navigate", { url: HTTP + "/overlays/Viewer%20overlay/viewers.html" });
  check(await v.until("document.body && document.body.innerText.includes('150')"), "viewer page shows the total of 150");
  await ask({ request: "SimulateChatViewers", platform: "youtube", count: 50 });
  check(await v.until("document.body.innerText.includes('200')"), "viewer page follows a change to 200");
  await sleep(1500);
  await v.shot("/tmp/szoverlay/viewers.png");
  console.log("     viewers text:", JSON.stringify(await v.js("document.body.innerText")));
  check(v.errors.length === 0, "viewer page has no errors " + v.errors.join("; "));

  // Ad badge: hidden, shown during a break, hidden when it ends.
  const a = await tab();
  await a.cmd("Page.navigate", { url: HTTP + "/overlays/Ad%20overlay/ad.html" });
  await sleep(1000);
  await a.shot("/tmp/szoverlay/ad-before.png");
  await ask({ request: "SimulateAdBreak", seconds: 3, automatic: true });
  await sleep(800);
  await a.shot("/tmp/szoverlay/ad-during.png");
  const during = await a.js("JSON.stringify(streamerz.state.ad)");
  check(during && JSON.parse(during).running === true, "ad page got the break: " + during);
  console.log("     ad text during:", JSON.stringify(await a.js("document.body.innerText")));
  await sleep(3500);
  await a.shot("/tmp/szoverlay/ad-after.png");
  check(JSON.parse(await a.js("JSON.stringify(streamerz.state.ad)")).running === false, "ad page saw the break end");
  check(a.errors.length === 0, "ad page has no errors " + a.errors.join("; "));

  // Custom page: settings (escaped by the page, not by us), a topic set before and after it opens, an event.
  await ask({ request: "StreamerzOverlayPublish", topic: "song", value: { title: "Before" } });
  const c = await tab();
  await c.cmd("Page.navigate", { url: HTTP + "/overlays/Test%20overlay/test.html" });
  check(await c.until("document.getElementById('o')?.textContent === 'Hello <b>&'"), "settings reach the page");
  check(await c.until("got.song.length === 1 && got.song[0].title === 'Before'"), "a topic set earlier arrives first");
  await ask({ request: "StreamerzOverlayPublish", topic: "song", value: "plain text" });
  check(await c.until("got.song[1] === 'plain text'"), "a new value arrives");
  check((await ask({request: "StreamerzSave", what: "action", item: {id: "overlay-test", name: "Overlay test", enabled: true, subActions: []}})).status === "ok", "test action saved");
  await ask({ request: "DoAction", action: {id: "overlay-test"} });
  check(await c.until("got.chat.length > 0"), "a Streamerz event arrives: " + await c.js("JSON.stringify(got.chat)"));
  check(c.errors.length === 0, "custom page has no errors " + c.errors.join("; "));

  await c.js('streamerz.on("constructor", v => window.specialTopic = v)');
  await sleep(200);
  await ask({request: "StreamerzOverlayPublish", topic: "constructor", value: "safe topic"});
  check(await c.until("window.specialTopic === 'safe topic'"), "topic names cannot collide with object properties");

  // Closing the pages stops the viewer polls' reason to run.
  const conns = async () => (await ask({ request: "StreamerzOverlays" })).status;
  check(await conns() === "ok", "still answering");

  // A copy saved to disk ("Save as page") talks to the server from file://.
  const files = (await ask({ request: "StreamerzGroupMeta", group: "Viewer overlay" }));
  console.log("     group meta keys:", Object.keys(files));
  const exported = await ask({request: "StreamerzGroupExport", group: "Test overlay"});
  check(exported.status === "ok" && exported.text, "overlay-only group exported");
  await ask({request: "StreamerzManageActionGroup", group: "Test overlay", operation: "delete"});
  check((await ask({request: "StreamerzActionImport", export: exported.export})).status === "ok", "overlay-only group imported");
  const restored = await ask({request: "StreamerzGroupMeta", group: "Test overlay"});
  check(JSON.stringify(restored.meta) === JSON.stringify(savedMeta), "files and settings survive export/import");
  const html = await (await fetch(HTTP + "/overlays/Test%20overlay/test.html")).text();
  const local = "/tmp/szoverlay/local.html";
  writeFileSync(local, html.replace('"server":""', '"server":"' + HTTP + '"'));
  const f = await tab();
  await f.cmd("Page.navigate", {url: "file://" + local});
  check(await f.until("got.song[0] === 'plain text'"), "local HTML receives live data");
  check(f.errors.length === 0, "local page has no errors " + f.errors.join("; "));
  f.close();
  // A page from another website is refused.
  const foreign = await fetch(HTTP + "/overlays/events?topics=viewers", { headers: { Origin: "https://evil.example" } });
  check(foreign.status === 403, "another website is refused: " + foreign.status);
  const missing = await fetch(HTTP + "/overlays/Nope/x.html");
  check(missing.status === 404, "a missing page is 404");
  [v, a, c].forEach(t => t.close());
} finally {
  chrome.kill();
  sz.close();
}
console.log(fails.length ? `\n${fails.length} failed` : "\nall passed");
process.exit(fails.length ? 1 : 0);
