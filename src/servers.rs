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
    loop {
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
        if rx.changed().await.is_err() {
            return;
        }
    }
}

fn flag(o: &Obj, k: &str) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(false)
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
                e.remote_connected(field(&c, "id"), field(&c, "name"));
                let (_sink, mut stream) = ws.split();
                while let Some(Ok(m)) = stream.next().await {
                    match frame_text(m) {
                        Some(Ok(text)) => {
                            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                                if let Some(event) = val.get("event") {
                                    if event.get("source").and_then(Value::as_str) == Some("StreamerbotRemote") {
                                        let ev_type = event.get("type").and_then(Value::as_str).unwrap_or("");
                                        let data =
                                            val.get("data").and_then(Value::as_object).cloned().unwrap_or_default();
                                        let inst = data.get("instanceId").and_then(Value::as_str).unwrap_or("");
                                        match ev_type {
                                            "InstanceTrigger" => {
                                                let ev_name =
                                                    data.get("eventName").and_then(Value::as_str).unwrap_or("");
                                                let args = data
                                                    .get("arguments")
                                                    .and_then(Value::as_object)
                                                    .cloned()
                                                    .unwrap_or_default();
                                                e.fire_remote_trigger(inst, ev_name, args);
                                            }
                                            "InstanceSignal" => {
                                                let sig_name =
                                                    data.get("signalName").and_then(Value::as_str).unwrap_or("");
                                                let args = data
                                                    .get("arguments")
                                                    .and_then(Value::as_object)
                                                    .cloned()
                                                    .unwrap_or_default();
                                                e.deliver_remote_signal(inst, sig_name, args);
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
                e.remote_disconnected(field(&c, "id"), field(&c, "name"));
                ws_event(&e, true, "Close", &c, Obj::new());
                if !flag(&c, "reconnectOnDisconnect") {
                    return;
                }
            }
            Err(err) => {
                crate::say(format!("WebSocket client {}: {url}: {err}", field(&c, "name")));
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
        Err(err) => return crate::say(format!("custom server {name}: can't listen on {addr}:{port} ({err})")),
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
            let Ok(ws) = accept(tcp, field(&s, "endpoint").to_string(), allowed).await else {
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
/// (403). The handshake callback's error type is tungstenite's (a whole HTTP response).
#[allow(clippy::result_large_err)]
async fn accept(
    tcp: TcpStream,
    endpoint: String,
    allowed: String,
) -> Result<tokio_tungstenite::WebSocketStream<TcpStream>, String> {
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
    use tokio_tungstenite::tungstenite::http::StatusCode;
    let check = move |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let h = |k: &str| req.headers().get(k).and_then(|v| v.to_str().ok());
        let status = if !endpoint.is_empty() && req.uri().path() != endpoint {
            StatusCode::NOT_FOUND
        } else if !origin_ok(h("origin"), h("sec-fetch-site"), &allowed) {
            crate::say(format!(
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
    tokio_tungstenite::accept_hdr_async(tcp, check)
        .await
        .map_err(|e| e.to_string())
}

fn info(e: &Engine) -> Value {
    let c = e.config();
    json!({"instanceId": c.instance_id, "name": c.instance_name, "version": API_VERSION,
        "os": if cfg!(windows) { "windows" } else { "linux" }, "osVersion": "unknown", "mode": "ui",
        "darkMode": c.dark, "source": "websocketServer",
        "streamerz": option_env!("STREAMERZ_VERSION").unwrap_or("dev")})
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

fn credits() -> Value {
    json!({"events": {}, "hypeTrain": {"conductors": [], "contributors": []}, "users": {}, "groups": {}, "custom": {}})
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
fn answer(e: &Rc<Engine>, req: &Value, authed: bool) -> Option<Value> {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let get = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("");
    let persisted = req.get("persisted").and_then(Value::as_bool).unwrap_or(true);
    Some(match get("request") {
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
        // Credits are a later phase: empty, like a fresh Streamer.bot.
        "GetCredits" | "TestCredits" => ok(&id, credits()),
        "ClearCredits" => ok(&id, json!({})),
        "GetActiveViewers" => ok(&id, json!({"count": 0, "viewers": []})),
        // Code triggers are registered by C#, which streamerz doesn't run.
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
        // Dev mode only (STREAMERZ_DEV=1): fake chat in, and the chat that would have gone out.
        "InjectChat" if e.dev() => match e.inject_chat(get("platform"), get("user"), get("role"), get("text")) {
            Ok(ran) => ok(&id, json!({"ran": ran})),
            Err(m) => error(&id, &m),
        },
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
            ok(&id, json!({"lines": crate::log_tail(n)}))
        }
        // Dev mode only: what's connected, and how much is set up.
        "GetStatus" if e.dev() => {
            let c = e.config();
            ok(
                &id,
                json!({"info": info(e), "connected": {
                    "twitch": e.twitch_connected(), "youtube": e.youtube_connected(), "kick": e.kick_connected(),
                    "tiktok": e.tiktok_connected(), "obs": e.obs_connected(None), "streamlabs": e.slobs_connected()},
                    "actions": c.actions.actions.len(), "commands": c.commands.len(), "timers": c.timers.len()}),
            )
        }
        _ => return None,
    })
}

async fn websocket(e: Rc<Engine>, s: WsSettings) {
    let l = match TcpListener::bind((s.address.as_str(), s.port)).await {
        Ok(l) => l,
        Err(err) => {
            return crate::say(format!(
                "WebSocket server: can't listen on {}:{} ({err})",
                s.address, s.port
            ))
        }
    };
    crate::say(format!(
        "WebSocket server on ws://{}:{}{}",
        s.address, s.port, s.endpoint
    ));
    let mut next = 0u64;
    loop {
        let Ok((tcp, _)) = l.accept().await else { continue };
        next += 1;
        let (e, s, id) = (e.clone(), s.clone(), next);
        tokio::task::spawn_local(async move {
            let client_inst_id = format!("remote-{id}");
            let client_inst_name = format!("Client {id}");
            e.remote_connected(&client_inst_id, &client_inst_name);
            if let Err(err) = connection(&e, &s, tcp, id).await {
                crate::say(format!("WebSocket client: {err}"));
            }
            e.remote_disconnected(&client_inst_id, &client_inst_name);
            e.listen(id, &events::Subs::new(), &tokio::sync::mpsc::unbounded_channel().0);
        });
    }
}

async fn connection(e: &Rc<Engine>, s: &WsSettings, tcp: TcpStream, conn: u64) -> Result<(), String> {
    let ws = accept(tcp, s.endpoint.clone(), s.allowed_origins.clone()).await?;
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
        Err(err) => return crate::say(format!("HTTP server: can't listen on {}:{} ({err})", s.address, s.port)),
    };
    crate::say(format!("HTTP server on http://{}:{}/", s.address, s.port));
    loop {
        let Ok((tcp, _)) = l.accept().await else { continue };
        let e = e.clone();
        tokio::task::spawn_local(async move {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(10), http_one(&e, tcp)).await;
        });
    }
}

/// The answer to one HTTP request: status and JSON body (None: no body).
fn http_answer(e: &Rc<Engine>, method: &str, path: &str, body: &[u8]) -> (u16, Option<Value>) {
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
        ("GET", "/GetCredits" | "/TestCredits") => (200, Some(credits())),
        ("GET", "/ClearCredits") => (204, None),
        ("GET", "/ClearFirstWordsCache") => {
            e.clear_first_words();
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
        ("POST", path) if path.starts_with("/api/") || path.starts_with("/") => {
            let p = path.trim_start_matches("/api/").trim_start_matches("/");
            match p {
                "kofi" | "streamelements" | "streamlabs" | "patreon" | "fourthwall" | "throne" | "tipeeestream"
                | "treatstream" | "donordrive" | "pallygg" | "pally" | "shopify" | "streamloots" => {
                    match crate::donations::handle_webhook(e, p, body) {
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
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        let n = tcp.read(&mut chunk).await?;
        if n == 0 || buf.len() > 64 << 10 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (method, target) = (first.next().unwrap_or(""), first.next().unwrap_or("/"));
    let path = target.split('?').next().unwrap_or("/");
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
        crate::say(format!(
            "HTTP server: refused a browser page from {}",
            h("origin").unwrap_or("another website")
        ));
        tcp.write_all(b"HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return tcp.shutdown().await;
    }
    let len = h("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0)
        .min(1 << 20);
    let mut body = buf.split_off(head_end + 4);
    while body.len() < len {
        let n = tcp.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let (status, json) = http_answer(e, method, path, &body);
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
        Err(err) => return crate::say(format!("UDP server: can't listen on port {} ({err})", s.port)),
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
            crate::say(format!("UDP: no action {a}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let _g = crate::TEST_LOCK.lock().unwrap();
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
            // streamerz's own client, talking to a one-shot server.
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

    #[test]
    fn http_answers() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let e =
                crate::engine::tests::engine(json!({"actions": [crate::engine::tests::action("a1", "hi", json!([]))]}));
            assert_eq!(
                http_answer(&e, "POST", "/DoAction", br#"{"action":{"name":"hi"}}"#).0,
                204
            );
            let (st, v) = http_answer(&e, "POST", "/DoAction", br#"{"action":{"name":"nope"}}"#);
            assert_eq!((st, v.unwrap()["error"].clone()), (400, json!("action not found")));
            let (st, v) = http_answer(&e, "GET", "/GetActions", b"");
            assert_eq!((st, v.unwrap()["count"].clone()), (200, json!(1)));
            let (st, v) = http_answer(&e, "OPTIONS", "/DoAction", b"");
            assert_eq!(
                (st, v.unwrap()["error"].clone()),
                (418, json!("'OPTIONS /DoAction' is an invalid request"))
            );
            assert_eq!(http_answer(&e, "POST", "/DoAction", b"xx").0, 400);
            assert_eq!(
                http_answer(&e, "POST", "/api/kofi", br#"{"type":"Donation","amount":"5.00"}"#).0,
                200
            );
            assert_eq!(
                http_answer(&e, "POST", "/streamelements", br#"{"type":"tip","amount":10.0}"#).0,
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
}
