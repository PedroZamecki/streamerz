//! Streamer.bot's servers, as drop-ins: the WebSocket server (requests, authentication, events),
//! the HTTP server and the UDP server. Answers are shaped like the real Streamer.bot's (captured from
//! it). They run on the engine's thread and restart when their settings change; a server that's off
//! opens nothing.

use crate::engine::{events, Engine};
use crate::model::Obj;
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::rc::Rc;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

/// The Streamer.bot API level copied (GetInfo/Hello report it, so version-checking clients work).
pub const API_VERSION: &str = "1.0.7";
const EVENTS: &str = include_str!("events.json");

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WsSettings {
    pub auto_start: bool,
    pub address: String,
    pub port: u16,
    pub endpoint: String,
    pub enable_auth: bool,
    pub auth_enforce: bool,
    pub auth_password: String,
    /// WebSocket clients ({id, name, endpoint, autoConnect, reconnectOnDisconnect, retryInterval, ...})
    /// and custom servers ({id, name, address, port, endpoint, autoStart, ...}).
    pub connections: Vec<Obj>,
    pub servers: Vec<Obj>,
    /// Websites (origins like `https://overlays.example.com`), one per line, allowed to connect from
    /// a browser besides local pages. Also used by the HTTP server.
    pub allowed_origins: String,
}

