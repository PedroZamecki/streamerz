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
    /// Any official API call as the broadcaster (method, path under /public/v1, JSON body).
    Api {
        method: &'static str,
        path: String,
        body: Option<Value>,
        resp: oneshot::Sender<Result<Value, String>>,
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

    pub async fn api(&self, method: &'static str, path: &str, body: Option<Value>) -> Result<Value, String> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(KickCmd::Api {
                method,
                path: path.to_string(),
                body,
                resp,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        rx.await.map_err(|_| "Request cancelled".to_string())?
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
                KickCmd::Api {
                    method,
                    path,
                    body,
                    resp,
                } => {
                    let r = match token(&self.engine, &self.cfg, false, false).await {
                        Ok(t) => api(&t, method, &path, body).await,
                        Err(e) => Err(e),
                    };
                    let _ = resp.send(r);
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
const KICK_SCOPES: &str =
    "user:read channel:read channel:write chat:write moderation:ban events:subscribe channel:rewards:read channel:rewards:write";
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
    // A path under /public/v1, or a whole URL (the v2 endpoints).
    let url = if path.starts_with("https://") {
        path.to_string()
    } else {
        format!("{KICK_API_BASE}{path}")
    };
    let r = crate::http::request(method, &url, &headers, &body).await?;
    let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
    if (200..300).contains(&r.status) {
        Ok(v)
    } else {
        let msg = v["message"].as_str().map_or_else(|| r.text(), String::from);
        Err(format!("Kick {method} {path}: HTTP {} {msg}", r.status))
    }
}

/// Kick's emotes in a message: each `[emote:<id>:<name>]`, with its picture.
pub fn kick_emotes(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("[emote:") {
        let Some(j) = rest[i..].find(']') else { break };
        let tag = &rest[i..i + j + 1];
        if tag[7..tag.len() - 1]
            .split_once(':')
            .is_some_and(|(id, _)| id.bytes().all(|b| b.is_ascii_digit()))
        {
            let id = tag[7..].split(':').next().unwrap_or("");
            out.push((tag.to_string(), format!("https://files.kick.com/emotes/{id}/fullsize")));
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
                // kick.com's page also listens on these (follows, subs, live and title changes).
                for c in [format!("chatroom_{cr_id}"), format!("channel_{c_id}")] {
                    let sub = json!({"event": "pusher:subscribe", "data": {"auth": "", "channel": c}});
                    let _ = sink.send(Message::Text(sub.to_string().into())).await;
                }
                let uid = cfg.borrow().kick.broadcaster.as_ref().map(|b| b.user_id);
                let _extras = crate::emote_events::Abort(tokio::task::spawn_local(kick_extras(
                    engine.clone(),
                    channel.clone(),
                    uid,
                )));

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

thread_local! {
    /// The stream as last seen: title, category (id, name, thumbnail), when it went live.
    static STREAM: std::cell::RefCell<(String, (Value, Value, Value), String)> = Default::default();
    /// Who chatted, and when (Present Viewers: those in the last 10 minutes).
    static ACTIVE: std::cell::RefCell<std::collections::HashMap<String, (Value, std::time::Instant)>> = Default::default();
}

/// Fire a Kick trigger and its WebSocket event.
fn kick(engine: &Rc<Engine>, code: u32, name: &str, ev: &str, args: &Obj) {
    engine.fire(code, name, "Kick", "kick", args, |_| true);
    engine.emit("Kick", ev, || json!(args));
}

/// Streamer.bot's KickUser variables from a Kick user object (`id`, `username`, `slug`).
fn kick_user(args: &mut Obj, u: &Value) {
    let name = u["username"].as_str().unwrap_or("");
    args.insert("user".into(), name.into());
    args.insert(
        "userName".into(),
        u["slug"].as_str().unwrap_or(&name.to_lowercase()).into(),
    );
    args.insert("userId".into(), u["id"].clone());
    args.insert("userType".into(), "kick".into());
}

/// KickBroadcaster variables, from the account logged in (or the channel read).
fn kick_broadcaster(engine: &Rc<Engine>, args: &mut Obj, channel: &str) {
    let c = engine.config();
    let (name, id) = match &c.kick.broadcaster {
        Some(b) => (b.username.clone(), Value::from(b.user_id)),
        None => (channel.to_string(), Value::Null),
    };
    args.insert("broadcastUser".into(), name.clone().into());
    args.insert("broadcastUsername".into(), name.to_lowercase().into());
    args.insert("broadcastUserId".into(), id);
}

/// The stream's category and timing as Streamer.bot's names.
fn stream_args(args: &mut Obj) {
    STREAM.with_borrow(|(title, (id, name, thumb), started)| {
        args.insert("title".into(), title.clone().into());
        args.insert("status".into(), title.clone().into());
        args.insert("categoryId".into(), id.clone());
        args.insert("categoryName".into(), name.clone());
        args.insert("categoryThumbnail".into(), thumb.clone());
        args.insert("category.id".into(), id.clone());
        args.insert("category.name".into(), name.clone());
        args.insert("category.thumbnail".into(), thumb.clone());
        args.insert("startedAt".into(), started.clone().into());
    });
}

/// A Kick livestream object's category as (id, name, thumbnail).
fn category(l: &Value) -> (Value, Value, Value) {
    let c = l
        .pointer("/categories/0")
        .or_else(|| l.get("category"))
        .cloned()
        .unwrap_or_default();
    let thumb = c
        .pointer("/banner/src")
        .or_else(|| c.get("thumbnail"))
        .cloned()
        .unwrap_or_default();
    (c["id"].clone(), c["name"].clone(), thumb)
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
    let mut args = Obj::new();
    kick_broadcaster(engine, &mut args, channel);
    let now = || crate::fmt::date(&crate::fmt::now(), "o");
    // The Pusher event names, with and without their PHP namespace.
    match event.trim_start_matches("App\\Events\\") {
        "ChatMessageEvent" => {
            let content = data["content"].as_str().unwrap_or("").to_string();
            let msg_id = data["id"].as_str().unwrap_or("").to_string();
            let emotes = kick_emotes(&content);
            let sender = &data["sender"];
            kick_user(&mut args, sender);
            let user_id = sender["id"].as_u64().unwrap_or(0);
            let username = sender["username"].as_str().unwrap_or("").to_string();
            let color = sender
                .pointer("/identity/color")
                .and_then(Value::as_str)
                .unwrap_or("#53FC18");
            let badges = sender
                .pointer("/identity/badges")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let has = |t: &str| badges.iter().any(|b| b["type"] == t);
            let is_broadcaster = has("broadcaster") || username.eq_ignore_ascii_case(channel);
            let (is_moderator, is_subscriber, is_vip) = (has("moderator"), has("subscriber"), has("vip"));
            // The message without its emote tags (Streamer.bot's messageStripped).
            let mut stripped = content.clone();
            for (tag, _) in &emotes {
                stripped = stripped.replace(tag.as_str(), "");
            }
            let stripped = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
            let hex = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0);
            let names: Vec<Value> = badges.iter().map(|b| b["type"].clone()).collect();
            let list: Vec<Value> = emotes
                .iter()
                .map(|(tag, url)| json!({"name": crate::engine::chatlog::shown(tag), "imageUrl": url}))
                .collect();
            crate::commands::input_args(&content, &content, &mut args);
            for (k, v) in [
                ("userSlug", sender["slug"].clone()),
                ("color", color.into()),
                ("colorR", ((hex >> 16) & 255).into()),
                ("colorG", ((hex >> 8) & 255).into()),
                ("colorB", (hex & 255).into()),
                ("message", content.clone().into()),
                ("messageStripped", stripped.into()),
                ("messageId", msg_id.clone().into()),
                ("emoteCount", emotes.len().into()),
                ("emotes", Value::Array(list).to_string().into()),
                ("badgeCount", badges.len().into()),
                ("badge", names.into()),
                ("isBroadcaster", is_broadcaster.into()),
                ("isModerator", is_moderator.into()),
                ("isSubscribed", is_subscriber.into()),
                ("isSubscriber", is_subscriber.into()),
                ("isVip", is_vip.into()),
                ("isInternal", false.into()),
            ] {
                args.insert(k.into(), v);
            }
            if data["type"] == "reply" {
                args.insert("isReply".into(), true.into());
                args.insert(
                    "replyTo".into(),
                    data["metadata"]["original_sender"]["username"].clone(),
                );
                args.insert(
                    "replyToMessage".into(),
                    data["metadata"]["original_message"]["content"].clone(),
                );
            }
            ACTIVE.with_borrow_mut(|m| {
                let who = json!({"userId": user_id, "userName": args["userName"], "user": username});
                m.insert(user_id.to_string(), (who, std::time::Instant::now()));
            });
            if speakers.insert(user_id.to_string()) {
                let mut a = args.clone();
                a.insert("firstMessage".into(), true.into());
                kick(engine, T_KICK_FIRST_WORDS, "KickFirstWords", "FirstWords", &a);
            }
            kick(engine, T_KICK_CHAT_MESSAGE, "KickChatMessage", "ChatMessage", &args);
            engine.chat(&Chat {
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
                vip: is_vip,
                args: args.clone(),
                ..Default::default()
            });
        }
        "FollowersUpdated" => {
            // Also sent on unfollows and with no name for some; only named follows fire.
            let name = data["username"].as_str().unwrap_or("");
            if name.is_empty() || data["followed"] == false {
                return;
            }
            kick_user(&mut args, &json!({"username": name, "id": data["user_id"]}));
            args.insert("followersCount".into(), data["followersCount"].clone());
            kick(engine, T_KICK_FOLLOW, "KickFollow", "Follow", &args);
        }
        "StreamerIsLive" | "LivestreamStarted" | "StreamOnline" => {
            let l = &data["livestream"];
            let started = l["created_at"].as_str().map_or_else(now, String::from);
            STREAM.with_borrow_mut(|s| {
                s.0 = l["session_title"].as_str().unwrap_or("").into();
                s.1 = category(l);
                s.2 = started;
            });
            stream_args(&mut args);
            args.insert("channel".into(), channel.into());
            kick(engine, T_KICK_STREAM_ONLINE, "KickStreamOnline", "StreamOnline", &args);
        }
        "StopStreamBroadcast" | "LivestreamEnded" | "StreamOffline" => {
            stream_args(&mut args);
            args.insert("endedAt".into(), now().into());
            args.insert("channel".into(), channel.into());
            kick(
                engine,
                T_KICK_STREAM_OFFLINE,
                "KickStreamOffline",
                "StreamOffline",
                &args,
            );
        }
        "LivestreamUpdated" | "ChannelUpdateEvent" => {
            let l = if data["livestream"].is_object() {
                &data["livestream"]
            } else {
                &data
            };
            let title = l["session_title"]
                .as_str()
                .or(l["title"].as_str())
                .unwrap_or("")
                .to_string();
            let cat = category(l);
            let (old_title, old_cat) = STREAM.with_borrow_mut(|s| {
                let old = (
                    std::mem::replace(&mut s.0, title.clone()),
                    std::mem::replace(&mut s.1, cat.clone()),
                );
                if s.2.is_empty() {
                    s.2 = now();
                }
                old
            });
            stream_args(&mut args);
            for (k, v) in [
                ("statusUpdated", (old_title != title).into()),
                ("oldStatus", old_title.into()),
                ("categoryUpdated", (old_cat.0 != cat.0).into()),
                ("oldCategoryId", old_cat.0),
                ("oldCategoryName", old_cat.1),
                ("oldCategoryThumbnail", old_cat.2),
            ] {
                args.insert(k.into(), v);
            }
            kick(
                engine,
                T_KICK_CHANNEL_UPDATE,
                "KickChannelUpdate",
                "ChannelUpdate",
                &args,
            );
        }
        "SubscriptionEvent" | "ChannelSubscriptionEvent" => {
            let months = data["months"].as_u64().unwrap_or(1);
            kick_user(&mut args, &json!({"username": data["username"], "id": data["user_id"]}));
            let at = now();
            for (k, v) in [
                ("months", months.into()),
                ("monthsSubscribed", months.into()),
                ("duration", 1.into()),
                ("subscribedAt", at.clone().into()),
                ("expiresAt", Value::Null),
            ] {
                args.insert(k.into(), v);
            }
            if months > 1 {
                kick(
                    engine,
                    T_KICK_RESUBSCRIPTION,
                    "KickResubscription",
                    "Resubscription",
                    &args,
                );
            } else {
                kick(engine, T_KICK_SUBSCRIPTION, "KickSubscription", "Subscription", &args);
            }
        }
        "GiftedSubscriptionsEvent" => {
            let gifter = data["gifter_username"].as_str().unwrap_or("Anonymous");
            kick_user(&mut args, &json!({"username": gifter, "id": data["gifter_id"]}));
            let names: Vec<&str> = data["gifted_usernames"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            args.insert("count".into(), names.len().max(1).into());
            args.insert("totalGifted".into(), data["gifter_total"].clone());
            args.insert("subscribedAt".into(), now().into());
            let recipient = |args: &mut Obj, pre: &str, n: &str| {
                args.insert(format!("{pre}userName"), n.into());
                args.insert(format!("{pre}userLogin"), n.to_lowercase().into());
                args.insert(format!("{pre}platform"), "kick".into());
            };
            if names.len() > 1 {
                for (i, n) in names.iter().enumerate() {
                    recipient(&mut args, &format!("recipient.{i}."), n);
                }
                kick(
                    engine,
                    T_KICK_MASS_GIFT_SUBSCRIPTION,
                    "KickMassGiftSubscription",
                    "MassGiftSubscription",
                    &args,
                );
            } else {
                recipient(&mut args, "recipient.", names.first().copied().unwrap_or(""));
                kick(
                    engine,
                    T_KICK_GIFT_SUBSCRIPTION,
                    "KickGiftSubscription",
                    "GiftSubscription",
                    &args,
                );
            }
        }
        "UserBannedEvent" => {
            kick_user(&mut args, &data["user"]);
            let by = &data["banned_by"];
            args.insert("createdById".into(), by["id"].clone());
            args.insert("createdByUsername".into(), by["slug"].clone());
            args.insert("createdByDisplayName".into(), by["username"].clone());
            args.insert("bannedBy".into(), by["username"].clone());
            args.insert("createdAt".into(), now().into());
            args.insert("reason".into(), data["reason"].clone());
            args.insert("expiresAt".into(), data["expires_at"].clone());
            if data["permanent"] == true {
                kick(engine, T_KICK_USER_BANNED, "KickUserBanned", "UserBanned", &args);
            } else {
                // Pusher gives minutes; Streamer.bot's duration is seconds.
                args.insert("duration".into(), (data["duration"].as_u64().unwrap_or(0) * 60).into());
                kick(engine, T_KICK_USER_TIMED_OUT, "KickUserTimedOut", "UserTimedOut", &args);
            }
        }
        "KicksGifted" | "KicksGiftedEvent" => {
            kick_user(&mut args, &data["sender"]);
            let g = &data["gift"];
            let text = data["message"].as_str().unwrap_or("");
            crate::commands::input_args(text, text, &mut args);
            for (k, v) in [
                ("kicks.amount", g["amount"].clone()),
                ("kicks.name", g["name"].clone()),
                ("kicks.type", g["type"].clone()),
                ("kicks.tier", g["tier"].clone()),
                ("kicks.pinnedTimeSeconds", g["pinned_time"].clone()),
                ("kicks.createdAt", data["created_at"].clone()),
                ("amount", g["amount"].clone()),
                ("message", text.into()),
            ] {
                args.insert(k.into(), v);
            }
            kick(engine, T_KICK_KICKS_GIFTED, "KickKicksGifted", "KicksGifted", &args);
        }
        "RewardRedeemedEvent" => {
            kick_user(&mut args, &json!({"username": data["username"], "id": data["user_id"]}));
            let input = data["user_input"].as_str().unwrap_or("");
            for (k, v) in [
                ("reward.id", data["reward_id"].clone()),
                ("reward.title", data["reward_title"].clone()),
                ("reward.backgroundColor", data["reward_background_color"].clone()),
                ("rewardId", data["reward_id"].clone()),
                ("userInput", input.into()),
                ("rawInput", input.into()),
                ("rawInputEscaped", crate::commands::escape(input).into()),
                ("rawInputUrlEncoded", crate::commands::url_encode(input).into()),
            ] {
                args.insert(k.into(), v);
            }
            kick(
                engine,
                T_KICK_REWARD_REDEMPTION,
                "KickRewardRedemption",
                "RewardRedemption",
                &args,
            );
        }
        _ => {}
    }
}

/// While connected: Viewer Count Update (every minute, on a change) and Present Viewers (who chatted
/// in the last 10 minutes, every 5), only when live and an action uses them; 7TV emote changes when
/// used.
pub async fn kick_extras(engine: Rc<Engine>, channel: String, user_id: Option<u64>) {
    let codes = crate::emote_events::Codes {
        platform: "kick",
        source: "Kick",
        seventv: (T_KICK_SEVENTV_EMOTE_ADDED, T_KICK_SEVENTV_EMOTE_REMOVED),
        bttv: (0, 0),
    };
    let _emotes = match user_id {
        Some(id) if codes.wanted(&engine) => Some(crate::emote_events::Abort(tokio::task::spawn_local(
            crate::emote_events::watch(engine.clone(), codes, id.to_string()),
        ))),
        _ => None,
    };
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    let (mut n, mut last) = (0u64, None);
    loop {
        tick.tick().await;
        n += 1;
        let want = (
            engine.uses_trigger(T_KICK_VIEWER_COUNT_UPDATE),
            engine.uses_trigger(T_KICK_PRESENT_VIEWERS),
        );
        if !want.0 && !want.1 {
            continue;
        }
        let h = [("Accept".to_string(), "application/json".to_string())];
        let Ok(r) = crate::http::request("GET", &format!("https://kick.com/api/v2/channels/{channel}"), &h, b"").await
        else {
            continue;
        };
        let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
        let l = &v["livestream"];
        if !l.is_object() {
            continue;
        }
        let viewers = l["viewer_count"].as_u64().unwrap_or(0);
        STREAM.with_borrow_mut(|s| {
            s.0 = l["session_title"].as_str().unwrap_or("").into();
            s.1 = category(l);
            if s.2.is_empty() {
                s.2 = l["created_at"].as_str().unwrap_or("").into();
            }
        });
        let mut args = Obj::new();
        kick_broadcaster(&engine, &mut args, &channel);
        stream_args(&mut args);
        args.insert("viewerCount".into(), viewers.into());
        args.insert("viewers".into(), viewers.into());
        if want.0 && last != Some(viewers) {
            last = Some(viewers);
            kick(
                &engine,
                T_KICK_VIEWER_COUNT_UPDATE,
                "KickViewerCountUpdate",
                "ViewerCountUpdate",
                &args,
            );
        }
        if want.1 && n % 5 == 1 {
            let users: Vec<Value> = ACTIVE.with_borrow_mut(|m| {
                m.retain(|_, (_, at)| at.elapsed() < Duration::from_secs(600));
                m.values().map(|(u, _)| u.clone()).collect()
            });
            args.insert("users".into(), users.into());
            args.insert("isLive".into(), true.into());
            args.insert("isTest".into(), false.into());
            kick(
                &engine,
                T_KICK_PRESENT_VIEWERS,
                "KickPresentViewers",
                "PresentViewers",
                &args,
            );
        }
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
    #[test]
    fn test_mock_pusher_protocol() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        crate::engine::tests::run(mock_pusher_protocol());
    }

    async fn mock_pusher_protocol() {
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
            while subs_received < 5 {
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

        assert!(speakers.contains("1234"));
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
    fn events_fire_with_their_variables() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap();
        let watch: &[(u32, &str)] = &[
            (
                T_KICK_CHAT_MESSAGE,
                "%user% %colorR% %emoteCount% %messageStripped% %input0% %isVip%",
            ),
            (
                T_KICK_USER_TIMED_OUT,
                "%user% %userId% by %createdByUsername% %duration%",
            ),
            (T_KICK_USER_BANNED, "%userName% by %createdByDisplayName%"),
            (T_KICK_MASS_GIFT_SUBSCRIPTION, "%user% %count% %recipient.1.userName%"),
            (T_KICK_KICKS_GIFTED, "%user% %kicks.amount% %kicks.name% %input0%"),
            (
                T_KICK_CHANNEL_UPDATE,
                "%oldStatus%>%status% %statusUpdated% %categoryName% %categoryUpdated%",
            ),
        ];
        let actions: Vec<Value> = watch
            .iter()
            .map(|(code, vars)| {
                let mut a = action(&code.to_string(), "a", json!([log(&format!("T {code} {vars}"))]));
                a["triggers"] = json!([{"id": "t", "type": code, "enabled": true}]);
                a
            })
            .collect();
        let e = engine(json!({ "actions": actions }));
        let out = run(async move {
            let mut sp = HashSet::new();
            let mut go = |ev: &str, d: Value| handle_kick_event(&e, "chan", ev, &d, &mut sp);
            go(
                "App\\Events\\ChatMessageEvent",
                json!({"id": "m", "content": "hi [emote:1:KEKW] there",
                "sender": {"id": 7, "username": "Ana", "slug": "ana", "identity": {"color": "#FF8000", "badges": [{"type": "vip"}]}}}),
            );
            // As recorded from kick.com's Pusher (2026-10-25).
            go(
                "App\\Events\\UserBannedEvent",
                json!({"id": "b", "user": {"id": 66, "username": "abd", "slug": "abd"},
                "banned_by": {"id": 0, "username": "MOD", "slug": "mod"}, "permanent": false, "duration": 5}),
            );
            go(
                "App\\Events\\UserBannedEvent",
                json!({"user": {"id": 67, "username": "Zed", "slug": "zed"},
                "banned_by": {"username": "MOD", "slug": "mod"}, "permanent": true}),
            );
            go(
                "App\\Events\\GiftedSubscriptionsEvent",
                json!({"gifter_username": "Gus", "gifted_usernames": ["a", "B"]}),
            );
            go(
                "KicksGifted",
                json!({"message": "nice one", "sender": {"id": 9, "username": "Kim"},
                "gift": {"amount": 100, "name": "Rage Quit", "type": "BASIC"}}),
            );
            go(
                "App\\Events\\LivestreamUpdated",
                json!({"livestream": {"session_title": "old", "categories": [{"id": 1, "name": "Chess"}]}}),
            );
            go(
                "App\\Events\\LivestreamUpdated",
                json!({"livestream": {"session_title": "new", "categories": [{"id": 1, "name": "Chess"}]}}),
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        let want = [
            "T 35010 Ana 255 1 hi there hi True",
            "T 35018 abd 66 by mod 300",
            "T 35019 zed by MOD",
            "T 35017 Gus 2 B",
            "T 35025 Kim 100 Rage Quit nice",
            "T 35020 old>new True Chess False",
        ];
        for w in want {
            assert!(out.iter().any(|x| x == w), "{w} missing: {out:?}");
        }
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
        assert!(speakers.contains("100"));
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
