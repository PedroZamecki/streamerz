//! YouTube integration for Streamerz (Phase 11):
//! - Zero-login public read via InnerTube / live_chat endpoint.
//! - Chat messages, Super Chats, Super Stickers, Memberships, Member Milestones, Gifts, Polls.
//! - Official YouTube Data API v3 with OAuth for sending chat, moderation, and updating broadcast metadata.
//! - Triggers (4001..=4031) and sub-actions (4001..=4012).

use crate::commands::Chat;
use crate::engine::Engine;
use crate::model::Obj;
use crate::runner::{ask, ConnectionCommand, Handle, ReplyErrors};
use crate::Config;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, watch};

/// Only supplied fields change. The API replaces each included metadata part.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct VideoUpdate {
    pub title: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub category: Option<String>,
    pub privacy: Option<String>,
}

fn video_update_body(id: &str, current: &Value, update: &VideoUpdate) -> Result<Value, String> {
    let snippet = current["snippet"]
        .as_object()
        .ok_or("YouTube: video metadata is unavailable")?;
    let mut kept = serde_json::Map::new();
    for key in [
        "title",
        "description",
        "tags",
        "categoryId",
        "defaultLanguage",
        "defaultAudioLanguage",
    ] {
        if let Some(value) = snippet.get(key) {
            kept.insert(key.into(), value.clone());
        }
    }
    for (key, value) in [
        ("title", &update.title),
        ("description", &update.description),
        ("categoryId", &update.category),
    ] {
        if let Some(value) = value {
            kept.insert(key.into(), value.clone().into());
        }
    }
    if let Some(tags) = &update.tags {
        kept.insert("tags".into(), json!(tags));
    }
    if kept.get("title").and_then(Value::as_str).is_none_or(str::is_empty)
        || kept.get("categoryId").and_then(Value::as_str).is_none_or(str::is_empty)
    {
        return Err("YouTube: the current title and category are required".into());
    }
    let mut body = json!({"id": id, "snippet": kept});
    if let Some(privacy) = &update.privacy {
        if !["public", "unlisted", "private"].contains(&privacy.as_str()) {
            return Err("YouTube: invalid privacy setting".into());
        }
        let status = current["status"]
            .as_object()
            .ok_or("YouTube: video privacy is unavailable")?;
        let mut kept = serde_json::Map::new();
        for key in [
            "embeddable",
            "license",
            "publicStatsViewable",
            "selfDeclaredMadeForKids",
            "containsSyntheticMedia",
        ] {
            if let Some(value) = status.get(key) {
                kept.insert(key.into(), value.clone());
            }
        }
        // A publishing schedule is valid only while private. An explicit change to public or
        // unlisted clears it, as in YouTube Studio; keeping private preserves the schedule.
        if privacy == "private" {
            if let Some(value) = status.get("publishAt") {
                kept.insert("publishAt".into(), value.clone());
            }
        }
        kept.insert("privacyStatus".into(), privacy.clone().into());
        body["status"] = kept.into();
    }
    Ok(body)
}

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

pub const UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

/// The text between the first `marker` in `page` and the next quote, with JSON's `\u0026` as `&`.
fn quoted_after(page: &[u8], marker: &[u8]) -> Option<String> {
    let at = page.windows(marker.len()).position(|w| w == marker)? + marker.len();
    let end = page[at..].iter().position(|b| *b == b'"')?;
    let text = std::str::from_utf8(&page[at..at + end]).ok()?;
    Some(text.replace("\\u0026", "&")).filter(|t| t.starts_with("http"))
}

/// A channel's banner and picture, read from its public page (the API has no banner for other people's
/// channels): what youtube.com's own channel card shows. None when it has none, or the page doesn't answer.
pub async fn public_channel(channel: &str) -> (Option<String>, Option<String>) {
    let (banner, picture, _) = public_profile(channel).await;
    (banner, picture)
}

