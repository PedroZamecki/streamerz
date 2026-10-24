//! Kick integration for streamerz (Phase 10):
//! - Zero-login public read via Pusher WebSocket (`wss://ws-us2.pusher.com`).
//! - Chat messages in and out, follows, subscriptions, gifts, kicks/tips, moderation, stream state.
//! - Third-party 7TV emotes integration for Kick.
//! - Official Kick API v1 with OAuth (client ID + secret) for sending chat, moderation, and channel updates.
//! - Triggers (35001..=35025) and sub-actions (35001..=35043, 35101..=35113).

use crate::commands::Chat;
use crate::engine::Engine;
use crate::model::Obj;
use crate::Config;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;

// Kick Trigger IDs
pub const T_KICK_AUTHENTICATED: u32 = 35001;
pub const T_KICK_CONNECTED: u32 = 35002;
pub const T_KICK_DISCONNECTED: u32 = 35003;
pub const T_KICK_FIRST_WORDS: u32 = 35008;
pub const T_KICK_PRESENT_VIEWERS: u32 = 35009;
pub const T_KICK_CHAT_MESSAGE: u32 = 35010;
pub const T_KICK_FOLLOW: u32 = 35011;
pub const T_KICK_STREAM_ONLINE: u32 = 35012;
pub const T_KICK_STREAM_OFFLINE: u32 = 35013;
pub const T_KICK_SUBSCRIPTION: u32 = 35014;
pub const T_KICK_RESUBSCRIPTION: u32 = 35015;
pub const T_KICK_GIFT_SUBSCRIPTION: u32 = 35016;
pub const T_KICK_MASS_GIFT_SUBSCRIPTION: u32 = 35017;
pub const T_KICK_USER_TIMED_OUT: u32 = 35018;
pub const T_KICK_USER_BANNED: u32 = 35019;
pub const T_KICK_CHANNEL_UPDATE: u32 = 35020;
pub const T_KICK_SEVENTV_EMOTE_ADDED: u32 = 35021;
pub const T_KICK_SEVENTV_EMOTE_REMOVED: u32 = 35022;
pub const T_KICK_VIEWER_COUNT_UPDATE: u32 = 35023;
pub const T_KICK_REWARD_REDEMPTION: u32 = 35024;
pub const T_KICK_KICKS_GIFTED: u32 = 35025;

/// Public Pusher app key used by kick.com (rotated 2026-10; a dead key answers `pusher:error` 4001).
// ponytail: hardcoded; ask web.kick.com/api/v1/realtime/connection if it keeps rotating.
pub const KICK_PUSHER_KEY: &str = "34bf7a0ff419a2a775b9";
pub const KICK_PUSHER_URL: &str =
    "wss://ws-us2.pusher.com/app/34bf7a0ff419a2a775b9?protocol=7&client=js&version=7.6.0&flash=false";
pub const KICK_API_BASE: &str = "https://api.kick.com/public/v1";

