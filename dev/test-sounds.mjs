// Sounds on this computer's real output, through a dev instance (WebSocket 8095, started with
// STREAMERZ_DEV_PORT=8095 and its own XDG_CONFIG_HOME): a file plays in parallel and one after another,
// through actions with Play Sound as a person sets them up, and through the StreamerzSound request.
// It hears nothing itself: it reads what Streamerz says is playing, what PipeWire shows (pactl) and what
// the Sound Started and Sound Ended actions wrote. With STREAMERZ_PID it also says what playing costs.
//   node dev/test-sounds.mjs ~/Downloads/test.mp3
// The file is played at a quarter of its volume for about a minute.
import { execSync } from "node:child_process";
import { readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";

const FILE = process.argv[2];
const LOG = tmpdir() + "/streamerz-test-sounds.log";
if (!FILE) { console.log("usage: node dev/test-sounds.mjs <a sound file of 10 seconds or more>"); process.exit(2); }
const sleep = ms => new Promise(r => setTimeout(r, ms));
const fails = [];
const check = (ok, what) => { console.log((ok ? "ok   " : "FAIL ") + what); if (!ok) fails.push(what); };
rmSync(LOG, { force: true });

const ws = new WebSocket("ws://127.0.0.1:8095/");
let n = 0; const waiting = {};
ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id && waiting[d.id]) { waiting[d.id](d); delete waiting[d.id]; } };
await new Promise(r => ws.onopen = r);
const ask = req => new Promise(r => { const id = "r" + ++n; waiting[id] = r; ws.send(JSON.stringify({ ...req, id })); });
const playing = async () => (await ask({ request: "StreamerzSound", op: "list" }));
const streams = () => {
  try { return (execSync("pactl list sink-inputs").toString().match(/application.name = "Streamerz"/g) || []).length; } catch { return -1; }
};
const lines = () => { try { return readFileSync(LOG, "utf8").trim().split("\n").filter(Boolean); } catch { return []; } };
const play = (name, wait) => ({ type: 1, soundFile: FILE, soundName: name, volume: 25, finishBeforeContinuing: wait });
const note = text => ({ type: 3, file: LOG, text: text + "\n", append: true });

const actions = [
  { id: "a-par", name: "Parallel", subActions: [play("p1", false), play("p2", false), play("p3", false), note("parallel started")] },
  { id: "a-seq", name: "Sequential", subActions: [play("s1", true), note("between"), play("s2", true), note("sequence done")] },
  { id: "a-on", name: "On start", triggers: [{ type: 99300 }], subActions: [note("started %soundName% %soundLengthMs%")] },
  { id: "a-off", name: "On end", triggers: [{ type: 99301 }], subActions: [note("ended %soundName% %reason%")] },
];
for (const a of actions) {
  const item = { enabled: true, queue: "", group: "", ...a, triggers: (a.triggers || []).map((t, i) => ({ id: a.id + "-t" + i, enabled: true, ...t })),
    subActions: a.subActions.map((s, i) => ({ id: a.id + "-s" + i, enabled: true, weight: 0, index: i, parentId: null, ...s })) };
  const r = await ask({ request: "StreamerzSave", what: "action", item });
  check(r.status === "ok", "saved " + a.name + " " + (r.error || ""));
}
const info = await ask({ request: "StreamerzSound", op: "info", file: FILE });
check(info.status === "ok" && info.durationMs >= 10000, "the file is read: " + JSON.stringify(info));
try {
  const probed = Math.round(Number(execSync(`ffprobe -v error -show_entries format=duration -of csv=p=0 "${FILE}"`)) * 1000);
  check(Math.abs(info.durationMs - probed) < 60, `its length is ffprobe's: ${info.durationMs} ms, ffprobe ${probed} ms`);
} catch { console.log("     (no ffprobe here: the length was not compared)"); }
const LEN = info.durationMs;

