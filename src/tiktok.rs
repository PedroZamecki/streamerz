//! TikTok Live, read natively (see webcast.rs): chat, gifts, likes, follows, shares, subscriptions
//! and joins fire the TikTok triggers (41001..=41009) and chat commands. Sending (43001) posts with the
//! logged-in account's session. Login is the tikstream-login window.

use crate::commands::Chat;
use crate::engine::chatlog::{local_hhmm, normalize_line_emotes, Line};
use crate::engine::Engine;
use crate::model::Obj;
use crate::webcast::RoomLookupError;
use crate::Config;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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
pub const T_TIKTOK_VIEWER_COUNT: u32 = 41010;
pub const T_TIKTOK_STREAM_ONLINE: u32 = 41011;
pub const T_TIKTOK_STREAM_OFFLINE: u32 = 41012;

/// The live being read: its viewers, likes and gifts, for Get Stream Info and Get Top Gifters.
#[derive(Default)]
struct Stream {
    room: String,
    viewers: u64,
    likes: u64,
    coins: u64,
    /// By user id: name and coins sent.
    gifters: std::collections::HashMap<String, (String, u64)>,
}

thread_local!(static STREAM: std::cell::RefCell<Stream> = std::cell::RefCell::default());

/// Get Stream Info's variables.
pub fn stream_info(live: bool) -> Obj {
    STREAM.with_borrow(|s| {
        let mut a = Obj::new();
        a.insert("isLive".into(), live.into());
        a.insert("roomId".into(), s.room.clone().into());
        a.insert("viewerCount".into(), s.viewers.into());
        a.insert("totalLikes".into(), s.likes.into());
        a.insert("totalCoins".into(), s.coins.into());
        a
    })
}

/// Get Top Gifters' variables: the `count` who sent the most coins this live.
pub fn top_gifters(count: usize) -> Obj {
    STREAM.with_borrow(|s| {
        let mut top: Vec<(&String, &(String, u64))> = s.gifters.iter().collect();
        top.sort_by(|a, b| b.1 .1.cmp(&a.1 .1).then_with(|| a.1 .0.cmp(&b.1 .0)));
        top.truncate(count);
        let mut a = Obj::new();
        a.insert("topGifterCount".into(), top.len().into());
        for (i, (id, (name, coins))) in top.iter().enumerate() {
            a.insert(format!("topGifter{i}"), name.clone().into());
            a.insert(format!("topGifter{i}UserId"), (*id).clone().into());
            a.insert(format!("topGifter{i}Coins"), (*coins).into());
        }
        a
    })
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TikTokAccount {
    /// TikTok username / handle (@handle or nickname).
    pub username: String,
    /// Session cookie string (must contain sessionid=...).
    pub session_cookie: String,
    /// Profile picture URL, seen at login (TikTok's profile pages are behind a bot check).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub avatar: String,
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

/// The TikTok window's browser data per account, next to streamerz.json, so logging in again is one
/// click and sending uses the same session. Log out deletes it.
pub fn profile(bot: bool) -> PathBuf {
    crate::config_path().with_file_name(if bot { "tiktok-bot" } else { "tiktok-broadcaster" })
}

/// The TikTok window (tiktok_web.rs) with `args`: its own binary on Linux, `streamerz.exe --tiktok` on
/// Windows. Also checks sibling target profiles (`release`/`debug`) so fast dev builds reuse it.
fn helper(args: &[&std::ffi::OsStr]) -> Result<std::process::Command, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new(exe);
        c.arg("--tiktok");
        c
    } else {
        let mut bin = exe.with_file_name("streamerz-tiktok");
        if !bin.is_file() {
            if let Some(target_dir) = exe.parent().and_then(|p| p.parent()) {
                for profile in ["release", "debug"] {
                    let alt = target_dir.join(profile).join("streamerz-tiktok");
                    if alt.is_file() {
                        bin = alt;
                        break;
                    }
                }
            }
        }
        if !bin.is_file() {
            return Err(format!(
                "{} is missing (build with `--features tiktok-web` when developing)",
                bin.display()
            ));
        }
        std::process::Command::new(bin)
    };
    cmd.args(args);
    Ok(cmd)
}

