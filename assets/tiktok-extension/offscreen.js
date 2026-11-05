const frame = document.createElement("iframe");
const origin = "https://www.tiktok.com";
const pending = new Map();
let next = 0;
let readyResolve;
const ready = new Promise(resolve => { readyResolve = resolve; });
window.addEventListener("message", e => {
  if (e.source !== frame.contentWindow || e.origin !== origin || e.data?.streamerz !== true) return;
  if (e.data.ready) { readyResolve(e.data.error || ""); return; }
  const p = pending.get(e.data.id);
  if (p) { clearTimeout(p.timer); pending.delete(e.data.id); p.resolve(e.data.result); }
});
frame.src = origin + "/live/history/center";
document.body.append(frame);
chrome.runtime.onMessage.addListener((m, sender, reply) => {
  if (sender.id !== chrome.runtime.id || m.target !== "offscreen") return;
  (async () => {
    const error = await Promise.race([ready, new Promise((_, reject) => {
      const timer = setTimeout(() => reject(new Error("TikTok didn't load. Try again, or log in from Accounts.")), 30000);
      ready.then(() => clearTimeout(timer));
    })]);
    if (error) throw new Error(error);
    return new Promise(resolve => {
      const id = ++next;
      const timer = setTimeout(() => { pending.delete(id); resolve({ error: "TikTok didn't answer. Try again." }); }, 30000);
      pending.set(id, { resolve, timer });
      frame.contentWindow.postMessage({ streamerz: true, id, request: m.request }, origin);
    });
  })().then(reply, e => reply({ error: String(e.message || e) }));
  return true;
});
