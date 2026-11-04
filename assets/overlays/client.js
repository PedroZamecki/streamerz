(() => {
  const C = __STREAMERZ__;
  const topics = new Set(), events = new Set(), handlers = Object.create(null), last = Object.create(null);
  let source, timer;
  const fire = (key, value, name) => (handlers[key] || []).forEach(f => {
    try { f(value, name); } catch (e) { console.error(e); }
  });
  const connect = () => { clearTimeout(timer); timer = setTimeout(open, 0); };
  function open() {
    if (source) source.close();
    const q = new URLSearchParams({ topics: [...topics].join(","), events: [...events].join(",") });
    source = new EventSource(C.server + "/overlays/events?" + q);
    source.onmessage = m => {
      const d = JSON.parse(m.data), e = d.event;
      if (e.source === "Overlay") {
        last[e.type] = d.data;
        fire(e.type, d.data);
      } else {
        const name = e.source + "." + e.type;
        fire("event:" + name, d.data, name);
        fire("event:" + e.source + ".*", d.data, name);
      }
    };
  }
  const streamerz = {
    settings: C.settings,
    group: C.group,
    state: last,
    // streamerz.on("viewers", v => ...): a topic. Gets the current value first, then every change.
    on(topic, fn) {
      (handlers[topic] ||= []).push(fn);
      topics.add(topic);
      if (topic in last) fn(last[topic]);
      connect();
      return streamerz;
    },
    // streamerz.event("Spotify.TrackChanged", data => ...): any Streamerz event ("Twitch.*" for all of one source).
    event(name, fn) {
      (handlers["event:" + name] ||= []).push(fn);
      events.add(name);
      connect();
      return streamerz;
    },
  };
  window.streamerz = streamerz;
})();
