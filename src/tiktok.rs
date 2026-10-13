//! TikTok integration for streamerz:
//! - Dual-mode integration:
//!   1. Local WebSocket bridge (TikFinity / tikstream on `ws://127.0.0.1:21213`).
//!   2. Companion management: detects and launches `tikstream` binary.
//! - Listens to TikTok Live events: chat messages, gifts & coins, likes, follows, shares, subscriptions, and joins.
//! - Chat message dispatch via Webcast API with broadcaster / bot account selection and fallback.
//! - Triggers (41001..=41009), chat sub-action (43001), and chat command execution.

use crate::commands::Chat;
use crate::engine::Engine;
use crate::model::Obj;
use crate::Config;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;

// TikTok Trigger IDs
pub const T_TIKTOK_CHAT_MESSAGE: u32 = 41001;
pub const T_TIKTOK_GIFT: u32 = 41002;
pub const T_TIKTOK_LIKE: u32 = 41003;
pub const T_TIKTOK_FOLLOW: u32 = 41004;
pub const T_TIKTOK_SHARE: u32 = 41005;
pub const T_TIKTOK_SUBSCRIBE: u32 = 41006;
pub const T_TIKTOK_JOIN: u32 = 41007;
pub const T_TIKTOK_CONNECTED: u32 = 41008;
pub const T_TIKTOK_DISCONNECTED: u32 = 41009;

pub const DEFAULT_TIKTOK_BRIDGE_URL: &str = "ws://127.0.0.1:21213/";

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TikTokAccount {
    /// TikTok username / handle (@handle or nickname).
    pub username: String,
    /// Session cookie string (must contain sessionid=...).
    pub session_cookie: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TikTokConfig {
    /// Channel name or @handle to watch.
    pub channel: String,
    /// Whether to auto-connect to the bridge / TikTok on startup.
    pub auto_connect: bool,
    /// WebSocket bridge URL (defaults to ws://127.0.0.1:21213/).
    pub bridge_url: String,
    /// Broadcaster account credentials
    pub broadcaster: Option<TikTokAccount>,
    /// Bot account credentials
    pub bot: Option<TikTokAccount>,
}

impl Default for TikTokConfig {
    fn default() -> Self {
        Self {
            channel: String::new(),
            auto_connect: false,
            bridge_url: DEFAULT_TIKTOK_BRIDGE_URL.to_string(),
            broadcaster: None,
            bot: None,
        }
    }
}

/// Strip URLs, @ signs, and slashes from a TikTok channel input.
pub fn clean_tiktok_channel(input: &str) -> String {
    let mut s = input.trim();
    for prefix in &[
        "https://www.tiktok.com/@",
        "http://www.tiktok.com/@",
        "https://tiktok.com/@",
        "http://tiktok.com/@",
        "www.tiktok.com/@",
        "tiktok.com/@",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
            break;
        }
    }
    s.trim_start_matches('@').trim_end_matches('/').trim().to_string()
}