impl Default for WsSettings {
    fn default() -> Self {
        WsSettings {
            auto_start: true,
            address: "127.0.0.1".into(),
            port: 8080,
            endpoint: "/".into(),
            enable_auth: false,
            auth_enforce: false,
            auth_password: String::new(),
            connections: Vec::new(),
            servers: Vec::new(),
            allowed_origins: String::new(),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HttpSettings {
    pub auto_start: bool,
    pub address: String,
    pub port: u16,
}

impl Default for HttpSettings {
    fn default() -> Self {
        HttpSettings {
            auto_start: false,
            address: "127.0.0.1".into(),
            port: 7474,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UdpSettings {
    pub auto_start: bool,
    pub port: u16,
}

impl Default for UdpSettings {
    fn default() -> Self {
        UdpSettings {
            auto_start: false,
            port: 4242,
        }
    }
}

/// Start the servers that are on, and restart them when their settings change.
pub async fn run(e: Rc<Engine>) {
    if e.config().instance_id.is_empty() {
        e.change(|c| c.instance_id = crate::engine::uuid());
    }
    let mut rx = e.subscribe();
    let mut running: Vec<tokio::task::AbortHandle> = Vec::new();
    let mut extra: Vec<tokio::task::AbortHandle> = Vec::new();
    let mut last = None;
    let mut last_extra = None;
    let mut deck: Option<(crate::streamdeck::Settings, Option<tokio::task::AbortHandle>)> = None;
    loop {
        // The Stream Deck plugin's server: on only when turned on, restarted when its settings change.
        let now_deck = rx.borrow().stream_deck.clone();
        if deck.as_ref().map(|d| &d.0) != Some(&now_deck) {
            if let Some(h) = deck.take().and_then(|d| d.1) {
                h.abort();
            }
            let h = now_deck
                .auto_start
                .then(|| tokio::task::spawn_local(crate::streamdeck::run(e.clone(), now_deck.clone())).abort_handle());
            deck = Some((now_deck, h));
        }
        let now_extra = {
            let c = rx.borrow();
            (c.websockets.connections.clone(), c.websockets.servers.clone())
        };
        if last_extra.as_ref() != Some(&now_extra) {
            for h in extra.drain(..) {
                h.abort();
            }
            let (clients, servers) = now_extra.clone();
            for c in clients.into_iter().filter(|c| flag(c, "autoConnect")) {
                extra.push(tokio::task::spawn_local(ws_client(e.clone(), c)).abort_handle());
            }
            for s in servers.into_iter().filter(|s| flag(s, "autoStart")) {
                extra.push(tokio::task::spawn_local(custom_server(e.clone(), s)).abort_handle());
            }
            last_extra = Some(now_extra);
        }
        let now = {
            let c = rx.borrow_and_update();
            let mut ws = c.websockets.clone();
            // Clients and custom servers don't restart these.
            (ws.connections, ws.servers) = (Vec::new(), Vec::new());
            (ws, c.http.clone(), c.udp.clone())
        };
        if last.as_ref() != Some(&now) {
            for h in running.drain(..) {
                h.abort();
            }
            let (ws, http, udp) = now.clone();
            if ws.auto_start {
                running.push(tokio::task::spawn_local(websocket(e.clone(), ws)).abort_handle());
            }
            if http.auto_start {
                running.push(tokio::task::spawn_local(http_server(e.clone(), http)).abort_handle());
            }
            if udp.auto_start {
                running.push(tokio::task::spawn_local(udp_server(e.clone(), udp)).abort_handle());
            }
            last = Some(now);
        }
        // Opt-in: listens only while a Hotkey Press (or Mouse Click) trigger is used.
        e.hotkeys_sync();
        // Opt-in: the process list is watched only while Process Started or Stopped is used.
        e.processes_sync();
        e.midi_sync();
        e.speech_sync();
        if rx.changed().await.is_err() {
            return;
        }
    }
}

fn flag(o: &Obj, k: &str) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// A remote instance reached by a WebSocket client: it leaves (Remote Instance Disconnected) when
/// this is dropped.
struct RemoteGuard(Rc<Engine>, Option<String>);

impl Drop for RemoteGuard {
    fn drop(&mut self) {
        if let Some(id) = self.1.take() {
            let name = self.0.remote_remove(&id);
            self.0.remote_disconnected(&id, &name);
        }
    }
}

fn text_of(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn field<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

/// Fire a WebSocket client/custom server trigger (filtered by its clientId/serverId) and its event.
fn ws_event(e: &Rc<Engine>, client: bool, kind: &str, owner: &Obj, args: Obj) {
    let id = field(owner, "id").to_string();
    let (code, name) = match (client, kind) {
        (true, "Open") => (803, "Websocket Client Opened"),
        (true, "Close") => (804, "Websocket Client Closed"),
        (true, _) => (805, "Websocket Client Message"),
        (false, "Open") => (2001, "Custom Server Connection Opened"),
        (false, "Close") => (2002, "Custom Server Connection Closed"),
        (false, _) => (2003, "Custom Server Message"),
    };
    let (category, source, key, events) = if client {
        (
            "Core/Websocket/Client",
            "websocketclient",
            "clientId",
            "WebsocketClient",
        )
    } else {
        (
            "Core/Websocket/Custom Server",
            "websocketcustomserver",
            "serverId",
            "WebsocketCustomServer",
        )
    };
    e.fire(code, name, category, source, &args, |t| field(t, key) == id);
    e.emit(events, kind, || {
        let mut d = json!({"id": id, "name": field(owner, "name")});
        if let Some(o) = d.as_object_mut() {
            o.extend(args);
        }
        d
    });
}

/// A text or binary frame's text; None for the rest (and Some("") never).
fn frame_text(m: Message) -> Option<Result<String, ()>> {
    match m {
        Message::Text(t) => Some(Ok(t.to_string())),
        Message::Binary(b) => Some(Ok(String::from_utf8_lossy(&b).into_owned())),
        Message::Close(_) => Some(Err(())),
        _ => None,
    }
}

/// A WebSocket client: connects, fires Opened / Message / Closed; retries every retryInterval
/// seconds after a failed attempt, and after a disconnect when reconnectOnDisconnect is on (a failed
/// attempt fires Closed too, as in Streamer.bot).
async fn ws_client(e: Rc<Engine>, c: Obj) {
    let url = field(&c, "endpoint").to_string();
    let secs = c.get("retryInterval").and_then(Value::as_u64).unwrap_or(5).max(1);
    loop {
        match crate::http::ws_connect(url.as_str()).await {
            Ok((ws, _)) => {
                crate::say(format!("WebSocket client {}: connected to {url}", field(&c, "name")));
                ws_event(&e, true, "Open", &c, Obj::new());
                let (mut sink, mut stream) = ws.split();
                // What goes out (remote triggers and signals), once the server said who it is.
                let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                // Leaves the remote instances however the connection ends (closed, or the task aborted
                // when the settings change).
                let mut remote = RemoteGuard(e.clone(), None);
                loop {
                    let m = tokio::select! {
                        m = stream.next() => match m { Some(Ok(m)) => m, _ => break },
                        o = out_rx.recv() => {
                            if let Some(o) = o {
                                if sink.send(Message::Text(o.into())).await.is_err() {
                                    break;
                                }
                            }
                            continue;
                        }
                    };
                    match frame_text(m) {
                        Some(Ok(text)) => {
                            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                                // A Streamer.bot or Streamerz server: a remote instance from now on.
                                let info = &val["info"];
                                if remote.1.is_none() && val["request"] == "Hello" && info["instanceId"].is_string() {
                                    let (id, name) = (text_of(&info["instanceId"]), text_of(&info["name"]));
                                    e.remote_add(&id, &name, out_tx.clone());
                                    e.remote_connected(&id, &name);
                                    remote.1 = Some(id);
                                }
                                if let Some(event) = val.get("event") {
                                    if event.get("source").and_then(Value::as_str) == Some("StreamerbotRemote") {
                                        let ev_type = event.get("type").and_then(Value::as_str).unwrap_or("");
                                        let data =
                                            val.get("data").and_then(Value::as_object).cloned().unwrap_or_default();
                                        let inst = data.get("instanceId").and_then(Value::as_str).unwrap_or("");
                                        let args = data
                                            .get("arguments")
                                            .and_then(Value::as_object)
                                            .cloned()
                                            .unwrap_or_default();
                                        match ev_type {
                                            "InstanceTrigger" => {
                                                let ev = data.get("eventName").and_then(Value::as_str).unwrap_or("");
                                                e.fire_remote_trigger(inst, ev, args);
                                            }
                                            "InstanceSignal" => {
                                                let sig = data.get("signalName").and_then(Value::as_str).unwrap_or("");
                                                e.deliver_remote_signal(inst, sig, args);
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            let mut args = Obj::new();
                            args.insert("message".into(), text.into());
                            ws_event(&e, true, "Message", &c, args);
                        }
                        Some(Err(())) => break,
                        None => {}
                    }
                }
                drop(remote);
                ws_event(&e, true, "Close", &c, Obj::new());
                if !flag(&c, "reconnectOnDisconnect") {
                    return;
                }
            }
            Err(err) => {
                crate::warn(format!("WebSocket client {}: {url}: {err}", field(&c, "name")));
                ws_event(&e, true, "Close", &c, Obj::new());
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
    }
}

/// A custom WebSocket server: each connection gets a sessionId; fires Opened / Message (data) /
/// Closed, with sessionId and ip.
async fn custom_server(e: Rc<Engine>, s: Obj) {
    let addr = field(&s, "address").to_string();
    let port = s.get("port").and_then(Value::as_u64).unwrap_or(0) as u16;
    let name = field(&s, "name").to_string();
    let l = match TcpListener::bind((addr.as_str(), port)).await {
        Ok(l) => l,
        Err(err) => return crate::error(format!("custom server {name}: can't listen on {addr}:{port} ({err})")),
    };
    crate::say(format!(
        "custom server {name} on ws://{addr}:{port}{}",
        field(&s, "endpoint")
    ));
    loop {
        let Ok((tcp, peer)) = l.accept().await else { continue };
        let (e, s) = (e.clone(), s.clone());
        tokio::task::spawn_local(async move {
            let allowed = e.config().websockets.allowed_origins.clone();
            let Ok((ws, _)) = accept(tcp, field(&s, "endpoint").to_string(), allowed, false).await else {
                return;
            };
            let mut args = Obj::new();
            args.insert("sessionId".into(), crate::engine::uuid().into());
            args.insert("ip".into(), peer.ip().to_string().into());
            ws_event(&e, false, "Open", &s, args.clone());
            let (_sink, mut stream) = ws.split();
            while let Some(Ok(m)) = stream.next().await {
                match frame_text(m) {
                    Some(Ok(data)) => {
                        let mut a = args.clone();
                        a.insert("data".into(), data.into());
                        ws_event(&e, false, "Message", &s, a);
                    }
                    Some(Err(())) => break,
                    None => {}
                }
            }
            ws_event(&e, false, "Close", &s, args);
        });
    }
}

/// Whether a request may reach the servers. Browsers let any website open `ws://127.0.0.1` or post
/// to `http://127.0.0.1`, so a request from a browser (it sends `Origin`, or `Sec-Fetch-Site` for
/// a plain GET) must come from a local page (`null`/`file:` pages, OBS's local files, localhost)
/// or an allowed website. Apps send neither header and are always let in.
pub fn origin_ok(origin: Option<&str>, fetch_site: Option<&str>, allowed: &str) -> bool {
    let Some(o) = origin.map(|o| o.trim().trim_end_matches('/').to_ascii_lowercase()) else {
        // A GET from another site (an <img> or link) has no Origin, only Sec-Fetch-Site.
        return fetch_site.is_none_or(|f| !f.eq_ignore_ascii_case("cross-site"));
    };
    if o == "null" || o.starts_with("file:") {
        return true;
    }
    let host = o.split_once("://").map_or("", |x| x.1);
    let host = host
        .rsplit_once(':')
        .filter(|(h, p)| !p.contains(']') && !h.is_empty())
        .map_or(host, |x| x.0);
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
        || allowed
            .lines()
            .any(|a| a.trim().trim_end_matches('/').eq_ignore_ascii_case(&o))
}

/// Accept a WebSocket connection on `endpoint` only (404 elsewhere), from allowed origins only
/// (403). With `bridge`, the TikTok extension's path is open too, to the extension only; the bool
/// says it's that. The handshake callback's error type is tungstenite's (a whole HTTP response).
#[allow(clippy::result_large_err)]
async fn accept(
    tcp: TcpStream,
    endpoint: String,
    allowed: String,
    bridge: bool,
) -> Result<(tokio_tungstenite::WebSocketStream<TcpStream>, bool), String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
    use tokio_tungstenite::tungstenite::http::StatusCode;
    let is_bridge = Arc::new(AtomicBool::new(false));
    let seen = is_bridge.clone();
    let check = move |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let h = |k: &str| req.headers().get(k).and_then(|v| v.to_str().ok());
        let status = if bridge && req.uri().path() == crate::tiktok_bridge::PATH {
            if crate::tiktok_bridge::from_extension(h("origin")) {
                seen.store(true, Ordering::Relaxed);
                return Ok(resp);
            }
            StatusCode::FORBIDDEN
        } else if !endpoint.is_empty() && req.uri().path() != endpoint {
            StatusCode::NOT_FOUND
        } else if !origin_ok(h("origin"), h("sec-fetch-site"), &allowed) {
            crate::warn(format!(
                "WebSocket: refused a browser page from {} (add it to Servers > WebSocket > allowed websites to let it in)",
                h("origin").unwrap_or("another website")
            ));
            StatusCode::FORBIDDEN
        } else {
            return Ok(resp);
        };
        let mut r = ErrorResponse::new(None);
        *r.status_mut() = status;
        Err(r)
    };
    let ws = tokio_tungstenite::accept_hdr_async(tcp, check)
        .await
        .map_err(|e| e.to_string())?;
    Ok((ws, is_bridge.load(Ordering::Relaxed)))
}

fn info(e: &Engine) -> Value {
    let c = e.config();
    json!({"instanceId": c.instance_id, "name": c.instance_name, "version": API_VERSION,
        "os": if cfg!(windows) { "windows" } else { "linux" }, "osVersion": "unknown", "mode": "ui",
        "darkMode": c.dark, "source": "websocketServer",
        "streamerz": crate::brand::VERSION})
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn random_b64() -> String {
    let bytes: Vec<u8> = (0..4).flat_map(|_| crate::math::secure_u64().to_le_bytes()).collect();
    b64(&bytes)
}

/// Streamer.bot's (and obs-websocket's) auth: base64(sha256(base64(sha256(password + salt)) + challenge)).
pub fn auth_response(password: &str, salt: &str, challenge: &str) -> String {
    let secret = b64(&Sha256::digest(format!("{password}{salt}")));
    b64(&Sha256::digest(format!("{secret}{challenge}")))
}

/// The action a request names: {id} or {name}.
fn find_action(e: &Engine, a: &Value) -> Option<String> {
    let c = e.config();
    let all = &c.actions.actions;
    let id = a.get("id").and_then(Value::as_str).filter(|s| !s.is_empty());
    let name = a.get("name").and_then(Value::as_str).filter(|s| !s.is_empty());
    id.and_then(|id| all.iter().find(|x| x.id == id))
        .or_else(|| name.and_then(|n| all.iter().find(|x| x.name == n)))
        .map(|x| x.id.clone())
}

/// Prefix `id`, then the fields, then status ok.
fn ok(id: &Value, fields: Value) -> Value {
    let mut out = json!({"id": id});
    if let (Some(o), Some(f)) = (out.as_object_mut(), fields.as_object()) {
        o.extend(f.clone());
        o.insert("status".into(), "ok".into());
    }
    out
}

fn error(id: &Value, msg: &str) -> Value {
    json!({"id": id, "status": "error", "error": msg})
}

/// Answer a WebSocket request (not Authenticate, Subscribe or UnSubscribe, which need the
/// connection). None: an unknown request, answered with just its id.
pub(crate) fn answer(e: &Rc<Engine>, req: &Value, authed: bool) -> Option<Value> {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let get = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("");
    let persisted = req.get("persisted").and_then(Value::as_bool).unwrap_or(true);
    Some(match get("request") {
        "StreamerzFirstWords" if e.dev() => ok(&id, json!({"first":e.first_words(get("platform"), get("user"))})),
        "StreamerzMemory" => {
            if let Some(enabled) = req.get("enabled").and_then(Value::as_bool) {
                e.change(|c| c.memory_saver = enabled);
            }
            ok(
                &id,
                json!({"active":e.memory_saver,"enabled":e.config().memory_saver,
                "restartRequired":e.memory_saver != e.config().memory_saver,
                "firstWords":"disk in saver mode, memory otherwise"}),
            )
        }
        "GetInfo" => ok(&id, json!({"info": info(e)})),
        "GetEvents" => ok(
            &id,
            json!({"events": serde_json::from_str::<Value>(EVENTS).unwrap_or_default()}),
        ),
        "GetActions" => {
            let c = e.config();
            let actions: Vec<Value> = c
                .actions
                .actions
                .iter()
                .map(|a| {
                    json!({"id": a.id, "name": a.name, "group": a.group, "enabled": a.enabled,
                        "subaction_count": a.sub_actions.len(), "trigger_count": a.triggers.len()})
                })
                .collect();
            ok(&id, json!({"count": actions.len(), "actions": actions}))
        }
        "DoAction" => {
            let a = req.get("action").cloned().unwrap_or_default();
            let args = req.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
            match find_action(e, &a).and_then(|x| e.queue(&x, args)) {
                Some(args) => ok(&id, json!({"args": args})),
                None => {
                    let what = a.get("name").or(a.get("id")).and_then(Value::as_str).unwrap_or("");
                    error(&id, &format!("The action '{what}' was not found"))
                }
            }
        }
        "GetBroadcaster" => {
            let tw = e.twitch_broadcaster();
            let mut platforms = Obj::new();
            let mut connected: Vec<&str> = Vec::new();
            let mut disconnected: Vec<&str> = vec!["youtube", "kick"];
            if let Some(b) = tw {
                platforms.insert(
                    "twitch".into(),
                    json!({
                        "id": b.user_id,
                        "userName": b.login,
                        "user": b.display_name
                    }),
                );
                connected.push("twitch");
            } else {
                disconnected.insert(0, "twitch");
            }
            ok(
                &id,
                json!({"platforms": platforms, "connected": connected, "disconnected": disconnected}),
            )
        }
        "GetCredits" => ok(&id, e.credits()),
        "TestCredits" => ok(&id, e.test_credits()),
        "ClearCredits" => {
            e.reset_credits();
            ok(&id, json!({}))
        }
        "GetActiveViewers" => ok(&id, json!({"count": 0, "viewers": []})),
        // Code triggers are registered by C#, which Streamerz doesn't run.
        "GetCodeTriggers" => ok(&id, json!({"count": 0, "triggers": []})),
        "ExecuteCodeTrigger" => {
            let name = get("triggerName").to_string();
            let args = req.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
            e.fire(18002, "Custom Code Event", "Custom", "code", &args, |t| {
                t.get("triggerName").or(t.get("eventName")).and_then(Value::as_str) == Some(name.as_str())
            });
            ok(&id, json!({}))
        }
        "GetCommands" => {
            let c = e.config();
            let cmds: Vec<Value> = c
                .commands
                .iter()
                .map(|x| {
                    let f = |k: &str, d: Value| x.get(k).cloned().unwrap_or(d);
                    json!({"id": f("id", "".into()), "enabled": f("enabled", true.into()), "name": f("name", "".into()),
                        "group": f("group", "".into()), "commands": crate::commands::aliases(x),
                        "caseSensitive": f("caseSensitive", false.into()), "ignoreInternal": f("ignoreInternal", true.into()),
                        "ignoreBotAccount": f("ignoreBotAccount", true.into()), "sources": f("sources", 1.into())})
                })
                .collect();
            ok(&id, json!({"commands": cmds, "count": cmds.len()}))
        }
        "TwitchGetEmotes" => {
            if e.twitch_connected() {
                ok(&id, json!({"emotes": []}))
            } else {
                error(&id, "Twitch is not connected")
            }
        }
        "YouTubeGetEmotes" => error(&id, "YouTube is not connected"),
        "GetGlobals" => {
            let v = e.globals_list(persisted);
            ok(&id, json!({"variables": v, "count": v.len()}))
        }
        "GetGlobal" => {
            let name = get("variable");
            match e.globals_list(persisted).get(name) {
                Some(v) => ok(&id, json!({"variable": v})),
                None => error(&id, &format!("Variable '{name}' not found")),
            }
        }
        "TwitchGetUserGlobals" => {
            let v = e.user_globals_list(
                "twitch",
                Some(get("variable")).filter(|v| !v.is_empty()),
                None,
                persisted,
            );
            ok(&id, json!({"variables": v, "count": v.len()}))
        }
        "TwitchGetUserGlobal" => {
            let name = Some(get("variable")).filter(|v| !v.is_empty());
            let v = e.user_globals_list("twitch", name, Some(get("userId")), persisted);
            ok(&id, json!({"variables": v, "count": v.len()}))
        }
        "SendMessage" if !authed && e.config().websockets.enable_auth => {
            error(&id, "Authentication required to use this method")
        }
        "SendMessage" => {
            let msg = req
                .get("message")
                .or_else(|| req.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let bot = req.get("bot").and_then(Value::as_bool).unwrap_or(false);
            let platform = match get("platform").to_ascii_lowercase().as_str() {
                "" => "twitch".to_string(),
                p => p.to_string(),
            };
            if !crate::engine::chatlog::PLATFORMS.contains(&platform.as_str()) && platform != "all" {
                error(&id, &format!("unknown platform '{platform}'"))
            } else if platform != "all" && !e.chat_connected(&platform) && !e.dev() {
                error(&id, &format!("{platform} is not connected"))
            } else {
                e.send_chat(&platform, msg, bot);
                ok(&id, json!({}))
            }
        }
        "GetUserPronouns" => {
            let login = get("userLogin");
            let pronoun_val = match crate::remote::get_cached_pronoun(login) {
                Some(info) => crate::remote::pronoun_to_json(&info, true),
                None => crate::remote::default_pronoun_json(),
            };
            ok(&id, json!({"userLogin": login, "pronoun": pronoun_val}))
        }
        "ExecuteRemoteTrigger" | "SendRemoteTrigger" | "RemoteTrigger" => {
            let instance_id = get("instanceId");
            let event_name = get("eventName");
            let args = req.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
            e.fire_remote_trigger(instance_id, event_name, args);
            ok(&id, json!({}))
        }
        "SendRemoteSignal" | "RemoteSignal" => {
            let instance_id = get("instanceId");
            let signal_name = get("signalName");
            let args = req.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
            e.deliver_remote_signal(instance_id, signal_name, args);
            ok(&id, json!({}))
        }
        // Streamerz's own: read and write what's set up (actions, commands...), for the MCP server.
        "StreamerzGet"
        | "StreamerzSave"
        | "StreamerzDelete"
        | "StreamerzCountdown"
        | "StreamerzOverlays"
        | "StreamerzOverlayPublish"
        | "StreamerzManageActionGroup"
        | "StreamerzActionGroups"
        | "StreamerzGroupIcon"
        | "StreamerzActionExport"
        | "StreamerzActionImport"
        | "StreamerzGroupVariables"
        | "StreamerzSetGroupVariables"
        | "StreamerzTemplateAdd"
        | "StreamerzGroupMeta"
        | "StreamerzSetGroupMeta"
        | "StreamerzGroupExport"
        | "StreamerzSaveGroupFiles"
        | "StreamerzSearchChannels"
        | "StreamerzTikTokBridge"
            if !authed && e.config().websockets.enable_auth =>
        {
            error(&id, "Authentication required to use this method")
        }
        "StreamerzPlugins" => {
            let on = e.plugins_on();
            let list: Vec<Value> = crate::engine::plugins::installed()
                .into_iter()
                .map(|(id, _)| {
                    let m = on.iter().find(|p| p.0 == id).map(|p| &p.1);
                    json!({"id": id, "on": m.is_some(), "manifest": m})
                })
                .collect();
            ok(&id, json!({"plugins": list, "folder": crate::engine::plugins::dir()}))
        }
        "StreamerzPluginEnable" if !authed && e.config().websockets.enable_auth => {
            error(&id, "Authentication required to use this method")
        }
        "StreamerzPluginEnable" => {
            let on = req.get("on").and_then(Value::as_bool).unwrap_or(true);
            e.plugin_enable(get("plugin"), on);
            ok(&id, json!({"on": e.plugins_on().iter().any(|p| p.0 == get("plugin"))}))
        }
        "StreamerzGet" | "StreamerzSave" | "StreamerzDelete" => match authoring(e, req) {
            Ok(v) => ok(&id, v),
            Err(m) => error(&id, &m),
        },
        "StreamerzManageActionGroup"
        | "StreamerzActionGroups"
        | "StreamerzGroupVariables"
        | "StreamerzSetGroupVariables"
        | "StreamerzGroupMeta"
        | "StreamerzSetGroupMeta"
        | "StreamerzGroupExport"
        | "StreamerzSaveGroupFiles" => match group_request(e, req) {
            Ok(v) => ok(&id, v),
            Err(m) => error(&id, &m),
        },
        "StreamerzTemplateAdd" => match crate::templates::TEMPLATES
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(get("name")))
        {
            None => error(&id, "template not found"),
            Some(t) => {
                let made = (t.make)();
                let has_timers = !made.timers.is_empty();
                let mut result = Ok(());
                e.change(|c| result = made.add_to(c));
                if has_timers {
                    e.timers();
                }
                match result {
                    Ok(()) => ok(&id, json!({})),
                    Err(m) => error(&id, &m),
                }
            }
        },
        "StreamerzGroupIcon" => {
            let group = get("group");
            if group.is_empty() {
                error(&id, "group is required")
            } else if let Some(icon) = req.get("icon") {
                if !icon.is_null() && crate::group_icons::decode(icon).is_none() {
                    error(&id, "invalid icon")
                } else {
                    e.change(|c| {
                        if icon.is_null() {
                            c.action_group_icons.remove(group);
                        } else {
                            c.action_group_icons.insert(group.into(), icon.clone());
                        }
                    });
                    ok(&id, json!({}))
                }
            } else {
                ok(&id, json!({"icon": e.config().action_group_icons.get(group)}))
            }
        }
        "StreamerzActionExport" => {
            let ids: Vec<String> = req["ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            let meta = &req["meta"];
            let get_meta = |k| meta.get(k).and_then(Value::as_str).unwrap_or("");
            let export = crate::edit::export(
                &e.config(),
                &ids,
                [
                    get_meta("name"),
                    get_meta("author"),
                    get_meta("version"),
                    get_meta("description"),
                ],
            );
            ok(
                &id,
                json!({"export": export, "text": crate::model::write_export(&export)}),
            )
        }
        "StreamerzActionImport" => {
            if let Some(x) = req["export"].as_object() {
                let programs = crate::edit::programs(x);
                if !programs.is_empty() && req["allowPrograms"] != true {
                    error(
                        &id,
                        "Import contains programs; confirm them before setting allowPrograms",
                    )
                } else {
                    let mut result = Ok(Vec::new());
                    e.change(|c| result = crate::edit::import(c, x));
                    match result {
                        Ok(unsupported) => ok(&id, json!({"unsupported": unsupported})),
                        Err(msg) => error(&id, &msg),
                    }
                }
            } else {
                error(&id, "export must be an object")
            }
        }
        "StreamerzOverlays" => {
            let files: Vec<Value> = crate::overlays::list(&e.config())
                .into_iter()
                .map(|(g, f)| json!({"group": g, "file": f, "url": crate::overlays::url(&e.config(), &g, &f)}))
                .collect();
            let topics = e.overlay_topics();
            ok(
                &id,
                json!({"overlays": files, "topics": topics, "base": crate::overlays::base(&e.config())}),
            )
        }
        "StreamerzOverlayPublish" => {
            let topic = get("topic").trim();
            if topic.is_empty() {
                error(&id, "topic is required")
            } else {
                e.overlay_publish(topic, req.get("value").cloned().unwrap_or(Value::Null));
                ok(&id, json!({}))
            }
        }
        // Sending to TikTok through the browser: status, install (writes the extension) or remove.
        "StreamerzTikTokBridge" => {
            let done = match get("action") {
                "" | "status" => Ok(()),
                "install" => crate::tiktok_bridge::install(e).map(drop),

                "remove" => {
                    crate::tiktok_bridge::remove(e);
                    Ok(())
                }
                other => Err(format!("unknown action {other}")),
            };
            match done {
                Ok(()) => ok(&id, crate::tiktok_bridge::status(e)),
                Err(msg) => error(&id, &msg),
            }
        }
        "StreamerzCountdown" => {
            let clock_id = get("clockId");
            match crate::templates::clock::get(&e.config(), clock_id, crate::templates::clock::now_ms()) {
                Some(snapshot) => ok(&id, json!({"clockId": clock_id, "snapshot": snapshot})),
                None => error(&id, "Countdown not found"),
            }
        }
        // Dev mode only (STREAMERZ_DEV=1): fake chat in, and the chat that would have gone out.
        "InjectChat" if e.dev() => match e.inject_chat(
            get("platform"),
            get("user"),
            get("userId"),
            get("role"),
            get("text"),
            req,
        ) {
            Ok(ran) => ok(&id, json!({"ran": ran})),
            Err(m) => error(&id, &m),
        },
        "SimulateStream" if e.dev() => {
            let platform = get("platform");
            if !crate::engine::chatlog::PLATFORMS.contains(&platform) {
                error(&id, "Unknown platform")
            } else {
                e.chat_stream(platform, req.get("live").and_then(Value::as_bool).unwrap_or(false));
                ok(&id, json!({}))
            }
        }
        "SimulateChatConnection" if e.dev() => {
            let platform = get("platform");
            if !crate::engine::chatlog::PLATFORMS.contains(&platform) {
                error(&id, "Unknown platform")
            } else {
                e.simulate_chat_connection(platform, req.get("connected").and_then(Value::as_bool));
                ok(&id, json!({}))
            }
        }
        "SimulateChatModes" if e.dev() => {
            let platform = get("platform");
            if !["twitch", "kick"].contains(&platform) {
                error(&id, "Unknown platform")
            } else {
                e.set_chat_modes(
                    platform,
                    Some(crate::engine::ChatModes {
                        emote_only: req.get("emote_only").and_then(Value::as_bool).unwrap_or(false),
                        slow_seconds: req.get("slow_seconds").and_then(Value::as_u64),
                    }),
                );
                ok(&id, json!({}))
            }
        }
        "SimulateChatViewers" if e.dev() => {
            let platform = get("platform");
            if !crate::engine::chatlog::PLATFORMS.contains(&platform) {
                error(&id, "Unknown platform")
            } else {
                e.set_chat_viewers(platform, req.get("count").and_then(Value::as_u64));
                ok(&id, json!({}))
            }
        }
        // Dev mode only: a Twitch ad break of "seconds" as overlays see it (the Ad Run trigger is FireTrigger's).
        "SimulateAdBreak" if e.dev() => {
            let seconds = req.get("seconds").and_then(Value::as_u64).unwrap_or(30);
            let automatic = req.get("automatic").and_then(Value::as_bool).unwrap_or(false);
            e.overlay_ad(seconds, automatic, &crate::engine::globals::iso_now());
            ok(&id, json!({}))
        }
        // Dev mode only: a message pinned in a chat ("seconds" 0: no end), or none ("unpin": true).
        "InjectPin" if e.dev() => {
            let platform = get("platform").to_string();
            if req.get("unpin").and_then(Value::as_bool) == Some(true) {
                e.chat_unpinned(&platform, "");
            } else {
                let secs = req.get("seconds").and_then(Value::as_u64).unwrap_or(0);
                e.chat_pinned(crate::engine::chatlog::Pin {
                    platform,
                    msg_id: crate::engine::uuid(),
                    user: get("user").into(),
                    text: get("text").into(),
                    by: get("by").into(),
                    until: (secs > 0).then(|| crate::fmt::unix_now().as_millis() as u64 + secs * 1000),
                });
            }
            ok(&id, json!({}))
        }
        // Dev mode only: a poll or prediction at the top of a chat: "title", "choices" as [[name, votes]],
        // "seconds" left (0: none), "prediction" and "over" as booleans.
        "InjectPoll" if e.dev() => {
            let flag = |k: &str| req.get(k).and_then(Value::as_bool) == Some(true);
            let now = crate::fmt::unix_now().as_millis() as u64;
            let secs = req.get("seconds").and_then(Value::as_u64).unwrap_or(0);
            e.chat_poll(crate::engine::chatlog::Poll {
                platform: get("platform").into(),
                id: get("id").into(),
                title: get("title").into(),
                prediction: flag("prediction"),
                choices: req
                    .get("choices")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|c| (c[0].as_str().unwrap_or("").to_string(), c[1].as_u64().unwrap_or(0)))
                    .collect(),
                ends: (secs > 0).then(|| now + secs * 1000),
                over: flag("over").then_some(now),
            });
            ok(&id, json!({}))
        }
        "GetChatOut" if e.dev() => ok(&id, json!({"messages": e.dev_take().unwrap_or_default()})),
        // Dev mode only: fire any trigger (code or name, e.g. 101 or "TwitchFollow") with these
        // arguments, as its platform would; how many actions ran.
        "FireTrigger" if e.dev() => {
            let t = req.get("trigger").cloned().unwrap_or_default();
            let found = crate::types::TRIGGERS.iter().find(|(c, n)| {
                t.as_u64() == Some(u64::from(*c)) || t.as_str().is_some_and(|s| s.eq_ignore_ascii_case(n))
            });
            match found {
                Some(&(code, name)) => {
                    let args = req.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
                    let ran = e.fire(code, name, "Test", "test", &args, |_| true);
                    ok(&id, json!({"trigger": code, "name": name, "ran": ran}))
                }
                None => error(&id, &format!("no trigger {t}")),
            }
        }
        // Dev mode only: the log's last lines (up to `lines`, default 50).
        "GetLog" if e.dev() => {
            let n = req.get("lines").and_then(Value::as_u64).unwrap_or(50) as usize;
            ok(&id, json!({"lines": crate::log_tail_tagged(n)}))
        }
        // Dev mode only: what's connected, and how much is set up.
        "GetStatus" if e.dev() => {
            let c = e.config();
            ok(
                &id,
                json!({"info": info(e), "connected": {
                    "twitch": e.twitch_connected(), "youtube": e.youtube_connected(), "kick": e.kick_connected(),
                    "tiktok": e.tiktok_connected(), "obs": e.obs_connected(None), "streamlabsDesktop": e.slobs_connected(),
                    "streamElements": e.donations.borrow().streamelements.get(),
                    "streamlabs": e.donations.borrow().streamlabs.get()},
                    "actions": c.actions.actions.len(), "commands": c.commands.len(), "timers": c.timers.len()}),
            )
        }
        _ => return None,
    })
}

/// The config's lists that can be read and written, by name.
const KINDS: [&str; 5] = ["action", "command", "timer", "group", "queue"];

/// The requests about action groups: StreamerzManageActionGroup {operation: create|rename|delete, group,
/// name?}, StreamerzActionGroups, StreamerzGroupVariables {group} and StreamerzSetGroupVariables {group,
/// values}, StreamerzGroupMeta {group} and StreamerzSetGroupMeta {group, meta}, StreamerzGroupExport
/// {group, meta?} and StreamerzSaveGroupFiles {group, folder}.
fn group_request(e: &Rc<Engine>, req: &Value) -> Result<Value, String> {
    use crate::groups;
    let text = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("");
    let group = text("group");
    if group.is_empty() && req["request"] != "StreamerzActionGroups" {
        return Err("group is required".into());
    }
    let known = |e: &Rc<Engine>| {
        if groups::exists(&e.config(), group) {
            Ok(())
        } else {
            Err("group not found".to_string())
        }
    };
    match req["request"].as_str().unwrap_or("") {
        "StreamerzManageActionGroup" => {
            let mut result = Ok(());
            let (op, name) = (text("operation"), text("name"));
            e.change(|c| {
                result = match op {
                    "create" if groups::create(c, group) => Ok(()),
                    "create" => Err("group is empty or already exists"),
                    "rename" if groups::rename(c, group, name) => Ok(()),
                    "rename" => Err("group not found, or the new name is empty or already exists"),
                    "delete" if groups::exists(c, group) => {
                        groups::remove(c, group);
                        Ok(())
                    }
                    "delete" => Err("group not found"),
                    _ => Err("unknown operation"),
                };
            });
            result.map(|_| json!({})).map_err(str::to_string)
        }
        "StreamerzActionGroups" => Ok(json!({"groups": groups::names(&e.config())})),
        "StreamerzGroupVariables" => {
            known(e)?;
            Ok(json!({"values": groups::values(&e.config(), group)}))
        }
        "StreamerzSetGroupVariables" => {
            known(e)?;
            let values = req["values"].as_object().ok_or("values must be an object")?.clone();
            let mut result = Ok(());
            e.change(|c| result = groups::set_values(c, group, values));
            result.map(|_| json!({}))
        }
        "StreamerzGroupMeta" => {
            known(e)?;
            let c = e.config();
            let (commands, timers) = groups::members(&c, group);
            Ok(json!({"meta": groups::meta(&c, group), "commands": commands, "timers": timers}))
        }
        "StreamerzSetGroupMeta" => {
            known(e)?;
            let meta: groups::Meta =
                serde_json::from_value(req["meta"].clone()).map_err(|err| format!("invalid meta: {err}"))?;
            let mut result = Ok(());
            e.change(|c| {
                let ids = |list: &[crate::model::Obj]| -> Vec<String> {
                    list.iter()
                        .filter_map(|o| o.get("id").and_then(Value::as_str).map(str::to_owned))
                        .collect()
                };
                let (commands, timers) = (ids(&c.commands), ids(&c.timers));
                result = if let Some(id) = meta.commands.iter().find(|i| !commands.contains(i)) {
                    Err(format!("command not found: {id}"))
                } else if let Some(id) = meta.timers.iter().find(|i| !timers.contains(i)) {
                    Err(format!("timer not found: {id}"))
                } else {
                    groups::set_meta(c, group, meta)
                };
                if result.is_ok() {
                    // New settings start at their defaults; the ones already set stay.
                    let values = groups::values(c, group);
                    result = groups::set_values(c, group, values);
                }
            });
            result.map(|_| json!({}))
        }
        "StreamerzGroupExport" => {
            known(e)?;
            let m = &req["meta"];
            let get = |k| m.get(k).and_then(Value::as_str).unwrap_or("");
            let export = crate::edit::export_group(
                &e.config(),
                group,
                [
                    if get("name").is_empty() { group } else { get("name") },
                    get("author"),
                    get("version"),
                    get("description"),
                ],
            );
            Ok(json!({"text": crate::model::write_export(&export), "export": export}))
        }
        "StreamerzSaveGroupFiles" => {
            known(e)?;
            let files = groups::meta(&e.config(), group).files;
            let saved = groups::save_files(&files, std::path::Path::new(text("folder")))?;
            Ok(json!({"files": saved}))
        }
        _ => Err("unknown group request".into()),
    }
}

/// StreamerzGet {what, id?|name?}: one item, or all of them; StreamerzSave {what, item}: adds it (a new
/// id when it has none) or replaces the one with its id; StreamerzDelete {what, id}.
fn authoring(e: &Rc<Engine>, req: &Value) -> Result<Value, String> {
    let what = req.get("what").and_then(Value::as_str).unwrap_or("");
    if !KINDS.contains(&what) {
        return Err(format!("what: one of {}", KINDS.join(", ")));
    }
    let list = |c: &crate::Config| -> Vec<Value> {
        match what {
            "action" => c
                .actions
                .actions
                .iter()
                .filter_map(|a| serde_json::to_value(a).ok())
                .collect(),
            "command" => c.commands.iter().cloned().map(Value::Object).collect(),
            "timer" => c.timers.iter().cloned().map(Value::Object).collect(),
            "group" => c.groups.iter().cloned().map(Value::Object).collect(),
            _ => c.actions.queues.iter().cloned().map(Value::Object).collect(),
        }
    };
    let key = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let (want_id, want_name) = (key(req, "id"), key(req, "name"));
    match req.get("request").and_then(Value::as_str).unwrap_or("") {
        "StreamerzGet" => {
            let all = list(&e.config());
            if want_id.is_empty() && want_name.is_empty() {
                return Ok(json!({ "items": all }));
            }
            all.into_iter()
                .find(|v| {
                    (!want_id.is_empty() && key(v, "id") == want_id)
                        || (!want_name.is_empty() && key(v, "name") == want_name)
                })
                .map(|v| json!({ "item": v }))
                .ok_or_else(|| format!("no {what} {want_id}{want_name}"))
        }
        "StreamerzSave" => {
            let Some(Value::Object(mut item)) = req.get("item").cloned() else {
                return Err("item: an object".into());
            };
            if key(&Value::Object(item.clone()), "id").is_empty() {
                item.insert("id".into(), crate::engine::uuid().into());
            }
            let new_id = key(&Value::Object(item.clone()), "id");
            if what == "action" {
                let a: crate::model::Action =
                    serde_json::from_value(Value::Object(item)).map_err(|err| format!("not an action: {err}"))?;
                e.change(|c| match c.actions.actions.iter_mut().find(|x| x.id == a.id) {
                    Some(x) => *x = a,
                    None => c.actions.actions.push(a),
                });
            } else {
                e.change(|c| {
                    let l = match what {
                        "command" => &mut c.commands,
                        "timer" => &mut c.timers,
                        "group" => &mut c.groups,
                        _ => &mut c.actions.queues,
                    };
                    let same = |x: &crate::model::Obj| x.get("id").and_then(Value::as_str) == Some(new_id.as_str());
                    match l.iter_mut().find(|x| same(x)) {
                        Some(x) => *x = item,
                        None => l.push(item),
                    }
                });
                if what == "timer" {
                    e.timers();
                }
            }
            Ok(json!({ "saved": new_id }))
        }
        _ => {
            if want_id.is_empty() {
                return Err("id: the one to delete".into());
            }
            let mut gone = false;
            e.change(|c| {
                let (before, after) = if what == "action" {
                    let before = c.actions.actions.len();
                    c.actions.actions.retain(|x| x.id != want_id);
                    (before, c.actions.actions.len())
                } else {
                    let l = match what {
                        "command" => &mut c.commands,
                        "timer" => &mut c.timers,
                        "group" => &mut c.groups,
                        _ => &mut c.actions.queues,
                    };
                    let before = l.len();
                    l.retain(|x| x.get("id").and_then(Value::as_str) != Some(want_id.as_str()));
                    (before, l.len())
                };
                gone = after < before;
            });
            if gone {
                Ok(json!({}))
            } else {
                Err(format!("no {what} {want_id}"))
            }
        }
    }
}

async fn websocket(e: Rc<Engine>, s: WsSettings) {
    let reserved = if crate::brand::IS_DEV {
        crate::take_dev_listener(s.port)
    } else {
        None
    };
    let listener = match reserved {
        Some(reserved) => TcpListener::from_std(reserved),
        None => TcpListener::bind((s.address.as_str(), s.port)).await,
    };
    let l = match listener {
        Ok(l) => l,
        Err(err) => {
            return crate::error(format!(
                "WebSocket server: can't listen on {}:{} ({err})",
                s.address, s.port
            ))
        }
    };
    crate::say(format!(
        "WebSocket server on ws://{}:{}{}",
        s.address, s.port, s.endpoint
    ));
    // The TikTok extension connects here: its files follow this address and version.
    crate::tiktok_bridge::refresh(&e.config());
    let mut next = 0u64;
    loop {
        let Ok((tcp, _)) = l.accept().await else { continue };
        next += 1;
        let (e, s, id) = (e.clone(), s.clone(), next);
        tokio::task::spawn_local(async move {
            if let Err(err) = connection(&e, &s, tcp, id).await {
                crate::warn(format!("WebSocket client: {err}"));
            }
            e.listen(id, &events::Subs::new(), &tokio::sync::mpsc::unbounded_channel().0);
        });
    }
}

/// Each connector behind a port, without secrets: for the MCP `connectors` tool.
pub(crate) fn connectors(e: &Rc<Engine>) -> Value {
    let c = e.config();
    let obs: Vec<Value> = c
        .obs
        .connections
        .iter()
        .map(crate::obs::ObsConnInfo::from_obj)
        .map(|i| {
            json!({"id": i.id, "name": i.name, "host": i.host, "port": i.port, "autoConnect": i.auto_connect,
            "connected": e.obs_connected(Some(&i.id))})
        })
        .collect();
    json!({
        "obs": {"started": e.obs().is_some(), "connections": obs},
        "spotify": {"enabled": c.spotify.enabled, "signedIn": !c.spotify.token.refresh_token.is_empty()},
    })
}

/// Channel search for the picker and MCP, including signed TikTok requests.
pub(crate) async fn search_request(e: &Rc<Engine>, req: &Value) -> Result<Value, String> {
    let platform = req["platform"].as_str().unwrap_or_default();
    let query = req["query"].as_str().unwrap_or_default().trim();
    let found = if query.is_empty() {
        Vec::new()
    } else if platform == "tiktok" {
        crate::tiktok_bridge::search(e, query).await?
    } else if crate::search::can(platform) {
        crate::search::channels(platform, query).await
    } else {
        return Err("Unknown platform".into());
    };
    Ok(
        json!({"channels": found.iter().map(|f| json!({"handle": f.handle, "name": f.name,
        "picture": f.picture, "detail": f.detail, "live": f.live})).collect::<Vec<_>>()}),
    )
}

pub(crate) async fn obs_request(e: &Rc<Engine>, req: &Value) -> Result<Value, String> {
    let kind = req["requestType"]
        .as_str()
        .filter(|k| !k.is_empty())
        .ok_or("requestType is required")?;
    if e.obs().is_none() && e.config().obs.connections.is_empty() {
        return Err("No OBS connection is set up".into());
    }
    let connection = req["connectionId"].as_str().filter(|c| !c.is_empty());
    let data = req
        .get("requestData")
        .filter(|d| d.is_object())
        .cloned()
        .unwrap_or(json!({}));
    let response = e.obs_start().send_request(connection, kind, data).await?;
    Ok(json!({"responseData": response}))
}

pub(crate) async fn spotify_request(e: &Rc<Engine>, req: &Value) -> Result<Value, String> {
    let music = e.music();
    match req["request"].as_str().unwrap_or("") {
        "StreamerzSpotifyLocal" => Ok(json!({"track": crate::spotify::local::current().await?, "source": "local"})),
        "StreamerzSpotifyCurrent" => {
            let track = music.current(e).await?;
            let source = if track.as_ref().is_some_and(|t| t.uri.is_empty()) {
                "local"
            } else {
                "api"
            };
            Ok(json!({"track": track, "source": source}))
        }
        "StreamerzSpotifyFind" => Ok(json!({"track": music.find(e, req["query"].as_str().unwrap_or("")).await?})),
        "StreamerzSpotifyPlayer" => {
            let player = music.player(e).await?;
            let queue: Vec<Value> = player
                .queue
                .iter()
                .map(|t| json!({"uri": t.uri, "name": t.name, "artists": t.artists}))
                .collect();
            Ok(
                json!({"playing": player.playing, "progressMs": player.progress_ms, "current": player.current,
                "queue": queue, "requests": e.song_requests()}),
            )
        }
        "StreamerzSpotifyQueue" => Ok(json!({"uri": music.queue(e, req["query"].as_str().unwrap_or("")).await?})),
        "StreamerzSpotifyPlayback" => {
            music.playback(e, req["command"].as_str().unwrap_or("")).await?;
            Ok(json!({}))
        }
        _ => Err("Unknown Spotify request".into()),
    }
}

async fn connection(e: &Rc<Engine>, s: &WsSettings, tcp: TcpStream, conn: u64) -> Result<(), String> {
    let bridge = crate::tiktok_bridge::set_up(&e.config());
    let (ws, is_bridge) = accept(tcp, s.endpoint.clone(), s.allowed_origins.clone(), bridge).await?;
    if is_bridge {
        crate::tiktok_bridge::serve(e.clone(), ws).await;
        return Ok(());
    }
    let (mut sink, mut stream) = ws.split();
    let (salt, challenge) = (random_b64(), random_b64());
    let mut hello = json!({"timestamp": crate::engine::globals::iso_now(), "session": crate::engine::uuid(),
        "request": "Hello", "info": info(e)});
    if s.enable_auth {
        hello["authentication"] = json!({"salt": salt, "challenge": challenge});
    }
    let text = |v: Value| Message::Text(v.to_string().into());
    sink.send(text(hello)).await.map_err(|e| e.to_string())?;
    let close = |reason: &'static str| {
        Message::Close(Some(CloseFrame {
            code: CloseCode::from(4009),
            reason: reason.into(),
        }))
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Arc<str>>();
    let mut subs = events::Subs::new();
    let mut authed = false;
    loop {
        tokio::select! {
            m = stream.next() => {
                let t = match m {
                    Some(Ok(Message::Text(t))) => t,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return Ok(()),
                    _ => continue,
                };
                let req = serde_json::from_str::<Value>(&t).ok().filter(|r| r.get("request").and_then(Value::as_str).is_some());
                let Some(req) = req else {
                    sink.send(Message::Text("{ \"error\": \"malformed command\" }".into())).await.map_err(|e| e.to_string())?;
                    continue;
                };
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let kind = req["request"].as_str().unwrap_or("");
                if s.enable_auth && s.auth_enforce && !authed && kind != "Authenticate" {
                    let _ = sink.send(close("Authentication required")).await;
                    return Ok(());
                }
                let reply = match kind {
                    "Authenticate" => {
                        let given = req.get("authentication").and_then(Value::as_str).unwrap_or("");
                        if s.enable_auth && given != auth_response(&s.auth_password, &salt, &challenge) {
                            let _ = sink.send(close("Authentication failed")).await;
                            return Ok(());
                        }
                        authed = true;
                        json!({"id": id, "status": "ok"})
                    }
                    "Subscribe" | "UnSubscribe" => {
                        let ev = req.get("events").cloned().unwrap_or(json!({}));
                        events::update(&mut subs, &ev, kind == "Subscribe");
                        e.listen(conn, &subs, &tx);
                        ok(&id, json!({"events": ev}))
                    }
                    "StreamerzSearchChannels" => {
                        if s.enable_auth && !authed { error(&id, "Authentication required to use this method") }
                        else { match search_request(e, &req).await { Ok(v) => ok(&id, v), Err(err) => error(&id, &err) } }
                    }
                    "StreamerzTikTokBridge" if req["action"] == "login" => {
                        if s.enable_auth && !authed { error(&id, "Authentication required to use this method") }
                        else { match e.tiktok_bridge.login().await { Ok(()) => ok(&id, crate::tiktok_bridge::status(e)), Err(err) => error(&id, &err) } }
                    }
                    "StreamerzSpotifyLocal"
                    | "StreamerzSpotifyCurrent"
                    | "StreamerzSpotifyQueue"
                    | "StreamerzSpotifyFind"
                    | "StreamerzSpotifyPlayer"
                    | "StreamerzSpotifyPlayback" => {
                        if s.enable_auth && !authed {
                            error(&id, "Authentication required to use this method")
                        } else {
                            match spotify_request(e, &req).await {
                                Ok(fields) => ok(&id, fields),
                                Err(message) => {
                                    if kind != "StreamerzSpotifyLocal" {
                                        crate::spotify::event(e, 99806, &message);
                                    }
                                    error(&id, &message)
                                },
                            }
                        }
                    }
                    "StreamerzConnectors" | "StreamerzObsRequest" => {
                        if s.enable_auth && !authed {
                            error(&id, "Authentication required to use this method")
                        } else if kind == "StreamerzConnectors" {
                            ok(&id, connectors(e))
                        } else if !e.dev() {
                            error(&id, "StreamerzObsRequest needs STREAMERZ_DEV=1")
                        } else {
                            match obs_request(e, &req).await {
                                Ok(fields) => ok(&id, fields),
                                Err(message) => error(&id, &message),
                            }
                        }
                    }
                    // Dev mode: what the window looks like, and where things are (see ui/dev.rs).
                    "StreamerzUi" if e.dev() => {
                        let cmd = req.get("do").and_then(Value::as_str).unwrap_or_default().to_string();
                        match crate::ui::dev(cmd) {
                            Some(rx) => ok(&id, json!({"result": rx.await.unwrap_or_default()})),
                            None => error(&id, "no window"),
                        }
                    }
                    _ => answer(e, &req, authed).unwrap_or_else(|| json!({"id": id})),
                };
                sink.send(text(reply)).await.map_err(|e| e.to_string())?;
            }
            Some(ev) = rx.recv() => {
                sink.send(Message::Text(ev.to_string().into())).await.map_err(|e| e.to_string())?;
            }
        }
    }
}

async fn http_server(e: Rc<Engine>, s: HttpSettings) {
    let l = match TcpListener::bind((s.address.as_str(), s.port)).await {
        Ok(l) => l,
        Err(err) => return crate::error(format!("HTTP server: can't listen on {}:{} ({err})", s.address, s.port)),
    };
    crate::say(format!("HTTP server on http://{}:{}/", s.address, s.port));
    // The open connections (an overlay's live feed stays open) end with the server.
    let mut open = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            accepted = l.accept() => {
                let Ok((tcp, _)) = accepted else { continue };
                let e = e.clone();
                open.spawn_local(async move {
                    let _ = http_one(&e, tcp).await;
                });
            }
            Some(_) = open.join_next(), if !open.is_empty() => {}
        }
    }
}

/// The answer to one HTTP request: status and JSON body (None: no body).
/// `signature`: a webhook's signature header (Patreon's), checked by the service's handler.
fn http_answer(e: &Rc<Engine>, method: &str, path: &str, body: &[u8], signature: Option<&str>) -> (u16, Option<Value>) {
    let invalid = || {
        let msg = format!("'{method} {path}' is an invalid request");
        (418, Some(json!({"status": "error", "error": msg})))
    };
    match (method, path) {
        ("GET", "/GetActions") => {
            let c = e.config();
            let actions: Vec<Value> = c
                .actions
                .actions
                .iter()
                .map(|a| json!({"id": a.id, "name": a.name, "group": a.group, "enabled": a.enabled, "subactions_count": a.sub_actions.len()}))
                .collect();
            (200, Some(json!({"count": actions.len(), "actions": actions})))
        }
        ("GET", "/GetCredits") => (200, Some(e.credits())),
        ("GET", "/TestCredits") => (200, Some(e.test_credits())),
        ("GET", "/ClearCredits") => {
            e.reset_credits();
            (204, None)
        }
        ("GET", "/ClearFirstWordsCache") => {
            e.reset_first_words(None);
            (204, None)
        }
        ("GET", "/") => (500, None),
        ("POST", "/DoAction") => {
            let req: Value = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(err) => {
                    let data = json!({"message": err.to_string()});
                    return (
                        400,
                        Some(json!({"status": "error", "error": "unable to parse data provided", "data": data})),
                    );
                }
            };
            let a = req.get("action").cloned().unwrap_or_default();
            let args = req.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
            match find_action(e, &a).and_then(|x| e.queue(&x, args)) {
                Some(_) => (204, None),
                None => (
                    400,
                    Some(json!({"status": "error", "error": "action not found", "data": {"action": a}})),
                ),
            }
        }
        // A Streamerz custom webhook (Streamer.bot's are made on its website): fires Custom Webhook.
        ("POST", path) if path.starts_with("/webhooks/") && path.len() > "/webhooks/".len() => {
            let name = path["/webhooks/".len()..].trim_end_matches('/');
            e.custom_webhook(&crate::donations::urlencoding_decode(name), body);
            (200, Some(json!({"status": "ok"})))
        }
        ("POST", path) if path.starts_with("/api/") || path.starts_with("/") => {
            let p = path.trim_start_matches("/api/").trim_start_matches("/");
            match p {
                "kofi" | "streamelements" | "streamlabs" | "patreon" | "fourthwall" | "throne" | "tipeeestream"
                | "treatstream" | "donordrive" | "pallygg" | "pally" | "shopify" | "streamloots" => {
                    match crate::donations::handle_webhook(e, p, body, signature) {
                        Ok(()) => (200, Some(json!({"status": "ok"}))),
                        Err(err) => (400, Some(json!({"status": "error", "error": err}))),
                    }
                }
                _ => invalid(),
            }
        }
        _ => invalid(),
    }
}

async fn http_one(e: &Rc<Engine>, mut tcp: TcpStream) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    // The request must arrive in 10 seconds; an overlay's live feed then stays open as long as it wants.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let late = || std::io::Error::from(std::io::ErrorKind::TimedOut);
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        let n = tokio::time::timeout_at(deadline, tcp.read(&mut chunk))
            .await
            .map_err(|_| late())??;
        if n == 0 || buf.len() > 64 << 10 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (method, target) = (first.next().unwrap_or(""), first.next().unwrap_or("/"));
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers: Vec<(&str, &str)> = lines.filter_map(|l| l.split_once(':')).collect();
    let h = |k: &str| {
        headers
            .iter()
            .find(|(n, _)| n.trim().eq_ignore_ascii_case(k))
            .map(|(_, v)| v.trim())
    };
    let allowed = e.config().websockets.allowed_origins.clone();
    // Webhooks (donations, Kick) come from servers, which send no Origin; browsers are checked.
    if !origin_ok(h("origin"), h("sec-fetch-site"), &allowed) {
        crate::warn(format!(
            "HTTP server: refused a browser page from {}",
            h("origin").unwrap_or("another website")
        ));
        tcp.write_all(b"HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return tcp.shutdown().await;
    }
    if method == "GET" && (path == "/overlays" || path.starts_with("/overlays/")) {
        if path == "/overlays/events" {
            return crate::overlays::events(e, tcp, query).await;
        }
        let (status, kind, body) = crate::overlays::file(&e.config(), path);
        return crate::overlays::respond(&mut tcp, status, kind, &body).await;
    }
    let len = h("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0)
        .min(1 << 20);
    let mut body = buf.split_off(head_end + 4);
    while body.len() < len {
        let n = tokio::time::timeout_at(deadline, tcp.read(&mut chunk))
            .await
            .map_err(|_| late())??;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let (status, json) = http_answer(e, method, path, &body, h("x-patreon-signature"));
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        418 => "I'm a teapot",
        _ => "Internal Server Error",
    };
    let body = json.map(|v| crate::model::to_json(&v, true)).unwrap_or_default();
    let mut out = format!(
        "HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    if !body.is_empty() {
        out += "Content-Type: application/json\r\n";
    }
    out += "\r\n";
    tcp.write_all(out.as_bytes()).await?;
    tcp.write_all(&body).await?;
    tcp.shutdown().await
}

async fn udp_server(e: Rc<Engine>, s: UdpSettings) {
    // All interfaces, like Streamer.bot (devices on the network send these).
    let sock = match UdpSocket::bind(("0.0.0.0", s.port)).await {
        Ok(x) => x,
        Err(err) => return crate::error(format!("UDP server: can't listen on port {} ({err})", s.port)),
    };
    crate::say(format!("UDP server on port {}", s.port));
    let mut buf = vec![0u8; 65536];
    loop {
        let Ok((n, _)) = sock.recv_from(&mut buf).await else {
            continue;
        };
        let Ok(req) = serde_json::from_slice::<Value>(&buf[..n]) else {
            continue;
        };
        if req.get("request").and_then(Value::as_str) != Some("DoAction") {
            continue;
        }
        let a = req.get("action").cloned().unwrap_or_default();
        let args = req.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
        if find_action(&e, &a).and_then(|x| e.queue(&x, args)).is_none() {
            crate::error(format!("UDP: no action {a}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_group_requests_preserve_data_on_rename_and_export() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            let ask = |req: Value| answer(&e, &req, true).unwrap();
            assert_eq!(
                ask(json!({"request":"StreamerzManageActionGroup", "group":"one", "operation":"create"}))["status"],
                "ok"
            );
            assert_eq!(
                ask(json!({"request":"StreamerzSetGroupVariables", "group":"one", "values":{"x":1}}))["status"],
                "ok"
            );
            assert_eq!(
                ask(json!({"request":"StreamerzManageActionGroup", "group":"one", "operation":"rename", "name":"two"}))
                    ["status"],
                "ok"
            );
            assert_eq!(
                ask(json!({"request":"StreamerzGroupVariables", "group":"two"}))["values"]["x"],
                1
            );
            assert_eq!(
                ask(json!({"request":"StreamerzActionGroups"}))["groups"],
                json!(["two"])
            );
            assert_eq!(
                ask(json!({"request":"StreamerzManageActionGroup", "group":"two", "operation":"delete"}))["status"],
                "ok"
            );
            assert_eq!(ask(json!({"request":"StreamerzActionGroups"}))["groups"], json!([]));
        });
    }

    #[test]
    fn countdown_request_obeys_authentication() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            e.change(|c| {
                c.websockets.enable_auth = true;
                crate::templates::clock::create(c, "test", 5_000, 10_000, crate::templates::clock::now_ms());
            });
            let req = json!({"id": "1", "request": "StreamerzCountdown", "clockId": "test"});
            assert_eq!(answer(&e, &req, false).unwrap()["status"], "error");
            let response = answer(&e, &req, true).unwrap();
            assert_eq!(response["status"], "ok");
            assert_eq!(response["clockId"], "test");
            assert_eq!(response["snapshot"]["maxMs"], 10_000);
        });
    }

    #[test]
    fn spotify_requests_require_an_enabled_login() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let cfg = Arc::new(tokio::sync::watch::channel(crate::Config::default()).0);
            let e = Engine::new(cfg, |_| {});
            let request = json!({"request": "StreamerzSpotifyQueue", "query": "song"});
            assert_eq!(spotify_request(&e, &request).await.unwrap_err(), "Spotify is disabled");
            e.change(|c| c.spotify.enabled = true);
            assert_eq!(
                spotify_request(&e, &request).await.unwrap_err(),
                "Spotify login is required"
            );
        });
    }

    #[test]
    fn memory_policy_is_inspectable_and_changes_require_restart() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let cfg = Arc::new(tokio::sync::watch::channel(crate::Config::default()).0);
            let e = Engine::new(cfg, |_| {});
            let result = answer(&e, &json!({"request":"StreamerzMemory","enabled":true}), true).unwrap();
            assert_eq!(result["restartRequired"], true);
            assert_eq!(result["active"], false);
            assert_eq!(result["enabled"], true);
        });
    }

    /// The TikTok extension's path: only its origin, only with this install's token, and only once
    /// sending is set up. Once in, Streamerz opens its tab and learns the browser's account.
    #[test]
    fn tiktok_extension_pairs_with_its_origin_and_token() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let e = crate::engine::tests::engine(json!({"actions": []}));
        crate::engine::tests::run(async move {
            e.dev_mode();
            e.change(|c| {
                c.websockets.auto_start = true;
                c.websockets.address = "127.0.0.1".into();
                c.websockets.port = port;
                c.websockets.endpoint = "/".into();
                c.tiktok.bridge_token = "right".into();
            });
            tokio::task::spawn_local(super::run(e.clone()));
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let open = |origin: &str| {
                let mut req = format!("ws://127.0.0.1:{port}{}", crate::tiktok_bridge::PATH)
                    .into_client_request()
                    .unwrap();
                req.headers_mut().insert("origin", origin.parse().unwrap());
                crate::http::ws_connect(req)
            };
            let ext = format!("chrome-extension://{}", crate::tiktok_bridge::EXTENSION_ID);
            // Any other page is refused.
            assert!(open("https://www.tiktok.com").await.is_err());
            // The extension with a wrong token: closed, never connected.
            let (mut ws, _) = open(&ext).await.unwrap();
            ws.send(Message::Text(
                json!({"op": "hello", "token": "wrong", "protocol": 2})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!e.tiktok_bridge.is_connected());
            // The right token: connected; it asks for the page, then who is logged in.
            let (mut ws, _) = open(&ext).await.unwrap();
            ws.send(Message::Text(
                json!({"op": "hello", "token": "right", "protocol": 2})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            let page = loop {
                if let Some(Ok(Message::Text(t))) = ws.next().await {
                    break serde_json::from_str::<Value>(&t).unwrap();
                }
            };
            assert_eq!(page["op"], "page");
            assert!(e.tiktok_bridge.is_connected());
            let reply = json!({"id": page["id"], "params": {"device_id": "42", "aid": "1988", "max_time": "0"}});
            ws.send(Message::Text(reply.to_string().into())).await.unwrap();
            let who = loop {
                if let Some(Ok(Message::Text(t))) = ws.next().await {
                    break serde_json::from_str::<Value>(&t).unwrap();
                }
            };
            assert_eq!(who["op"], "fetch");
            assert!(who["url"].as_str().unwrap().contains("/passport/web/account/info/"));
            let body = json!({"data": {"username": "tester", "user_id_str": "42", "avatar_url": ""}}).to_string();
            let reply = json!({"id": who["id"], "status": 200, "body": body});
            ws.send(Message::Text(reply.to_string().into())).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            // Passport's missing avatar is filled from the existing signed search endpoint.
            let avatar_request = match tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
            {
                Message::Text(t) => serde_json::from_str::<Value>(&t).unwrap(),
                _ => panic!("avatar search"),
            };
            assert_eq!(avatar_request["op"], "search");
            assert_eq!(avatar_request["query"], "tester");
            let reply = json!({"id": avatar_request["id"], "status": 200, "body": json!({"status_code": 0,
                "user_list": [{"user_info": {"unique_id": "tester", "avatar_thumb": {"url_list": ["https://example.test/avatar.png"]}}}]}).to_string()});
            ws.send(Message::Text(reply.to_string().into())).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            assert_eq!(e.tiktok_bridge.user(), "tester");
            assert_eq!(e.tiktok_bridge.avatar(), "https://example.test/avatar.png");
            assert!(e.tiktok_bridge.ready());
            let search_engine = e.clone();
            let search = tokio::task::spawn_local(async move {
                search_request(&search_engine, &json!({"platform": "tiktok", "query": "tester"})).await
            });
            let request = match ws.next().await.unwrap().unwrap() {
                Message::Text(t) => serde_json::from_str::<Value>(&t).unwrap(),
                _ => panic!("search request"),
            };
            assert_eq!(request["op"], "search");
            let reply = json!({"id": request["id"], "status": 200, "body": json!({"status_code": 0,
                "user_list": [{"user_info": {"unique_id": "tester", "nickname": "Tester"}}]}).to_string()});
            ws.send(Message::Text(reply.to_string().into())).await.unwrap();
            assert_eq!(search.await.unwrap().unwrap()["channels"][0]["handle"], "tester");
            ws.send(Message::Text(json!({"op": "session_changed"}).to_string().into()))
                .await
                .unwrap();
            let reload = match tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
            {
                Message::Text(t) => serde_json::from_str::<Value>(&t).unwrap(),
                _ => panic!("reload request"),
            };
            assert_eq!(reload["op"], "page");
            assert!(e.tiktok_bridge.user().is_empty());
            assert!(e.tiktok_bridge.avatar().is_empty());
            assert_eq!(crate::roles::get("tiktok").0, crate::roles::Role::Anonymous);
            assert!(!e.tiktok_bridge.ready());
            ws.send(Message::Text(
                json!({"id": reload["id"], "error": "Log in to TikTok"})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            assert_eq!(e.tiktok_bridge.error(), "Log in to TikTok");
            assert!(!e.tiktok_bridge.ready());
            // It goes away with its socket.
            ws.close(None).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!e.tiktok_bridge.is_connected());
            assert!(!e.tiktok_bridge.ready());
            assert!(search_request(&e, &json!({"platform": "tiktok", "query": "tester"}))
                .await
                .unwrap_err()
                .contains("isn't connected"));
            // Not set up: the path isn't there at all.
            e.change(|c| c.tiktok.bridge_token.clear());
            assert!(open(&ext).await.is_err());
        });
    }

    #[test]
    fn origins() {
        let ok = |o: Option<&str>, f: Option<&str>| origin_ok(o, f, "https://Overlays.example.com/\r\n");
        assert!(ok(None, None)); // apps
        assert!(ok(None, Some("none")));
        assert!(!ok(None, Some("cross-site"))); // <img src=...> from a website
        for o in [
            "null",
            "file://",
            "http://localhost:3000",
            "http://127.0.0.1",
            "http://[::1]:8080",
        ] {
            assert!(ok(Some(o), None), "{o}");
        }
        assert!(ok(Some("https://overlays.example.com"), Some("cross-site")));
        for o in [
            "https://evil.com",
            "http://localhost.evil.com",
            "http://127.0.0.1.evil.com",
            "https://overlays.example.com.evil.com",
        ] {
            assert!(!ok(Some(o), None), "{o}");
        }
    }

    #[test]
    fn auth() {
        // obs-websocket's documented example (same scheme).
        assert_eq!(
            auth_response(
                "supersecretpassword",
                "lM1GncleQOaCu9lT1yeUZhFYnqhsLLP1G5lAGo3ixaI=",
                "+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY="
            ),
            "1Ct943GAT+6YQUUX47Ia/ncufilbe6+oD6lY+5kaCu4="
        );
    }

    #[test]
    fn client_and_custom_server() {
        use crate::engine::tests::{action, engine, log};
        let in_engine = crate::engine::tests::run;
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let watch = |id: &str, kind: u32, key: &str, owner: &str, msg: &str| {
            let mut a = action(id, id, json!([log(msg)]));
            a["triggers"] = json!([{key: owner, "id": id, "type": kind, "enabled": true}]);
            a
        };
        let e = engine(json!({"actions": [
            watch("1", 803, "clientId", "c", "T client open %triggerCategory%"),
            watch("2", 805, "clientId", "c", "T client got %message%"),
            watch("3", 2001, "serverId", "s", "T server open %ip%"),
            watch("4", 2003, "serverId", "s", "T server got %data%"),
            watch("5", 2002, "serverId", "s", "T server closed"),
            watch("6", 804, "clientId", "c", "T client closed"),
        ]}));
        let out = in_engine(async move {
            e.change(|c| {
                c.websockets.auto_start = false;
                let o = |v: Value| v.as_object().unwrap().clone();
                c.websockets.servers = vec![o(json!({"id": "s", "name": "S", "address": "127.0.0.1", "port": port,
                "endpoint": "/x", "autoStart": true}))];
            });
            tokio::task::spawn_local(super::run(e.clone()));
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            // An outside client says hello to the custom server.
            let (mut ws, _) = crate::http::ws_connect(&format!("ws://127.0.0.1:{port}/x"))
                .await
                .unwrap();
            ws.send(Message::Text("hi".into())).await.unwrap();
            ws.close(None).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            // Streamerz's own client, talking to a one-shot server.
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let p2 = l.local_addr().unwrap().port();
            tokio::task::spawn_local(async move {
                let (tcp, _) = l.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
                ws.send(Message::Text("yo".into())).await.unwrap();
                ws.close(None).await.unwrap();
            });
            e.change(|c| {
                c.websockets.connections = vec![
                    json!({"id": "c", "name": "C", "endpoint": format!("ws://127.0.0.1:{p2}/"),
                    "autoConnect": true, "reconnectOnDisconnect": false})
                    .as_object()
                    .unwrap()
                    .clone(),
                ];
            });
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        });
        for w in [
            "T server open 127.0.0.1",
            "T server got hi",
            "T server closed",
            "T client open Core/Websocket/Client",
            "T client got yo",
            "T client closed",
        ] {
            assert!(out.iter().any(|x| x == w), "{w} missing: {out:?}");
        }
    }

    /// A WebSocket client connected to an instance's server makes it a remote instance: Remote Instance
    /// Connected, then a Send Remote Trigger and Signal reach it (here its own server: a loopback).
    #[test]
    fn remote_triggers_and_signals_go_over_the_websocket() {
        use crate::engine::tests::{action, engine, log};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let on = |id: &str, kind: u32, extra: Value, msg: &str| {
            let mut a = action(id, id, json!([log(msg)]));
            let mut t = json!({"id": id, "type": kind, "enabled": true});
            t.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            a["triggers"] = json!([t]);
            a
        };
        let send = action(
            "send",
            "send",
            json!([
            {"type": 20001, "instanceId": "", "eventName": "boom", "customArgs": {"color": "blue"}},
            {"type": 20002, "instanceId": "Any", "signalName": "go", "customArgs": {"n": "5"}}]),
        );
        let wait = action(
            "wait",
            "wait",
            json!([
            {"type": 20003, "instanceId": "", "signalName": "go", "timeout": 2000},
            log("T signal n=%n% timeout=%signalTimeout%")]),
        );
        let e = engine(json!({"actions": [
            on("up", 29001, json!({}), "T remote up %instanceName%"),
            on("boom", 29003, json!({"eventName": "boom"}), "T remote boom %color%"),
            on("sig", 29004, json!({"signalName": "go"}), "T remote signal %signalName%"),
            on("down", 29002, json!({}), "T remote down %instanceName%"),
            send, wait]}));
        let out = crate::engine::tests::run(async move {
            e.change(|c| {
                c.instance_name = "Loop".into();
                c.websockets.auto_start = true;
                c.websockets.address = "127.0.0.1".into();
                c.websockets.port = port;
                c.websockets.endpoint = "/".into();
            });
            tokio::task::spawn_local(super::run(e.clone()));
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            e.change(|c| {
                c.websockets.connections = vec![
                    json!({"id": "c", "name": "C", "endpoint": format!("ws://127.0.0.1:{port}/"),
                    "autoConnect": true, "reconnectOnDisconnect": false})
                    .as_object()
                    .unwrap()
                    .clone(),
                ];
            });
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            e.queue("wait", Obj::new());
            e.queue("send", Obj::new());
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            // Removing the client (its task is aborted) leaves the remote instance.
            e.change(|c| c.websockets.connections.clear());
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        });
        for w in [
            "T remote up Loop",
            "T remote boom blue",
            "T remote signal go",
            "T signal n=5 timeout=False",
            "T remote down Loop",
        ] {
            assert!(out.iter().any(|x| x == w), "{w} missing: {out:?}");
        }
    }

    /// The Stream Deck plugin's side, as it talked to Streamer.bot 1.0.7 (dev/ notes): no Hello, GetActions,
    /// a button's DoAction, then the sub-actions' pushes; Connected and Disconnected around it.
    #[test]
    fn the_stream_deck_plugin_gets_its_pushes() {
        use crate::engine::tests::{action, engine, log};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let on = |id: &str, kind: u32, msg: &str| {
            let mut a = action(id, id, json!([log(msg)]));
            a["triggers"] = json!([{"id": id, "type": kind, "enabled": true}]);
            a
        };
        let press = action(
            "press",
            "press",
            json!([
            log("T pressed %sdButtonId% %eventSource% %__source%"),
            {"type": 11001, "buttonId": "%sdButtonId%", "title": "Hello", "state": 0},
            {"type": 11002, "buttonId": "b1", "image": "", "color": "#FF0000", "state": 1},
            {"type": 11004, "buttonId": "b1", "state": 1},
            {"type": 11005, "buttonId": "b1"},
            {"type": 11006, "buttonId": "b1"},
            {"type": 11007, "buttonId": "b1"},
            {"type": 11008, "buttonId": "b1", "value": "+=5"}]),
        );
        let e = engine(json!({"actions": [on("up", 17002, "T deck up"), on("down", 17003, "T deck down"), press]}));
        let got = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Value>::new()));
        let got2 = got.clone();
        let out = crate::engine::tests::run(async move {
            e.change(|c| {
                c.websockets.auto_start = false;
                c.stream_deck = crate::streamdeck::Settings {
                    auto_start: true,
                    address: "127.0.0.1".into(),
                    port,
                };
            });
            tokio::task::spawn_local(super::run(e.clone()));
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let (mut ws, _) = crate::http::ws_connect(&format!("ws://127.0.0.1:{port}/"))
                .await
                .unwrap();
            ws.send(Message::Text(r#"{"request":"GetActions","id":"ga"}"#.into()))
                .await
                .unwrap();
            ws.send(Message::Text(
                r#"{"request":"DoAction","id":"da","action":{"name":"press"},"args":{"sdButtonId":"b9"}}"#.into(),
            ))
            .await
            .unwrap();
            let until = tokio::time::Instant::now() + std::time::Duration::from_millis(600);
            while let Ok(Some(Ok(Message::Text(t)))) = tokio::time::timeout_at(until, ws.next()).await {
                got2.borrow_mut().push(serde_json::from_str(&t).unwrap());
            }
            ws.close(None).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        });
        let got = got.borrow();
        assert_eq!(got[0]["id"], "ga", "no Hello first: {got:?}");
        assert_eq!(got[0]["count"], 3);
        let pushes: Vec<&Value> = got.iter().filter(|m| m.get("request").is_some()).collect();
        let want = [
            json!({"request": "SetTitle", "data": {"buttonId": "b9", "text": "Hello", "state": 0}}),
            json!({"request": "SetImage", "data": {"buttonId": "b1", "color": "#FF0000", "state": 1}}),
            json!({"request": "SetState", "data": {"buttonId": "b1", "state": 1}}),
            json!({"request": "ShowAlert", "data": {"buttonId": "b1"}}),
            json!({"request": "ShowOk", "data": {"buttonId": "b1"}}),
            json!({"request": "ToggleState", "data": {"buttonId": "b1"}}),
            json!({"request": "SetValue", "data": {"buttonId": "b1", "value": "+=5"}}),
        ];
        assert_eq!(pushes, want.iter().collect::<Vec<_>>());
        for w in ["T deck up", "T pressed b9 streamdeck 17001", "T deck down"] {
            assert!(out.iter().any(|x| x == w), "{w} missing: {out:?}");
        }
    }

    #[test]
    fn overlay_requests_list_pages_and_set_topics() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            e.change(|c| crate::templates::overlays::viewers().add_to(c).unwrap());
            let ask = |req: Value| answer(&e, &req, true).unwrap();
            let list = ask(json!({"request": "StreamerzOverlays", "id": "1"}));
            assert_eq!(list["overlays"][0]["group"], "Viewer overlay");
            assert_eq!(
                list["overlays"][0]["url"],
                format!(
                    "{}/overlays/Viewer%20overlay/viewers.html",
                    list["base"].as_str().unwrap()
                )
            );
            let publish = json!({"request": "StreamerzOverlayPublish", "topic": " song ", "value": {"title": "A"}});
            assert_eq!(ask(publish.clone())["status"], "ok");
            assert_eq!(e.overlay_value("song"), Some(json!({"title": "A"})));
            assert!(ask(json!({"request": "StreamerzOverlays"}))["topics"]
                .as_array()
                .unwrap()
                .contains(&json!("song")));
            assert_eq!(
                ask(json!({"request": "StreamerzOverlayPublish", "topic": " "}))["status"],
                "error"
            );
            // With authentication on, a client that hasn't logged in can't set topics.
            e.change(|c| c.websockets.enable_auth = true);
            assert_eq!(answer(&e, &publish, false).unwrap()["status"], "error");
        });
    }

    #[test]
    fn overlays_are_served_with_a_live_feed() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let publish = json!([{"type": 99880, "topic": "custom", "value": "{\"n\": 5}"}]);
            let e =
                crate::engine::tests::engine(json!({"actions": [crate::engine::tests::action("a1", "pub", publish)]}));
            e.change(|c| crate::templates::overlays::ad().add_to(c).unwrap());
            // The real server, on a free port.
            let addr = TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
            let settings = HttpSettings {
                auto_start: true,
                address: "127.0.0.1".into(),
                port: addr.port(),
            };
            let server = tokio::task::spawn_local(http_server(e.clone(), settings));
            let get = |path: &'static str| async move {
                let mut t = loop {
                    match TcpStream::connect(addr).await {
                        Ok(t) => break t,
                        Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
                    }
                };
                t.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
                    .await
                    .unwrap();
                t
            };
            let mut page = get("/overlays/Ad%20overlay/ad.html").await;
            let mut text = String::new();
            page.read_to_string(&mut text).await.unwrap();
            assert!(
                text.starts_with("HTTP/1.1 200") && text.contains("streamerz.on(\"ad\""),
                "{text}"
            );
            let mut none = get("/overlays/Nope/x.html").await;
            let mut text = String::new();
            none.read_to_string(&mut text).await.unwrap();
            assert!(text.starts_with("HTTP/1.1 404"));
            // The feed starts with what a topic has, then follows changes.
            e.overlay_publish("ad", json!({"running": false}));
            let mut feed = get("/overlays/events?topics=ad,viewers,custom&events=Raw.ActionCompleted").await;
            let mut seen = String::new();
            let read = async |feed: &mut TcpStream, seen: &mut String, want: &str| {
                let mut chunk = [0u8; 2048];
                while !seen.contains(want) {
                    let n = tokio::time::timeout(std::time::Duration::from_secs(5), feed.read(&mut chunk))
                        .await
                        .unwrap_or_else(|_| panic!("the feed went quiet waiting for {want}: {seen}"))
                        .unwrap();
                    assert!(n > 0, "the feed closed: {seen}");
                    *seen += &String::from_utf8_lossy(&chunk[..n]);
                }
            };
            read(&mut feed, &mut seen, "\"running\":false").await;
            assert!(seen.contains("text/event-stream") && seen.contains("\"total\":0"));
            // The Overlay sub-action: a JSON value arrives as JSON.
            e.queue("a1", crate::model::Obj::new()).unwrap();
            read(&mut feed, &mut seen, "\"n\":5").await;
            e.overlay_publish("ad", json!({"running": true}));
            read(&mut feed, &mut seen, "\"running\":true").await;
            e.emit("Raw", "ActionCompleted", || json!({"name": "overlay event test"}));
            read(&mut feed, &mut seen, "overlay event test").await;
            assert!(e.listening("Overlay", "viewers"));
            drop(feed);
            for _ in 0..50 {
                if !e.listening("Overlay", "viewers") {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            assert!(!e.listening("Overlay", "viewers"), "a closed page stops listening");
            // Turning the server off ends the feeds that are open.
            let mut feed = get("/overlays/events?topics=viewers").await;
            let mut seen = String::new();
            read(&mut feed, &mut seen, "\"total\"").await;
            assert!(e.listening("Overlay", "viewers"));
            server.abort();
            let mut rest = Vec::new();
            let closed = tokio::time::timeout(std::time::Duration::from_secs(5), feed.read_to_end(&mut rest)).await;
            assert!(closed.is_ok(), "the feed is still open after the server stopped");
            assert!(
                !e.listening("Overlay", "viewers"),
                "a stopped server leaves no listener"
            );
        });
    }

