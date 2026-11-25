//! Streamlabs Desktop, through its remote-control API (Settings → Remote Control): JSON-RPC over a
//! plain WebSocket at ws://host:port/api/websocket, authenticated with the token, as Streamer.bot
//! uses it (checked against it, see DEVELOPING.md). One task per connection, started only when it's
//! set to connect at start or connected by hand.

use crate::engine::Engine;
use crate::model::Obj;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

/// Streamer.bot's "any connection" (triggers) and "the default connection" (sub-actions).
pub const ZERO: &str = "00000000-0000-0000-0000-000000000000";

/// Streamer.bot's settings.json slobsSettings: connections {id, name, host, port, autoConnect,
/// reconnectOnDisconnect, token, retryInterval, default}.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub connections: Vec<Obj>,
}

pub(crate) type Reply = oneshot::Sender<Result<Value, String>>;
/// A call: resource, method, args.
pub(crate) type Call = (String, String, Value, Reply);

struct Conn {
    cfg: Obj,
    tx: mpsc::UnboundedSender<Call>,
    task: tokio::task::AbortHandle,
    up: Rc<Cell<bool>>,
}

/// The running connections, by id.
#[derive(Default)]
pub struct Slobs {
    conns: RefCell<HashMap<String, Conn>>,
}

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

fn flag(o: &Obj, k: &str) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(false)
}

fn num(o: &Obj, k: &str, default: u64) -> u64 {
    o.get(k)
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

fn opt_u64(o: &Obj, k: &str) -> u64 {
    o.get(k)
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_f64().filter(|x| x.is_finite() && *x >= 0.0).map(|x| x as u64))
                .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        })
        .unwrap_or(0)
}

fn opt_f64(o: &Obj, k: &str) -> f64 {
    o.get(k)
        .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
        .filter(|x| x.is_finite())
        .unwrap_or(0.0)
}

pub(crate) fn ws_url(o: &Obj) -> String {
    let host = s(o, "host").trim();
    let host = if host.is_empty() { "127.0.0.1" } else { host };
    let port = u16::try_from(num(o, "port", 59650))
        .ok()
        .filter(|&p| p > 0)
        .unwrap_or(59650);
    if host.contains(':') && !host.starts_with('[') {
        format!("ws://[{host}]:{port}/api/websocket")
    } else {
        format!("ws://{host}:{port}/api/websocket")
    }
}

pub(crate) fn reconnect_delay(base_secs: u64, attempt: u32) -> Duration {
    let base = base_secs.max(1);
    let cap = base.max(60);
    let factor = 1u64 << attempt.saturating_sub(1).min(4);
    Duration::from_secs(base.saturating_mul(factor).min(cap))
}

/// The connection a sub-action means: its id; empty or all zeros, the default one (else the first).
fn pick(list: &[Obj], id: &str) -> Option<Obj> {
    if id.is_empty() || id == ZERO {
        list.iter().find(|o| flag(o, "default")).or(list.first()).cloned()
    } else {
        list.iter().find(|o| s(o, "id") == id).cloned()
    }
}

/// The trigger and WebSocket event for a Streamlabs event: (type, trigger name, event name), as
/// Streamer.bot maps them (Streaming Started on "starting", Recording Started on "recording"...).
fn event(resource: &str, data: &Value) -> Option<(u32, &'static str, &'static str)> {
    Some(match (resource, data.as_str()) {
        ("ScenesService.sceneSwitched", _) => (22003, "Scene Changed", "SceneChanged"),
        ("StreamingService.streamingStatusChange", Some("starting")) => {
            (22004, "Streaming Started", "StreamingStarted")
        }
        ("StreamingService.streamingStatusChange", Some("ending")) => (22005, "Streaming Stopped", "StreamingStopped"),
        ("StreamingService.recordingStatusChange", Some("recording")) => {
            (22006, "Recording Started", "RecordingStarted")
        }
        ("StreamingService.recordingStatusChange", Some("stopping")) => {
            (22007, "Recording Stopped", "RecordingStopped")
        }
        _ => return None,
    })
}

