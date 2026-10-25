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

type Reply = oneshot::Sender<Result<Value, String>>;
/// A call: resource, method, args.
type Call = (String, String, Value, Reply);

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
    o.get(k).and_then(Value::as_u64).filter(|n| *n > 0).unwrap_or(default)
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
        self.slobs.conns.borrow_mut().retain(|id, c| {
            let keep = !c.task.is_finished() && list.contains(&c.cfg);
            if !keep {
                c.task.abort();
                if !c.task.is_finished() && list.iter().any(|o| s(o, "id") == id) {
                    restart.push(id.clone());
                }
            }
            keep
        });
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

    /// One connection: connect, and reconnect every retryInterval seconds if set to.
    async fn slobs_run(self: Rc<Self>, o: Obj, mut rx: mpsc::UnboundedReceiver<Call>, up: Rc<Cell<bool>>) {
        let url = format!("ws://{}:{}/api/websocket", s(&o, "host"), num(&o, "port", 59650));
        let retry = Duration::from_secs(num(&o, "retryInterval", 30));
        loop {
            if let Err(e) = self.slobs_session(&o, &url, &mut rx, &up).await {
                crate::say(format!("Streamlabs Desktop {}: {e}", s(&o, "name")));
            }
            if up.replace(false) {
                self.slobs_fire(&o, 22002, "Disconnected", "Disconnected", None);
            }
            if !flag(&o, "reconnectOnDisconnect") {
                return;
            }
            // Calls made while waiting fail at once rather than hang.
            let wait = tokio::time::sleep(retry);
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
    ) -> Result<(), String> {
        let (ws, _) = crate::http::ws_connect(url).await.map_err(|e| e.to_string())?;
        let (mut tx, mut stream) = ws.split();
        let req = |id: &str, resource: &str, method: &str, args: Value| {
            Message::Text(
                json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {"resource": resource, "args": args}})
                    .to_string()
                    .into(),
            )
        };
        let mut first = vec![req("auth", "TcpServerService", "auth", json!([s(o, "token")]))];
        // Only the events the triggers need.
        for (resource, method) in [
            ("ScenesService", "sceneSwitched"),
            ("StreamingService", "streamingStatusChange"),
            ("StreamingService", "recordingStatusChange"),
        ] {
            first.push(req("sub", resource, method, json!([])));
        }
        for m in first {
            tx.send(m).await.map_err(|e| e.to_string())?;
        }
        let mut pending: HashMap<u64, Reply> = HashMap::new();
        let mut next = 0u64;
        loop {
            tokio::select! {
                m = stream.next() => {
                    let Some(Ok(m)) = m else { return Ok(()) };
                    let Message::Text(t) = m else { continue };
                    let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                    let result = v.get("result").unwrap_or(&Value::Null);
                    let err = v["error"]["message"].as_str().map(String::from);
                    match v["id"].as_str() {
                        Some("auth") => {
                            if err.is_some() || result != &Value::Bool(true) {
                                return Err(format!("refused the token ({})", err.unwrap_or_default()));
                            }
                            up.set(true);
                            crate::say(format!("Streamlabs Desktop {}: connected", s(o, "name")));
                            self.slobs_fire(o, 22001, "Connected", "Connected", None);
                        }
                        Some(id) => {
                            if let Some(reply) = id.parse().ok().and_then(|id| pending.remove(&id)) {
                                let _ = reply.send(err.map_or_else(|| Ok(result.clone()), Err));
                            }
                        }
                        None if result["_type"] == "EVENT" => {
                            let data = &result["data"];
                            if let Some((kind, trigger, ev)) = event(result["resourceId"].as_str().unwrap_or(""), data) {
                                self.slobs_fire(o, kind, trigger, ev, data["name"].as_str());
                            }
                        }
                        None => {}
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
                    tx.send(req(&next.to_string(), &resource, &method, args)).await.map_err(|e| e.to_string())?;
                }
            }
        }
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
        answer.await.map_err(|_| "not connected")?
    }

    /// A Streamlabs Desktop sub-action, its text fields already expanded. Get Current Scene's scene.
    pub(crate) async fn slobs_sub(self: &Rc<Self>, kind: u64, o: &Obj) -> Result<Option<String>, String> {
        let conn = s(o, "connectionId");
        let call = |resource: &str, method: &'static str, args: Value| {
            self.slobs_call(conn, resource.to_string(), method, args)
        };
        let state = o.get("state").and_then(Value::as_u64).unwrap_or(0);
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
        let mut sources: Vec<&str> = items.iter().map(|n| s(n, "sourceId")).collect();
        sources.dedup();
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
                let mode = o.get("flipMode").and_then(Value::as_u64).unwrap_or(0);
                if mode != 1 {
                    item("flipX", json!([])).await?;
                }
                if mode != 0 {
                    item("flipY", json!([])).await?;
                }
            }
            // Rotate: to an angle, or by it (Additive).
            408 => {
                let deg = o.get("degrees").and_then(Value::as_f64).unwrap_or(0.0);
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
                    call(&format!("Source[\"{src}\"]"), "updateSettings", json!([{k: v}])).await?;
                }
            }
            413 => {
                let audio = call("AudioService", "getSourcesForScene", json!([scene_id])).await?;
                let name = s(o, "sourceName");
                for a in audio.as_array().into_iter().flatten().filter(|a| a["name"] == name) {
                    let v = wanted(state, a["muted"].as_bool().unwrap_or(false));
                    call(a["resourceId"].as_str().unwrap_or(""), "setMuted", json!([v])).await?;
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
}
