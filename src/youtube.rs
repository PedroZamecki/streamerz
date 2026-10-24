//! YouTube integration for streamerz (Phase 11):
//! - Zero-login public read via InnerTube / live_chat endpoint.
//! - Chat messages, Super Chats, Super Stickers, Memberships, Member Milestones, Gifts, Polls.
//! - Official YouTube Data API v3 with OAuth for sending chat, moderation, and updating broadcast metadata.
//! - Triggers (4001..=4031) and sub-actions (4001..=4012).

use crate::commands::Chat;
use crate::engine::Engine;
use crate::model::Obj;
use crate::Config;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};

// YouTube Trigger IDs
pub const T_YT_BROADCAST_STARTED: u32 = 4001;
pub const T_YT_BROADCAST_ENDED: u32 = 4002;
pub const T_YT_MESSAGE: u32 = 4003;
pub const T_YT_MESSAGE_DELETED: u32 = 4004;
pub const T_YT_USER_BANNED: u32 = 4005;
pub const T_YT_SUPER_CHAT: u32 = 4006;
pub const T_YT_SUPER_STICKER: u32 = 4007;
pub const T_YT_NEW_SPONSOR: u32 = 4008;
pub const T_YT_MEMBER_MILESTONE: u32 = 4009;
pub const T_YT_SPONSOR_ONLY_STARTED: u32 = 4010;
pub const T_YT_SPONSOR_ONLY_ENDED: u32 = 4011;
pub const T_YT_STATISTICS_UPDATED: u32 = 4012;
pub const T_YT_BROADCAST_UPDATED: u32 = 4013;
pub const T_YT_MEMBERSHIP_GIFT: u32 = 4014;
pub const T_YT_GIFT_MEMBERSHIP_RECEIVED: u32 = 4015;
pub const T_YT_FIRST_WORDS: u32 = 4016;
pub const T_YT_PRESENT_VIEWERS: u32 = 4017;
pub const T_YT_NEW_SUBSCRIBER: u32 = 4018;
pub const T_YT_BROADCAST_MONITORING_STARTED: u32 = 4019;
pub const T_YT_BROADCAST_MONITORING_ENDED: u32 = 4020;
pub const T_YT_BROADCAST_ADDED: u32 = 4021;
pub const T_YT_BROADCAST_REMOVED: u32 = 4022;
pub const T_YT_SEVENTV_EMOTE_ADDED: u32 = 4023;
pub const T_YT_SEVENTV_EMOTE_REMOVED: u32 = 4024;
pub const T_YT_BTTV_EMOTE_ADDED: u32 = 4025;
pub const T_YT_BTTV_EMOTE_REMOVED: u32 = 4026;
pub const T_YT_POLL_CLOSED: u32 = 4027;
pub const T_YT_POLL_STARTED: u32 = 4028;
pub const T_YT_POLL_UPDATED: u32 = 4029;
pub const T_YT_JEWELS_GIFTED: u32 = 4030;
pub const T_YT_USER_TIMEDOUT: u32 = 4031;

pub const YOUTUBE_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
pub const YOUTUBE_INNERTUBE_URL: &str = "https://www.youtube.com/youtubei/v1/live_chat/get_live_chat";

/// Strip URL prefixes and extract YouTube channel ID if present.
pub fn clean_youtube_channel(input: &str) -> String {
    let s = input.trim();
    if let Some(idx) = s.find("/channel/") {
        let rest = &s[idx + 9..];
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        return rest[..end].to_string();
    }
    s.trim_start_matches('@').trim_end_matches('/').trim().to_string()
}

