//! TikTok Live, read natively (see webcast.rs): chat, gifts, likes, follows, shares, subscriptions
//! and joins fire the TikTok triggers and chat commands. The browser extension handles account access.

use crate::commands::Chat;
use crate::engine::chatlog::{local_hhmm, normalize_line_emotes, Line};
use crate::engine::Engine;
use crate::model::Obj;
use crate::webcast::RoomLookupError;
use crate::Config;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

pub mod gifts;

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
    paused: bool,
    pin_id: String,
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
        a.insert("isPaused".into(), (live && s.paused).into());
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
#[serde(default, rename_all = "camelCase", from = "StoredTikTokConfig")]
pub struct TikTokConfig {
    pub channel: String,
    pub manual_connect: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bridge_token: String,
}

/// Old logins are read only to preserve the chosen channel, never their credentials.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct StoredTikTokConfig {
    channel: String,
    manual_connect: bool,
    bridge_token: String,
    broadcaster: Option<LegacyAccount>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct LegacyAccount {
    username: String,
}
impl From<StoredTikTokConfig> for TikTokConfig {
    fn from(old: StoredTikTokConfig) -> Self {
        let channel = if old.channel.trim().is_empty() {
            old.broadcaster
                .map(|a| clean_tiktok_channel(&a.username))
                .unwrap_or_default()
        } else {
            old.channel
        };
        Self {
            channel,
            manual_connect: old.manual_connect,
            bridge_token: old.bridge_token,
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
    s.trim_start_matches('@')
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
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
    RefreshGifts,
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
        crate::host::changed();
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

enum LiveEnd {
    Standby { never_live: bool },
    NotFound,
    Error(String, u64),
}

impl Runner {
    async fn run(mut self) {
        if !self.cfg.borrow().tiktok.manual_connect && !self.cfg.borrow().tiktok.channel.trim().is_empty() {
            self.connect();
        }
        let mut queued = None;
        while let Some(cmd) = match queued.take() {
            Some(cmd) => Some(cmd),
            None => self.rx.recv().await,
        } {
            match cmd {
                TikTokCmd::RefreshGifts => {
                    queued = refresh_command(gifts::refresh(&self.engine), &mut self.rx).await;
                    if queued.is_some() {
                        gifts::invalidate();
                    }
                }
                TikTokCmd::Connect => self.connect(),
                TikTokCmd::Disconnect => {
                    if let Some(task) = self.read_task.take() {
                        task.abort();
                        if self.shared.connected.load(Ordering::Relaxed) {
                            disconnected(&self.engine, &self.cfg.borrow().tiktok.channel);
                        }
                    }
                    self.engine.chat_polls_clear("tiktok");
                    self.engine.chat_unpinned("tiktok", "");
                    crate::host::polls_changed();
                    self.shared.set(TikTokStatus::Disconnected);
                    crate::say("TikTok: disconnected".into());
                }
            }
        }
    }

    fn connect(&mut self) {
        gifts::invalidate();
        self.shared.connected.store(false, Ordering::Relaxed);
        if let Ok(mut room) = self.shared.room_id.lock() {
            *room = None;
        }
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
                    let known = engine.tiktok_bridge.user().eq_ignore_ascii_case(&ch);
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
                            crate::warn(format!("TikTok: {msg}"));
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
                            crate::warn(format!("TikTok (@{channel}): {e}"));
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
                        // Only worth a line when it changes what happens (not while connected).
                        if !shared.connected.load(Ordering::Relaxed) {
                            crate::say(format!("TikTok: stream started, checking @{channel}"));
                        }
                    }
                }
            }
        }));
    }
}

/// A catalog read must not delay Connect or Disconnect. Repeated refresh clicks share the read.
async fn refresh_command(
    refresh: impl std::future::Future<Output = Result<Value, String>>,
    commands: &mut mpsc::UnboundedReceiver<TikTokCmd>,
) -> Option<TikTokCmd> {
    tokio::pin!(refresh);
    loop {
        tokio::select! {
            _ = &mut refresh => return None,
            command = commands.recv() => match command {
                Some(TikTokCmd::RefreshGifts) => {},
                command => return command,
            },
        }
    }
}

