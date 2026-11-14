// Run with node dev/test-subathon-overlay.cjs. No browser or network is needed.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

const html = fs.readFileSync('assets/examples/subathon/files/index.html', 'utf8');
const manifest = JSON.parse(fs.readFileSync('assets/examples/subathon/template.json', 'utf8'));
const settings = Object.fromEntries(manifest.meta.variables.map(v => [v.name, v.default]));
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
assert(!html.includes('WebSocket'), 'the page uses the countdown feed');

function overlay(now) {
  const display = { textContent: '--:--:--' };
  let tick, feed, asked, preview, update;
  const context = {
    document: { getElementById: id => id === 'timer' ? display : {}, documentElement: { style: { setProperty() {} } } },
    Date: { now: () => now.ms },
    setTimeout: fn => { tick = fn; }, clearTimeout: () => { tick = null; },
    streamerz: {
      onSettings: fn => { update = fn; fn(settings); },
      countdown: (id, fn) => { asked = id; feed = fn; },
      on: (_, fn) => { preview = fn; }, testing: false,
    },
  };
  vm.runInNewContext(script, context);
  return { display, tick: () => tick?.(), ticking: () => !!tick, feed: state => feed(state), asked: () => asked,
    preview: state => preview(state), update, context };
}

const now = { ms: 1_000_000 };
const page = overlay(now);
assert.equal(page.asked(), settings.clockId);
assert(!page.ticking(), 'no timer before a clock arrives');
page.feed({ running: true, remainingMs: 3_725_000, deadlineMs: now.ms + 3_725_000 });
assert.equal(page.display.textContent, '01:02:05');
now.ms += 5_000;
page.tick();
assert.equal(page.display.textContent, '01:02:00');
page.feed({ running: true, remainingMs: 4_020_000, deadlineMs: now.ms + 4_020_000 });
assert.equal(page.display.textContent, '01:07:00');
page.feed({ running: false, remainingMs: 90_000, deadlineMs: null });
now.ms += 60_000;
page.tick();
assert.equal(page.display.textContent, '00:01:30');
assert(!page.ticking(), 'no timer on a paused clock');
page.feed({ running: true, remainingMs: 1_000, deadlineMs: now.ms + 1_000 });
now.ms += 10_000;
page.tick();
assert.equal(page.display.textContent, '00:00:00');
assert(!page.ticking(), 'no timer once the countdown ends');
page.preview({ running: false, remainingMs: 50_000 });
assert.equal(page.display.textContent, '00:00:00', 'a live topic cannot replace the real clock');
page.context.streamerz.testing = true;
page.preview({ running: false, remainingMs: 50_000 });
assert.equal(page.display.textContent, '00:00:50');
page.update({ ...settings, clockId: 'another-clock' });
assert.equal(page.asked(), 'another-clock');
assert.equal(page.display.textContent, '--:--:--');
console.log('subathon overlay: ok');
