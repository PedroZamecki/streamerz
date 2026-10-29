//! YouTube integration for Streamerz (Phase 11):
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

/// The text between the first `marker` in `page` and the next quote, with JSON's `\u0026` as `&`.
fn quoted_after(page: &[u8], marker: &[u8]) -> Option<String> {
    let at = page.windows(marker.len()).position(|w| w == marker)? + marker.len();
    let end = page[at..].iter().position(|b| *b == b'"')?;
    let text = std::str::from_utf8(&page[at..at + end]).ok()?;
    Some(text.replace("\\u0026", "&")).filter(|t| t.starts_with("http"))
}

/// A channel's banner and picture, read from its public page (the API has no banner for other people's
/// channels): what youtube.com's own channel card shows. None when it has none, or the page doesn't answer.
pub async fn public_channel(channel_id: &str) -> (Option<String>, Option<String>) {
    if !channel_id.starts_with("UC")
        || !channel_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return (None, None);
    }
    // The cookie keeps the consent page out of the way.
    let headers = [
        ("Accept-Language".to_string(), "en".to_string()),
        ("Cookie".to_string(), "SOCS=CAI; CONSENT=YES+1".to_string()),
        (
            "User-Agent".to_string(),
            "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0".to_string(),
        ),
    ];
    let url = format!("https://www.youtube.com/channel/{channel_id}");
    let Ok(r) = crate::http::request("GET", &url, &headers, b"").await else {
        return (None, None);
    };
    if r.status != 200 {
        return (None, None);
    }
    (
        quoted_after(&r.body, br#""imageBannerViewModel":{"image":{"sources":[{"url":""#),
        quoted_after(&r.body, br#""avatar":{"thumbnails":[{"url":""#),
    )
}

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
    /// Not connected at start (it is by default).
    pub manual_connect: bool,
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
    /// The live video being read ("" when none).
    LiveVideo(oneshot::Sender<String>),
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
        resp: oneshot::Sender<Result<String, String>>,
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
    standby: Arc<AtomicBool>,
}

impl YouTubeHandle {
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn is_standby(&self) -> bool {
        self.standby.load(Ordering::Relaxed) && !self.is_connected()
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

    /// The new poll's id.
    /// The live video's watch page facts: start time (Unix seconds; None when not live) and title.
    async fn live_page(&self) -> Result<(Option<i64>, String), String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(YouTubeCmd::LiveVideo(tx))
            .map_err(|_| "YouTube runner is down".to_string())?;
        let video = rx.await.unwrap_or_default();
        if video.is_empty() {
            return Ok((None, String::new()));
        }
        let headers = [
            ("User-Agent".to_string(), UA.to_string()),
            ("Accept-Language".to_string(), "en".to_string()),
        ];
        let page = crate::http::request("GET", &format!("{YOUTUBE_WEB}/watch?v={video}"), &headers, b"")
            .await?
            .text();
        let start = find(&page, "startTimestamp").and_then(|t| {
            // "2026-10-24T09:00:05+00:00"
            crate::engine::rest_parse_iso(t)
        });
        let title = page
            .split("<meta name=\"title\" content=\"")
            .nth(1)
            .and_then(|x| x.split('"').next())
            .unwrap_or("")
            .replace("&amp;", "&")
            .replace("&#39;", "'")
            .replace("&quot;", "\"");
        Ok((start, title))
    }

    pub async fn live_started(&self) -> Result<Option<i64>, String> {
        Ok(self.live_page().await?.0)
    }

    pub async fn live_title(&self) -> Result<String, String> {
        Ok(self.live_page().await?.1)
    }