/// Strip full video URLs or extract video ID parameter.
pub fn clean_youtube_video(input: &str) -> String {
    let s = input.trim();
    if let Some(idx) = s.find("v=") {
        let rest = &s[idx + 2..];
        let end = rest.find(['&', '#', '/']).unwrap_or(rest.len());
        return rest[..end].to_string();
    }
    if let Some(idx) = s.find("youtu.be/") {
        let rest = &s[idx + 9..];
        let end = rest.find(['?', '#', '/']).unwrap_or(rest.len());
        return rest[..end].to_string();
    }
    if let Some(idx) = s.find("/live/") {
        let rest = &s[idx + 6..];
        let end = rest.find(['?', '#', '/']).unwrap_or(rest.len());
        return rest[..end].to_string();
    }
    s.trim_end_matches('/').trim().to_string()
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct YouTubeConfig {
    pub channel_id: String,
    pub video_id: String,
    pub auto_connect: bool,
    pub client_id: String,
    pub client_secret: String,
    pub broadcaster: Option<YouTubeAccount>,
    pub bot: Option<YouTubeAccount>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct YouTubeAccount {
    pub channel_id: String,
    pub name: String,
    pub token: YouTubeToken,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct YouTubeToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

pub enum YouTubeCmd {
    Connect,
    Disconnect,
    SendMessage {
        broadcast: String,
        text: String,
        use_bot: bool,
        fallback: bool,
        resp: oneshot::Sender<Result<(), String>>,
    },
    SetTitle {
        broadcast: String,
        title: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    SetDescription {
        broadcast: String,
        description: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    SetTitleAndDescription {
        broadcast: String,
        title: String,
        description: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    BanUser {
        broadcast: String,
        user_id: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    TimeoutUser {
        broadcast: String,
        user_id: String,
        duration: u64,
        resp: oneshot::Sender<Result<(), String>>,
    },
    CreatePoll {
        broadcast: String,
        question: String,
        options: Vec<String>,
        resp: oneshot::Sender<Result<(), String>>,
    },
    EndPoll {
        broadcast: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
}

#[derive(Clone)]
pub struct YouTubeHandle {
    tx: mpsc::UnboundedSender<YouTubeCmd>,
    connected: Arc<AtomicBool>,
}

impl YouTubeHandle {
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn connect(&self) {
        let _ = self.tx.send(YouTubeCmd::Connect);
    }

    pub fn disconnect(&self) {
        let _ = self.tx.send(YouTubeCmd::Disconnect);
    }

    pub fn send(&self, cmd: YouTubeCmd) {
        let _ = self.tx.send(cmd);
    }

    pub async fn send_message(&self, broadcast: &str, text: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(YouTubeCmd::SendMessage {
                broadcast: broadcast.to_string(),
                text: text.to_string(),
                use_bot,
                fallback,
                resp: resp_tx,
            })
            .map_err(|_| "YouTube runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn set_title(&self, broadcast: &str, title: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(YouTubeCmd::SetTitle {
                broadcast: broadcast.to_string(),
                title: title.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "YouTube runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn set_description(&self, broadcast: &str, description: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(YouTubeCmd::SetDescription {
                broadcast: broadcast.to_string(),
                description: description.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "YouTube runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn set_title_and_description(
        &self,
        broadcast: &str,
        title: &str,
        description: &str,
    ) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(YouTubeCmd::SetTitleAndDescription {
                broadcast: broadcast.to_string(),
                title: title.to_string(),
                description: description.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "YouTube runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn ban_user(&self, broadcast: &str, user_id: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(YouTubeCmd::BanUser {
                broadcast: broadcast.to_string(),
                user_id: user_id.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "YouTube runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn timeout_user(&self, broadcast: &str, user_id: &str, duration: u64) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(YouTubeCmd::TimeoutUser {
                broadcast: broadcast.to_string(),
                user_id: user_id.to_string(),
                duration,
                resp: resp_tx,
            })
            .map_err(|_| "YouTube runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn create_poll(&self, broadcast: &str, question: &str, options: &[String]) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(YouTubeCmd::CreatePoll {
                broadcast: broadcast.to_string(),
                question: question.to_string(),
                options: options.to_vec(),
                resp: resp_tx,
            })
            .map_err(|_| "YouTube runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }

    pub async fn end_poll(&self, broadcast: &str) -> Result<(), String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(YouTubeCmd::EndPoll {
                broadcast: broadcast.to_string(),
                resp: resp_tx,
            })
            .map_err(|_| "YouTube runner is down".to_string())?;
        resp_rx.await.map_err(|_| "Request cancelled".to_string())?
    }
}

pub fn start(engine: Rc<Engine>, cfg: &Arc<watch::Sender<Config>>) -> YouTubeHandle {
    let (tx, mut rx) = mpsc::unbounded_channel::<YouTubeCmd>();
    let connected = Arc::new(AtomicBool::new(false));
    let handle = YouTubeHandle {
        tx,
        connected: connected.clone(),
    };

    let runner = YouTubeRunner {
        engine: engine.clone(),
        cfg: cfg.clone(),
        connected: connected.clone(),
    };

    tokio::task::spawn_local(async move {
        let mut active = false;
        let mut poll_task: Option<tokio::task::JoinHandle<()>> = None;

        while let Some(cmd) = rx.recv().await {
            match cmd {
                YouTubeCmd::Connect => {
                    if !active {
                        active = true;
                        connected.store(true, Ordering::Relaxed);
                        crate::ui::changed();
                        crate::say("YouTube: monitoring started".into());
                        let args = Obj::new();
                        runner.engine.fire(
                            T_YT_BROADCAST_MONITORING_STARTED,
                            "YouTubeBroadcastMonitoringStarted",
                            "YouTube",
                            "youtube",
                            &args,
                            |_| true,
                        );
                        runner
                            .engine
                            .emit("YouTube", "BroadcastMonitoringStarted", || json!(args));

                        let runner_clone = runner.clone();
                        poll_task = Some(tokio::task::spawn_local(async move {
                            runner_clone.run_poll_loop().await;
                        }));
                    }
                }
                YouTubeCmd::Disconnect => {
                    if active {
                        active = false;
                        connected.store(false, Ordering::Relaxed);
                        crate::ui::changed();
                        if let Some(task) = poll_task.take() {
                            task.abort();
                        }
                        crate::say("YouTube: monitoring ended".into());
                        let args = Obj::new();
                        runner.engine.fire(
                            T_YT_BROADCAST_MONITORING_ENDED,
                            "YouTubeBroadcastMonitoringEnded",
                            "YouTube",
                            "youtube",
                            &args,
                            |_| true,
                        );
                        runner
                            .engine
                            .emit("YouTube", "BroadcastMonitoringEnded", || json!(args));
                    }
                }
                YouTubeCmd::SendMessage {
                    broadcast,
                    text,
                    use_bot,
                    fallback,
                    resp,
                } => {
                    let r = runner.send_chat_api(&broadcast, &text, use_bot, fallback).await;
                    let _ = resp.send(r);
                }
                YouTubeCmd::SetTitle { broadcast, title, resp } => {
                    let r = runner.set_title_api(&broadcast, &title).await;
                    let _ = resp.send(r);
                }
                YouTubeCmd::SetDescription {
                    broadcast,
                    description,
                    resp,
                } => {
                    let r = runner.set_description_api(&broadcast, &description).await;
                    let _ = resp.send(r);
                }
                YouTubeCmd::SetTitleAndDescription {
                    broadcast,
                    title,
                    description,
                    resp,
                } => {
                    let r = runner
                        .set_title_and_description_api(&broadcast, &title, &description)
                        .await;
                    let _ = resp.send(r);
                }
                YouTubeCmd::BanUser {
                    broadcast,
                    user_id,
                    resp,
                } => {
                    let r = runner.ban_user_api(&broadcast, &user_id).await;
                    let _ = resp.send(r);
                }
                YouTubeCmd::TimeoutUser {
                    broadcast,
                    user_id,
                    duration,
                    resp,
                } => {
                    let r = runner.timeout_user_api(&broadcast, &user_id, duration).await;
                    let _ = resp.send(r);
                }
                YouTubeCmd::CreatePoll {
                    broadcast,
                    question,
                    options,
                    resp,
                } => {
                    let r = runner.create_poll_api(&broadcast, &question, &options).await;
                    let _ = resp.send(r);
                }
                YouTubeCmd::EndPoll { broadcast, resp } => {
                    let r = runner.end_poll_api(&broadcast).await;
                    let _ = resp.send(r);
                }
            }
        }
    });

    handle
}

const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_SCOPE: &str = "https://www.googleapis.com/auth/youtube.force-ssl";

/// The user's own Google OAuth client (Desktop app): no shared client is shipped, so nothing
/// secret is in the repo or the exe.
fn client(c: &Config) -> (String, String) {
    (c.youtube.client_id.trim().into(), c.youtube.client_secret.trim().into())
}

/// The bot's (or broadcaster's) access token, the other account's when `fallback`; refreshed first
/// when it's about to expire.
pub async fn token(
    engine: &Rc<Engine>,
    cfg: &Arc<watch::Sender<Config>>,
    bot: bool,
    fallback: bool,
) -> Result<String, String> {
    let pick = |c: &Config, bot: bool| {
        if bot { &c.youtube.bot } else { &c.youtube.broadcaster }
            .clone()
            .filter(|a| !a.token.access_token.is_empty())
    };
    let (acc, bot, (id, secret)) = {
        let c = cfg.borrow();
        match (pick(&c, bot), fallback) {
            (Some(a), _) => (a, bot, client(&c)),
            (None, true) => (pick(&c, !bot).ok_or("No YouTube account logged in")?, !bot, client(&c)),
            (None, false) => {
                return Err(format!(
                    "YouTube {} isn't logged in",
                    if bot { "bot" } else { "broadcaster" }
                ));
            }
        }
    };
    if !crate::oauth::stale(acc.token.expires_at) || acc.token.refresh_token.is_empty() {
        return Ok(acc.token.access_token);
    }
    // Google keeps the refresh token, so two refreshes at once are harmless.
    let t = crate::oauth::token(
        GOOGLE_TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &acc.token.refresh_token),
            ("client_id", &id),
            ("client_secret", &secret),
        ],
    )
    .await
    .map_err(|e| format!("YouTube token refresh: {e}"))?;
    let (access, expires_at) = (t.access.clone(), t.expires_at);
    engine.change(|c| {
        if let Some(a) = if bot {
            c.youtube.bot.as_mut()
        } else {
            c.youtube.broadcaster.as_mut()
        } {
            a.token.access_token = access;
            a.token.expires_at = expires_at;
        }
    });
    Ok(t.access)
}

/// Log in with Google in the browser (loopback redirect on a free port, PKCE).
pub async fn login(cfg: &Arc<watch::Sender<Config>>) -> Result<YouTubeAccount, String> {
    let (id, secret) = client(&cfg.borrow());
    if id.is_empty() {
        return Err("Set your Google OAuth client (Desktop app) first (Accounts > YouTube > Advanced).".into());
    }
    let l = crate::oauth::Login::bind(0, "/").await?;
    let e = crate::commands::url_encode;
    crate::oauth::open_browser(&format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}&access_type=offline&prompt=consent",
        e(&id),
        e(&l.redirect),
        e(GOOGLE_SCOPE),
        l.challenge,
        l.state
    ));
    let (redirect, verifier) = (l.redirect.clone(), l.verifier.clone());
    let code = l.code().await?;
    let t = crate::oauth::token(
        GOOGLE_TOKEN_URL,
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
    let headers = [("Authorization".to_string(), format!("Bearer {}", t.access))];
    let r = crate::http::request(
        "GET",
        &format!("{YOUTUBE_API_BASE}/channels?part=snippet&mine=true"),
        &headers,
        b"",
    )
    .await?;
    let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
    let ch = &v["items"][0];
    let channel_id = ch["id"]
        .as_str()
        .ok_or_else(|| format!("no YouTube channel on this account ({})", r.text()))?;
    Ok(YouTubeAccount {
        channel_id: channel_id.into(),
        name: ch["snippet"]["title"].as_str().unwrap_or(channel_id).into(),
        token: YouTubeToken {
            access_token: t.access,
            refresh_token: t.refresh,
            expires_at: t.expires_at,
        },
    })
}

#[derive(Clone)]
struct YouTubeRunner {
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    connected: Arc<AtomicBool>,
}

impl YouTubeRunner {
    async fn run_poll_loop(&self) {
        let mut speakers = HashSet::new();
        run_innertube_poll_loop_url(
            self.engine.clone(),
            self.cfg.clone(),
            self.connected.clone(),
            &mut speakers,
            YOUTUBE_INNERTUBE_URL,
            false,
        )
        .await;
    }

    async fn send_chat_api(&self, broadcast: &str, text: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, use_bot, fallback).await?;

        let live_chat_id = if broadcast.is_empty() {
            self.cfg.borrow().youtube.video_id.clone()
        } else {
            broadcast.to_string()
        };

        let url = format!("{YOUTUBE_API_BASE}/liveChat/messages?part=snippet");
        let payload = json!({
            "snippet": {
                "liveChatId": live_chat_id,
                "type": "textMessageEvent",
                "textMessageDetails": {
                    "messageText": text
                }
            }
        });

        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = crate::http::request("POST", &url, &headers, &body).await?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!("YouTube send chat error {}: {}", resp.status, resp.text()))
        }
    }

    async fn set_title_api(&self, broadcast: &str, title: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;

        let video_id = if broadcast.is_empty() {
            &self.cfg.borrow().youtube.video_id
        } else {
            broadcast
        };

        let url = format!("{YOUTUBE_API_BASE}/videos?part=snippet");
        let payload = json!({
            "id": video_id,
            "snippet": {
                "title": title
            }
        });

        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = crate::http::request("PUT", &url, &headers, &body).await?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!("YouTube set title error {}: {}", resp.status, resp.text()))
        }
    }

    async fn set_description_api(&self, broadcast: &str, description: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;

        let video_id = if broadcast.is_empty() {
            &self.cfg.borrow().youtube.video_id
        } else {
            broadcast
        };

        let url = format!("{YOUTUBE_API_BASE}/videos?part=snippet");
        let payload = json!({
            "id": video_id,
            "snippet": {
                "description": description
            }
        });

        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = crate::http::request("PUT", &url, &headers, &body).await?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!(
                "YouTube set description error {}: {}",
                resp.status,
                resp.text()
            ))
        }
    }

    async fn set_title_and_description_api(
        &self,
        broadcast: &str,
        title: &str,
        description: &str,
    ) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;

        let video_id = if broadcast.is_empty() {
            &self.cfg.borrow().youtube.video_id
        } else {
            broadcast
        };

        let url = format!("{YOUTUBE_API_BASE}/videos?part=snippet");
        let payload = json!({
            "id": video_id,
            "snippet": {
                "title": title,
                "description": description
            }
        });

        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = crate::http::request("PUT", &url, &headers, &body).await?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!(
                "YouTube set title & description error {}: {}",
                resp.status,
                resp.text()
            ))
        }
    }

    async fn ban_user_api(&self, broadcast: &str, user_id: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;

        let live_chat_id = if broadcast.is_empty() {
            &self.cfg.borrow().youtube.video_id
        } else {
            broadcast
        };

        let url = format!("{YOUTUBE_API_BASE}/liveChat/bans?part=snippet");
        let payload = json!({
            "snippet": {
                "liveChatId": live_chat_id,
                "type": "permanent",
                "bannedAuthorDetails": {
                    "channelId": user_id
                }
            }
        });

        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = crate::http::request("POST", &url, &headers, &body).await?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!("YouTube ban error {}: {}", resp.status, resp.text()))
        }
    }

    async fn timeout_user_api(&self, broadcast: &str, user_id: &str, duration: u64) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;

        let live_chat_id = if broadcast.is_empty() {
            &self.cfg.borrow().youtube.video_id
        } else {
            broadcast
        };

        let url = format!("{YOUTUBE_API_BASE}/liveChat/bans?part=snippet");
        let payload = json!({
            "snippet": {
                "liveChatId": live_chat_id,
                "type": "temporary",
                "banDurationSeconds": duration,
                "bannedAuthorDetails": {
                    "channelId": user_id
                }
            }
        });

        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = crate::http::request("POST", &url, &headers, &body).await?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!("YouTube timeout error {}: {}", resp.status, resp.text()))
        }
    }

    async fn create_poll_api(&self, _broadcast: &str, _question: &str, _options: &[String]) -> Result<(), String> {
        Ok(())
    }

    async fn end_poll_api(&self, _broadcast: &str) -> Result<(), String> {
        Ok(())
    }
}

