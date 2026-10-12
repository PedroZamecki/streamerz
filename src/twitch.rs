//! Twitch integration for streamerz (Phase 6):
//! - Login via Device Code OAuth (broadcaster + optional bot account).
//! - EventSub WebSocket session (`wss://eventsub.wss.twitch.tv/ws`).
//! - Chat messages in and out, whispers, announcements, raids.
//! - Triggers: Follow, Cheer, Sub, Resub, Gift Sub, Gift Bomb, Raid, Chat Message,
//!   Whisper, Bot Whisper, Announcement, First Words, Stream Online, Stream Offline,
//!   and connection lifecycle triggers.
//! - Sub-actions: Send Message (10), Send Announcement (23), Start Raid (500),
//!   Cancel Raid (501), Reply to Message (560).

use crate::commands::{self, Chat};
use crate::engine::Engine;
use crate::model::Obj;
use crate::Config;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, watch};

/// Public Client ID from the registered Twitch Developer application.
pub const CLIENT_ID: &str = "6id5pmouq63yr29ee6pb8fcqufelfb";
pub const DEFAULT_EVENTSUB_URL: &str = "wss://eventsub.wss.twitch.tv/ws";
pub const HELIX_URL: &str = "https://api.twitch.tv/helix";
pub const OAUTH_DEVICE_URL: &str = "https://id.twitch.tv/oauth2/device";
pub const OAUTH_TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";

pub const BROADCASTER_SCOPES: &[&str] = &[
    "user:read:chat",
    "user:write:chat",
    "user:bot",
    "channel:bot",
    "channel:read:subscriptions",
    "bits:read",
    "moderator:read:followers",
    "channel:manage:raids",
    "channel:read:raids",
    "moderator:manage:announcements",
    "user:read:whispers",
    "user:manage:whispers",
];

pub const BOT_SCOPES: &[&str] = &[
    "user:read:chat",
    "user:write:chat",
    "user:bot",
    "user:read:whispers",
    "user:manage:whispers",
];