    pub async fn create_poll(&self, broadcast: &str, question: &str, options: &[String]) -> Result<String, String> {
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

fn channel_label(y: &YouTubeConfig) -> String {
    if let Some(b) = &y.broadcaster {
        if !b.name.trim().is_empty() {
            return b.name.trim().to_string();
        }
    }
    if !y.video_id.trim().is_empty() {
        return format!("video {}", y.video_id.trim());
    }
    if !y.channel_id.trim().is_empty() {
        return y.channel_id.trim().to_string();
    }
    "unconfigured channel".into()
}

pub fn start(engine: Rc<Engine>, cfg: &Arc<watch::Sender<Config>>) -> YouTubeHandle {
    let (tx, mut rx) = mpsc::unbounded_channel::<YouTubeCmd>();
    let connected = Arc::new(AtomicBool::new(false));
    let standby = Arc::new(AtomicBool::new(false));
    let handle = YouTubeHandle {
        tx,
        connected: connected.clone(),
        standby: standby.clone(),
    };

    let runner = YouTubeRunner {
        engine: engine.clone(),
        cfg: cfg.clone(),
        connected: connected.clone(),
        standby: standby.clone(),
        live: Default::default(),
        poll: Default::default(),
    };

    tokio::task::spawn_local(async move {
        let mut active = false;
        let mut poll_task: Option<tokio::task::JoinHandle<()>> = None;

        while let Some(cmd) = rx.recv().await {
            match cmd {
                YouTubeCmd::LiveVideo(resp) => {
                    let _ = resp.send(runner.live.borrow().0.clone());
                }
                YouTubeCmd::Connect => {
                    if !active {
                        active = true;
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
                    } else {
                        runner.engine.live_wake.notify_waiters();
                    }
                }
                YouTubeCmd::Disconnect => {
                    if active {
                        active = false;
                        connected.store(false, Ordering::Relaxed);
                        standby.store(false, Ordering::Relaxed);
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
            (None, true) => (
                pick(&c, !bot).ok_or_else(|| {
                    if crate::secrets::is_locked() {
                        "System keyring is locked; unlock it to use saved YouTube login"
                    } else {
                        "No YouTube account logged in"
                    }
                })?,
                !bot,
                client(&c),
            ),
            (None, false) => {
                if crate::secrets::is_locked() {
                    return Err("System keyring is locked; unlock it to use saved YouTube login".into());
                }
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
    standby: Arc<AtomicBool>,
    /// The live video being read, and its Data API live chat id once looked up (for sending).
    live: Rc<std::cell::RefCell<(String, String)>>,
    /// The poll created here (its chat message id), to end it.
    poll: Rc<std::cell::RefCell<String>>,
}

impl YouTubeRunner {
    async fn run_poll_loop(&self) {
        read_chat_with_standby(
            &self.engine,
            &self.cfg,
            &self.connected,
            Some(&self.standby),
            &self.live,
            YOUTUBE_WEB,
            false,
        )
        .await;
    }

    /// The Data API live chat id of the video being read (or `broadcast`, a video id), looked up once.
    async fn live_chat_id(&self, broadcast: &str, token: &str) -> Result<String, String> {
        let video = if broadcast.is_empty() {
            self.live.borrow().0.clone()
        } else {
            broadcast.to_string()
        };
        if video.is_empty() {
            return Err("YouTube: no live stream found yet (connect while live)".into());
        }
        if broadcast.is_empty() && !self.live.borrow().1.is_empty() {
            return Ok(self.live.borrow().1.clone());
        }
        let headers = [("Authorization".to_string(), format!("Bearer {token}"))];
        let url = format!("{YOUTUBE_API_BASE}/videos?part=liveStreamingDetails&id={video}");
        let r = crate::http::request("GET", &url, &headers, b"").await?;
        let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
        let id = v
            .pointer("/items/0/liveStreamingDetails/activeLiveChatId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("YouTube: {video} has no active live chat (HTTP {})", r.status))?
            .to_string();
        if broadcast.is_empty() {
            self.live.borrow_mut().1.clone_from(&id);
        }
        Ok(id)
    }

    async fn send_chat_api(&self, broadcast: &str, text: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, use_bot, fallback).await?;

        let live_chat_id = self.live_chat_id(broadcast, &token).await?;

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

    /// The video `broadcast` names, else the live one being read.
    fn video(&self, broadcast: &str) -> Result<String, String> {
        if !broadcast.is_empty() {
            return Ok(broadcast.to_string());
        }
        let live = self.live.borrow().0.clone();
        if live.is_empty() {
            return Err("YouTube: no live stream found yet (connect while live)".into());
        }
        Ok(live)
    }

    /// Change a video's title and/or description. videos.update replaces the whole snippet (and
    /// requires its categoryId), so the current one is read and sent back with the changes.
    async fn update_snippet(
        &self,
        broadcast: &str,
        title: Option<&str>,
        description: Option<&str>,
    ) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let video = self.video(broadcast)?;
        let auth = ("Authorization".to_string(), format!("Bearer {token}"));
        let r = crate::http::request(
            "GET",
            &format!(
                "{YOUTUBE_API_BASE}/videos?part=snippet&id={}",
                crate::commands::url_encode(&video)
            ),
            std::slice::from_ref(&auth),
            b"",
        )
        .await?;
        let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
        let mut snippet = v
            .pointer("/items/0/snippet")
            .cloned()
            .ok_or_else(|| format!("YouTube: can't read video {video} (HTTP {})", r.status))?;
        if let Some(t) = title {
            snippet["title"] = json!(t);
        }
        if let Some(d) = description {
            snippet["description"] = json!(d);
        }
        // Read-only or derived fields the update refuses or ignores.
        if let Some(o) = snippet.as_object_mut() {
            for k in [
                "publishedAt",
                "channelId",
                "thumbnails",
                "channelTitle",
                "liveBroadcastContent",
                "localized",
            ] {
                o.remove(k);
            }
        }
        let body = serde_json::to_vec(&json!({"id": video, "snippet": snippet})).unwrap_or_default();
        let headers = [auth, ("Content-Type".to_string(), "application/json".to_string())];
        let r = crate::http::request(
            "PUT",
            &format!("{YOUTUBE_API_BASE}/videos?part=snippet"),
            &headers,
            &body,
        )
        .await?;
        if (200..300).contains(&r.status) {
            Ok(())
        } else {
            Err(format!("YouTube video update: HTTP {} {}", r.status, r.text()))
        }
    }

    async fn set_title_api(&self, broadcast: &str, title: &str) -> Result<(), String> {
        self.update_snippet(broadcast, Some(title), None).await
    }

    async fn set_description_api(&self, broadcast: &str, description: &str) -> Result<(), String> {
        self.update_snippet(broadcast, None, Some(description)).await
    }

    async fn set_title_and_description_api(
        &self,
        broadcast: &str,
        title: &str,
        description: &str,
    ) -> Result<(), String> {
        self.update_snippet(broadcast, Some(title), Some(description)).await
    }

    async fn ban_user_api(&self, broadcast: &str, user_id: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;

        let live_chat_id = &self.live_chat_id(broadcast, &token).await?;

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

        let live_chat_id = &self.live_chat_id(broadcast, &token).await?;

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

    /// Post a poll in the live chat (liveChatMessages.insert, a pollEvent); kept to end it later.
    async fn create_poll_api(&self, broadcast: &str, question: &str, options: &[String]) -> Result<String, String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let chat = self.live_chat_id(broadcast, &token).await?;
        let opts: Vec<Value> = options
            .iter()
            .filter(|o| !o.trim().is_empty())
            .map(|o| json!({"optionText": o}))
            .collect();
        let body = json!({"snippet": {"liveChatId": chat, "type": "pollEvent",
            "pollDetails": {"metadata": {"questionText": question, "options": opts}}}});
        let headers = [
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let r = crate::http::request(
            "POST",
            &format!("{YOUTUBE_API_BASE}/liveChat/messages?part=snippet"),
            &headers,
            &serde_json::to_vec(&body).unwrap_or_default(),
        )
        .await?;
        let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
        match v["id"].as_str() {
            Some(id) if (200..300).contains(&r.status) => {
                *self.poll.borrow_mut() = id.to_string();
                Ok(id.to_string())
            }
            _ => Err(format!("YouTube create poll: HTTP {} {}", r.status, r.text())),
        }
    }

    /// End the poll this created (liveChatMessages.transition to closed).
    async fn end_poll_api(&self, _broadcast: &str) -> Result<(), String> {
        let id = std::mem::take(&mut *self.poll.borrow_mut());
        if id.is_empty() {
            return Err("YouTube: no poll to end (only polls made here can be ended)".into());
        }
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let headers = [("Authorization".to_string(), format!("Bearer {token}"))];
        let url = format!(
            "{YOUTUBE_API_BASE}/liveChat/messages/transition?id={}&status=closed&part=snippet",
            crate::commands::url_encode(&id)
        );
        let r = crate::http::request("POST", &url, &headers, b"").await?;
        if (200..300).contains(&r.status) {
            Ok(())
        } else {
            Err(format!("YouTube end poll: HTTP {} {}", r.status, r.text()))
        }
    }
}

pub const YOUTUBE_WEB: &str = "https://www.youtube.com";

const UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

/// The first `"key":"value"` in a page.
fn find<'a>(page: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\":\"");
    let i = page.find(&pat)? + pat.len();
    page[i..].split('"').next()
}

/// The channel's current live video (the page `/channel/<id>/live` redirects to it), if live.
async fn live_video(web: &str, channel: &str) -> Result<String, String> {
    let headers = [
        ("User-Agent".to_string(), UA.to_string()),
        ("Accept-Language".to_string(), "en".to_string()),
    ];
    let r = crate::http::request("GET", &format!("{web}/channel/{channel}/live"), &headers, b"").await?;
    let page = r.text();
    let canon = page
        .split("<link rel=\"canonical\" href=\"")
        .nth(1)
        .and_then(|x| x.split('"').next())
        .unwrap_or("");
    match canon.split_once("watch?v=") {
        Some((_, v)) if page.contains("\"isLive\":true") => Ok(v.to_string()),
        _ => Err("not live".into()),
    }
}

/// Extract `ytInitialData`'s initial `actions` array from a `/live_chat` popout HTML page, if present.
pub fn extract_initial_actions(page: &str) -> Vec<Value> {
    for marker in ["window[\"ytInitialData\"] = ", "var ytInitialData = "] {
        if let Some(idx) = page.find(marker) {
            let rest = page[idx + marker.len()..].trim_start();
            if let Some(end) = rest.find(";</script>") {
                if let Ok(v) = serde_json::from_str::<Value>(&rest[..end]) {
                    if let Some(arr) = v
                        .pointer("/contents/liveChatRenderer/actions")
                        .and_then(Value::as_array)
                    {
                        return arr.clone();
                    }
                }
            }
        }
    }
    Vec::new()
}

/// Check OAuth `liveBroadcasts.list(mine=true)` (when logged in) for an active live video ID or an
/// upcoming scheduled broadcast start timestamp (Unix seconds).
async fn oauth_broadcast_hint(engine: &Rc<Engine>, cfg: &Arc<watch::Sender<Config>>) -> (Option<String>, Option<i64>) {
    let has_oauth = cfg
        .borrow()
        .youtube
        .broadcaster
        .as_ref()
        .is_some_and(|b| !b.token.access_token.is_empty());
    if !has_oauth {
        return (None, None);
    }
    let Ok(tok) = token(engine, cfg, false, false).await else {
        return (None, None);
    };
    let headers = [("Authorization".to_string(), format!("Bearer {tok}"))];
    let url = format!("{YOUTUBE_API_BASE}/liveBroadcasts?part=snippet,status&mine=true&maxResults=5");
    let Ok(res) = crate::http::request("GET", &url, &headers, b"").await else {
        return (None, None);
    };
    if res.status != 200 {
        return (None, None);
    }
    let Ok(v) = serde_json::from_slice::<Value>(&res.body) else {
        return (None, None);
    };
    let mut nearest_sched: Option<i64> = None;
    for item in v["items"].as_array().into_iter().flatten() {
        let state = item
            .pointer("/status/lifeCycleStatus")
            .and_then(Value::as_str)
            .unwrap_or("");
        if matches!(state, "live" | "liveStarting") {
            if let Some(id) = item["id"].as_str().filter(|s| !s.is_empty()) {
                return (Some(id.to_string()), None);
            }
        }
        if matches!(state, "ready" | "created" | "testing") {
            if let Some(ts) = item
                .pointer("/snippet/scheduledStartTime")
                .and_then(Value::as_str)
                .and_then(crate::engine::rest_parse_iso)
            {
                nearest_sched = Some(nearest_sched.map_or(ts, |prev| prev.min(ts)));
            }
        }
    }
    (None, nearest_sched)
}

pub async fn read_chat(
    engine: &Rc<Engine>,
    cfg: &Arc<watch::Sender<Config>>,
    connected: &Arc<AtomicBool>,
    live: &Rc<std::cell::RefCell<(String, String)>>,
    web: &str,
    run_once: bool,
) {
    read_chat_with_standby(engine, cfg, connected, None, live, web, run_once).await;
}

/// Read a live chat through InnerTube (what youtube.com's chat uses): the video is the configured
/// one, else the channel's live; the first continuation comes from its chat page, and each fetch
/// waits the `timeoutMs` the server returns. When offline, enters Standby, deduplicates logs,
/// wakes immediately when OBS/SLOBS starts streaming, and checks scheduled OAuth broadcasts.
#[allow(clippy::too_many_arguments)]
pub async fn read_chat_with_standby(
    engine: &Rc<Engine>,
    cfg: &Arc<watch::Sender<Config>>,
    connected: &Arc<AtomicBool>,
    standby: Option<&Arc<AtomicBool>>,
    live: &Rc<std::cell::RefCell<(String, String)>>,
    web: &str,
    run_once: bool,
) {
    let set_state = |on: bool, sb: bool| {
        let mut ch = connected.swap(on, Ordering::Relaxed) != on;
        if let Some(s) = standby {
            ch |= s.swap(sb, Ordering::Relaxed) != sb;
        }
        if ch {
            crate::ui::changed();
        }
    };
    let headers = [
        ("User-Agent".to_string(), UA.to_string()),
        ("Accept-Language".to_string(), "en".to_string()),
    ];
    let mut last_offline_note = String::new();
    let mut last_oauth_check: Option<std::time::Instant> = None;
    let mut scheduled_at: Option<i64> = None;

    loop {
        let (video_cfg, channel, label) = {
            let c = cfg.borrow();
            (
                c.youtube.video_id.trim().to_string(),
                c.youtube.channel_id.trim().to_string(),
                channel_label(&c.youtube),
            )
        };
        let mut video = if !video_cfg.is_empty() {
            Ok(video_cfg)
        } else if !channel.is_empty() {
            live_video(web, &channel).await
        } else {
            Err("no channel or video configured".into())
        };
        if video.is_err() && !run_once && web == YOUTUBE_WEB {
            let due = last_oauth_check.is_none_or(|t| t.elapsed() >= Duration::from_secs(300));
            if due {
                last_oauth_check = Some(std::time::Instant::now());
                let (live_id, sched) = oauth_broadcast_hint(engine, cfg).await;
                scheduled_at = sched;
                if let Some(id) = live_id {
                    video = Ok(id);
                }
            }
        }
        let start = match video {
            Ok(v) => crate::http::request("GET", &format!("{web}/live_chat?is_popout=1&v={v}"), &headers, b"")
                .await
                .map(|r| (v, r.text())),
            Err(e) => Err(e),
        };
        let (video, page) = match start {
            Ok(x) => x,
            Err(e) => {
                let is_offline = e == "not live";
                set_state(false, is_offline);
                if run_once {
                    return;
                }
                // Not live is no news: the connections window shows the standby.
                let msg = if is_offline {
                    String::new()
                } else {
                    format!("YouTube: {label}: {e}")
                };
                if last_offline_note != msg && !msg.is_empty() {
                    crate::warn(msg.clone());
                    last_offline_note = msg;
                }
                let now_unix = crate::fmt::unix_now().as_secs() as i64;
                let near_sched = scheduled_at.is_some_and(|ts| (ts - now_unix).abs() <= 900);
                let wait = if engine.stream_warming_up() {
                    Duration::from_secs(5)
                } else if near_sched {
                    Duration::from_secs(15)
                } else {
                    Duration::from_secs(90)
                };
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = engine.live_wake.notified() => {}
                }
                continue;
            }
        };
        let (Some(mut cont), version) = (
            find(&page, "continuation").map(String::from),
            find(&page, "INNERTUBE_CONTEXT_CLIENT_VERSION")
                .unwrap_or("2.20261023.01.00")
                .to_string(),
        ) else {
            set_state(false, true);
            if run_once {
                return;
            }

            let wait = if engine.stream_warming_up() {
                Duration::from_secs(5)
            } else {
                Duration::from_secs(60)
            };
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = engine.live_wake.notified() => {}
            }
            continue;
        };
        last_offline_note.clear();
        *live.borrow_mut() = (video.clone(), String::new());
        crate::say(format!("YouTube: connected to live chat ({label}, video {video})"));
        let joined_usec = (crate::fmt::unix_now().as_millis() as i64) * 1000;
        let mut initial_hist: Vec<crate::engine::chatlog::Line> = if !run_once {
            extract_initial_actions(&page)
                .iter()
                .filter_map(innertube_action_to_line)
                .collect()
        } else {
            Vec::new()
        };
        let mut welcomed = run_once;
        broadcast_started(engine, cfg, &video).await;
        let extras =
            (!run_once).then(|| tokio::task::spawn_local(while_live(engine.clone(), cfg.clone(), video.clone())));
        let json_headers = [("Content-Type".to_string(), "application/json".to_string())];
        let url = format!("{web}/youtubei/v1/live_chat/get_live_chat?prettyPrint=false");
        loop {
            let body =
                json!({"context": {"client": {"clientName": "WEB", "clientVersion": version}}, "continuation": cont});
            let r = crate::http::request(
                "POST",
                &url,
                &json_headers,
                &serde_json::to_vec(&body).unwrap_or_default(),
            )
            .await;
            let v: Value = match r {
                Ok(r) if (200..300).contains(&r.status) => serde_json::from_slice(&r.body).unwrap_or_default(),
                Ok(r) => {
                    crate::warn(format!("YouTube: chat read failed (HTTP {})", r.status));
                    Value::Null
                }
                Err(e) => {
                    crate::warn(format!("YouTube: chat read failed ({e})"));
                    Value::Null
                }
            };
            let c = &v["continuationContents"]["liveChatContinuation"];
            let next = c["continuations"][0]
                .as_object()
                .and_then(|o| o.values().next())
                .cloned()
                .unwrap_or_default();
            let Some(n) = next["continuation"].as_str() else {
                // The stream ended (or the read failed): look for a live again.
                break;
            };
            set_state(true, false);
            cont = n.to_string();
            if !welcomed {
                welcomed = true;
                let mut live_actions = Vec::new();
                if let Some(actions) = c["actions"].as_array() {
                    for act in actions {
                        let ts_usec = act
                            .pointer("/addChatItemAction/item/liveChatTextMessageRenderer/timestampUsec")
                            .and_then(Value::as_str)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        if ts_usec > 0 && ts_usec + 3_000_000 < joined_usec {
                            if let Some(l) = innertube_action_to_line(act) {
                                initial_hist.push(l);
                            }
                        } else {
                            live_actions.push(act.clone());
                        }
                    }
                }
                engine.chat_welcome("youtube", std::mem::take(&mut initial_hist));
                if !live_actions.is_empty() {
                    handle_innertube_actions(engine, &live_actions);
                }
            } else if let Some(actions) = c["actions"].as_array() {
                handle_innertube_actions(engine, actions);
            }
            if run_once {
                return;
            }
            let wait = next["timeoutMs"].as_u64().unwrap_or(5000).clamp(1000, 30000);
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }
        set_state(false, true);
        *live.borrow_mut() = Default::default();
        if let Some(t) = extras {
            t.abort();
        }
        broadcast_ended(engine);
        crate::say(format!("YouTube: live stream ended ({label}); back to standby"));
        if run_once {
            return;
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

/// A message's text from its runs: text as is, standard emoji as the character, YouTube's own
/// emoji as their shortcut (":yt:"), which are also returned as the message's emotes (with their
/// picture).
pub fn runs_text(runs: Option<&Value>) -> (String, Vec<(String, String)>) {
    let (mut text, mut emotes) = (String::new(), Vec::new());
    for r in runs.and_then(Value::as_array).into_iter().flatten() {
        if let Some(t) = r["text"].as_str() {
            text.push_str(t);
        } else if let Some(e) = r.get("emoji") {
            let short = e["shortcuts"][0].as_str().unwrap_or("");
            if e["isCustomEmoji"].as_bool() == Some(true) && !short.is_empty() {
                text.push_str(short);
                let url = e
                    .pointer("/image/thumbnails")
                    .and_then(Value::as_array)
                    .and_then(|t| t.last());
                let url = url.and_then(|t| t["url"].as_str()).unwrap_or("");
                emotes.push((short.to_string(), url.to_string()));
            } else {
                text.push_str(e["emojiId"].as_str().unwrap_or(short));
            }
        }
    }
    (text, emotes)
}

/// Plain text of a `simpleText` or `runs` field.
fn text_of(v: &Value) -> String {
    match v["simpleText"].as_str() {
        Some(t) => t.to_string(),
        None => v["runs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["text"].as_str())
            .collect(),
    }
}

/// The last (largest) thumbnail URL.
fn thumb(v: &Value) -> String {
    v["thumbnails"]
        .as_array()
        .and_then(|t| t.last())
        .and_then(|t| t["url"].as_str())
        .unwrap_or("")
        .to_string()
}

/// "$5.00", "R$ 10,00", "€2.50", "CA$20.00", "JPY 500": (micros, ISO 4217 code). ponytail: the common
/// symbols only; InnerTube gives the text the viewer saw, not the currency.
pub fn money(text: &str) -> (u64, String) {
    const SYMBOLS: [(&str, &str); 16] = [
        ("CA$", "CAD"),
        ("A$", "AUD"),
        ("NZ$", "NZD"),
        ("MX$", "MXN"),
        ("R$", "BRL"),
        ("HK$", "HKD"),
        ("NT$", "TWD"),
        ("$", "USD"),
        ("€", "EUR"),
        ("£", "GBP"),
        ("¥", "JPY"),
        ("₹", "INR"),
        ("₩", "KRW"),
        ("₱", "PHP"),
        ("₽", "RUB"),
        ("₺", "TRY"),
    ];
    let t = text.trim();
    let code = SYMBOLS
        .iter()
        .find(|(s, _)| t.starts_with(s))
        .map(|(_, c)| c.to_string())
        .or_else(|| {
            let w: String = t.chars().take_while(char::is_ascii_alphabetic).collect();
            (w.len() == 3).then(|| w.to_uppercase())
        })
        .unwrap_or_default();
    let digits: String = t
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    // The last separator followed by exactly two digits is the decimal one.
    let (whole, cents) = match digits.rfind(['.', ',']) {
        Some(i) if digits.len() - i == 3 => (&digits[..i], &digits[i + 1..]),
        _ => (digits.as_str(), "0"),
    };
    let whole: u64 = whole.replace(['.', ','], "").parse().unwrap_or(0);
    let cents: u64 = cents.parse().unwrap_or(0);
    (whole * 1_000_000 + cents * 10_000, code)
}

/// The first number in a text ("Member for 6 months", "Gifted 5 memberships").
fn number_in(t: &str) -> u64 {
    t.split(|c: char| !c.is_ascii_digit())
        .find(|w| !w.is_empty())
        .and_then(|w| w.parse().ok())
        .unwrap_or(0)
}

/// Official YouTube Live Chat author name color by role (or paid message ARGB int).
pub fn youtube_author_color(r: &Value, owner: bool, moderator: bool, member: bool) -> String {
    if let Some(argb) = r["authorNameTextColor"].as_u64() {
        let rgb = (argb as u32) & 0x00FF_FFFF;
        if rgb != 0 {
            return format!("#{rgb:06X}");
        }
    }
    if owner {
        "#FFD600".into()
    } else if moderator {
        "#5E84F1".into()
    } else if member {
        "#2BA640".into()
    } else {
        "#AAAAAA".into()
    }
}

/// Extract YouTube Live Chat's authentic `(badge_key, image_url)` list from `authorBadges`.
pub fn yt_badge_list(r: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for b in r["authorBadges"].as_array().into_iter().flatten() {
        let br = &b["liveChatAuthorBadgeRenderer"];
        let custom = thumb(&br["customThumbnail"]);
        if !custom.is_empty() {
            out.push(("youtube:member".into(), custom));
            continue;
        }
        match br.pointer("/icon/iconType").and_then(Value::as_str).unwrap_or("") {
            "OWNER" => out.push(("youtube:owner".into(), String::new())),
            "MODERATOR" => out.push(("youtube:moderator".into(), String::new())),
            "VERIFIED" => out.push(("youtube:verified".into(), String::new())),
            _ => {}
        }
    }
    out
}

/// Convert an InnerTube `addChatItemAction` text message into a historical chat `Line` (no triggers).
pub fn innertube_action_to_line(act: &Value) -> Option<crate::engine::chatlog::Line> {
    let msg = act.pointer("/addChatItemAction/item/liveChatTextMessageRenderer")?;
    let raw_name = text_of(&msg["authorName"]);
    let user = raw_name.trim_start_matches('@').to_string();
    if user.is_empty() {
        return None;
    }
    let user_id = msg["authorExternalChannelId"].as_str().unwrap_or("").to_string();
    let msg_id = msg["id"].as_str().unwrap_or("").to_string();
    let (raw_text, raw_emotes) = runs_text(msg.pointer("/message/runs"));
    let (text, emotes) = crate::engine::chatlog::normalize_line_emotes("youtube", raw_text, &raw_emotes);
    if text.is_empty() && emotes.is_empty() {
        return None;
    }
    let badges = yt_badge_list(msg);
    let owner = badges.iter().any(|(k, _)| k == "youtube:owner");
    let moderator = badges.iter().any(|(k, _)| k == "youtube:moderator");
    let member = badges.iter().any(|(k, _)| k == "youtube:member");
    let color = youtube_author_color(msg, owner, moderator, member);
    let usec = msg["timestampUsec"]
        .as_str()
        .and_then(|t| t.parse::<i64>().ok())
        .unwrap_or(0);
    let time = if usec > 0 {
        crate::engine::chatlog::local_hhmm(usec / 1_000_000)
    } else {
        String::new()
    };
    Some(crate::engine::chatlog::Line {
        platform: "youtube".into(),
        ts: usec / 1000,
        msg_id,
        time,
        user,
        user_id,
        text,
        emotes,
        broadcaster: owner,
        moderator,
        subscriber: member,
        badges,
        color,
        historical: true,
        ..Default::default()
    })
}

/// Streamer.bot's YouTubeUser, YouTubeBroadcaster and YouTubeBroadcast variables, plus the item's
/// id and time, for one chat item.
fn yt_base(engine: &Rc<Engine>, r: &Value) -> (Obj, bool, bool, bool) {
    let badges = r["authorBadges"].as_array().cloned().unwrap_or_default();
    let icon = |t: &str| {
        badges.iter().any(|b| {
            b.pointer("/liveChatAuthorBadgeRenderer/icon/iconType")
                .and_then(Value::as_str)
                == Some(t)
        })
    };
    let (owner, moderator) = (icon("OWNER"), icon("MODERATOR"));
    let member = badges
        .iter()
        .any(|b| b.pointer("/liveChatAuthorBadgeRenderer/customThumbnail").is_some());
    let ui_badges: Vec<Value> = yt_badge_list(r)
        .into_iter()
        .map(|(k, u)| json!({"key": k, "name": k, "imageUrl": u}))
        .collect();
    let color = youtube_author_color(r, owner, moderator, member);
    let name = text_of(&r["authorName"]);
    let mut a = Obj::new();
    let mut put = |k: &str, v: Value| drop(a.insert(k.into(), v));
    put("user", name.trim_start_matches('@').into());
    put("userName", name.clone().into());
    put("userId", r["authorExternalChannelId"].clone());
    put("userType", "youtube".into());
    put("userProfileUrl", thumb(&r["authorPhoto"]).into());
    put("color", color.into());
    put("badges", ui_badges.into());
    put("isSubscribed", member.into());
    put("userIsSponsor", member.into());
    put("isSponsor", member.into());
    put("isModerator", moderator.into());
    put("isOwner", owner.into());
    put("isVerified", icon("VERIFIED").into());
    put("messageId", r["id"].clone());
    put("id", r["id"].clone());
    let usec = r["timestampUsec"]
        .as_str()
        .and_then(|t| t.parse::<i64>().ok())
        .unwrap_or(0);
    if usec > 0 {
        let t = crate::engine::civil_time(usec / 1_000_000);
        put("publishedAt", crate::fmt::date(&t, "o").into());
    }
    let c = engine.config();
    if let Some(b) = &c.youtube.broadcaster {
        put("broadcastUsername", b.name.clone().into());
        put("broadcastUserId", b.channel_id.clone().into());
    }
    drop(c);
    BROADCAST.with_borrow(|b| a.extend(b.clone()));
    (a, owner, moderator, member)
}

thread_local! {
    /// The live broadcast's variables (broadcastId, broadcast.*), set when its chat is read.
    static BROADCAST: std::cell::RefCell<Obj> = Default::default();
    /// The poll on screen (for Poll Updated/Closed): its arguments.
    static POLL: std::cell::RefCell<Obj> = Default::default();
    /// Who chatted, and when (Present Viewers: those in the last 10 minutes).
    static ACTIVE: std::cell::RefCell<std::collections::HashMap<String, (Value, std::time::Instant)>> = Default::default();
}

/// Fire a YouTube trigger and its WebSocket event.
fn yt(engine: &Rc<Engine>, code: u32, name: &str, ev: &str, args: &Obj) {
    engine.fire(code, name, "YouTube", "youtube", args, |_| true);
    engine.emit("YouTube", ev, || json!(args));
}

/// A poll panel as Streamer.bot's poll.* arguments (InnerTube gives each option's share, not its votes).
fn poll_args(p: &Value) -> Obj {
    let mut a = Obj::new();
    let question = text_of(&p["header"]["pollHeaderRenderer"]["pollQuestion"]);
    a.insert("poll.id".into(), p["liveChatPollId"].clone());
    a.insert("poll.question".into(), question.clone().into());
    a.insert("question".into(), question.into());
    let choices = p["choices"].as_array().cloned().unwrap_or_default();
    a.insert("poll.options.count".into(), choices.len().into());
    let mut best: Option<(usize, f64)> = None;
    for (i, c) in choices.iter().enumerate() {
        let ratio = c["voteRatio"].as_f64().unwrap_or(0.0);
        a.insert(format!("poll.option{i}.text"), text_of(&c["text"]).into());
        a.insert(
            format!("poll.option{i}.votePercentage"),
            text_of(&c["votePercentage"]).into(),
        );
        a.insert(format!("poll.option{i}.voteRatio"), ratio.into());
        if best.is_none_or(|b| ratio > b.1) {
            best = Some((i, ratio));
        }
    }
    let total = number_in(&text_of(&p["header"]["pollHeaderRenderer"]["metadataText"]));
    a.insert("poll.totalVotes".into(), total.into());
    for (i, c) in choices.iter().enumerate() {
        let votes = (c["voteRatio"].as_f64().unwrap_or(0.0) * total as f64).round() as u64;
        a.insert(format!("poll.option{i}.votes"), votes.into());
        if best.is_some_and(|b| b.0 == i) {
            a.insert("poll.winningOption.text".into(), text_of(&c["text"]).into());
            a.insert("poll.winningOption.votes".into(), votes.into());
        }
    }
    a
}

/// The poll on the live now (its poll.* arguments), if any.
pub fn current_poll() -> Obj {
    POLL.with_borrow(Clone::clone)
}

/// A message pinned at the top of the live chat (YouTube's banner): until it's taken down.
fn yt_pin(banner: &Value) -> Option<crate::engine::chatlog::Pin> {
    let r = banner.pointer("/contents/liveChatTextMessageRenderer")?;
    let (text, _) = runs_text(r.pointer("/message/runs"));
    Some(crate::engine::chatlog::Pin {
        platform: "youtube".into(),
        msg_id: r["id"].as_str().unwrap_or("").into(),
        user: text_of(&r["authorName"]).trim_start_matches('@').into(),
        text,
        by: String::new(),
        until: None,
    })
}

pub fn handle_innertube_actions(engine: &Rc<Engine>, actions: &[Value]) {
    for act in actions {
        if let Some(b) = act.pointer("/addBannerToLiveChatCommand/bannerRenderer/liveChatBannerRenderer") {
            if let Some(p) = yt_pin(b) {
                engine.chat_pinned(p);
            }
        }
        // One pin at a time on YouTube: whichever it names, it's that one.
        if act.get("removeBannerForLiveChatCommand").is_some() {
            engine.chat_unpinned("youtube", "");
        }
        let item = act.pointer("/addChatItemAction/item").cloned().unwrap_or_default();
        if let Some(msg) = item.get("liveChatTextMessageRenderer") {
            let (mut args, owner, moderator, member) = yt_base(engine, msg);
            let (text, emotes) = runs_text(msg.pointer("/message/runs"));
            crate::commands::input_args(&text, &text, &mut args);
            args.insert("message".into(), text.clone().into());
            let parts: Vec<Value> = msg["message"]["runs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| match r["text"].as_str() {
                    Some(t) => json!({"text": t}),
                    None => json!({"emoji": r["emoji"]["shortcuts"][0], "url": thumb(&r["emoji"]["image"])}),
                })
                .collect();
            args.insert("parts".into(), Value::Array(parts).to_string().into());
            let mut at = 0;
            let list: Vec<Value> = emotes
                .iter()
                .map(|(name, url)| {
                    let start = text[at..].find(name.as_str()).map_or(at, |i| at + i);
                    at = start + name.len();
                    json!({"name": name, "startIndex": start, "endIndex": at - 1, "imageUrl": url})
                })
                .collect();
            args.insert("emotes".into(), Value::Array(list).to_string().into());
            let id = msg["authorExternalChannelId"].as_str().unwrap_or("").to_string();
            ACTIVE.with_borrow_mut(|m| {
                let who = json!({"userId": id, "userName": args["userName"], "user": args["user"]});
                m.insert(id.clone(), (who, std::time::Instant::now()));
            });
            if engine.first_words("youtube", &id) {
                yt(engine, T_YT_FIRST_WORDS, "YouTubeFirstWords", "FirstWords", &args);
            }
            yt(engine, T_YT_MESSAGE, "YouTubeMessage", "Message", &args);
            engine.chat(&Chat {
                platform: "youtube".into(),
                source: 1024,
                user: args["user"].as_str().unwrap_or("").into(),
                user_id: id,
                text,
                emotes,
                msg_id: msg["id"].as_str().unwrap_or("").into(),
                broadcaster: owner,
                moderator,
                subscriber: member,
                args: args.clone(),
                ..Default::default()
            });
        }
        if let Some(sc) = item.get("liveChatPaidMessageRenderer") {
            let (mut args, ..) = yt_base(engine, sc);
            let amount = text_of(&sc["purchaseAmountText"]);
            let (micros, code) = money(&amount);
            args.insert("amount".into(), amount.into());
            args.insert("microAmount".into(), micros.into());
            args.insert("currencyCode".into(), code.into());
            args.insert("tier".into(), sc["headerBackgroundColor"].clone());
            args.insert("message".into(), text_of(&sc["message"]).into());
            yt(engine, T_YT_SUPER_CHAT, "YouTubeSuperChat", "SuperChat", &args);
        }
        if let Some(ss) = item.get("liveChatPaidStickerRenderer") {
            let (mut args, ..) = yt_base(engine, ss);
            let amount = text_of(&ss["purchaseAmountText"]);
            let (micros, code) = money(&amount);
            let label = ss.pointer("/sticker/accessibility/accessibilityData/label").cloned();
            args.insert("amount".into(), amount.into());
            args.insert("microAmount".into(), micros.into());
            args.insert("currencyCode".into(), code.into());
            args.insert("tier".into(), ss["backgroundColor"].clone());
            args.insert("stickerAltText".into(), label.unwrap_or_default());
            args.insert("stickerImageUrl".into(), thumb(&ss["sticker"]).into());
            args.insert("stickerLanguage".into(), "en".into());
            yt(engine, T_YT_SUPER_STICKER, "YouTubeSuperSticker", "SuperSticker", &args);
        }
        if let Some(m) = item.get("liveChatMembershipItemRenderer") {
            let (mut args, ..) = yt_base(engine, m);
            let primary = text_of(&m["headerPrimaryText"]);
            let sub = text_of(&m["headerSubtext"]);
            if primary.is_empty() {
                // "Welcome to <level>!" or "Upgraded membership to <level>!"
                let level = sub
                    .trim_end_matches('!')
                    .rsplit_once(" to ")
                    .map_or(sub.as_str(), |x| x.1)
                    .to_string();
                args.insert("levelName".into(), level.into());
                args.insert("isUpgrade".into(), sub.to_lowercase().starts_with("upgraded").into());
                yt(engine, T_YT_NEW_SPONSOR, "YouTubeNewSponsor", "NewSponsor", &args);
            } else {
                args.insert("months".into(), number_in(&primary).into());
                args.insert("levelName".into(), sub.into());
                args.insert("message".into(), text_of(&m["message"]).into());
                yt(
                    engine,
                    T_YT_MEMBER_MILESTONE,
                    "YouTubeMemberMileStone",
                    "MemberMileStone",
                    &args,
                );
            }
        }
        if let Some(g) = item.get("liveChatSponsorshipsGiftPurchaseAnnouncementRenderer") {
            let h = &g["header"]["liveChatSponsorshipsHeaderRenderer"];
            let mut r = h.clone();
            r["authorExternalChannelId"] = g["authorExternalChannelId"].clone();
            r["id"] = g["id"].clone();
            r["timestampUsec"] = g["timestampUsec"].clone();
            let (mut args, ..) = yt_base(engine, &r);
            let primary = text_of(&h["primaryText"]);
            args.insert("count".into(), number_in(&primary).into());
            args.insert("tier".into(), primary.into());
            yt(
                engine,
                T_YT_MEMBERSHIP_GIFT,
                "YouTubeMembershipGift",
                "MembershipGift",
                &args,
            );
        }
        if let Some(g) = item.get("liveChatSponsorshipsGiftRedemptionAnnouncementRenderer") {
            let (mut args, ..) = yt_base(engine, g);
            // "received a gift membership by @someone"
            let msg = text_of(&g["message"]);
            let gifter = msg.rsplit_once(" by ").map_or("", |x| x.1).trim().to_string();
            args.insert("gifterUser".into(), gifter.trim_start_matches('@').into());
            args.insert("gifterUserName".into(), gifter.clone().into());
            args.insert("gifterUserType".into(), "youtube".into());
            args.insert("message".into(), msg.into());
            yt(
                engine,
                T_YT_GIFT_MEMBERSHIP_RECEIVED,
                "YouTubeGiftMembershipReceived",
                "GiftMembershipReceived",
                &args,
            );
        }
        if let Some(j) = item.get("liveChatJewelsGiftRenderer") {
            let (mut args, ..) = yt_base(engine, j);
            args.insert("jewelCount".into(), j["jewelCount"].clone());
            args.insert("gift.jewelsAmount".into(), j["jewelCount"].clone());
            yt(engine, T_YT_JEWELS_GIFTED, "YouTubeJewelsGifted", "JewelsGifted", &args);
        }
        if let Some(mc) = item.get("liveChatModeChangeMessageRenderer") {
            let (args, ..) = yt_base(engine, mc);
            let t = format!("{} {}", text_of(&mc["text"]), text_of(&mc["subtext"])).to_lowercase();
            if t.contains("members-only") || t.contains("members only") {
                if t.contains(" off") || t.contains("disabled") {
                    yt(
                        engine,
                        T_YT_SPONSOR_ONLY_ENDED,
                        "YouTubeSponsorOnlyEnded",
                        "SponsorOnlyEnded",
                        &args,
                    );
                } else {
                    yt(
                        engine,
                        T_YT_SPONSOR_ONLY_STARTED,
                        "YouTubeSponsorOnlyStarted",
                        "SponsorOnlyStarted",
                        &args,
                    );
                }
            }
        }
        if let Some(del) = act.get("markChatItemAsDeletedAction") {
            engine.chat_remove("youtube", del["targetItemId"].as_str().unwrap_or(""), "");
            let mut args = BROADCAST.with_borrow(Clone::clone);
            args.insert("messageId".into(), del["targetItemId"].clone());
            yt(
                engine,
                T_YT_MESSAGE_DELETED,
                "YouTubeMessageDeleted",
                "MessageDeleted",
                &args,
            );
        }
        if let Some(ban) = act.get("markChatItemsByAuthorAsDeletedAction") {
            // ponytail: InnerTube doesn't say ban or timeout; viewers' chat only sees the removal.
            let mut args = BROADCAST.with_borrow(Clone::clone);
            let id = ban["externalChannelId"].clone();
            engine.chat_remove("youtube", "", id.as_str().unwrap_or(""));
            if let Some((who, _)) = ACTIVE.with_borrow(|m| m.get(id.as_str().unwrap_or("")).cloned()) {
                args.extend(who.as_object().cloned().unwrap_or_default());
            }
            args.insert("userId".into(), id);
            args.insert("userType".into(), "youtube".into());
            args.insert("banType".into(), "unknown".into());
            yt(engine, T_YT_USER_BANNED, "YouTubeUserBanned", "UserBanned", &args);
        }
        let shown = act.pointer("/showLiveChatActionPanelAction/panel/liveChatPollRenderer");
        let updated = act.pointer("/updateLiveChatPollAction/pollToUpdate/pollRenderer");
        if let Some(p) = shown.or(updated) {
            let mut args = BROADCAST.with_borrow(Clone::clone);
            args.extend(poll_args(p));
            POLL.with_borrow_mut(|x| *x = args.clone());
            crate::ui::polls_changed();
            if shown.is_some() {
                yt(engine, T_YT_POLL_STARTED, "YouTubePollStarted", "PollStarted", &args);
            } else {
                yt(engine, T_YT_POLL_UPDATED, "YouTubePollUpdated", "PollUpdated", &args);
            }
        }
        if let Some(close) = act.get("closeLiveChatActionPanelAction") {
            let mut args = POLL.with_borrow_mut(std::mem::take);
            if args.is_empty() {
                continue;
            }
            crate::ui::polls_changed();
            args.insert("panelId".into(), close["targetPanelId"].clone());
            yt(engine, T_YT_POLL_CLOSED, "YouTubePollClosed", "PollClosed", &args);
        }
    }
}

/// The live video's broadcast.* (and broadcastId) for every YouTube trigger; Broadcast Started.
async fn broadcast_started(engine: &Rc<Engine>, cfg: &Arc<watch::Sender<Config>>, video: &str) {
    let mut b = Obj::new();
    b.insert("broadcastId".into(), video.into());
    b.insert("broadcast.id".into(), video.into());
    // The title and so on need the Data API (a login); without one, just the id.
    if let Ok(t) = token(engine, cfg, false, true).await {
        let url = format!("{YOUTUBE_API_BASE}/videos?part=snippet,status&id={video}");
        let h = [("Authorization".to_string(), format!("Bearer {t}"))];
        if let Ok(r) = crate::http::request("GET", &url, &h, b"").await {
            let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
            let (s, st) = (&v["items"][0]["snippet"], &v["items"][0]["status"]);
            for (k, x) in [
                ("title", &s["title"]),
                ("description", &s["description"]),
                ("publishedAt", &s["publishedAt"]),
                ("defaultLanguage", &s["defaultLanguage"]),
                ("defaultAudioLanguage", &s["defaultAudioLanguage"]),
                ("privacy", &st["privacyStatus"]),
                ("status", &s["liveBroadcastContent"]),
            ] {
                b.insert(format!("broadcast.{k}"), x.clone());
            }
            let tags: Vec<&str> = s["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            b.insert("broadcast.tags".into(), tags.join(", ").into());
            b.insert("broadcastUserProfileImage".into(), thumb(&s["thumbnails"]).into());
        }
    }
    BROADCAST.with_borrow_mut(|x| *x = b.clone());
    yt(
        engine,
        T_YT_BROADCAST_STARTED,
        "YouTubeBroadcastStarted",
        "BroadcastStarted",
        &b,
    );
}

fn broadcast_ended(engine: &Rc<Engine>) {
    let b = BROADCAST.with_borrow_mut(std::mem::take);
    if !b.is_empty() {
        yt(
            engine,
            T_YT_BROADCAST_ENDED,
            "YouTubeBroadcastEnded",
            "BroadcastEnded",
            &b,
        );
    }
}

/// While live: Statistics Updated (a minute apart, Data API: needs a login), Present Viewers (who
/// chatted in the last 10 minutes, every 5) and New Subscriber (recent public subscribers, every 2
/// minutes), each only when an action uses it; and the BTTV/7TV emote changes when used.
async fn while_live(engine: Rc<Engine>, cfg: Arc<watch::Sender<Config>>, video: String) {
    let channel = cfg.borrow().youtube.channel_id.clone();
    let codes = crate::emote_events::Codes {
        platform: "youtube",
        source: "YouTube",
        seventv: (T_YT_SEVENTV_EMOTE_ADDED, T_YT_SEVENTV_EMOTE_REMOVED),
        bttv: (T_YT_BTTV_EMOTE_ADDED, T_YT_BTTV_EMOTE_REMOVED),
    };
    let emotes = (codes.wanted(&engine) && !channel.is_empty())
        .then(|| tokio::task::spawn_local(crate::emote_events::watch(engine.clone(), codes, channel)));
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    let (mut n, mut subs): (u64, Option<HashSet<String>>) = (0, None);
    loop {
        tick.tick().await;
        n += 1;
        let base = BROADCAST.with_borrow(Clone::clone);
        if engine.uses_trigger(T_YT_STATISTICS_UPDATED) {
            if let Ok(t) = token(&engine, &cfg, false, true).await {
                let url = format!("{YOUTUBE_API_BASE}/videos?part=statistics,liveStreamingDetails&id={video}");
                let h = [("Authorization".to_string(), format!("Bearer {t}"))];
                if let Ok(r) = crate::http::request("GET", &url, &h, b"").await {
                    let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
                    let (s, l) = (&v["items"][0]["statistics"], &v["items"][0]["liveStreamingDetails"]);
                    let num = |x: &Value| x.as_str().and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
                    let mut a = base.clone();
                    for k in [
                        "likeCount",
                        "dislikeCount",
                        "viewCount",
                        "favoriteCount",
                        "commentCount",
                    ] {
                        a.insert(k.into(), num(&s[k]).into());
                    }
                    a.insert("concurrentViewers".into(), num(&l["concurrentViewers"]).into());
                    yt(
                        &engine,
                        T_YT_STATISTICS_UPDATED,
                        "YouTubeStatisticsUpdated",
                        "StatisticsUpdated",
                        &a,
                    );
                }
            }
        }
        if n % 5 == 0 && engine.uses_trigger(T_YT_PRESENT_VIEWERS) {
            let users: Vec<Value> = ACTIVE.with_borrow_mut(|m| {
                m.retain(|_, (_, at)| at.elapsed() < Duration::from_secs(600));
                m.values().map(|(u, _)| u.clone()).collect()
            });
            let mut a = base.clone();
            a.insert("users".into(), users.into());
            a.insert("isLive".into(), true.into());
            a.insert("isTest".into(), false.into());
            yt(
                &engine,
                T_YT_PRESENT_VIEWERS,
                "YouTubePresentViewers",
                "PresentViewers",
                &a,
            );
        }
        if n % 2 == 0 && engine.uses_trigger(T_YT_NEW_SUBSCRIBER) {
            if let Ok(t) = token(&engine, &cfg, false, false).await {
                let url = format!(
                    "{YOUTUBE_API_BASE}/subscriptions?part=subscriberSnippet&myRecentSubscribers=true&maxResults=50"
                );
                let h = [("Authorization".to_string(), format!("Bearer {t}"))];
                if let Ok(r) = crate::http::request("GET", &url, &h, b"").await {
                    let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
                    let now: Vec<&Value> = v["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|i| &i["subscriberSnippet"])
                        .collect();
                    let ids: HashSet<String> = now
                        .iter()
                        .filter_map(|s| s["channelId"].as_str().map(String::from))
                        .collect();
                    if let Some(old) = &subs {
                        for s in now
                            .iter()
                            .filter(|s| !old.contains(s["channelId"].as_str().unwrap_or("")))
                        {
                            let mut a = base.clone();
                            a.insert("user".into(), s["title"].clone());
                            a.insert("userName".into(), s["title"].clone());
                            a.insert("userId".into(), s["channelId"].clone());
                            a.insert("userType".into(), "youtube".into());
                            a.insert("userProfileUrl".into(), thumb(&s["thumbnails"]).into());
                            yt(
                                &engine,
                                T_YT_NEW_SUBSCRIBER,
                                "YouTubeNewSubscriber",
                                "NewSubscriber",
                                &a,
                            );
                        }
                    }
                    subs = Some(ids);
                }
            }
        }
        let _ = &emotes;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_banner_in_a_channel_page() {
        let page = br#"x"banner":{"imageBannerViewModel":{"image":{"sources":[{"url":"https://yt3.example/b=w1060\u0026k","width":1}]}}},"avatar":{"thumbnails":[{"url":"https://yt3.example/a=s900","w":1}]}"#;
        assert_eq!(
            super::quoted_after(page, br#""imageBannerViewModel":{"image":{"sources":[{"url":""#).as_deref(),
            Some("https://yt3.example/b=w1060&k")
        );
        assert_eq!(
            super::quoted_after(page, br#""avatar":{"thumbnails":[{"url":""#).as_deref(),
            Some("https://yt3.example/a=s900")
        );
        assert_eq!(super::quoted_after(page, b"nothing"), None);
    }

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
    fn pinned_banners_come_and_go() {
        use crate::engine::tests::{engine, run};
        let _g = crate::TEST_LOCK.lock().unwrap();
        run(async {
            let e = engine(json!({"actions": []}));
            let msg = json!({"id": "m1", "authorName": {"simpleText": "@Ana"}, "message": {"runs": [{"text": "hi"}]}});
            let banner = json!({"addBannerToLiveChatCommand": {"bannerRenderer": {"liveChatBannerRenderer":
                {"actionId": "a1", "contents": {"liveChatTextMessageRenderer": msg}}}}});
            handle_innertube_actions(&e, &[banner]);
            let pins = e.chat_pins();
            assert_eq!(
                (pins.len(), pins[0].user.as_str(), pins[0].text.as_str()),
                (1, "Ana", "hi")
            );
            handle_innertube_actions(
                &e,
                &[json!({"removeBannerForLiveChatCommand": {"targetActionId": "a1"}})],
            );
            assert!(e.chat_pins().is_empty());
        });
    }

    #[test]
    fn youtube_config_defaults() {
        let cfg = YouTubeConfig::default();
        assert_eq!(cfg.channel_id, "");
        assert_eq!(cfg.video_id, "");
        assert!(!cfg.manual_connect);
    }

    #[test]
    fn items_fire_with_their_variables() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap();
        assert_eq!(money("$5.00"), (5_000_000, "USD".into()));
        assert_eq!(money("R$ 1.234,50"), (1_234_500_000, "BRL".into()));
        assert_eq!(money("¥500"), (500_000_000, "JPY".into()));
        let watch: &[(u32, &str)] = &[
            (
                T_YT_MESSAGE,
                "%user% %userName% %input0% %publishedAt% %userProfileUrl%",
            ),
            (T_YT_SUPER_CHAT, "%amount% %microAmount% %currencyCode% %message%"),
            (T_YT_MEMBER_MILESTONE, "%months% %levelName% %message%"),
            (T_YT_NEW_SPONSOR, "%levelName% %isUpgrade%"),
            (T_YT_MEMBERSHIP_GIFT, "%user% %count%"),
            (T_YT_GIFT_MEMBERSHIP_RECEIVED, "%user% %gifterUser%"),
            (
                T_YT_POLL_STARTED,
                "%poll.question% %poll.option1.text% %poll.winningOption.text%",
            ),
            (T_YT_POLL_CLOSED, "%poll.question% %poll.totalVotes%"),
            (T_YT_SPONSOR_ONLY_STARTED, "on"),
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
            let author = json!({"authorName": {"simpleText": "@ana"}, "authorExternalChannelId": "UCa",
                "authorPhoto": {"thumbnails": [{"url": "s"}, {"url": "big"}]}, "timestampUsec": "1700000000000000"});
            let with = |extra: Value| {
                let mut a = author.clone();
                a.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
                a
            };
            let add = |kind: &str, r: Value| json!({"addChatItemAction": {"item": {kind: r}}});
            let choices = json!([{"text": {"simpleText": "Cats"}, "voteRatio": 0.25}, {"text": {"simpleText": "Dogs"}, "voteRatio": 0.75}]);
            let poll = json!({"liveChatPollId": "p", "choices": choices, "header": {"pollHeaderRenderer": {
                "pollQuestion": {"simpleText": "Pets?"}, "metadataText": {"runs": [{"text": "40 votes"}]}}}});
            let actions = [
                add(
                    "liveChatTextMessageRenderer",
                    with(json!({"id": "m", "message": {"runs": [{"text": "hi there"}]}})),
                ),
                add(
                    "liveChatPaidMessageRenderer",
                    with(json!({"purchaseAmountText": {"simpleText": "€2.50"}, "message": {"runs": [{"text": "gg"}]}})),
                ),
                add(
                    "liveChatMembershipItemRenderer",
                    with(
                        json!({"headerPrimaryText": {"runs": [{"text": "Member for "}, {"text": "6"}, {"text": " months"}]},
                    "headerSubtext": {"simpleText": "Gold"}, "message": {"runs": [{"text": "yay"}]}}),
                    ),
                ),
                add(
                    "liveChatMembershipItemRenderer",
                    with(json!({"headerSubtext": {"runs": [{"text": "Welcome to Gold!"}]}})),
                ),
                add(
                    "liveChatSponsorshipsGiftPurchaseAnnouncementRenderer",
                    json!({"authorExternalChannelId": "UCa",
                    "header": {"liveChatSponsorshipsHeaderRenderer": {"authorName": {"simpleText": "@ana"},
                    "primaryText": {"runs": [{"text": "Gifted "}, {"text": "5"}, {"text": " Gold memberships"}]}}}}),
                ),
                add(
                    "liveChatSponsorshipsGiftRedemptionAnnouncementRenderer",
                    json!({"authorName": {"simpleText": "@bo"},
                    "message": {"runs": [{"text": "received a gift membership by "}, {"text": "@ana"}]}}),
                ),
                add(
                    "liveChatModeChangeMessageRenderer",
                    json!({"text": {"runs": [{"text": "Members-only mode is on"}]}}),
                ),
                json!({"showLiveChatActionPanelAction": {"panel": {"liveChatPollRenderer": poll}}}),
                json!({"closeLiveChatActionPanelAction": {"targetPanelId": "p"}}),
            ];
            handle_innertube_actions(&e, &actions);
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        let want = [
            "T 4003 ana @ana hi 2023-11-14T22:13:20.0000000+00:00 big",
            "T 4006 €2.50 2500000 EUR gg",
            "T 4009 6 Gold yay",
            "T 4008 Gold False",
            "T 4014 ana 5",
            "T 4015 bo ana",
            "T 4028 Pets? Dogs Dogs",
            "T 4027 Pets? 40",
            "T 4010 on",
        ];
        for w in want {
            assert!(out.iter().any(|x| x == w), "{w} missing: {out:?}");
        }
    }

    #[test]
    fn parse_innertube_chat_message() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});

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

        handle_innertube_actions(&engine, &actions);
        assert!(!engine.first_words("youtube", "uc1234567890"));
    }

    #[test]
    fn parse_innertube_super_chat_and_stickers() {
        let (tx, _rx) = watch::channel(Config::default());
        let engine = Engine::new(Arc::new(tx), |_| {});

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

        handle_innertube_actions(&engine, &actions);
    }

    #[test]
    fn mock_live_chat_read() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            // The channel's /live page, its chat page, then one InnerTube answer.
            tokio::task::spawn_local(async move {
                for _ in 0..3 {
                    let (mut sock, _) = listener.accept().await.unwrap();
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap();
                    let head = String::from_utf8_lossy(&buf[..n]).to_string();
                    let body = if head.starts_with("GET /channel/UCx/live") {
                        r#"<link rel="canonical" href="https://www.youtube.com/watch?v=vid123"> "isLive":true"#.to_string()
                    } else if head.starts_with("GET /live_chat?is_popout=1&v=vid123") {
                        r#"x "INNERTUBE_CONTEXT_CLIENT_VERSION":"2.1" y "continuation":"first" z"#.to_string()
                    } else {
                        assert!(head.contains("\"continuation\":\"first\""), "{head}");
                        json!({"continuationContents": {"liveChatContinuation": {
                            "continuations": [{"invalidationContinuationData": {"continuation": "next", "timeoutMs": 10000}}],
                            "actions": [{"addChatItemAction": {"item": {"liveChatTextMessageRenderer": {
                                "id": "m1", "authorName": {"simpleText": "PollViewer"}, "authorExternalChannelId": "UCPoll",
                                "message": {"runs": [{"text": "hi "}, {"emoji": {"emojiId": "😀", "shortcuts": [":grinning:"]}},
                                    {"emoji": {"emojiId": "UCx/abc", "shortcuts": [":yt:"], "isCustomEmoji": true}}]}
                            }}}}]
                        }}})
                        .to_string()
                    };
                    let r = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    sock.write_all(r.as_bytes()).await.unwrap();
                }
            });
            let mut config = Config::default();
            config.youtube.channel_id = "UCx".into();
            let cfg = Arc::new(watch::channel(config).0);
            let engine = Engine::new(cfg.clone(), |_| {});
            let connected = Arc::new(AtomicBool::new(false));
            let live = Rc::new(std::cell::RefCell::new((String::new(), String::new())));
            read_chat(&engine, &cfg, &connected, &live, &format!("http://{addr}"), true).await;
            assert!(!engine.first_words("youtube", "ucpoll"));
            assert!(connected.load(Ordering::Relaxed));
            assert_eq!(live.borrow().0, "vid123");
            let (_, _, lines) = engine.chat_sync(999, 0);
            assert_eq!(lines[0].text, "hi 😀 :yt:");
            assert_eq!(lines[0].emotes[0].0, ":yt:");
            assert_eq!(lines[0].color, "#AAAAAA");
        });
    }

    #[test]
    fn youtube_badges_colors_and_initial_history() {
        let act = json!({
            "addChatItemAction": {
                "item": {
                    "liveChatTextMessageRenderer": {
                        "id": "yt-m-1",
                        "authorName": {"simpleText": "@ModPerson"},
                        "authorExternalChannelId": "UCmod",
                        "timestampUsec": "1700000000000000",
                        "message": {"runs": [{"text": "welcome!"}]},
                        "authorBadges": [
                            {"liveChatAuthorBadgeRenderer": {"icon": {"iconType": "MODERATOR"}}},
                            {"liveChatAuthorBadgeRenderer": {"customThumbnail": {"thumbnails": [{"url": "https://yt3.ggpht.com/member.png"}]}}}
                        ]
                    }
                }
            }
        });
        let l = innertube_action_to_line(&act).expect("valid yt line");
        assert_eq!(l.user, "ModPerson");
        assert_eq!(l.color, "#5E84F1");
        assert!(l.moderator && l.subscriber && l.historical);
        assert_eq!(l.badges[0], ("youtube:moderator".into(), String::new()));
        assert_eq!(
            l.badges[1],
            ("youtube:member".into(), "https://yt3.ggpht.com/member.png".into())
        );

        let html = format!(
            "<script>var ytInitialData = {};</script>",
            json!({"contents": {"liveChatRenderer": {"actions": [act]}}})
        );
        assert_eq!(extract_initial_actions(&html).len(), 1);
    }
}