/// One connection to the channel's live, until it drops or the stream ends.
async fn live(
    engine: &Rc<Engine>,
    _cfg: &watch::Sender<Config>,
    channel: &str,
    known_account: bool,
    shared: &Shared,
) -> Result<(), LiveEnd> {
    if !matches!(shared.status.lock().map(|s| s.clone()), Ok(TikTokStatus::Standby(_))) {
        shared.set(TikTokStatus::Connecting(format!("checking @{channel}'s live...")));
    }
    let session = engine.tiktok_bridge.session().await;
    let ttwid = match browser_ttwid(&session) {
        Some(cookie) => cookie.to_string(),
        None => crate::webcast::ttwid().await.map_err(|e| LiveEnd::Error(e, 15))?,
    };
    let lookup = crate::webcast::room_id(channel, &ttwid, known_account).await;
    // Public lookup can be challenged while the browser's signed, logged-in request still works.
    let lookup = if matches!(lookup, Err(RoomLookupError::Transient(_))) && engine.tiktok_bridge.ready() {
        crate::tiktok_bridge::room_id(engine, channel).await
    } else {
        lookup
    };
    let room = match lookup {
        Ok(r) => r,
        Err(RoomLookupError::NeverLive) => return Err(LiveEnd::Standby { never_live: true }),
        Err(RoomLookupError::Offline) => return Err(LiveEnd::Standby { never_live: false }),
        Err(RoomLookupError::NotFound) => return Err(LiveEnd::NotFound),
        Err(RoomLookupError::Transient(e)) => return Err(LiveEnd::Error(e, 30)),
    };
    if let Ok(mut r) = shared.room_id.lock() {
        *r = Some(room.to_string());
    }
    gifts::invalidate();
    let cookie = if browser_ttwid(&session).is_some() {
        session
    } else {
        let mut cookie = format!("ttwid={ttwid}");
        if !session.is_empty() {
            cookie.push_str("; ");
            cookie.push_str(&session);
        }
        cookie
    };
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
    engine.settle_role("tiktok");
    engine.chat_stream("tiktok", true);
    let mut args = Obj::new();
    args.insert("channel".into(), channel.into());
    args.insert("roomId".into(), room.to_string().into());
    // A room not seen before is a new live: its tallies start over, and Stream Online fires.
    let new_live = STREAM.with_borrow_mut(|s| {
        s.paused = false;
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
    let gift_request = gifts::refresh(engine);
    tokio::pin!(gift_request);
    let mut gifts_done = false;
    let joined_inst = std::time::Instant::now();
    loop {
        tokio::select! {
            _ = &mut gift_request, if !gifts_done => { gifts_done = true; }
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
                            if !engine.chat_stream_ended("tiktok") {
                                crate::notify::send(crate::notify::Kind::Streams, "Streamerz", &crate::i18n::trf("chat.state_ended_title", &[("platform", "TikTok")]));
                            }
                            engine.chat_stream("tiktok", false);
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

/// Reuse the browser's existing device cookie instead of downloading an anonymous page each lookup.
fn browser_ttwid(session: &str) -> Option<&str> {
    session
        .split(';')
        .find_map(|part| part.trim().strip_prefix("ttwid=").filter(|s| !s.is_empty()))
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

pub(crate) fn tiktok_emotes(d: &Value) -> Vec<(String, String)> {
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
        ts: ts as i64 * 1000,
        user,
        user_id: s("userId"),
        handle: d["uniqueId"].as_str().filter(|s| !s.is_empty()).map(Box::<str>::from),
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
    gifts::invalidate();
    engine.chat_unpinned("tiktok", "");
    STREAM.with_borrow_mut(|s| {
        s.paused = false;
        s.pin_id.clear();
    });
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
    if presentation_event(engine, channel, event, d) {
        return;
    }
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
        engine.set_chat_viewers("tiktok", d["viewerCount"].as_u64());
        if STREAM.with_borrow_mut(|s| std::mem::replace(&mut s.viewers, n)) == n {
            return;
        }
    }
    let s = |k: &str| d[k].as_str().unwrap_or("").to_string();
    let user = Some(s("nickname"))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| s("uniqueId"));
    let mut args: Obj = d.as_object().cloned().unwrap_or_default().into_iter().collect();
    if event == "gift" {
        gifts::enrich(&mut args);
    }
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
            gifts::observe(d);
            let coins = args
                .get("diamondCount")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .saturating_mul(d["repeatCount"].as_u64().unwrap_or(1).max(1));
            args.insert("coins".into(), coins.into());
            // A combo gift (type 1) repeats with a growing count while it lasts: counted when it ends.
            let done =
                args.get("giftType").and_then(Value::as_u64) != Some(1) || d["repeatEnd"].as_bool().unwrap_or(true);
            let total = STREAM.with_borrow_mut(|st| {
                let g = st.gifters.entry(s("userId")).or_insert_with(|| (user.clone(), 0));
                if done {
                    g.1 = g.1.saturating_add(coins);
                    st.coins = st.coins.saturating_add(coins);
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
    engine.fire(kind, name, "TikTok", "tiktok", &args, |t| {
        event != "gift" || gifts::matches_args(t, &args)
    });
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

/// Updates existing chat presentation without inventing Streamer.bot trigger IDs.
fn presentation_event(engine: &Rc<Engine>, channel: &str, event: &str, d: &Value) -> bool {
    let emitted = match event {
        "messageDelete" => {
            for id in d["deleteMsgIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                engine.chat_remove("tiktok", id, "");
                engine.chat_unpinned("tiktok", id);
            }
            for id in d["deleteUserIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                engine.chat_remove("tiktok", "", id);
            }
            "MessageDeleted"
        }
        "roomPin" => {
            let chat = &d["chatMessage"];
            let msg_id = chat["msgId"].as_str().filter(|id| *id != "0").unwrap_or_default();
            match d["action"].as_u64() {
                Some(1) if !msg_id.is_empty() && chat["comment"].as_str().is_some_and(|s| !s.is_empty()) => {
                    STREAM.with_borrow_mut(|s| s.pin_id = d["pinMsgId"].as_str().unwrap_or_default().into());
                    let mut text = chat["comment"].as_str().unwrap_or_default().to_string();
                    for (token, _) in tiktok_emotes(chat) {
                        text = text.replace(&token, crate::i18n::tr("tip.tiktok_emote"));
                    }
                    engine.chat_pinned(crate::engine::chatlog::Pin {
                        platform: "tiktok".into(),
                        msg_id: msg_id.into(),
                        user: chat["nickname"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .or_else(|| chat["uniqueId"].as_str())
                            .unwrap_or_default()
                            .into(),
                        text,
                        by: d["operator"]["nickname"].as_str().unwrap_or_default().into(),
                        // Expiry units are not established by the schema. Follow explicit cancellation.
                        until: None,
                    });
                }
                Some(2) => {
                    let id = d["pinMsgId"].as_str().filter(|id| !id.is_empty() && *id != "0");
                    if STREAM.with_borrow(|s| id.is_some_and(|id| !s.pin_id.is_empty() && id != s.pin_id)) {
                        return true;
                    }
                    engine.chat_unpinned("tiktok", msg_id);
                    STREAM.with_borrow_mut(|s| s.pin_id.clear());
                }
                _ => return true,
            }
            "MessagePinned"
        }
        "streamPaused" | "streamResumed" => {
            let paused = event == "streamPaused";
            if STREAM.with_borrow_mut(|s| std::mem::replace(&mut s.paused, paused)) == paused {
                return true;
            }
            engine.chat_note(
                "tiktok",
                crate::i18n::tr(if paused {
                    "chat.tiktok_paused"
                } else {
                    "chat.tiktok_resumed"
                })
                .into(),
            );
            if paused {
                "StreamPaused"
            } else {
                "StreamResumed"
            }
        }
        _ => return false,
    };
    engine.emit("TikTok", emitted, || {
        let mut data = d.clone();
        data["channel"] = channel.into();
        data
    });
    true
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn disconnect_interrupts_a_slow_catalog_read_and_refresh_clicks_are_coalesced() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (send, mut commands) = mpsc::unbounded_channel();
                send.send(TikTokCmd::RefreshGifts).unwrap();
                send.send(TikTokCmd::Disconnect).unwrap();
                let next = tokio::time::timeout(
                    Duration::from_secs(1),
                    refresh_command(std::future::pending(), &mut commands),
                )
                .await
                .unwrap();
                assert!(matches!(next, Some(TikTokCmd::Disconnect)));
            });
    }

    #[test]
    fn reconnect_clears_the_old_room_before_looking_up_the_new_channel() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let shared = Shared::default();
            shared.connected.store(true, Ordering::Relaxed);
            *shared.room_id.lock().unwrap() = Some("old room".into());
            let mut runner = Runner {
                engine: crate::engine::tests::engine(json!({"actions":[]})),
                cfg: Arc::new(watch::channel(Config::default()).0),
                rx: mpsc::unbounded_channel().1,
                shared: shared.clone(),
                read_task: None,
            };
            runner.connect();
            assert!(!shared.connected.load(Ordering::Relaxed));
            assert!(shared.room_id.lock().unwrap().is_none());
            runner.read_task.take().unwrap().abort();
        });
    }

    #[test]
    fn old_accounts_preserve_the_channel_but_not_their_credentials() {
        let old = json!({"broadcaster": {"username": "@creator", "sessionCookie": "old"},
            "bot": {"username": "old_bot", "sessionCookie": "old"}, "bridgeToken": "pairing"});
        let config: TikTokConfig = serde_json::from_value(old).unwrap();
        assert_eq!(config.channel, "creator");
        assert_eq!(config.bridge_token, "pairing");
        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved.get("broadcaster").is_none() && saved.get("bot").is_none());
        let chosen: TikTokConfig =
            serde_json::from_value(json!({"channel": "other", "broadcaster": {"username": "creator"}})).unwrap();
        assert_eq!(chosen.channel, "other");
    }

    #[test]
    fn test_clean_tiktok_channel() {
        assert_eq!(clean_tiktok_channel("https://www.tiktok.com/@streamer"), "streamer");
        assert_eq!(clean_tiktok_channel("http://tiktok.com/@cooluser/"), "cooluser");
        assert_eq!(clean_tiktok_channel("@awesome_creator"), "awesome_creator");
        assert_eq!(clean_tiktok_channel("just_name"), "just_name");
        assert_eq!(clean_tiktok_channel("  @spaced_name/  "), "spaced_name");
        for suffix in ["/live", "/live?lang=en", "/video/123#comments", "?lang=en", "/?share=1"] {
            assert_eq!(
                clean_tiktok_channel(&format!("https://www.tiktok.com/@creator{suffix}")),
                "creator"
            );
        }
    }

    #[test]
    fn browser_device_cookie_is_reused_without_accepting_similar_names() {
        assert_eq!(
            browser_ttwid("sessionid=s; ttwid=device%7Ctoken; uid_tt=u"),
            Some("device%7Ctoken")
        );
        assert_eq!(browser_ttwid("ttwid=; sessionid=s"), None);
        assert_eq!(browser_ttwid("other_ttwid=no; notttwid=no"), None);
        assert_eq!(browser_ttwid(""), None);
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

    #[test]
    fn deleted_messages_and_user_history_remove_their_pins() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            e.dev_mode();
            e.change(|c| c.window.chat.save_history = true);
            let chat =
                json!({"msgId": "100", "userId": "42", "uniqueId": "creator", "nickname": "Creator", "comment": ":)"});
            handle_tiktok_event(&e, "fixture", "chat", &chat);
            handle_tiktok_event(
                &e,
                "fixture",
                "roomPin",
                &json!({"action": 1, "pinMsgId": "200", "chatMessage": chat}),
            );
            assert_eq!(e.chat_pins()[0].text, ":)");
            // A cancellation for the previous pin cannot remove the newer one.
            handle_tiktok_event(&e, "fixture", "roomPin", &json!({"action": 2, "pinMsgId": "199"}));
            assert_eq!(e.chat_pins().len(), 1);
            handle_tiktok_event(&e, "fixture", "messageDelete", &json!({"deleteMsgIds": ["100"]}));
            assert!(e.chat_pins().is_empty());
            assert!(
                e.chat_sync(999, 0)
                    .2
                    .iter()
                    .find(|l| l.msg_id == "100")
                    .unwrap()
                    .deleted
            );
            let mut next = chat.clone();
            next["msgId"] = json!("101");
            handle_tiktok_event(&e, "fixture", "chat", &next);
            handle_tiktok_event(
                &e,
                "fixture",
                "roomPin",
                &json!({"action": 1, "pinMsgId": "201", "chatMessage": next}),
            );
            handle_tiktok_event(&e, "fixture", "messageDelete", &json!({"deleteUserIds": ["42"]}));
            assert!(e.chat_pins().is_empty());
            assert!(e
                .chat_sync(999, 0)
                .2
                .iter()
                .filter(|l| l.user_id == "42")
                .all(|l| l.deleted));
        });
    }

    #[test]
    fn paused_lives_keep_the_connection_and_emit_only_changes() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            e.dev_mode();
            e.change(|c| c.window.chat.save_history = true);
            e.simulate_chat_connection("tiktok", Some(true));
            STREAM.with_borrow_mut(|s| *s = Stream::default());
            let (tx, mut rx) = mpsc::unbounded_channel();
            e.listen(
                900,
                &std::collections::HashMap::from([("TikTok".into(), std::collections::HashSet::from(["*".into()]))]),
                &tx,
            );
            handle_tiktok_event(&e, "fixture", "streamPaused", &json!({}));
            handle_tiktok_event(&e, "fixture", "streamPaused", &json!({}));
            assert!(e.chat_connected("tiktok"));
            assert_eq!(stream_info(true)["isPaused"], true);
            assert_eq!(stream_info(false)["isPaused"], false);
            assert!(rx.try_recv().unwrap().contains("StreamPaused"));
            assert!(rx.try_recv().is_err());
            handle_tiktok_event(&e, "fixture", "streamResumed", &json!({}));
            assert_eq!(stream_info(true)["isPaused"], false);
            assert!(rx.try_recv().unwrap().contains("StreamResumed"));
            assert_eq!(e.chat_sync(999, 0).2.iter().filter(|l| l.note).count(), 2);
        });
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
