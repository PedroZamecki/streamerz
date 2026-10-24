//! TikTok Live, read natively (see webcast.rs): chat, gifts, likes, follows, shares, subscriptions
//! and joins fire the TikTok triggers (41001..=41009) and chat commands. Sending (43001) posts with the
//! logged-in account's session. Login is the tikstream-login window.

use crate::commands::Chat;
use crate::engine::Engine;
use crate::model::Obj;
use crate::Config;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
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

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TikTokAccount {
    /// TikTok username / handle (@handle or nickname).
    pub username: String,
    /// Session cookie string (must contain sessionid=...).
    pub session_cookie: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TikTokConfig {
    /// Channel name or @handle to watch.
    pub channel: String,
    /// Whether to connect on startup.
    pub auto_connect: bool,
    /// Broadcaster account credentials
    pub broadcaster: Option<TikTokAccount>,
    /// Bot account credentials
    pub bot: Option<TikTokAccount>,
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

#[derive(Clone, Debug, PartialEq, Default)]
pub enum TikTokStatus {
    #[default]
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
}

/// What the reader shares with the handle (read from the UI's queries).
#[derive(Clone, Default)]
struct Shared {
    connected: Arc<AtomicBool>,
    status: Arc<Mutex<TikTokStatus>>,
    room_id: Arc<Mutex<Option<String>>>,
}

impl Shared {
    fn set(&self, s: TikTokStatus) {
        self.connected
            .store(matches!(s, TikTokStatus::Connected(_)), Ordering::Relaxed);
        if let Ok(mut m) = self.status.lock() {
            *m = s;
        }
        crate::ui::changed();
    }
}

#[derive(Clone)]
pub struct TikTokHandle {
    tx: mpsc::UnboundedSender<TikTokCmd>,
    shared: Shared,
}

impl TikTokHandle {
    pub fn send(&self, cmd: TikTokCmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn room_id(&self) -> Option<String> {
        self.shared.room_id.lock().ok().and_then(|r| r.clone())
    }

    pub fn is_connected(&self) -> bool {
        self.shared.connected.load(Ordering::Relaxed)
    }

    pub fn current_status(&self) -> TikTokStatus {
        self.shared.status.lock().map(|s| s.clone()).unwrap_or_default()
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
    let shared = Shared::default();
    let runner = Runner {
        engine,
        cfg: cfg.clone(),
        rx,
        shared: shared.clone(),
        read_task: None,
    };
    tokio::task::spawn_local(runner.run());
    TikTokHandle { tx, shared }
}

struct Runner {
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    rx: mpsc::UnboundedReceiver<TikTokCmd>,
    shared: Shared,
    read_task: Option<tokio::task::JoinHandle<()>>,
}

impl Runner {
    async fn run(mut self) {
        if self.cfg.borrow().tiktok.auto_connect {
            self.connect();
        }
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                TikTokCmd::Connect => self.connect(),
                TikTokCmd::Disconnect => {
                    if let Some(task) = self.read_task.take() {
                        task.abort();
                        if self.shared.connected.load(Ordering::Relaxed) {
                            disconnected(&self.engine, &self.cfg.borrow().tiktok.channel);
                        }
                    }
                    self.shared.set(TikTokStatus::Disconnected);
                    crate::say("TikTok: disconnected".into());
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
            let rid = self.shared.room_id.lock().ok().and_then(|r| r.clone());
            (cookie, rid)
        };

        if room_id.is_none() {
            let chan = clean_tiktok_channel(&self.cfg.borrow().tiktok.channel);
            if let Ok(t) = crate::webcast::ttwid().await {
                room_id = crate::webcast::room_id(&chan, &t).await.ok().map(|r| r.to_string());
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

    fn connect(&mut self) {
        if let Some(task) = self.read_task.take() {
            task.abort();
        }
        let (engine, cfg, shared) = (self.engine.clone(), self.cfg.clone(), self.shared.clone());
        self.read_task = Some(tokio::task::spawn_local(async move {
            loop {
                let channel = clean_tiktok_channel(&cfg.borrow().tiktok.channel);
                if channel.is_empty() {
                    shared.set(TikTokStatus::Error("set the channel".into()));
                    return;
                }
                let wait = match live(&engine, &cfg, &channel, &shared).await {
                    Ok(()) => 2,
                    Err((e, wait)) => {
                        if shared.connected.load(Ordering::Relaxed) {
                            disconnected(&engine, &channel);
                        }
                        crate::say(format!("TikTok: {e}"));
                        shared.set(TikTokStatus::Error(format!("{e}, retrying in {wait}s")));
                        wait
                    }
                };
                tokio::time::sleep(Duration::from_secs(wait)).await;
            }
        }));
    }
}

/// One connection to the channel's live, until it drops or the stream ends. The error says why, and
/// how many seconds to wait before trying again.
async fn live(
    engine: &Rc<Engine>,
    cfg: &watch::Sender<Config>,
    channel: &str,
    shared: &Shared,
) -> Result<(), (String, u64)> {
    shared.set(TikTokStatus::Connecting(format!("looking for @{channel}'s live...")));
    let ttwid = crate::webcast::ttwid().await.map_err(|e| (e, 10))?;
    // ponytail: fixed 30s while offline; TikTok answers 429 to faster lookups
    let room = crate::webcast::room_id(channel, &ttwid).await.map_err(|e| (e, 30))?;
    if let Ok(mut r) = shared.room_id.lock() {
        *r = Some(room.to_string());
    }
    let mut cookie = format!("ttwid={ttwid}");
    // A login lets 18+ rooms in.
    if let Some(a) = cfg
        .borrow()
        .tiktok
        .broadcaster
        .as_ref()
        .filter(|a| !a.session_cookie.is_empty())
    {
        cookie = format!("{cookie}; {}", a.session_cookie);
    }
    let mut req = crate::webcast::ws_url(room)
        .into_client_request()
        .map_err(|e| (e.to_string(), 30))?;
    for (k, v) in [
        ("User-Agent", crate::webcast::UA),
        ("Referer", "https://www.tiktok.com/"),
        ("Origin", "https://www.tiktok.com"),
        ("Cookie", &cookie),
    ] {
        if let Ok(v) = v.parse() {
            req.headers_mut().insert(k, v);
        }
    }
    let (ws, _) = crate::http::ws_connect(req).await.map_err(|e| {
        // A rejected handshake says why in its headers (DEVICE_BLOCKED: a new ttwid fixes it).
        let why = match &e {
            tokio_tungstenite::tungstenite::Error::Http(r) => r
                .headers()
                .get("Handshake-Msg")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("refused")
                .to_string(),
            e => e.to_string(),
        };
        (format!("couldn't join @{channel}'s live ({why})"), 5)
    })?;
    let (mut w, mut r) = ws.split();
    let send = |m: Vec<u8>| Message::Binary(m.into());
    w.send(send(crate::webcast::heartbeat(room)))
        .await
        .map_err(|e| (e.to_string(), 5))?;
    w.send(send(crate::webcast::enter_room(room)))
        .await
        .map_err(|e| (e.to_string(), 5))?;

    shared.set(TikTokStatus::Connected(format!("live @{channel}")));
    crate::say(format!("TikTok: connected to @{channel}'s live (room {room})"));
    let mut args = Obj::new();
    args.insert("channel".into(), channel.into());
    args.insert("roomId".into(), room.to_string().into());
    engine.fire(T_TIKTOK_CONNECTED, "TikTokConnected", "TikTok", "tiktok", &args, |_| {
        true
    });
    engine.emit("TikTok", "Connected", || json!(args));

    let mut hb = tokio::time::interval(Duration::from_secs(10));
    hb.tick().await;
    loop {
        tokio::select! {
            _ = hb.tick() => {
                if let Err(e) = w.send(send(crate::webcast::heartbeat(room))).await {
                    return Err((format!("connection lost ({e})"), 2));
                }
            }
            m = tokio::time::timeout(Duration::from_secs(60), r.next()) => match m {
                Ok(Some(Ok(Message::Binary(d)))) => {
                    let (events, ack) = crate::webcast::read(&d);
                    if let Some(a) = ack {
                        let _ = w.send(send(a)).await;
                    }
                    for (name, data) in events {
                        if name == "streamEnd" {
                            return Err(("the stream ended".into(), 30));
                        }
                        handle_tiktok_event(engine, channel, name, &data);
                    }
                }
                Ok(Some(Ok(_))) => {}
                Ok(Some(Err(e))) => return Err((format!("connection lost ({e})"), 2)),
                Ok(None) => return Err(("TikTok closed the connection".into(), 2)),
                Err(_) => return Err(("no data for 60s".into(), 2)),
            }
        }
    }
}

fn disconnected(engine: &Rc<Engine>, channel: &str) {
    let mut args = Obj::new();
    args.insert("channel".into(), clean_tiktok_channel(channel).into());
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

/// One TikTok event (TikTok-Live-Connector names and fields, as webcast.rs makes them): its
/// trigger, the WebSocket event, and for chat the commands and the Chat tab.
pub fn handle_tiktok_event(engine: &Rc<Engine>, channel: &str, event: &str, d: &Value) {
    let (kind, name, ev) = match event {
        "chat" => (T_TIKTOK_CHAT_MESSAGE, "TikTokChatMessage", "ChatMessage"),
        "gift" => (T_TIKTOK_GIFT, "TikTokGift", "Gift"),
        "like" => (T_TIKTOK_LIKE, "TikTokLike", "Like"),
        "follow" => (T_TIKTOK_FOLLOW, "TikTokFollow", "Follow"),
        "share" => (T_TIKTOK_SHARE, "TikTokShare", "Share"),
        "subscribe" => (T_TIKTOK_SUBSCRIBE, "TikTokSubscribe", "Subscribe"),
        "member" => (T_TIKTOK_JOIN, "TikTokJoin", "Join"),
        _ => return,
    };
    let s = |k: &str| d[k].as_str().unwrap_or("").to_string();
    let user = Some(s("nickname"))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| s("uniqueId"));
    let mut args: Obj = d.as_object().cloned().unwrap_or_default().into_iter().collect();
    args.insert("user".into(), user.clone().into());
    args.insert("channel".into(), channel.into());
    match event {
        "chat" => {
            args.insert("message".into(), s("comment").into());
        }
        "gift" => {
            let coins = d["diamondCount"].as_u64().unwrap_or(0) * d["repeatCount"].as_u64().unwrap_or(1).max(1);
            args.insert("coins".into(), coins.into());
        }
        "like" => {
            args.insert("totalLikes".into(), d["totalLikeCount"].clone());
        }
        _ => {}
    }
    engine.fire(kind, name, "TikTok", "tiktok", &args, |_| true);
    engine.emit("TikTok", ev, || json!(args));
    if event == "chat" {
        let flag = |k: &str| d[k].as_bool().unwrap_or(false);
        engine.chat(&Chat {
            platform: "tiktok".into(),
            source: 4_194_304,
            user,
            user_id: s("userId"),
            text: s("comment"),
            broadcaster: s("uniqueId").eq_ignore_ascii_case(channel),
            moderator: flag("isModerator"),
            subscriber: flag("isSubscriber"),
            args,
            ..Default::default()
        });
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

    /// A chat from TikTok's wire format runs a command, as the real stream would.
    #[test]
    fn chat_runs_commands() {
        let cfg = Arc::new(watch::channel(Config::default()).0);
        let engine = Engine::new(cfg, |_| {});
        let d = json!({ "nickname": "Tester", "uniqueId": "tester", "userId": "1", "comment": "hi" });
        handle_tiktok_event(&engine, "mychannel", "chat", &d);
        let gift = json!({ "nickname": "G", "giftName": "Rose", "diamondCount": 1, "repeatCount": 10 });
        handle_tiktok_event(&engine, "mychannel", "gift", &gift);
        handle_tiktok_event(&engine, "mychannel", "roomUser", &json!({}));
    }
}