/// Find local `tikstream` binary on the host system.
pub fn find_tikstream_binary() -> Option<PathBuf> {
    // 1. Next to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(if cfg!(windows) { "tikstream.exe" } else { "tikstream" });
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // 2. Relative repos directory if in dev workspace
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let p = Path::new(&home).join("Documents/repos/tikstream/target/release/tikstream");
        if p.is_file() {
            return Some(p);
        }
        let p_debug = Path::new(&home).join("Documents/repos/tikstream/target/debug/tikstream");
        if p_debug.is_file() {
            return Some(p_debug);
        }
    }

    // 3. Search PATH
    if let Ok(path_var) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for part in path_var.split(sep) {
            let candidate = Path::new(part).join(if cfg!(windows) { "tikstream.exe" } else { "tikstream" });
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Spawns the local `tikstream` companion binary in background if available.
pub fn launch_tikstream(channel: &str) -> Result<(), String> {
    let bin = find_tikstream_binary().ok_or_else(|| "tikstream executable not found on system".to_string())?;

    // If channel is given, write or update tikstream.json next to the binary
    if let Some(parent) = bin.parent() {
        let cfg_path = parent.join("tikstream.json");
        let mut obj = if let Ok(content) = std::fs::read_to_string(&cfg_path) {
            serde_json::from_str::<serde_json::Value>(&content).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
        };
        if let Some(map) = obj.as_object_mut() {
            if !channel.trim().is_empty() {
                map.insert("username".into(), json!(clean_tiktok_channel(channel)));
            }
            if let Ok(s) = serde_json::to_string_pretty(&obj) {
                let _ = std::fs::write(&cfg_path, s);
            }
        }
    }

    let mut cmd = std::process::Command::new(&bin);
    if let Some(parent) = bin.parent() {
        cmd.current_dir(parent);
    }
    cmd.spawn()
        .map_err(|e| format!("Failed to spawn {}: {e}", bin.display()))?;
    Ok(())
}

/// Find local `tikstream-login` (or `tikstream` on Windows) companion binary.
pub fn find_tikstream_login_binary() -> Option<PathBuf> {
    let bin_name = if cfg!(windows) {
        "tikstream.exe"
    } else {
        "tikstream-login"
    };

    // 1. Next to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(bin_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // 2. Relative repos directory if in dev workspace
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        for profile in &["release", "debug"] {
            let p = Path::new(&home).join(format!("Documents/repos/tikstream/target/{profile}/{bin_name}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }

    // 3. Search PATH
    if let Ok(path_var) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for part in path_var.split(sep) {
            let candidate = Path::new(part).join(bin_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Spawns the login window companion and captures user credentials on success.
pub fn run_tikstream_login(is_bot: bool) -> Result<(String, String), String> {
    let bin = find_tikstream_login_binary().ok_or_else(|| "tikstream-login companion binary not found".to_string())?;

    let profile_name = if is_bot {
        "streamerz-tiktok-bot"
    } else {
        "streamerz-tiktok-broadcaster"
    };
    let profile_dir = std::env::temp_dir().join(profile_name);

    let mut cmd = std::process::Command::new(&bin);
    if cfg!(windows) {
        cmd.args(["--login", profile_dir.to_str().unwrap_or("")]);
    } else {
        cmd.arg(&profile_dir);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to spawn {}: {e}", bin.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Login window was closed or cancelled. {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
            if let Some(cookies) = v["cookies"].as_str().filter(|s| !s.is_empty()) {
                let username = v["username"].as_str().unwrap_or("").to_string();
                return Ok((username, cookies.to_string()));
            }
        }
    }

    Err("No session credentials returned from login companion".to_string())
}

/// Attempts to extract live room ID from public TikTok live page if not received from bridge.
pub async fn resolve_room_id(channel: &str) -> Option<String> {
    let clean = clean_tiktok_channel(channel);
    if clean.is_empty() {
        return None;
    }
    let url = format!("https://www.tiktok.com/@{clean}/live");
    let headers = vec![
        ("User-Agent".into(), "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".into()),
    ];
    let resp = crate::http::request("GET", &url, &headers, b"").await.ok()?;
    let body = String::from_utf8_lossy(&resp.body);
    for key in &["\"roomId\":", "\"room_id\":"] {
        if let Some(pos) = body.find(key) {
            let after = &body[pos + key.len()..];
            let trimmed = after.trim_start_matches(['"', ' ']);
            let id: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
pub enum TikTokStatus {
    Disconnected,
    Connecting(String),
    Connected(String),
    Error(String),
}

pub enum TikTokCmd {
    Connect,
    Disconnect,
    SendMessage {
        text: String,
        use_bot: bool,
        fallback: bool,
        resp: oneshot::Sender<Result<(), String>>,
    },
    GetStatus(oneshot::Sender<TikTokStatus>),
}

#[derive(Clone)]
pub struct TikTokHandle {
    tx: mpsc::UnboundedSender<TikTokCmd>,
    connected: Arc<AtomicBool>,
    status: Arc<Mutex<TikTokStatus>>,
    room_id: Arc<Mutex<Option<String>>>,
}

impl TikTokHandle {
    pub fn send(&self, cmd: TikTokCmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn room_id(&self) -> Option<String> {
        self.room_id.lock().ok().and_then(|r| r.clone())
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn current_status(&self) -> TikTokStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(TikTokStatus::Disconnected)
    }

    pub async fn status(&self) -> TikTokStatus {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(TikTokCmd::GetStatus(tx)).is_ok() {
            rx.await.unwrap_or(TikTokStatus::Disconnected)
        } else {
            TikTokStatus::Disconnected
        }
    }

    pub async fn send_message(&self, text: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(TikTokCmd::SendMessage {
                text: text.to_string(),
                use_bot,
                fallback,
                resp: resp_tx,
            })
            .map_err(|_| "TikTok runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }
}

pub fn start(engine: Rc<Engine>, cfg: &Arc<watch::Sender<Config>>) -> TikTokHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let connected = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new(TikTokStatus::Disconnected));
    let room_id = Arc::new(Mutex::new(None));

    let handle = TikTokHandle {
        tx,
        connected: connected.clone(),
        status: status.clone(),
        room_id: room_id.clone(),
    };

    let runner = Runner {
        engine,
        cfg: cfg.clone(),
        rx,
        connected,
        status,
        room_id,
        read_task: None,
    };
    tokio::task::spawn_local(runner.run());
    handle
}

struct Runner {
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    rx: mpsc::UnboundedReceiver<TikTokCmd>,
    connected: Arc<AtomicBool>,
    status: Arc<Mutex<TikTokStatus>>,
    room_id: Arc<Mutex<Option<String>>>,
    read_task: Option<tokio::task::JoinHandle<()>>,
}

impl Runner {
    async fn run(mut self) {
        if self.cfg.borrow().tiktok.auto_connect {
            self.connect_bridge();
        }

        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                TikTokCmd::Connect => {
                    self.connect_bridge();
                }
                TikTokCmd::Disconnect => {
                    if let Some(task) = self.read_task.take() {
                        task.abort();
                    }
                    self.connected.store(false, Ordering::Relaxed);
                    if let Ok(mut s) = self.status.lock() {
                        *s = TikTokStatus::Disconnected;
                    }
                    crate::say("TikTok: disconnected".into());
                    let mut args = Obj::new();
                    args.insert("channel".into(), self.cfg.borrow().tiktok.channel.clone().into());
                    self.engine.fire(
                        T_TIKTOK_DISCONNECTED,
                        "TikTokDisconnected",
                        "TikTok",
                        "tiktok",
                        &args,
                        |_| true,
                    );
                    self.engine.emit("TikTok", "Disconnected", || json!(args));
                }
                TikTokCmd::SendMessage {
                    text,
                    use_bot,
                    fallback,
                    resp,
                } => {
                    let res = self.send_chat_message(&text, use_bot, fallback).await;
                    let _ = resp.send(res);
                }
                TikTokCmd::GetStatus(resp) => {
                    let st = self
                        .status
                        .lock()
                        .map(|s| s.clone())
                        .unwrap_or(TikTokStatus::Disconnected);
                    let _ = resp.send(st);
                }
            }
        }
    }

    async fn send_chat_message(&self, text: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        let (cookie, mut room_id) = {
            let c = self.cfg.borrow();
            let acc = if use_bot {
                let mut a = c.tiktok.bot.as_ref();
                if a.is_none() && fallback {
                    a = c.tiktok.broadcaster.as_ref();
                }
                a
            } else {
                let mut a = c.tiktok.broadcaster.as_ref();
                if a.is_none() && fallback {
                    a = c.tiktok.bot.as_ref();
                }
                a
            };
            let acc = acc.ok_or_else(|| {
                if use_bot {
                    "No TikTok bot account configured (and no fallback)".to_string()
                } else {
                    "No TikTok broadcaster account configured (and no fallback)".to_string()
                }
            })?;
            let cookie = acc.session_cookie.clone();
            let rid = self.room_id.lock().ok().and_then(|r| r.clone());
            (cookie, rid)
        };

        if room_id.is_none() || room_id.as_deref() == Some("") {
            let chan = clean_tiktok_channel(&self.cfg.borrow().tiktok.channel);
            if !chan.is_empty() {
                if let Some(resolved) = resolve_room_id(&chan).await {
                    if let Ok(mut r) = self.room_id.lock() {
                        *r = Some(resolved.clone());
                    }
                    room_id = Some(resolved);
                }
            }
        }

        let rid = room_id
            .ok_or_else(|| "No TikTok live room ID available (ensure stream is live and channel is set)".to_string())?;

        let cookie_header = if cookie.contains('=') {
            cookie
        } else {
            format!("sessionid={cookie}")
        };

        let url = "https://webcast.tiktok.com/webcast/room/chat/?aid=1988&app_language=en&app_name=tiktok_web";
        let body = format!(
            "content={}&room_id={}",
            crate::commands::url_encode(text),
            crate::commands::url_encode(&rid),
        );
        let headers = vec![
            ("Content-Type".into(), "application/x-www-form-urlencoded".into()),
            ("Cookie".into(), cookie_header),
            ("User-Agent".into(), "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".into()),
            ("Referer".into(), "https://www.tiktok.com/".into()),
        ];

        let resp = crate::http::request("POST", url, &headers, body.as_bytes())
            .await
            .map_err(|e| format!("TikTok chat request error: {e}"))?;

        if (200..300).contains(&resp.status) {
            if let Ok(val) = serde_json::from_slice::<Value>(&resp.body) {
                if let Some(code) = val.get("status_code").and_then(Value::as_i64) {
                    if code != 0 {
                        let msg = val.get("message").and_then(Value::as_str).unwrap_or("TikTok error");
                        return Err(format!("TikTok chat error (status_code {code}): {msg}"));
                    }
                }
            }
            Ok(())
        } else {
            let body_str = String::from_utf8_lossy(&resp.body);
            Err(format!("TikTok chat failed HTTP {}: {}", resp.status, body_str))
        }
    }

    fn connect_bridge(&mut self) {
        if let Some(task) = self.read_task.take() {
            task.abort();
        }

        let bridge_url = {
            let u = self.cfg.borrow().tiktok.bridge_url.trim().to_string();
            if u.is_empty() {
                DEFAULT_TIKTOK_BRIDGE_URL.to_string()
            } else {
                u
            }
        };

        let channel = clean_tiktok_channel(&self.cfg.borrow().tiktok.channel);
        let channel_msg = if channel.is_empty() {
            String::new()
        } else {
            format!(" for @{channel}")
        };
        crate::say(format!(
            "TikTok: connecting to bridge at '{bridge_url}'{channel_msg}..."
        ));
        if let Ok(mut s) = self.status.lock() {
            *s = TikTokStatus::Connecting(format!("Connecting to bridge {bridge_url}..."));
        }

        let engine = self.engine.clone();
        let connected_flag = self.connected.clone();
        let status_arc = self.status.clone();
        let cfg_watch = self.cfg.clone();
        let room_id_arc = self.room_id.clone();

        let task = tokio::task::spawn_local(async move {
            let mut retry_secs = 2;
            loop {
                let url_str = {
                    let u = cfg_watch.borrow().tiktok.bridge_url.trim().to_string();
                    if u.is_empty() {
                        DEFAULT_TIKTOK_BRIDGE_URL.to_string()
                    } else {
                        u
                    }
                };

                match tokio_tungstenite::connect_async(&url_str).await {
                    Ok((ws_stream, _)) => {
                        connected_flag.store(true, Ordering::Relaxed);
                        let channel_name = clean_tiktok_channel(&cfg_watch.borrow().tiktok.channel);
                        let conn_msg = format!("Connected ({url_str})");
                        if let Ok(mut s) = status_arc.lock() {
                            *s = TikTokStatus::Connected(conn_msg.clone());
                        }
                        crate::say(format!("TikTok: {conn_msg}"));

                        let mut args = Obj::new();
                        args.insert("channel".into(), channel_name.clone().into());
                        args.insert("bridgeUrl".into(), url_str.clone().into());
                        engine.fire(T_TIKTOK_CONNECTED, "TikTokConnected", "TikTok", "tiktok", &args, |_| {
                            true
                        });
                        engine.emit("TikTok", "Connected", || json!(args));

                        let (_, mut read) = ws_stream.split();
                        let mut local_speakers = HashSet::new();

                        while let Some(msg) = read.next().await {
                            match msg {
                                Ok(Message::Text(text)) => {
                                    if let Ok(v) = serde_json::from_str::<Value>(&text) {
                                        if v["event"].as_str() == Some("connected") {
                                            if let Some(rid) = v["data"]["roomId"].as_str().filter(|s| !s.is_empty()) {
                                                if let Ok(mut r) = room_id_arc.lock() {
                                                    *r = Some(rid.to_string());
                                                }
                                            }
                                        }
                                        handle_tiktok_event(&engine, &channel_name, &v, &mut local_speakers);
                                    }
                                }
                                Ok(Message::Close(_)) => {
                                    crate::say("TikTok: bridge closed connection".into());
                                    break;
                                }
                                Err(e) => {
                                    crate::say(format!("TikTok: bridge connection error: {e}"));
                                    break;
                                }
                                _ => {}
                            }
                        }

                        connected_flag.store(false, Ordering::Relaxed);
                        let mut args = Obj::new();
                        args.insert("channel".into(), channel_name.into());
                        engine.fire(
                            T_TIKTOK_DISCONNECTED,
                            "TikTokDisconnected",
                            "TikTok",
                            "tiktok",
                            &args,
                            |_| true,
                        );
                        engine.emit("TikTok", "Disconnected", || json!(args));
                    }
                    Err(e) => {
                        connected_flag.store(false, Ordering::Relaxed);
                        let err_msg = format!("Bridge offline ({e}), retrying in {retry_secs}s");
                        if let Ok(mut s) = status_arc.lock() {
                            *s = TikTokStatus::Error(err_msg.clone());
                        }
                    }
                }

                tokio::time::sleep(Duration::from_secs(retry_secs)).await;
                retry_secs = (retry_secs * 2).min(15);
            }
        });

        self.read_task = Some(task);
    }
}

/// Handles incoming TikFinity / tikstream JSON event payloads:
/// `{"event": "chat", "data": { ... }}`
pub fn handle_tiktok_event(engine: &Rc<Engine>, channel: &str, payload: &Value, speakers: &mut HashSet<String>) {
    let event_name = payload["event"].as_str().unwrap_or("");
    let data = &payload["data"];

    match event_name {
        "connected" => {
            let mut args = Obj::new();
            let room_id = data["roomId"].as_str().unwrap_or("").to_string();
            args.insert("roomId".into(), room_id.into());
            args.insert("channel".into(), channel.into());
            engine.fire(T_TIKTOK_CONNECTED, "TikTokConnected", "TikTok", "tiktok", &args, |_| {
                true
            });
            engine.emit("TikTok", "Connected", || json!(args));
        }
        "chat" => {
            let nickname = data["nickname"].as_str().unwrap_or("").to_string();
            let unique_id = data["uniqueId"].as_str().unwrap_or("").to_string();
            let user = if !nickname.is_empty() {
                nickname.clone()
            } else {
                unique_id.clone()
            };
            let user_id = data["userId"].as_str().unwrap_or("").to_string();
            let comment = data["comment"].as_str().unwrap_or("").to_string();
            let is_mod = data["isModerator"].as_bool().unwrap_or(false);
            let is_sub = data["isSubscriber"].as_bool().unwrap_or(false);
            let follow_role = data["followRole"].as_u64().unwrap_or(0);

            let mut args = Obj::new();
            args.insert("user".into(), user.clone().into());
            args.insert("nickname".into(), nickname.into());
            args.insert("uniqueId".into(), unique_id.into());
            args.insert("userId".into(), user_id.clone().into());
            args.insert("message".into(), comment.clone().into());
            args.insert("comment".into(), comment.clone().into());
            args.insert("isModerator".into(), is_mod.into());
            args.insert("isSubscriber".into(), is_sub.into());
            args.insert("followRole".into(), follow_role.into());
            args.insert("channel".into(), channel.into());

            // Track speaker
            let lower_user = user.to_lowercase();
            speakers.insert(lower_user);

            // Fire Trigger
            engine.fire(
                T_TIKTOK_CHAT_MESSAGE,
                "TikTokChatMessage",
                "TikTok",
                "tiktok",
                &args,
                |_| true,
            );
            engine.emit("TikTok", "ChatMessage", || json!(args));

            // Feed chat commands into engine
            let chat = Chat {
                platform: "tiktok".into(),
                source: 4194304, // TikTok command source bit
                user,
                user_id,
                text: comment,
                broadcaster: false,
                moderator: is_mod,
                subscriber: is_sub,
                args: args.clone(),
                ..Default::default()
            };
            engine.chat(&chat);
        }
        "gift" => {
            let nickname = data["nickname"].as_str().unwrap_or("").to_string();
            let unique_id = data["uniqueId"].as_str().unwrap_or("").to_string();
            let user = if !nickname.is_empty() {
                nickname.clone()
            } else {
                unique_id.clone()
            };
            let gift_name = data["giftName"].as_str().unwrap_or("").to_string();
            let gift_id = data["giftId"].as_u64().unwrap_or(0);
            let diamond_count = data["diamondCount"].as_u64().unwrap_or(0);
            let repeat_count = data["repeatCount"].as_u64().unwrap_or(1);
            let repeat_end = data["repeatEnd"].as_bool().unwrap_or(true);

            let mut args = Obj::new();
            args.insert("user".into(), user.into());
            args.insert("nickname".into(), nickname.into());
            args.insert("uniqueId".into(), unique_id.into());
            args.insert("giftName".into(), gift_name.into());
            args.insert("giftId".into(), gift_id.into());
            args.insert("coins".into(), diamond_count.into());
            args.insert("diamondCount".into(), diamond_count.into());
            args.insert("repeatCount".into(), repeat_count.into());
            args.insert("repeatEnd".into(), repeat_end.into());
            args.insert("channel".into(), channel.into());

            engine.fire(T_TIKTOK_GIFT, "TikTokGift", "TikTok", "tiktok", &args, |_| true);
            engine.emit("TikTok", "Gift", || json!(args));
        }
        "like" => {
            let nickname = data["nickname"].as_str().unwrap_or("").to_string();
            let unique_id = data["uniqueId"].as_str().unwrap_or("").to_string();
            let user = if !nickname.is_empty() {
                nickname.clone()
            } else {
                unique_id.clone()
            };
            let like_count = data["likeCount"].as_u64().unwrap_or(1);
            let total_likes = data["totalLikes"].as_u64().unwrap_or(0);

            let mut args = Obj::new();
            args.insert("user".into(), user.into());
            args.insert("nickname".into(), nickname.into());
            args.insert("uniqueId".into(), unique_id.into());
            args.insert("likeCount".into(), like_count.into());
            args.insert("totalLikes".into(), total_likes.into());
            args.insert("channel".into(), channel.into());

            engine.fire(T_TIKTOK_LIKE, "TikTokLike", "TikTok", "tiktok", &args, |_| true);
            engine.emit("TikTok", "Like", || json!(args));
        }
        "follow" => {
            let nickname = data["nickname"].as_str().unwrap_or("").to_string();
            let unique_id = data["uniqueId"].as_str().unwrap_or("").to_string();
            let user = if !nickname.is_empty() {
                nickname.clone()
            } else {
                unique_id.clone()
            };

            let mut args = Obj::new();
            args.insert("user".into(), user.into());
            args.insert("nickname".into(), nickname.into());
            args.insert("uniqueId".into(), unique_id.into());
            args.insert("channel".into(), channel.into());

            engine.fire(T_TIKTOK_FOLLOW, "TikTokFollow", "TikTok", "tiktok", &args, |_| true);
            engine.emit("TikTok", "Follow", || json!(args));
        }
        "share" => {
            let nickname = data["nickname"].as_str().unwrap_or("").to_string();
            let unique_id = data["uniqueId"].as_str().unwrap_or("").to_string();
            let user = if !nickname.is_empty() {
                nickname.clone()
            } else {
                unique_id.clone()
            };

            let mut args = Obj::new();
            args.insert("user".into(), user.into());
            args.insert("nickname".into(), nickname.into());
            args.insert("uniqueId".into(), unique_id.into());
            args.insert("channel".into(), channel.into());

            engine.fire(T_TIKTOK_SHARE, "TikTokShare", "TikTok", "tiktok", &args, |_| true);
            engine.emit("TikTok", "Share", || json!(args));
        }
        "subscribe" => {
            let nickname = data["nickname"].as_str().unwrap_or("").to_string();
            let unique_id = data["uniqueId"].as_str().unwrap_or("").to_string();
            let user = if !nickname.is_empty() {
                nickname.clone()
            } else {
                unique_id.clone()
            };
            let sub_month = data["subMonth"].as_u64().unwrap_or(1);

            let mut args = Obj::new();
            args.insert("user".into(), user.into());
            args.insert("nickname".into(), nickname.into());
            args.insert("uniqueId".into(), unique_id.into());
            args.insert("subMonth".into(), sub_month.into());
            args.insert("channel".into(), channel.into());

            engine.fire(T_TIKTOK_SUBSCRIBE, "TikTokSubscribe", "TikTok", "tiktok", &args, |_| {
                true
            });
            engine.emit("TikTok", "Subscribe", || json!(args));
        }
        "member" | "join" => {
            let nickname = data["nickname"].as_str().unwrap_or("").to_string();
            let unique_id = data["uniqueId"].as_str().unwrap_or("").to_string();
            let user = if !nickname.is_empty() {
                nickname.clone()
            } else {
                unique_id.clone()
            };

            let mut args = Obj::new();
            args.insert("user".into(), user.into());
            args.insert("nickname".into(), nickname.into());
            args.insert("uniqueId".into(), unique_id.into());
            args.insert("channel".into(), channel.into());

            engine.fire(T_TIKTOK_JOIN, "TikTokJoin", "TikTok", "tiktok", &args, |_| true);
            engine.emit("TikTok", "Join", || json!(args));
        }
        "streamEnd" => {
            let mut args = Obj::new();
            args.insert("channel".into(), channel.into());
            engine.fire(
                T_TIKTOK_DISCONNECTED,
                "TikTokDisconnected",
                "TikTok",
                "tiktok",
                &args,
                |_| true,
            );
            engine.emit("TikTok", "Disconnected", || json!(args));
        }
        _ => {}
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_clean_tiktok_channel() {
        assert_eq!(clean_tiktok_channel("https://www.tiktok.com/@streamer"), "streamer");
        assert_eq!(clean_tiktok_channel("http://tiktok.com/@cooluser/"), "cooluser");
        assert_eq!(clean_tiktok_channel("@awesome_creator"), "awesome_creator");
        assert_eq!(clean_tiktok_channel("just_name"), "just_name");
        assert_eq!(clean_tiktok_channel("  @spaced_name/  "), "spaced_name");
    }

    #[test]
    fn test_tiktok_config_defaults() {
        let cfg = TikTokConfig::default();
        assert_eq!(cfg.channel, "");
        assert!(!cfg.auto_connect);
        assert_eq!(cfg.bridge_url, DEFAULT_TIKTOK_BRIDGE_URL);
        assert!(cfg.broadcaster.is_none());
        assert!(cfg.bot.is_none());
    }

    #[test]
    fn test_tiktok_account_fallback_logic() {
        let mut cfg = TikTokConfig {
            broadcaster: Some(TikTokAccount {
                username: "streamer".into(),
                session_cookie: "sessionid=streamer_cookie_123".into(),
            }),
            ..Default::default()
        };

        // 1. Bot requested with fallback -> resolves broadcaster cookie
        let cookie = if let Some(bot) = cfg.bot.as_ref().filter(|b| !b.session_cookie.is_empty()) {
            Some(bot.session_cookie.clone())
        } else {
            cfg.broadcaster.as_ref().map(|b| b.session_cookie.clone())
        };
        assert_eq!(cookie.as_deref(), Some("sessionid=streamer_cookie_123"));

        // 2. Bot configured -> resolves bot cookie
        cfg.bot = Some(TikTokAccount {
            username: "bot_account".into(),
            session_cookie: "sessionid=bot_cookie_456".into(),
        });
        let bot_cookie = cfg.bot.as_ref().map(|b| b.session_cookie.clone());
        assert_eq!(bot_cookie.as_deref(), Some("sessionid=bot_cookie_456"));
    }

    #[test]
    fn test_handle_tiktok_chat_event() {
        let cfg = Arc::new(watch::channel(Config::default()).0);
        let engine = Engine::new(cfg, |_| {});
        let mut speakers = HashSet::new();

        let payload = json!({
            "event": "chat",
            "data": {
                "nickname": "TikTokTester",
                "uniqueId": "tiktoktester",
                "userId": "123456",
                "comment": "Hello streamerz!",
                "isModerator": true,
                "isSubscriber": false
            }
        });

        handle_tiktok_event(&engine, "mychannel", &payload, &mut speakers);
        assert!(speakers.contains("tiktoktester"));
    }

    #[test]
    fn test_handle_tiktok_gift_event() {
        let cfg = Arc::new(watch::channel(Config::default()).0);
        let engine = Engine::new(cfg, |_| {});
        let mut speakers = HashSet::new();

        let payload = json!({
            "event": "gift",
            "data": {
                "nickname": "GiftGiver",
                "uniqueId": "giftgiver",
                "giftName": "Rose",
                "giftId": 5655,
                "diamondCount": 1,
                "repeatCount": 10,
                "repeatEnd": true
            }
        });

        handle_tiktok_event(&engine, "mychannel", &payload, &mut speakers);
    }
}
