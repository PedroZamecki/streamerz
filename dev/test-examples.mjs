// Browser check of the bundled example overlays (assets/examples): a dev instance (WS on 8095, HTTP on
// 7495) and headless Chromium. Each example is added, opened, fed test data and photographed into
// /tmp/szoverlay/example-<slug>.png. The isolated dev fixture needs a Twitch broadcaster (empty token,
// manualConnect: true) so simulated connections can exercise the replies to chat without a login.
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
const chrome = spawn("chromium", ["--headless=new", "--remote-debugging-port=9334", "--no-first-run",
  "--user-data-dir=/tmp/szoverlay/profile-examples", "--window-size=900,500", "about:blank"], { stdio: "ignore" });
let targets;
for (let i = 0; i < 50 && !targets; i++) {
  try { targets = await (await fetch("http://127.0.0.1:9334/json")).json(); } catch { await sleep(200); }
}
async function tab() {
  const t = await (await fetch("http://127.0.0.1:9334/json/new?about:blank", { method: "PUT" })).json();
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
  // A tab behind another one stops drawing: it comes to the front for its picture.
  const shot = async file => (await cmd("Page.bringToFront"), await sleep(400), writeFileSync(file, Buffer.from((await cmd("Page.captureScreenshot")).result.data, "base64")));
  return { cmd, js, until, shot, errors, close: async () => { await fetch("http://127.0.0.1:9334/json/close/" + t.id); ws.close(); } };
}