// Trigger codes from src/types.rs TRIGGERS
pub const T_FOLLOW: u32 = 101;
pub const T_CHEER: u32 = 102;
pub const T_SUB: u32 = 103;
pub const T_RESUB: u32 = 104;
pub const T_GIFT_SUB: u32 = 105;
pub const T_GIFT_BOMB: u32 = 106;
pub const T_RAID: u32 = 107;
pub const T_WHISPER: u32 = 119;
pub const T_FIRST_WORDS: u32 = 120;
pub const T_CHAT_MESSAGE: u32 = 133;
pub const T_ANNOUNCEMENT: u32 = 138;
pub const T_BOT_WHISPER: u32 = 140;
pub const T_STREAM_ONLINE: u32 = 154;
pub const T_STREAM_OFFLINE: u32 = 155;
pub const T_RAID_START: u32 = 158;
pub const T_RAID_SEND: u32 = 159;
pub const T_RAID_CANCELLED: u32 = 160;
pub const T_AUTHED: u32 = 173;
pub const T_CHAT_CONNECTED: u32 = 174;
pub const T_CHAT_DISCONNECTED: u32 = 175;
pub const T_EVENTSUB_CONNECTED: u32 = 178;
pub const T_EVENTSUB_DISCONNECTED: u32 = 179;
pub const T_BOT_EVENTSUB_CONNECTED: u32 = 184;
pub const T_BOT_EVENTSUB_DISCONNECTED: u32 = 185;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TwitchConfig {
    pub auto_connect: bool,
    pub broadcaster: Option<TwitchAccount>,
    pub bot: Option<TwitchAccount>,
    pub first_words_auto_reset: bool,
    pub ignore_bot_internal: bool,
    /// Overrides the EventSub WebSocket URL (for testing or proxying).
    pub eventsub_url: Option<String>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TwitchAccount {
    pub user_id: String,
    pub login: String,
    pub display_name: String,
    pub token: TwitchToken,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TwitchToken {
    pub access_token: String,
    pub refresh_token: String,
    pub scope: Vec<String>,
    pub expires_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceCodeResp {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenResp {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub scope: Vec<String>,
    pub token_type: String,
}

/// Commands sent to the Twitch background runner.
pub enum TwitchCmd {
    SendMessage {
        text: String,
        reply_id: Option<String>,
        use_bot: bool,
        fallback: bool,
        resp: oneshot::Sender<Result<(), String>>,
    },
    SendAnnouncement {
        text: String,
        color: String,
        use_bot: bool,
        fallback: bool,
        resp: oneshot::Sender<Result<(), String>>,
    },
    StartRaid {
        target_login: String,
        resp: oneshot::Sender<Result<bool, String>>,
    },
    CancelRaid {
        resp: oneshot::Sender<Result<(), String>>,
    },
    ClearFirstWords,
    Connect,
    Disconnect,
    GetStatus(oneshot::Sender<TwitchStatus>),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TwitchStatus {
    pub connected: bool,
    pub broadcaster: Option<String>,
    pub bot: Option<String>,
    pub session_id: Option<String>,
}

/// A cloneable handle to communicate with the Twitch task.
#[derive(Clone)]
pub struct TwitchHandle {
    tx: mpsc::UnboundedSender<TwitchCmd>,
    connected: Arc<AtomicBool>,
}

impl TwitchHandle {
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn send(&self, cmd: TwitchCmd) {
        let _ = self.tx.send(cmd);
    }

    pub async fn send_message(
        &self,
        text: String,
        reply_id: Option<String>,
        use_bot: bool,
        fallback: bool,
    ) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send(TwitchCmd::SendMessage {
            text,
            reply_id,
            use_bot,
            fallback,
            resp: resp_tx,
        });
        resp_rx.await.map_err(|_| String::from("Twitch task stopped"))?
    }

    pub async fn send_announcement(
        &self,
        text: String,
        color: String,
        use_bot: bool,
        fallback: bool,
    ) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send(TwitchCmd::SendAnnouncement {
            text,
            color,
            use_bot,
            fallback,
            resp: resp_tx,
        });
        resp_rx.await.map_err(|_| String::from("Twitch task stopped"))?
    }

    pub async fn start_raid(&self, target_login: String) -> Result<bool, String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send(TwitchCmd::StartRaid {
            target_login,
            resp: resp_tx,
        });
        resp_rx.await.map_err(|_| String::from("Twitch task stopped"))?
    }

    pub async fn cancel_raid(&self) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send(TwitchCmd::CancelRaid { resp: resp_tx });
        resp_rx.await.map_err(|_| String::from("Twitch task stopped"))?
    }

    pub fn clear_first_words(&self) {
        self.send(TwitchCmd::ClearFirstWords);
    }

    pub async fn status(&self) -> TwitchStatus {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send(TwitchCmd::GetStatus(resp_tx));
        resp_rx.await.unwrap_or_default()
    }
}

/// Start Device Code OAuth flow.
pub async fn device_flow_start(client_id: &str, scopes: &[&str]) -> Result<DeviceCodeResp, String> {
    let scope_str = scopes.join(" ");
    let body = format!(
        "client_id={}&scopes={}",
        commands::url_encode(client_id),
        commands::url_encode(&scope_str)
    );
    let headers = vec![("Content-Type".into(), "application/x-www-form-urlencoded".into())];
    let res = crate::http::request("POST", OAUTH_DEVICE_URL, &headers, body.as_bytes()).await?;
    if res.status != 200 {
        return Err(format!("Device flow start failed: HTTP {} {}", res.status, res.text()));
    }
    serde_json::from_slice(&res.body).map_err(|e| format!("Failed to parse device response: {e}"))
}

/// Poll Device Code OAuth token endpoint. Returns Ok(None) while authorization is pending.
pub async fn device_flow_poll(client_id: &str, device_code: &str) -> Result<Option<TokenResp>, String> {
    let body = format!(
        "client_id={}&device_code={}&grant_type=urn:ietf:params:oauth:grant-type:device_code",
        commands::url_encode(client_id),
        commands::url_encode(device_code)
    );
    let headers = vec![("Content-Type".into(), "application/x-www-form-urlencoded".into())];
    let res = crate::http::request("POST", OAUTH_TOKEN_URL, &headers, body.as_bytes()).await?;
    if res.status == 200 {
        let token: TokenResp = serde_json::from_slice(&res.body).map_err(|e| format!("Bad token response: {e}"))?;
        Ok(Some(token))
    } else {
        let text = res.text();
        if text.contains("authorization_pending") {
            Ok(None)
        } else {
            Err(format!("Token poll failed: HTTP {} {}", res.status, text))
        }
    }
}

/// Refresh an OAuth token.
pub async fn refresh_token(client_id: &str, refresh_token: &str) -> Result<TokenResp, String> {
    let body = format!(
        "client_id={}&grant_type=refresh_token&refresh_token={}",
        commands::url_encode(client_id),
        commands::url_encode(refresh_token)
    );
    let headers = vec![("Content-Type".into(), "application/x-www-form-urlencoded".into())];
    let res = crate::http::request("POST", OAUTH_TOKEN_URL, &headers, body.as_bytes()).await?;
    if res.status != 200 {
        return Err(format!("Refresh token failed: HTTP {} {}", res.status, res.text()));
    }
    serde_json::from_slice(&res.body).map_err(|e| format!("Bad refresh response: {e}"))
}

/// Fetch user details from Helix `/users`.
pub async fn fetch_user(client_id: &str, access_token: &str, login: Option<&str>) -> Result<TwitchAccount, String> {
    let url = match login {
        Some(l) => format!("{HELIX_URL}/users?login={}", commands::url_encode(l)),
        None => format!("{HELIX_URL}/users"),
    };
    let headers = vec![
        ("Client-Id".into(), client_id.into()),
        ("Authorization".into(), format!("Bearer {access_token}")),
    ];
    let res = crate::http::request("GET", &url, &headers, b"").await?;
    if res.status != 200 {
        return Err(format!("Helix users failed: HTTP {} {}", res.status, res.text()));
    }
    let val: Value = serde_json::from_slice(&res.body).map_err(|e| format!("Bad users JSON: {e}"))?;
    let data = val.get("data").and_then(Value::as_array).ok_or("No user data array")?;
    let user = data.first().ok_or("No user found")?;
    Ok(TwitchAccount {
        user_id: user.get("id").and_then(Value::as_str).unwrap_or("").into(),
        login: user.get("login").and_then(Value::as_str).unwrap_or("").into(),
        display_name: user.get("display_name").and_then(Value::as_str).unwrap_or("").into(),
        token: TwitchToken {
            access_token: access_token.into(),
            ..Default::default()
        },
    })
}

/// Helper to parse hex colors like `#e5e5e5` into RGB.
pub fn parse_hex_color(hex: &str) -> (i64, i64, i64) {
    let s = hex.trim_start_matches('#');
    if s.len() >= 6 {
        let r = i64::from_str_radix(&s[0..2], 16).unwrap_or(255);
        let g = i64::from_str_radix(&s[2..4], 16).unwrap_or(255);
        let b = i64::from_str_radix(&s[4..6], 16).unwrap_or(255);
        (r, g, b)
    } else {
        (255, 255, 255)
    }
}

pub fn populate_broadcaster_vars(args: &mut Obj, broadcaster: &TwitchAccount) {
    args.insert("broadcastUser".into(), broadcaster.display_name.as_str().into());
    args.insert("broadcastUserName".into(), broadcaster.login.as_str().into());
    args.insert("broadcastUserId".into(), broadcaster.user_id.as_str().into());
    args.insert("broadcastIsAffiliate".into(), false.into());
    args.insert("broadcastIsPartner".into(), false.into());
}

pub fn populate_user_vars(args: &mut Obj, user_id: &str, login: &str, display: &str, role: u8, sub: bool) {
    args.insert("user".into(), display.into());
    args.insert("userName".into(), login.into());
    args.insert("userId".into(), user_id.into());
    args.insert("userType".into(), "twitch".into());
    args.insert("isSubscribed".into(), sub.into());
    args.insert("isModerator".into(), (role == 3 || role == 4).into());
    args.insert("isVip".into(), (role == 2).into());
    args.insert("badges".into(), Value::Array(vec![]));
    args.insert("badgeCount".into(), 0.into());
}

#[derive(Default)]
pub struct ReplyInfo {
    pub msg_id: String,
    pub user_id: String,
    pub user_login: String,
    pub user_name: String,
    pub msg_body: String,
}

#[derive(Default)]
pub struct ChatContext<'a> {
    pub text: &'a str,
    pub msg_id: &'a str,
    pub bits: u64,
    pub role: u8,
    pub color: &'a str,
    pub reply: Option<&'a ReplyInfo>,
    pub internal: bool,
}