/// The nodes of `scene` called `name` of this kind ("item" or "folder"). `display` picks the
/// horizontal (1) or vertical (2) copy in a dual-output scene; otherwise both.
fn named<'a>(scene: &'a Value, name: &str, kind: &str, display: Option<&Value>) -> Vec<&'a Obj> {
    let want = match display {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(Value::String(t)) if t.eq_ignore_ascii_case("horizontal") => 1,
        Some(Value::String(t)) if t.eq_ignore_ascii_case("vertical") => 2,
        _ => 0,
    };
    nodes(scene)
        .filter(|n| s(n, "name") == name && s(n, "sceneNodeType") == kind)
        .filter(|n| match want {
            1 => s(n, "display") != "vertical",
            2 => s(n, "display") == "vertical",
            _ => true,
        })
        .collect()
}

fn nodes(scene: &Value) -> impl Iterator<Item = &Obj> {
    scene["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
}

/// A state field: 0 on, 1 off, 2 the opposite of `now`.
fn wanted(state: u64, now: bool) -> bool {
    match state {
        0 => true,
        1 => false,
        _ => !now,
    }
}

impl Engine {
    /// Any Streamlabs Desktop connection up.
    pub fn slobs_connected(&self) -> bool {
        self.slobs.conns.borrow().values().any(|c| c.up.get())
    }

    /// The Streamlabs Desktop connection `id` is up.
    pub fn slobs_up(&self, id: &str) -> bool {
        self.slobs.conns.borrow().get(id).is_some_and(|c| c.up.get())
    }

    /// Match the running connections to the settings: drop removed ones, restart changed ones, and
    /// (`start`) start those set to connect at start.
    pub fn slobs_sync(self: &Rc<Self>, start: bool) {
        let list = self.config().slobs.connections.clone();
        let mut restart = Vec::new();
        let mut dropped_up = Vec::new();
        self.slobs.conns.borrow_mut().retain(|id, c| {
            let was_running = !c.task.is_finished();
            let keep = was_running && list.contains(&c.cfg);
            if !keep {
                c.task.abort();
                if c.up.replace(false) {
                    dropped_up.push(c.cfg.clone());
                }
                if was_running && list.iter().any(|o| s(o, "id") == id) {
                    restart.push(id.clone());
                }
            }
            keep
        });
        for cfg in dropped_up {
            self.slobs_fire(&cfg, 22002, "Disconnected", "Disconnected", None);
        }
        for o in &list {
            if restart.iter().any(|r| r == s(o, "id")) || (start && flag(o, "autoConnect")) {
                self.slobs_connect(s(o, "id"));
            }
        }
    }

    /// Connect (all of them for "").
    pub fn slobs_connect(self: &Rc<Self>, id: &str) {
        let list = self.config().slobs.connections.clone();
        for o in list.into_iter().filter(|o| id.is_empty() || s(o, "id") == id) {
            let cid = s(&o, "id").to_string();
            if self
                .slobs
                .conns
                .borrow()
                .get(&cid)
                .is_some_and(|c| !c.task.is_finished())
            {
                continue;
            }
            let (tx, rx) = mpsc::unbounded_channel();
            let up = Rc::new(Cell::new(false));
            let task = tokio::task::spawn_local(self.clone().slobs_run(o.clone(), rx, up.clone())).abort_handle();
            self.slobs.conns.borrow_mut().insert(cid, Conn { cfg: o, tx, task, up });
        }
    }

    /// Disconnect (all of them for "").
    pub fn slobs_disconnect(self: &Rc<Self>, id: &str) {
        let gone: Vec<Conn> = {
            let mut conns = self.slobs.conns.borrow_mut();
            let ids: Vec<String> = conns.keys().filter(|k| id.is_empty() || *k == id).cloned().collect();
            ids.iter().filter_map(|k| conns.remove(k)).collect()
        };
        for c in gone {
            c.task.abort();
            if c.up.get() {
                self.slobs_fire(&c.cfg, 22002, "Disconnected", "Disconnected", None);
            }
        }
    }

    /// One connection: connect, and reconnect with backoff if set to.
    async fn slobs_run(self: Rc<Self>, o: Obj, mut rx: mpsc::UnboundedReceiver<Call>, up: Rc<Cell<bool>>) {
        let url = ws_url(&o);
        let base_retry = num(&o, "retryInterval", 30);
        let status_key = format!("slobs:{}", s(&o, "id"));
        let mut attempt: u32 = 0;
        let mut last_err: Option<String> = None;
        loop {
            match self.slobs_session(&o, &url, &mut rx, &up).await {
                Ok(()) => {
                    if up.get() {
                        crate::core::status::record_failure(&status_key, crate::core::status::Failure::Closed);
                    }
                }
                Err((reason, e)) => {
                    crate::core::status::record_failure(&status_key, reason);
                    if last_err.as_deref() != Some(e.as_str()) {
                        crate::warn(format!("Streamlabs Desktop {}: {e}", s(&o, "name")));
                        last_err = Some(e);
                    }
                }
            }
            if up.replace(false) {
                attempt = 0;
                last_err = None;
                self.slobs_fire(&o, 22002, "Disconnected", "Disconnected", None);
            }
            if !flag(&o, "reconnectOnDisconnect") {
                return;
            }
            attempt = attempt.saturating_add(1);
            // Calls made while waiting fail at once rather than hang.
            let wait = tokio::time::sleep(reconnect_delay(base_retry, attempt));
            tokio::pin!(wait);
            loop {
                tokio::select! {
                    _ = &mut wait => break,
                    call = rx.recv() => match call {
                        Some(c) => drop(c.3.send(Err("not connected".into()))),
                        None => return,
                    },
                }
            }
        }
    }

    async fn slobs_session(
        self: &Rc<Self>,
        o: &Obj,
        url: &str,
        rx: &mut mpsc::UnboundedReceiver<Call>,
        up: &Cell<bool>,
    ) -> Result<(), (crate::core::status::Failure, String)> {
        use crate::core::status::Failure;
        let connect_fut = tokio::time::timeout(Duration::from_secs(15), crate::http::ws_connect(url));
        tokio::pin!(connect_fut);
        let ws = loop {
            tokio::select! {
                res = &mut connect_fut => match res {
                    Ok(Ok((ws, _))) => break ws,
                    Ok(Err(e)) => return Err((Failure::Connect, e.to_string())),
                    Err(_) => return Err((Failure::Connect, "connection timed out".into())),
                },
                call = rx.recv() => match call {
                    Some(c) => drop(c.3.send(Err("not connected".into()))),
                    None => return Ok(()),
                },
            }
        };
        let (mut tx, mut stream) = ws.split();
        let req = |id: &str, resource: &str, method: &str, args: Value| {
            Message::Text(
                json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {"resource": resource, "args": args}})
                    .to_string()
                    .into(),
            )
        };
        tx.send(req("auth", "TcpServerService", "auth", json!([s(o, "token")])))
            .await
            .map_err(|e| (Failure::Handshake, e.to_string()))?;

        let mut pending: HashMap<u64, Reply> = HashMap::new();
        let mut next = 0u64;
        let auth_deadline = tokio::time::sleep(Duration::from_secs(15));
        tokio::pin!(auth_deadline);
        loop {
            tokio::select! {
                _ = &mut auth_deadline, if !up.get() => {
                    return Err((Failure::Handshake, "auth timed out".into()));
                }
                m = stream.next() => {
                    let Some(Ok(m)) = m else { return Ok(()) };
                    match m {
                        Message::Ping(data) => {
                            let _ = tx.send(Message::Pong(data)).await;
                            continue;
                        }
                        Message::Close(_) => return Ok(()),
                        Message::Text(t) => {
                            let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                            let result = v.get("result").unwrap_or(&Value::Null);
                            let err = v["error"]["message"].as_str().map(String::from);
                            let id_str = v["id"].as_str();
                            let id_num = id_str
                                .and_then(|x| x.parse::<u64>().ok())
                                .or_else(|| v["id"].as_u64());
                            match id_str {
                                Some("auth") => {
                                    if err.is_some() || result != &Value::Bool(true) {
                                        return Err((Failure::Handshake, format!("refused the token ({})", err.unwrap_or_default())));
                                    }
                                    // Subscribe to the events our triggers need only after auth succeeds.
                                    for (resource, method) in [
                                        ("ScenesService", "sceneSwitched"),
                                        ("StreamingService", "streamingStatusChange"),
                                        ("StreamingService", "recordingStatusChange"),
                                    ] {
                                        tx.send(req("sub", resource, method, json!([])))
                                            .await
                                            .map_err(|e| (Failure::Closed, e.to_string()))?;
                                    }
                                    up.set(true);
                                    crate::core::status::clear_failure(&format!("slobs:{}", s(o, "id")));
                                    crate::say(format!("Streamlabs Desktop {}: connected", s(o, "name")));
                                    self.slobs_fire(o, 22001, "Connected", "Connected", None);
                                }
                                Some("sub") => {}
                                _ if id_num.is_some() => {
                                    if let Some(reply) = id_num.and_then(|id| pending.remove(&id)) {
                                        let _ = reply.send(err.map_or_else(|| Ok(result.clone()), Err));
                                    }
                                }
                                None if result["_type"] == "EVENT" => self.slobs_heard(o, result),
                                _ => {}
                            }
                        }
                        _ => continue,
                    }
                }
                call = rx.recv() => {
                    let Some((resource, method, args, reply)) = call else { return Ok(()) };
                    if !up.get() {
                        let _ = reply.send(Err("not connected".into()));
                        continue;
                    }
                    next += 1;
                    pending.insert(next, reply);
                    tx.send(req(&next.to_string(), &resource, &method, args))
                        .await
                        .map_err(|e| (Failure::Closed, e.to_string()))?;
                }
            }
        }
    }

    /// An event Streamlabs Desktop sent (`result` of a message with no id).
    pub(crate) fn slobs_heard(self: &Rc<Self>, o: &Obj, result: &Value) {
        let data = &result["data"];
        if let Some((kind, trigger, ev)) = event(result["resourceId"].as_str().unwrap_or(""), data) {
            if kind == 22004 {
                self.notify_stream_started();
            }
            self.slobs_fire(o, kind, trigger, ev, data["name"].as_str());
        }
    }

    /// Connection `o` as up, with no socket: whoever holds the receiver answers its calls (scenarios).
    pub(crate) fn slobs_detached(self: &Rc<Self>, o: Obj) -> mpsc::UnboundedReceiver<Call> {
        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::task::spawn_local(std::future::pending::<()>()).abort_handle();
        let up = Rc::new(Cell::new(true));
        let id = s(&o, "id").to_string();
        self.slobs.conns.borrow_mut().insert(id, Conn { cfg: o, tx, task, up });
        rx
    }

    /// Fire a Streamlabs Desktop trigger (sd.id, sd.name, sd.host, and sd.sceneName for scene
    /// changes) and its WebSocket event.
    fn slobs_fire(self: &Rc<Self>, o: &Obj, kind: u32, trigger: &str, ev: &str, scene: Option<&str>) {
        let id = s(o, "id");
        let mut args = Obj::new();
        args.insert("sd.id".into(), id.into());
        args.insert("sd.name".into(), s(o, "name").into());
        args.insert("sd.host".into(), s(o, "host").into());
        if let Some(scene) = scene {
            args.insert("sd.sceneName".into(), scene.into());
        }
        self.fire(kind, trigger, "Streamlabs Desktop", "streamlabsdesktop", &args, |t| {
            let want = s(t, "sdId");
            (want.is_empty() || want == ZERO || want == id)
                && (s(t, "sceneName").is_empty() || Some(s(t, "sceneName")) == scene)
        });
        self.emit("StreamlabsDesktop", ev, || {
            let mut d = args.clone();
            d.shift_insert(0, "__source".into(), kind.into());
            Value::Object(d)
        });
    }

    /// Call a Streamlabs Desktop method on the connection `conn` (see `pick`).
    async fn slobs_call(
        &self,
        conn: &str,
        resource: String,
        method: &'static str,
        args: Value,
    ) -> Result<Value, String> {
        let o = pick(&self.config().slobs.connections, conn).ok_or("no such connection")?;
        let tx = self
            .slobs
            .conns
            .borrow()
            .get(s(&o, "id"))
            .map(|c| c.tx.clone())
            .ok_or("not connected")?;
        let (reply, answer) = oneshot::channel();
        tx.send((resource, method.into(), args, reply))
            .map_err(|_| "not connected")?;
        tokio::time::timeout(Duration::from_secs(30), answer)
            .await
            .map_err(|_| "request timed out")?
            .map_err(|_| "not connected")?
    }

    /// A Streamlabs Desktop sub-action, its text fields already expanded. Get Current Scene's scene.
    pub(crate) async fn slobs_sub(self: &Rc<Self>, kind: u64, o: &Obj) -> Result<Option<String>, String> {
        let conn = s(o, "connectionId");
        let call = |resource: &str, method: &'static str, args: Value| {
            self.slobs_call(conn, resource.to_string(), method, args)
        };
        let state = opt_u64(o, "state");
        match kind {
            // Streaming: 0 stop, 1 start. Recording: 0 start, 1 stop; Pause/Resume do nothing
            // (Streamlabs can't pause). Streamlabs only has toggles.
            404 | 415 => {
                let (key, toggle) = if kind == 404 {
                    ("streamingStatus", "toggleStreaming")
                } else {
                    ("recordingStatus", "toggleRecording")
                };
                let want = match (kind, state) {
                    (404, 1) | (415, 0) => true,
                    (404, 0) | (415, 1) => false,
                    _ => return Ok(None),
                };
                let model = call("StreamingService", "getModel", json!([])).await?;
                if (model[key].as_str().unwrap_or("offline") != "offline") != want {
                    call("StreamingService", toggle, json!([])).await?;
                }
                return Ok(None);
            }
            414 => {
                let scene = call("ScenesService", "activeScene", json!([])).await?;
                return Ok(Some(scene["name"].as_str().unwrap_or("").into()));
            }
            _ => {}
        }
        // The scene: by name, or the active one.
        let scene = match s(o, "sceneName") {
            "" => call("ScenesService", "activeScene", json!([])).await?,
            name => call("ScenesService", "getScenes", json!([]))
                .await?
                .as_array()
                .and_then(|l| l.iter().find(|x| x["name"] == name).cloned())
                .ok_or(format!("no scene {name}"))?,
        };
        let scene_id = scene["id"].as_str().unwrap_or("").to_string();
        let display = o.get("display");
        let items = named(&scene, s(o, "sourceName"), "item", display);
        let item = |method: &'static str, args: Value| {
            let calls: Vec<_> = items
                .iter()
                .map(|n| call(s(n, "resourceId"), method, args.clone()))
                .collect();
            async move {
                for c in calls {
                    c.await?;
                }
                Ok::<_, String>(())
            }
        };
        let mut sources: Vec<&str> = Vec::new();
        for n in &items {
            let sid = s(n, "sourceId");
            if !sid.is_empty() && !sources.contains(&sid) {
                sources.push(sid);
            }
        }
        match kind {
            401 => {
                for n in &items {
                    let v = wanted(state, flag(n, "visible"));
                    call(s(n, "resourceId"), "setVisibility", json!([v])).await?;
                }
            }
            // Random group source: one of the group's hidden sources is shown. Hide group's sources.
            402 | 403 => {
                for g in named(&scene, s(o, "groupName"), "folder", display) {
                    let children: Vec<&Obj> = nodes(&scene)
                        .filter(|n| s(n, "parentId") == s(g, "id") && s(n, "sceneNodeType") == "item")
                        .filter(|n| flag(n, "visible") == (kind == 403))
                        .collect();
                    let pick: Vec<&Obj> = if kind == 403 {
                        children
                    } else if children.is_empty() {
                        vec![]
                    } else {
                        vec![children[crate::math::secure_u64() as usize % children.len()]]
                    };
                    for n in pick {
                        call(s(n, "resourceId"), "setVisibility", json!([kind == 402])).await?;
                    }
                }
            }
            405 => {
                call("ScenesService", "makeSceneActive", json!([scene_id])).await?;
            }
            406 | 411 | 412 => {
                for src in sources {
                    self.slobs_filters(conn, src, kind, s(o, "filterName"), state).await?;
                }
            }
            417 => {
                self.slobs_filters(conn, &scene_id, 406, s(o, "filterName"), state)
                    .await?
            }
            // Flip: 0 horizontal, 1 vertical, 2 both.
            407 => {
                let mode = opt_u64(o, "flipMode");
                if mode != 1 {
                    item("flipX", json!([])).await?;
                }
                if mode != 0 {
                    item("flipY", json!([])).await?;
                }
            }
            // Rotate: to an angle, or by it (Additive).
            408 => {
                let deg = opt_f64(o, "degrees");
                if flag(o, "additive") {
                    item("rotate", json!([deg])).await?;
                } else {
                    item("setTransform", json!([{"rotation": deg}])).await?;
                }
            }
            // ponytail: Streamer.bot 1.0.7 sends nothing for these two; this is Streamlabs' own way.
            409 | 410 => {
                let (k, v) = if kind == 409 {
                    ("url", s(o, "url"))
                } else {
                    ("text", s(o, "text"))
                };
                for src in sources {
                    let quoted = serde_json::to_string(src).unwrap_or_else(|_| format!("\"{src}\""));
                    call(&format!("Source[{quoted}]"), "updateSettings", json!([{k: v}])).await?;
                }
            }
            413 => {
                let audio = call("AudioService", "getSourcesForScene", json!([scene_id])).await?;
                let name = s(o, "sourceName");
                for a in audio.as_array().into_iter().flatten().filter(|a| a["name"] == name) {
                    let Some(rid) = a["resourceId"].as_str().filter(|r| !r.is_empty()) else {
                        continue;
                    };
                    let v = wanted(state, a["muted"].as_bool().unwrap_or(false));
                    call(rid, "setMuted", json!([v])).await?;
                }
            }
            _ => {}
        }
        Ok(None)
    }

    /// A source's (or scene's) filters: Set Filter State (406) the one named `name`, Set State of a
    /// Random Filter (411) one of them, Hide Source's Filters (412) all of them.
    async fn slobs_filters(&self, conn: &str, source: &str, kind: u64, name: &str, state: u64) -> Result<(), String> {
        let list = self
            .slobs_call(conn, "SourceFiltersService".into(), "getFilters", json!([source]))
            .await?;
        let all: Vec<&Value> = list.as_array().into_iter().flatten().collect();
        let chosen: Vec<&Value> = match kind {
            411 if !all.is_empty() => vec![all[crate::math::secure_u64() as usize % all.len()]],
            412 => all,
            _ => all.into_iter().filter(|f| f["name"] == name).collect(),
        };
        for f in chosen {
            let v = if kind == 412 {
                false
            } else {
                wanted(state, f["visible"].as_bool().unwrap_or(false))
            };
            self.slobs_call(
                conn,
                "SourceFiltersService".into(),
                "setVisibility",
                json!([source, f["name"], v]),
            )
            .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_and_nodes() {
        assert_eq!(
            event("StreamingService.streamingStatusChange", &json!("starting"))
                .unwrap()
                .0,
            22004
        );
        assert_eq!(event("StreamingService.streamingStatusChange", &json!("live")), None);
        assert_eq!(
            event("StreamingService.recordingStatusChange", &json!("stopping"))
                .unwrap()
                .0,
            22007
        );
        assert_eq!(
            event("ScenesService.sceneSwitched", &json!({"name": "Brb"})).unwrap().0,
            22003
        );
        let scene = json!({"nodes": [
            {"id": "a", "name": "Cam", "sceneNodeType": "item", "display": "horizontal"},
            {"id": "b", "name": "Cam", "sceneNodeType": "item", "display": "vertical"},
            {"id": "f", "name": "Cam", "sceneNodeType": "folder"}]});
        let ids = |d: Value| -> Vec<String> {
            named(&scene, "Cam", "item", Some(&d))
                .iter()
                .map(|n| s(n, "id").to_string())
                .collect()
        };
        assert_eq!(ids(Value::Null), ["a", "b"]);
        assert_eq!(ids(json!(1)), ["a"]);
        assert_eq!(ids(json!("vertical")), ["b"]);
        let list = vec![
            json!({"id": "x"}).as_object().cloned().unwrap(),
            json!({"id": "y", "default": true}).as_object().cloned().unwrap(),
        ];
        assert_eq!(s(&pick(&list, ZERO).unwrap(), "id"), "y");
        assert_eq!(s(&pick(&list, "x").unwrap(), "id"), "x");
        assert!(pick(&list, "z").is_none());
        assert!(wanted(2, false) && !wanted(1, true) && wanted(0, true));
    }

    #[test]
    fn url_formatting_and_backoff() {
        let obj = |v: Value| v.as_object().cloned().unwrap();
        assert_eq!(ws_url(&obj(json!({}))), "ws://127.0.0.1:59650/api/websocket");
        assert_eq!(
            ws_url(&obj(json!({"host": "::1", "port": 59650}))),
            "ws://[::1]:59650/api/websocket"
        );
        assert_eq!(
            ws_url(&obj(json!({"host": "localhost", "port": 99999}))),
            "ws://localhost:59650/api/websocket"
        );
        let seq: Vec<u64> = (1..=5).map(|n| reconnect_delay(10, n).as_secs()).collect();
        assert_eq!(seq, [10, 20, 40, 60, 60]);
    }

    #[test]
    fn mock_slobs_auth_order_rpc_and_events() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mk = |id: &str, code: u32, msg: &str| {
            let mut a = action(id, id, json!([log(msg)]));
            a["triggers"] = json!([{"id": "t", "type": code, "enabled": true}]);
            a
        };
        let out = run(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            tokio::spawn(async move {
                let (tcp, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
                // First message must be auth, NOT subscribe.
                let first: Value = match ws.next().await.unwrap().unwrap() {
                    Message::Text(t) => serde_json::from_str(&t).unwrap(),
                    other => panic!("expected text, got {other:?}"),
                };
                assert_eq!(first["id"], "auth");
                assert_eq!(first["params"]["args"][0], "tok");
                ws.send(Message::Text(
                    json!({"jsonrpc": "2.0", "id": "auth", "result": true})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();

                // Next 3 messages are the event subscriptions.
                for _ in 0..3 {
                    let sub: Value = match ws.next().await.unwrap().unwrap() {
                        Message::Text(t) => serde_json::from_str(&t).unwrap(),
                        other => panic!("expected sub, got {other:?}"),
                    };
                    assert_eq!(sub["id"], "sub");
                    ws.send(Message::Text(
                        json!({"jsonrpc": "2.0", "id": "sub", "result": {}}).to_string().into(),
                    ))
                    .await
                    .unwrap();
                }

                // Emit a sceneSwitched event.
                ws.send(Message::Text(
                    json!({
                        "jsonrpc": "2.0",
                        "result": {
                            "_type": "EVENT",
                            "resourceId": "ScenesService.sceneSwitched",
                            "data": {"name": "Main"}
                        }
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();

                // Handle one RPC call, replying with a numeric id to verify both string and integer ids match.
                if let Some(Ok(Message::Text(t))) = ws.next().await {
                    let req: Value = serde_json::from_str(&t).unwrap();
                    let id: u64 = req["id"].as_str().unwrap().parse().unwrap();
                    ws.send(Message::Text(
                        json!({"jsonrpc": "2.0", "id": id, "result": {"name": "Main"}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                }
                let _ = ws.close(None).await;
            });

            let e = engine(json!({
                "actions": [
                    mk("up", 22001, "T up %sd.name%"),
                    mk("down", 22002, "T down"),
                    mk("scene", 22003, "T scene %sd.sceneName%"),
                ]
            }));
            e.change(|c| {
                c.slobs.connections = vec![json!({
                    "id": "s1",
                    "name": "SLOBS",
                    "host": "127.0.0.1",
                    "port": port,
                    "token": "tok",
                    "autoConnect": true,
                    "reconnectOnDisconnect": false
                })
                .as_object()
                .cloned()
                .unwrap()];
            });
            e.slobs_sync(true);
            let start = std::time::Instant::now();
            while !e.slobs_up("s1") && start.elapsed() < Duration::from_secs(2) {
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
            assert!(e.slobs_up("s1"));
            let got = e
                .slobs_sub(414, json!({"connectionId": "s1"}).as_object().unwrap())
                .await
                .unwrap();
            assert_eq!(got.as_deref(), Some("Main"));
            tokio::time::sleep(Duration::from_millis(120)).await;
            assert!(!e.slobs_up("s1"));
        });
        assert!(out.contains(&"T up SLOBS".to_string()), "{out:?}");
        assert!(out.contains(&"T scene Main".to_string()), "{out:?}");
        assert!(out.contains(&"T down".to_string()), "{out:?}");
    }

    #[test]
    fn slobs_sub_actions_dedup_sources_and_sync_fires_disconnect() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut down = action("down", "down", json!([log("T sync-down")]));
        down["triggers"] = json!([{"id": "t", "type": 22002, "enabled": true}]);
        let e = engine(json!({"actions": [down]}));
        let conn_obj = json!({"id": "c1", "name": "Desk", "host": "127.0.0.1", "port": 59650, "autoConnect": false})
            .as_object()
            .cloned()
            .unwrap();

        let out = run(async move {
            e.change(|c| c.slobs.connections = vec![conn_obj.clone()]);
            let mut rx = e.slobs_detached(conn_obj);
            let calls = tokio::task::spawn_local(async move {
                let mut seen = Vec::new();
                while let Some((res, method, args, reply)) = rx.recv().await {
                    let answer = match method.as_str() {
                        "activeScene" => json!({
                            "id": "sc1",
                            "name": "Main",
                            "nodes": [
                                {"id": "n1", "name": "Browser", "sceneNodeType": "item", "sourceId": "src\"1", "resourceId": "SceneItem[\"n1\"]", "visible": true, "display": "horizontal"},
                                {"id": "n2", "name": "Browser", "sceneNodeType": "item", "sourceId": "src2", "resourceId": "SceneItem[\"n2\"]", "visible": false, "display": "vertical"},
                                {"id": "n3", "name": "Browser", "sceneNodeType": "item", "sourceId": "src\"1", "resourceId": "SceneItem[\"n3\"]", "visible": true, "display": "horizontal"}
                            ]
                        }),
                        _ => json!({}),
                    };
                    seen.push((res, method, args));
                    let _ = reply.send(Ok(answer));
                }
                seen
            });

            // 409: Set Browser Source URL deduplicates non-adjacent sources and escapes quotes in Source[...].
            e.slobs_sub(
                409,
                json!({"connectionId": "c1", "sourceName": "Browser", "url": "https://example.test"})
                    .as_object()
                    .unwrap(),
            )
            .await
            .unwrap();

            // 408: Rotate accepts string degrees.
            e.slobs_sub(
                408,
                json!({"connectionId": "c1", "sourceName": "Browser", "degrees": "90", "display": "vertical"})
                    .as_object()
                    .unwrap(),
            )
            .await
            .unwrap();

            // Removing the connection in slobs_sync aborts it and fires Disconnected (22002).
            e.change(|c| c.slobs.connections.clear());
            e.slobs_sync(false);
            let seen = calls.await.unwrap();
            let updates: Vec<&str> = seen
                .iter()
                .filter(|(_, m, _)| m == "updateSettings")
                .map(|(r, _, _)| r.as_str())
                .collect();
            assert_eq!(updates, vec!["Source[\"src\\\"1\"]", "Source[\"src2\"]"]);
            let rots: Vec<(&str, &Value)> = seen
                .iter()
                .filter(|(_, m, _)| m == "setTransform")
                .map(|(r, _, a)| (r.as_str(), a))
                .collect();
            assert_eq!(rots, vec![("SceneItem[\"n2\"]", &json!([{"rotation": 90.0}]))]);
            tokio::time::sleep(Duration::from_millis(80)).await;
        });
        assert_eq!(out, ["T sync-down"]);
    }
}
