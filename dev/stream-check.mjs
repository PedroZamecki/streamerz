// Live check of the Stream page against the platforms themselves: a dev instance (STREAMERZ_DEV=1) with
// the platforms connected. It reads the page, asks each platform directly through `StreamerzPlatformApi`
// (GETs only: nothing changes; YouTube spends 2 or 3 units of quota), and fails where they disagree.
//   node dev/stream-check.mjs [ws://127.0.0.1:8090/]
const WS = process.argv[2] || process.env.STREAMERZ_TEST_WS || process.env.STREAMERZ_URL || "ws://127.0.0.1:8090/";
const fails = [];
let checked = 0;
const check = (ok, what) => { checked++; console.log((ok ? "ok   " : "FAIL ") + what); if (!ok) fails.push(what); };
const skip = what => console.log("skip " + what);

const sz = new WebSocket(WS);
let n = 0; const waiting = {};
sz.onmessage = m => { const d = JSON.parse(m.data); if (d.id && waiting[d.id]) { waiting[d.id](d); delete waiting[d.id]; } };
await new Promise((ok, no) => { sz.onopen = ok; sz.onerror = () => no(new Error(`no Streamerz at ${WS}`)); });
const ask = req => new Promise(r => { const id = "r" + ++n; waiting[id] = r; sz.send(JSON.stringify({ ...req, id })); });
const api = async (platform, path, query) => {
  const r = await ask({ request: "StreamerzPlatformApi", platform, path, query });
  if (r.status !== "ok") throw new Error(r.error);
  return r.answer;
};
const sleep = ms => new Promise(r => setTimeout(r, ms));

// The page, once it has read the platforms (Refresh, then wait for the status line to settle).
await ask({ request: "StreamerzCoreAction", domain: "stream", action: "refresh" });
let page;
for (let i = 0; i < 40; i++) {
  await sleep(500);
  page = await ask({ request: "StreamerzCoreView", domain: "stream" });
  const status = page.sections?.[0]?.rows?.[1]?.status;
  if (!status || status.health !== "busy") break;
}
const cards = Object.fromEntries((page.sections || []).filter(s => s.card).map(s => [s.brand, s]));
const headline = c => (c.headlineMuted ? "" : c.headline || "");
const chips = c => (c.chips || []).map(chip => chip.text);
const live = c => c.state?.text;
const text = c => JSON.stringify(c.lines || []);
console.log("cards:", Object.keys(cards).join(", ") || "none");

if (cards.twitch && cards.twitch.state.health !== "off") {
  const c = cards.twitch;
  const [channel, streams] = await Promise.all([
    api("twitch", "channels", { broadcaster_id: "{broadcaster_id}" }),
    api("twitch", "streams", { user_id: "{broadcaster_id}" }),
  ]);
  const ch = channel.data[0], on = streams.data[0]?.type === "live";
  check(headline(c) === ch.title, `Twitch title: page "${headline(c)}", Twitch "${ch.title}"`);
  check(!ch.game_name || chips(c).includes(ch.game_name), `Twitch category ${ch.game_name || "(none)"} shown`);
  check(ch.tags.every(t => chips(c).includes(t)), `Twitch tags ${ch.tags.join(" ") || "(none)"} shown`);
  check((live(c) === "Live") === on, `Twitch live: page "${live(c)}", Twitch ${on ? "live" : "off air"}`);
  const form = await ask({ request: "StreamerzEntityForm", kind: "stream_info.twitch" });
  check(form.item?.language === ch.broadcaster_language, "Twitch language prefilled");
  if (ch.delay > 0) check(form.item?.delay === ch.delay, "Twitch delay prefilled");
  check(form.item?.branded === ch.is_branded_content, "Twitch branded content prefilled");
  for (const [key, id] of Object.entries({ label_politics: "DebatedSocialIssuesAndPolitics", label_drugs: "DrugsIntoxication",
    label_gambling: "Gambling", label_profanity: "ProfanityVulgarity", label_sexual: "SexualThemes", label_violence: "ViolentGraphic" })) {
    check(form.item?.[key] === (ch.content_classification_labels || []).includes(id), `Twitch ${id} prefilled`);
  }
  const off = (c.buttons || []).filter(b => ["twitch.clip", "twitch.marker", "twitch.snooze", "twitch.ad"].includes(b.id));
  check(off.every(b => (b.enabled === false) === !on), `Twitch clip, marker, snooze and ad ${on ? "on" : "off"} ${on ? "while live" : "off air"}`);
} else skip("Twitch: not connected or not logged in");

if (cards.kick && cards.kick.state.health !== "off") {
  const c = cards.kick;
  const channel = await api("kick", "channels");
  const ch = channel.data[0], on = ch.stream?.is_live === true;
  check(headline(c) === ch.stream_title, `Kick title: page "${headline(c)}", Kick "${ch.stream_title}"`);
  check(!ch.category?.name || chips(c).includes(ch.category.name), `Kick category ${ch.category?.name || "(none)"} shown`);
  check((live(c) === "Live") === on, `Kick live: page "${live(c)}", Kick ${on ? "live" : "off air"}`);
} else skip("Kick: not connected or not logged in");

if (cards.youtube && cards.youtube.state.health !== "off") {
  const c = cards.youtube;
  const list = await api("youtube", "liveBroadcasts", { part: "snippet,status", mine: "true", maxResults: "10" });
  const state = b => b.status.lifeCycleStatus;
  const b = list.items.find(b => ["live", "liveStarting"].includes(state(b)))
    || list.items.filter(b => ["ready", "created", "testing", "testStarting"].includes(state(b)))
      .sort((x, y) => (x.snippet.scheduledStartTime || "~").localeCompare(y.snippet.scheduledStartTime || "~"))[0];
  if (b) {
    const on = ["live", "liveStarting"].includes(state(b));
    const testing = ["testing", "testStarting"].includes(state(b));
    const video = (await api("youtube", "videos", { part: "snippet,status,liveStreamingDetails,statistics", id: b.id })).items[0];
    const form = await ask({ request: "StreamerzEntityForm", kind: "stream_info.youtube" });
    check(headline(c) === video.snippet.title, "YouTube title matches video metadata");
    check(form.item?.description === video.snippet.description, "YouTube description prefilled");
    check(JSON.stringify(form.item?.tags || []) === JSON.stringify(video.snippet.tags || []), "YouTube tags prefilled");
    check(form.item?.youtube_category === video.snippet.categoryId, "YouTube category prefilled");
    check(form.item?.privacy === video.status.privacyStatus, "YouTube privacy prefilled");
    check((live(c) === "Live") === on, `YouTube live: page "${live(c)}", YouTube ${state(b)}`);
    if (testing) check(live(c) === "Testing", `YouTube testing: page "${live(c)}"`);
    const privacy = video.status.privacyStatus;
    check(privacy === "public" || text(c).includes(privacy === "private" ? "Private" : "Unlisted"),
      `YouTube privacy ${privacy} shown when it isn't public`);
  } else check(live(c) !== "Live", "YouTube: no broadcast, and the page doesn't say live");
} else skip("YouTube: not connected or not logged in");

skip("TikTok: verify room details and title changes with the owner checklist");
if (cards.tiktok) console.log(`info TikTok: "${cards.tiktok.headline}" (${live(cards.tiktok)})`);

if (!checked) check(false, "no connected platform was checked");
sz.close();
console.log(fails.length ? `${fails.length} failed` : "connected platform reads agree; review skipped platforms and live-only checks separately");
process.exit(fails.length ? 1 : 0);
