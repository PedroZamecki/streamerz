import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";

function fixture() {
  return {
    page: { sections: [
      { card: true, brand: "twitch", headline: "Stream title", state: { text: "Not live", health: "standby" },
        chips: [{ text: "Art" }, { text: "English" }], buttons: ["clip", "marker", "snooze", "ad"].map(id => ({ id: `twitch.${id}`, enabled: false })) },
      { card: true, brand: "kick", headline: "Kick title", state: { text: "Not live", health: "standby" }, chips: [{ text: "IRL" }] },
      { card: true, brand: "youtube", headline: "Next stream", state: { text: "Not live", health: "standby" }, lines: [{ text: "Private: only you can watch it." }] },
    ] },
    twitch: { title: "Stream title", game_name: "Art", tags: ["English"], broadcaster_language: "pt", is_branded_content: true, content_classification_labels: ["Gambling", "MatureGame"] },
    kick: { stream_title: "Kick title", category: { name: "IRL" }, stream: { is_live: false } },
    broadcasts: [{ id: "next", snippet: { title: "Next stream" }, status: { lifeCycleStatus: "ready", privacyStatus: "private" } }],
    video: { snippet: { title: "Next stream", description: "About", tags: ["Music"], categoryId: "10" }, status: { privacyStatus: "private" } },
    forms: {
      "stream_info.twitch": { item: { language: "pt", branded: true, label_politics: false, label_drugs: false,
        label_gambling: true, label_profanity: false, label_sexual: false, label_violence: false } },
      "stream_info.youtube": { item: { description: "About", tags: ["Music"], youtube_category: "10", privacy: "private" } },
    },
  };
}

function run(data) {
  // Exercise the real read-only script with API fixtures. No socket or token is needed.
  const preload = `
    const fixture = ${JSON.stringify(data)};
    globalThis.WebSocket = class {
      constructor() { queueMicrotask(() => this.onopen()); }
      send(raw) {
        const q = JSON.parse(raw);
        if (q.method && q.method !== "GET") throw new Error("a verification script must not write");
        let value = {};
        if (q.request === "StreamerzCoreView") value = fixture.page;
        else if (q.request === "StreamerzEntityForm") value = fixture.forms[q.kind];
        else if (q.request === "StreamerzPlatformApi") {
          let answer;
          if (q.platform === "twitch") answer = {data: q.path === "channels" ? [fixture.twitch] : []};
          else if (q.platform === "kick") answer = {data: [fixture.kick]};
          else answer = {items: q.path === "videos" ? [fixture.video] : fixture.broadcasts};
          value = {answer};
        } else if (q.request !== "StreamerzCoreAction" || q.action !== "refresh") throw new Error("unexpected mutation");
        queueMicrotask(() => this.onmessage({data: JSON.stringify({status: "ok", ...value, id: q.id})}));
      }
      close() {}
    };`;
  return spawnSync(process.execPath, ["--import", `data:text/javascript,${encodeURIComponent(preload)}`, "dev/stream-check.mjs"], { encoding: "utf8", timeout: 10000 });
}

test("platform comparison checks editor metadata without any platform writes", () => {
  const result = run(fixture());
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /Twitch Gambling prefilled/);
  assert.match(result.stdout, /YouTube category prefilled/);
  assert.match(result.stdout, /review skipped platforms and live-only checks separately/);
});

test("platform comparison fails when the editor has stale language or privacy", () => {
  const data = fixture();
  data.forms["stream_info.twitch"].item.language = "en";
  data.forms["stream_info.youtube"].item.privacy = "public";
  const result = run(data);
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /FAIL Twitch language prefilled/);
  assert.match(result.stdout, /FAIL YouTube privacy prefilled/);
});

test("an instance with no connected platforms cannot report success", () => {
  const data = fixture();
  data.page.sections = [];
  const result = run(data);
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /no connected platform was checked/);
});