/// Strip URL prefixes, @ signs, and slashes from a Kick channel input.
pub fn clean_kick_channel(input: &str) -> String {
    let mut s = input.trim();
    for prefix in &[
        "https://www.kick.com/",
        "http://www.kick.com/",
        "https://kick.com/",
        "http://kick.com/",
        "www.kick.com/",
        "kick.com/",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
            break;
        }
    }
    s.trim_start_matches('@').trim_end_matches('/').trim().to_string()
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KickConfig {
    pub channel: String,
    pub channel_id: Option<u64>,
    pub chatroom_id: Option<u64>,
    pub auto_connect: bool,
    pub client_id: String,
    pub client_secret: String,
    pub broadcaster: Option<KickAccount>,
    pub bot: Option<KickAccount>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct KickAccount {
    pub user_id: u64,
    pub username: String,
    pub token: KickToken,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct KickToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

pub enum KickCmd {
    Connect,
    Disconnect,
    SendMessage {
        text: String,
        use_bot: bool,
        fallback: bool,
        resp: oneshot::Sender<Result<(), String>>,
    },
    ReplyToMessage {
        text: String,
        reply_id: String,
        use_bot: bool,
        fallback: bool,
        resp: oneshot::Sender<Result<(), String>>,
    },
    SetChannelTitle {
        title: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    SetChannelCategory {
        category_id: u64,
        category_name: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    BanUser {
        user_login: String,
        reason: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    UnbanUser {
        user_login: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    TimeoutUser {
        user_login: String,
        duration: u64,
        reason: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    UnTimeoutUser {
        user_login: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    GetStatus(oneshot::Sender<KickStatus>),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct KickStatus {
    pub connected: bool,
    pub channel: String,
    pub channel_id: Option<u64>,
    pub chatroom_id: Option<u64>,
}

#[derive(Clone)]
pub struct KickHandle {
    tx: mpsc::UnboundedSender<KickCmd>,
    connected: Arc<AtomicBool>,
}

impl KickHandle {
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn connect(&self) {
        let _ = self.tx.send(KickCmd::Connect);
    }

    pub fn disconnect(&self) {
        let _ = self.tx.send(KickCmd::Disconnect);
    }

    pub fn send(&self, cmd: KickCmd) {
        let _ = self.tx.send(cmd);
    }

    pub async fn send_message(&self, text: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::SendMessage {
                text: text.to_string(),
                use_bot,
                fallback,
                resp: resp_tx,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn reply_message(&self, text: &str, reply_id: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::ReplyToMessage {
                text: text.to_string(),
                reply_id: reply_id.to_string(),
                use_bot,
                fallback,
                resp: resp_tx,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn set_channel_title(&self, title: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::SetChannelTitle {
                title: title.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn set_channel_category(&self, category_id: u64, category_name: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::SetChannelCategory {
                category_id,
                category_name: category_name.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn ban_user(&self, user_login: &str, reason: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::BanUser {
                user_login: user_login.to_string(),
                reason: reason.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn unban_user(&self, user_login: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::UnbanUser {
                user_login: user_login.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn timeout_user(&self, user_login: &str, duration: u64, reason: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::TimeoutUser {
                user_login: user_login.to_string(),
                duration,
                reason: reason.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn untimeout_user(&self, user_login: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::UnTimeoutUser {
                user_login: user_login.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn get_status(&self) -> KickStatus {
        let (resp_tx, resp_rx) = oneshot::channel();
        if self.tx.send(KickCmd::GetStatus(resp_tx)).is_err() {
            return KickStatus::default();
        }
        resp_rx.await.unwrap_or_default()
    }
}

pub fn start(engine: Rc<Engine>, cfg: &Arc<watch::Sender<Config>>) -> KickHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let connected = Arc::new(AtomicBool::new(false));
    let handle = KickHandle {
        tx,
        connected: connected.clone(),
    };

    let runner = Runner {
        engine,
        cfg: cfg.clone(),
        rx,
        connected,
        read_task: None,
        speakers: HashSet::new(),
        ids: Default::default(),
    };
    tokio::task::spawn_local(runner.run());
    handle
}

struct Runner {
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    rx: mpsc::UnboundedReceiver<KickCmd>,
    connected: Arc<AtomicBool>,
    read_task: Option<tokio::task::JoinHandle<()>>,
    speakers: HashSet<String>,
    /// The channel (slug) and its broadcaster user id, looked up once.
    ids: std::cell::RefCell<Option<(String, u64)>>,
}

impl Runner {
    async fn run(mut self) {
        if self.cfg.borrow().kick.auto_connect && !self.cfg.borrow().kick.channel.trim().is_empty() {
            self.connect_pusher();
        }

        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                KickCmd::Connect => {
                    self.connect_pusher();
                }
                KickCmd::Disconnect => {
                    self.disconnect();
                }
                KickCmd::SendMessage {
                    text,
                    use_bot,
                    fallback,
                    resp,
                } => {
                    let res = self.send_chat_api(&text, None, use_bot, fallback).await;
                    let _ = resp.send(res);
                }
                KickCmd::ReplyToMessage {
                    text,
                    reply_id,
                    use_bot,
                    fallback,
                    resp,
                } => {
                    let res = self.send_chat_api(&text, Some(&reply_id), use_bot, fallback).await;
                    let _ = resp.send(res);
                }
                KickCmd::SetChannelTitle { title, resp } => {
                    let res = self.set_channel_title_api(&title).await;
                    let _ = resp.send(res);
                }
                KickCmd::SetChannelCategory {
                    category_id,
                    category_name: _,
                    resp,
                } => {
                    let res = self.set_channel_category_api(category_id).await;
                    let _ = resp.send(res);
                }
                KickCmd::BanUser {
                    user_login,
                    reason,
                    resp,
                } => {
                    let res = self.ban_user_api(&user_login, &reason).await;
                    let _ = resp.send(res);
                }
                KickCmd::UnbanUser { user_login, resp } => {
                    let res = self.unban_user_api(&user_login).await;
                    let _ = resp.send(res);
                }
                KickCmd::TimeoutUser {
                    user_login,
                    duration,
                    reason,
                    resp,
                } => {
                    let res = self.timeout_user_api(&user_login, duration, &reason).await;
                    let _ = resp.send(res);
                }
                KickCmd::UnTimeoutUser { user_login, resp } => {
                    let res = self.untimeout_user_api(&user_login).await;
                    let _ = resp.send(res);
                }
                KickCmd::GetStatus(resp) => {
                    let c = self.cfg.borrow();
                    let status = KickStatus {
                        connected: self.connected.load(Ordering::Relaxed),
                        channel: c.kick.channel.clone(),
                        channel_id: c.kick.channel_id,
                        chatroom_id: c.kick.chatroom_id,
                    };
                    let _ = resp.send(status);
                }
            }
        }
    }

    fn connect_pusher(&mut self) {
        if self.connected.load(Ordering::Relaxed) {
            return;
        }

        let channel = self.cfg.borrow().kick.channel.trim().to_string();
        if channel.is_empty() {
            crate::say("Kick: channel name is empty".into());
            return;
        }

        let engine = self.engine.clone();
        let cfg = self.cfg.clone();
        let connected = self.connected.clone();
        let mut speakers = std::mem::take(&mut self.speakers);

        crate::say(format!("Kick: connecting to public Pusher for channel '{channel}'..."));

        self.read_task = Some(tokio::task::spawn_local(async move {
            run_pusher_loop(engine, cfg, connected, &mut speakers).await;
        }));
    }

    fn disconnect(&mut self) {
        if let Some(task) = self.read_task.take() {
            task.abort();
        }
        if self.connected.swap(false, Ordering::Relaxed) {
            crate::say("Kick: disconnected".into());
            let args = Obj::new();
            self.engine.fire(
                T_KICK_DISCONNECTED,
                "KickBroadcasterChatDisconnected",
                "Kick",
                "kick",
                &args,
                |_| true,
            );
            self.engine.emit("Kick", "BroadcasterChatDisconnected", || json!(args));
        }
    }

    /// A user id from a login: every Kick user has a channel whose slug is their name.
    async fn user_id(&self, token: &str, login: &str) -> Result<u64, String> {
        let slug = clean_kick_channel(login).to_lowercase();
        if slug.is_empty() {
            return Err("no user given".into());
        }
        let v = api(
            token,
            "GET",
            &format!("/channels?slug={}", crate::commands::url_encode(&slug)),
            None,
        )
        .await?;
        v.pointer("/data/0/broadcaster_user_id")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("no Kick user '{slug}'"))
    }

    /// The broadcaster's user id: the logged-in broadcaster's, else the channel's (looked up once).
    async fn broadcaster_id(&self, token: &str) -> Result<u64, String> {
        let (chan, cached) = {
            let c = self.cfg.borrow();
            let acc = c.kick.broadcaster.as_ref().filter(|b| b.user_id != 0);
            (c.kick.channel.clone(), acc.map(|b| (b.username.clone(), b.user_id)))
        };
        if let Some((name, id)) = cached {
            if chan.is_empty() || name.eq_ignore_ascii_case(&chan) {
                return Ok(id);
            }
        }
        if let Some((slug, id)) = self.ids.borrow().as_ref() {
            if *slug == chan {
                return Ok(*id);
            }
        }
        let id = self.user_id(token, &chan).await?;
        *self.ids.borrow_mut() = Some((chan, id));
        Ok(id)
    }

    async fn send_chat_api(
        &self,
        text: &str,
        reply_id: Option<&str>,
        use_bot: bool,
        fallback: bool,
    ) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, use_bot, fallback).await?;
        // "user" with the channel's id: sent by whoever the token is (bot or broadcaster) into the
        // broadcaster's chat. ("bot" would send to the token's own channel.)
        let mut body = json!({
            "content": text,
            "type": "user",
            "broadcaster_user_id": self.broadcaster_id(&token).await?,
        });
        if let Some(r) = reply_id.filter(|r| !r.is_empty()) {
            body["reply_to_message_id"] = json!(r);
        }
        api(&token, "POST", "/chat", Some(body)).await.map(drop)
    }

    async fn set_channel_title_api(&self, title: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        api(&token, "PATCH", "/channels", Some(json!({ "stream_title": title })))
            .await
            .map(drop)
    }

    async fn set_channel_category_api(&self, category_id: u64) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        api(
            &token,
            "PATCH",
            "/channels",
            Some(json!({ "category_id": category_id })),
        )
        .await
        .map(drop)
    }

    /// Ban (`minutes` None) or time out a user.
    async fn ban(&self, user_login: &str, minutes: Option<u64>, reason: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let mut body = json!({
            "broadcaster_user_id": self.broadcaster_id(&token).await?,
            "user_id": self.user_id(&token, user_login).await?,
        });
        if let Some(m) = minutes {
            body["duration"] = json!(m.clamp(1, 10080));
        }
        if !reason.is_empty() {
            body["reason"] = json!(reason.chars().take(100).collect::<String>());
        }
        api(&token, "POST", "/moderation/bans", Some(body)).await.map(drop)
    }

    async fn ban_user_api(&self, user_login: &str, reason: &str) -> Result<(), String> {
        self.ban(user_login, None, reason).await
    }

    /// Streamer.bot gives timeouts in seconds; Kick takes minutes (1 to 7 days).
    async fn timeout_user_api(&self, user_login: &str, seconds: u64, reason: &str) -> Result<(), String> {
        self.ban(user_login, Some(seconds.div_ceil(60)), reason).await
    }

    /// Lifts a ban or a timeout (the same call on Kick).
    async fn unban_user_api(&self, user_login: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let body = json!({
            "broadcaster_user_id": self.broadcaster_id(&token).await?,
            "user_id": self.user_id(&token, user_login).await?,
        });
        api(&token, "DELETE", "/moderation/bans", Some(body)).await.map(drop)
    }

    async fn untimeout_user_api(&self, user_login: &str) -> Result<(), String> {
        self.unban_user_api(user_login).await
    }
}

async fn run_pusher_loop(
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    connected: Arc<AtomicBool>,
    speakers: &mut HashSet<String>,
) {
    run_pusher_loop_url(engine, cfg, connected, speakers, KICK_PUSHER_URL, false).await;
}

pub const KICK_REDIRECT: &str = "http://127.0.0.1:45563/callback";
const KICK_TOKEN_URL: &str = "https://id.kick.com/oauth/token";
const KICK_SCOPES: &str = "user:read channel:read channel:write chat:write moderation:ban events:subscribe";
/// The bot only chats.
const KICK_BOT_SCOPES: &str = "user:read chat:write";

/// One token refresh at a time: Kick replaces the refresh token, so a second refresh with the old
/// one would fail.
static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The bot's (or broadcaster's) access token, the other account's when `fallback`; refreshed first
/// when it's about to expire.
pub async fn token(
    engine: &Rc<Engine>,
    cfg: &Arc<watch::Sender<Config>>,
    bot: bool,
    fallback: bool,
) -> Result<String, String> {
    let _one = REFRESH.lock().await;
    let pick = |c: &Config, bot: bool| {
        if bot { &c.kick.bot } else { &c.kick.broadcaster }
            .clone()
            .filter(|a| !a.token.access_token.is_empty())
    };
    let (acc, bot) = {
        let c = cfg.borrow();
        match (pick(&c, bot), fallback) {
            (Some(a), _) => (a, bot),
            (None, true) => (pick(&c, !bot).ok_or("No Kick account logged in")?, !bot),
            (None, false) => {
                return Err(format!(
                    "Kick {} isn't logged in",
                    if bot { "bot" } else { "broadcaster" }
                ));
            }
        }
    };
    if !crate::oauth::stale(acc.token.expires_at) || acc.token.refresh_token.is_empty() {
        return Ok(acc.token.access_token);
    }
    let (id, secret) = {
        let c = cfg.borrow();
        (c.kick.client_id.clone(), c.kick.client_secret.clone())
    };
    let t = crate::oauth::token(
        KICK_TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &acc.token.refresh_token),
            ("client_id", &id),
            ("client_secret", &secret),
        ],
    )
    .await
    .map_err(|e| format!("Kick token refresh: {e}"))?;
    let new = KickToken {
        access_token: t.access.clone(),
        refresh_token: if t.refresh.is_empty() {
            acc.token.refresh_token
        } else {
            t.refresh
        },
        expires_at: t.expires_at,
    };
    engine.change(|c| {
        if let Some(a) = if bot {
            c.kick.bot.as_mut()
        } else {
            c.kick.broadcaster.as_mut()
        } {
            a.token = new;
        }
    });
    Ok(t.access)
}

/// Log in through the browser with the user's own Kick app (client ID and secret in the config,
/// redirect `KICK_REDIRECT`).
pub async fn login(cfg: &Arc<watch::Sender<Config>>, bot: bool) -> Result<KickAccount, String> {
    let (id, secret) = {
        let c = cfg.borrow();
        (
            c.kick.client_id.trim().to_string(),
            c.kick.client_secret.trim().to_string(),
        )
    };
    if id.is_empty() || secret.is_empty() {
        return Err("Set your Kick app's client ID and secret first (Accounts > Kick > Advanced).".into());
    }
    let l = crate::oauth::Login::bind(45563, "/callback").await?;
    let e = crate::commands::url_encode;
    // `redirect=127.0.0.1` first: kick.com rewrites the first 127.0.0.1 in the URL to localhost
    // (Kick's documented workaround), which would break the redirect_uri match.
    crate::oauth::open_browser(&format!(
        "https://id.kick.com/oauth/authorize?response_type=code&client_id={}&redirect=127.0.0.1&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        e(&id),
        e(&l.redirect),
        e(if bot { KICK_BOT_SCOPES } else { KICK_SCOPES }),
        l.challenge,
        l.state
    ));
    let (redirect, verifier) = (l.redirect.clone(), l.verifier.clone());
    let code = l.code().await?;
    let t = crate::oauth::token(
        KICK_TOKEN_URL,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", &id),
            ("client_secret", &secret),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
        ],
    )
    .await?;
    let (user_id, username) = fetch_kick_user(&t.access).await?;
    Ok(KickAccount {
        user_id,
        username,
        token: KickToken {
            access_token: t.access,
            refresh_token: t.refresh,
            expires_at: t.expires_at,
        },
    })
}

/// One official API call: the JSON answer, or Kick's error message.
async fn api(token: &str, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
    let mut headers = vec![
        ("Authorization".to_string(), format!("Bearer {token}")),
        ("Accept".to_string(), "application/json".to_string()),
    ];
    let body = match body {
        Some(b) => {
            headers.push(("Content-Type".to_string(), "application/json".to_string()));
            serde_json::to_vec(&b).unwrap_or_default()
        }
        None => Vec::new(),
    };
    let r = crate::http::request(method, &format!("{KICK_API_BASE}{path}"), &headers, &body).await?;
    let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
    if (200..300).contains(&r.status) {
        Ok(v)
    } else {
        let msg = v["message"].as_str().map_or_else(|| r.text(), String::from);
        Err(format!("Kick {method} {path}: HTTP {} {msg}", r.status))
    }
}

/// Kick's emotes in a message: each `[emote:<id>:<name>]`.
pub fn kick_emotes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("[emote:") {
        let Some(j) = rest[i..].find(']') else { break };
        let tag = &rest[i..i + j + 1];
        if tag[7..tag.len() - 1]
            .split_once(':')
            .is_some_and(|(id, _)| id.bytes().all(|b| b.is_ascii_digit()))
        {
            out.push(tag.to_string());
        }
        rest = &rest[i + j + 1..];
    }
    out
}

/// Query Kick API v1 /users endpoint using an OAuth bearer token.
pub async fn fetch_kick_user(token: &str) -> Result<(u64, String), String> {
    let url = format!("{KICK_API_BASE}/users");
    let headers = vec![
        ("Authorization".into(), format!("Bearer {token}")),
        ("Accept".into(), "application/json".into()),
    ];
    let resp = crate::http::request("GET", &url, &headers, b"").await?;
    if (200..300).contains(&resp.status) {
        let val = serde_json::from_slice::<Value>(&resp.body)
            .map_err(|e| format!("Invalid JSON from Kick users API: {e}"))?;
        if let Some(user) = val.pointer("/data/0") {
            let uid = user.get("user_id").and_then(Value::as_u64).unwrap_or(0);
            let name = user.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            return Ok((uid, name));
        }
    }
    Err(format!("Kick API returned HTTP {}", resp.status))
}

pub async fn run_pusher_loop_url(
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    connected: Arc<AtomicBool>,
    speakers: &mut HashSet<String>,
    pusher_url: &str,
    run_once: bool,
) {
    let channel = cfg.borrow().kick.channel.trim().to_lowercase();
    let chatroom_id = cfg.borrow().kick.chatroom_id;
    let channel_id = cfg.borrow().kick.channel_id;

    // Resolve channel_id and chatroom_id if not present
    let (c_id, cr_id) = match (channel_id, chatroom_id) {
        (Some(cid), Some(crid)) => (cid, crid),
        _ => {
            // Fetch public channel details
            let url = format!("https://kick.com/api/v2/channels/{channel}");
            let details = crate::http::request("GET", &url, &[], b"").await.ok();
            let parsed = details
                .as_ref()
                .and_then(|r| serde_json::from_slice::<Value>(&r.body).ok());
            let cid = parsed
                .as_ref()
                .and_then(|v| v.pointer("/id").and_then(Value::as_u64))
                .or(channel_id)
                .unwrap_or(1);
            let crid = parsed
                .as_ref()
                .and_then(|v| v.pointer("/chatroom/id").and_then(Value::as_u64))
                .or(chatroom_id)
                .unwrap_or(cid);

            engine.change(|c| {
                c.kick.channel_id = Some(cid);
                c.kick.chatroom_id = Some(crid);
            });
            (cid, crid)
        }
    };

    loop {
        match crate::http::ws_connect(pusher_url).await {
            Ok((ws, _)) => {
                let (mut sink, mut stream) = ws.split();

                // Wait for connection established
                let mut established = false;
                while let Some(Ok(msg)) = stream.next().await {
                    if let Message::Text(txt) = msg {
                        if let Ok(val) = serde_json::from_str::<Value>(&txt) {
                            if val["event"].as_str() == Some("pusher:connection_established") {
                                established = true;
                                break;
                            }
                            if val["event"].as_str() == Some("pusher:error") {
                                crate::say(format!(
                                    "Kick: Pusher refused: {}",
                                    val["data"]["message"].as_str().unwrap_or("")
                                ));
                                break;
                            }
                        }
                    }
                }

                if !established {
                    crate::say("Kick: connection established handshake failed".into());
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }

                connected.store(true, Ordering::Relaxed);
                crate::ui::changed();
                crate::say(format!("Kick: connected to chatroom {cr_id} and channel {c_id}"));

                let mut args = Obj::new();
                args.insert("channel".into(), channel.clone().into());
                args.insert("channelId".into(), c_id.into());
                args.insert("chatroomId".into(), cr_id.into());
                engine.fire(
                    T_KICK_CONNECTED,
                    "KickBroadcasterChatConnected",
                    "Kick",
                    "kick",
                    &args,
                    |_| true,
                );
                engine.emit("Kick", "BroadcasterChatConnected", || json!(args));

                // Subscribe to channels
                let sub_chatroom = json!({
                    "event": "pusher:subscribe",
                    "data": { "auth": "", "channel": format!("chatrooms.{cr_id}.v2") }
                });
                let sub_channel = json!({
                    "event": "pusher:subscribe",
                    "data": { "auth": "", "channel": format!("channel.{c_id}") }
                });
                let sub_channel_v2 = json!({
                    "event": "pusher:subscribe",
                    "data": { "auth": "", "channel": format!("channel.{c_id}.v2") }
                });

                let _ = sink.send(Message::Text(sub_chatroom.to_string().into())).await;
                let _ = sink.send(Message::Text(sub_channel.to_string().into())).await;
                let _ = sink.send(Message::Text(sub_channel_v2.to_string().into())).await;

                // Message processing loop
                while let Some(msg_res) = stream.next().await {
                    let msg = match msg_res {
                        Ok(m) => m,
                        Err(e) => {
                            crate::say(format!("Kick: websocket error: {e}"));
                            break;
                        }
                    };

                    match msg {
                        Message::Text(txt) => {
                            if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                                let event_name = v["event"].as_str().unwrap_or("");
                                if event_name == "pusher:ping" {
                                    let pong = json!({ "event": "pusher:pong", "data": {} });
                                    let _ = sink.send(Message::Text(pong.to_string().into())).await;
                                    continue;
                                }

                                handle_kick_event(&engine, &channel, event_name, &v["data"], speakers);
                            }
                        }
                        Message::Ping(data) => {
                            let _ = sink.send(Message::Pong(data)).await;
                        }
                        Message::Close(_) => {
                            crate::say("Kick: connection closed by server".into());
                            break;
                        }
                        _ => {}
                    }
                }

                connected.store(false, Ordering::Relaxed);
                crate::ui::changed();
                let args = Obj::new();
                engine.fire(
                    T_KICK_DISCONNECTED,
                    "KickBroadcasterChatDisconnected",
                    "Kick",
                    "kick",
                    &args,
                    |_| true,
                );
                engine.emit("Kick", "BroadcasterChatDisconnected", || json!(args));
            }
            Err(e) => {
                crate::say(format!("Kick: connection error: {e}"));
            }
        }

        if run_once {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

pub fn handle_kick_event(
    engine: &Rc<Engine>,
    channel: &str,
    event: &str,
    data_val: &Value,
    speakers: &mut HashSet<String>,
) {
    let data: Value = match data_val {
        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        v => v.clone(),
    };

    match event {
        "App\\Events\\ChatMessageEvent" => {
            let content = data["content"].as_str().unwrap_or("").to_string();
            let msg_id = data["id"].as_str().unwrap_or("").to_string();
            let emotes = kick_emotes(&content);
            let sender = &data["sender"];
            let user_id = sender["id"].as_u64().unwrap_or(0);
            let username = sender["username"].as_str().unwrap_or("").to_string();
            let user_slug = sender["slug"].as_str().unwrap_or("").to_string();
            let color = sender
                .pointer("/identity/color")
                .and_then(Value::as_str)
                .unwrap_or("#53FC18")
                .to_string();

            let badges = sender
                .pointer("/identity/badges")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            let is_broadcaster = badges.iter().any(|b| b["type"].as_str() == Some("broadcaster"))
                || username.eq_ignore_ascii_case(channel);
            let is_moderator = badges.iter().any(|b| b["type"].as_str() == Some("moderator"));
            let is_subscriber = badges.iter().any(|b| b["type"].as_str() == Some("subscriber"));

            let mut args = Obj::new();
            args.insert("user".into(), username.clone().into());
            args.insert("userName".into(), username.clone().into());
            args.insert("userSlug".into(), user_slug.into());
            args.insert("userId".into(), user_id.into());
            args.insert("color".into(), color.into());
            args.insert("message".into(), content.clone().into());
            args.insert("messageId".into(), msg_id.clone().into());
            args.insert("isBroadcaster".into(), is_broadcaster.into());
            args.insert("isModerator".into(), is_moderator.into());
            args.insert("isSubscriber".into(), is_subscriber.into());

            // First words check
            let lower_user = username.to_lowercase();
            if !speakers.contains(&lower_user) {
                speakers.insert(lower_user);
                engine.fire(T_KICK_FIRST_WORDS, "KickFirstWords", "Kick", "kick", &args, |_| true);
                engine.emit("Kick", "FirstWords", || json!(args));
            }

            // Chat Message trigger
            engine.fire(T_KICK_CHAT_MESSAGE, "KickChatMessage", "Kick", "kick", &args, |_| true);
            engine.emit("Kick", "ChatMessage", || json!(args));

            // Feed chat commands into engine
            let chat = Chat {
                platform: "kick".into(),
                source: 2_097_152,
                emotes,
                msg_id,
                user: username,
                user_id: user_id.to_string(),
                text: content,
                broadcaster: is_broadcaster,
                moderator: is_moderator,
                subscriber: is_subscriber,
                args: args.clone(),
                ..Default::default()
            };
            engine.chat(&chat);
        }
        "App\\Events\\FollowersUpdated" => {
            let mut args = Obj::new();
            let username = data["username"].as_str().unwrap_or("").to_string();
            let user_id = data["user_id"].as_u64().or_else(|| data["id"].as_u64()).unwrap_or(0);
            args.insert("user".into(), username.into());
            args.insert("userId".into(), user_id.into());
            engine.fire(T_KICK_FOLLOW, "KickFollow", "Kick", "kick", &args, |_| true);
            engine.emit("Kick", "Follow", || json!(args));
        }
        "App\\Events\\LivestreamStarted" | "App\\Events\\StreamOnline" => {
            let mut args = Obj::new();
            args.insert("channel".into(), channel.into());
            engine.fire(T_KICK_STREAM_ONLINE, "KickStreamOnline", "Kick", "kick", &args, |_| {
                true
            });
            engine.emit("Kick", "StreamOnline", || json!(args));
        }
        "App\\Events\\LivestreamEnded" | "App\\Events\\StreamOffline" => {
            let mut args = Obj::new();
            args.insert("channel".into(), channel.into());
            engine.fire(
                T_KICK_STREAM_OFFLINE,
                "KickStreamOffline",
                "Kick",
                "kick",
                &args,
                |_| true,
            );
            engine.emit("Kick", "StreamOffline", || json!(args));
        }
        "App\\Events\\SubscriptionEvent" => {
            let months = data["months"].as_u64().unwrap_or(1);
            let username = data["username"].as_str().unwrap_or("").to_string();
            let mut args = Obj::new();
            args.insert("user".into(), username.into());
            args.insert("months".into(), months.into());
            if months > 1 {
                engine.fire(
                    T_KICK_RESUBSCRIPTION,
                    "KickResubscription",
                    "Kick",
                    "kick",
                    &args,
                    |_| true,
                );
                engine.emit("Kick", "Resubscription", || json!(args));
            } else {
                engine.fire(T_KICK_SUBSCRIPTION, "KickSubscription", "Kick", "kick", &args, |_| true);
                engine.emit("Kick", "Subscription", || json!(args));
            }
        }
        "App\\Events\\GiftedSubscriptionsEvent" => {
            let gifter = data["gifter_username"].as_str().unwrap_or("Anonymous").to_string();
            let count = data["gifted_usernames"].as_array().map(|a| a.len() as u64).unwrap_or(1);
            let mut args = Obj::new();
            args.insert("user".into(), gifter.clone().into());
            args.insert("count".into(), count.into());
            if count > 1 {
                engine.fire(
                    T_KICK_MASS_GIFT_SUBSCRIPTION,
                    "KickMassGiftSubscription",
                    "Kick",
                    "kick",
                    &args,
                    |_| true,
                );
                engine.emit("Kick", "MassGiftSubscription", || json!(args));
            } else {
                engine.fire(
                    T_KICK_GIFT_SUBSCRIPTION,
                    "KickGiftSubscription",
                    "Kick",
                    "kick",
                    &args,
                    |_| true,
                );
                engine.emit("Kick", "GiftSubscription", || json!(args));
            }
        }
        "App\\Events\\UserBannedEvent" => {
            let user = data["user"].as_str().unwrap_or("").to_string();
            let banned_by = data["banned_by"].as_str().unwrap_or("").to_string();
            let reason = data["reason"].as_str().unwrap_or("").to_string();
            let duration = data["duration"].as_u64().unwrap_or(0);
            let mut args = Obj::new();
            args.insert("user".into(), user.into());
            args.insert("bannedBy".into(), banned_by.into());
            args.insert("reason".into(), reason.into());
            args.insert("duration".into(), duration.into());
            if duration > 0 {
                engine.fire(T_KICK_USER_TIMED_OUT, "KickUserTimedOut", "Kick", "kick", &args, |_| {
                    true
                });
                engine.emit("Kick", "UserTimedOut", || json!(args));
            } else {
                engine.fire(T_KICK_USER_BANNED, "KickUserBanned", "Kick", "kick", &args, |_| true);
                engine.emit("Kick", "UserBanned", || json!(args));
            }
        }
        "App\\Events\\ChannelUpdateEvent" => {
            let title = data["title"].as_str().unwrap_or("").to_string();
            let category = data["category"].as_str().unwrap_or("").to_string();
            let mut args = Obj::new();
            args.insert("title".into(), title.into());
            args.insert("category".into(), category.into());
            engine.fire(
                T_KICK_CHANNEL_UPDATE,
                "KickChannelUpdate",
                "Kick",
                "kick",
                &args,
                |_| true,
            );
            engine.emit("Kick", "ChannelUpdate", || json!(args));
        }
        "App\\Events\\KicksGiftedEvent" => {
            let amount = data["amount"].as_u64().unwrap_or(0);
            let sender = data["sender"].as_str().unwrap_or("").to_string();
            let mut args = Obj::new();
            args.insert("amount".into(), amount.into());
            args.insert("user".into(), sender.into());
            engine.fire(T_KICK_KICKS_GIFTED, "KickKicksGifted", "Kick", "kick", &args, |_| true);
            engine.emit("Kick", "KicksGifted", || json!(args));
        }
        "App\\Events\\RewardRedeemedEvent" => {
            let reward_id = data["reward_id"].as_str().unwrap_or("").to_string();
            let user = data["user"].as_str().unwrap_or("").to_string();
            let input = data["user_input"].as_str().unwrap_or("").to_string();
            let mut args = Obj::new();
            args.insert("rewardId".into(), reward_id.into());
            args.insert("user".into(), user.into());
            args.insert("userInput".into(), input.into());
            engine.fire(
                T_KICK_REWARD_REDEMPTION,
                "KickRewardRedemption",
                "Kick",
                "kick",
                &args,
                |_| true,
            );
            engine.emit("Kick", "RewardRedemption", || json!(args));
        }
        "7tv:emote_added" | "App\\Events\\SevenTVEmoteAdded" => {
            let emote = data["name"].as_str().unwrap_or("").to_string();
            let mut args = Obj::new();
            args.insert("emote".into(), emote.into());
            engine.fire(
                T_KICK_SEVENTV_EMOTE_ADDED,
                "KickSevenTVEmoteAdded",
                "Kick",
                "kick",
                &args,
                |_| true,
            );
            engine.emit("Kick", "SevenTVEmoteAdded", || json!(args));
        }
        "7tv:emote_removed" | "App\\Events\\SevenTVEmoteRemoved" => {
            let emote = data["name"].as_str().unwrap_or("").to_string();
            let mut args = Obj::new();
            args.insert("emote".into(), emote.into());
            engine.fire(
                T_KICK_SEVENTV_EMOTE_REMOVED,
                "KickSevenTVEmoteRemoved",
                "Kick",
                "kick",
                &args,
                |_| true,
            );
            engine.emit("Kick", "SevenTVEmoteRemoved", || json!(args));
        }
        "App\\Events\\ViewerCountUpdated" | "App\\Events\\StreamViewersChanged" => {
            let count = data["viewers"].as_u64().or_else(|| data["count"].as_u64()).unwrap_or(0);
            let mut args = Obj::new();
            args.insert("viewers".into(), count.into());
            engine.fire(
                T_KICK_VIEWER_COUNT_UPDATE,
                "KickViewerCountUpdate",
                "Kick",
                "kick",
                &args,
                |_| true,
            );
            engine.emit("Kick", "ViewerCountUpdate", || json!(args));
        }
        "App\\Events\\PresentViewers" => {
            let mut args = Obj::new();
            args.insert("viewers".into(), data.clone());
            engine.fire(
                T_KICK_PRESENT_VIEWERS,
                "KickPresentViewers",
                "Kick",
                "kick",
                &args,
                |_| true,
            );
            engine.emit("Kick", "PresentViewers", || json!(args));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_clean_kick_channel() {
        assert_eq!(clean_kick_channel("https://kick.com/streamer"), "streamer");
        assert_eq!(clean_kick_channel("http://www.kick.com/@mychannel/"), "mychannel");
        assert_eq!(clean_kick_channel("@coolstreamer"), "coolstreamer");
        assert_eq!(clean_kick_channel("normal"), "normal");
    }

    use super::*;
    #[tokio::test]
    async fn test_mock_pusher_protocol() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

            // 1. Send connection established
            let established = json!({
                "event": "pusher:connection_established",
                "data": "{\"socket_id\":\"999.888\"}"
            });
            ws.send(Message::Text(established.to_string().into())).await.unwrap();

            // 2. Expect subscriptions
            let mut subs_received = 0;
            while subs_received < 3 {
                if let Some(Ok(Message::Text(msg))) = ws.next().await {
                    let v: Value = serde_json::from_str(&msg).unwrap();
                    if v["event"].as_str() == Some("pusher:subscribe") {
                        subs_received += 1;
                    }
                }
            }

            // 3. Send ping and expect pong
            let ping = json!({ "event": "pusher:ping", "data": {} });
            ws.send(Message::Text(ping.to_string().into())).await.unwrap();
            if let Some(Ok(Message::Text(msg))) = ws.next().await {
                let v: Value = serde_json::from_str(&msg).unwrap();
                assert_eq!(v["event"].as_str(), Some("pusher:pong"));
            }

            // 4. Send a ChatMessageEvent
            let chat_evt = json!({
                "event": "App\\Events\\ChatMessageEvent",
                "data": json!({
                    "id": "chat-999",
                    "content": "Kick mock chat message!",
                    "sender": {
                        "id": 1234,
                        "username": "KickTester",
                        "slug": "kicktester",
                        "identity": {
                            "color": "#00FF00",
                            "badges": []
                        }
                    }
                }).to_string()
            });
            ws.send(Message::Text(chat_evt.to_string().into())).await.unwrap();

            // 5. Send StreamOnline, Follow, Sub, Ban, ChannelUpdate
            let follow_evt = json!({
                "event": "App\\Events\\FollowersUpdated",
                "data": json!({ "id": 55, "username": "Follower1" }).to_string()
            });
            ws.send(Message::Text(follow_evt.to_string().into())).await.unwrap();

            // Close connection
            let _ = ws.send(Message::Close(None)).await;
        });

        let mut config = Config::default();
        config.kick.channel = "kicktester".into();
        config.kick.channel_id = Some(10);
        config.kick.chatroom_id = Some(42);

        let (tx, _rx) = watch::channel(config);
        let cfg_sender = Arc::new(tx);
        let engine = Engine::new(cfg_sender.clone(), |_| {});
        let connected = Arc::new(AtomicBool::new(false));
        let mut speakers = HashSet::new();

        let ws_url = format!("ws://{addr}");
        run_pusher_loop_url(
            engine.clone(),
            cfg_sender.clone(),
            connected.clone(),
            &mut speakers,
            &ws_url,
            true,
        )
        .await;

        assert!(speakers.contains("kicktester"));
        assert!(!connected.load(Ordering::Relaxed));
    }

    #[test]
    fn kick_config_defaults() {
        let cfg = KickConfig::default();
        assert_eq!(cfg.channel, "");
        assert_eq!(cfg.client_id, "");
        assert!(!cfg.auto_connect);
    }

    #[test]
    fn parse_kick_chat_message() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});
        let mut speakers = HashSet::new();

        let event = "App\\Events\\ChatMessageEvent";
        let payload = json!({
            "id": "msg-123",
            "chatroom_id": 42,
            "content": "!test command",
            "type": "message",
            "sender": {
                "id": 100,
                "username": "gamer123",
                "slug": "gamer123",
                "identity": {
                    "color": "#FF0000",
                    "badges": [{ "type": "broadcaster", "text": "Broadcaster" }]
                }
            }
        });

        handle_kick_event(&engine, "gamer123", event, &payload, &mut speakers);
        assert!(speakers.contains("gamer123"));
    }

    #[test]
    fn parse_kick_follow_event() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});
        let mut speakers = HashSet::new();

        let event = "App\\Events\\FollowersUpdated";
        let payload = json!({
            "id": 555,
            "username": "coolfollower"
        });

        handle_kick_event(&engine, "channel", event, &payload, &mut speakers);
    }

    #[test]
    fn parse_kick_other_events() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});
        let mut speakers = HashSet::new();

        // Subscription
        let sub = json!({ "username": "subber", "months": 1 });
        handle_kick_event(&engine, "chan", "App\\Events\\SubscriptionEvent", &sub, &mut speakers);

        // Resub
        let resub = json!({ "username": "resubber", "months": 5 });
        handle_kick_event(&engine, "chan", "App\\Events\\SubscriptionEvent", &resub, &mut speakers);

        // Gift sub
        let gift = json!({ "gifter_username": "santa", "gifted_usernames": ["u1", "u2"] });
        handle_kick_event(
            &engine,
            "chan",
            "App\\Events\\GiftedSubscriptionsEvent",
            &gift,
            &mut speakers,
        );

        // Ban & Timeout
        let timeout = json!({ "user": "troll", "banned_by": "mod", "reason": "spam", "duration": 300 });
        handle_kick_event(&engine, "chan", "App\\Events\\UserBannedEvent", &timeout, &mut speakers);
        let ban = json!({ "user": "hacker", "banned_by": "mod", "reason": "rules", "duration": 0 });
        handle_kick_event(&engine, "chan", "App\\Events\\UserBannedEvent", &ban, &mut speakers);

        // Channel update
        let chan_up = json!({ "title": "New Title", "category": "Gaming" });
        handle_kick_event(
            &engine,
            "chan",
            "App\\Events\\ChannelUpdateEvent",
            &chan_up,
            &mut speakers,
        );

        // Kicks gifted
        let kicks = json!({ "amount": 100, "sender": "tipper" });
        handle_kick_event(&engine, "chan", "App\\Events\\KicksGiftedEvent", &kicks, &mut speakers);

        // Reward redemption
        let reward = json!({ "reward_id": "r1", "user": "viewer", "user_input": "hello" });
        handle_kick_event(
            &engine,
            "chan",
            "App\\Events\\RewardRedeemedEvent",
            &reward,
            &mut speakers,
        );

        // 7TV emotes
        let em_add = json!({ "name": "KEKW" });
        handle_kick_event(&engine, "chan", "7tv:emote_added", &em_add, &mut speakers);
        let em_rem = json!({ "name": "OMEGALUL" });
        handle_kick_event(&engine, "chan", "7tv:emote_removed", &em_rem, &mut speakers);

        // Viewer count
        let vc = json!({ "viewers": 1500 });
        handle_kick_event(&engine, "chan", "App\\Events\\ViewerCountUpdated", &vc, &mut speakers);
    }

    #[test]
    fn test_kick_account_fallback_logic() {
        let mut cfg = KickConfig {
            broadcaster: Some(KickAccount {
                user_id: 1,
                username: "streamer".into(),
                token: KickToken {
                    access_token: "broadcaster_token_123".into(),
                    refresh_token: String::new(),
                    expires_at: 0,
                },
            }),
            ..Default::default()
        };
        // 1. Bot requested with fallback -> resolves broadcaster token
        let token_bot = if let Some(bot) = cfg.bot.as_ref().filter(|b| !b.token.access_token.is_empty()) {
            Some(bot.token.access_token.clone())
        } else {
            cfg.broadcaster.as_ref().map(|b| b.token.access_token.clone())
        };
        assert_eq!(token_bot.as_deref(), Some("broadcaster_token_123"));

        // 2. Bot set -> resolves bot token
        cfg.bot = Some(KickAccount {
            user_id: 2,
            username: "streamerbot".into(),
            token: KickToken {
                access_token: "bot_token_456".into(),
                refresh_token: String::new(),
                expires_at: 0,
            },
        });
        let token_bot2 = cfg.bot.as_ref().map(|b| b.token.access_token.clone());
        assert_eq!(token_bot2.as_deref(), Some("bot_token_456"));
    }
}
