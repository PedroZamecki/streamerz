// MAIN world: TikTok's own fetch wrapper signs the requests. Only our parent may ask for one.
(() => {
  if (window.parent === window) return;
  let parentOrigin = "chrome-extension://lhijoeeghjcohapolkfkacipklhbnecc";
  try {
    if (document.referrer) {
      const u = new URL(document.referrer);
      if (u.protocol === "moz-extension:" || u.protocol === "chrome-extension:") {
        parentOrigin = u.origin;
      }
    }
  } catch {}
  const send = data => parent.postMessage({ streamerz: true, ...data }, parentOrigin);
  if (location.pathname.startsWith("/login")) {
    send({ ready: true, error: "Log in to TikTok from Accounts > TikTok > Set up.", code: "login_required" });
    return;
  }
  // The page's own requests say who the browser is (device_id, aid, region...). Keep them all: a busy page
  // fills the browser's default record of 250 before the one we need.
  try { performance.setResourceTimingBufferSize(2000); } catch {}
  const common = () => {
    const urls = performance.getEntriesByType("resource").map(e => e.name)
      .filter(u => /^https:\/\/([a-z0-9-]+\.)*tiktok\.com\//.test(u) && u.includes("device_id=") && u.includes("aid="));
    // A webcast request first (the LIVE ones), else any of TikTok's own (they carry the same keys).
    const best = urls.filter(u => /^https:\/\/webcast[a-z0-9.-]*\.tiktok\.com\//.test(u)).pop() || urls.pop();
    if (!best) return null;
    const params = Object.fromEntries(new URL(best).searchParams);
    // Only webcast requests carry verifyFp: it is TikTok's s_v_web_id cookie.
    if (!params.verifyFp) {
      const fp = document.cookie.match(/(?:^|; )s_v_web_id=([^;]+)/);
      if (fp) params.verifyFp = fp[1];
    }
    return params;
  };
  async function request(m) {
    if (m.op === "page") {
      const params = common();
      if (!params) throw new Error("TikTok hasn't finished loading. Try again.");
      return { params };
    }
    let url = m.url;
    if (m.op === "search") {
      const params = common();
      if (!params) throw new Error("TikTok hasn't finished loading. Try again.");
      for (const k of Object.keys(params)) if (k.startsWith("X-") || k === "msToken") delete params[k];
      const ctx = JSON.parse(document.getElementById("__UNIVERSAL_DATA_FOR_REHYDRATION__")?.textContent || "{}").__DEFAULT_SCOPE__?.["webapp.app-context"];
      Object.assign(params, { aid: "1988", app_name: "tiktok_web", channel: "tiktok_web", keyword: m.query,
        count: "12", cursor: "0", offset: "0", from_page: "search", is_non_personalized_search: "0", odinId: ctx?.user?.uid || "",
        web_search_code: JSON.stringify({ tiktok: { client_params_x: { search_engine: {
          ies_mt_user_live_video_card_use_libra: 1, mt_search_general_user_live_card: 1 } }, search_server: {} } }) });
      url = "https://www.tiktok.com/api/search/user/full/?" + new URLSearchParams(params);
    }
    const u = new URL(url);
    if (u.protocol !== "https:" || !["www.tiktok.com", "webcast.tiktok.com"].includes(u.hostname)) throw new Error("unsupported TikTok address");
    const res = await fetch(url, { ...m.init, credentials: "include" });
    return { status: res.status, body: await res.text() };
  }
  window.addEventListener("message", async e => {
    if (e.source !== parent || (e.origin !== parentOrigin && !e.origin.startsWith("moz-extension://") && !e.origin.startsWith("chrome-extension://")) || e.data?.streamerz !== true) return;
    if (e.origin.startsWith("moz-extension://") || e.origin.startsWith("chrome-extension://")) parentOrigin = e.origin;
    let result;
    try { result = await request(e.data.request); } catch (err) { result = { error: String(err.message || err) }; }
    send({ id: e.data.id, result });
  });
  // Only while loading, never an idle poll.
  (async () => {
    for (let i = 0; i < 50; i++) {
      if (window.byted_acrawler && window.secsdk && common()) {
        send({ ready: true });
        try {
          window.requestAnimationFrame = () => 0;
          window.stop();
        } catch {}
        return;
      }
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    // Given up: say what is missing, so the log tells why.
    const missing = [!window.byted_acrawler && "signer", !window.secsdk && "security SDK", !common() && "request details"].filter(Boolean);
    send({ ready: true, error: "TikTok's page didn't finish loading (no " + missing.join(", ") + "). Try again." });
  })();
})();