    #[test]
    fn http_answers() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let e =
                crate::engine::tests::engine(json!({"actions": [crate::engine::tests::action("a1", "hi", json!([]))]}));
            assert_eq!(
                http_answer(&e, "POST", "/DoAction", br#"{"action":{"name":"hi"}}"#, None).0,
                204
            );
            let (st, v) = http_answer(&e, "POST", "/DoAction", br#"{"action":{"name":"nope"}}"#, None);
            assert_eq!((st, v.unwrap()["error"].clone()), (400, json!("action not found")));
            let (st, v) = http_answer(&e, "GET", "/GetActions", b"", None);
            assert_eq!((st, v.unwrap()["count"].clone()), (200, json!(1)));
            let (st, v) = http_answer(&e, "OPTIONS", "/DoAction", b"", None);
            assert_eq!(
                (st, v.unwrap()["error"].clone()),
                (418, json!("'OPTIONS /DoAction' is an invalid request"))
            );
            assert_eq!(http_answer(&e, "POST", "/DoAction", b"xx", None).0, 400);
            // A custom webhook by name (spaces encoded); /webhooks/ alone is no webhook.
            assert_eq!(
                http_answer(&e, "POST", "/webhooks/my%20hook", br#"{"a":1}"#, None).0,
                200
            );
            assert_eq!(http_answer(&e, "POST", "/webhooks/", b"", None).0, 418);
            assert_eq!(
                http_answer(&e, "POST", "/api/kofi", br#"{"type":"Donation","amount":"5.00"}"#, None).0,
                200
            );
            assert_eq!(
                http_answer(&e, "POST", "/streamelements", br#"{"type":"tip","amount":10.0}"#, None).0,
                200
            );
            let req = json!({"request": "DoAction", "id": "1", "action": {"name": "hi"}, "args": {"k": 1}});
            let r = answer(&e, &req, false).unwrap();
            assert_eq!(r["args"]["queueName"], "Default");
            assert_eq!(r["args"]["k"], 1);
            let keys: Vec<&String> = r.as_object().unwrap().keys().collect();
            assert_eq!(keys, ["id", "args", "status"]);
            assert!(answer(&e, &json!({"request": "Bogus", "id": "x"}), false).is_none());
        });
    }

    #[test]
    fn group_settings_templates_and_export_work_over_requests() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            let ask = |req: Value| answer(&e, &req, true).unwrap();
            assert_eq!(
                ask(json!({"request": "StreamerzTemplateAdd", "name": "nope"}))["status"],
                "error"
            );
            assert_eq!(
                ask(json!({"request": "StreamerzTemplateAdd", "name": "Subathon timer"}))["status"],
                "ok"
            );
            let meta = ask(json!({"request": "StreamerzGroupMeta", "group": "Subathon"}));
            assert_eq!(meta["meta"]["variables"].as_array().unwrap().len(), 5);
            assert_eq!(meta["meta"]["files"][0]["name"], "subathon.html");
            let mut changed = meta["meta"].clone();
            changed["variables"][1]["default"] = json!(7_200_000);
            changed["variables"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name": "bonus", "label": "Bonus", "kind": "integer", "default": 3}));
            assert_eq!(
                ask(json!({"request": "StreamerzSetGroupMeta", "group": "Subathon", "meta": changed}))["status"],
                "ok"
            );
            let values = ask(json!({"request": "StreamerzGroupVariables", "group": "Subathon"}))["values"].clone();
            assert_eq!(
                (values["bonus"].clone(), values["currency"].clone()),
                (json!(3), json!("USD"))
            );
            let wrong =
                ask(json!({"request": "StreamerzSetGroupVariables", "group": "Subathon", "values": {"start": "soon"}}));
            assert_eq!(wrong["status"], "error");
            let mut bad = changed.clone();
            bad["variables"][0]["default"] = json!("x");
            assert_eq!(
                ask(json!({"request": "StreamerzSetGroupMeta", "group": "Subathon", "meta": bad}))["status"],
                "error"
            );
            let mut missing = changed.clone();
            missing["commands"] = json!(["no-such-command"]);
            assert_eq!(
                ask(json!({"request": "StreamerzSetGroupMeta", "group": "Subathon", "meta": missing}))["status"],
                "error"
            );
            let exported = ask(json!({"request": "StreamerzGroupExport", "group": "Subathon"}));
            assert_eq!(
                exported["export"]["streamerz"]["groups"]["Subathon"]["variables"][1]["default"],
                7_200_000
            );
            let dir = std::env::temp_dir().join(format!("streamerz-group-files-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let saved = ask(json!({"request": "StreamerzSaveGroupFiles", "group": "Subathon", "folder": dir}));
            assert_eq!(saved["status"], "ok");
            assert!(dir.join("subathon.html").exists());
            assert_eq!(
                ask(json!({"request": "StreamerzSaveGroupFiles", "group": "Subathon", "folder": dir}))["status"],
                "error"
            );
            std::fs::remove_dir_all(dir).unwrap();
            assert_eq!(
                ask(json!({"request": "StreamerzGroupMeta", "group": "missing"}))["status"],
                "error"
            );
        });
    }
}