/// Banner, picture and display name of a channel's public page.
pub async fn public_profile(channel: &str) -> (Option<String>, Option<String>, Option<String>) {
    let clean = channel.trim();
    if clean.is_empty() {
        return (None, None, None);
    }
    let path = channel_path(clean);
    // The cookie keeps the consent page out of the way.
    let headers = [
        ("Accept-Language".to_string(), "en".to_string()),
        ("Cookie".to_string(), "SOCS=CAI; CONSENT=YES+1".to_string()),
        ("User-Agent".to_string(), UA.to_string()),
    ];
    let url = format!("https://www.youtube.com{path}");
    let Ok(r) = crate::http::request("GET", &url, &headers, b"").await else {
        return (None, None, None);
    };
    if r.status != 200 {
        return (None, None, None);
    }
    let name = {
        let m = br#"<meta property="og:title" content=""#;
        r.body.windows(m.len()).position(|w| w == m).and_then(|at| {
            let rest = &r.body[at + m.len()..];
            let end = rest.iter().position(|b| *b == b'"')?;
            let t = std::str::from_utf8(&rest[..end]).ok()?;
            Some(
                t.replace("&amp;", "&")
                    .replace("&quot;", "\"")
                    .replace("&#39;", "'")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">"),
            )
            .filter(|t| !t.trim().is_empty())
        })
    };
    let avatar = quoted_after(&r.body, br#""avatar":{"thumbnails":[{"url":""#)
        .or_else(|| quoted_after(&r.body, br#""avatar":{"image":{"sources":[{"url":""#));
    (
        quoted_after(&r.body, br#""imageBannerViewModel":{"image":{"sources":[{"url":""#),
        avatar,
        name,
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
    if let Some(idx) = s.find("/@") {
        let rest = &s[idx + 2..];
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

impl YouTubeConfig {
    fn has_target(&self) -> bool {
        !self.video_id.trim().is_empty() || !self.channel_id.trim().is_empty()
    }

    fn refresh_target(
        &mut self,
        bot: bool,
        login: &YouTubeAccount,
        credentials: (&str, &str),
    ) -> Option<&mut YouTubeToken> {
        if (self.client_id.trim(), self.client_secret.trim()) != credentials {
            return None;
        }
        let account = if bot { &mut self.bot } else { &mut self.broadcaster };
        account
            .as_mut()
            .filter(|a| a.channel_id == login.channel_id && a.token == login.token)
            .map(|a| &mut a.token)
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct YouTubeAccount {
    pub channel_id: String,
    pub name: String,
    pub handle: String,
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
    /// Any Data API call as the broadcaster (method, path with its query, JSON body), with `{video}`
    /// and `{chat}` filled in from `broadcast` (empty: the live one).
    Data {
        method: &'static str,
        broadcast: String,
        path: String,
        body: Option<Value>,
        resp: oneshot::Sender<Result<Value, String>>,
    },
    Connect,
    /// The live video being read ("" when none).
    LiveVideo(oneshot::Sender<String>),
    /// The channel's live broadcast, else its next one, as (video id, title).
    NextBroadcast(oneshot::Sender<Result<Option<(String, String)>, String>>),
    /// One Data API GET as the broadcaster (path and query under /youtube/v3), the JSON answer.
    Api {
        path: String,
        resp: oneshot::Sender<Result<Value, String>>,
    },
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
    /// Title, description and tags in one videos.update (each one costs 50 units of quota).
    UpdateVideo {
        broadcast: String,
        update: VideoUpdate,
        resp: oneshot::Sender<Result<(), String>>,
    },
    BanUser {
        broadcast: String,
        user_id: String,
        resp: oneshot::Sender<Result<String, String>>,
    },
    TimeoutUser {
        broadcast: String,
        user_id: String,
        duration: u64,
        resp: oneshot::Sender<Result<String, String>>,
    },
    RemoveBan {
        id: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    DeleteMessage {
        message_id: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    AddModerator {
        broadcast: String,
        user_id: String,
        resp: oneshot::Sender<Result<String, String>>,
    },
    RemoveModerator {
        broadcast: String,
        moderator_id: String,
        user_id: String,
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
    SetThumbnail {
        broadcast: String,
        file_path: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    RunCommercial {
        broadcast: String,
        duration_secs: u64,
        resp: oneshot::Sender<Result<(), String>>,
    },
}

pub type YouTubeHandle = Handle<YouTubeCmd, YouTubeConnection>;

#[derive(Clone, Default)]
pub struct YouTubeConnection {
    connected: Arc<AtomicBool>,
    standby: Arc<AtomicBool>,
}

impl AsRef<AtomicBool> for YouTubeConnection {
    fn as_ref(&self) -> &AtomicBool {
        &self.connected
    }
}

impl ConnectionCommand for YouTubeCmd {
    const CONNECT: Self = Self::Connect;
    const DISCONNECT: Self = Self::Disconnect;
}

const REPLY_ERRORS: ReplyErrors = ReplyErrors::new("YouTube runner is down");

impl YouTubeHandle {
    pub fn is_standby(&self) -> bool {
        self.state.standby.load(Ordering::Relaxed) && !self.is_connected()
    }

    pub async fn send_message(&self, broadcast: &str, text: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::SendMessage {
            broadcast: broadcast.to_string(),
            text: text.to_string(),
            use_bot,
            fallback,
            resp,
        })
        .await
    }

    pub async fn set_title(&self, broadcast: &str, title: &str) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::SetTitle {
            broadcast: broadcast.to_string(),
            title: title.to_string(),
            resp,
        })
        .await
    }

    pub async fn set_description(&self, broadcast: &str, description: &str) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::SetDescription {
            broadcast: broadcast.to_string(),
            description: description.to_string(),
            resp,
        })
        .await
    }

    pub async fn set_title_and_description(
        &self,
        broadcast: &str,
        title: &str,
        description: &str,
    ) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::SetTitleAndDescription {
            broadcast: broadcast.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            resp,
        })
        .await
    }

    /// Change the supplied video metadata in one update.
    pub async fn update_video(&self, broadcast: &str, update: VideoUpdate) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::UpdateVideo {
            broadcast: broadcast.to_string(),
            update,
            resp,
        })
        .await
    }

    /// Any Data API call as the broadcaster; `{video}` and `{chat}` in the path or body are the
    /// broadcast's video and live chat.
    pub async fn data(
        &self,
        method: &'static str,
        broadcast: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::Data {
            method,
            broadcast: broadcast.to_string(),
            path: path.to_string(),
            body,
            resp,
        })
        .await
    }

    pub async fn ban_user(&self, broadcast: &str, user_id: &str) -> Result<(), String> {
        self.ban_user_tracked(broadcast, user_id, None).await.map(drop)
    }

    /// Returns the ban resource ID, which YouTube requires to remove it later.
    pub async fn ban_user_tracked(
        &self,
        broadcast: &str,
        user_id: &str,
        duration: Option<u64>,
    ) -> Result<String, String> {
        ask(&self.tx, REPLY_ERRORS, |resp| {
            if let Some(duration) = duration {
                YouTubeCmd::TimeoutUser {
                    broadcast: broadcast.to_string(),
                    user_id: user_id.to_string(),
                    duration,
                    resp,
                }
            } else {
                YouTubeCmd::BanUser {
                    broadcast: broadcast.to_string(),
                    user_id: user_id.to_string(),
                    resp,
                }
            }
        })
        .await
    }

    pub async fn timeout_user(&self, broadcast: &str, user_id: &str, duration: u64) -> Result<(), String> {
        self.ban_user_tracked(broadcast, user_id, Some(duration))
            .await
            .map(drop)
    }

    pub async fn remove_ban(&self, id: &str) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::RemoveBan {
            id: id.to_string(),
            resp,
        })
        .await
    }

    pub async fn delete_message(&self, message_id: &str) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::DeleteMessage {
            message_id: message_id.to_string(),
            resp,
        })
        .await
    }

    pub async fn add_moderator(&self, broadcast: &str, user_id: &str) -> Result<String, String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::AddModerator {
            broadcast: broadcast.to_string(),
            user_id: user_id.to_string(),
            resp,
        })
        .await
    }

    pub async fn remove_moderator(&self, broadcast: &str, moderator_id: &str, user_id: &str) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::RemoveModerator {
            broadcast: broadcast.to_string(),
            moderator_id: moderator_id.to_string(),
            user_id: user_id.to_string(),
            resp,
        })
        .await
    }

    pub async fn set_thumbnail(&self, broadcast: &str, file_path: &str) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::SetThumbnail {
            broadcast: broadcast.to_string(),
            file_path: file_path.to_string(),
            resp,
        })
        .await
    }

    pub async fn run_commercial(&self, broadcast: &str, duration_secs: u64) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::RunCommercial {
            broadcast: broadcast.to_string(),
            duration_secs,
            resp,
        })
        .await
    }

    /// The new poll's id.
    /// The live video's watch page facts: start time (Unix seconds; None when not live) and title.
    async fn live_page(&self) -> Result<(Option<i64>, String), String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(YouTubeCmd::LiveVideo(tx))
            .map_err(|_| REPLY_ERRORS.down.to_string())?;
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
            crate::fmt::parse_iso(t)
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

    /// The channel's live broadcast, else its next one (scheduled, or the ready one its next live
    /// uses), as (video id, title); None when there is neither.
    pub async fn next_broadcast(&self) -> Result<Option<(String, String)>, String> {
        ask(&self.tx, REPLY_ERRORS, YouTubeCmd::NextBroadcast).await
    }

    /// One Data API read as the broadcaster: `path` is under /youtube/v3, with its query
    /// (`videos?part=snippet&id=x`). Each read costs 1 unit of quota.
    pub async fn api(&self, path: &str) -> Result<Value, String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::Api {
            path: path.to_string(),
            resp,
        })
        .await
    }

    pub async fn live_title(&self) -> Result<String, String> {
        Ok(self.live_page().await?.1)
    }

    pub async fn create_poll(&self, broadcast: &str, question: &str, options: &[String]) -> Result<String, String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::CreatePoll {
            broadcast: broadcast.to_string(),
            question: question.to_string(),
            options: options.to_vec(),
            resp,
        })
        .await
    }

    pub async fn end_poll(&self, broadcast: &str) -> Result<(), String> {
        ask(&self.tx, REPLY_ERRORS, |resp| YouTubeCmd::EndPoll {
            broadcast: broadcast.to_string(),
            resp,
        })
        .await
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
    let state = YouTubeConnection::default();
    let handle = YouTubeHandle::new(tx, state.clone());

    let runner = YouTubeRunner {
        engine: engine.clone(),
        cfg: cfg.clone(),
        connected: state.connected,
        standby: state.standby,
        live: Default::default(),
        poll: Default::default(),
    };

    tokio::task::spawn_local(async move {
        let mut poll_task: Option<crate::emote_events::Abort> = None;

        while let Some(cmd) = rx.recv().await {
            match cmd {
                YouTubeCmd::LiveVideo(resp) => {
                    let _ = resp.send(runner.live.borrow().0.clone());
                }
                YouTubeCmd::NextBroadcast(resp) => {
                    let _ = resp.send(find_next_broadcast(&runner.engine, &runner.cfg).await);
                }
                YouTubeCmd::Api { path, resp } => {
                    let _ = resp.send(api_get(&runner.engine, &runner.cfg, &path).await);
                }
                YouTubeCmd::Connect => {
                    if !runner.cfg.borrow().youtube.has_target() {
                        continue;
                    }
                    if poll_task.as_ref().is_none_or(|t| t.0.is_finished()) {
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

                        let logout = broadcaster_logout(&runner.cfg);
                        let runner_clone = runner.clone();
                        poll_task = Some(crate::emote_events::Abort(tokio::task::spawn_local(async move {
                            runner_clone.monitor(logout, runner_clone.run_poll_loop()).await;
                        })));
                    } else {
                        runner.engine.live_wake.notify_waiters();
                    }
                }
                YouTubeCmd::Disconnect => {
                    let was_active = poll_task.as_ref().is_some_and(|task| !task.0.is_finished());
                    drop(poll_task.take());
                    runner.stop_monitoring(was_active);
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
                YouTubeCmd::UpdateVideo {
                    broadcast,
                    update,
                    resp,
                } => {
                    let _ = resp.send(runner.update_snippet(&broadcast, &update).await);
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
                YouTubeCmd::Data {
                    method,
                    broadcast,
                    path,
                    body,
                    resp,
                } => {
                    let r = runner.data_api(method, &broadcast, &path, body).await;
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
                YouTubeCmd::RemoveBan { id, resp } => {
                    let _ = resp.send(runner.remove_ban_api(&id).await);
                }
                YouTubeCmd::DeleteMessage { message_id, resp } => {
                    let _ = resp.send(runner.delete_message_api(&message_id).await);
                }
                YouTubeCmd::AddModerator {
                    broadcast,
                    user_id,
                    resp,
                } => {
                    let _ = resp.send(runner.add_moderator_api(&broadcast, &user_id).await);
                }
                YouTubeCmd::RemoveModerator {
                    broadcast,
                    moderator_id,
                    user_id,
                    resp,
                } => {
                    let _ = resp.send(runner.remove_moderator_api(&broadcast, &moderator_id, &user_id).await);
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
                YouTubeCmd::SetThumbnail {
                    broadcast,
                    file_path,
                    resp,
                } => {
                    let r = runner.set_thumbnail_api(&broadcast, &file_path).await;
                    let _ = resp.send(r);
                }
                YouTubeCmd::RunCommercial {
                    broadcast,
                    duration_secs,
                    resp,
                } => {
                    let r = runner.run_commercial_api(&broadcast, duration_secs).await;
                    let _ = resp.send(r);
                }
            }
        }
    });

    handle
}

const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_SCOPE: &str = "https://www.googleapis.com/auth/youtube.force-ssl";
static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    token_with(
        engine,
        cfg,
        bot,
        fallback,
        crate::oauth::token,
        crate::secrets::is_locked,
    )
    .await
}

async fn token_with(
    engine: &Rc<Engine>,
    cfg: &Arc<watch::Sender<Config>>,
    bot: bool,
    fallback: bool,
    exchange: impl AsyncFnOnce(&str, &[(&str, &str)]) -> Result<crate::oauth::Token, String>,
    locked: impl Fn() -> bool,
) -> Result<String, String> {
    let _one = REFRESH.lock().await;
    let (acc, bot, (id, secret)) = {
        let c = cfg.borrow();
        let (acc, bot) = crate::oauth::select_account(
            &c.youtube.broadcaster,
            &c.youtube.bot,
            bot,
            fallback,
            "YouTube",
            |a| &a.token.access_token,
            locked,
        )?;
        (acc, bot, client(&c))
    };
    if !crate::oauth::needs_refresh(acc.token.expires_at, &acc.token.refresh_token) {
        return Ok(acc.token.access_token);
    }
    crate::oauth::Refresh {
        platform: "YouTube",
        url: GOOGLE_TOKEN_URL,
        bot,
        credentials: (&id, &secret),
    }
    .run(
        engine,
        &acc.token.refresh_token,
        exchange,
        |e| format!("YouTube token refresh: {e}"),
        |c| {
            c.youtube
                .refresh_target(bot, &acc, (&id, &secret))
                .map(|token| (&mut token.access_token, &mut token.refresh_token, &mut token.expires_at))
        },
    )
    .await
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
    let r = call(&t.access, "GET", "channels?part=snippet&mine=true", None).await?;
    let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
    let ch = &v["items"][0];
    let channel_id = ch["id"]
        .as_str()
        .ok_or_else(|| format!("no YouTube channel on this account ({})", r.text()))?;
    Ok(YouTubeAccount {
        channel_id: channel_id.into(),
        name: ch["snippet"]["title"].as_str().unwrap_or(channel_id).into(),
        handle: ch["snippet"]["customUrl"].as_str().unwrap_or("").into(),
        token: YouTubeToken {
            access_token: t.access,
            refresh_token: t.refresh,
            expires_at: t.expires_at,
        },
    })
}

/// A broadcaster logout stops the active reader without requiring an extra command or a timer.
fn broadcaster_logout(cfg: &watch::Sender<Config>) -> impl Future<Output = ()> {
    let mut config = cfg.subscribe();
    let mut signed_in = config.borrow().youtube.broadcaster.is_some();
    async move {
        let _ = config
            .wait_for(|c| {
                let was_signed_in = signed_in;
                signed_in = c.youtube.broadcaster.is_some();
                was_signed_in && !signed_in
            })
            .await;
    }
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
    async fn monitor(&self, logout: impl Future<Output = ()>, reader: impl Future<Output = ()>) {
        tokio::select! {
            biased;
            () = logout => {}
            () = reader => {}
        }
        self.stop_monitoring(true);
    }

    fn stop_monitoring(&self, was_active: bool) {
        clear_poll();
        *self.live.borrow_mut() = Default::default();
        self.poll.borrow_mut().clear();
        self.engine.set_chat_viewers("youtube", None);
        BROADCAST.with_borrow_mut(Obj::clear);
        ACTIVE.with_borrow_mut(std::collections::HashMap::clear);
        self.engine.chat_polls_clear("youtube");
        self.engine.chat_unpinned("youtube", "");
        crate::host::polls_changed();
        if was_active {
            self.connected.store(false, Ordering::Relaxed);
            self.standby.store(false, Ordering::Relaxed);
            crate::host::changed();
            crate::say("YouTube: monitoring ended".into());
            let args = Obj::new();
            self.engine.fire(
                T_YT_BROADCAST_MONITORING_ENDED,
                "YouTubeBroadcastMonitoringEnded",
                "YouTube",
                "youtube",
                &args,
                |_| true,
            );
            self.engine.emit("YouTube", "BroadcastMonitoringEnded", || json!(args));
        }
    }

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
        let path = format!(
            "videos?part=liveStreamingDetails&id={}",
            crate::commands::url_encode(&video)
        );
        let r = call(token, "GET", &path, None).await?;
        let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
        let id = v
            .pointer("/items/0/liveStreamingDetails/activeLiveChatId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("YouTube: {video} has no active live chat (HTTP {})", r.status))?
            .to_string();
        if broadcast.is_empty() {
            let mut live = self.live.borrow_mut();
            if live.0 != video {
                return Err("YouTube: the live video changed during chat lookup".into());
            }
            live.1.clone_from(&id);
        }
        Ok(id)
    }

    async fn send_chat_api(&self, broadcast: &str, text: &str, use_bot: bool, fallback: bool) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, use_bot, fallback).await?;

        let live_chat_id = self.live_chat_id(broadcast, &token).await?;

        let path = "liveChat/messages?part=snippet";
        let payload = json!({
            "snippet": {
                "liveChatId": live_chat_id,
                "type": "textMessageEvent",
                "textMessageDetails": {
                    "messageText": text
                }
            }
        });

        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = call(&token, "POST", path, Some(&body)).await?;
        if (200..300).contains(&resp.status) {
            // Read the chat again soon, so the message shows up where it was sent from.
            let engine = self.engine.clone();
            tokio::task::spawn_local(async move {
                tokio::time::sleep(Duration::from_millis(700)).await;
                engine.chat_poke.notify_one();
            });
            Ok(())
        } else {
            Err(format!("YouTube send chat error {}: {}", resp.status, resp.text()))
        }
    }

    /// The video a change goes to: `broadcast`, else the live one, else the channel's next broadcast
    /// (scheduled, or the ready one its next live uses).
    async fn target(&self, broadcast: &str) -> Result<String, String> {
        if let Ok(video) = self.video(broadcast) {
            return Ok(video);
        }
        match find_next_broadcast(&self.engine, &self.cfg).await? {
            Some((id, _)) => Ok(id),
            None => Err(NO_TARGET.into()),
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

    /// Read the current resource so replacing a part preserves unrelated writable fields.
    async fn update_snippet(&self, broadcast: &str, update: &VideoUpdate) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let video = self.target(broadcast).await?;
        let part = if update.privacy.is_some() {
            "snippet,status"
        } else {
            "snippet"
        };
        let r = call(
            &token,
            "GET",
            &format!("videos?part={part}&id={}", crate::commands::url_encode(&video)),
            None,
        )
        .await?;
        if !(200..300).contains(&r.status) {
            return Err(format!("YouTube: can't read video {video} (HTTP {})", r.status));
        }
        let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
        let body = video_update_body(&video, &v["items"][0], update)?;
        let r = call(
            &token,
            "PUT",
            &format!("videos?part={part}"),
            Some(&serde_json::to_vec(&body).unwrap_or_default()),
        )
        .await?;
        if (200..300).contains(&r.status) {
            Ok(())
        } else {
            Err(format!("YouTube video update: HTTP {} {}", r.status, r.text()))
        }
    }

    async fn set_title_api(&self, broadcast: &str, title: &str) -> Result<(), String> {
        self.update_snippet(
            broadcast,
            &VideoUpdate {
                title: Some(title.into()),
                ..VideoUpdate::default()
            },
        )
        .await
    }

    async fn set_description_api(&self, broadcast: &str, description: &str) -> Result<(), String> {
        self.update_snippet(
            broadcast,
            &VideoUpdate {
                description: Some(description.into()),
                ..VideoUpdate::default()
            },
        )
        .await
    }

    async fn set_title_and_description_api(
        &self,
        broadcast: &str,
        title: &str,
        description: &str,
    ) -> Result<(), String> {
        self.update_snippet(
            broadcast,
            &VideoUpdate {
                title: Some(title.into()),
                description: Some(description.into()),
                ..VideoUpdate::default()
            },
        )
        .await
    }

    /// Any Data API call as the broadcaster (see `YouTubeHandle::data`).
    async fn data_api(
        &self,
        method: &'static str,
        broadcast: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let mut path = path.to_string();
        let mut body = body.map(|b| b.to_string()).unwrap_or_default();
        if path.contains("{video}") || body.contains("\"{video}\"") {
            let video = self.target(broadcast).await?;
            path = path.replace("{video}", &crate::commands::url_encode(&video));
            body = body.replace("\"{video}\"", &Value::from(video).to_string());
        }
        if path.contains("{chat}") || body.contains("\"{chat}\"") {
            let chat = self.live_chat_id(broadcast, &token).await?;
            path = path.replace("{chat}", &crate::commands::url_encode(&chat));
            body = body.replace("\"{chat}\"", &Value::from(chat).to_string());
        }
        let body = (!body.is_empty()).then_some(body.as_bytes());
        let r = call(&token, method, path.trim_start_matches('/'), body).await?;
        if !(200..300).contains(&r.status) {
            let said = serde_json::from_slice::<Value>(&r.body)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
                .unwrap_or_else(|| r.text());
            return Err(format!("HTTP {}: {said}", r.status));
        }
        Ok(serde_json::from_slice(&r.body).unwrap_or(Value::Null))
    }

    async fn ban_user_api(&self, broadcast: &str, user_id: &str) -> Result<String, String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;

        let live_chat_id = &self.live_chat_id(broadcast, &token).await?;

        let path = "liveChat/bans?part=snippet";
        let payload = live_chat_ban_payload(live_chat_id, user_id, None);

        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = call(&token, "POST", path, Some(&body)).await?;
        if (200..300).contains(&resp.status) {
            let id = ban_id(&resp.body)?;
            crate::core::moderation::remember_youtube(&self.engine, &id, user_id, live_chat_id, None);
            Ok(id)
        } else {
            Err(format!("YouTube ban error {}: {}", resp.status, resp.text()))
        }
    }

    async fn timeout_user_api(&self, broadcast: &str, user_id: &str, duration: u64) -> Result<String, String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;

        let live_chat_id = &self.live_chat_id(broadcast, &token).await?;

        let path = "liveChat/bans?part=snippet";
        let payload = live_chat_ban_payload(live_chat_id, user_id, Some(duration));

        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = call(&token, "POST", path, Some(&body)).await?;
        if (200..300).contains(&resp.status) {
            let id = ban_id(&resp.body)?;
            crate::core::moderation::remember_youtube(&self.engine, &id, user_id, live_chat_id, Some(duration));
            Ok(id)
        } else {
            Err(format!("YouTube timeout error {}: {}", resp.status, resp.text()))
        }
    }

    async fn remove_ban_api(&self, id: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let path = format!("liveChat/bans?id={}", crate::commands::url_encode(id));
        let resp = call(&token, "DELETE", &path, None).await?;
        // A 404: already lifted or run out, so it is forgotten all the same.
        if (200..300).contains(&resp.status) || resp.status == 404 {
            self.engine
                .change(|config| config.youtube_moderation_bans.retain(|ban| ban.id != id));
            Ok(())
        } else {
            Err(format!("YouTube unban error {}: {}", resp.status, resp.text()))
        }
    }

    async fn delete_message_api(&self, message_id: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let path = format!("liveChat/messages?id={}", crate::commands::url_encode(message_id));
        let resp = call(&token, "DELETE", &path, None).await?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!("YouTube delete message error {}: {}", resp.status, resp.text()))
        }
    }

    async fn add_moderator_api(&self, broadcast: &str, user_id: &str) -> Result<String, String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let live_chat_id = self.live_chat_id(broadcast, &token).await?;
        let path = "liveChat/moderators?part=snippet";
        let payload = json!({
            "snippet": {
                "liveChatId": live_chat_id,
                "moderatorDetails": {
                    "channelId": user_id
                }
            }
        });
        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = call(&token, "POST", path, Some(&body)).await?;
        if (200..300).contains(&resp.status) {
            let value: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
            Ok(value["id"].as_str().unwrap_or_default().to_string())
        } else {
            Err(format!("YouTube add moderator error {}: {}", resp.status, resp.text()))
        }
    }

    async fn remove_moderator_api(&self, broadcast: &str, moderator_id: &str, user_id: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let target_id = if !moderator_id.trim().is_empty() {
            moderator_id.trim().to_string()
        } else {
            let live_chat_id = self.live_chat_id(broadcast, &token).await?;
            let mut page_token = String::new();
            let mut pages = HashSet::new();
            let mut found = None;
            loop {
                let mut path = format!(
                    "liveChat/moderators?part=snippet&liveChatId={}&maxResults=50",
                    crate::commands::url_encode(&live_chat_id)
                );
                if !page_token.is_empty() {
                    path.push_str("&pageToken=");
                    path.push_str(&crate::commands::url_encode(&page_token));
                }
                let resp = call(&token, "GET", &path, None).await?;
                if !(200..300).contains(&resp.status) {
                    return Err(format!(
                        "YouTube list moderators error {}: {}",
                        resp.status,
                        resp.text()
                    ));
                }
                let value: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
                if let Some(item) = value["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|item| item["snippet"]["moderatorDetails"]["channelId"].as_str() == Some(user_id))
                {
                    if let Some(id) = item["id"].as_str().filter(|s| !s.is_empty()) {
                        found = Some(id.to_string());
                        break;
                    }
                }
                match value["nextPageToken"].as_str().filter(|s| !s.is_empty()) {
                    Some(next) if pages.insert(next.to_string()) => page_token = next.to_string(),
                    Some(_) => return Err("YouTube: repeated moderator page token".into()),
                    None => break,
                }
            }
            found.ok_or_else(|| format!("YouTube: moderator {user_id} not found in live chat"))?
        };
        let path = format!("liveChat/moderators?id={}", crate::commands::url_encode(&target_id));
        let resp = call(&token, "DELETE", &path, None).await?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!(
                "YouTube remove moderator error {}: {}",
                resp.status,
                resp.text()
            ))
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
        let r = call(
            &token,
            "POST",
            "liveChat/messages?part=snippet",
            Some(&serde_json::to_vec(&body).unwrap_or_default()),
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
        let id = self.poll.borrow().clone();
        if id.is_empty() {
            return Err("YouTube: no poll to end (only polls made here can be ended)".into());
        }
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let path = format!(
            "liveChat/messages/transition?id={}&status=closed&part=snippet",
            crate::commands::url_encode(&id)
        );
        let r = call(&token, "POST", &path, None).await?;
        if (200..300).contains(&r.status) {
            let mut poll = self.poll.borrow_mut();
            if *poll == id {
                poll.clear();
            }
            Ok(())
        } else {
            Err(format!("YouTube end poll: HTTP {} {}", r.status, r.text()))
        }
    }

    async fn set_thumbnail_api(&self, broadcast: &str, file_path: &str) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let video = self.target(broadcast).await?;
        let bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| format!("Failed to read thumbnail file: {e}"))?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Thumbnail file exceeds 2MB limit".into());
        }
        let mime = if file_path.ends_with(".png") {
            "image/png"
        } else {
            "image/jpeg"
        };
        let url = format!(
            "https://www.googleapis.com/upload/youtube/v3/thumbnails/set?videoId={}&uploadType=media",
            crate::commands::url_encode(&video)
        );
        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Content-Type".to_string(), mime.to_string()),
        ];
        let resp = crate::http::request("POST", &url, &headers, &bytes).await?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!("YouTube set thumbnail error {}: {}", resp.status, resp.text()))
        }
    }

    async fn run_commercial_api(&self, broadcast: &str, duration_secs: u64) -> Result<(), String> {
        let token = token(&self.engine, &self.cfg, false, false).await?;
        let video = self.video(broadcast)?;
        let duration = if duration_secs == 0 { 30 } else { duration_secs };
        let path = format!("liveBroadcasts/cuepoint?id={}", crate::commands::url_encode(&video));
        // A cuepoint resource, sent as is (no part, no snippet); it plays as soon as it can.
        let payload = json!({"cueType": "cueTypeAd", "durationSecs": duration});
        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let resp = call(&token, "POST", &path, Some(&body)).await?;
        if (200..300).contains(&resp.status) {
            Ok(())
        } else {
            Err(format!("YouTube cuepoint error {}: {}", resp.status, resp.text()))
        }
    }
}