pub fn populate_chat_vars(args: &mut Obj, ctx: &ChatContext) {
    let text = ctx.text;
    args.insert("message".into(), text.into());
    args.insert("messageStripped".into(), text.into());
    args.insert("messageCheermotesStripped".into(), text.into());
    args.insert("msgId".into(), ctx.msg_id.into());
    args.insert("bits".into(), ctx.bits.into());
    args.insert("role".into(), (ctx.role as i64).into());
    let color_str = if ctx.color.is_empty() { "#FFFFFF" } else { ctx.color };
    args.insert("color".into(), color_str.into());
    let (r, g, b) = parse_hex_color(color_str);
    args.insert("colorR".into(), r.into());
    args.insert("colorG".into(), g.into());
    args.insert("colorB".into(), b.into());
    args.insert("emoteCount".into(), 0.into());
    args.insert("emotes".into(), Value::Array(vec![]));
    args.insert("isHighlight".into(), false.into());
    args.insert("isAction".into(), false.into());
    args.insert("internal".into(), ctx.internal.into());
    commands::input_args(text, text, args);
    if let Some(rep) = ctx.reply {
        args.insert("isReply".into(), true.into());
        args.insert("reply.msgId".into(), rep.msg_id.as_str().into());
        args.insert("reply.userId".into(), rep.user_id.as_str().into());
        args.insert("reply.userLogin".into(), rep.user_login.as_str().into());
        args.insert("reply.userName".into(), rep.user_name.as_str().into());
        args.insert("reply.msgBody".into(), rep.msg_body.replace(' ', r"\s").into());
        args.insert("reply.threadMsgId".into(), rep.msg_id.as_str().into());
        args.insert("reply.threadUserLogin".into(), rep.user_login.as_str().into());
    } else {
        args.insert("isReply".into(), false.into());
    }
}

/// Helper to parse role and subscriber status from Twitch badges array.
pub fn parse_badges(badges: &[Value], is_broadcaster: bool) -> (u8, bool) {
    if is_broadcaster {
        return (4, false);
    }
    let mut role = 1;
    let mut sub = false;
    for b in badges {
        let set_id = b.get("set_id").and_then(Value::as_str).unwrap_or("");
        match set_id {
            "broadcaster" => role = 4,
            "moderator" if role < 3 => role = 3,
            "vip" if role < 2 => role = 2,
            "subscriber" | "founder" => sub = true,
            _ => {}
        }
    }
    (role, sub)
}

/// Start the Twitch runner and return the communication handle.
pub fn start(engine: Rc<Engine>, cfg: &Arc<watch::Sender<Config>>) -> TwitchHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let is_connected = Arc::new(AtomicBool::new(false));
    let handle = TwitchHandle {
        tx,
        connected: is_connected.clone(),
    };
    let cfg_rx = cfg.subscribe();
    let runner = Runner {
        engine,
        cfg: cfg.clone(),
        cfg_rx,
        rx,
        speakers: HashSet::new(),
        session_id: None,
        connected: false,
        is_connected,
    };
    tokio::task::spawn_local(runner.run());
    handle
}

struct Runner {
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    cfg_rx: watch::Receiver<Config>,
    rx: mpsc::UnboundedReceiver<TwitchCmd>,
    speakers: HashSet<String>,
    session_id: Option<String>,
    connected: bool,
    is_connected: Arc<AtomicBool>,
}

impl Runner {
    async fn run(mut self) {
        // Auto connect if configured
        if self.cfg.borrow().twitch.auto_connect && self.cfg.borrow().twitch.broadcaster.is_some() {
            crate::say("Twitch auto-connecting...".into());
            self.connect_eventsub().await;
        }

        loop {
            tokio::select! {
                Some(cmd) = self.rx.recv() => {
                    self.handle_cmd(cmd).await;
                }
                Ok(_) = self.cfg_rx.changed() => {
                    // Check if config changed
                }
            }
        }
    }

    async fn handle_cmd(&mut self, cmd: TwitchCmd) {
        match cmd {
            TwitchCmd::Connect => {
                self.connect_eventsub().await;
            }
            TwitchCmd::Disconnect => {
                self.disconnect();
            }
            TwitchCmd::ClearFirstWords => {
                self.speakers.clear();
                crate::say("Twitch First Words cache cleared".into());
            }
            TwitchCmd::GetStatus(resp) => {
                let c = self.cfg.borrow();
                let status = TwitchStatus {
                    connected: self.connected,
                    broadcaster: c.twitch.broadcaster.as_ref().map(|b| b.login.clone()),
                    bot: c.twitch.bot.as_ref().map(|b| b.login.clone()),
                    session_id: self.session_id.clone(),
                };
                let _ = resp.send(status);
            }
            TwitchCmd::SendMessage {
                text,
                reply_id,
                use_bot,
                fallback,
                resp,
            } => {
                let res = self
                    .send_chat_message(&text, reply_id.as_deref(), use_bot, fallback)
                    .await;
                let _ = resp.send(res);
            }
            TwitchCmd::SendAnnouncement {
                text,
                color,
                use_bot,
                fallback,
                resp,
            } => {
                let res = self.send_announcement(&text, &color, use_bot, fallback).await;
                let _ = resp.send(res);
            }
            TwitchCmd::StartRaid { target_login, resp } => {
                let res = self.start_raid(&target_login).await;
                let _ = resp.send(res);
            }
            TwitchCmd::CancelRaid { resp } => {
                let res = self.cancel_raid().await;
                let _ = resp.send(res);
            }
        }
    }

    fn disconnect(&mut self) {
        if self.connected {
            self.connected = false;
            self.is_connected.store(false, Ordering::Relaxed);
            self.session_id = None;
            self.engine.fire(
                T_EVENTSUB_DISCONNECTED,
                "Broadcaster EventSub Disconnected",
                "Twitch/Connections",
                "twitch",
                &Obj::new(),
                |_| true,
            );
            self.engine.fire(
                T_CHAT_DISCONNECTED,
                "Broadcaster Chat Disconnected",
                "Twitch/Connections",
                "twitch",
                &Obj::new(),
                |_| true,
            );
            self.engine
                .emit("Twitch", "BroadcasterEventSubDisconnected", || json!({}));
            self.engine.emit("Twitch", "BroadcasterChatDisconnected", || json!({}));
            if self.cfg.borrow().twitch.bot.is_some() {
                self.engine.fire(
                    T_BOT_EVENTSUB_DISCONNECTED,
                    "Bot EventSub Disconnected",
                    "Twitch/Connections",
                    "twitch",
                    &Obj::new(),
                    |_| true,
                );
                self.engine.emit("Twitch", "BotChatDisconnected", || json!({}));
            }
            crate::say("Twitch EventSub disconnected".into());
            crate::ui::changed();
        }
    }