// In parallel: three at once, one output stream mixing them.
let t0 = Date.now();
await ask({ request: "DoAction", action: { name: "Parallel" } });
await sleep(1500);
let now = await playing();
const names = now.playing.map(p => p.name).sort().join(",");
check(names === "p1,p2,p3", "three play at once: " + names);
check(now.playing.every(p => p.positionMs > 500 && p.positionMs < 3000), "and they move: " + now.playing.map(p => p.positionMs).join(", "));
check(streams() === 1 && now.openOutputs === 1, "mixed into one stream on the output (PipeWire shows " + streams() + ")");
check(lines().includes("parallel started") && lines().filter(l => new RegExp(`^started p\\d ${LEN}$`).test(l)).length === 3, "the action went on at once and Sound Started fired three times: " + JSON.stringify(lines()));
await sleep(2000);
check((await ask({ request: "StreamerzSound", op: "stop", name: "p2" })).count === 1, "one of them is stopped by name");
await sleep(300);
now = await playing();
check(now.playing.map(p => p.name).sort().join(",") === "p1,p3", "the other two play on");
check((await ask({ request: "StreamerzSound", op: "stop", fadeMs: 800 })).count === 2, "the rest fade out");
await sleep(1500);
now = await playing();
check(now.playing.length === 0, "nothing plays after the fade");
check(lines().filter(l => /^ended p\d stopped$/.test(l)).length === 3, "Sound Ended said stopped for all three");
await sleep(500);
check((await playing()).openOutputs === 0 && streams() === 0, "the output is closed again (PipeWire shows " + streams() + ")");

// One after another: the second starts when the first ends.
rmSync(LOG, { force: true });
t0 = Date.now();
await ask({ request: "DoAction", action: { name: "Sequential" } });
const seen = [];
for (let s = 0; s < (2 * LEN) / 1000 + 6 && !lines().includes("sequence done"); s++) {
  await sleep(1000);
  const p = (await playing()).playing.map(x => x.name).join(",");
  seen.push(p);
}
const took = (Date.now() - t0) / 1000;
console.log("     what played each second: " + seen.join(" | "));
check(seen.every(p => p === "s1" || p === "s2" || p === ""), "never both at once");
check(seen.indexOf("s2") > seen.lastIndexOf("s1"), "s2 only after s1");
check(took > (2 * LEN) / 1000 - 1 && took < (2 * LEN) / 1000 + 4, `two sounds of ${(LEN / 1000).toFixed(1)} s took ${took.toFixed(1)} s`);
// A trigger's action runs from the queue, beside the action that played the sound (as in Streamer.bot):
// what is ordered is the sounds themselves and the action's own steps.
const at = l => lines().indexOf(l);
check(lines().length === 6 && at(`started s1 ${LEN}`) < at("ended s1 finished") && at("ended s1 finished") < at(`started s2 ${LEN}`)
  && at(`started s2 ${LEN}`) < at("ended s2 finished") && at("between") < at(`started s2 ${LEN}`) && at("between") > at(`started s1 ${LEN}`)
  && at("sequence done") > at(`started s2 ${LEN}`), "the order: " + JSON.stringify(lines()));
await sleep(500);
check((await playing()).openOutputs === 0, "and the output closes after");

// Through the request (as sound_play does): several at once, each answer matched to its request.
const ids = [];
for (const k of [1, 2, 3, 4]) {
  const r = await ask({ request: "StreamerzSound", op: "play", file: FILE, name: "r" + k, volume: 15 });
  check(r.status === "ok" && r.soundId > 0 && r.durationMs === LEN, `play ${k} answers with its sound: ${JSON.stringify(r)}`);
  ids.push(r.soundId);
}
check(new Set(ids).size === 4, "four sounds, four ids");
await sleep(1000);
check((await playing()).playing.length === 4 && streams() === 1, "four play at once in one stream");
const pid = process.env.STREAMERZ_PID;
if (pid) {
  const rss = () => Number(/VmRSS:\s+(\d+)/.exec(readFileSync(`/proc/${pid}/status`, "utf8"))[1]);
  const ticks = () => { const f = readFileSync(`/proc/${pid}/stat`, "utf8").split(") ")[1].split(" "); return Number(f[11]) + Number(f[12]); };
  const threads = () => readdirSync(`/proc/${pid}/task`).length;
  const cpu = async ms => { const t = ticks(); await sleep(ms); return ((ticks() - t) / (ms / 1000)).toFixed(1) + "% of a core"; };
  console.log(`     4 playing: ${rss()} kB, ${threads()} threads, ${await cpu(4000)}`);
  await ask({ request: "StreamerzSound", op: "stop" });
  await sleep(1500);
  console.log(`     after:     ${rss()} kB, ${threads()} threads, ${await cpu(3000)}`);
}
check((await ask({ request: "StreamerzSound", op: "stop" })).status === "ok", "stop all");
await sleep(500);
check((await playing()).openOutputs === 0 && streams() === 0, "and the output closes");
ws.close();
console.log(fails.length ? `\n${fails.length} failed` : "\nall passed");
process.exit(fails.length ? 1 : 0);