pub const YOUTUBE_WEB: &str = "https://www.youtube.com";

/// The first `"key":"value"` in a page.
fn find<'a>(page: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\":\"");
    let i = page.find(&pat)? + pat.len();
    page[i..].split('"').next()
}

/// The watch page's live count, not the video's cumulative view count.
fn live_viewer_count(page: &str) -> Option<u64> {
    let start = page.find("\"videoViewCountRenderer\":{")?;
    let section = &page[start..];
    let end = section.find("\"originalViewCount\":\"")?;
    if !section[..end].contains("\"isLive\":true") {
        return None;
    }
    find(&section[end..], "originalViewCount")?.parse().ok()
}

#[cfg(test)]
#[test]
fn live_viewer_count_uses_live_count_only() {
    assert_eq!(
        live_viewer_count(r#"{"videoViewCountRenderer":{"isLive":true,"originalViewCount":"132589"}}"#),
        Some(132589)
    );
    assert_eq!(
        live_viewer_count(r#"{"videoViewCountRenderer":{"isLive":false,"originalViewCount":"132589"}}"#),
        None
    );
    assert_eq!(live_viewer_count(r#"{"viewCount":"123"}"#), None);
}

/// The channel's page path: `/channel/UC...` for an id, `/@name` for a handle (what people type).
pub fn channel_path(channel: &str) -> String {
    let c = channel.trim().trim_start_matches('@');
    if c.starts_with("UC") && c.len() >= 20 && c.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        format!("/channel/{c}")
    } else {
        format!("/@{c}")
    }
}

/// The channel's current live video (the page `<channel>/live` redirects to it), if live.
async fn live_video(web: &str, channel: &str) -> Result<String, String> {
    let headers = [
        ("User-Agent".to_string(), UA.to_string()),
        ("Accept-Language".to_string(), "en".to_string()),
    ];
    let r = crate::http::request("GET", &format!("{web}{}/live", channel_path(channel)), &headers, b"").await?;
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

/// Extract `ytInitialData` object from an HTML page, if present.
pub fn extract_initial_data(page: &str) -> Option<Value> {
    for marker in ["window[\"ytInitialData\"] = ", "var ytInitialData = "] {
        if let Some(idx) = page.find(marker) {
            let rest = page[idx + marker.len()..].trim_start();
            if let Some(end) = rest.find(";</script>") {
                if let Ok(v) = serde_json::from_str::<Value>(&rest[..end]) {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Recursively scan `ytInitialData` for videos marked as LIVE.
pub fn extract_live_streams_from_data(val: &Value, out: &mut Vec<(String, String)>) {
    fn scan(v: &Value, out: &mut Vec<(String, String)>) {
        match v {
            Value::Object(map) => {
                if let Some(vid) = map.get("videoId").and_then(Value::as_str) {
                    if vid.len() == 11 {
                        let is_live = map.get("badges").and_then(Value::as_array).is_some_and(|b| {
                            b.iter().any(|item| {
                                item.pointer("/metadataBadgeRenderer/style")
                                    .and_then(Value::as_str)
                                    .is_some_and(|s| s == "BADGE_STYLE_TYPE_LIVE_NOW")
                                    || item
                                        .pointer("/metadataBadgeRenderer/label")
                                        .and_then(Value::as_str)
                                        .is_some_and(|l| l.eq_ignore_ascii_case("live"))
                            })
                        }) || v.pointer("/thumbnailOverlays").and_then(Value::as_array).is_some_and(
                            |arr| {
                                arr.iter().any(|ov| {
                                    ov.pointer("/thumbnailOverlayTimeStatusRenderer/style")
                                        .and_then(Value::as_str)
                                        .is_some_and(|s| s == "LIVE")
                                })
                            },
                        ) || map.get("isLive").and_then(Value::as_bool).unwrap_or(false);

                        if is_live && !out.iter().any(|(id, _)| id == vid) {
                            let title = v
                                .pointer("/title/runs/0/text")
                                .or_else(|| v.pointer("/title/simpleText"))
                                .or_else(|| v.pointer("/headline/simpleText"))
                                .and_then(Value::as_str)
                                .unwrap_or("Livestream")
                                .to_string();
                            out.push((vid.to_string(), title));
                        }
                    }
                }
                for child in map.values() {
                    scan(child, out);
                }
            }
            Value::Array(arr) => {
                for item in arr {
                    scan(item, out);
                }
            }
            _ => {}
        }
    }
    scan(val, out);
}

/// Discover active livestreams on a YouTube channel.
/// Returns a list of `(video_id, title)`.
async fn discover_channel_lives(web: &str, channel: &str, run_once: bool) -> Vec<(String, String)> {
    let mut lives: Vec<(String, String)> = Vec::new();
    if run_once || web != YOUTUBE_WEB {
        if let Ok(canon_id) = live_video(web, channel).await {
            lives.push((canon_id, "YouTube Live".to_string()));
        }
        return lives;
    }
    let headers = [
        ("User-Agent".to_string(), UA.to_string()),
        ("Accept-Language".to_string(), "en".to_string()),
        ("Cookie".to_string(), "SOCS=CAI; CONSENT=YES+1".to_string()),
    ];

    // 1. Fetch channel's /streams page (where YouTube lists live and upcoming streams)
    let streams_url = format!("{web}{}/streams", channel_path(channel));
    if let Ok(r) = crate::http::request("GET", &streams_url, &headers, b"").await {
        let text = r.text();
        if let Some(val) = extract_initial_data(&text) {
            extract_live_streams_from_data(&val, &mut lives);
        }
    }

    // 2. Fetch channel's /live page to ensure canonical live is present
    if let Ok(canon_id) = live_video(web, channel).await {
        if !lives.iter().any(|(id, _)| id == &canon_id) {
            lives.push((canon_id, "YouTube Live".to_string()));
        }
    }

    lives
}

/// Extract `ytInitialData`'s initial `actions` array from a `/live_chat` popout HTML page, if present.
pub fn extract_initial_actions(page: &str) -> Vec<Value> {
    if let Some(v) = extract_initial_data(page) {
        if let Some(arr) = v
            .pointer("/contents/liveChatRenderer/actions")
            .and_then(Value::as_array)
        {
            return arr.clone();
        }
    }
    Vec::new()
}

/// What a change says when YouTube has nothing to change.
pub const NO_TARGET: &str =
    "YouTube: no live or scheduled stream to change (go live, or schedule one in YouTube Studio)";

/// The logged-in channel's live broadcast, else its next one (scheduled, or the ready one its next live
/// uses), as (video id, title); None when there is neither. One liveBroadcasts list: 1 unit of quota.
async fn find_next_broadcast(
    engine: &Rc<Engine>,
    cfg: &Arc<watch::Sender<Config>>,
) -> Result<Option<(String, String)>, String> {
    let tok = token(engine, cfg, false, false).await?;
    let path = "liveBroadcasts?part=snippet,status&mine=true&maxResults=10";
    let res = call(&tok, "GET", path, None).await?;
    if res.status != 200 {
        return Err(format!("YouTube broadcasts: HTTP {} {}", res.status, res.text()));
    }
    let v: Value = serde_json::from_slice(&res.body).unwrap_or_default();
    let items = v["items"].as_array().map_or(&[][..], Vec::as_slice);
    remember_broadcasts(items);
    Ok(pick_broadcast(items))
}

/// The live and upcoming broadcasts a list held, for a sub-action's Broadcast to pick from by title.
fn remember_broadcasts(items: &[Value]) {
    let found = items
        .iter()
        .filter(|item| {
            matches!(
                item.pointer("/status/lifeCycleStatus").and_then(Value::as_str),
                Some("live" | "liveStarting" | "ready" | "created" | "testing" | "testStarting")
            )
        })
        .filter_map(|item| {
            let id = item["id"].as_str().filter(|id| !id.is_empty())?;
            Some((
                id.to_owned(),
                item["snippet"]["title"].as_str().unwrap_or_default().to_owned(),
            ))
        })
        .collect();
    crate::catalog::set("youtube.broadcasts", found);
}

/// Send a Data API request. Each endpoint handles its status and response body.
async fn call(token: &str, method: &str, path: &str, body: Option<&[u8]>) -> Result<crate::http::Response, String> {
    let url = format!("{YOUTUBE_API_BASE}/{path}");
    let auth = ("Authorization".to_string(), format!("Bearer {token}"));
    let mut headers = [auth, Default::default()];
    let headers = if body.is_some() {
        headers[1] = ("Content-Type".to_string(), "application/json".to_string());
        &headers[..]
    } else {
        &headers[..1]
    };
    crate::http::request(method, &url, headers, body.unwrap_or_default()).await
}

/// One Data API GET as the broadcaster: the JSON answer, or YouTube's own error message.
async fn api_get(engine: &Rc<Engine>, cfg: &Arc<watch::Sender<Config>>, path: &str) -> Result<Value, String> {
    let tok = token(engine, cfg, false, false).await?;
    let res = call(&tok, "GET", path.trim_start_matches('/'), None).await?;
    let v: Value = serde_json::from_slice(&res.body).unwrap_or_default();
    if res.status == 200 {
        return Ok(v);
    }
    let why = v["error"]["message"].as_str().map_or_else(|| res.text(), str::to_owned);
    Err(format!("YouTube: HTTP {} {why}", res.status))
}

/// From a liveBroadcasts list: the live one, else the soonest of those still to come.
fn pick_broadcast(items: &[Value]) -> Option<(String, String)> {
    fn state(item: &Value) -> &str {
        item.pointer("/status/lifeCycleStatus")
            .and_then(Value::as_str)
            .unwrap_or("")
    }
    let found = items
        .iter()
        .find(|item| matches!(state(item), "live" | "liveStarting"))
        .or_else(|| {
            items
                .iter()
                .filter(|item| matches!(state(item), "ready" | "created" | "testing" | "testStarting"))
                .min_by_key(|item| {
                    item.pointer("/snippet/scheduledStartTime")
                        .and_then(Value::as_str)
                        .unwrap_or("~")
                })
        })?;
    let id = found["id"].as_str().filter(|id| !id.is_empty())?;
    Some((
        id.to_owned(),
        found["snippet"]["title"].as_str().unwrap_or_default().to_owned(),
    ))
}

/// Cooldown shared by the reader's automatic Data API calls.
#[derive(Default)]
struct BackgroundApi(std::cell::Cell<Option<Instant>>);

impl BackgroundApi {
    fn cooling_down(&self) -> bool {
        self.0.get().is_some_and(|until| Instant::now() < until)
    }

    async fn get(&self, token: &str, path: &str) -> Option<Value> {
        if self.cooling_down() {
            return None;
        }
        let r = call(token, "GET", path, None).await.ok()?;
        let v: Value = serde_json::from_slice(&r.body).ok()?;
        if let Some(wait) = quota_cooldown(r.status, &v) {
            self.0.set(Some(Instant::now() + wait));
        }
        (r.status == 200).then_some(v)
    }
}

fn quota_cooldown(status: u16, body: &Value) -> Option<Duration> {
    if status == 403
        && body["error"]["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|err| matches!(err["reason"].as_str(), Some("quotaExceeded" | "dailyLimitExceeded")))
    {
        Some(Duration::from_secs(3600))
    } else if matches!(status, 403 | 429) {
        Some(Duration::from_secs(300))
    } else {
        None
    }
}

fn own_oauth_channel(c: &YouTubeConfig, channel: &str) -> bool {
    c.broadcaster.as_ref().is_some_and(|b| {
        !b.token.access_token.is_empty()
            && (channel == b.channel_id || channel.eq_ignore_ascii_case(b.handle.trim_start_matches('@')))
    })
}

/// The signed-in channel's active video and next scheduled time from one quota-bearing list call.
async fn oauth_broadcast_hint(
    engine: &Rc<Engine>,
    cfg: &Arc<watch::Sender<Config>>,
    api: &BackgroundApi,
) -> Option<(Option<String>, Option<i64>)> {
    if api.cooling_down() {
        return None;
    }
    let tok = token(engine, cfg, false, false).await.ok()?;
    let v = api
        .get(&tok, "liveBroadcasts?part=snippet,status&mine=true&maxResults=5")
        .await?;
    remember_broadcasts(v["items"].as_array().map_or(&[][..], Vec::as_slice));
    let mut nearest_sched: Option<i64> = None;
    for item in v["items"].as_array().into_iter().flatten() {
        let state = item
            .pointer("/status/lifeCycleStatus")
            .and_then(Value::as_str)
            .unwrap_or("");
        if matches!(state, "live" | "liveStarting") {
            if let Some(id) = item["id"].as_str().filter(|s| !s.is_empty()) {
                return Some((Some(id.to_string()), None));
            }
        }
        if matches!(state, "ready" | "created" | "testing") {
            if let Some(ts) = item
                .pointer("/snippet/scheduledStartTime")
                .and_then(Value::as_str)
                .and_then(crate::fmt::parse_iso)
            {
                nearest_sched = Some(nearest_sched.map_or(ts, |prev| prev.min(ts)));
            }
        }
    }
    Some((None, nearest_sched))
}

/// Failed reads in a row before a chat session is given up and looked up again.
const RETRIES: u32 = 8;

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
        if on || sb {
            crate::core::status::clear_failure("youtube");
        }
        let mut ch = connected.swap(on, Ordering::Relaxed) != on;
        if let Some(s) = standby {
            ch |= s.swap(sb, Ordering::Relaxed) != sb;
        }
        if ch {
            crate::host::changed();
        }
    };
    let headers = [
        ("User-Agent".to_string(), UA.to_string()),
        ("Accept-Language".to_string(), "en".to_string()),
    ];
    let mut last_offline_note = String::new();
    let api = Rc::new(BackgroundApi::default());
    let mut last_oauth_check: Option<Instant> = None;
    let mut hint = (None, None);
    let mut last_channel = String::new();
    let mut recent = RecentChatActions::default();

    loop {
        let (video_cfg, channel, label) = {
            let c = cfg.borrow();
            (
                c.youtube.video_id.trim().to_string(),
                c.youtube.channel_id.trim().to_string(),
                channel_label(&c.youtube),
            )
        };
        if video_cfg.is_empty() && channel.is_empty() {
            set_state(false, false);
            return;
        }
        if last_channel != channel {
            last_channel.clone_from(&channel);
            hint = (None, None);
            last_oauth_check = None;
        }
        let video = if !video_cfg.is_empty() {
            Ok(video_cfg.clone())
        } else {
            let lives = discover_channel_lives(web, &channel, run_once).await;
            if lives.len() > 1 {
                let rx = crate::host::ask_youtube_stream(lives.clone());
                match rx.await {
                    Ok(Some(chosen)) => Ok(chosen),
                    _ => Ok(lives[0].0.clone()),
                }
            } else if let Some((id, _)) = lives.into_iter().next() {
                Ok(id)
            } else {
                if !run_once && web == YOUTUBE_WEB && own_oauth_channel(&cfg.borrow().youtube, &channel) {
                    if last_oauth_check.is_none_or(|t| t.elapsed() >= Duration::from_secs(300)) {
                        last_oauth_check = Some(Instant::now());
                        hint = oauth_broadcast_hint(engine, cfg, &api).await.unwrap_or_default();
                    }
                    hint.0.clone().ok_or_else(|| "not live".into())
                } else {
                    Err("not live".into())
                }
            }
        };
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
                if !is_offline {
                    crate::core::status::record_failure("youtube", crate::core::status::Failure::Connect);
                }
                set_state(false, is_offline);
                if !live.borrow().0.is_empty() {
                    *live.borrow_mut() = Default::default();
                    clear_poll();
                    if is_offline {
                        engine.chat_stream("youtube", false);
                    }
                    engine.chat_polls_clear("youtube");
                    engine.chat_unpinned("youtube", "");
                    crate::host::polls_changed();
                }
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
                let now_unix = crate::fmt::now_secs() as i64;
                let near_sched = hint.1.is_some_and(|ts| ts.abs_diff(now_unix) <= 900);
                let wait = if engine.stream_warming_up() {
                    Duration::from_secs(5)
                } else if near_sched {
                    Duration::from_secs(15)
                } else {
                    Duration::from_secs(30)
                };
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = engine.live_wake.notified() => {}
                }
                continue;
            }
        };
        let (Some(mut cont), version) = (
            find(&page, "continuation")
                .filter(|token| !token.is_empty())
                .map(String::from),
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
        recent.for_video(&video);
        *live.borrow_mut() = (video.clone(), String::new());
        clear_poll();
        engine.chat_polls_clear("youtube");
        engine.chat_unpinned("youtube", "");
        crate::host::polls_changed();
        crate::say(format!("YouTube: connected to live chat ({label}, video {video})"));
        engine.settle_role("youtube");
        let joined_usec = (crate::fmt::now_ms() as i64) * 1000;
        let mut initial_hist: Vec<crate::engine::chatlog::Line> = if !run_once {
            extract_initial_actions(&page)
                .iter()
                .filter_map(innertube_action_to_line)
                .collect()
        } else {
            Vec::new()
        };
        let mut welcomed = run_once;
        let mut extras = None;
        let json_headers = [("Content-Type".to_string(), "application/json".to_string())];
        let url = format!("{web}/youtubei/v1/live_chat/get_live_chat?prettyPrint=false");
        // A read that fails (a network blip, a 5xx) is retried on the same continuation with a growing
        // wait; only a long run of failures, or an answer that has no next page, ends the session.
        let mut failures = 0u32;
        let mut ended = false;
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
                other => {
                    let why = match other {
                        Ok(r) => format!("HTTP {}", r.status),
                        Err(e) => e,
                    };
                    failures += 1;
                    if failures == 1 {
                        crate::core::status::record_failure("youtube", crate::core::status::Failure::Closed);
                        crate::warn(format!("YouTube: chat read failed ({why}), retrying"));
                    }
                    if run_once || failures > RETRIES {
                        break;
                    }
                    tokio::time::sleep(crate::http::backoff(failures)).await;
                    continue;
                }
            };
            if failures > 0 {
                crate::say("YouTube: chat reading again".into());
                failures = 0;
            }
            let c = &v["continuationContents"]["liveChatContinuation"];
            let next = chat_continuation(c);
            if next.is_none() && extras.is_none() {
                ended = c.is_object();
                break;
            }
            if run_once || extras.is_none() {
                broadcast_started(engine, cfg, &api, &video).await;
                if !run_once {
                    extras = Some(crate::emote_events::Abort(tokio::task::spawn_local(while_live(
                        engine.clone(),
                        cfg.clone(),
                        api.clone(),
                        video.clone(),
                    ))));
                }
            }
            set_state(true, false);
            if !welcomed {
                welcomed = true;
                let mut live_actions = Vec::new();
                if let Some(actions) = c["actions"].as_array() {
                    for act in actions {
                        if is_history_item(act, joined_usec) {
                            if let Some(l) = innertube_action_to_line(act) {
                                initial_hist.push(l);
                            }
                        } else if recent.first(act) {
                            live_actions.push(act.clone());
                        }
                    }
                }
                engine.chat_welcome("youtube", std::mem::take(&mut initial_hist));
                if !live_actions.is_empty() {
                    handle_innertube_actions(engine, &live_actions);
                }
            } else if let Some(actions) = c["actions"].as_array() {
                for action in actions.iter().filter(|a| recent.first(a)) {
                    handle_innertube_actions(engine, std::slice::from_ref(action));
                }
            }
            if run_once {
                return;
            }
            let Some((token, wait)) = next else {
                ended = c.is_object();
                break;
            };
            cont = token.to_owned();
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = engine.chat_poke.notified() => {}
                _ = engine.live_wake.notified() => {}
            }
            let (cur_video, cur_ch) = {
                let c = cfg.borrow();
                (
                    c.youtube.video_id.trim().to_string(),
                    c.youtube.channel_id.trim().to_string(),
                )
            };
            if cur_video != video_cfg || cur_ch != channel {
                break;
            }
        }
        set_state(false, ended);
        *live.borrow_mut() = Default::default();
        clear_poll();
        engine.chat_polls_clear("youtube");
        engine.chat_unpinned("youtube", "");
        crate::host::polls_changed();
        let was_live = extras.is_some();
        drop(extras);
        ACTIVE.with_borrow_mut(std::collections::HashMap::clear);
        if ended && was_live {
            broadcast_ended(engine);
            crate::say(format!("YouTube: live stream ended ({label}); back to standby"));
        } else if was_live {
            engine.chat_stream("youtube", false);
            BROADCAST.with_borrow_mut(Obj::clear);
        }
        if ended && hint.0.as_deref() == Some(&video) {
            hint.0 = None;
        }
        if run_once {
            return;
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

fn action_renderer(action: &Value) -> Option<&Value> {
    action.pointer("/addChatItemAction/item")?.as_object()?.values().next()
}

fn is_history_item(action: &Value, joined_usec: i64) -> bool {
    action_renderer(action)
        .and_then(|r| r["timestampUsec"].as_str())
        .and_then(|ts| ts.parse::<i64>().ok())
        .is_some_and(|ts| ts > 0 && ts < joined_usec.saturating_sub(3_000_000))
}

/// Keep at least the last 256 item IDs, including across reader reconnects to the same video.
#[derive(Default)]
struct RecentChatActions {
    video: String,
    latest: HashSet<String>,
    previous: HashSet<String>,
}

impl RecentChatActions {
    fn for_video(&mut self, video: &str) {
        if self.video != video {
            self.video = video.to_owned();
            self.latest.clear();
            self.previous.clear();
        }
    }

    fn first(&mut self, action: &Value) -> bool {
        let Some(id) = action_renderer(action)
            .and_then(|r| r["id"].as_str())
            .filter(|id| !id.is_empty())
        else {
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

/// Find the next usable page, even when the response includes other continuation entries.
fn chat_continuation(chat: &Value) -> Option<(&str, Duration)> {
    for entry in chat["continuations"].as_array()?.iter().filter_map(Value::as_object) {
        for data in entry.values() {
            if let Some(token) = data["continuation"].as_str().filter(|t| !t.is_empty()) {
                let ms = data["timeoutMs"]
                    .as_u64()
                    .and_then(|ms| u32::try_from(ms).ok())
                    .unwrap_or(5000);
                return Some((token, Duration::from_millis(u64::from(ms.max(1000)))));
            }
        }
    }
    None
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
    (whole.saturating_mul(1_000_000).saturating_add(cents * 10_000), code)
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
    crate::platform_args::user(
        &mut a,
        "youtube",
        r["authorExternalChannelId"].clone(),
        &name,
        name.trim_start_matches('@'),
    );
    let mut put = |k: &str, v: Value| drop(a.insert(k.into(), v));
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
        let t = crate::fmt::civil(usec / 1_000_000);
        put("publishedAt", crate::fmt::date(&t, "o").into());
    }
    let c = engine.config();
    if let Some(b) = &c.youtube.broadcaster {
        crate::platform_args::broadcaster(
            &mut a,
            "broadcastUsername",
            b.channel_id.as_str().into(),
            b.name.as_str(),
            None,
        );
    }
    drop(c);
    BROADCAST.with_borrow(|b| a.extend(b.clone()));
    (a, owner, moderator, member)
}

static POLL: std::sync::LazyLock<std::sync::Mutex<Obj>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(serde_json::Map::new()));

/// Clear the active YouTube poll from memory.
pub fn clear_poll() {
    if let Ok(mut p) = POLL.lock() {
        p.clear();
    }
}

thread_local! {
    /// The live broadcast's variables (broadcastId, broadcast.*), set when its chat is read.
    static BROADCAST: std::cell::RefCell<Obj> = Default::default();
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
    POLL.lock().map(|p| p.clone()).unwrap_or_default()
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
            crate::core::moderation::emit_linked_moderation_event(
                engine,
                "youtube",
                crate::core::moderation::LinkedModerationEvent::Banned,
                &args,
            );
        }
        let shown = act.pointer("/showLiveChatActionPanelAction/panel/liveChatPollRenderer");
        let updated = act.pointer("/updateLiveChatPollAction/pollToUpdate/pollRenderer");
        if let Some(p) = shown.or(updated) {
            let mut args = BROADCAST.with_borrow(Clone::clone);
            args.extend(poll_args(p));
            if let Ok(mut lock) = POLL.lock() {
                *lock = args.clone();
            }
            engine.chat_poll(crate::engine::chatlog::Poll {
                platform: "youtube".into(),
                id: "youtube".into(),
                title: args.get("poll.question").and_then(Value::as_str).unwrap_or("").into(),
                prediction: false,
                choices: (0..args.get("poll.options.count").and_then(Value::as_u64).unwrap_or(0))
                    .map(|i| {
                        (
                            args.get(&format!("poll.option{i}.text"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            args.get(&format!("poll.option{i}.votes"))
                                .and_then(Value::as_u64)
                                .unwrap_or(0),
                        )
                    })
                    .collect(),
                ends: None,
                over: None,
            });
            crate::host::polls_changed();
            if shown.is_some() {
                yt(engine, T_YT_POLL_STARTED, "YouTubePollStarted", "PollStarted", &args);
            } else {
                yt(engine, T_YT_POLL_UPDATED, "YouTubePollUpdated", "PollUpdated", &args);
            }
        }
        if let Some(close) = act.get("closeLiveChatActionPanelAction") {
            let mut args = POLL.lock().map(|mut p| std::mem::take(&mut *p)).unwrap_or_default();
            if args.is_empty() {
                continue;
            }
            crate::host::polls_changed();
            engine.chat_polls_clear("youtube");
            args.insert("panelId".into(), close["targetPanelId"].clone());
            yt(engine, T_YT_POLL_CLOSED, "YouTubePollClosed", "PollClosed", &args);
        }
    }
}

/// The live video's broadcast.* (and broadcastId) for every YouTube trigger; Broadcast Started.
async fn broadcast_started(engine: &Rc<Engine>, cfg: &Arc<watch::Sender<Config>>, api: &BackgroundApi, video: &str) {
    engine.chat_stream("youtube", true);
    let mut b = Obj::new();
    b.insert("broadcastId".into(), video.into());
    b.insert("broadcast.id".into(), video.into());
    // The title and so on need the Data API (a login); without one, just the id.
    let token = if api.cooling_down() {
        None
    } else {
        token(engine, cfg, false, true).await.ok()
    };
    if let Some(t) = token {
        let path = format!("videos?part=snippet,status&id={}", crate::commands::url_encode(video));
        if let Some(v) = api.get(&t, &path).await {
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
    if !engine.chat_stream_ended("youtube") {
        crate::notify::send(
            crate::notify::Kind::Streams,
            "Streamerz",
            &crate::i18n::trf("chat.state_ended_title", &[("platform", "YouTube")]),
        );
    }
    engine.chat_stream("youtube", false);
    clear_poll();
    engine.chat_polls_clear("youtube");
    engine.chat_unpinned("youtube", "");
    crate::host::polls_changed();
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
async fn while_live(engine: Rc<Engine>, cfg: Arc<watch::Sender<Config>>, api: Rc<BackgroundApi>, video: String) {
    engine.set_chat_viewers("youtube", None);
    let channel = cfg.borrow().youtube.channel_id.clone();
    let codes = crate::emote_events::Codes {
        platform: "youtube",
        source: "YouTube",
        seventv: (T_YT_SEVENTV_EMOTE_ADDED, T_YT_SEVENTV_EMOTE_REMOVED),
        bttv: (T_YT_BTTV_EMOTE_ADDED, T_YT_BTTV_EMOTE_REMOVED),
    };
    let _emotes = (codes.wanted(&engine) && !channel.is_empty()).then(|| {
        crate::emote_events::Abort(tokio::task::spawn_local(crate::emote_events::watch(
            engine.clone(),
            codes,
            channel,
        )))
    });
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let (mut n, mut subs): (u64, Option<HashSet<String>>) = (0, None);
    loop {
        tick.tick().await;
        n += 1;
        if engine.viewers_wanted() {
            let headers = [
                ("User-Agent".to_string(), UA.to_string()),
                ("Accept-Language".to_string(), "en".to_string()),
            ];
            let url = format!("{YOUTUBE_WEB}/watch?v={video}");
            let count = match crate::http::request("GET", &url, &headers, b"").await {
                Ok(r) if r.status == 200 => live_viewer_count(&r.text()),
                _ => None,
            };
            engine.set_chat_viewers("youtube", count);
        }
        let base = BROADCAST.with_borrow(Clone::clone);
        if !api.cooling_down() && engine.uses_trigger(T_YT_STATISTICS_UPDATED) {
            if let Ok(t) = token(&engine, &cfg, false, true).await {
                let path = format!(
                    "videos?part=statistics,liveStreamingDetails&id={}",
                    crate::commands::url_encode(&video)
                );
                if let Some(v) = api.get(&t, &path).await {
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
        if n % 2 == 0 && !api.cooling_down() && engine.uses_trigger(T_YT_NEW_SUBSCRIBER) {
            if let Ok(t) = token(&engine, &cfg, false, false).await {
                let path = "subscriptions?part=subscriberSnippet&myRecentSubscribers=true&maxResults=50";
                if let Some(v) = api.get(&t, path).await {
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
    }
}

/// Build the documented liveChatBan resource for either kind of restriction.
fn ban_id(body: &[u8]) -> Result<String, String> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| "YouTube accepted the ban but returned an invalid response".to_string())?;
    value["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "YouTube accepted the ban but did not return its ID".to_string())
}

fn live_chat_ban_payload(live_chat_id: &str, user_id: &str, duration: Option<u64>) -> Value {
    let mut snippet = json!({
        "liveChatId": live_chat_id,
        "type": if duration.is_some() { "temporary" } else { "permanent" },
        "bannedUserDetails": { "channelId": user_id }
    });
    if let Some(duration) = duration {
        snippet["banDurationSeconds"] = duration.into();
    }
    json!({ "snippet": snippet })
}

#[cfg(test)]
mod tests {
    #[test]
    fn video_updates_preserve_writable_metadata_and_handle_publishing_schedules() {
        let current = json!({"snippet": {"title": "Old", "categoryId": "20", "tags": ["keep"],
            "description": "About", "defaultLanguage": "pt", "thumbnails": {"default": {}}, "channelId": "owner"},
            "status": {"privacyStatus": "private", "license": "creativeCommon", "embeddable": false,
                "publicStatsViewable": false, "selfDeclaredMadeForKids": true, "containsSyntheticMedia": true,
                "publishAt": "2027-01-01T00:00:00Z", "uploadStatus": "processed", "madeForKids": true}});
        let update = VideoUpdate {
            title: Some("New".into()),
            category: Some("10".into()),
            privacy: Some("private".into()),
            ..VideoUpdate::default()
        };
        let body = video_update_body("video", &current, &update).unwrap();
        assert_eq!(
            body["snippet"],
            json!({"title": "New", "categoryId": "10", "tags": ["keep"], "description": "About", "defaultLanguage": "pt"})
        );
        assert_eq!(
            body["status"],
            json!({"privacyStatus": "private", "license": "creativeCommon", "embeddable": false,
            "publicStatsViewable": false, "selfDeclaredMadeForKids": true, "containsSyntheticMedia": true, "publishAt": "2027-01-01T00:00:00Z"})
        );
        let update = VideoUpdate {
            privacy: Some("unlisted".into()),
            ..VideoUpdate::default()
        };
        let body = video_update_body("video", &current, &update).unwrap();
        assert_eq!(body["status"]["privacyStatus"], "unlisted");
        assert!(body["status"].get("publishAt").is_none());
        let body = video_update_body(
            "video",
            &current,
            &VideoUpdate {
                tags: Some(vec![]),
                ..VideoUpdate::default()
            },
        )
        .unwrap();
        assert_eq!(body["snippet"]["tags"], json!([]));
        assert!(body.get("status").is_none());
        assert!(video_update_body("video", &json!({"snippet": {"title": "Missing category"}}), &update).is_err());
        assert!(video_update_body("video", &json!({"snippet": current["snippet"]}), &update).is_err());
        assert!(video_update_body(
            "video",
            &current,
            &VideoUpdate {
                privacy: Some("friends".into()),
                ..VideoUpdate::default()
            }
        )
        .is_err());
    }

    /// A Broadcast field offers what YouTube listed live or still to come, by title.
    #[test]
    fn found_broadcasts_are_offered_by_title() {
        remember_broadcasts(&[
            json!({"id": "live1", "snippet": {"title": "Now"}, "status": {"lifeCycleStatus": "live"}}),
            json!({"id": "next1", "snippet": {"title": "Friday"}, "status": {"lifeCycleStatus": "ready"}}),
            json!({"id": "old1", "snippet": {"title": "Done"}, "status": {"lifeCycleStatus": "complete"}}),
        ]);
        assert_eq!(
            crate::catalog::choices("youtube.broadcasts"),
            vec![
                ("next1".to_string(), "Friday".to_string()),
                ("live1".to_string(), "Now".to_string())
            ]
        );
    }

    #[test]
    fn a_change_goes_to_the_live_broadcast_else_the_next_one() {
        let item = |id: &str, state: &str, at: &str| {
            json!({"id": id, "status": {"lifeCycleStatus": state},
                "snippet": {"title": format!("t{id}"), "scheduledStartTime": at}})
        };
        let later = item("b", "ready", "2026-12-02T10:00:00Z");
        let sooner = item("c", "created", "2026-12-01T10:00:00Z");
        let over = item("d", "complete", "2026-11-01T10:00:00Z");
        assert_eq!(
            pick_broadcast(&[later.clone(), sooner.clone(), over.clone()]),
            Some(("c".into(), "tc".into()))
        );
        let live = item("a", "live", "2026-11-11T10:00:00Z");
        assert_eq!(pick_broadcast(&[later, live, sooner]).unwrap().0, "a");
        assert_eq!(pick_broadcast(&[over]), None);
    }

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
    fn a_handle_or_an_id_finds_the_live_page() {
        assert_eq!(channel_path("UmananaAI"), "/@UmananaAI");
        assert_eq!(channel_path("@umananaai"), "/@umananaai");
        assert_eq!(
            channel_path("UCabcdefghijklmnopqrstuv"),
            "/channel/UCabcdefghijklmnopqrstuv"
        );
        // A handle that starts with UC isn't an id unless it is as long as one.
        assert_eq!(channel_path("UCLA"), "/@UCLA");
        assert_eq!(
            clean_youtube_channel("https://www.youtube.com/@umananaai/live"),
            "umananaai"
        );
        assert_eq!(clean_youtube_channel("@umananaai"), "umananaai");
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

    #[test]
    fn multiple_livestreams_detection() {
        let html = r#"
            <html><body>
            <script>var ytInitialData = {
                "contents": {
                    "items": [
                        {
                            "videoId": "stream11111",
                            "title": {"runs": [{"text": "Stream 1 Title"}]},
                            "badges": [{"metadataBadgeRenderer": {"style": "BADGE_STYLE_TYPE_LIVE_NOW"}}]
                        },
                        {
                            "videoId": "stream22222",
                            "headline": {"simpleText": "Stream 2 Headline"},
                            "thumbnailOverlays": [{"thumbnailOverlayTimeStatusRenderer": {"style": "LIVE"}}]
                        },
                        {
                            "videoId": "offline1234",
                            "title": {"simpleText": "Past Video"},
                            "badges": []
                        }
                    ]
                }
            };</script>
            </body></html>
        "#;
        let val = extract_initial_data(html).expect("initial data extracted");
        let mut lives = Vec::new();
        extract_live_streams_from_data(&val, &mut lives);
        assert_eq!(lives.len(), 2);
        assert_eq!(lives[0], ("stream11111".to_string(), "Stream 1 Title".to_string()));
        assert_eq!(lives[1], ("stream22222".to_string(), "Stream 2 Headline".to_string()));
    }

    use super::*;

    #[test]
    fn youtube_user_and_broadcaster_argument_bytes() {
        let mut config = Config::default();
        config.youtube.broadcaster = Some(YouTubeAccount {
            channel_id: "UC-alice".into(),
            name: "Alice".into(),
            ..Default::default()
        });
        let cfg = Arc::new(watch::channel(config).0);
        let engine = Engine::new(cfg.clone(), |_| {});
        let item = json!({
            "authorName": {"simpleText": "@Bob"},
            "authorExternalChannelId": "UC-bob",
            "id": "message",
        });
        let common = concat!(
            r#"{"user":"Bob","userName":"@Bob","userId":"UC-bob","userType":"youtube","userProfileUrl":"","#,
            r##""color":"#AAAAAA","badges":[],"isSubscribed":false,"userIsSponsor":false,"isSponsor":false,"##,
            r#""isModerator":false,"isOwner":false,"isVerified":false,"messageId":"message","id":"message""#,
        );
        let (args, _, _, _) = yt_base(&engine, &item);
        assert_eq!(
            serde_json::to_string(&args).unwrap(),
            format!(r#"{common},"broadcastUsername":"Alice","broadcastUserId":"UC-alice"}}"#)
        );

        cfg.send_modify(|config| config.youtube.broadcaster = None);
        let (args, _, _, _) = yt_base(&engine, &item);
        assert_eq!(serde_json::to_string(&args).unwrap(), format!("{common}}}"));
    }

    #[test]
    fn broadcaster_logout_cancels_youtube_reader() {
        crate::engine::tests::run(async {
            let mut config = Config::default();
            config.youtube.broadcaster = Some(YouTubeAccount::default());
            let cfg = Arc::new(watch::channel(config).0);
            let runner = YouTubeRunner {
                engine: Engine::new(cfg.clone(), |_| {}),
                cfg: cfg.clone(),
                connected: Arc::new(AtomicBool::new(true)),
                standby: Arc::new(AtomicBool::new(false)),
                live: Rc::new(std::cell::RefCell::new(("video".into(), "chat".into()))),
                poll: Rc::new(std::cell::RefCell::new("poll".into())),
            };
            let (send, recv) = oneshot::channel::<()>();
            let reader = async move {
                let _ = recv.await;
            };
            let logout = broadcaster_logout(&cfg);
            let mut monitor = std::pin::pin!(runner.monitor(logout, reader));
            assert!(futures_util::poll!(&mut monitor).is_pending());
            cfg.send_modify(|c| c.youtube.broadcaster = None);
            assert!(futures_util::poll!(&mut monitor).is_ready());
            assert!(send.is_closed(), "the in-flight reader was dropped");
            assert!(!runner.connected.load(Ordering::Relaxed));
            assert!(runner.live.borrow().0.is_empty());
            assert!(runner.poll.borrow().is_empty());
        });
    }

    #[test]
    fn oauth_selects_the_account_and_keeps_login_errors() {
        crate::engine::tests::run(async {
            let cfg = Arc::new(watch::channel(Config::default()).0);
            let engine = Engine::new(cfg.clone(), |_| {});
            let unused = async |_: &str, _: &[(&str, &str)]| panic!("should not refresh");
            assert_eq!(
                token_with(&engine, &cfg, true, false, unused, || false).await,
                Err("YouTube bot isn't logged in".into())
            );
            assert_eq!(
                token_with(&engine, &cfg, true, true, unused, || false).await,
                Err("No YouTube account logged in".into())
            );
            assert_eq!(
                token_with(&engine, &cfg, true, true, unused, || true).await,
                Err("System keyring is locked; unlock it to use saved YouTube login".into())
            );
            cfg.send_modify(|c| {
                c.youtube.broadcaster = Some(YouTubeAccount {
                    token: YouTubeToken {
                        access_token: "broadcaster".into(),
                        expires_at: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                });
                c.youtube.bot = Some(YouTubeAccount {
                    token: YouTubeToken {
                        access_token: "bot".into(),
                        expires_at: u64::MAX,
                        refresh_token: "unused".into(),
                    },
                    ..Default::default()
                });
            });
            assert_eq!(
                token_with(&engine, &cfg, true, true, unused, || false).await,
                Ok("bot".into())
            );
            cfg.send_modify(|c| c.youtube.bot.as_mut().unwrap().token.access_token.clear());
            assert_eq!(
                token_with(&engine, &cfg, true, true, unused, || false).await,
                Ok("broadcaster".into())
            );
        });
    }

    #[test]
    fn oauth_refresh_uses_the_selected_account_and_preserves_the_latest_login() {
        let _guard = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let cfg = Arc::new(watch::channel(Config::default()).0);
            let engine = Engine::new(cfg.clone(), |_| {});
            cfg.send_modify(|c| {
                c.youtube.client_id = " app ".into();
                c.youtube.client_secret = " placeholder ".into();
                c.youtube.broadcaster = Some(YouTubeAccount {
                    token: YouTubeToken {
                        access_token: "old".into(),
                        refresh_token: "original".into(),
                        expires_at: 1,
                    },
                    ..Default::default()
                });
            });
            for (before, refresh, after) in [("original", "", "original"), ("original", "rotated", "rotated")] {
                let exchange = async |url: &str, form: &[(&str, &str)]| {
                    assert_eq!(url, GOOGLE_TOKEN_URL);
                    assert_eq!(
                        form,
                        &[
                            ("grant_type", "refresh_token"),
                            ("refresh_token", before),
                            ("client_id", "app"),
                            ("client_secret", "placeholder"),
                        ]
                    );
                    Ok(crate::oauth::Token {
                        access: "refreshed".into(),
                        refresh: refresh.into(),
                        expires_at: 60,
                    })
                };
                assert_eq!(
                    token_with(&engine, &cfg, true, true, exchange, || false).await,
                    Ok("refreshed".into())
                );
                {
                    let config = cfg.borrow();
                    let token = &config.youtube.broadcaster.as_ref().unwrap().token;
                    assert_eq!(
                        (
                            token.access_token.as_str(),
                            token.refresh_token.as_str(),
                            token.expires_at
                        ),
                        ("refreshed", after, 60)
                    );
                    assert!(config.youtube.bot.is_none());
                }
            }
            let exchange = async |_: &str, _: &[(&str, &str)]| Err("token request failed (HTTP 500): detail".into());
            assert_eq!(
                token_with(&engine, &cfg, false, false, exchange, || false).await,
                Err("YouTube token refresh: token request failed (HTTP 500): detail".into())
            );
            let exchange = async |_: &str, _: &[(&str, &str)]| {
                cfg.send_modify(|c| c.youtube.broadcaster.as_mut().unwrap().token.access_token = "replacement".into());
                Ok(crate::oauth::Token {
                    access: "superseded".into(),
                    refresh: "other".into(),
                    expires_at: 50,
                })
            };
            assert_eq!(
                token_with(&engine, &cfg, false, false, exchange, || false).await,
                Err("YouTube: login changed during token refresh".into())
            );
            let config = cfg.borrow();
            let token = &config.youtube.broadcaster.as_ref().unwrap().token;
            assert_eq!(
                (token.access_token.as_str(), token.refresh_token.as_str()),
                ("replacement", "rotated")
            );
        });
    }

    #[test]
    fn oauth_refresh_does_not_replace_a_new_login() {
        let login = YouTubeAccount {
            channel_id: "UCoriginal".into(),
            token: YouTubeToken {
                access_token: "old".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut cfg = YouTubeConfig {
            broadcaster: Some(login.clone()),
            client_id: "app".into(),
            client_secret: "placeholder".into(),
            ..Default::default()
        };
        let credentials = ("app", "placeholder");
        assert!(cfg.refresh_target(false, &login, credentials).is_some());
        cfg.broadcaster.as_mut().unwrap().token.access_token = "new".into();
        assert!(cfg.refresh_target(false, &login, credentials).is_none());
        cfg.broadcaster = None;
        assert!(cfg.refresh_target(false, &login, credentials).is_none());
        cfg.broadcaster = Some(login.clone());
        cfg.client_secret = "replacement".into();
        assert!(cfg.refresh_target(false, &login, credentials).is_none());
    }

    #[test]
    fn empty_youtube_configuration_has_no_reader_target() {
        let mut c = YouTubeConfig {
            channel_id: " ".into(),
            ..Default::default()
        };
        assert!(!c.has_target());
        c.video_id = " abc123 ".into();
        assert!(c.has_target());
    }

    #[test]
    fn quota_errors_slow_automatic_calls() {
        let quota = json!({"error": {"code": 403, "errors": [
            {"domain": "youtube.quota", "reason": "quotaExceeded"}
        ]}});
        assert_eq!(quota_cooldown(403, &quota), Some(Duration::from_secs(3600)));
        assert_eq!(quota_cooldown(429, &Value::Null), Some(Duration::from_secs(300)));
        assert_eq!(quota_cooldown(500, &Value::Null), None);
    }

    #[test]
    fn oauth_discovery_is_scoped_to_the_channel_being_read() {
        let cfg = YouTubeConfig {
            broadcaster: Some(YouTubeAccount {
                channel_id: "UCmine".into(),
                handle: "@MyHandle".into(),
                token: YouTubeToken {
                    access_token: "example".into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(own_oauth_channel(&cfg, "UCmine"));
        assert!(own_oauth_channel(&cfg, "myhandle"));
        assert!(!own_oauth_channel(&cfg, "another-streamer"));
    }

    #[test]
    fn innertube_page_keeps_the_server_wait_and_skips_empty_tokens() {
        let reply = json!({"continuations": [
            {"timedContinuationData": {"continuation": ""}},
            {"invalidationContinuationData": {"continuation": "0ofMyContinuation==", "timeoutMs": 12000}}
        ]});
        assert_eq!(
            chat_continuation(&reply),
            Some(("0ofMyContinuation==", Duration::from_secs(12)))
        );
        assert_eq!(chat_continuation(&json!({"actions": []})), None);
    }

    #[test]
    fn super_chat_amount_cannot_overflow_micros() {
        assert_eq!(money("$18446744073709551615.99"), (u64::MAX, "USD".into()));
    }

    #[test]
    fn replayed_paid_items_are_history_and_repeated_ids_are_scoped_to_the_video() {
        let donation = json!({"addChatItemAction": {"item": {"liveChatPaidMessageRenderer": {
            "id": "LCC.donation-1", "timestampUsec": "1700000000000000",
            "purchaseAmountText": {"simpleText": "$5.00"}
        }}}});
        assert!(is_history_item(&donation, 1_700_000_010_000_000));
        assert!(!is_history_item(&donation, 1_700_000_001_000_000));
        let mut recent = RecentChatActions::default();
        recent.for_video("same");
        assert!(recent.first(&donation));
        recent.for_video("same");
        assert!(!recent.first(&donation));
        recent.for_video("other");
        assert!(recent.first(&donation));
    }

    #[test]
    fn live_chat_ban_uses_documented_user_and_duration_fields() {
        assert_eq!(
            live_chat_ban_payload("chat", "user", None),
            json!({
                "snippet": { "liveChatId": "chat", "type": "permanent", "bannedUserDetails": { "channelId": "user" } }
            })
        );
        assert_eq!(
            live_chat_ban_payload("chat", "user", Some(300)),
            json!({
                "snippet": { "liveChatId": "chat", "type": "temporary", "banDurationSeconds": 300,
                    "bannedUserDetails": { "channelId": "user" } }
            })
        );
    }

    #[test]
    fn successful_ban_requires_a_usable_resource_id() {
        assert_eq!(ban_id(br#"{"id":"ban-1"}"#).unwrap(), "ban-1");
        assert!(ban_id(br#"{}"#).unwrap_err().contains("accepted the ban"));
        assert!(ban_id(b"not json").is_err());
    }

    #[test]
    fn pinned_banners_come_and_go() {
        use crate::engine::tests::{engine, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
                    let body = if head.starts_with("GET /channel/UCxxxxxxxxxxxxxxxxxxxxxx/live") {
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
            config.youtube.channel_id = "UCxxxxxxxxxxxxxxxxxxxxxx".into();
            let on = |id: &str, code: u32| {
                let mut a = crate::engine::tests::action(id, id, json!([]));
                a["triggers"] = json!([{"id": "t", "type": code, "enabled": true}]);
                a
            };
            config.actions = serde_json::from_value(json!({"actions": [
                on("started", T_YT_BROADCAST_STARTED)]}))
            .unwrap();
            let cfg = Arc::new(watch::channel(config).0);
            let engine = Engine::new(cfg.clone(), |_| {});
            let connected = Arc::new(AtomicBool::new(false));
            let live = Rc::new(std::cell::RefCell::new((String::new(), String::new())));
            read_chat(&engine, &cfg, &connected, &live, &format!("http://{addr}"), true).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            let ran: Vec<String> = engine.history().iter().map(|r| r.name.clone()).collect();
            assert_eq!(ran, ["started"], "Broadcast Started fires once the live is found");
            assert!(!engine.first_words("youtube", "ucpoll"));
            assert!(connected.load(Ordering::Relaxed));
            assert_eq!(live.borrow().0, "vid123");
            let (_, _, lines) = engine.chat_sync(999, 0);
            assert_eq!(lines[0].text, "hi 😀 :yt:");
            assert_eq!(lines[0].emotes[0].0, ":yt:");
            assert_eq!(lines[0].color, "#AAAAAA");
        });
    }

    /// A read that fails once is retried on the same continuation: the session isn't dropped and found
    /// again (which would announce the stream ended and started again).
    #[test]
    fn a_failed_read_is_retried_without_a_new_session() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let pages = Rc::new(std::cell::Cell::new(0));
            let (served, reads) = (pages.clone(), Rc::new(std::cell::Cell::new(0)));
            let reads_seen = reads.clone();
            tokio::task::spawn_local(async move {
                loop {
                    let (mut sock, _) = listener.accept().await.unwrap();
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap();
                    let head = String::from_utf8_lossy(&buf[..n]).to_string();
                    let (status, body) = if head.starts_with("GET /@someone/live") {
                        (200, r#"<link rel="canonical" href="https://www.youtube.com/watch?v=vid9"> "isLive":true"#.to_string())
                    } else if head.starts_with("GET /live_chat") {
                        served.set(served.get() + 1);
                        (200, r#"x "INNERTUBE_CONTEXT_CLIENT_VERSION":"2.1" y "continuation":"first" z"#.to_string())
                    } else {
                        reads.set(reads.get() + 1);
                        if reads.get() == 1 {
                            (500, "oops".to_string())
                        } else {
                            assert!(head.contains("\"continuation\":\"first\""), "{head}");
                            (200, json!({"continuationContents": {"liveChatContinuation": {
                                "continuations": [{"invalidationContinuationData": {"continuation": "next", "timeoutMs": 10000}}],
                                "actions": [{"addChatItemAction": {"item": {"liveChatTextMessageRenderer": {
                                    "id": "m2", "authorName": {"simpleText": "Late"}, "authorExternalChannelId": "UCLate",
                                    "message": {"runs": [{"text": "still here"}]}
                                }}}}]
                            }}}).to_string())
                        }
                    };
                    let r = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    sock.write_all(r.as_bytes()).await.unwrap();
                }
            });
            let mut config = Config::default();
            // The handle people type, not an id.
            config.youtube.channel_id = "someone".into();
            let cfg = Arc::new(watch::channel(config).0);
            let engine = Engine::new(cfg.clone(), |_| {});
            let connected = Arc::new(AtomicBool::new(false));
            let live = Rc::new(std::cell::RefCell::new((String::new(), String::new())));
            let web = format!("http://{addr}");
            let _ = tokio::time::timeout(
                Duration::from_millis(3500),
                read_chat(&engine, &cfg, &connected, &live, &web, false),
            )
            .await;
            assert_eq!(pages.get(), 1, "one session");
            assert!(reads_seen.get() >= 2);
            let (_, _, lines) = engine.chat_sync(999, 0);
            assert!(lines.iter().any(|l| l.text == "still here"));
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

    #[test]
    fn youtube_thumbnail_and_commercial_metadata() {
        assert_eq!(
            crate::types::name(crate::types::SUB_ACTIONS, 4013),
            Some("YouTubeSetThumbnail")
        );
        assert_eq!(
            crate::types::name(crate::types::SUB_ACTIONS, 4014),
            Some("YouTubeRunCommercial")
        );

        let title_of = |code| {
            crate::types::SUB_ACTION_INFO
                .binary_search_by_key(&code, |x| x.0)
                .ok()
                .map(|i| crate::types::SUB_ACTION_INFO[i].1)
        };
        assert_eq!(title_of(4013), Some("Set Thumbnail"));
        assert_eq!(title_of(4014), Some("Run Commercial"));

        // Verify commercial payload format
        let duration = 30u64;
        let payload = json!({
            "snippet": {
                "cueType": "cueTypeAd",
                "durationSecs": duration
            }
        });
        assert_eq!(payload["snippet"]["cueType"], "cueTypeAd");
        assert_eq!(payload["snippet"]["durationSecs"], 30);
    }
}