    async fn connect_eventsub(&mut self) {
        let (broadcaster, url) = {
            let c = self.cfg.borrow();
            let b = match &c.twitch.broadcaster {
                Some(b) => b.clone(),
                None => {
                    crate::say("Twitch connect: no broadcaster account configured".into());
                    return;
                }
            };
            let u = c
                .twitch
                .eventsub_url
                .clone()
                .unwrap_or_else(|| DEFAULT_EVENTSUB_URL.into());
            (b, u)
        };

        crate::say(format!("Connecting to Twitch EventSub at {url}..."));
        let ws = match tokio_tungstenite::connect_async(&url).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                crate::say(format!("Twitch EventSub connection failed: {e}"));
                return;
            }
        };

        self.connected = true;
        self.is_connected.store(true, Ordering::Relaxed);
        crate::say("Twitch EventSub connected, waiting for welcome...".into());
        crate::ui::changed();

        // Spawn read loop
        let me_engine = self.engine.clone();
        let me_cfg = self.cfg.clone();
        let me_is_conn = self.is_connected.clone();
        tokio::task::spawn_local(async move {
            run_eventsub_loop(ws, me_engine, me_cfg, broadcaster, me_is_conn).await;
        });
    }

    async fn send_chat_message(
        &self,
        text: &str,
        reply_id: Option<&str>,
        use_bot: bool,
        fallback: bool,
    ) -> Result<(), String> {
        let (token, broadcaster_id, sender_id) = self.pick_account(use_bot, fallback)?;
        let url = format!("{HELIX_URL}/chat/messages");
        let headers = vec![
            ("Client-Id".into(), CLIENT_ID.into()),
            ("Authorization".into(), format!("Bearer {token}")),
            ("Content-Type".into(), "application/json".into()),
        ];
        let mut body_map = json!({
            "broadcaster_id": broadcaster_id,
            "sender_id": sender_id,
            "message": text,
        });
        if let Some(r) = reply_id {
            if !r.is_empty() {
                body_map["reply_parent_message_id"] = r.into();
            }
        }
        let body = serde_json::to_vec(&body_map).unwrap();
        let res = crate::http::request("POST", &url, &headers, &body).await?;
        if res.status != 200 {
            return Err(format!("Send chat message failed: HTTP {} {}", res.status, res.text()));
        }
        Ok(())
    }

    async fn send_announcement(&self, text: &str, color: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        let (token, broadcaster_id, sender_id) = self.pick_account(use_bot, fallback)?;
        let color_norm = match color.to_ascii_lowercase().as_str() {
            "blue" => "blue",
            "green" => "green",
            "orange" => "orange",
            "purple" => "purple",
            _ => "primary",
        };
        let url = format!("{HELIX_URL}/chat/announcements?broadcaster_id={broadcaster_id}&moderator_id={sender_id}");
        let headers = vec![
            ("Client-Id".into(), CLIENT_ID.into()),
            ("Authorization".into(), format!("Bearer {token}")),
            ("Content-Type".into(), "application/json".into()),
        ];
        let body = serde_json::to_vec(&json!({
            "message": text,
            "color": color_norm,
        }))
        .unwrap();
        let res = crate::http::request("POST", &url, &headers, &body).await?;
        if res.status != 200 && res.status != 204 {
            return Err(format!("Send announcement failed: HTTP {} {}", res.status, res.text()));
        }
        Ok(())
    }

    async fn start_raid(&self, target_login: &str) -> Result<bool, String> {
        let c = self.cfg.borrow();
        let broadcaster = c.twitch.broadcaster.as_ref().ok_or("No broadcaster account")?;
        let token = &broadcaster.token.access_token;
        let from_id = &broadcaster.user_id;

        // Resolve target user
        let target = fetch_user(CLIENT_ID, token, Some(target_login)).await?;
        let to_id = &target.user_id;

        let url = format!("{HELIX_URL}/raids?from_broadcaster_id={from_id}&to_broadcaster_id={to_id}");
        let headers = vec![
            ("Client-Id".into(), CLIENT_ID.into()),
            ("Authorization".into(), format!("Bearer {token}")),
        ];
        let res = crate::http::request("POST", &url, &headers, b"").await?;
        if res.status == 200 || res.status == 204 {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            args.insert("raidUser".into(), target.display_name.into());
            args.insert("raidUserName".into(), target.login.into());
            args.insert("raidUserId".into(), target.user_id.into());
            self.engine
                .fire(T_RAID_START, "Raid Start", "Twitch/Raid", "twitch", &args, |_| true);
            self.engine.emit("Twitch", "RaidStart", || json!(args));
            Ok(true)
        } else {
            crate::say(format!("Start raid failed: HTTP {} {}", res.status, res.text()));
            Ok(false)
        }
    }

    async fn cancel_raid(&self) -> Result<(), String> {
        let c = self.cfg.borrow();
        let broadcaster = c.twitch.broadcaster.as_ref().ok_or("No broadcaster account")?;
        let token = &broadcaster.token.access_token;
        let from_id = &broadcaster.user_id;

        let url = format!("{HELIX_URL}/raids?broadcaster_id={from_id}");
        let headers = vec![
            ("Client-Id".into(), CLIENT_ID.into()),
            ("Authorization".into(), format!("Bearer {token}")),
        ];
        let res = crate::http::request("DELETE", &url, &headers, b"").await?;
        if res.status == 200 || res.status == 204 {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            self.engine.fire(
                T_RAID_CANCELLED,
                "Raid Cancelled",
                "Twitch/Raid",
                "twitch",
                &args,
                |_| true,
            );
            self.engine.emit("Twitch", "RaidCancelled", || json!(args));
            Ok(())
        } else {
            Err(format!("Cancel raid failed: HTTP {} {}", res.status, res.text()))
        }
    }

    fn pick_account(&self, use_bot: bool, fallback: bool) -> Result<(String, String, String), String> {
        let c = self.cfg.borrow();
        let broadcaster = c.twitch.broadcaster.as_ref().ok_or("No broadcaster account")?;
        if use_bot {
            if let Some(bot) = &c.twitch.bot {
                return Ok((
                    bot.token.access_token.clone(),
                    broadcaster.user_id.clone(),
                    bot.user_id.clone(),
                ));
            } else if fallback {
                return Ok((
                    broadcaster.token.access_token.clone(),
                    broadcaster.user_id.clone(),
                    broadcaster.user_id.clone(),
                ));
            } else {
                return Err("Bot account not configured and fallback disabled".into());
            }
        }
        Ok((
            broadcaster.token.access_token.clone(),
            broadcaster.user_id.clone(),
            broadcaster.user_id.clone(),
        ))
    }
}

