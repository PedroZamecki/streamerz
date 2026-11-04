// Run with node dev/test-subathon-overlay.cjs. No browser or network is needed.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const { webcrypto } = require('node:crypto');
const { TextEncoder } = require('node:util');

const html = fs.readFileSync('assets/overlays/subathon.html', 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];

function overlay(search = '') {
  const display = { textContent: '' };
  const sockets = [];
  const timeouts = [];
  let tick;
  class WebSocket {
    static OPEN = 1;
    constructor() { this.readyState = WebSocket.OPEN; sockets.push(this); }
    send(text) { this.sent = JSON.parse(text); }
    close() { this.readyState = 3; this.onclose?.({ code: 1000, reason: '' }); }
  }
  const context = { WebSocket, URLSearchParams, TextEncoder, crypto: webcrypto,
    btoa: s => Buffer.from(s, 'binary').toString('base64'),
    document: { getElementById: () => display }, location: { search },
    setTimeout: fn => timeouts.push(fn), setInterval: fn => { tick = fn; },
  };
  vm.runInNewContext(script, context);
  return { display, sockets, timeouts, tick };
}

(async () => {
  const missing = overlay();
  await missing.sockets[0].onmessage({ data: JSON.stringify({ request: 'Hello', authentication: {} }) });
  assert.equal(missing.display.textContent, 'Authentication required');
  assert.equal(missing.timeouts.length, 0);
  missing.tick();
  assert.equal(missing.display.textContent, 'Authentication required');

  const rejected = overlay('?password=wrong');
  const sock = rejected.sockets[0];
  await sock.onmessage({ data: JSON.stringify({ request: 'Hello', authentication: { salt: 'salt', challenge: 'challenge' } }) });
  assert.equal(sock.sent.request, 'Authenticate');
  sock.readyState = 3;
  sock.onclose({ code: 4009, reason: 'Authentication failed' });
  assert.equal(rejected.display.textContent, 'Authentication failed');
  assert.equal(rejected.timeouts.length, 0);
  rejected.tick();
  assert.equal(rejected.display.textContent, 'Authentication failed');

  const disconnected = overlay();
  disconnected.sockets[0].onclose({ code: 1006, reason: '' });
  assert.equal(disconnected.display.textContent, 'Disconnected');
  assert.equal(disconnected.timeouts.length, 1);
  disconnected.timeouts[0]();
  assert.equal(disconnected.sockets.length, 2);
})().catch(error => { console.error(error); process.exitCode = 1; });