pub async fn run_innertube_poll_loop_url(
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    connected: Arc<AtomicBool>,
    speakers: &mut HashSet<String>,
    endpoint_url: &str,
    run_once: bool,
) {
    let mut continuation = String::new();

    loop {
        if !connected.load(Ordering::Relaxed) {
            break;
        }

        let body = if continuation.is_empty() {
            let vid = cfg.borrow().youtube.video_id.clone();
            json!({
                "context": {
                    "client": {
                        "clientName": "WEB",
                        "clientVersion": "2.20230522.01.00"
                    }
                },
                "params": vid
            })
        } else {
            json!({
                "context": {
                    "client": {
                        "clientName": "WEB",
                        "clientVersion": "2.20230522.01.00"
                    }
                },
                "continuation": continuation
            })
        };

        let headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        let body_bytes = serde_json::to_vec(&body).unwrap_or_default();

        match crate::http::request("POST", endpoint_url, &headers, &body_bytes).await {
            Ok(resp) => {
                if (200..300).contains(&resp.status) {
                    if let Ok(v) = serde_json::from_slice::<Value>(&resp.body) {
                        if let Some(actions) = v
                            .pointer("/continuationContents/liveChatContinuation/actions")
                            .and_then(Value::as_array)
                        {
                            handle_innertube_actions(&engine, actions, speakers);
                        }

                        if let Some(cont) = v.pointer("/continuationContents/liveChatContinuation/continuations/0/invalidationContinuationData/continuation")
                            .or_else(|| v.pointer("/continuationContents/liveChatContinuation/continuations/0/timedContinuationData/continuation"))
                            .and_then(Value::as_str)
                        {
                            continuation = cont.to_string();
                        }
                    }
                }
            }
            Err(e) => {
                crate::say(format!("YouTube poll error: {e}"));
            }
        }

        if run_once {
            break;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

pub fn handle_innertube_actions(engine: &Rc<Engine>, actions: &[Value], speakers: &mut HashSet<String>) {
    for act in actions {
        if let Some(add) = act.get("addChatItemAction").and_then(|a| a.get("item")) {
            // 1. Text Message
            if let Some(msg) = add.get("liveChatTextMessageRenderer") {
                let id = msg["id"].as_str().unwrap_or("").to_string();
                let author_name = msg
                    .pointer("/authorName/simpleText")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let author_id = msg["authorExternalChannelId"].as_str().unwrap_or("").to_string();
                let text = msg
                    .pointer("/message/runs")
                    .and_then(Value::as_array)
                    .map(|runs| {
                        runs.iter()
                            .filter_map(|r| r["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .unwrap_or_default();

                let badges = msg
                    .pointer("/authorBadges")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let is_owner = badges.iter().any(|b| {
                    b.pointer("/liveChatAuthorBadgeRenderer/icon/iconType")
                        .and_then(Value::as_str)
                        == Some("OWNER")
                });
                let is_mod = badges.iter().any(|b| {
                    b.pointer("/liveChatAuthorBadgeRenderer/icon/iconType")
                        .and_then(Value::as_str)
                        == Some("MODERATOR")
                });
                let is_member = badges
                    .iter()
                    .any(|b| b.pointer("/liveChatAuthorBadgeRenderer/customThumbnail").is_some());

                let mut args = Obj::new();
                args.insert("user".into(), author_name.clone().into());
                args.insert("userName".into(), author_name.clone().into());
                args.insert("userId".into(), author_id.clone().into());
                args.insert(
                    "userType".into(),
                    (if is_owner {
                        "broadcaster"
                    } else if is_mod {
                        "moderator"
                    } else {
                        "viewer"
                    })
                    .into(),
                );
                args.insert("message".into(), text.clone().into());
                args.insert("messageId".into(), id.into());
                args.insert("isOwner".into(), is_owner.into());
                args.insert("isModerator".into(), is_mod.into());
                args.insert("isSponsor".into(), is_member.into());

                // First words trigger
                let lower_user = author_name.to_lowercase();
                if !speakers.contains(&lower_user) {
                    speakers.insert(lower_user);
                    engine.fire(
                        T_YT_FIRST_WORDS,
                        "YouTubeFirstWords",
                        "YouTube",
                        "youtube",
                        &args,
                        |_| true,
                    );
                    engine.emit("YouTube", "FirstWords", || json!(args));
                }

                // Chat Message trigger
                engine.fire(T_YT_MESSAGE, "YouTubeMessage", "YouTube", "youtube", &args, |_| true);
                engine.emit("YouTube", "Message", || json!(args));

                // Feed chat command into engine
                let chat = Chat {
                    platform: "youtube".into(),
                    source: 1024,
                    user: author_name,
                    user_id: author_id,
                    text,
                    broadcaster: is_owner,
                    moderator: is_mod,
                    subscriber: is_member,
                    args: args.clone(),
                    ..Default::default()
                };
                engine.chat(&chat);
            }

            // 2. Super Chat (Paid Message)
            if let Some(sc) = add.get("liveChatPaidMessageRenderer") {
                let author_name = sc
                    .pointer("/authorName/simpleText")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let author_id = sc["authorExternalChannelId"].as_str().unwrap_or("").to_string();
                let amount = sc
                    .pointer("/purchaseAmountText/simpleText")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let text = sc
                    .pointer("/message/runs")
                    .and_then(Value::as_array)
                    .map(|runs| {
                        runs.iter()
                            .filter_map(|r| r["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .unwrap_or_default();

                let mut args = Obj::new();
                args.insert("user".into(), author_name.into());
                args.insert("userId".into(), author_id.into());
                args.insert("amount".into(), amount.into());
                args.insert("message".into(), text.into());

                engine.fire(T_YT_SUPER_CHAT, "YouTubeSuperChat", "YouTube", "youtube", &args, |_| {
                    true
                });
                engine.emit("YouTube", "SuperChat", || json!(args));
            }

            // 3. Super Sticker (Paid Sticker)
            if let Some(ss) = add.get("liveChatPaidStickerRenderer") {
                let author_name = ss
                    .pointer("/authorName/simpleText")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let author_id = ss["authorExternalChannelId"].as_str().unwrap_or("").to_string();
                let amount = ss
                    .pointer("/purchaseAmountText/simpleText")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();

                let mut args = Obj::new();
                args.insert("user".into(), author_name.into());
                args.insert("userId".into(), author_id.into());
                args.insert("amount".into(), amount.into());

                engine.fire(
                    T_YT_SUPER_STICKER,
                    "YouTubeSuperSticker",
                    "YouTube",
                    "youtube",
                    &args,
                    |_| true,
                );
                engine.emit("YouTube", "SuperSticker", || json!(args));
            }

            // 4. Membership item
            if let Some(mem) = add.get("liveChatMembershipItemRenderer") {
                let author_name = mem
                    .pointer("/authorName/simpleText")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let author_id = mem["authorExternalChannelId"].as_str().unwrap_or("").to_string();
                let header = mem
                    .pointer("/headerSubtext/runs")
                    .and_then(Value::as_array)
                    .map(|runs| {
                        runs.iter()
                            .filter_map(|r| r["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .unwrap_or_default();

                let mut args = Obj::new();
                args.insert("user".into(), author_name.into());
                args.insert("userId".into(), author_id.into());
                args.insert("header".into(), header.clone().into());

                if header.to_lowercase().contains("month") {
                    engine.fire(
                        T_YT_MEMBER_MILESTONE,
                        "YouTubeMemberMileStone",
                        "YouTube",
                        "youtube",
                        &args,
                        |_| true,
                    );
                    engine.emit("YouTube", "MemberMileStone", || json!(args));
                } else {
                    engine.fire(
                        T_YT_NEW_SPONSOR,
                        "YouTubeNewSponsor",
                        "YouTube",
                        "youtube",
                        &args,
                        |_| true,
                    );
                    engine.emit("YouTube", "NewSponsor", || json!(args));
                }
            }

            // 5. Membership Gift
            if let Some(gift) = add.get("liveChatSponsorshipsGiftPurchaseAnnouncementRenderer") {
                let author_name = gift
                    .pointer("/header/liveChatSponsorshipsHeaderRenderer/authorName/simpleText")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let mut args = Obj::new();
                args.insert("user".into(), author_name.into());
                engine.fire(
                    T_YT_MEMBERSHIP_GIFT,
                    "YouTubeMembershipGift",
                    "YouTube",
                    "youtube",
                    &args,
                    |_| true,
                );
                engine.emit("YouTube", "MembershipGift", || json!(args));
            }

            // 6. Gift Membership Received
            if let Some(rec) = add.get("liveChatSponsorshipsGiftRedemptionAnnouncementRenderer") {
                let author_name = rec
                    .pointer("/authorName/simpleText")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let mut args = Obj::new();
                args.insert("user".into(), author_name.into());
                engine.fire(
                    T_YT_GIFT_MEMBERSHIP_RECEIVED,
                    "YouTubeGiftMembershipReceived",
                    "YouTube",
                    "youtube",
                    &args,
                    |_| true,
                );
                engine.emit("YouTube", "GiftMembershipReceived", || json!(args));
            }

            // 7. Jewels Gifted
            if let Some(jewel) = add.get("liveChatJewelsGiftRenderer") {
                let author_name = jewel
                    .pointer("/authorName/simpleText")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let count = jewel["jewelCount"].as_u64().unwrap_or(0);
                let mut args = Obj::new();
                args.insert("user".into(), author_name.into());
                args.insert("jewelCount".into(), count.into());
                engine.fire(
                    T_YT_JEWELS_GIFTED,
                    "YouTubeJewelsGifted",
                    "YouTube",
                    "youtube",
                    &args,
                    |_| true,
                );
                engine.emit("YouTube", "JewelsGifted", || json!(args));
            }
        }

        // Deleted item
        if let Some(del) = act.get("markChatItemAsDeletedAction") {
            let target_id = del["targetItemId"].as_str().unwrap_or("").to_string();
            let mut args = Obj::new();
            args.insert("messageId".into(), target_id.into());
            engine.fire(
                T_YT_MESSAGE_DELETED,
                "YouTubeMessageDeleted",
                "YouTube",
                "youtube",
                &args,
                |_| true,
            );
            engine.emit("YouTube", "MessageDeleted", || json!(args));
        }

        // Author deleted / banned
        if let Some(ban) = act.get("markChatItemsByAuthorAsDeletedAction") {
            let author_id = ban["externalChannelId"].as_str().unwrap_or("").to_string();
            let mut args = Obj::new();
            args.insert("userId".into(), author_id.into());
            engine.fire(
                T_YT_USER_BANNED,
                "YouTubeUserBanned",
                "YouTube",
                "youtube",
                &args,
                |_| true,
            );
            engine.emit("YouTube", "UserBanned", || json!(args));
        }

        // Polls
        if let Some(poll) = act
            .get("showLiveChatActionPanelAction")
            .and_then(|p| p.get("panel"))
            .and_then(|p| p.get("liveChatPollRenderer"))
        {
            let question = poll
                .pointer("/header/pollHeaderRenderer/metadataText/runs/0/text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let mut args = Obj::new();
            args.insert("question".into(), question.into());
            engine.fire(
                T_YT_POLL_STARTED,
                "YouTubePollStarted",
                "YouTube",
                "youtube",
                &args,
                |_| true,
            );
            engine.emit("YouTube", "PollStarted", || json!(args));
        }

        if let Some(close_panel) = act.get("closeLiveChatActionPanelAction") {
            let panel_id = close_panel["targetPanelId"].as_str().unwrap_or("").to_string();
            let mut args = Obj::new();
            args.insert("panelId".into(), panel_id.into());
            engine.fire(
                T_YT_POLL_CLOSED,
                "YouTubePollClosed",
                "YouTube",
                "youtube",
                &args,
                |_| true,
            );
            engine.emit("YouTube", "PollClosed", || json!(args));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_clean_youtube_urls() {
        assert_eq!(
            clean_youtube_video("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            "dQw4w9WgXcQ"
        );
        assert_eq!(clean_youtube_video("https://youtu.be/dQw4w9WgXcQ?t=42"), "dQw4w9WgXcQ");
        assert_eq!(
            clean_youtube_video("https://www.youtube.com/live/live_video_id?feature=share"),
            "live_video_id"
        );
        assert_eq!(clean_youtube_video("simple_vid_id"), "simple_vid_id");

        assert_eq!(
            clean_youtube_channel("https://www.youtube.com/channel/UC1234567890"),
            "UC1234567890"
        );
        assert_eq!(clean_youtube_channel("@MyCreatorHandle"), "MyCreatorHandle");
    }

    use super::*;

    #[test]
    fn youtube_config_defaults() {
        let cfg = YouTubeConfig::default();
        assert_eq!(cfg.channel_id, "");
        assert_eq!(cfg.video_id, "");
        assert!(!cfg.auto_connect);
    }

    #[test]
    fn parse_innertube_chat_message() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});
        let mut speakers = HashSet::new();

        let actions = vec![json!({
            "addChatItemAction": {
                "item": {
                    "liveChatTextMessageRenderer": {
                        "id": "msg-yt-1",
                        "authorName": { "simpleText": "YTUser1" },
                        "authorExternalChannelId": "UC1234567890",
                        "message": {
                            "runs": [{ "text": "!hello world" }]
                        },
                        "authorBadges": [{
                            "liveChatAuthorBadgeRenderer": {
                                "icon": { "iconType": "OWNER" }
                            }
                        }]
                    }
                }
            }
        })];

        handle_innertube_actions(&engine, &actions, &mut speakers);
        assert!(speakers.contains("ytuser1"));
    }

    #[test]
    fn parse_innertube_super_chat_and_stickers() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});
        let mut speakers = HashSet::new();

        let actions = vec![
            json!({
                "addChatItemAction": {
                    "item": {
                        "liveChatPaidMessageRenderer": {
                            "authorName": { "simpleText": "BigDonator" },
                            "authorExternalChannelId": "UC999",
                            "purchaseAmountText": { "simpleText": "$50.00" },
                            "message": { "runs": [{ "text": "Keep it up!" }] }
                        }
                    }
                }
            }),
            json!({
                "addChatItemAction": {
                    "item": {
                        "liveChatPaidStickerRenderer": {
                            "authorName": { "simpleText": "StickerFan" },
                            "authorExternalChannelId": "UC888",
                            "purchaseAmountText": { "simpleText": "$5.00" }
                        }
                    }
                }
            }),
            json!({
                "addChatItemAction": {
                    "item": {
                        "liveChatMembershipItemRenderer": {
                            "authorName": { "simpleText": "LoyalFan" },
                            "authorExternalChannelId": "UC777",
                            "headerSubtext": { "runs": [{ "text": "Member for 6 months" }] }
                        }
                    }
                }
            }),
            json!({
                "addChatItemAction": {
                    "item": {
                        "liveChatJewelsGiftRenderer": {
                            "authorName": { "simpleText": "JewelSender" },
                            "jewelCount": 500
                        }
                    }
                }
            }),
            json!({
                "markChatItemAsDeletedAction": {
                    "targetItemId": "msg-yt-1"
                }
            }),
            json!({
                "markChatItemsByAuthorAsDeletedAction": {
                    "externalChannelId": "UC_BANNED"
                }
            }),
        ];

        handle_innertube_actions(&engine, &actions, &mut speakers);
    }

    #[tokio::test]
    async fn test_mock_innertube_poll() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await.unwrap();

            let resp_body = json!({
                "continuationContents": {
                    "liveChatContinuation": {
                        "actions": [{
                            "addChatItemAction": {
                                "item": {
                                    "liveChatTextMessageRenderer": {
                                        "id": "poll-msg-1",
                                        "authorName": { "simpleText": "PollViewer" },
                                        "authorExternalChannelId": "UCPoll",
                                        "message": { "runs": [{ "text": "hello from mock poll" }] }
                                    }
                                }
                            }
                        }],
                        "continuations": [{
                            "invalidationContinuationData": {
                                "continuation": "next_token_123"
                            }
                        }]
                    }
                }
            })
            .to_string();

            let http_response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            socket.write_all(http_response.as_bytes()).await.unwrap();
        });

        let mut config = Config::default();
        config.youtube.video_id = "test_video_123".into();

        let (tx, _rx) = watch::channel(config);
        let cfg_sender = Arc::new(tx);
        let engine = Engine::new(cfg_sender.clone(), |_| {});
        let connected = Arc::new(AtomicBool::new(true));
        let mut speakers = HashSet::new();

        let mock_url = format!("http://{addr}/live_chat");
        run_innertube_poll_loop_url(
            engine.clone(),
            cfg_sender.clone(),
            connected.clone(),
            &mut speakers,
            &mock_url,
            true,
        )
        .await;

        assert!(speakers.contains("pollviewer"));
    }
}