async fn run_eventsub_loop(
    mut ws: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    broadcaster: TwitchAccount,
    is_connected: Arc<AtomicBool>,
) {
    use futures_util::StreamExt;
    let mut speakers = HashSet::new();

    while let Some(msg) = ws.next().await {
        let raw = match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(t)) => t,
            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
            Err(_) => break,
            _ => continue,
        };

        let val: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let mtype = val
            .pointer("/metadata/message_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        match mtype {
            "session_welcome" => {
                let sid = val.pointer("/payload/session/id").and_then(Value::as_str).unwrap_or("");
                crate::say(format!("Twitch EventSub session welcome: {sid}"));
                let mut args = Obj::new();
                populate_broadcaster_vars(&mut args, &broadcaster);
                engine.fire(
                    T_AUTHED,
                    "Broadcaster Authenticated",
                    "Twitch/Connections",
                    "twitch",
                    &args,
                    |_| true,
                );
                engine.fire(
                    T_EVENTSUB_CONNECTED,
                    "Broadcaster EventSub Connected",
                    "Twitch/Connections",
                    "twitch",
                    &args,
                    |_| true,
                );
                engine.fire(
                    T_CHAT_CONNECTED,
                    "Broadcaster Chat Connected",
                    "Twitch/Connections",
                    "twitch",
                    &args,
                    |_| true,
                );
                engine.emit("Twitch", "BroadcasterAuthenticated", || json!(args));
                engine.emit("Twitch", "BroadcasterEventSubConnected", || json!(args));
                engine.emit("Twitch", "BroadcasterChatConnected", || json!(args));

                if cfg.borrow().twitch.bot.is_some() {
                    engine.fire(
                        T_BOT_EVENTSUB_CONNECTED,
                        "Bot EventSub Connected",
                        "Twitch/Connections",
                        "twitch",
                        &args,
                        |_| true,
                    );
                    engine.emit("Twitch", "BotChatConnected", || json!(args));
                }

                // Create subscriptions via Helix
                let token = broadcaster.token.access_token.clone();
                let bid = broadcaster.user_id.clone();
                let sid = sid.to_string();
                let bot_id = cfg.borrow().twitch.bot.as_ref().map(|b| b.user_id.clone());
                tokio::task::spawn_local(async move {
                    create_all_subscriptions(&sid, &token, &bid, bot_id.as_deref()).await;
                });
            }
            "notification" => {
                let sub_type = val
                    .pointer("/metadata/subscription_type")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let event = val.pointer("/payload/event").cloned().unwrap_or_default();
                handle_notification(&engine, &broadcaster, &cfg, &mut speakers, sub_type, &event);
            }
            "session_reconnect" => {
                let url = val
                    .pointer("/payload/session/reconnect_url")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                crate::say(format!("Twitch EventSub reconnect request: {url}"));
                break;
            }
            "revocation" => {
                let sub_type = val
                    .pointer("/payload/subscription/type")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                crate::say(format!("Twitch subscription revoked: {sub_type}"));
            }
            _ => {}
        }
    }

    is_connected.store(false, Ordering::Relaxed);
    crate::say("Twitch EventSub connection dropped".into());
    let mut args = Obj::new();
    populate_broadcaster_vars(&mut args, &broadcaster);
    engine.fire(
        T_EVENTSUB_DISCONNECTED,
        "Broadcaster EventSub Disconnected",
        "Twitch/Connections",
        "twitch",
        &args,
        |_| true,
    );
    engine.fire(
        T_CHAT_DISCONNECTED,
        "Broadcaster Chat Disconnected",
        "Twitch/Connections",
        "twitch",
        &args,
        |_| true,
    );
    engine.emit("Twitch", "BroadcasterEventSubDisconnected", || json!(args));
    engine.emit("Twitch", "BroadcasterChatDisconnected", || json!(args));
    crate::ui::changed();
}