const view = async () => (await ask({ request: "StreamerzCoreView", domain: "overlays" })).tables[0].rows;
const act = (action, itemId, domain = "overlays") => ask({ request: "StreamerzCoreAction", domain, action, itemId });
// Change some of a group's settings, keeping the rest.
const values = async (group, change) => {
  const now = (await ask({ request: "StreamerzGroupVariables", group })).values || {};
  const answer = await ask({ request: "StreamerzSetGroupVariables", group, values: { ...now, ...change } });
  check(answer.status === "ok", `${group} takes ${Object.keys(change).join(", ")} ${answer.error || ""}`);
};
// A page over a grey backdrop (as over a stream), opened and listening.
async function open(group, w = 640, h = 260, file = "") {
  const row = (await view()).find(r => r.cells[1] === group && r.cells[5].endsWith("/" + (file || r.cells[5].split("/").pop())) && (file || !/dock\.html$/.test(r.cells[5]) || group === "Chat dock"));
  check(!!row, `${group} is on the Overlays page`);
  const t = await tab();
  await t.cmd("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 1, mobile: false });
  await t.cmd("Emulation.setDefaultBackgroundColorOverride", { color: { r: 70, g: 80, b: 100, a: 255 } });
  await t.cmd("Page.navigate", { url: row.cells[5] });
  check(await t.until("!!window.streamerz"), `${group} opens with the script`);
  // Its feed is open once Streamerz counts the page as in use.
  let used = false;
  for (let i = 0; i < 50 && !used; i++) {
    used = (await view()).find(r => r.id === row.id).cells[4] !== "Not open";
    if (!used) await sleep(100);
  }
  return { ...t, id: row.id, group, used, width: w, height: h };
}
const done = async (t, slug) => {
  await t.shot(`/tmp/szoverlay/example-${slug}.png`);
  if (["Subathon", "Top gifters", "Credits roll", "Song request queue"].includes(t.group)) {
    await t.cmd("Emulation.setDeviceMetricsOverride", { width: 360, height: t.height, deviceScaleFactor: 1, mobile: false });
    if (t.group === "Credits roll") await t.js("roll.getAnimations()[0].currentTime = 13000; 1");
    await t.shot(`/tmp/szoverlay/example-${slug}-narrow.png`);
    check(await t.js("document.documentElement.scrollWidth <= innerWidth"), `${t.group} fits a narrow source`);
  }
  check(t.errors.length === 0, `${t.group} has no page errors ${t.errors.join(" | ")}`);
  await t.close();
};

try {
  // A dev instance starts with the HTTP server off: the pages are served by it.
  await ask({ request: "StreamerzSave", what: "server_http", item: { autoStart: true, address: "127.0.0.1", port: 7495 } });
  const examples = (await ask({ request: "StreamerzCoreView", domain: "overlay_examples" })).tables[0].rows;
  check(examples.length === 19, "examples listed: " + examples.map(r => r.id).join(", "));
  for (const e of examples) check((await act("add_example", e.id, "overlay_examples")).status === "ok", `example ${e.id} added`);
  const again = (await ask({ request: "StreamerzCoreView", domain: "overlay_examples" })).tables[0].rows;
  check(again.every(r => r.cells[2] === "Added" && r.can.add_example === false), "every example says it is here");
  // Only the examples with actions are groups of actions, each connected to its overlay.
  const groups = await ask({ request: "StreamerzActionGroups" });
  check(JSON.stringify(groups.groups) === JSON.stringify(["Buttons dock", "Poll", "Prize wheel", "Subathon", "Top gifters"]), "only the ones with actions are groups: " + JSON.stringify(groups.groups));
  check(groups.overlays.length === examples.length, "and every one is an overlay");
  const rows = await view();
  check(rows.find(r => r.cells[1] === "Prize wheel").cells[3].text.includes("1 action"), "the Overlays page says what is connected");
  check(rows.find(r => r.cells[1] === "Clock").cells[3] === "", "and nothing for an overlay alone");
  for (const p of ["twitch", "kick", "youtube", "tiktok"]) await ask({ request: "SimulateChatConnection", platform: p, connected: true });
  const test = async (t, times = 1) => {
    for (let i = 0; i < times; i++) check((await act("test", t.id)).status === "ok", `Test is sent to ${t.group}`);
  };

  // Clock: settings only.
  await values("Clock", { seconds: false, label: "" });
  let t = await open("Clock", 420, 160);
  check(await t.until("/^\\d\\d:\\d\\d$/.test(time.textContent)"), "clock shows the time: " + await t.js("time.textContent"));
  await values("Clock", { seconds: true, label: "Local time", color: "#ffd166" });
  check(await t.until("/^\\d\\d:\\d\\d:\\d\\d$/.test(time.textContent) && label.textContent === 'Local time'"), "clock follows its settings without a reload");
  await test(t);
  await done(t, "clock");

  // Now playing: the song of the Spotify app on this computer when it runs (cover, length, place),
  // then the made-up one, then the other layouts.
  t = await open("Now playing", 760, 220);
  check(t.used, "now playing counts as in use");
  await values("Now playing", { hideWhenPaused: false });
  const real = await t.until("!card.classList.contains('away') && title.textContent.length > 0", 4000);
  if (real) {
    const seen = await t.js("JSON.stringify({title: title.textContent, artist: artist.textContent, cover: !!cover.style.backgroundImage, end: end.textContent, now: now.textContent})");
    console.log("     Spotify here says: " + seen);
    check(JSON.parse(seen).cover && /\d+:\d\d/.test(JSON.parse(seen).end), "the real song comes with its cover and length");
    await sleep(1500);
    await t.shot("/tmp/szoverlay/example-now-playing-real.png");
  } else {
    console.log("     (Spotify is not running here: the real song was not tried)");
  }
  await test(t);
  check(await t.until("!card.classList.contains('away') && title.textContent === 'A test song' && artist.textContent === 'Streamerz'"), "the test song shows");
  check(await t.until("cover.style.backgroundImage.includes('data:image') && end.textContent === '3:35'"), "with its cover and its length");
  const before = await t.js("now.textContent");
  await sleep(2200);
  check(await t.js("now.textContent") !== before, "the time counts on by itself: " + before + " then " + await t.js("now.textContent"));
  await t.shot("/tmp/szoverlay/example-now-playing-card.png");
  await values("Now playing", { layout: "Vinyl", side: "Right", background: "Solid color", backgroundColor: "#1e1b4bf2", label: "On air", showAlbum: true, radius: 40 });
  check(await t.until("stage.classList.contains('vinyl') && stage.classList.contains('right') && label.textContent === 'On air'"), "now playing follows its settings");
  await sleep(700);
  await t.shot("/tmp/szoverlay/example-now-playing-vinyl.png");
  await values("Now playing", { layout: "Compact", side: "Left", background: "None", label: "", showAlbum: false, scale: 130 });
  await sleep(700);
  await t.shot("/tmp/szoverlay/example-now-playing-compact.png");
  await values("Now playing", { layout: "Card", background: "Blurred cover", scale: 100, radius: 18, hideWhenPaused: true });
  await ask({ request: "StreamerzOverlayPublish", topic: "song", value: { title: "Real song", artist: "Real artist", playing: false } });
  check(await t.until("card.classList.contains('away')"), "it leaves when the music stops");
  await done(t, "now-playing");

  // Chat: lines as the Chat page draws them, at once, and nothing a chatter types read as HTML.
  t = await open("Chat overlay", 520, 560);
  await t.js("window.seen = {}; new MutationObserver(() => { for (const l of chat.children) seen[l.textContent] ||= Date.now(); }).observe(chat, {childList: true}); 1");
  const sent = Date.now();
  await ask({ request: "InjectChat", platform: "twitch", user: "ana", userId: "1", text: "hello <b>there</b> :)" });
  check(await t.until("chat.children.length >= 1"), "a chat line arrives");
  const took = (await t.js("Math.min(...Object.values(seen))")) - sent;
  check(took < 250, `it is on the page ${took} ms after it was said`);
  await ask({ request: "InjectChat", platform: "youtube", user: "bruno", userId: "2", text: "good stream today" });
  await ask({ request: "InjectChat", platform: "kick", user: "caio", userId: "3", role: "moderator", text: "first time here, this is great" });
  await ask({ request: "InjectChat", platform: "tiktok", user: "duda", userId: "4", role: "subscriber", text: "love it 😀" });
  await ask({ request: "InjectChat", platform: "twitch", user: "ana", userId: "1", text: "!hidden command" });
  check(await t.until("chat.children.length >= 4"), "chat lines of every platform arrive: " + await t.js("chat.children.length"));
  check(await t.js("!chat.querySelector('b') && chat.textContent.includes('<b>there</b>')"), "what a chatter types is shown as text");
  check(await t.js("!chat.textContent.includes('!hidden')"), "a command is left out");
  check(await t.js("chat.querySelectorAll('.sz-platform').length >= 4 && !!chat.querySelector('.sz-badge')"), "with the platform's logo and the badges");
  check(await t.js("!!chat.querySelector('.sz-emote')"), "an emoji is drawn as the Chat page draws it");
  await test(t, 3);
  check(await t.until("!!chat.querySelector('.sz-event')"), "an event shows among the messages");
  await sleep(600);
  await t.shot("/tmp/szoverlay/example-chat-bubbles.png");
  await values("Chat overlay", { look: "Cards", showAvatars: true, showTime: true, max: 4 });
  check(await t.until("document.body.classList.contains('cards') && chat.children.length === 4"), "only as many messages as the setting says stay");
  await test(t, 2);
  await sleep(600);
  await t.shot("/tmp/szoverlay/example-chat-cards.png");
  await values("Chat overlay", { look: "Bubbles", showAvatars: false, showTime: false, max: 15, platforms: ["youtube"] });
  await ask({ request: "InjectChat", platform: "twitch", user: "ana", userId: "1", text: "only on twitch" });
  await ask({ request: "InjectChat", platform: "youtube", user: "bruno", userId: "2", text: "only on youtube" });
  check(await t.until("chat.textContent.includes('only on youtube')"), "a platform that is ticked shows");
  check(!(await t.js("chat.textContent.includes('only on twitch')")), "one that is not stays out");
  await values("Chat overlay", { platforms: ["twitch", "youtube", "kick", "tiktok"] });
  await done(t, "chat");

  // Flying chat.
  t = await open("Flying chat", 1280, 720);
  for (const [i, text] of ["this flies", "so does this one, a little longer", "gg", "what a play", "hello from youtube", "first"].entries())
    await ask({ request: "InjectChat", platform: ["twitch", "youtube", "kick", "tiktok"][i % 4], user: "viewer" + i, userId: "f" + i, text });
  await test(t, 2);
  check(await t.until("sky.querySelectorAll('.fly').length >= 6"), "messages fly: " + await t.js("sky.querySelectorAll('.fly').length"));
  const lanes = await t.js("new Set([...sky.children].map(e => e.style.top)).size");
  check(lanes >= 3, "on different lanes: " + lanes);
  await sleep(2500);
  check(await t.js("[...sky.children].some(e => e.getBoundingClientRect().left < innerWidth)"), "and they move across");
  await done(t, "danmaku");

  // Alerts: a card for each kind of event, from every platform.
  t = await open("Alerts", 700, 620);
  await test(t, 5);
  check(await t.until("list.querySelectorAll('.card:not(.old)').length === 5"), "five alerts, five cards: " + await t.js("list.children.length"));
  check(await t.js("new Set([...list.querySelectorAll('.logo')].map(l => l.className)).size >= 2"), "of several platforms");
  check(await t.js("!!list.querySelector('.face img')"), "with the picture of who did it");
  await sleep(900);
  await t.shot("/tmp/szoverlay/example-alerts-colors.png");
  await values("Alerts", { look: "Dark", side: "Right", gifts: false });
  await test(t, 8);
  await sleep(900);
  check(await t.js("document.body.classList.contains('dark') && ![...list.querySelectorAll('.card:not(.old) .what')].some(w => /gifted|sent Rose/.test(w.textContent))"), "a kind that is switched off stays out");
  await values("Alerts", { look: "Platform colors", side: "Left", gifts: true });
  await done(t, "alerts");

  // TikTok gifts: one pill for a combo, its count growing.
  t = await open("TikTok gifts", 760, 520);
  await test(t, 3);
  check(await t.until("list.children.length === 1 && list.querySelector('.count').textContent === 'x3'"), "a combo is one pill that counts up: " + await t.js("list.textContent"));
  await test(t, 3);
  check(await t.until("list.querySelectorAll('.gift:not(.old)').length === 2 && list.firstChild.querySelector('.count').textContent === 'x1'"), "the next gift gets its own: " + await t.js("list.textContent"));
  await sleep(700);
  await done(t, "tiktok-gifts");

  // Poll: Test shows a made-up poll with votes; the command starts a real one; the dock sees it.
  t = await open("Poll", 700, 520);
  const dock = await tab();
  const dockRow = (await view()).find(r => r.cells[1] === "Poll" && /dock/i.test(r.cells.join(" ")) && r.cells[5].includes("dock.html"));
  check(!!dockRow, "the poll has a dock page");
  await dock.cmd("Emulation.setDeviceMetricsOverride", { width: 340, height: 520, deviceScaleFactor: 1, mobile: false });
  await dock.cmd("Page.navigate", { url: dockRow.cells[5] });
  await dock.until("!!window.streamerz");
  await test(t);
  check(await t.until("question.textContent === 'What should I have for dinner?' && choices.children.length === 4"), "the made-up poll shows");
  check(await t.until("choices.children[2].querySelector('.label span').textContent === '40% (4)'"), "with its votes counted: " + await t.js("choices.textContent"));
  await sleep(700);
  await t.shot("/tmp/szoverlay/example-poll.png");
  await ask({ request: "GetChatOut" });
  await ask({ request: "InjectChat", platform: "twitch", user: "streamer", userId: "b1", role: "broadcaster", text: "!poll Best pet? | Cat | Dog" });
  check(await t.until("question.textContent === 'Best pet?' && choices.children.length === 2"), "!poll starts one from the chat");
  await ask({ request: "InjectChat", platform: "twitch", user: "ana", userId: "1", text: "2" });
  await ask({ request: "InjectChat", platform: "youtube", user: "bruno", userId: "2", text: "2" });
  await ask({ request: "InjectChat", platform: "kick", user: "caio", userId: "3", text: "1" });
  await ask({ request: "InjectChat", platform: "kick", user: "caio", userId: "3", text: "hello, not a vote" });
  check(await t.until("choices.children[1].querySelector('.label span').textContent === '67% (2)'"), "typing a number votes, once per person");
  check(await dock.until("rows.children.length === 2 && rows.textContent.includes('67% (2)')"), "the dock follows it: " + await dock.js("document.getElementById('state').textContent"));
  await dock.shot("/tmp/szoverlay/example-poll-dock.png");
  await ask({ request: "InjectChat", platform: "twitch", user: "streamer", userId: "b1", role: "broadcaster", text: "!endpoll" });
  check(await t.until("document.getElementById('poll').classList.contains('over') && choices.children[1].classList.contains('won')"), "!endpoll ends it and the winner stands out");
  await sleep(300);
  const said = JSON.stringify((await ask({ request: "GetChatOut" })).messages);
  check(said.includes("Best pet?") && said.includes("Dog won with 2 of 3 votes"), "the start and the result are written in the chat");
  await dock.js("question.value = 'From the dock?'; document.getElementById('options').value = 'Yes\\nNo'; seconds.value = 30; start.click(); 1");
  check(await t.until("question.textContent === 'From the dock?'"), "the dock starts one too");
  check(dock.errors.length === 0, "the dock has no page errors " + dock.errors.join(" | "));
  await dock.close();
  await done(t, "poll-running");

  // Prize wheel: !spin turns it and the result is written in the chat.
  t = await open("Prize wheel", 800, 900);
  await values("Prize wheel", { spinSeconds: 2, slices: [{ label: "Red", weight: 1 }, { label: "Green", weight: 1 },
    { label: "Blue", weight: 1 }, { label: "Gold", weight: 3 }] });
  check(await t.until("wheel.querySelectorAll('path').length === 4"), "the slices are the rows of the list");
  check(await t.js("Math.round(slices[3].to - slices[3].from)") === 180, "a slice with 3 chances is as wide as three");
  await ask({ request: "GetChatOut" });
  await ask({ request: "InjectChat", platform: "twitch", user: "ana", userId: "1", text: "!spin" });
  check(await t.until("!stage.classList.contains('away') && wheel.style.transform.includes('rotate')"), "!spin turns the wheel");
  check(await t.until("result.classList.contains('on') && ['Red', 'Green', 'Blue', 'Gold'].includes(result.textContent)", 6000), "it lands on a slice: " + await t.js("result.textContent"));
  const got = await t.js("result.textContent");
  await sleep(300);
  check(JSON.stringify((await ask({ request: "GetChatOut" })).messages).includes("ana spun the wheel: " + got), "and says so in the chat");
  await done(t, "wheel");

  // Chat dock: reads the chat and sends to it.
  t = await open("Chat dock", 380, 520);
  await ask({ request: "InjectChat", platform: "twitch", user: "ana", userId: "1", text: "can you read this in the dock?" });
  check(await t.until("[...chat.children].some(l => l.textContent.includes('in the dock'))"), "the dock shows the chat");
  await test(t, 2);
  await t.js("text.value = 'hi from the dock'; bar.requestSubmit(); 1");
  check(await t.until("text.value === '' && note.textContent === ''"), "the dock sends a message: " + await t.js("note.textContent"));
  await t.js("chat.querySelector('.sz-name').click(); 1");
  check(await t.js("text.value.startsWith('@')"), "a click on a name starts an answer");
  await done(t, "chat-dock");

  // Viewer counter: each platform and the total, following the topic.
  await ask({ request: "SimulateChatViewers", platform: "twitch", count: 120 });
  await ask({ request: "SimulateChatViewers", platform: "kick", count: 30 });
  t = await open("Viewer counter", 900, 160);
  check(await t.until("items.total.classList.contains('on') && items.total.textContent.includes('150')", 4000), "the total of every platform: " + await t.js("row.textContent"));
  await values("Viewer counter", { platforms: ["kick"] });
  check(await t.until("!items.twitch.classList.contains('on') && items.kick.classList.contains('on')"), "only the platforms ticked are shown");
  await values("Viewer counter", { platforms: ["twitch", "youtube", "kick", "tiktok"] });
  await sleep(900);
  await done(t, "viewers");

  // Ad break: Test plays a made-up ad.
  t = await open("Ad break", 560, 140);
  check(await t.js("box.classList.contains('away')"), "no ad, no badge");
  await test(t);
  check(await t.until("!box.classList.contains('away') && /^0:\\d\\d$/.test(time.textContent)"), "an ad shows the time left: " + await t.js("time.textContent"));
  await sleep(600);
  await done(t, "ad");

  // Goal bar: follows count, and the count is kept in its setting; what Test sends is not kept.
  await values("Goal bar", { target: 4, current: 1, title: "{current} of {target} follows" });
  t = await open("Goal bar", 900, 140);
  check(await t.until("title.textContent === '1 of 4 follows'"), "the goal starts where its setting says: " + await t.js("title.textContent"));
  // A follow as Twitch sends it, and one from Kick the goal is told to leave out.
  await values("Goal bar", { platforms: ["twitch"] });
  for (const id of ["g1", "g2"]) await ask({ request: "FireTrigger", trigger: 101, args: { user: "fan_" + id, userId: id } });
  await ask({ request: "FireTrigger", trigger: 35011, args: { user: "kicker", userId: "k1" } });
  check(await t.until("title.textContent === '3 of 4 follows'"), "each follow adds one: " + await t.js("title.textContent"));
  await sleep(300);
  check((await ask({ request: "StreamerzGroupVariables", group: "Goal bar" })).values.current === 3, "only from the platforms ticked, and the count is kept");
  for (let i = 0; i < 10; i++) await act("test", t.id);
  await sleep(500);
  check((await ask({ request: "StreamerzGroupVariables", group: "Goal bar" })).values.current === 3, "what Test sends is shown, never kept");
  await values("Goal bar", { current: 4 });
  check(await t.until("goal.classList.contains('done') && reached.textContent === 'Goal reached!'"), "reaching it is celebrated");
  await sleep(500);
  await done(t, "goal");

  // Starting soon: its picture is a file of the overlay, given to the page as an address.
  t = await open("Starting soon", 1280, 720);
  check(await t.until("picture.style.backgroundImage.includes('/overlays/Starting%20soon/img/background.svg')"), "the picture setting is an address: " + await t.js("picture.style.backgroundImage"));
  check(await t.js("fetch(streamerz.settings.background).then(r => r.status)") === 200, "and the picture is served");
  check(await t.until("/^\\d+:\\d\\d$/.test(clock.textContent)"), "the countdown runs: " + await t.js("clock.textContent"));
  await sleep(500);
  await done(t, "starting-soon");

  // Buttons dock: a button for each connected action; a press shows the banner.
  t = await open("Buttons dock", 1280, 200, "banner.html");
  const buttons = await tab();
  const dockPage = (await view()).find(r => r.cells[1] === "Buttons dock" && r.cells[5].includes("dock.html"));
  await buttons.cmd("Emulation.setDeviceMetricsOverride", { width: 340, height: 300, deviceScaleFactor: 1, mobile: false });
  await buttons.cmd("Page.navigate", { url: dockPage.cells[5] });
  check(await buttons.until("grid.querySelectorAll('button').length === 4"), "a button for each action: " + await buttons.js("grid.textContent"));
  const brb = (await ask({ request: "StreamerzOverlay", op: "read", overlay: "Buttons dock" })).status === "ok";
  check(brb, "the dock's overlay reads by request");
  await values("Buttons dock", { main: "a0000010-5a5a-4000-8000-000000000003" });
  check(await buttons.until("grid.firstChild.classList.contains('main') && grid.firstChild.textContent === 'Banner: Thanks for watching'"), "the big button is the action picked");
  await buttons.js("[...grid.children].find(b => b.textContent === 'Banner: Be right back').click(); 1");
  check(await t.until("!banner.classList.contains('away') && banner.textContent === 'Be right back'"), "a press runs its action and the banner shows");
  await buttons.shot("/tmp/szoverlay/example-control-dock.png");
  await buttons.js("[...grid.children].find(b => b.textContent === 'Banner: hide').click(); 1");
  check(await t.until("banner.classList.contains('away')"), "and another hides it");
  check(buttons.errors.length === 0, "the dock has no page errors " + buttons.errors.join(" | "));
  await buttons.close();
  await test(t);
  await done(t, "control-dock");

  // Subathon: real actions change its persisted clock; Test stays on the page.
  t = await open("Subathon", 700, 200);
  await values("Subathon", { start: 10000, max: 20000, subscription: 1000, donation: 1000 });
  await ask({ request: "DoAction", action: { name: "Subathon: reset" } });
  check(await t.until("timer.textContent === '00:00:10'"), "reset starts the bundled Subathon clock");
  await ask({ request: "FireTrigger", trigger: 106, args: { user: "gifter", userId: "sub-1", gifts: 3 } });
  check(await t.until("timer.textContent === '00:00:13'"), "a community gift adds one unit per sub");
  await values("Subathon", { color: "#ffd166", font: "sans-serif", label: "Time left" });
  check(await t.until("label.textContent === 'Time left' && getComputedStyle(timer).color === 'rgb(255, 209, 102)'"), "Subathon look changes live");
  await test(t);
  check(await t.until("timer.textContent === '01:02:05'"), "Test previews a clock");
  await done(t, "subathon");

  // Emote wall: actual chat parts, a per-message cap and a total cap; moderation removes images.
  t = await open("Emote wall", 1280, 720);
  await values("Emote wall", { most: 10, perMessage: 4, seconds: 2 });
  await test(t);
  check(await t.until("wall.children.length === 4"), "only the allowed emotes per message appear");
  check(await t.js("[...wall.children].every(e => e.tagName === 'IMG' && e.alt === 'Test emote')"), "emotes use the chat's resolved pictures");
  await test(t, 3);
  check(await t.js("wall.children.length <= 10"), "the wall never exceeds its image cap");
  await sleep(400);
  check(await t.js("wall.firstChild && wall.firstChild.getBoundingClientRect().top < innerHeight"), "emotes move up the screen");
  await t.shot("/tmp/szoverlay/example-emote-wall.png");
  await t.cmd("Emulation.setDeviceMetricsOverride", { width: 360, height: 720, deviceScaleFactor: 1, mobile: false });
  await t.shot("/tmp/szoverlay/example-emote-wall-narrow.png");
  await ask({ request: "StreamerzOverlay", op: "test", overlay: "Emote wall", events: [["Chat.Removed", { platform: "twitch", msgId: "wall-test" }]] });
  check(await t.until("wall.children.length === 0"), "deleting the message removes its emotes");
  await values("Emote wall", { most: 80, perMessage: 12, seconds: 8 });
  await done(t, "emote-wall-empty");

  // Gifters: actions counted even with the page closed, same names remain separate by ID.
  await values("Top gifters", { period: 100 });
  for (const [id, count] of [["board-1", 5], ["board-2", 2]])
    await ask({ request: "FireTrigger", trigger: 106, args: { user: "same <b>name</b>", userId: id, gifts: count } });
  t = await open("Top gifters", 620, 560);
  check(await t.until("rows.children.length === 2 && rows.firstChild.querySelector('.count').textContent === '5'"), "gift totals grow with the page closed");
  check(await t.js("!rows.querySelector('b') && rows.textContent.includes('<b>name</b>')"), "gifter names are text");
  await ask({ request: "FireTrigger", trigger: 105, args: { user: "same <b>name</b>", userId: "board-1", fromGiftBomb: true } });
  await sleep(150);
  check(await t.js("rows.firstChild.querySelector('.count').textContent === '5'"), "individual notifications of a gift bomb are not counted twice");
  await test(t);
  check(await t.until("rows.firstChild.querySelector('.count').textContent === '25'"), "Test previews the leaderboard");
  check((await ask({ request: "StreamerzGroupVariables", group: "Top gifters" })).values["score_100_twitch_board-1"] === 5, "Test does not store made-up totals");
  await done(t, "top-gifters");
  await values("Top gifters", { period: 101 });
  t = await open("Top gifters", 620, 560);
  check(await t.until("rows.children.length === 0"), "a new period starts an empty board");
  await values("Top gifters", { period: 100 });
  check(await t.until("rows.children.length === 2"), "previous periods remain available");
  await done(t, "top-gifters-real");

  // Credits: one HTTP read, selected sections, a bounded animation and names treated as text.
  await ask({ request: "TestCredits" });
  t = await open("Credits roll", 700, 720);
  check(await t.until("document.querySelectorAll('#roll .name').length > 0"), "credits read the real GetCredits response");
  check(await t.js("performance.getEntriesByType('resource').filter(r => r.name.endsWith('/GetCredits')).length === 1"), "credits are read once");
  await test(t);
  check(await t.until("roll.textContent.includes('mod_one')"), "Test previews the chosen credits sections");
  await t.js("roll.getAnimations()[0].currentTime = 13000; 1");
  await done(t, "credits");
  await values("Credits roll", { speed: 120, repeat: true, sections: [{ key: "events.follows", title: "Our followers" }] });
  t = await open("Credits roll", 700, 720);
  await test(t);
  check(await t.until("roll.querySelectorAll('h2').length === 2 && roll.textContent.includes('Our followers')"), "credits show only selected sections");
  check(await t.js("roll.getAnimations()[0].effect.getTiming().iterations === Infinity"), "credits repeat only when asked");
  await done(t, "credits-selected");

  // Queue: receives the current topic and live changes from another template, with no polling.
  await values("Song request queue", { topic: "test.queue" });
  await ask({ request: "StreamerzOverlayPublish", topic: "test.queue", value: [{ title: "<b>A song</b>", artist: "Artist", user: "ana" }, { title: "Removed", skip: true }] });
  t = await open("Song request queue", 700, 520);
  check(await t.until("rows.children.length === 1 && rows.textContent.includes('ana')"), "queue opens with the last published value and leaves out removed requests");
  check(await t.js("!rows.querySelector('b') && rows.textContent.includes('<b>A song</b>')"), "song names are text");
  await ask({ request: "StreamerzOverlayPublish", topic: "test.queue", value: [] });
  check(await t.until("rows.children.length === 0 && !empty.hidden"), "an empty queue clears the page live");
  await values("Song request queue", { topic: "songrequests.queue" });
  await t.close();
  t = await open("Song request queue", 700, 520);
  await test(t);
  check(await t.until("rows.children.length === 2 && rows.firstChild.textContent.includes('A test song')"), "the bundled queue has its own test data");
  await values("Song request queue", { showRequester: false, rows: 1 });
  check(await t.until("rows.children.length === 1 && !rows.textContent.includes('Requested by')"), "queue settings change live");
  await done(t, "song-queue");

  // Without a browser of its own: a picture of a page, with Test, as an assistant takes one.
  const shot = await ask({ request: "StreamerzOverlay", op: "shot", overlay: "Alerts", test: 3, waitMs: 2000, path: "/tmp/szoverlay/shot-alerts.png" });
  check(shot.status === "ok" && shot.bytes > 5000 && shot.errors.length === 0, "a picture of a page by request: " + JSON.stringify(shot).slice(0, 200));

  // Working on a page: a saved file loads the open page again, and the page's own error is in the log
  // with the line of the file it is on.
  t = await open("Clock", 420, 160);
  await t.js("window.mark = 1");
  const clock = (await ask({ request: "StreamerzGroupMeta", group: "Clock", files: true })).meta;
  const page = clock.files.find(f => f.name === "index.html");
  const original = page.content;
  const broken = Buffer.from(original, "base64").toString().replace("</body>", "<script>\nnotThere();\n</script>\n</body>");
  page.content = Buffer.from(broken).toString("base64");
  check((await ask({ request: "StreamerzSetGroupMeta", group: "Clock", meta: clock })).status === "ok", "the clock's page is saved with a mistake in it");
  check(await t.until("window.mark !== 1 && !!window.streamerz", 6000), "a saved file loads the open page again");
  const line = broken.slice(0, broken.indexOf("notThere()")).split("\n").length;
  let logged = "";
  for (let i = 0; i < 30 && !logged.includes("notThere is not defined"); i++) {
    await sleep(100);
    logged = JSON.stringify(await ask({ request: "GetLog", lines: 60 }));
  }
  check(logged.includes("Overlay Clock: ") && logged.includes("notThere is not defined (line " + line + ")"),
    "the page's error is in the log with its line (" + line + "): " + (logged.match(/Overlay Clock: [^"]*/) || [""])[0]);
  page.content = original;
  await ask({ request: "StreamerzSetGroupMeta", group: "Clock", meta: clock });
  check(await t.until("/^\\d\\d:\\d\\d/.test(document.getElementById('time')?.textContent || '')", 6000), "and saved right again, it works again");
  await t.close();

  // The guide, from Streamerz itself.
  t = await tab();
  await t.cmd("Page.navigate", { url: HTTP + "/overlays/-/guide" });
  check(await t.until("document.querySelectorAll('h2').length >= 8 && document.querySelectorAll('table').length >= 5 && nav.children.length >= 8"), "the guide is drawn as a page: " + await t.js("document.querySelectorAll('h2').length + ' parts, ' + document.querySelectorAll('table').length + ' tables'"));
  check(await t.js("!document.body.innerText.includes('**') && [...document.querySelectorAll('td')].some(td => td.textContent.includes('streamerz.chat('))"), "with its text and tables read right");
  await t.shot("/tmp/szoverlay/guide.png");
  check(t.errors.length === 0, "the guide has no page errors " + t.errors.join(" | "));
  await t.close();

  // More pages than a browser allows plain connections to one address (six): all of them stay live.
  const many = [];
  for (let i = 0; i < 8; i++) many.push(await open("Chat overlay", 300, 200));
  await sleep(1000);
  await ask({ request: "InjectChat", platform: "twitch", user: "ana", userId: "1", text: "to every page at once" });
  let live = 0;
  for (const page of many) live += (await page.until("chat.textContent.includes('to every page at once')", 3000)) ? 1 : 0;
  check(live === 8, `eight pages open at once all get a message: ${live}`);
  await Promise.all(many.map(page => page.close()));
} finally {
  chrome.kill();
  sz.close();
}
console.log(fails.length ? `\n${fails.length} failed` : "\nall passed");
process.exit(fails.length ? 1 : 0);
