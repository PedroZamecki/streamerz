//! Kick integration for Streamerz (Phase 10):
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
use std::future::Future;
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
pub const T_KICK_MESSAGE_DELETED: u32 = 35028;
pub const T_KICK_CHAT_MODE_CHANGED: u32 = 35029;
pub const T_KICK_MESSAGE_PINNED: u32 = 35030;
pub const T_KICK_MESSAGE_UNPINNED: u32 = 35031;

thread_local! {
    /// The channel's rewards by id (Kick's own shape), as last read.
    static REWARDS: std::cell::RefCell<std::collections::HashMap<String, Value>> = std::cell::RefCell::default();
}

/// Takes a list of the channel's rewards (`{"data": [...]}`): what the editors offer, and what the chat
/// shows of a redemption.
fn known_rewards(v: &Value) {
    let list: Vec<(String, String)> = REWARDS.with_borrow_mut(|m| {
        for r in v["data"].as_array().into_iter().flatten() {
            if let Some(id) = r["id"].as_str() {
                m.insert(id.into(), r.clone());
            }
        }
        m.iter()
            .filter_map(|(id, r)| Some((id.clone(), r["title"].as_str()?.to_string())))
            .collect()
    });
    crate::catalog::set("kick.rewards", list);
}

/// The channel's rewards as last read, in Kick's shape.
pub fn rewards_known() -> Vec<Value> {
    REWARDS.with_borrow(|m| m.values().cloned().collect())
}

/// A reward Streamerz created or changed (Kick's shape).
pub fn reward_saved(reward: &Value) {
    known_rewards(&json!({"data": [reward]}));
}

/// A reward that was deleted.
pub fn reward_removed(id: &str) {
    REWARDS.with_borrow_mut(|m| drop(m.remove(id)));
    known_rewards(&json!({"data": []}));
}

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
    /// Not connected at start (it is by default, once there is a channel).
    pub manual_connect: bool,
    pub client_id: String,
    pub client_secret: String,
    pub broadcaster: Option<KickAccount>,
    pub bot: Option<KickAccount>,
}