/// Opens TikTok's login in the TikTok window; blocks until it closes.
pub fn login(bot: bool) -> Result<TikTokAccount, String> {
    let p = profile(bot);
    let out = helper(&["login".as_ref(), p.as_os_str()])?
        .output()
        .map_err(|e| format!("can't open the TikTok window: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "the login window was closed. {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "the login window answered nothing".to_string())?;
    match v["cookies"].as_str().filter(|c| !c.is_empty()) {
        Some(c) => Ok(TikTokAccount {
            username: v["username"].as_str().unwrap_or("").into(),
            session_cookie: c.into(),
            avatar: v["avatar"].as_str().unwrap_or("").into(),
        }),
        None => Err("no TikTok session after login".into()),
    }
}

/// A running send window: the account's session on the channel's live page.
struct Sender {
    channel: String,
    _child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    out: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
}

impl Sender {
    fn spawn(bot: bool, channel: &str) -> Result<Sender, String> {
        let p = profile(bot);
        let mut cmd = tokio::process::Command::from(helper(&["send".as_ref(), p.as_os_str(), channel.as_ref()])?);
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("can't open the TikTok window: {e}"))?;
        let (Some(stdin), Some(out)) = (child.stdin.take(), child.stdout.take()) else {
            return Err("can't talk to the TikTok window".into());
        };
        use tokio::io::AsyncBufReadExt;
        Ok(Sender {
            channel: channel.into(),
            _child: child,
            stdin,
            out: tokio::io::BufReader::new(out).lines(),
        })
    }

    /// None: the window is gone (it closes after 2 idle minutes); start another.
    async fn send(&mut self, text: &str) -> Option<Result<(), String>> {
        use tokio::io::AsyncWriteExt;
        let line = format!("{}\n", serde_json::to_string(text).unwrap_or_default());
        self.stdin.write_all(line.as_bytes()).await.ok()?;
        // The first message waits for the live page to load.
        let r = tokio::time::timeout(Duration::from_secs(90), self.out.next_line()).await;
        let v: Value = match r {
            Ok(Ok(Some(l))) => serde_json::from_str(&l).unwrap_or_default(),
            Ok(_) => return None,
            Err(_) => return Some(Err("TikTok's page didn't answer in 90s".into())),
        };
        Some(match v["error"].as_str() {
            Some(e) => Err(e.to_string()),
            None => Ok(()),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum TikTokStatus {
    #[default]
    Disconnected,
    /// Account exists and is currently offline, waiting for stream start.
    Standby(String),
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

    pub fn is_standby(&self) -> bool {
        matches!(self.current_status(), TikTokStatus::Standby(_))
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
        senders: [None, None],
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
    /// Broadcaster's and bot's send windows, opened on first use.
    senders: [Option<Sender>; 2],
}

fn account_matches_channel(acct: &Option<TikTokAccount>, channel: &str) -> bool {
    let Some(a) = acct else { return false };
    let norm = |s: &str| {
        clean_tiktok_channel(s)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_lowercase()
    };
    let u = norm(&a.username);
    let ch = norm(channel);
    !ch.is_empty() && u == ch
}

enum LiveEnd {
    Standby { never_live: bool },
    NotFound,
    Error(String, u64),
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

    async fn send_chat_message(&mut self, text: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        let (bot, channel) = {
            let c = self.cfg.borrow();
            if crate::secrets::is_locked() {
                return Err("TikTok: keyring is locked, click Unlock in Accounts".into());
            }
            let has = |bot: bool| {
                if bot {
                    c.tiktok.bot.is_some()
                } else {
                    c.tiktok.broadcaster.is_some()
                }
            };
            let bot = if has(use_bot) {
                use_bot
            } else if fallback && has(!use_bot) {
                !use_bot
            } else {
                return Err(format!(
                    "TikTok: log in the {} first",
                    if use_bot { "bot" } else { "broadcaster" }
                ));
            };
            (bot, clean_tiktok_channel(&c.tiktok.channel))
        };
        if channel.is_empty() {
            return Err("TikTok: set the channel first".into());
        }
        let slot = &mut self.senders[usize::from(bot)];
        for _ in 0..2 {
            if slot.as_ref().is_none_or(|s| s.channel != channel) {
                *slot = Some(Sender::spawn(bot, &channel)?);
            }
            if let Some(r) = slot.as_mut().unwrap().send(text).await {
                return r;
            }
            *slot = None;
        }
        Err("the TikTok window closed".into())
    }

    fn connect(&mut self) {
        if let Some(task) = self.read_task.take() {
            task.abort();
        }
        let (engine, cfg, shared) = (self.engine.clone(), self.cfg.clone(), self.shared.clone());
        self.read_task = Some(tokio::task::spawn_local(async move {
            let mut said = String::new();
            let mut standby_logged = false;
            loop {
                let (channel, known_account) = {
                    let c = cfg.borrow();
                    let ch = clean_tiktok_channel(&c.tiktok.channel);
                    let known = account_matches_channel(&c.tiktok.broadcaster, &ch)
                        || account_matches_channel(&c.tiktok.bot, &ch);
                    (ch, known)
                };
                if channel.is_empty() {
                    shared.set(TikTokStatus::Error("set the channel".into()));
                    return;
                }
                let wait = match live(&engine, &cfg, &channel, known_account, &shared).await {
                    Ok(()) => {
                        said.clear();
                        standby_logged = false;
                        2
                    }
                    Err(LiveEnd::Standby { never_live }) => {
                        if shared.connected.load(Ordering::Relaxed) {
                            disconnected(&engine, &channel);
                        }
                        // Not live is no news: the connections window shows the standby.
                        if !standby_logged {
                            standby_logged = true;
                            said.clear();
                        }
                        let label = if never_live {
                            format!("@{channel} isn't live yet")
                        } else {
                            format!("@{channel} isn't live, waiting")
                        };
                        shared.set(TikTokStatus::Standby(label));
                        if engine.stream_warming_up() {
                            10
                        } else if never_live {
                            120
                        } else {
                            90
                        }
                    }
                    Err(LiveEnd::NotFound) => {
                        if shared.connected.load(Ordering::Relaxed) {
                            disconnected(&engine, &channel);
                        }
                        let msg = format!("@{channel} not found, check the @handle in Accounts");
                        if msg != said {
                            crate::say(format!("TikTok: {msg}"));
                            said.clone_from(&msg);
                            standby_logged = false;
                        }
                        shared.set(TikTokStatus::Error(msg));
                        180
                    }
                    Err(LiveEnd::Error(e, wait)) => {
                        if shared.connected.load(Ordering::Relaxed) {
                            disconnected(&engine, &channel);
                        }
                        if e != said {
                            crate::say(format!("TikTok (@{channel}): {e}"));
                            said.clone_from(&e);
                            standby_logged = false;
                        }
                        shared.set(TikTokStatus::Error(format!("{e}, retrying in {wait}s")));
                        wait
                    }
                };
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(wait)) => {}
                    _ = engine.live_wake.notified() => {
                        crate::say(format!("TikTok: stream started, checking @{channel}"));
                    }
                }
            }
        }));
    }
}

/// One connection to the channel's live, until it drops or the stream ends.
async fn live(
    engine: &Rc<Engine>,
    cfg: &watch::Sender<Config>,
    channel: &str,
    known_account: bool,
    shared: &Shared,
) -> Result<(), LiveEnd> {
    if !matches!(shared.status.lock().map(|s| s.clone()), Ok(TikTokStatus::Standby(_))) {
        shared.set(TikTokStatus::Connecting(format!("checking @{channel}'s live...")));
    }
    let ttwid = crate::webcast::ttwid().await.map_err(|e| LiveEnd::Error(e, 15))?;
    let room = match crate::webcast::room_id(channel, &ttwid, known_account).await {
        Ok(r) => r,
        Err(RoomLookupError::NeverLive) => return Err(LiveEnd::Standby { never_live: true }),
        Err(RoomLookupError::Offline) => return Err(LiveEnd::Standby { never_live: false }),
        Err(RoomLookupError::NotFound) => return Err(LiveEnd::NotFound),
        Err(RoomLookupError::Transient(e)) => return Err(LiveEnd::Error(e, 30)),
    };
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
        .map_err(|e| LiveEnd::Error(e.to_string(), 30))?;
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
        let why = match &e {
            tokio_tungstenite::tungstenite::Error::Http(r) => r
                .headers()
                .get("Handshake-Msg")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("refused")
                .to_string(),
            e => e.to_string(),
        };
        LiveEnd::Error(format!("couldn't join @{channel}'s live ({why})"), 5)
    })?;
    let (mut w, mut r) = ws.split();
    let send = |m: Vec<u8>| Message::Binary(m.into());
    w.send(send(crate::webcast::heartbeat(room)))
        .await
        .map_err(|e| LiveEnd::Error(e.to_string(), 5))?;
    w.send(send(crate::webcast::enter_room(room)))
        .await
        .map_err(|e| LiveEnd::Error(e.to_string(), 5))?;

    let joined_sec = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    engine.chat_welcome("tiktok", Vec::new());

    shared.set(TikTokStatus::Connected(format!("live @{channel}")));
    crate::say(format!("TikTok: connected to @{channel}'s live (room {room})"));
    let mut args = Obj::new();
    args.insert("channel".into(), channel.into());
    args.insert("roomId".into(), room.to_string().into());
    // A room not seen before is a new live: its tallies start over, and Stream Online fires.
    let new_live = STREAM.with_borrow_mut(|s| {
        let new = s.room != room.to_string();
        if new {
            *s = Stream {
                room: room.to_string(),
                ..Stream::default()
            };
        }
        new
    });
    if new_live {
        engine.fire(
            T_TIKTOK_STREAM_ONLINE,
            "TikTokStreamOnline",
            "TikTok",
            "tiktok",
            &args,
            |_| true,
        );
        engine.emit("TikTok", "StreamOnline", || json!(args));
        engine.reset_first_words(Some("tiktok"));
    }
    engine.fire(T_TIKTOK_CONNECTED, "TikTokConnected", "TikTok", "tiktok", &args, |_| {
        true
    });
    engine.emit("TikTok", "Connected", || json!(args));

    let mut hb = tokio::time::interval(Duration::from_secs(10));
    hb.tick().await;
    let mut prejoin_window_open = true;
    let joined_inst = std::time::Instant::now();
    loop {
        tokio::select! {
            _ = hb.tick() => {
                if let Err(e) = w.send(send(crate::webcast::heartbeat(room))).await {
                    return Err(LiveEnd::Error(format!("connection lost ({e})"), 2));
                }
            }
            m = tokio::time::timeout(Duration::from_secs(60), r.next()) => match m {
                Ok(Some(Ok(Message::Binary(d)))) => {
                    let (events, ack) = crate::webcast::read(&d);
                    if let Some(a) = ack {
                        let _ = w.send(send(a)).await;
                    }
                    if prejoin_window_open && joined_inst.elapsed() > Duration::from_secs(4) {
                        prejoin_window_open = false;
                    }
                    let mut prejoin_batch = Vec::new();
                    for (name, data) in events {
                        if name == "streamEnd" {
                            crate::say(format!("TikTok: @{channel}'s live stream ended"));
                            let mut args = stream_info(false);
                            args.insert("channel".into(), channel.into());
                            engine.fire(T_TIKTOK_STREAM_OFFLINE, "TikTokStreamOffline", "TikTok", "tiktok", &args, |_| true);
                            engine.emit("TikTok", "StreamOffline", || json!(args));
                            return Err(LiveEnd::Standby { never_live: false });
                        }
                        if prejoin_window_open && name == "chat" && is_prejoin_comment(&data, joined_sec) {
                            if let Some(line) = tiktok_chat_line(channel, &data) {
                                prejoin_batch.push(line);
                            }
                            continue;
                        }
                        handle_tiktok_event(engine, channel, name, &data);
                    }
                    if !prejoin_batch.is_empty() {
                        engine.chat_welcome("tiktok", prejoin_batch);
                    }
                }
                Ok(Some(Ok(_))) => {}
                Ok(Some(Err(e))) => return Err(LiveEnd::Error(format!("connection lost ({e})"), 2)),
                Ok(None) => return Err(LiveEnd::Error("TikTok closed the connection".into(), 2)),
                Err(_) => return Err(LiveEnd::Error("no data for 60s".into(), 2)),
            }
        }
    }
}

fn comment_unix_sec(d: &Value) -> u64 {
    let raw = d["createTime"].as_u64().unwrap_or(0);
    if raw > 10_000_000_000 {
        raw / 1000
    } else {
        raw
    }
}

fn is_prejoin_comment(d: &Value, joined_sec: u64) -> bool {
    let ts = comment_unix_sec(d);
    ts > 0 && ts.saturating_add(2) < joined_sec
}

/// Extract TikTok badge list (platform CDN images + role badges) and TikTok Live dark-mode username color.
pub fn tiktok_badges_and_color(channel: &str, d: &Value) -> (Vec<(String, String)>, String) {
    let unique_id = d["uniqueId"].as_str().unwrap_or("");
    let broadcaster = !channel.is_empty() && unique_id.eq_ignore_ascii_case(channel);
    let moderator = d["isModerator"].as_bool().unwrap_or(false);
    let subscriber = d["isSubscriber"].as_bool().unwrap_or(false);
    let friend = d["followRole"].as_u64() == Some(2);

    let mut badges: Vec<(String, String)> = Vec::new();
    if broadcaster {
        badges.push(("tiktok:host".into(), String::new()));
    } else if moderator {
        badges.push(("tiktok:moderator".into(), String::new()));
    }
    let mut has_img_badge = false;
    for u in d["badgeUrls"].as_array().into_iter().flatten() {
        if let Some(url) = u.as_str().filter(|u| u.starts_with("https://")) {
            badges.push(("tiktok:badge".into(), url.to_string()));
            has_img_badge = true;
        }
    }
    if subscriber && !has_img_badge {
        badges.push(("tiktok:subscriber".into(), String::new()));
    }
    if friend && !broadcaster {
        badges.push(("tiktok:friend".into(), String::new()));
    }

    let color = if broadcaster {
        "#FE2C55"
    } else if moderator {
        "#25F4EE"
    } else if subscriber {
        "#FF9F2E"
    } else {
        "#8EC5FF"
    };
    (badges, color.to_string())
}

fn tiktok_emotes(d: &Value) -> Vec<(String, String)> {
    d["emotes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let tok = e["token"].as_str()?;
            let url = e["url"].as_str()?;
            (!tok.is_empty() && url.starts_with("https://")).then(|| (tok.to_string(), url.to_string()))
        })
        .collect()
}

fn tiktok_chat_line(channel: &str, d: &Value) -> Option<Line> {
    let s = |k: &str| d[k].as_str().unwrap_or("").to_string();
    let user = Some(s("nickname"))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| s("uniqueId"));
    let comment = s("comment");
    if user.is_empty() || comment.is_empty() {
        return None;
    }
    let raw_emotes = tiktok_emotes(d);
    let (text, emotes) = normalize_line_emotes("tiktok", comment, &raw_emotes);
    let (badges, color) = tiktok_badges_and_color(channel, d);
    let ts = comment_unix_sec(d);
    let time = if ts > 0 {
        local_hhmm(ts as i64)
    } else {
        crate::fmt::shown_time(&crate::fmt::now())
    };
    Some(Line {
        platform: "tiktok".into(),
        msg_id: s("msgId"),
        time,
        user,
        user_id: s("userId"),
        text,
        emotes,
        broadcaster: s("uniqueId").eq_ignore_ascii_case(channel),
        moderator: d["isModerator"].as_bool().unwrap_or(false),
        subscriber: d["isSubscriber"].as_bool().unwrap_or(false),
        badges,
        color,
        historical: true,
        ..Default::default()
    })
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
        "roomUser" => (T_TIKTOK_VIEWER_COUNT, "TikTokViewerCountUpdate", "ViewerCountUpdate"),
        _ => return,
    };
    // Every few seconds, mostly the same: only a change fires.
    if event == "roomUser" {
        let n = d["viewerCount"].as_u64().unwrap_or(0);
        if STREAM.with_borrow_mut(|s| std::mem::replace(&mut s.viewers, n)) == n {
            return;
        }
    }
    let s = |k: &str| d[k].as_str().unwrap_or("").to_string();
    let user = Some(s("nickname"))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| s("uniqueId"));
    let mut args: Obj = d.as_object().cloned().unwrap_or_default().into_iter().collect();
    args.insert("user".into(), user.clone().into());
    args.insert("channel".into(), channel.into());
    let (badges, color) = tiktok_badges_and_color(channel, d);
    match event {
        "chat" => {
            args.insert("message".into(), s("comment").into());
            args.insert("color".into(), color.into());
            let badge_json: Vec<Value> = badges
                .iter()
                .map(|(k, url)| json!({ "key": k, "name": k, "imageUrl": url }))
                .collect();
            args.insert("badges".into(), Value::Array(badge_json));
        }
        "gift" => {
            let coins = d["diamondCount"].as_u64().unwrap_or(0) * d["repeatCount"].as_u64().unwrap_or(1).max(1);
            args.insert("coins".into(), coins.into());
            // A combo gift (type 1) repeats with a growing count while it lasts: counted when it ends.
            let done = d["giftType"].as_u64() != Some(1) || d["repeatEnd"].as_bool().unwrap_or(true);
            let total = STREAM.with_borrow_mut(|st| {
                let g = st.gifters.entry(s("userId")).or_insert_with(|| (user.clone(), 0));
                if done {
                    g.1 += coins;
                    st.coins += coins;
                }
                g.1
            });
            args.insert("userTotalCoins".into(), total.into());
        }
        "like" => {
            args.insert("totalLikes".into(), d["totalLikeCount"].clone());
            STREAM.with_borrow_mut(|st| st.likes = st.likes.max(d["totalLikeCount"].as_u64().unwrap_or(0)));
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
            msg_id: s("msgId"),
            user,
            user_id: s("userId"),
            text: s("comment"),
            emotes: tiktok_emotes(d),
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

    #[test]
    fn tiktok_badges_and_colors_from_platform_payload() {
        let d = json!({
            "uniqueId": "mod_user",
            "nickname": "Mod",
            "isModerator": true,
            "isSubscriber": true,
            "badgeUrls": ["https://p16-webcast.tiktokcdn.com/sub_lv1.png"],
        });
        let (badges, color) = tiktok_badges_and_color("streamer", &d);
        assert_eq!(color, "#25F4EE");
        assert_eq!(badges[0].0, "tiktok:moderator");
        assert_eq!(
            badges[1],
            (
                "tiktok:badge".into(),
                "https://p16-webcast.tiktokcdn.com/sub_lv1.png".into()
            )
        );
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

    /// Combo gifts count once, when they end; the top gifters and stream info come from that.
    #[test]
    fn tallies_gifts_and_viewers() {
        let cfg = Arc::new(watch::channel(Config::default()).0);
        let engine = Engine::new(cfg, |_| {});
        let gift = |id: &str, n: u64, end: bool| json!({ "userId": id, "nickname": id, "giftType": 1, "diamondCount": 5, "repeatCount": n, "repeatEnd": end });
        for (id, n, end) in [("a", 1, false), ("a", 3, true), ("b", 2, true), ("b", 1, true)] {
            handle_tiktok_event(&engine, "me", "gift", &gift(id, n, end));
        }
        handle_tiktok_event(&engine, "me", "roomUser", &json!({ "viewerCount": 42 }));
        let top = top_gifters(5);
        assert_eq!(
            (top["topGifter0"].clone(), top["topGifter0Coins"].clone()),
            (json!("a"), json!(15))
        );
        assert_eq!(top["topGifter1Coins"], 15);
        let info = stream_info(true);
        assert_eq!(
            (info["totalCoins"].clone(), info["viewerCount"].clone()),
            (json!(30), json!(42))
        );
    }
}