async fn create_all_subscriptions(sid: &str, token: &str, bid: &str, bot_id: Option<&str>) {
    let subs = [
        (
            "channel.chat.message",
            "1",
            json!({"broadcaster_user_id": bid, "user_id": bid}),
        ),
        (
            "channel.chat.notification",
            "1",
            json!({"broadcaster_user_id": bid, "user_id": bid}),
        ),
        (
            "channel.follow",
            "2",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        ("channel.subscribe", "1", json!({"broadcaster_user_id": bid})),
        ("channel.subscription.message", "1", json!({"broadcaster_user_id": bid})),
        ("channel.subscription.gift", "1", json!({"broadcaster_user_id": bid})),
        ("channel.cheer", "1", json!({"broadcaster_user_id": bid})),
        ("channel.raid", "1", json!({"to_broadcaster_user_id": bid})),
        ("stream.online", "1", json!({"broadcaster_user_id": bid})),
        ("stream.offline", "1", json!({"broadcaster_user_id": bid})),
        ("user.whisper.message", "1", json!({"user_id": bid})),
    ];

    for (stype, ver, cond) in subs {
        create_sub(sid, token, stype, ver, cond).await;
    }

    if let Some(bot) = bot_id {
        create_sub(sid, token, "user.whisper.message", "1", json!({"user_id": bot})).await;
    }
}

async fn create_sub(sid: &str, token: &str, sub_type: &str, version: &str, condition: Value) {
    let url = format!("{HELIX_URL}/eventsub/subscriptions");
    let headers = vec![
        ("Client-Id".into(), CLIENT_ID.into()),
        ("Authorization".into(), format!("Bearer {token}")),
        ("Content-Type".into(), "application/json".into()),
    ];
    let body = serde_json::to_vec(&json!({
        "type": sub_type,
        "version": version,
        "condition": condition,
        "transport": {
            "method": "websocket",
            "session_id": sid,
        }
    }))
    .unwrap();
    let res = match crate::http::request("POST", &url, &headers, &body).await {
        Ok(r) => r,
        Err(e) => {
            crate::say(format!("Create sub {sub_type}: {e}"));
            return;
        }
    };
    if res.status != 202 && res.status != 200 {
        crate::say(format!("Create sub {sub_type} HTTP {}: {}", res.status, res.text()));
    }
}

fn handle_notification(
    engine: &Rc<Engine>,
    broadcaster: &TwitchAccount,
    cfg: &Arc<watch::Sender<Config>>,
    speakers: &mut HashSet<String>,
    sub_type: &str,
    event: &Value,
) {
    match sub_type {
        "channel.chat.message" => {
            let chatter_id = event.pointer("/chatter_user_id").and_then(Value::as_str).unwrap_or("");
            let chatter_login = event
                .pointer("/chatter_user_login")
                .and_then(Value::as_str)
                .unwrap_or("");
            let chatter_name = event
                .pointer("/chatter_user_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let text = event.pointer("/message/text").and_then(Value::as_str).unwrap_or("");
            let msg_id = event.pointer("/message_id").and_then(Value::as_str).unwrap_or("");
            let color = event.pointer("/color").and_then(Value::as_str).unwrap_or("");
            let badges = event
                .pointer("/badges")
                .and_then(Value::as_array)
                .map(|a| a.as_slice())
                .unwrap_or(&[]);
            let (role, sub) = parse_badges(badges, chatter_id == broadcaster.user_id);
            let cheer_bits = event.pointer("/cheer/bits").and_then(Value::as_u64).unwrap_or(0);

            let mut reply_info = None;
            if let Some(r) = event.get("reply") {
                reply_info = Some(ReplyInfo {
                    msg_id: r.get("parent_message_id").and_then(Value::as_str).unwrap_or("").into(),
                    user_id: r.get("parent_user_id").and_then(Value::as_str).unwrap_or("").into(),
                    user_login: r.get("parent_user_login").and_then(Value::as_str).unwrap_or("").into(),
                    user_name: r.get("parent_user_name").and_then(Value::as_str).unwrap_or("").into(),
                    msg_body: r
                        .get("parent_message_body")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                });
            }

            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            populate_user_vars(&mut args, chatter_id, chatter_login, chatter_name, role, sub);
            populate_chat_vars(
                &mut args,
                &ChatContext {
                    text,
                    msg_id,
                    bits: cheer_bits,
                    role,
                    color,
                    reply: reply_info.as_ref(),
                    internal: false,
                },
            );

            // First Words detection
            let norm_login = chatter_login.to_lowercase();
            if !speakers.contains(&norm_login) {
                speakers.insert(norm_login.clone());
                let mut fw_args = args.clone();
                fw_args.insert("firstMessage".into(), true.into());
                fw_args.insert("first_words".into(), true.into());
                engine.fire(T_FIRST_WORDS, "First Words", "Twitch/Chat", "twitch", &fw_args, |t| {
                    let user_filter = t.get("userLogon").and_then(Value::as_str).unwrap_or("");
                    user_filter.is_empty() || user_filter.eq_ignore_ascii_case(chatter_login)
                });
                engine.emit("Twitch", "FirstWords", || json!(fw_args));
            }

            // Command runner
            let is_bot = cfg
                .borrow()
                .twitch
                .bot
                .as_ref()
                .is_some_and(|b| b.user_id == chatter_id);
            let chat_cmd = Chat {
                platform: "twitch".into(),
                source: 1, // 1 = Twitch message
                user_id: chatter_id.into(),
                user: chatter_name.into(),
                broadcaster: role == 4,
                moderator: role >= 3,
                vip: role == 2,
                subscriber: sub,
                bot: is_bot,
                internal: false,
                msg_id: msg_id.into(),
                text: text.into(),
                args: args.clone(),
            };
            engine.chat(&chat_cmd);

            // Fire Twitch Chat Message trigger
            engine.fire(T_CHAT_MESSAGE, "Chat Message", "Twitch/Chat", "twitch", &args, |t| {
                let contains = t.get("contains").and_then(Value::as_str).unwrap_or("");
                contains.is_empty() || text.to_lowercase().contains(&contains.to_lowercase())
            });
            engine.emit("Twitch", "ChatMessage", || json!(args));

            // If bits cheer
            if cheer_bits > 0 {
                engine.fire(T_CHEER, "Cheer", "Twitch/Chat", "twitch", &args, |t| {
                    let min = t.get("bitsMin").and_then(Value::as_u64).unwrap_or(0);
                    let max = t.get("bitsMax").and_then(Value::as_u64).unwrap_or(0);
                    cheer_bits >= min && (max == 0 || cheer_bits <= max)
                });
                engine.emit("Twitch", "Cheer", || json!(args));
            }
        }
        "channel.chat.notification" => {
            let ntype = event.pointer("/notice_type").and_then(Value::as_str).unwrap_or("");
            let chatter_id = event.pointer("/chatter_user_id").and_then(Value::as_str).unwrap_or("");
            let chatter_login = event
                .pointer("/chatter_user_login")
                .and_then(Value::as_str)
                .unwrap_or("");
            let chatter_name = event
                .pointer("/chatter_user_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let sys_msg = event.pointer("/system_message").and_then(Value::as_str).unwrap_or("");
            let badges = event
                .pointer("/badges")
                .and_then(Value::as_array)
                .map(|a| a.as_slice())
                .unwrap_or(&[]);
            let (role, sub) = parse_badges(badges, chatter_id == broadcaster.user_id);

            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            populate_user_vars(&mut args, chatter_id, chatter_login, chatter_name, role, sub);
            populate_chat_vars(
                &mut args,
                &ChatContext {
                    text: sys_msg,
                    role,
                    ..Default::default()
                },
            );

            match ntype {
                "announcement" => {
                    let color = event
                        .pointer("/announcement/color")
                        .and_then(Value::as_str)
                        .unwrap_or("PRIMARY");
                    args.insert("announceColor".into(), color.into());
                    engine.fire(
                        T_ANNOUNCEMENT,
                        "Announcement",
                        "Twitch/General",
                        "twitch",
                        &args,
                        |_| true,
                    );
                    engine.emit("Twitch", "Announcement", || json!(args));
                }
                "sub" => {
                    let tier = event.pointer("/sub/sub_tier").and_then(Value::as_str).unwrap_or("1000");
                    let tier_name = match tier {
                        "2000" => "tier 2",
                        "3000" => "tier 3",
                        _ => "tier 1",
                    };
                    args.insert("tier".into(), tier_name.into());
                    args.insert("isMultiMonth".into(), false.into());
                    args.insert("multiMonthDuration".into(), 1.into());
                    engine.fire(T_SUB, "Subscription", "Twitch/Subscriptions", "twitch", &args, |_| true);
                    engine.emit("Twitch", "Sub", || json!(args));
                }
                "resub" => {
                    let tier = event
                        .pointer("/resub/sub_tier")
                        .and_then(Value::as_str)
                        .unwrap_or("1000");
                    let tier_name = match tier {
                        "2000" => "tier 2",
                        "3000" => "tier 3",
                        _ => "tier 1",
                    };
                    let cumulative = event
                        .pointer("/resub/cumulative_months")
                        .and_then(Value::as_u64)
                        .unwrap_or(1);
                    let streak = event
                        .pointer("/resub/streak_months")
                        .and_then(Value::as_u64)
                        .unwrap_or(1);
                    args.insert("tier".into(), tier_name.into());
                    args.insert("cumulative".into(), cumulative.into());
                    args.insert("monthStreak".into(), streak.into());
                    args.insert("streakShared".into(), true.into());
                    engine.fire(
                        T_RESUB,
                        "Resubscription",
                        "Twitch/Subscriptions",
                        "twitch",
                        &args,
                        |_| true,
                    );
                    engine.emit("Twitch", "ReSub", || json!(args));
                }
                "sub_gift" => {
                    let tier = event
                        .pointer("/sub_gift/sub_tier")
                        .and_then(Value::as_str)
                        .unwrap_or("1000");
                    let recipient_id = event
                        .pointer("/sub_gift/recipient_user_id")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let recipient_login = event
                        .pointer("/sub_gift/recipient_user_login")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let recipient_name = event
                        .pointer("/sub_gift/recipient_user_name")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let total = event
                        .pointer("/sub_gift/cumulative_total")
                        .and_then(Value::as_u64)
                        .unwrap_or(1);
                    args.insert("tier".into(), tier.into());
                    args.insert("recipientId".into(), recipient_id.into());
                    args.insert("recipientUserName".into(), recipient_login.into());
                    args.insert("recipientUser".into(), recipient_name.into());
                    args.insert("totalSubsGifted".into(), total.into());
                    args.insert("monthsGifted".into(), 1.into());
                    args.insert("fromGiftBomb".into(), false.into());
                    engine.fire(
                        T_GIFT_SUB,
                        "Gift Subscription",
                        "Twitch/Subscriptions",
                        "twitch",
                        &args,
                        |_| true,
                    );
                    engine.emit("Twitch", "GiftSub", || json!(args));
                }
                "community_sub_gift" => {
                    let tier = event
                        .pointer("/community_sub_gift/sub_tier")
                        .and_then(Value::as_str)
                        .unwrap_or("1000");
                    let total = event
                        .pointer("/community_sub_gift/total")
                        .and_then(Value::as_u64)
                        .unwrap_or(1);
                    args.insert("tier".into(), tier.into());
                    args.insert("gifts".into(), total.into());
                    args.insert("totalGifts".into(), total.into());
                    args.insert("anonymous".into(), chatter_id.is_empty().into());
                    engine.fire(
                        T_GIFT_BOMB,
                        "Gift Bomb",
                        "Twitch/Subscriptions",
                        "twitch",
                        &args,
                        |_| true,
                    );
                    engine.emit("Twitch", "GiftBomb", || json!(args));
                }
                "raid" => {
                    let raider_id = event.pointer("/raid/user_id").and_then(Value::as_str).unwrap_or("");
                    let raider_login = event.pointer("/raid/user_login").and_then(Value::as_str).unwrap_or("");
                    let raider_name = event.pointer("/raid/user_name").and_then(Value::as_str).unwrap_or("");
                    let viewers = event.pointer("/raid/viewer_count").and_then(Value::as_u64).unwrap_or(0);
                    populate_user_vars(&mut args, raider_id, raider_login, raider_name, 1, false);
                    args.insert("viewers".into(), viewers.into());
                    engine.fire(T_RAID, "Raid", "Twitch/Raid", "twitch", &args, |_| true);
                    engine.emit("Twitch", "Raid", || json!(args));
                }
                _ => {}
            }
        }
        "channel.follow" => {
            let uid = event.pointer("/user_id").and_then(Value::as_str).unwrap_or("");
            let ulogin = event.pointer("/user_login").and_then(Value::as_str).unwrap_or("");
            let uname = event.pointer("/user_name").and_then(Value::as_str).unwrap_or("");
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            populate_user_vars(&mut args, uid, ulogin, uname, 1, false);
            engine.fire(T_FOLLOW, "Follow", "Twitch/Channel", "twitch", &args, |_| true);
            engine.emit("Twitch", "Follow", || json!(args));
        }
        "channel.subscribe" => {
            let uid = event.pointer("/user_id").and_then(Value::as_str).unwrap_or("");
            let ulogin = event.pointer("/user_login").and_then(Value::as_str).unwrap_or("");
            let uname = event.pointer("/user_name").and_then(Value::as_str).unwrap_or("");
            let tier = event.pointer("/tier").and_then(Value::as_str).unwrap_or("1000");
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            populate_user_vars(&mut args, uid, ulogin, uname, 1, true);
            args.insert("tier".into(), tier.into());
            args.insert("isMultiMonth".into(), false.into());
            engine.fire(T_SUB, "Subscription", "Twitch/Subscriptions", "twitch", &args, |_| true);
            engine.emit("Twitch", "Sub", || json!(args));
        }
        "channel.subscription.message" => {
            let uid = event.pointer("/user_id").and_then(Value::as_str).unwrap_or("");
            let ulogin = event.pointer("/user_login").and_then(Value::as_str).unwrap_or("");
            let uname = event.pointer("/user_name").and_then(Value::as_str).unwrap_or("");
            let tier = event.pointer("/tier").and_then(Value::as_str).unwrap_or("1000");
            let streak = event.pointer("/streak_months").and_then(Value::as_u64).unwrap_or(1);
            let cumulative = event.pointer("/cumulative_months").and_then(Value::as_u64).unwrap_or(1);
            let text = event.pointer("/message/text").and_then(Value::as_str).unwrap_or("");
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            populate_user_vars(&mut args, uid, ulogin, uname, 1, true);
            populate_chat_vars(
                &mut args,
                &ChatContext {
                    text,
                    role: 1,
                    ..Default::default()
                },
            );
            args.insert("tier".into(), tier.into());
            args.insert("cumulative".into(), cumulative.into());
            args.insert("monthStreak".into(), streak.into());
            args.insert("streakShared".into(), true.into());
            engine.fire(
                T_RESUB,
                "Resubscription",
                "Twitch/Subscriptions",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "ReSub", || json!(args));
        }
        "channel.subscription.gift" => {
            let uid = event.pointer("/user_id").and_then(Value::as_str).unwrap_or("");
            let ulogin = event.pointer("/user_login").and_then(Value::as_str).unwrap_or("");
            let uname = event.pointer("/user_name").and_then(Value::as_str).unwrap_or("");
            let total = event.pointer("/total").and_then(Value::as_u64).unwrap_or(1);
            let tier = event.pointer("/tier").and_then(Value::as_str).unwrap_or("1000");
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            populate_user_vars(&mut args, uid, ulogin, uname, 1, false);
            args.insert("gifts".into(), total.into());
            args.insert("totalGifts".into(), total.into());
            args.insert("tier".into(), tier.into());
            args.insert("anonymous".into(), uid.is_empty().into());
            engine.fire(
                T_GIFT_BOMB,
                "Gift Bomb",
                "Twitch/Subscriptions",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "GiftBomb", || json!(args));
        }
        "channel.cheer" => {
            let uid = event.pointer("/user_id").and_then(Value::as_str).unwrap_or("");
            let ulogin = event.pointer("/user_login").and_then(Value::as_str).unwrap_or("");
            let uname = event.pointer("/user_name").and_then(Value::as_str).unwrap_or("");
            let bits = event.pointer("/bits").and_then(Value::as_u64).unwrap_or(0);
            let text = event.pointer("/message").and_then(Value::as_str).unwrap_or("");
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            populate_user_vars(&mut args, uid, ulogin, uname, 1, false);
            populate_chat_vars(
                &mut args,
                &ChatContext {
                    text,
                    bits,
                    role: 1,
                    ..Default::default()
                },
            );
            engine.fire(T_CHEER, "Cheer", "Twitch/Chat", "twitch", &args, |_| true);
            engine.emit("Twitch", "Cheer", || json!(args));
        }
        "channel.raid" => {
            let raider_id = event
                .pointer("/from_broadcaster_user_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let raider_login = event
                .pointer("/from_broadcaster_user_login")
                .and_then(Value::as_str)
                .unwrap_or("");
            let raider_name = event
                .pointer("/from_broadcaster_user_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let viewers = event.pointer("/viewers").and_then(Value::as_u64).unwrap_or(0);
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            populate_user_vars(&mut args, raider_id, raider_login, raider_name, 1, false);
            args.insert("viewers".into(), viewers.into());
            engine.fire(T_RAID, "Raid", "Twitch/Raid", "twitch", &args, |_| true);
            engine.emit("Twitch", "Raid", || json!(args));
        }
        "stream.online" => {
            if cfg.borrow().twitch.first_words_auto_reset {
                speakers.clear();
            }
            let started_at = event.pointer("/started_at").and_then(Value::as_str).unwrap_or("");
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            args.insert("startedAt".into(), started_at.into());
            args.insert("game".into(), "".into());
            args.insert("gameId".into(), 0.into());
            args.insert("tagCount".into(), 0.into());
            args.insert("tags".into(), Value::Array(vec![]));
            engine.fire(
                T_STREAM_ONLINE,
                "Stream Online",
                "Twitch/Channel",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "StreamOnline", || json!(args));
        }
        "stream.offline" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            args.insert("endedAt".into(), crate::engine::globals::iso_now().into());
            engine.fire(
                T_STREAM_OFFLINE,
                "Stream Offline",
                "Twitch/Channel",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "StreamOffline", || json!(args));
        }
        "user.whisper.message" => {
            let from_id = event.pointer("/from_user_id").and_then(Value::as_str).unwrap_or("");
            let from_login = event.pointer("/from_user_login").and_then(Value::as_str).unwrap_or("");
            let from_name = event.pointer("/from_user_name").and_then(Value::as_str).unwrap_or("");
            let to_id = event.pointer("/to_user_id").and_then(Value::as_str).unwrap_or("");
            let text = event.pointer("/whisper/text").and_then(Value::as_str).unwrap_or("");

            let is_bot = cfg.borrow().twitch.bot.as_ref().is_some_and(|b| b.user_id == to_id);
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            populate_user_vars(&mut args, from_id, from_login, from_name, 1, false);
            populate_chat_vars(
                &mut args,
                &ChatContext {
                    text,
                    role: 1,
                    ..Default::default()
                },
            );

            if is_bot {
                engine.fire(T_BOT_WHISPER, "Bot Whisper", "Twitch/Chat", "twitch", &args, |_| true);
                engine.emit("Twitch", "BotWhisper", || json!(args));
            } else {
                engine.fire(T_WHISPER, "Whisper", "Twitch/Chat", "twitch", &args, |_| true);
                engine.emit("Twitch", "Whisper", || json!(args));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn parse_colors() {
        assert_eq!(parse_hex_color("#FF8000"), (255, 128, 0));
        assert_eq!(parse_hex_color("1E90FF"), (30, 144, 255));
        assert_eq!(parse_hex_color(""), (255, 255, 255));
    }

    #[test]
    fn parse_roles() {
        let broadcaster = json!([{"set_id": "broadcaster", "id": "1"}]);
        assert_eq!(parse_badges(broadcaster.as_array().unwrap(), true), (4, false));

        let mod_sub = json!([{"set_id": "moderator", "id": "1"}, {"set_id": "subscriber", "id": "1"}]);
        assert_eq!(parse_badges(mod_sub.as_array().unwrap(), false), (3, true));

        let vip = json!([{"set_id": "vip", "id": "1"}]);
        assert_eq!(parse_badges(vip.as_array().unwrap(), false), (2, false));

        let viewer = json!([]);
        assert_eq!(parse_badges(viewer.as_array().unwrap(), false), (1, false));
    }

    #[test]
    fn populate_vars() {
        let b = TwitchAccount {
            user_id: "123".into(),
            login: "alice".into(),
            display_name: "Alice".into(),
            token: TwitchToken::default(),
        };
        let mut args = Obj::new();
        populate_broadcaster_vars(&mut args, &b);
        assert_eq!(args["broadcastUser"], "Alice");
        assert_eq!(args["broadcastUserName"], "alice");
        assert_eq!(args["broadcastUserId"], "123");

        populate_user_vars(&mut args, "456", "bob", "Bob", 3, true);
        assert_eq!(args["user"], "Bob");
        assert_eq!(args["userName"], "bob");
        assert_eq!(args["userId"], "456");
        assert_eq!(args["userType"], "twitch");
        assert_eq!(args["isModerator"], true);
        assert_eq!(args["isSubscribed"], true);

        populate_chat_vars(
            &mut args,
            &ChatContext {
                text: "hello world",
                msg_id: "msg-1",
                bits: 100,
                role: 3,
                color: "#FF0000",
                ..Default::default()
            },
        );
        assert_eq!(args["message"], "hello world");
        assert_eq!(args["msgId"], "msg-1");
        assert_eq!(args["bits"], 100);
        assert_eq!(args["colorR"], 255);
        assert_eq!(args["colorG"], 0);
        assert_eq!(args["colorB"], 0);
        assert_eq!(args["input0"], "hello");
        assert_eq!(args["input1"], "world");
    }
}