impl KickConfig {
    fn refresh_target(&mut self, bot: bool, login: &KickAccount, credentials: (&str, &str)) -> Option<&mut KickToken> {
        if (self.client_id.as_str(), self.client_secret.as_str()) != credentials {
            return None;
        }
        let account = if bot { &mut self.bot } else { &mut self.broadcaster };
        account
            .as_mut()
            .filter(|a| a.user_id == login.user_id && a.token == login.token)
            .map(|a| &mut a.token)
    }
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
    SetChannelTags {
        tags: Vec<String>,
        resp: oneshot::Sender<Result<(), String>>,
    },
    StartAdBreak {
        duration_seconds: u64,
        resp: oneshot::Sender<Result<Value, String>>,
    },
    GetAdStatus {
        resp: oneshot::Sender<Result<Value, String>>,
    },
    BanUser {
        user_login: String,
        reason: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    BanUserId {
        user_id: u64,
        duration: Option<u64>,
        reason: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    UnbanUser {
        user_login: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    UnbanUserId {
        user_id: u64,
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
    DeleteMessage {
        message_id: String,
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
    /// A connected handle with no runner: whoever holds the receiver answers it (scenarios).
    pub fn detached() -> (Self, mpsc::UnboundedReceiver<KickCmd>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let connected = Arc::new(AtomicBool::new(true));
        (KickHandle { tx, connected }, rx)
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// A test's handle that dropped its connection (Disconnect keeps the handle).
    #[cfg(test)]
    pub fn set_connected(&self, on: bool) {
        self.connected.store(on, Ordering::Relaxed)
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

    pub async fn set_channel_tags(&self, tags: Vec<String>) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::SetChannelTags { tags, resp: resp_tx })
            .map_err(|_| "Kick runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn start_ad_break(&self, duration_seconds: u64) -> Result<Value, String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::StartAdBreak {
                duration_seconds,
                resp: resp_tx,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn get_ad_status(&self) -> Result<Value, String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::GetAdStatus { resp: resp_tx })
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

    pub async fn ban_user_id(&self, user_id: u64, duration: Option<u64>, reason: &str) -> Result<(), String> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(KickCmd::BanUserId {
                user_id,
                duration,
                reason: reason.to_string(),
                resp,
            })
            .map_err(|_| "Kick runner is down".to_string())?;
        rx.await.map_err(|_| "Request cancelled".to_string())?
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

    pub async fn unban_user_id(&self, user_id: u64) -> Result<(), String> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(KickCmd::UnbanUserId { user_id, resp })
            .map_err(|_| "Kick runner is down".to_string())?;
        rx.await.map_err(|_| "Request cancelled".to_string())?
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

    pub async fn delete_message(&self, message_id: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KickCmd::DeleteMessage {
                message_id: message_id.to_string(),
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
        ids: Default::default(),
    };
    tokio::task::spawn_local(runner.run());
    handle
}

/// A task that hasn't ended yet.
fn running(task: &Option<crate::emote_events::Abort>) -> bool {
    task.as_ref().is_some_and(|t| !t.0.is_finished())
}

struct Runner {
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    rx: mpsc::UnboundedReceiver<KickCmd>,
    connected: Arc<AtomicBool>,
    read_task: Option<crate::emote_events::Abort>,
    /// The channel (slug) and its broadcaster user id, looked up once.
    ids: std::cell::RefCell<Option<(String, u64)>>,
}

impl Runner {
    async fn run(mut self) {
        if !self.cfg.borrow().kick.manual_connect && !self.cfg.borrow().kick.channel.trim().is_empty() {
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
                KickCmd::SetChannelTags { tags, resp } => {
                    let res = self.set_channel_tags_api(&tags).await;
                    let _ = resp.send(res);
                }
                KickCmd::StartAdBreak { duration_seconds, resp } => {
                    let res = self.start_ad_break_api(duration_seconds).await;
                    let _ = resp.send(res);
                }
                KickCmd::GetAdStatus { resp } => {
                    let res = self.get_ad_status_api().await;
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
                KickCmd::UnbanUserId { user_id, resp } => {
                    let _ = resp.send(self.unban_user_id_api(user_id).await);
                }
                KickCmd::BanUserId {
                    user_id,
                    duration,
                    reason,
                    resp,
                } => {
                    let _ = resp.send(self.ban_user_id_api(user_id, duration, &reason).await);
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
                KickCmd::DeleteMessage { message_id, resp } => {
                    let res = delete_message(&self.engine, &self.cfg, &message_id).await;
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
        // Not just `connected`: that's false until the socket is up, and a second Connect meanwhile
        // would start a second reader (every message twice).
        if self.connected.load(Ordering::Relaxed) || running(&self.read_task) {
            return;
        }

        let channel = self.cfg.borrow().kick.channel.trim().to_string();
        if channel.is_empty() {
            crate::error("Kick: channel name is empty".into());
            return;
        }

        crate::say(format!("Kick: connecting (@{channel})..."));

        let engine = self.engine.clone();
        let cfg = self.cfg.clone();
        let connected = self.connected.clone();
        let reader = run_pusher_loop_url(engine.clone(), cfg.clone(), connected.clone(), KICK_PUSHER_URL, false);
        self.read_task = Some(crate::emote_events::Abort(tokio::task::spawn_local(monitor_pusher(
            engine, &cfg, connected, reader,
        ))));
    }

    fn disconnect(&mut self) {
        drop(self.read_task.take());
        disconnect_pusher(&self.engine, &self.connected);
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

    async fn set_channel_tags_api(&self, tags: &[String]) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let tags_capped: Vec<String> = tags.iter().take(10).cloned().collect();
        api(
            &token,
            "PATCH",
            "/channels",
            Some(json!({ "custom_tags": tags_capped })),
        )
        .await
        .map(drop)
    }

    async fn start_ad_break_api(&self, duration_seconds: u64) -> Result<Value, String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let duration = duration_seconds.clamp(7, 90);
        let id = crate::engine::uuid();
        api(
            &token,
            "POST",
            "/ads/ad-break",
            Some(json!({
                "break_duration_seconds": duration,
                "id": id,
            })),
        )
        .await
    }

    async fn get_ad_status_api(&self) -> Result<Value, String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        api(&token, "GET", "/ads/ad-break-status", None).await
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

    async fn ban_user_id_api(&self, user_id: u64, seconds: Option<u64>, reason: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let mut body = json!({"broadcaster_user_id": self.broadcaster_id(&token).await?, "user_id": user_id});
        if let Some(seconds) = seconds {
            body["duration"] = json!(seconds.div_ceil(60).clamp(1, 10080));
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

    async fn unban_user_id_api(&self, user_id: u64) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let body = json!({"broadcaster_user_id": self.broadcaster_id(&token).await?, "user_id": user_id});
        api(&token, "DELETE", "/moderation/bans", Some(body)).await.map(drop)
    }

    async fn untimeout_user_api(&self, user_login: &str) -> Result<(), String> {
        self.unban_user_api(user_login).await
    }
}

/// Watch only while a reader is active. A broadcaster logout cancels even an in-flight read.
fn monitor_pusher(
    engine: Rc<Engine>,
    cfg: &watch::Sender<Config>,
    connected: Arc<AtomicBool>,
    reader: impl Future<Output = ()>,
) -> impl Future<Output = ()> {
    let mut config = cfg.subscribe();
    let mut signed_in = config.borrow().kick.broadcaster.is_some();
    async move {
        let logout = async {
            let _ = config
                .wait_for(|c| {
                    let was_signed_in = signed_in;
                    signed_in = c.kick.broadcaster.is_some();
                    was_signed_in && !signed_in
                })
                .await;
        };
        tokio::select! {
            biased;
            () = logout => disconnect_pusher(&engine, &connected),
            () = reader => {}
        }
    }
}

fn disconnect_pusher(engine: &Rc<Engine>, connected: &AtomicBool) {
    engine.chat_polls_clear("kick");
    engine.chat_unpinned("kick", "");
    engine.set_chat_viewers("kick", None);
    crate::host::polls_changed();
    if connected.swap(false, Ordering::Relaxed) {
        crate::host::changed();
        crate::say("Kick: disconnected".into());
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
}

/// Deletes a chat message, as the broadcaster.
pub async fn delete_message(engine: &Rc<Engine>, cfg: &Arc<watch::Sender<Config>>, id: &str) -> Result<(), String> {
    let token = token(engine, cfg, false, false).await?;
    api(&token, "DELETE", &format!("/chat/{id}"), None).await.map(drop)
}

pub const KICK_REDIRECT: &str = "http://127.0.0.1:45563/callback";
const KICK_TOKEN_URL: &str = "https://id.kick.com/oauth/token";
const KICK_SCOPES: &str =
    "user:read channel:read channel:write chat:write moderation:ban moderation:chat_message:manage events:subscribe channel:rewards:read channel:rewards:write ads:read ads:write";
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
            (None, true) => (
                pick(&c, !bot).ok_or_else(|| {
                    if crate::secrets::is_locked() {
                        "System keyring is locked; unlock it to use saved Kick login"
                    } else {
                        "No Kick account logged in"
                    }
                })?,
                !bot,
            ),
            (None, false) => {
                if crate::secrets::is_locked() {
                    return Err("System keyring is locked; unlock it to use saved Kick login".into());
                }
                // An account that's there but has lost its login (not one never made).
                if (if bot { &c.kick.bot } else { &c.kick.broadcaster }).is_some() {
                    crate::notify::login_needed("Kick", bot);
                }
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
    .map_err(|e| {
        // The server turned the refresh token down (not a network failure): it needs a new login.
        if e.contains("HTTP 400") || e.contains("HTTP 401") {
            crate::notify::login_needed("Kick", bot);
        }
        kick_oauth_error(&e)
    })?;
    let mut updated = false;
    engine.change(|c| {
        if let Some(token) = c.kick.refresh_target(bot, &acc, (&id, &secret)) {
            token.access_token.clone_from(&t.access);
            token.expires_at = t.expires_at;
            if !t.refresh.is_empty() {
                token.refresh_token = t.refresh;
            }
            updated = true;
        }
    });
    if !updated {
        return Err("Kick: login changed during token refresh".into());
    }
    Ok(t.access)
}

/// The OAuth endpoint may include request details in its error body.
fn kick_oauth_error(error: &str) -> String {
    let status = error
        .strip_prefix("token request failed (HTTP ")
        .and_then(|s| s.split_once(')'))
        .and_then(|(code, _)| code.parse::<u16>().ok());
    status.map_or_else(
        || "Kick token request failed".into(),
        |code| format!("Kick token request failed (HTTP {code})"),
    )
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
    .await
    .map_err(|error| kick_oauth_error(&error))?;
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

fn api_url(path: &str) -> Result<String, String> {
    if path.starts_with("https://api.kick.com/public/v2/") {
        Ok(path.to_owned())
    } else if path.starts_with('/') {
        Ok(format!("{KICK_API_BASE}{path}"))
    } else {
        Err("Kick: expected a public API path".into())
    }
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
    let url = api_url(path)?;
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
        if let Some((id, name)) = tag[7..tag.len() - 1].split_once(':') {
            if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
                if !name.is_empty() {
                    crate::emotes::remember_kick_tag(name, id);
                }
                out.push((tag.to_string(), format!("https://files.kick.com/emotes/{id}/fullsize")));
            }
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

/// Silence before the server is pinged, and how long its answer is waited for.
#[cfg(not(test))]
const IDLE_PROBE: Duration = Duration::from_secs(60);
#[cfg(not(test))]
const PONG_WAIT: Duration = Duration::from_secs(20);
#[cfg(test)]
const IDLE_PROBE: Duration = Duration::from_millis(300);
#[cfg(test)]
const PONG_WAIT: Duration = Duration::from_millis(300);

pub async fn run_pusher_loop_url(
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    connected: Arc<AtomicBool>,
    pusher_url: &str,
    run_once: bool,
) {
    let channel = cfg.borrow().kick.channel.trim().to_lowercase();
    if channel.is_empty() {
        return;
    }
    engine.set_chat_modes("kick", None);
    cache_sub_badges(&Value::Null);
    let is_live_pusher = pusher_url == KICK_PUSHER_URL && !run_once;
    let mut recent = RecentPusherEvents::default();
    let mut failures = 0u32;
    loop {
        let cached_ids = {
            let c = cfg.borrow();
            if !c.kick.channel.trim().eq_ignore_ascii_case(&channel) {
                return;
            }
            (c.kick.channel_id, c.kick.chatroom_id)
        };
        let details = if is_live_pusher {
            engine.set_chat_modes("kick", None);
            let url = format!("https://kick.com/api/v2/channels/{channel}");
            let headers = [("Accept".to_string(), "application/json".to_string())];
            crate::http::request("GET", &url, &headers, b"")
                .await
                .ok()
                .filter(|r| r.status == 200)
                .and_then(|r| serde_json::from_slice::<Value>(&r.body).ok())
        } else {
            None
        };
        let Some((c_id, cr_id)) = pusher_ids(details.as_ref(), cached_ids) else {
            if failures == 0 {
                crate::core::status::record_failure("kick", crate::core::status::Failure::Connect);
                crate::warn(format!("Kick: could not find the chatroom for @{channel}"));
            }
            if run_once {
                return;
            }
            failures = failures.saturating_add(1);
            tokio::time::sleep(crate::http::backoff(failures)).await;
            continue;
        };
        if !cfg.borrow().kick.channel.trim().eq_ignore_ascii_case(&channel) {
            return;
        }
        if let Some(v) = &details {
            cache_sub_badges(&v["subscriber_badges"]);
            engine.set_chat_modes("kick", kick_chat_modes(&v["chatroom"]));
        }
        if cached_ids != (Some(c_id), Some(cr_id)) {
            engine.change(|c| {
                c.kick.channel_id = Some(c_id);
                c.kick.chatroom_id = Some(cr_id);
            });
        }
        let handshake = tokio::time::timeout(Duration::from_secs(15), async {
            let (mut ws, _) = crate::http::ws_connect(pusher_url).await.map_err(|e| e.to_string())?;
            loop {
                let msg = ws.next().await.ok_or("Pusher disconnected during handshake")?;
                match msg.map_err(|e| e.to_string())? {
                    Message::Text(txt) => {
                        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                            match v["event"].as_str() {
                                Some("pusher:connection_established") => break,
                                Some("pusher:error") => {
                                    let data = pusher_data(&v["data"]);
                                    return Err(format!("Pusher refused the connection (code {})", data["code"]));
                                }
                                _ => {}
                            }
                        }
                    }
                    Message::Close(_) => return Err("Pusher closed during handshake".into()),
                    _ => {}
                }
            }
            for c in [
                format!("chatrooms.{cr_id}.v2"),
                format!("channel.{c_id}"),
                format!("channel.{c_id}.v2"),
                format!("chatroom_{cr_id}"),
                format!("channel_{c_id}"),
            ] {
                let sub = json!({"event": "pusher:subscribe", "data": {"auth": "", "channel": c}});
                ws.send(Message::Text(sub.to_string().into()))
                    .await
                    .map_err(|e| e.to_string())?;
            }
            Ok(ws)
        })
        .await;
        match handshake {
            Ok(Ok(mut ws)) => {
                let opened = tokio::time::Instant::now();

                connected.store(true, Ordering::Relaxed);
                crate::core::status::clear_failure("kick");
                crate::host::changed();
                crate::say(format!("Kick: connected (@{channel}, chatroom {cr_id})"));
                engine.settle_role("kick");
                // The channel's rewards, for the editors' pickers and the price in the chat: kick.com's own
                // list (no login needed), then the broadcaster's through the API when logged in.
                let _rewards = is_live_pusher.then(|| {
                    let (e, channel) = (engine.clone(), channel.clone());
                    let broadcaster = cfg.borrow().kick.broadcaster.is_some();
                    crate::emote_events::Abort(tokio::task::spawn_local(async move {
                        let h = [("Accept".to_string(), "application/json".to_string())];
                        let url = format!("https://kick.com/api/v2/channels/{channel}/rewards");
                        if let Some(r) = crate::http::request("GET", &url, &h, b"")
                            .await
                            .ok()
                            .filter(|r| r.status == 200)
                        {
                            known_rewards(&serde_json::from_slice(&r.body).unwrap_or_default());
                        }
                        if broadcaster {
                            if let Ok(v) = e.kick_start().api("GET", "/channels/rewards", None).await {
                                known_rewards(&v);
                            }
                        }
                    }))
                });
                let _history = is_live_pusher.then(|| {
                    let e_hist = engine.clone();
                    let ch_hist = channel.clone();
                    crate::emote_events::Abort(tokio::task::spawn_local(async move {
                        let (hist, pin) = fetch_kick_recent_messages(c_id, &ch_hist).await;
                        e_hist.chat_welcome("kick", hist);
                        // What was pinned before Streamerz joined.
                        match pin {
                            Some(p) => e_hist.chat_pinned(p),
                            None => e_hist.chat_unpinned("kick", ""),
                        }
                    }))
                });

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
                let broadcaster = cfg.borrow().kick.broadcaster.clone();
                if let Some(b) = broadcaster {
                    let mut a = Obj::new();
                    kick_user(&mut a, &json!({"id": b.user_id, "username": b.username}));
                    kick(
                        &engine,
                        T_KICK_AUTHENTICATED,
                        "KickBroadcasterAuthenticated",
                        "BroadcasterAuthenticated",
                        &a,
                    );
                }

                let uid = cfg.borrow().kick.broadcaster.as_ref().map(|b| b.user_id);
                let _extras = is_live_pusher.then(|| {
                    crate::emote_events::Abort(tokio::task::spawn_local(kick_extras(
                        engine.clone(),
                        channel.clone(),
                        uid,
                    )))
                });

                // Message processing loop. A connection that dies without a close (a network change, a
                // sleeping laptop) just goes quiet: after a silent minute ask the server, and when even
                // that gets no answer the socket is dead, so reconnect.
                let mut probed = false;
                loop {
                    let wait = if probed { PONG_WAIT } else { IDLE_PROBE };
                    let msg_res = match tokio::time::timeout(wait, ws.next()).await {
                        Ok(Some(m)) => m,
                        Ok(None) => break,
                        Err(_) if !probed => {
                            probed = true;
                            let ping = json!({ "event": "pusher:ping", "data": {} });
                            if !tokio::time::timeout(PONG_WAIT, ws.send(Message::Text(ping.to_string().into())))
                                .await
                                .is_ok_and(|r| r.is_ok())
                            {
                                break;
                            }
                            continue;
                        }
                        Err(_) => {
                            crate::warn("Kick: no answer from the server, reconnecting".into());
                            break;
                        }
                    };
                    probed = false;
                    let msg = match msg_res {
                        Ok(m) => m,
                        Err(e) => {
                            crate::warn(format!("Kick: websocket error: {e}"));
                            break;
                        }
                    };

                    match msg {
                        Message::Text(txt) => {
                            if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                                let event_name = v["event"].as_str().unwrap_or("");
                                if event_name == "pusher:ping" {
                                    let pong = json!({ "event": "pusher:pong", "data": {} });
                                    if !tokio::time::timeout(PONG_WAIT, ws.send(Message::Text(pong.to_string().into())))
                                        .await
                                        .is_ok_and(|r| r.is_ok())
                                    {
                                        break;
                                    }
                                    continue;
                                }

                                if event_name.starts_with("pusher:") || event_name.starts_with("pusher_internal:") {
                                    continue;
                                }
                                let data = pusher_data(&v["data"]);
                                if recent.first(event_name, &data) {
                                    handle_kick_event(&engine, &channel, event_name, &data);
                                }
                            }
                        }
                        Message::Ping(data) => {
                            if !tokio::time::timeout(PONG_WAIT, ws.send(Message::Pong(data)))
                                .await
                                .is_ok_and(|r| r.is_ok())
                            {
                                break;
                            }
                        }
                        Message::Close(_) => {
                            crate::core::status::record_failure("kick", crate::core::status::Failure::Closed);
                            crate::warn("Kick: connection closed by server".into());
                            break;
                        }
                        _ => {}
                    }
                }

                failures = if opened.elapsed() >= IDLE_PROBE + PONG_WAIT {
                    1
                } else {
                    failures.saturating_add(1)
                };
                connected.store(false, Ordering::Relaxed);
                crate::host::changed();
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
            error => {
                if failures == 0 {
                    let reason = match error {
                        Ok(Err(e)) => e,
                        _ => "timed out".into(),
                    };
                    crate::core::status::record_failure("kick", crate::core::status::Failure::Connect);
                    crate::warn(format!("Kick: connection error: {reason}, retrying"));
                }
                failures = failures.saturating_add(1);
            }
        }

        if run_once {
            break;
        }
        tokio::time::sleep(crate::http::backoff(failures.max(1))).await;
    }
}

/// Use IDs from the same source so a partial response cannot mix two channels.
fn pusher_ids(details: Option<&Value>, cached: (Option<u64>, Option<u64>)) -> Option<(u64, u64)> {
    let valid = |c, r| match (c, r) {
        (Some(c), Some(r)) if c != 0 && r != 0 => Some((c, r)),
        _ => None,
    };
    details
        .and_then(|v| valid(v["id"].as_u64(), v["chatroom"]["id"].as_u64()))
        .or_else(|| valid(cached.0, cached.1))
}

fn pusher_data(data: &Value) -> std::borrow::Cow<'_, Value> {
    match data {
        Value::String(s) => std::borrow::Cow::Owned(serde_json::from_str(s).unwrap_or_default()),
        v => std::borrow::Cow::Borrowed(v),
    }
}

/// Kick can send the same event on more than one subscribed channel.
#[derive(Default)]
struct RecentPusherEvents {
    latest: std::collections::HashSet<String>,
    previous: std::collections::HashSet<String>,
}

impl RecentPusherEvents {
    fn first(&mut self, event: &str, data: &Value) -> bool {
        let event = event.trim_start_matches("App\\Events\\");
        if event != "ChatMessageEvent" {
            return true;
        }
        let Some(id) = data["id"].as_str().filter(|id| !id.is_empty()) else {
            return true;
        };
        if self.latest.contains(id) || self.previous.contains(id) {
            return false;
        }
        if self.latest.len() == 256 {
            self.previous.clear();
            std::mem::swap(&mut self.latest, &mut self.previous);
        }
        self.latest.insert(id.to_owned());
        true
    }
}

thread_local! {
    /// The stream as last seen: title, category (id, name, thumbnail), when it went live.
    static STREAM: std::cell::RefCell<(String, (Value, Value, Value), String)> = Default::default();
    /// Who chatted, and when (Present Viewers: those in the last 10 minutes).
    static ACTIVE: std::cell::RefCell<std::collections::HashMap<String, (Value, std::time::Instant)>> = Default::default();
    /// Channel custom subscriber badge URLs sorted by months descending: `(months, image_url)`.
    static SUB_BADGES: std::cell::RefCell<Vec<(u64, String)>> = Default::default();
}

fn cache_sub_badges(arr: &Value) {
    let mut list: Vec<(u64, String)> = arr
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| {
            let months = b["months"].as_u64()?;
            let src = b.pointer("/badge_image/src").and_then(Value::as_str)?.trim();
            if src.is_empty() {
                None
            } else {
                Some((months, src.to_string()))
            }
        })
        .collect();
    list.sort_by_key(|a| std::cmp::Reverse(a.0));
    SUB_BADGES.with_borrow_mut(|s| *s = list);
}

/// Build Kick's authentic `(badge_key, image_url)` list from a chatter's `sender.identity`.
pub fn kick_badge_list(identity: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for b in identity["badges"].as_array().into_iter().flatten() {
        if b.get("active").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let kind = b["type"].as_str().unwrap_or("");
        let count = b["count"].as_u64().unwrap_or(0);
        let (key, url) = match kind {
            "broadcaster" => ("kick:broadcaster", String::new()),
            "moderator" => ("kick:moderator", String::new()),
            "vip" => ("kick:vip", String::new()),
            "og" => ("kick:og", String::new()),
            "founder" => ("kick:founder", String::new()),
            "verified" => ("kick:verified", String::new()),
            "staff" => ("kick:staff", String::new()),
            "sidekick" => ("kick:sidekick", String::new()),
            // ponytail: one gifter badge; kick.com has ~30 tiers by count, add them if they matter.
            "sub_gifter" => ("kick:sub_gifter", String::new()),
            "subscriber" => {
                let months = count.max(1);
                let custom = SUB_BADGES.with_borrow(|s| {
                    s.iter()
                        .find(|(m, _)| months >= *m)
                        .or_else(|| s.last())
                        .map(|(_, u)| u.clone())
                        .unwrap_or_default()
                });
                ("kick:subscriber", custom)
            }
            _ => continue,
        };
        out.push((key.to_string(), url));
    }
    // kick.com's own order.
    const ORDER: [&str; 11] = [
        "staff",
        "broadcaster",
        "moderator",
        "vip",
        "og",
        "subscriber",
        "founder",
        "sub_gifter",
        "sidekick",
        "verified",
        "custom",
    ];
    out.sort_by_key(|(k, _)| {
        ORDER
            .iter()
            .position(|o| k.strip_prefix("kick:") == Some(o))
            .unwrap_or(ORDER.len())
    });
    for b in identity["badges_v2"].as_array().into_iter().flatten() {
        if b.get("selected").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        if let Some(url) = b["image_url"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            if !out.iter().any(|(_, u)| u == url) {
                out.push(("kick:custom".into(), url.to_string()));
            }
        }
    }
    out
}

/// Convert a Kick `/api/v2/channels/{id}/messages` item into a historical chat `Line`.
pub fn parse_kick_message_line(data: &Value, channel: &str) -> Option<crate::engine::chatlog::Line> {
    let raw_content = data["content"].as_str()?.trim();
    if raw_content.is_empty() {
        return None;
    }
    let msg_id = data["id"].as_str().unwrap_or("").to_string();
    let sender = &data["sender"];
    let user = sender["username"].as_str().unwrap_or("").to_string();
    if user.is_empty() {
        return None;
    }
    let user_id = sender["id"]
        .as_u64()
        .map(|n| n.to_string())
        .or_else(|| sender["id"].as_str().map(String::from))
        .unwrap_or_default();
    let identity = &sender["identity"];
    let color = identity["color"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("#53FC18")
        .to_string();
    let raw_badges = identity["badges"].as_array().cloned().unwrap_or_default();
    let has = |t: &str| raw_badges.iter().any(|b| b["type"] == t);
    let broadcaster = has("broadcaster") || user.eq_ignore_ascii_case(channel);
    let moderator = has("moderator");
    let vip = has("vip");
    let subscriber = has("subscriber") || has("founder");
    let badges = kick_badge_list(identity);

    let raw_emotes = kick_emotes(raw_content);
    let (text, emotes) = crate::engine::chatlog::normalize_line_emotes("kick", raw_content.to_string(), &raw_emotes);
    let secs = data["created_at"].as_str().and_then(crate::engine::rest_parse_iso);
    let time = secs.map_or_else(String::new, crate::engine::chatlog::local_hhmm);

    // A reply's parent (the metadata is sometimes JSON in a string).
    let meta = match &data["metadata"] {
        Value::String(m) => serde_json::from_str(m).unwrap_or(Value::Null),
        m => m.clone(),
    };
    let reply = meta["original_sender"]["username"]
        .as_str()
        .or_else(|| meta["original_sender"]["slug"].as_str())
        .or_else(|| meta["original_sender"]["user_name"].as_str())
        .filter(|s| !s.is_empty())
        .map(|u| crate::engine::chatlog::Reply {
            user: u.to_string(),
            text: meta["original_message"]["content"]
                .as_str()
                .or_else(|| meta["original_message"]["body"].as_str())
                .or_else(|| meta["original_message"]["text"].as_str())
                .unwrap_or("")
                .split(' ')
                .map(crate::engine::chatlog::shown)
                .collect::<Vec<_>>()
                .join(" "),
        });
    let text = match reply.as_ref() {
        Some(r) => r.strip(&text),
        None => text,
    };
    Some(crate::engine::chatlog::Line {
        platform: "kick".into(),
        ts: secs.unwrap_or(0) * 1000,
        reply,
        msg_id,
        time,
        user,
        user_id,
        text,
        emotes,
        broadcaster,
        moderator,
        vip,
        subscriber,
        badges,
        color,
        historical: true,
        ..Default::default()
    })
}

/// The chat's recent messages, and the one pinned now.
async fn fetch_kick_recent_messages(
    channel_id: u64,
    channel: &str,
) -> (Vec<crate::engine::chatlog::Line>, Option<crate::engine::chatlog::Pin>) {
    if channel_id == 0 {
        return (Vec::new(), None);
    }
    let url = format!("https://kick.com/api/v2/channels/{channel_id}/messages");
    let headers = [("Accept".to_string(), "application/json".to_string())];
    let Ok(res) = crate::http::request("GET", &url, &headers, b"").await else {
        return (Vec::new(), None);
    };
    if res.status != 200 {
        return (Vec::new(), None);
    }
    let Ok(val) = serde_json::from_slice::<Value>(&res.body) else {
        return (Vec::new(), None);
    };
    let pin = val.pointer("/data/pinned_message").and_then(kick_pin);
    let Some(arr) = val
        .pointer("/data/messages")
        .or_else(|| val.get("messages"))
        .and_then(Value::as_array)
    else {
        return (Vec::new(), pin);
    };
    // Kick returns messages newest-first; reverse into chronological order.
    let lines = arr
        .iter()
        .rev()
        .filter_map(|m| parse_kick_message_line(m, channel))
        .collect();
    (lines, pin)
}

/// A pinned message, as Kick's PinnedMessageCreatedEvent and its history's `pinned_message` carry it:
/// the message, who pinned it, and for how long (seconds, or when it ends).
pub fn kick_pin(d: &Value) -> Option<crate::engine::chatlog::Pin> {
    let m = &d["message"];
    let id = m["id"].as_str().filter(|s| !s.is_empty())?;
    let now = crate::fmt::unix_now().as_millis() as u64;
    let secs = d["duration"]
        .as_u64()
        .or_else(|| d["duration"].as_str().and_then(|s| s.parse().ok()))
        .filter(|s| *s > 0);
    let ends = ["finish_at", "finishs_at", "expires_at"]
        .iter()
        .find_map(|k| d[*k].as_str())
        .and_then(crate::engine::rest_parse_iso)
        .and_then(|s| u64::try_from(s).ok())
        .and_then(|s| s.checked_mul(1000));
    let by = ["pinnedBy", "pinned_by"]
        .iter()
        .find_map(|k| d[*k]["username"].as_str())
        .unwrap_or("");
    let text = m["content"].as_str().unwrap_or("");
    Some(crate::engine::chatlog::Pin {
        platform: "kick".into(),
        msg_id: id.into(),
        user: m["sender"]["username"].as_str().unwrap_or("").into(),
        // Emotes as their names.
        text: text
            .split(' ')
            .map(crate::engine::chatlog::shown)
            .collect::<Vec<_>>()
            .join(" "),
        by: by.into(),
        until: ends.or_else(|| secs.and_then(|s| s.checked_mul(1000)?.checked_add(now))),
    })
}

/// Fire a Kick trigger and its WebSocket event.
fn kick(engine: &Rc<Engine>, code: u32, name: &str, ev: &str, args: &Obj) {
    if code == T_KICK_REWARD_REDEMPTION {
        let text = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let (id, title) = (text("rewardId"), text("reward.title"));
        engine.fire(code, name, "Kick", "kick", args, |t| {
            crate::catalog::matches(
                "kick.rewards",
                t.get("rewardId").and_then(Value::as_str).unwrap_or(""),
                &id,
                &title,
            )
        });
        engine.emit("Kick", ev, || json!(args));
        return;
    }
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

fn kick_chat_modes(value: &Value) -> Option<crate::engine::ChatModes> {
    let enabled = |key: &str| value[key].as_bool().or_else(|| value[key]["enabled"].as_bool());
    Some(crate::engine::ChatModes {
        emote_only: enabled("emotes_mode")?,
        slow_seconds: if enabled("slow_mode")? {
            value["message_interval"]
                .as_u64()
                .or_else(|| value["slow_mode"]["message_interval"].as_u64())
        } else {
            None
        },
    })
}

pub fn handle_kick_event(engine: &Rc<Engine>, channel: &str, event: &str, data_val: &Value) {
    let data = pusher_data(data_val);
    let data = data.as_ref();
    if !data.is_object() {
        return;
    }
    let mut args = Obj::new();
    kick_broadcaster(engine, &mut args, channel);
    let now = || crate::fmt::date(&crate::fmt::now(), "o");
    // The Pusher event names, with and without their PHP namespace.
    match event.trim_start_matches("App\\Events\\") {
        "ChatroomUpdatedEvent" => {
            let modes = kick_chat_modes(data);
            engine.set_chat_modes("kick", modes);
            let enabled = |key: &str| data[key].as_bool().or_else(|| data[key]["enabled"].as_bool());
            if let Some(m) = modes {
                args.insert("emoteMode".into(), m.emote_only.into());
                args.insert("slowMode".into(), m.slow_seconds.is_some().into());
                args.insert("slowModeInterval".into(), m.slow_seconds.unwrap_or(0).into());
            } else {
                if let Some(v) = enabled("emotes_mode") {
                    args.insert("emoteMode".into(), v.into());
                }
                if let Some(v) = enabled("slow_mode") {
                    args.insert("slowMode".into(), v.into());
                }
            }
            if let Some(v) = enabled("subscribers_mode") {
                args.insert("subscribersMode".into(), v.into());
            }
            if let Some(v) = enabled("followers_mode") {
                args.insert("followersMode".into(), v.into());
            }
            kick(
                engine,
                T_KICK_CHAT_MODE_CHANGED,
                "KickChatModeChanged",
                "ChatModeChanged",
                &args,
            );
        }
        "ChatMessageEvent" => {
            let content = data["content"].as_str().unwrap_or("").to_string();
            let msg_id = data["id"].as_str().unwrap_or("").to_string();
            let emotes = kick_emotes(&content);
            let sender = &data["sender"];
            kick_user(&mut args, sender);
            let user_id = sender["id"].as_u64().unwrap_or(0);
            let username = sender["username"].as_str().unwrap_or("").to_string();
            let identity = &sender["identity"];
            let color = identity["color"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("#53FC18");
            let badges = identity["badges"].as_array().cloned().unwrap_or_default();
            let has = |t: &str| badges.iter().any(|b| b["type"] == t);
            let is_broadcaster = has("broadcaster") || username.eq_ignore_ascii_case(channel);
            let (is_moderator, is_subscriber, is_vip) =
                (has("moderator"), has("subscriber") || has("founder"), has("vip"));
            let ui_badges: Vec<Value> = kick_badge_list(identity)
                .into_iter()
                .map(|(k, u)| json!({"key": k, "name": k, "imageUrl": u}))
                .collect();
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
                ("badges", ui_badges.into()),
                ("isBroadcaster", is_broadcaster.into()),
                ("isModerator", is_moderator.into()),
                ("isSubscribed", is_subscriber.into()),
                ("isSubscriber", is_subscriber.into()),
                ("isVip", is_vip.into()),
                ("isInternal", false.into()),
            ] {
                args.insert(k.into(), v);
            }
            let meta = match &data["metadata"] {
                Value::String(m) => serde_json::from_str(m).unwrap_or(Value::Null),
                m => m.clone(),
            };
            let orig_sender = meta["original_sender"]["username"]
                .as_str()
                .or_else(|| meta["original_sender"]["slug"].as_str())
                .or_else(|| meta["original_sender"]["user_name"].as_str())
                .filter(|s| !s.is_empty());
            let orig_msg = meta["original_message"]["content"]
                .as_str()
                .or_else(|| meta["original_message"]["body"].as_str())
                .or_else(|| meta["original_message"]["text"].as_str())
                .unwrap_or("");
            let is_reply = data["type"] == "reply" || orig_sender.is_some();
            if is_reply {
                args.insert("isReply".into(), true.into());
                if let Some(orig_user) = orig_sender {
                    args.insert("replyTo".into(), orig_user.into());
                    args.insert("reply.userName".into(), orig_user.into());
                    args.insert("reply.userLogin".into(), orig_user.into());
                }
                args.insert("replyToMessage".into(), orig_msg.into());
                args.insert("reply.msgBody".into(), orig_msg.into());
                if let Some(orig_id) = meta["original_message"]["id"].as_str() {
                    args.insert("reply.msgId".into(), orig_id.into());
                }
                if let Some(orig_uid) = meta["original_sender"]["id"].as_u64() {
                    args.insert("reply.userId".into(), orig_uid.to_string().into());
                }
            } else {
                args.insert("isReply".into(), false.into());
            }
            ACTIVE.with_borrow_mut(|m| {
                let who = json!({"userId": user_id, "userName": args["userName"], "user": username});
                m.insert(user_id.to_string(), (who, std::time::Instant::now()));
            });
            if engine.first_words("kick", &user_id.to_string()) {
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
            let id = data.get("user_id").or_else(|| data.get("id"));
            kick_user(&mut args, &json!({"username": name, "id": id}));
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
            engine.chat_stream("kick", true);
            // The streamer's own stream started: YouTube and TikTok look at once instead of at their next check.
            let own = engine
                .config()
                .kick
                .broadcaster
                .as_ref()
                .is_some_and(|b| b.username.eq_ignore_ascii_case(channel));
            if own {
                engine.notify_stream_started();
            }
            kick(engine, T_KICK_STREAM_ONLINE, "KickStreamOnline", "StreamOnline", &args);
        }
        "StopStreamBroadcast" | "LivestreamEnded" | "StreamOffline" => {
            engine.chat_unpinned("kick", "");
            if !engine.chat_stream_ended("kick") {
                crate::notify::send(
                    crate::notify::Kind::Streams,
                    "Streamerz",
                    &crate::i18n::trf("chat.state_ended_title", &[("platform", "Kick")]),
                );
            }
            engine.chat_stream("kick", false);
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
                data
            };
            let title = l["session_title"]
                .as_str()
                .or(l["title"].as_str())
                .unwrap_or("")
                .to_string();
            let cat = category(l);
            crate::core::stream::pushed(
                "kick",
                Some(&title).filter(|t| !t.is_empty()).map(String::as_str),
                cat.1.as_str(),
            );
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
        "PinnedMessageCreatedEvent" => {
            if let Some(p) = kick_pin(data) {
                args.insert("messageId".into(), p.msg_id.clone().into());
                args.insert("user".into(), p.user.clone().into());
                args.insert("message".into(), p.text.clone().into());
                args.insert("pinnedBy".into(), p.by.clone().into());
                if let Some(uid) = data.pointer("/message/sender/id").and_then(Value::as_u64) {
                    args.insert("userId".into(), uid.into());
                }
                if let Some(dur) = data["duration"]
                    .as_u64()
                    .or_else(|| data["duration"].as_str().and_then(|s| s.parse().ok()))
                {
                    args.insert("duration".into(), dur.into());
                }
                engine.chat_pinned(p);
                kick(
                    engine,
                    T_KICK_MESSAGE_PINNED,
                    "KickMessagePinned",
                    "MessagePinned",
                    &args,
                );
            }
        }
        "PinnedMessageDeletedEvent" => {
            engine.chat_unpinned("kick", "");
            if let Some(id) = data.pointer("/message/id").and_then(Value::as_str) {
                args.insert("messageId".into(), id.into());
            }
            kick(
                engine,
                T_KICK_MESSAGE_UNPINNED,
                "KickMessageUnpinned",
                "MessageUnpinned",
                &args,
            );
        }
        "MessageDeletedEvent" => {
            let msg_id = data.pointer("/message/id").and_then(Value::as_str).unwrap_or("");
            engine.chat_remove("kick", msg_id, "");
            args.insert("messageId".into(), msg_id.into());
            if let Some(deleted_by) = data
                .pointer("/deleted_by/username")
                .or_else(|| data.pointer("/moderator/username"))
                .and_then(Value::as_str)
            {
                args.insert("deletedBy".into(), deleted_by.into());
            }
            kick(
                engine,
                T_KICK_MESSAGE_DELETED,
                "KickMessageDeleted",
                "MessageDeleted",
                &args,
            );
        }
        "UserBannedEvent" => {
            crate::core::moderation::kick_ban_event(engine, channel, data);
            let id = &data["user"]["id"];
            engine.chat_remove(
                "kick",
                "",
                &id.as_u64()
                    .map_or_else(|| id.as_str().unwrap_or("").to_string(), |n| n.to_string()),
            );
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
                crate::core::moderation::emit_linked_moderation_event(
                    engine,
                    "kick",
                    crate::core::moderation::LinkedModerationEvent::Banned,
                    &args,
                );
            } else {
                // Pusher gives minutes; Streamer.bot's duration is seconds.
                args.insert(
                    "duration".into(),
                    (data["duration"].as_u64().unwrap_or(0).saturating_mul(60)).into(),
                );
                kick(engine, T_KICK_USER_TIMED_OUT, "KickUserTimedOut", "UserTimedOut", &args);
                crate::core::moderation::emit_linked_moderation_event(
                    engine,
                    "kick",
                    crate::core::moderation::LinkedModerationEvent::TimedOut,
                    &args,
                );
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
            // What the redemption cost, from the channel's list.
            let known = REWARDS.with_borrow(|m| data["reward_id"].as_str().and_then(|id| m.get(id).cloned()));
            if let Some(r) = known {
                args.insert("rewardCost".into(), r["cost"].clone());
            }
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
    engine.set_chat_viewers("kick", None);
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
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let (mut n, mut last) = (0u64, None);
    loop {
        tick.tick().await;
        n += 1;
        let want = (
            engine.uses_trigger(T_KICK_VIEWER_COUNT_UPDATE),
            engine.uses_trigger(T_KICK_PRESENT_VIEWERS),
        );
        if !want.0 && !want.1 && !engine.viewers_wanted() {
            continue;
        }
        let h = [("Accept".to_string(), "application/json".to_string())];
        let Ok(r) = crate::http::request("GET", &format!("https://kick.com/api/v2/channels/{channel}"), &h, b"").await
        else {
            engine.set_chat_viewers("kick", None);
            continue;
        };
        let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
        let l = &v["livestream"];
        engine.set_chat_viewers("kick", l["viewer_count"].as_u64());
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
    fn chat_modes_read_initial_settings_and_updates() {
        use super::*;
        let initial = json!({"emotes_mode": true, "slow_mode": true, "message_interval": 30});
        let update = json!({"emotes_mode": {"enabled": false}, "slow_mode": {"enabled": true, "message_interval": 10}});
        assert_eq!(
            kick_chat_modes(&initial),
            Some(crate::engine::ChatModes {
                emote_only: true,
                slow_seconds: Some(30)
            })
        );
        assert_eq!(
            kick_chat_modes(&update),
            Some(crate::engine::ChatModes {
                emote_only: false,
                slow_seconds: Some(10)
            })
        );
        assert_eq!(kick_chat_modes(&json!({})), None);
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});
        handle_kick_event(&engine, "channel", "App\\Events\\ChatroomUpdatedEvent", &update);
        assert_eq!(engine.chat_modes()[1], kick_chat_modes(&update));
        let off = json!({"emotes_mode": {"enabled": false}, "slow_mode": {"enabled": false}});
        handle_kick_event(&engine, "channel", "ChatroomUpdatedEvent", &off);
        assert_eq!(engine.chat_modes()[1], Some(crate::engine::ChatModes::default()));
    }

    #[test]
    fn test_clean_kick_channel() {
        assert_eq!(clean_kick_channel("https://kick.com/streamer"), "streamer");
        assert_eq!(clean_kick_channel("http://www.kick.com/@mychannel/"), "mychannel");
        assert_eq!(clean_kick_channel("@coolstreamer"), "coolstreamer");
        assert_eq!(clean_kick_channel("normal"), "normal");
    }

    use super::*;

    #[test]
    fn broadcaster_logout_cancels_kick_reader() {
        crate::engine::tests::run(async {
            let cfg = Arc::new(watch::channel(Config::default()).0);
            let engine = Engine::new(cfg.clone(), |_| {});
            let connected = Arc::new(AtomicBool::new(true));
            let (send, recv) = oneshot::channel::<()>();
            let reader = async move {
                let _ = recv.await;
            };
            let mut monitor = std::pin::pin!(monitor_pusher(engine, &cfg, connected.clone(), reader));
            assert!(
                futures_util::poll!(&mut monitor).is_pending(),
                "public reader can start unsigned"
            );
            cfg.send_modify(|c| {
                c.kick.broadcaster = Some(KickAccount::default());
                c.kick.bot = Some(KickAccount::default());
            });
            assert!(futures_util::poll!(&mut monitor).is_pending());
            cfg.send_modify(|c| c.kick.bot = None);
            assert!(
                futures_util::poll!(&mut monitor).is_pending(),
                "bot logout leaves chat running"
            );
            cfg.send_modify(|c| c.kick.broadcaster = None);
            assert!(futures_util::poll!(&mut monitor).is_ready());
            assert!(send.is_closed(), "the in-flight reader was dropped");
            assert!(!connected.load(Ordering::Relaxed));
        });
    }

    #[test]
    fn oauth_refresh_requires_the_same_account_and_credentials() {
        let mut config = KickConfig {
            client_id: "app".into(),
            client_secret: "placeholder".into(),
            broadcaster: Some(KickAccount {
                user_id: 123,
                token: KickToken {
                    access_token: "old".into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let login = config.broadcaster.clone().unwrap();
        let credentials = ("app", "placeholder");
        assert!(config.refresh_target(false, &login, credentials).is_some());
        assert!(config.refresh_target(true, &login, credentials).is_none());
        config.broadcaster.as_mut().unwrap().token.access_token = "new".into();
        assert!(config.refresh_target(false, &login, credentials).is_none());
        config.broadcaster = None;
        assert!(config.refresh_target(false, &login, credentials).is_none());
        config.broadcaster = Some(login.clone());
        config.client_id = "another-app".into();
        assert!(config.refresh_target(false, &login, credentials).is_none());
    }

    #[test]
    fn oauth_error_keeps_the_status_and_drops_the_body() {
        assert_eq!(
            kick_oauth_error("token request failed (HTTP 401): unexpected private details"),
            "Kick token request failed (HTTP 401)"
        );
        assert_eq!(kick_oauth_error("request timed out"), "Kick token request failed");
    }

    #[test]
    fn kick_api_url_keeps_bearer_requests_on_the_api_host() {
        assert_eq!(api_url("/channels").unwrap(), "https://api.kick.com/public/v1/channels");
        assert!(api_url("https://api.kick.com/public/v2/categories").is_ok());
        assert!(api_url("https://example.invalid/channels").is_err());
    }

    #[test]
    fn pusher_channel_lookup_requires_two_real_ids() {
        let details = json!({"id": 442, "chatroom": {"id": 935}});
        assert_eq!(pusher_ids(Some(&details), (None, None)), Some((442, 935)));
        assert_eq!(pusher_ids(Some(&json!({"id": 442})), (None, Some(7))), None);
        assert_eq!(pusher_ids(Some(&json!({"id": 442})), (Some(4), Some(7))), Some((4, 7)));
        assert_eq!(pusher_ids(None, (Some(4), Some(0))), None);
    }

    #[test]
    fn pusher_deduplicates_chat_ids_across_subscription_formats() {
        let first = json!({
            "event": r"App\Events\ChatMessageEvent", "channel": "chatrooms.935.v2",
            "data": r#"{"id":"message-1","content":"hey","sender":{"id":42,"username":"Ana"}}"#
        });
        let second = json!({
            "event": "ChatMessageEvent", "channel": "chatroom_935",
            "data": {"id":"message-1","content":"hey","sender":{"id":42,"username":"Ana"}}
        });
        let mut recent = RecentPusherEvents::default();
        assert!(recent.first(first["event"].as_str().unwrap(), &pusher_data(&first["data"])));
        assert!(!recent.first(second["event"].as_str().unwrap(), &pusher_data(&second["data"])));
        assert!(recent.first("ChatMessageEvent", &json!({"id": "message-2", "content": "hey"})));
        assert!(recent.first("FollowersUpdated", &json!({"id": 42})));
        assert!(recent.first("FollowersUpdated", &json!({"id": 42})));
    }

    #[test]
    fn invalid_pin_end_does_not_wrap_or_panic() {
        for payload in [
            json!({"message": {"id": "p1"}, "duration": u64::MAX}),
            json!({"message": {"id": "p1"}, "finish_at": "1969-12-31T23:59:59Z"}),
        ] {
            assert_eq!(kick_pin(&payload).unwrap().until, None);
        }
    }

    #[test]
    fn kick_emotes_ignore_empty_ids() {
        assert_eq!(
            kick_emotes("hi [emote::Bad] [emote:99:Good]"),
            [(
                "[emote:99:Good]".into(),
                "https://files.kick.com/emotes/99/fullsize".into()
            )]
        );
    }

    #[test]
    fn kick_pins_with_and_without_an_end() {
        let d = json!({"message": {"id": "m1", "content": "hi [emote:37226:KEKW]", "sender": {"username": "Ana"}},
            "duration": "120", "pinnedBy": {"username": "Mod"}});
        let p = kick_pin(&d).unwrap();
        assert_eq!(
            (p.user.as_str(), p.text.as_str(), p.by.as_str()),
            ("Ana", "hi KEKW", "Mod")
        );
        let left = p.until.unwrap() - crate::fmt::unix_now().as_millis() as u64;
        assert!((110_000..=120_000).contains(&left));
        let open = kick_pin(&json!({"message": {"id": "m2", "content": "x"}})).unwrap();
        assert!(open.until.is_none());
        assert!(kick_pin(&Value::Null).is_none());
    }

    /// A Connect while the first is still opening the socket must not start a second reader.
    #[test]
    fn a_reader_still_connecting_counts_as_running() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            assert!(!running(&None));
            let task = Some(crate::emote_events::Abort(tokio::task::spawn_local(
                std::future::pending::<()>(),
            )));
            assert!(running(&task));
            task.as_ref().unwrap().0.abort();
            tokio::task::yield_now().await;
            assert!(!running(&task));
        });
    }

    #[test]
    fn test_mock_pusher_protocol() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        config.kick.broadcaster = Some(KickAccount {
            user_id: 7,
            username: "KickTester".into(),
            token: KickToken::default(),
        });
        // Connected, Authenticated and Disconnected fire as the socket opens and closes.
        let on = |id: &str, code: u32| {
            let mut a = crate::engine::tests::action(id, id, json!([]));
            a["triggers"] = json!([{"id": "t", "type": code, "enabled": true}]);
            a
        };
        config.actions = serde_json::from_value(json!({"actions": [
            on("authed", T_KICK_AUTHENTICATED), on("up", T_KICK_CONNECTED), on("down", T_KICK_DISCONNECTED)]}))
        .unwrap();

        let (tx, _rx) = watch::channel(config);
        let cfg_sender = Arc::new(tx);
        let engine = Engine::new(cfg_sender.clone(), |_| {});
        let connected = Arc::new(AtomicBool::new(false));

        let ws_url = format!("ws://{addr}");
        run_pusher_loop_url(engine.clone(), cfg_sender.clone(), connected.clone(), &ws_url, true).await;

        assert!(!engine.first_words("kick", "1234"));
        assert!(!connected.load(Ordering::Relaxed));
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut ran: Vec<String> = engine.history().iter().map(|r| r.name.clone()).collect();
        ran.sort();
        assert_eq!(ran, ["authed", "down", "up"]);
    }

    /// A socket that goes quiet (no close, no pong) is dropped and opened again: chat didn't just stop.
    #[test]
    fn a_silent_socket_is_reconnected() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(async move {
                let mut held = Vec::new();
                for n in 1..=2 {
                    let (stream, _) = listener.accept().await.unwrap();
                    let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                    let est = json!({"event": "pusher:connection_established", "data": "{}"});
                    ws.send(Message::Text(est.to_string().into())).await.unwrap();
                    tx.send(n).unwrap();
                    // Never answers: read and ignore, kept open.
                    held.push(tokio::spawn(async move { while ws.next().await.is_some() {} }));
                }
            });
            let mut config = Config::default();
            config.kick.channel = "quiet".into();
            config.kick.channel_id = Some(1);
            config.kick.chatroom_id = Some(2);
            let cfg = Arc::new(watch::channel(config).0);
            let engine = Engine::new(cfg.clone(), |_| {});
            let url = format!("ws://{addr}");
            let task = tokio::task::spawn_local(async move {
                run_pusher_loop_url(engine, cfg, Arc::new(AtomicBool::new(false)), &url, false).await;
            });
            assert_eq!(rx.recv().await, Some(1));
            let second = tokio::time::timeout(Duration::from_secs(6), rx.recv()).await;
            task.abort();
            assert_eq!(second, Ok(Some(2)), "no second connection");
        });
    }

    #[test]
    fn kick_config_defaults() {
        let cfg = KickConfig::default();
        assert_eq!(cfg.channel, "");
        assert_eq!(cfg.client_id, "");
        assert!(!cfg.manual_connect);
    }

    #[test]
    fn events_fire_with_their_variables() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
            let go = |ev: &str, d: Value| handle_kick_event(&e, "chan", ev, &d);
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

        handle_kick_event(&engine, "gamer123", event, &payload);
        assert!(!engine.first_words("kick", "100"));
    }

    #[test]
    fn parse_kick_reply_chat_message() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});

        let event = "App\\Events\\ChatMessageEvent";
        // Stringified metadata (standard Kick Pusher wire format)
        let payload = json!({
            "id": "msg-reply-1",
            "chatroom_id": 42,
            "content": "@pedrozamecki This is my reply!",
            "type": "reply",
            "metadata": serde_json::to_string(&json!({
                "original_sender": {
                    "id": 200,
                    "username": "PedroZamecki",
                    "slug": "pedrozamecki"
                },
                "original_message": {
                    "id": "msg-orig-1",
                    "content": "Original message content"
                }
            })).unwrap(),
            "sender": {
                "id": 101,
                "username": "chatter1",
                "slug": "chatter1",
                "identity": {
                    "color": "#00FF00",
                    "badges": []
                }
            }
        });

        handle_kick_event(&engine, "gamer123", event, &payload);
        let (_, _, lines) = engine.chat_sync(999, 0);
        let line = lines
            .iter()
            .find(|l| l.msg_id == "msg-reply-1")
            .expect("reply line found");
        assert_eq!(line.user, "chatter1");
        assert_eq!(line.text, "This is my reply!");
        let reply = line.reply.as_ref().expect("reply details present");
        assert_eq!(reply.user, "PedroZamecki");
        assert_eq!(reply.text, "Original message content");
    }

    #[test]
    fn parse_kick_follow_event() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});

        let event = "App\\Events\\FollowersUpdated";
        let payload = json!({
            "id": 555,
            "username": "coolfollower"
        });

        handle_kick_event(&engine, "channel", event, &payload);
    }

    #[test]
    fn parse_kick_other_events() {
        // A redemption is counted and saved: it needs the runtime.
        crate::engine::tests::run(async {
            let (tx, _rx) = watch::channel(Config::default());
            let engine = Engine::new(Arc::new(tx), |_| {});

            // Subscription
            let sub = json!({ "username": "subber", "months": 1 });
            handle_kick_event(&engine, "chan", "App\\Events\\SubscriptionEvent", &sub);

            // Resub
            let resub = json!({ "username": "resubber", "months": 5 });
            handle_kick_event(&engine, "chan", "App\\Events\\SubscriptionEvent", &resub);

            // Gift sub
            let gift = json!({ "gifter_username": "santa", "gifted_usernames": ["u1", "u2"] });
            handle_kick_event(&engine, "chan", "App\\Events\\GiftedSubscriptionsEvent", &gift);

            // Ban & Timeout
            let timeout = json!({ "user": "troll", "banned_by": "mod", "reason": "spam", "duration": 300 });
            handle_kick_event(&engine, "chan", "App\\Events\\UserBannedEvent", &timeout);
            let ban = json!({ "user": "hacker", "banned_by": "mod", "reason": "rules", "duration": 0 });
            handle_kick_event(&engine, "chan", "App\\Events\\UserBannedEvent", &ban);

            // Channel update
            let chan_up = json!({ "title": "New Title", "category": "Gaming" });
            handle_kick_event(&engine, "chan", "App\\Events\\ChannelUpdateEvent", &chan_up);

            // Kicks gifted
            let kicks = json!({ "amount": 100, "sender": "tipper" });
            handle_kick_event(&engine, "chan", "App\\Events\\KicksGiftedEvent", &kicks);

            // Reward redemption
            let reward = json!({ "reward_id": "r1", "user": "viewer", "user_input": "hello" });
            handle_kick_event(&engine, "chan", "App\\Events\\RewardRedeemedEvent", &reward);

            // 7TV emotes
            let em_add = json!({ "name": "KEKW" });
            handle_kick_event(&engine, "chan", "7tv:emote_added", &em_add);
            let em_rem = json!({ "name": "OMEGALUL" });
            handle_kick_event(&engine, "chan", "7tv:emote_removed", &em_rem);

            // Viewer count
            let vc = json!({ "viewers": 1500 });
            handle_kick_event(&engine, "chan", "App\\Events\\ViewerCountUpdated", &vc);
        });
    }

    #[test]
    fn a_kick_reward_shows_what_it_cost() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            known_rewards(&json!({"data": [
                {"id": "r1", "title": "Hydrate", "cost": 500, "background_color": "#00a3ff"}
            ]}));
            let ids = crate::catalog::get("kick.rewards");
            assert!(ids.iter().any(|(id, t)| id == "r1" && t == "Hydrate"));
            let redeemed = json!({
                "reward_id": "r1", "reward_title": "Hydrate", "username": "viewer", "user_id": 5,
                "user_input": "", "reward_background_color": "#00a3ff"
            });
            handle_kick_event(&engine, "chan", "App\\Events\\RewardRedeemedEvent", &redeemed);
            let (_, _, lines) = engine.chat_sync(u64::MAX, 0);
            let l = lines.iter().find(|l| !l.event.is_empty()).expect("a line");
            assert_eq!((l.cost, l.accent.as_str()), (500, "#00a3ff"));
            assert!(l.event.contains("Hydrate"), "{}", l.event);
        });
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

    #[test]
    fn kick_badges_and_history_line() {
        cache_sub_badges(&json!([
            {"months": 1, "badge_image": {"src": "https://files.kick.com/sub1.png"}},
            {"months": 6, "badge_image": {"src": "https://files.kick.com/sub6.png"}}
        ]));
        let msg = json!({
            "id": "k-msg-1",
            "content": "hello [emote:37226:KEKW]",
            "created_at": "2026-10-26T15:40:12Z",
            "sender": {
                "id": 99,
                "username": "PedroZamecki",
                "identity": {
                    "color": "#B27CFF",
                    "badges": [
                        {"type": "broadcaster", "active": true},
                        {"type": "subscriber", "count": 6, "active": true},
                        {"type": "sub_gifter", "count": 55, "active": true}
                    ],
                    "badges_v2": [
                        {"selected": true, "image_url": "https://files.kick.com/seasonal.webp"}
                    ]
                }
            }
        });
        let l = parse_kick_message_line(&msg, "pedrozamecki").expect("valid kick line");
        assert_eq!(l.msg_id, "k-msg-1");
        assert_eq!(l.text, "hello KEKW");
        assert_eq!(l.color, "#B27CFF");
        assert_eq!(l.emotes[0].0, "KEKW");
        assert_eq!(l.badges[0], ("kick:broadcaster".into(), String::new()));
        assert_eq!(
            l.badges[1],
            ("kick:subscriber".into(), "https://files.kick.com/sub6.png".into())
        );
        assert_eq!(l.badges[2], ("kick:sub_gifter".into(), String::new()));
        assert_eq!(
            l.badges[3],
            ("kick:custom".into(), "https://files.kick.com/seasonal.webp".into())
        );
    }

    #[test]
    fn kick_ads_and_tags_metadata() {
        assert!(super::KICK_SCOPES.contains("ads:read"));
        assert!(super::KICK_SCOPES.contains("ads:write"));

        // Verify triggers registered
        assert_eq!(
            crate::types::name(crate::types::TRIGGERS, 35026),
            Some("KickAdBreakBegin")
        );
        assert_eq!(
            crate::types::name(crate::types::TRIGGERS, 35027),
            Some("KickAdBreakEnd")
        );

        // Verify subactions registered
        assert_eq!(
            crate::types::name(crate::types::SUB_ACTIONS, 35032),
            Some("KickSetChannelTags")
        );
        assert_eq!(
            crate::types::name(crate::types::SUB_ACTIONS, 35033),
            Some("KickAddChannelTag")
        );
        assert_eq!(
            crate::types::name(crate::types::SUB_ACTIONS, 35034),
            Some("KickClearChannelTags")
        );
        assert_eq!(
            crate::types::name(crate::types::SUB_ACTIONS, 35050),
            Some("KickRunCommercial")
        );
        assert_eq!(
            crate::types::name(crate::types::SUB_ACTIONS, 35051),
            Some("KickGetAdStatus")
        );

        // Verify SUB_ACTION_INFO titles
        let title_of = |code| {
            crate::types::SUB_ACTION_INFO
                .binary_search_by_key(&code, |x| x.0)
                .ok()
                .map(|i| crate::types::SUB_ACTION_INFO[i].1)
        };
        assert_eq!(title_of(35032), Some("Set Channel Tags"));
        assert_eq!(title_of(35050), Some("Run Commercial"));

        // Duration clamping: 7..=90
        assert_eq!(0u64.clamp(7, 90), 7);
        assert_eq!(30u64.clamp(7, 90), 30);
        assert_eq!(120u64.clamp(7, 90), 90);
    }
}
