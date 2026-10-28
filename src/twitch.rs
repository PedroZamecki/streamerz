//! Twitch integration for Streamerz (Phase 6):
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
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};

/// Public Client ID from the registered Twitch Developer application.
pub const CLIENT_ID: &str = "u1ojrkynb7b8fwcvs8ve3l3c2o838l";
pub const DEFAULT_EVENTSUB_URL: &str = "wss://eventsub.wss.twitch.tv/ws";
pub const HELIX_URL: &str = "https://api.twitch.tv/helix";
pub const OAUTH_DEVICE_URL: &str = "https://id.twitch.tv/oauth2/device";
pub const OAUTH_TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";

pub const BROADCASTER_SCOPES: &[&str] = &[
    "user:read:chat",
    "user:write:chat",
    "user:read:emotes",
    "user:bot",
    "channel:bot",
    "channel:read:subscriptions",
    "bits:read",
    "moderator:read:followers",
    "channel:manage:raids",
    "moderator:manage:announcements",
    "user:read:whispers",
    "user:manage:whispers",
    "channel:read:redemptions",
    "channel:manage:redemptions",
    "channel:read:polls",
    "channel:manage:polls",
    "channel:read:predictions",
    "channel:manage:predictions",
    "channel:read:hype_train",
    "channel:read:goals",
    "channel:edit:commercial",
    "channel:read:ads",
    "channel:manage:ads",
    "channel:manage:broadcast",
    "clips:edit",
    "moderation:read",
    "channel:moderate",
    "moderator:manage:banned_users",
    "moderator:read:banned_users",
    "moderator:manage:chat_messages",
    "moderator:read:chat_settings",
    "moderator:manage:chat_settings",
    "moderator:manage:shield_mode",
    "moderator:read:shield_mode",
    "moderator:manage:warnings",
    "moderator:read:warnings",
    "moderator:manage:unban_requests",
    "moderator:read:unban_requests",
    "moderator:manage:automod",
    "moderator:read:automod_settings",
    "moderator:manage:shoutouts",
    "moderator:read:shoutouts",
    "moderator:read:suspicious_users",
    "moderator:manage:suspicious_users",
    "moderator:read:chatters",
    // channel.moderate (shared chat bans and deletes) wants these too.
    "moderator:read:blocked_terms",
    "moderator:read:moderators",
    "moderator:read:vips",
    "channel:manage:moderators",
    "channel:manage:vips",
    "channel:read:vips",
    "channel:read:charity",
    "channel:read:guest_star",
    "channel:manage:guest_star",
];

pub const BOT_SCOPES: &[&str] = &[
    "user:read:chat",
    "user:write:chat",
    "user:read:emotes",
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

// Phase 7 (Twitch B) Trigger codes
pub const T_HYPE_TRAIN_START: u32 = 108;
pub const T_HYPE_TRAIN_UPDATE: u32 = 109;
pub const T_HYPE_TRAIN_LEVEL_UP: u32 = 110;
pub const T_HYPE_TRAIN_END: u32 = 111;
pub const T_REWARD_REDEMPTION: u32 = 112;
pub const T_REWARD_CREATED: u32 = 113;
pub const T_REWARD_UPDATED: u32 = 114;
pub const T_REWARD_DELETED: u32 = 115;
pub const T_COMMUNITY_GOAL_CONTRIBUTION: u32 = 116;
pub const T_COMMUNITY_GOAL_ENDED: u32 = 117;
pub const T_STREAM_UPDATE: u32 = 118;
pub const T_POLL_CREATED: u32 = 125;
pub const T_POLL_UPDATED: u32 = 126;
pub const T_POLL_COMPLETED: u32 = 127;
pub const T_PREDICTION_CREATED: u32 = 128;
pub const T_PREDICTION_UPDATED: u32 = 129;
pub const T_PREDICTION_COMPLETED: u32 = 130;
pub const T_PREDICTION_CANCELED: u32 = 131;
pub const T_PREDICTION_LOCKED: u32 = 132;
pub const T_AD_RUN: u32 = 139;
pub const T_GOAL_BEGIN: u32 = 148;
pub const T_GOAL_PROGRESS: u32 = 149;
pub const T_GOAL_END: u32 = 150;
pub const T_POLL_TERMINATED: u32 = 161;
pub const T_VIEWER_COUNT_UPDATE: u32 = 164;
pub const T_REWARD_REDEMPTION_UPDATED: u32 = 171;
pub const T_UPCOMING_AD: u32 = 186;
pub const T_AUTOMATIC_REWARD_REDEMPTION: u32 = 194;
pub const T_POWER_UP_REDEMPTION: u32 = 196;
pub const T_POLL_ARCHIVED: u32 = 473;
pub const T_CUSTOM_POWER_UP_REDEMPTION: u32 = 496;

// Phase 8 (Twitch C) Trigger codes
pub const T_SUB_COUNTER_ROLLOVER: u32 = 121;
pub const T_PRESENT_VIEWERS: u32 = 124;
pub const T_CHAT_MESSAGE_DELETED: u32 = 135;
pub const T_USER_TIMED_OUT: u32 = 136;
pub const T_USER_BANNED: u32 = 137;
pub const T_CHARITY_DONATION: u32 = 141;
pub const T_CHARITY_COMPLETED: u32 = 142;
pub const T_SHOUTOUT_CREATED: u32 = 144;
pub const T_USER_UNTIMED_OUT: u32 = 145;
pub const T_CHARITY_STARTED: u32 = 146;
pub const T_CHARITY_PROGRESS: u32 = 147;
pub const T_SHIELD_MODE_BEGIN: u32 = 151;
pub const T_SHIELD_MODE_END: u32 = 152;
pub const T_SHOUTOUT_RECEIVED: u32 = 156;
pub const T_CHAT_CLEARED: u32 = 157;
pub const T_PYRAMID_SUCCESS: u32 = 162;
pub const T_PYRAMID_BROKEN: u32 = 163;
pub const T_GUEST_STAR_SESSION_BEGIN: u32 = 165;
pub const T_GUEST_STAR_SESSION_END: u32 = 166;
pub const T_GUEST_STAR_GUEST_UPDATE: u32 = 167;
pub const T_GUEST_STAR_SETTINGS_UPDATE: u32 = 169;
pub const T_SEVEN_TV_EMOTE_ADDED: u32 = 180;
pub const T_SEVEN_TV_EMOTE_REMOVED: u32 = 181;
pub const T_BETTER_TTV_EMOTE_ADDED: u32 = 182;
pub const T_BETTER_TTV_EMOTE_REMOVED: u32 = 183;
pub const T_MODERATOR_ADDED: u32 = 187;
pub const T_MODERATOR_REMOVED: u32 = 188;
pub const T_VIP_ADDED: u32 = 189;
pub const T_VIP_REMOVED: u32 = 190;
pub const T_USER_UNBANNED: u32 = 191;
pub const T_UNBAN_REQUEST_APPROVED: u32 = 192;
pub const T_UNBAN_REQUEST_DENIED: u32 = 193;
pub const T_UNBAN_REQUEST_CREATED: u32 = 195;
pub const T_CHAT_MODE_EMOTE_ON: u32 = 451;
pub const T_CHAT_MODE_EMOTE_OFF: u32 = 452;
pub const T_CHAT_MODE_FOLLOWER_ON: u32 = 453;
pub const T_CHAT_MODE_FOLLOWER_OFF: u32 = 454;
pub const T_CHAT_MODE_FOLLOWER_CHANGED: u32 = 455;
pub const T_CHAT_MODE_SLOW_ON: u32 = 456;
pub const T_CHAT_MODE_SLOW_OFF: u32 = 457;
pub const T_CHAT_MODE_SLOW_CHANGED: u32 = 458;
pub const T_CHAT_MODE_SUBSCRIBER_ON: u32 = 459;
pub const T_CHAT_MODE_SUBSCRIBER_OFF: u32 = 460;
pub const T_CHAT_MODE_UNIQUE_ON: u32 = 461;
pub const T_CHAT_MODE_UNIQUE_OFF: u32 = 462;
pub const T_AUTOMOD_MESSAGE_HELD: u32 = 463;
pub const T_AUTOMOD_MESSAGE_UPDATED: u32 = 464;
pub const T_BLOCKED_TERMS_ADDED: u32 = 465;
pub const T_BLOCKED_TERMS_DELETED: u32 = 466;
pub const T_WARNED_USER: u32 = 467;
pub const T_SUSPICIOUS_USER_UPDATE: u32 = 468;
pub const T_PERMITTED_TERMS_ADDED: u32 = 469;
pub const T_PERMITTED_TERMS_DELETED: u32 = 470;
pub const T_WARNING_ACKNOWLEDGED: u32 = 471;
pub const T_WATCH_STREAK: u32 = 472;
pub const T_SHARED_CHAT_SESSION_BEGIN: u32 = 474;
pub const T_SHARED_CHAT_SESSION_UPDATE: u32 = 475;
pub const T_SHARED_CHAT_SESSION_END: u32 = 476;
pub const T_PRIME_PAID_UPGRADE: u32 = 477;
pub const T_PAY_IT_FORWARD: u32 = 478;
pub const T_GIFT_PAID_UPGRADE: u32 = 479;
pub const T_BITS_BADGE_TIER: u32 = 480;
pub const T_SHARED_CHAT_ANNOUNCEMENT: u32 = 481;
pub const T_SHARED_CHAT_RAID: u32 = 482;
pub const T_SHARED_CHAT_PRIME_PAID_UPGRADE: u32 = 483;
pub const T_SHARED_CHAT_GIFT_PAID_UPGRADE: u32 = 484;
pub const T_SHARED_CHAT_PAY_IT_FORWARD: u32 = 485;
pub const T_SHARED_CHAT_SUB: u32 = 486;
pub const T_SHARED_CHAT_RESUB: u32 = 487;
pub const T_SHARED_CHAT_SUB_GIFT: u32 = 488;
pub const T_SHARED_CHAT_COMMUNITY_SUB_GIFT: u32 = 489;
pub const T_SHARED_CHAT_USER_BANNED: u32 = 490;
pub const T_SHARED_CHAT_USER_UNBANNED: u32 = 491;
pub const T_SHARED_CHAT_USER_TIMED_OUT: u32 = 492;
pub const T_SHARED_CHAT_USER_UNTIMED_OUT: u32 = 493;
pub const T_SHARED_CHAT_MESSAGE_DELETED: u32 = 494;
pub const T_SUSPICIOUS_USER_MESSAGE: u32 = 495;
pub const T_MODIVERSARY: u32 = 497;
pub const T_SHARED_MODIVERSARY: u32 = 498;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TwitchConfig {
    pub auto_connect: bool,
    pub broadcaster: Option<TwitchAccount>,
    pub bot: Option<TwitchAccount>,
    pub first_words_auto_reset: bool,
    /// The sub counter starts again at this (and Sub Counter Rollover fires); 0 never.
    pub sub_counter_rollover: u64,
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
    Helix {
        method: &'static str,
        path: String,
        query: Vec<(String, String)>,
        body: Option<Value>,
        use_bot: bool,
        fallback: bool,
        resp: oneshot::Sender<Result<Value, String>>,
    },
    ResolveUserId {
        login: String,
        resp: oneshot::Sender<Result<String, String>>,
    },
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

    pub fn connect(&self) {
        self.send(TwitchCmd::Connect);
    }

    pub fn disconnect(&self) {
        self.send(TwitchCmd::Disconnect);
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

    pub async fn helix(
        &self,
        method: &'static str,
        path: &str,
        query: Vec<(&str, &str)>,
        body: Option<Value>,
        use_bot: bool,
        fallback: bool,
    ) -> Result<Value, String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send(TwitchCmd::Helix {
            method,
            path: path.to_string(),
            query: query.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body,
            use_bot,
            fallback,
            resp: resp_tx,
        });
        resp_rx.await.map_err(|_| String::from("Twitch task stopped"))?
    }

    pub async fn resolve_user_id(&self, login: &str) -> Result<String, String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send(TwitchCmd::ResolveUserId {
            login: login.to_string(),
            resp: resp_tx,
        });
        resp_rx.await.map_err(|_| String::from("Twitch task stopped"))?
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

/// Twitch's official 15-color default username palette (used when a chatter hasn't set a custom color).
pub const DEFAULT_COLORS: [&str; 15] = [
    "#FF0000", "#0000FF", "#008000", "#B22222", "#FF7F50", "#9ACD32", "#FF4500", "#2E8B57", "#DAA520", "#D2691E",
    "#5F9EA0", "#1E90FF", "#FF69B4", "#8A2BE2", "#00FF7F",
];

/// Deterministic default Twitch chat color for `login_or_name`.
pub fn default_user_color(login_or_name: &str) -> &'static str {
    let s = login_or_name.trim();
    if s.is_empty() {
        return "#ADADB8";
    }
    let sum = s
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_add(b.to_ascii_lowercase() as usize));
    DEFAULT_COLORS[sum % DEFAULT_COLORS.len()]
}

/// Streamer.bot's `badges` (Name, Version, ImageUrl) and `badgeCount`, from a message's badges.
pub fn set_badges(args: &mut Obj, badges: &[Value]) {
    let list: Vec<Value> = badges
        .iter()
        .map(|b| {
            let (set, v) = (b["set_id"].as_str().unwrap_or(""), b["id"].as_str().unwrap_or(""));
            let key = format!("{set}/{v}");
            let url = SEEN.with_borrow(|s| s.badges.get(&key).cloned().unwrap_or_default());
            json!({"name": set, "version": v.parse::<i64>().unwrap_or(0), "key": key, "imageUrl": url})
        })
        .collect();
    args.insert("badgeCount".into(), list.len().into());
    args.insert("badges".into(), list.into());
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
    let fallback_color = args
        .get("userName")
        .or_else(|| args.get("user"))
        .and_then(Value::as_str)
        .map_or("#ADADB8", default_user_color);
    let color_str = if ctx.color.is_empty() {
        fallback_color
    } else {
        ctx.color
    };
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
    let runner = Runner {
        engine,
        cfg: cfg.clone(),
        rx,
        session_id: None,
        is_connected,
        wanted: Arc::new(AtomicBool::new(false)),
        read_task: None,
        bot_read_task: None,
    };
    tokio::task::spawn_local(runner.run());
    handle
}

struct Runner {
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    rx: mpsc::UnboundedReceiver<TwitchCmd>,
    session_id: Option<String>,
    is_connected: Arc<AtomicBool>,
    wanted: Arc<AtomicBool>,
    /// The EventSub supervisor + read loop; aborted on Disconnect, which closes the socket.
    read_task: Option<tokio::task::JoinHandle<()>>,
    /// Extra bot task slot (also managed inside supervisor).
    bot_read_task: Option<tokio::task::JoinHandle<()>>,
}

/// A token is refreshed 5 minutes before it expires (or when its expiry is unknown).
pub fn needs_refresh(t: &TwitchToken, now: u64) -> bool {
    !t.refresh_token.is_empty() && (t.expires_at == 0 || now + 300 >= t.expires_at)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Refresh broadcaster and/or bot tokens when about to expire (or unconditionally if `force`).
async fn refresh_accounts(engine: &Rc<Engine>, cfg: &Arc<watch::Sender<Config>>, force: bool) {
    let now = now_secs();
    for bot in [false, true] {
        let rt = {
            let c = cfg.borrow();
            let a = if bot { &c.twitch.bot } else { &c.twitch.broadcaster };
            match a {
                Some(a) if !a.token.refresh_token.is_empty() && (force || needs_refresh(&a.token, now)) => {
                    a.token.refresh_token.clone()
                }
                _ => continue,
            }
        };
        let who = if bot { "bot" } else { "broadcaster" };
        match refresh_token(CLIENT_ID, &rt).await {
            Ok(t) => {
                let token = TwitchToken {
                    access_token: t.access_token,
                    refresh_token: t.refresh_token,
                    scope: t.scope,
                    expires_at: now + t.expires_in,
                };
                engine.change(move |c| {
                    let a = if bot {
                        &mut c.twitch.bot
                    } else {
                        &mut c.twitch.broadcaster
                    };
                    if let Some(a) = a {
                        a.token = token;
                    }
                });
                crate::say(format!("Twitch: refreshed {who} token"));
            }
            // A refresh token is good once: refused, it never works again. Forget it so nothing
            // retries it and Accounts shows that re-login is needed.
            Err(e) if e.contains("Invalid refresh token") => {
                engine.change(move |c| {
                    let a = if bot {
                        &mut c.twitch.bot
                    } else {
                        &mut c.twitch.broadcaster
                    };
                    if let Some(a) = a {
                        a.token = TwitchToken::default();
                    }
                });
                crate::error(format!("Twitch: {who} login expired; log in again in Accounts"));
            }
            Err(e) => crate::warn(format!("Twitch: token refresh failed for {who} ({e}); will retry")),
        }
    }
}

impl Runner {
    async fn run(mut self) {
        if self.cfg.borrow().twitch.auto_connect && self.cfg.borrow().twitch.broadcaster.is_some() {
            self.connect_eventsub().await;
        }

        while let Some(cmd) = self.rx.recv().await {
            self.handle_cmd(cmd).await;
        }
    }

    async fn refresh_if_needed(&self) {
        refresh_accounts(&self.engine, &self.cfg, false).await;
    }

    async fn handle_cmd(&mut self, cmd: TwitchCmd) {
        if !matches!(
            cmd,
            TwitchCmd::Connect | TwitchCmd::Disconnect | TwitchCmd::GetStatus(_)
        ) {
            self.refresh_if_needed().await;
        }
        match cmd {
            TwitchCmd::Connect => {
                self.connect_eventsub().await;
            }
            TwitchCmd::Disconnect => {
                self.disconnect();
            }
            TwitchCmd::GetStatus(resp) => {
                let c = self.cfg.borrow();
                let status = TwitchStatus {
                    connected: self.is_connected.load(Ordering::Relaxed),
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
            TwitchCmd::Helix {
                method,
                path,
                query,
                body,
                use_bot,
                fallback,
                resp,
            } => {
                let res = self
                    .run_helix(method, &path, &query, body.as_ref(), use_bot, fallback)
                    .await;
                let _ = resp.send(res);
            }
            TwitchCmd::ResolveUserId { login, resp } => {
                let res = self.resolve_user_id(&login).await;
                let _ = resp.send(res);
            }
        }
    }

    fn disconnect(&mut self) {
        self.wanted.store(false, Ordering::Relaxed);
        if let Some(t) = self.read_task.take() {
            t.abort();
        }
        if let Some(t) = self.bot_read_task.take() {
            t.abort();
        }
        self.session_id = None;
        if self.is_connected.swap(false, Ordering::Relaxed) {
            crate::ui::changed();
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
                self.engine.emit("Twitch", "BotEventSubDisconnected", || json!({}));
                self.engine.emit("Twitch", "BotChatDisconnected", || json!({}));
            }
            crate::say("Twitch: disconnected".into());
            crate::ui::changed();
        }
    }

    async fn connect_eventsub(&mut self) {
        if self.is_connected.load(Ordering::Relaxed) && self.read_task.as_ref().is_some_and(|t| !t.is_finished()) {
            return;
        }
        if self.cfg.borrow().twitch.broadcaster.is_none() {
            crate::error("Twitch: no broadcaster account configured".into());
            return;
        }
        if let Some(t) = self.read_task.take() {
            t.abort();
        }
        if let Some(t) = self.bot_read_task.take() {
            t.abort();
        }
        self.wanted.store(true, Ordering::Relaxed);
        let engine = self.engine.clone();
        let cfg = self.cfg.clone();
        let is_conn = self.is_connected.clone();
        let wanted = self.wanted.clone();
        self.read_task = Some(tokio::task::spawn_local(async move {
            supervise_eventsub(engine, cfg, is_conn, wanted).await;
        }));
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
        let broadcaster = self.cfg.borrow().twitch.broadcaster.clone();
        let broadcaster = broadcaster.as_ref().ok_or("No broadcaster account")?;
        let token = &broadcaster.token.access_token;
        let from_id = &broadcaster.user_id;

        // Resolve target user (and their picture, for the raid triggers)
        let url = format!("{HELIX_URL}/users?login={}", commands::url_encode(target_login));
        let headers = vec![
            ("Client-Id".into(), CLIENT_ID.into()),
            ("Authorization".into(), format!("Bearer {token}")),
        ];
        let r = crate::http::request("GET", &url, &headers, b"").await?;
        let user = serde_json::from_slice::<Value>(&r.body).unwrap_or_default()["data"][0].clone();
        let to_id = user["id"]
            .as_str()
            .ok_or_else(|| format!("no Twitch user {target_login}"))?;

        let url = format!("{HELIX_URL}/raids?from_broadcaster_id={from_id}&to_broadcaster_id={to_id}");
        let res = crate::http::request("POST", &url, &headers, b"").await?;
        if res.status == 200 || res.status == 204 {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let raided = raid_target(&user);
            args.extend(raided.clone());
            // Raid Send and Raid Cancelled say who too.
            SEEN.with_borrow_mut(|s| s.raid = Some(raided));
            go(
                &self.engine,
                T_RAID_START,
                "Raid Start",
                "Twitch/Raid",
                "RaidStart",
                &args,
            );
            Ok(true)
        } else {
            crate::error(format!("Start raid failed: HTTP {} {}", res.status, res.text()));
            Ok(false)
        }
    }

    async fn cancel_raid(&self) -> Result<(), String> {
        let broadcaster = self.cfg.borrow().twitch.broadcaster.clone();
        let broadcaster = broadcaster.as_ref().ok_or("No broadcaster account")?;
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
            args.extend(SEEN.with_borrow_mut(|s| s.raid.take()).unwrap_or_default());
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

    async fn resolve_user_id(&self, login: &str) -> Result<String, String> {
        if login.is_empty() {
            return Err("Empty login".into());
        }
        let broadcaster = self.cfg.borrow().twitch.broadcaster.clone();
        let broadcaster = broadcaster.as_ref().ok_or("No broadcaster account")?;
        let token = &broadcaster.token.access_token;
        match fetch_user(CLIENT_ID, token, Some(login)).await {
            Ok(u) => Ok(u.user_id),
            Err(e) => {
                if login.chars().all(|c| c.is_ascii_digit()) {
                    Ok(login.to_string())
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn run_helix(
        &self,
        method: &'static str,
        path: &str,
        query: &[(String, String)],
        body: Option<&Value>,
        use_bot: bool,
        fallback: bool,
    ) -> Result<Value, String> {
        let (token, broadcaster_id, sender_id) = self.pick_account(use_bot, fallback)?;
        let mut qs = Vec::new();
        for (k, v) in query {
            let val = match v.as_str() {
                "{broadcaster_id}" => broadcaster_id.as_str(),
                "{sender_id}" => sender_id.as_str(),
                _ => v.as_str(),
            };
            qs.push(format!("{}={}", commands::url_encode(k), commands::url_encode(val)));
        }
        let url = if qs.is_empty() {
            format!("{HELIX_URL}/{path}")
        } else {
            format!("{HELIX_URL}/{path}?{}", qs.join("&"))
        };
        let mut headers = vec![
            ("Client-Id".into(), CLIENT_ID.into()),
            ("Authorization".into(), format!("Bearer {token}")),
        ];
        let bytes = if let Some(b) = body {
            headers.push(("Content-Type".into(), "application/json".into()));
            // The same placeholders as the query, as whole JSON strings in the body.
            let ids = |x: &str| serde_json::to_string(x).unwrap_or_default();
            serde_json::to_string(b)
                .unwrap_or_default()
                .replace("\"{broadcaster_id}\"", &ids(&broadcaster_id))
                .replace("\"{sender_id}\"", &ids(&sender_id))
                .into_bytes()
        } else {
            Vec::new()
        };
        let res = crate::http::request(method, &url, &headers, &bytes).await?;
        if res.status >= 200 && res.status < 300 {
            if res.body.is_empty() {
                Ok(Value::Null)
            } else {
                serde_json::from_slice(&res.body).map_err(|e| format!("Invalid JSON from Helix: {e}"))
            }
        } else {
            Err(format!(
                "Helix {method} {path} failed: HTTP {} {}",
                res.status,
                res.text()
            ))
        }
    }

    fn pick_account(&self, use_bot: bool, fallback: bool) -> Result<(String, String, String), String> {
        let c = self.cfg.borrow();
        let broadcaster = c.twitch.broadcaster.as_ref().ok_or("No broadcaster account")?;
        if broadcaster.token.access_token.is_empty() {
            if crate::secrets::is_locked() {
                return Err("System keyring is locked; unlock it to use saved Twitch login".into());
            }
            return Err("Twitch broadcaster login expired; log in again in Accounts".into());
        }
        if use_bot {
            if let Some(bot) = &c.twitch.bot {
                if !bot.token.access_token.is_empty() {
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
                    return Err("Twitch bot login expired and fallback disabled".into());
                }
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

async fn supervise_eventsub(
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    is_connected: Arc<AtomicBool>,
    wanted: Arc<AtomicBool>,
) {
    let mut backoff_secs = 2u64;
    let mut logged_locked = false;
    let mut first_connect_log = true;

    while wanted.load(Ordering::Relaxed) {
        refresh_accounts(&engine, &cfg, false).await;

        let (broadcaster, bot, url, custom_url) = {
            let c = cfg.borrow();
            let Some(b) = c.twitch.broadcaster.clone() else {
                wanted.store(false, Ordering::Relaxed);
                break;
            };
            if b.token.access_token.is_empty() {
                if crate::secrets::is_locked() {
                    if !logged_locked {
                        crate::warn("Twitch: waiting for system keyring to unlock saved login...".into());
                        logged_locked = true;
                        crate::ui::changed();
                    }
                    drop(c);
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
                crate::error(format!(
                    "Twitch: @{} login missing or expired; log in again in Accounts",
                    b.login
                ));
                wanted.store(false, Ordering::Relaxed);
                crate::ui::changed();
                break;
            }
            logged_locked = false;
            let bot = c.twitch.bot.clone().filter(|bt| !bt.token.access_token.is_empty());
            let custom = c
                .twitch
                .eventsub_url
                .as_deref()
                .is_some_and(|u| !u.is_empty() && u != DEFAULT_EVENTSUB_URL);
            let u = c
                .twitch
                .eventsub_url
                .clone()
                .unwrap_or_else(|| DEFAULT_EVENTSUB_URL.into());
            (b, bot, u, custom)
        };

        SUBS_URL.with_borrow_mut(|s| *s = subs_url(&url));
        if first_connect_log {
            crate::say(format!("Twitch: connecting to EventSub (@{})...", broadcaster.login));
            first_connect_log = false;
        }
        let ws = match crate::http::ws_connect(&url).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                crate::warn(format!(
                    "Twitch: EventSub connect failed for @{} ({e}); retrying in {backoff_secs}s",
                    broadcaster.login
                ));
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(30);
                continue;
            }
        };

        is_connected.store(true, Ordering::Relaxed);
        crate::ui::changed();

        let bot_task = bot.map(|bot_acct| {
            let bot_engine = engine.clone();
            let bot_cfg = cfg.clone();
            let bot_url = url.clone();
            let b_acct = broadcaster.clone();
            tokio::task::spawn_local(async move {
                match crate::http::ws_connect(&bot_url).await {
                    Ok((bot_ws, _)) => {
                        run_bot_eventsub_loop(bot_ws, bot_engine, bot_cfg, b_acct, bot_acct).await;
                    }
                    Err(e) => {
                        crate::warn(format!("Twitch: bot EventSub connect failed ({e})"));
                    }
                }
            })
        });

        let started = std::time::Instant::now();
        let auth_refused = Arc::new(AtomicBool::new(false));
        run_eventsub_loop(
            ws,
            engine.clone(),
            cfg.clone(),
            broadcaster.clone(),
            is_connected.clone(),
            custom_url,
            auth_refused.clone(),
        )
        .await;

        if let Some(t) = bot_task {
            t.abort();
        }

        if !wanted.load(Ordering::Relaxed) {
            break;
        }
        if started.elapsed() >= Duration::from_secs(15) {
            backoff_secs = 2;
        }
        if auth_refused.load(Ordering::Relaxed) {
            refresh_accounts(&engine, &cfg, true).await;
            let still_has_token = cfg
                .borrow()
                .twitch
                .broadcaster
                .as_ref()
                .is_some_and(|b| !b.token.access_token.is_empty());
            if !still_has_token {
                wanted.store(false, Ordering::Relaxed);
                crate::ui::changed();
                break;
            }
        }
        crate::say(format!(
            "Twitch: reconnecting @{} in {backoff_secs}s...",
            broadcaster.login
        ));
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(30);
    }
}

async fn run_eventsub_loop(
    mut ws: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    broadcaster: TwitchAccount,
    is_connected: Arc<AtomicBool>,
    custom_eventsub: bool,
    auth_refused: Arc<AtomicBool>,
) {
    use futures_util::StreamExt;
    let mut reconnecting = false;
    let mut poll_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut emote_task: Option<tokio::task::JoinHandle<()>> = None;

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
                if std::mem::take(&mut reconnecting) {
                    crate::say(format!("Twitch: EventSub reconnected (@{}, {sid})", broadcaster.login));
                    continue;
                }
                crate::say(format!("Twitch: connected (@{}, session {sid})", broadcaster.login));
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

                // Create subscriptions via Helix, seed channel metadata + badges, and load pre-join history.
                let token = broadcaster.token.access_token.clone();
                let bid = broadcaster.user_id.clone();
                let login = broadcaster.login.clone();
                let sid = sid.to_string();
                let e = engine.clone();
                let refused = auth_refused.clone();
                // The welcome waits only for the history: the ~40 subscriptions take seconds.
                tokio::task::spawn_local(async move {
                    let hist_fut = async {
                        if custom_eventsub || login.is_empty() {
                            Vec::new()
                        } else {
                            fetch_recent_messages(&login, &bid).await
                        }
                    };
                    let subs = create_all_subscriptions(&sid, &token, &bid, &refused);
                    let welcome = async {
                        let mut hist = hist_fut.await;
                        SEEN.with_borrow(|s| {
                            for l in &mut hist {
                                for (k, u) in &mut l.badges {
                                    if u.is_empty() {
                                        if let Some(found) = s.badges.get(k) {
                                            *u = found.clone();
                                        }
                                    }
                                }
                            }
                        });
                        e.chat_welcome("twitch", hist);
                    };
                    tokio::join!(subs, seed(&e), welcome);
                    e.refresh_chat_badges("twitch", |k| SEEN.with_borrow(|s| s.badges.get(k).cloned()));
                    e.twitch_check_pin();
                });
                if let Some(t) = poll_task.replace(tokio::task::spawn_local(poll(engine.clone(), broadcaster.clone())))
                {
                    t.abort();
                }
                let codes = crate::emote_events::Codes {
                    platform: "twitch",
                    source: "Twitch",
                    seventv: (T_SEVEN_TV_EMOTE_ADDED, T_SEVEN_TV_EMOTE_REMOVED),
                    bttv: (T_BETTER_TTV_EMOTE_ADDED, T_BETTER_TTV_EMOTE_REMOVED),
                };
                if codes.wanted(&engine) && emote_task.is_none() {
                    let watch = crate::emote_events::watch(engine.clone(), codes, broadcaster.user_id.clone());
                    emote_task = Some(tokio::task::spawn_local(watch));
                }
            }
            "notification" => {
                let sub_type = val
                    .pointer("/metadata/subscription_type")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let event = val.pointer("/payload/event").cloned().unwrap_or_default();
                handle_notification(&engine, &broadcaster, &cfg, sub_type, &event);
            }
            "session_reconnect" => {
                let url = val
                    .pointer("/payload/session/reconnect_url")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                crate::debug(format!("Twitch: EventSub session handoff (@{})", broadcaster.login));
                match crate::http::ws_connect(url).await {
                    Ok((new_ws, _)) => {
                        ws = new_ws;
                        reconnecting = true;
                    }
                    Err(e) => {
                        crate::warn(format!("Twitch: EventSub handoff failed ({e})"));
                        break;
                    }
                }
            }
            "revocation" => {
                let sub_type = val
                    .pointer("/payload/subscription/type")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                crate::warn(format!("Twitch: subscription revoked ({sub_type})"));
            }
            _ => {}
        }
    }

    for t in [poll_task, emote_task].into_iter().flatten() {
        t.abort();
    }
    if !is_connected.swap(false, Ordering::Relaxed) {
        return;
    }
    crate::warn(format!("Twitch: EventSub connection lost (@{})", broadcaster.login));
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
    if cfg.borrow().twitch.bot.is_some() {
        engine.fire(
            T_BOT_EVENTSUB_DISCONNECTED,
            "Bot EventSub Disconnected",
            "Twitch/Connections",
            "twitch",
            &args,
            |_| true,
        );
        engine.emit("Twitch", "BotEventSubDisconnected", || json!(args));
        engine.emit("Twitch", "BotChatDisconnected", || json!(args));
    }
    crate::ui::changed();
}

async fn run_bot_eventsub_loop(
    mut ws: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    engine: Rc<Engine>,
    cfg: Arc<watch::Sender<Config>>,
    broadcaster: TwitchAccount,
    bot: TwitchAccount,
) {
    use futures_util::StreamExt;
    let mut reconnecting = false;

    while let Some(msg) = ws.next().await {
        let raw = match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(t)) => t,
            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) | Err(_) => break,
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
                if std::mem::take(&mut reconnecting) {
                    crate::say(format!("Twitch: bot EventSub reconnected (@{})", bot.login));
                    continue;
                }
                crate::say(format!("Twitch: bot connected (@{}, session {sid})", bot.login));
                let mut args = Obj::new();
                populate_broadcaster_vars(&mut args, &broadcaster);
                populate_user_vars(&mut args, &bot.user_id, &bot.login, &bot.display_name, 1, false);
                engine.fire(
                    T_BOT_EVENTSUB_CONNECTED,
                    "Bot EventSub Connected",
                    "Twitch/Connections",
                    "twitch",
                    &args,
                    |_| true,
                );
                engine.emit("Twitch", "BotEventSubConnected", || json!(args));
                engine.emit("Twitch", "BotChatConnected", || json!(args));

                let token = bot.token.access_token.clone();
                let bid = bot.user_id.clone();
                let sid = sid.to_string();
                let dummy = AtomicBool::new(false);
                tokio::task::spawn_local(async move {
                    let _ = create_sub(
                        &sid,
                        &token,
                        "user.whisper.message",
                        "1",
                        json!({"user_id": bid}),
                        &dummy,
                    )
                    .await;
                });
            }
            "notification" => {
                let sub_type = val
                    .pointer("/metadata/subscription_type")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let event = val.pointer("/payload/event").cloned().unwrap_or_default();
                handle_notification(&engine, &broadcaster, &cfg, sub_type, &event);
            }
            "session_reconnect" => {
                let url = val
                    .pointer("/payload/session/reconnect_url")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match crate::http::ws_connect(url).await {
                    Ok((new_ws, _)) => {
                        ws = new_ws;
                        reconnecting = true;
                    }
                    Err(e) => {
                        crate::warn(format!("Twitch: bot EventSub handoff failed ({e})"));
                        break;
                    }
                }
            }
            "revocation" => {
                let sub_type = val
                    .pointer("/payload/subscription/type")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                crate::warn(format!("Twitch: bot subscription revoked ({sub_type})"));
            }
            _ => {}
        }
    }

    let mut args = Obj::new();
    populate_broadcaster_vars(&mut args, &broadcaster);
    populate_user_vars(&mut args, &bot.user_id, &bot.login, &bot.display_name, 1, false);
    engine.fire(
        T_BOT_EVENTSUB_DISCONNECTED,
        "Bot EventSub Disconnected",
        "Twitch/Connections",
        "twitch",
        &args,
        |_| true,
    );
    engine.emit("Twitch", "BotEventSubDisconnected", || json!(args));
    engine.emit("Twitch", "BotChatDisconnected", || json!(args));
}

async fn create_all_subscriptions(sid: &str, token: &str, bid: &str, auth_refused: &AtomicBool) {
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
        ("channel.raid", "1", json!({"from_broadcaster_user_id": bid})),
        // Subs, resubs, gifts and incoming raids come as chat notifications; cheers with the message.
        (
            "channel.chat.message_delete",
            "1",
            json!({"broadcaster_user_id": bid, "user_id": bid}),
        ),
        (
            "channel.moderate",
            "2",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "automod.message.hold",
            "2",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "automod.message.update",
            "2",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "automod.terms.update",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.unban_request.create",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.unban_request.resolve",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.warning.send",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.warning.acknowledge",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.suspicious_user.update",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.suspicious_user.message",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.channel_points_automatic_reward_redemption.add",
            "2",
            json!({"broadcaster_user_id": bid}),
        ),
        ("channel.bits.use", "1", json!({"broadcaster_user_id": bid})),
        ("stream.online", "1", json!({"broadcaster_user_id": bid})),
        ("stream.offline", "1", json!({"broadcaster_user_id": bid})),
        ("user.whisper.message", "1", json!({"user_id": bid})),
        (
            "channel.channel_points_custom_reward.add",
            "1",
            json!({"broadcaster_user_id": bid}),
        ),
        (
            "channel.channel_points_custom_reward.update",
            "1",
            json!({"broadcaster_user_id": bid}),
        ),
        (
            "channel.channel_points_custom_reward.remove",
            "1",
            json!({"broadcaster_user_id": bid}),
        ),
        (
            "channel.channel_points_custom_reward_redemption.add",
            "1",
            json!({"broadcaster_user_id": bid}),
        ),
        (
            "channel.channel_points_custom_reward_redemption.update",
            "1",
            json!({"broadcaster_user_id": bid}),
        ),
        ("channel.poll.begin", "1", json!({"broadcaster_user_id": bid})),
        ("channel.poll.progress", "1", json!({"broadcaster_user_id": bid})),
        ("channel.poll.end", "1", json!({"broadcaster_user_id": bid})),
        ("channel.prediction.begin", "1", json!({"broadcaster_user_id": bid})),
        ("channel.prediction.progress", "1", json!({"broadcaster_user_id": bid})),
        ("channel.prediction.lock", "1", json!({"broadcaster_user_id": bid})),
        ("channel.prediction.end", "1", json!({"broadcaster_user_id": bid})),
        ("channel.hype_train.begin", "2", json!({"broadcaster_user_id": bid})),
        ("channel.hype_train.progress", "2", json!({"broadcaster_user_id": bid})),
        ("channel.hype_train.end", "2", json!({"broadcaster_user_id": bid})),
        ("channel.goal.begin", "1", json!({"broadcaster_user_id": bid})),
        ("channel.goal.progress", "1", json!({"broadcaster_user_id": bid})),
        ("channel.goal.end", "1", json!({"broadcaster_user_id": bid})),
        ("channel.update", "2", json!({"broadcaster_user_id": bid})),
        ("channel.ad_break.begin", "1", json!({"broadcaster_user_id": bid})),
        ("channel.ban", "1", json!({"broadcaster_user_id": bid})),
        ("channel.unban", "1", json!({"broadcaster_user_id": bid})),
        ("channel.moderator.add", "1", json!({"broadcaster_user_id": bid})),
        ("channel.moderator.remove", "1", json!({"broadcaster_user_id": bid})),
        ("channel.vip.add", "1", json!({"broadcaster_user_id": bid})),
        ("channel.vip.remove", "1", json!({"broadcaster_user_id": bid})),
        (
            "channel.shield_mode.begin",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.shield_mode.end",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.shoutout.create",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.shoutout.receive",
            "1",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.chat.clear",
            "1",
            json!({"broadcaster_user_id": bid, "user_id": bid}),
        ),
        (
            "channel.chat.clear_user_messages",
            "1",
            json!({"broadcaster_user_id": bid, "user_id": bid}),
        ),
        (
            "channel.chat_settings.update",
            "1",
            json!({"broadcaster_user_id": bid, "user_id": bid}),
        ),
        (
            "channel.charity_campaign.donate",
            "1",
            json!({"broadcaster_user_id": bid}),
        ),
        (
            "channel.charity_campaign.start",
            "1",
            json!({"broadcaster_user_id": bid}),
        ),
        (
            "channel.charity_campaign.progress",
            "1",
            json!({"broadcaster_user_id": bid}),
        ),
        (
            "channel.charity_campaign.stop",
            "1",
            json!({"broadcaster_user_id": bid}),
        ),
        (
            "channel.guest_star_session.begin",
            "beta",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.guest_star_session.end",
            "beta",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.guest_star_guest.update",
            "beta",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        (
            "channel.guest_star_settings.update",
            "beta",
            json!({"broadcaster_user_id": bid, "moderator_user_id": bid}),
        ),
        ("channel.shared_chat.begin", "1", json!({"broadcaster_user_id": bid})),
        ("channel.shared_chat.update", "1", json!({"broadcaster_user_id": bid})),
        ("channel.shared_chat.end", "1", json!({"broadcaster_user_id": bid})),
    ];

    for (stype, ver, cond) in subs {
        if !create_sub(sid, token, stype, ver, cond, auth_refused).await {
            break;
        }
    }
}

thread_local! {
    /// Where subscriptions are made: Helix, or the mock server when `eventsub_url` points to one
    /// (the Twitch CLI's `twitch event websocket start-server`: ws://127.0.0.1:PORT/ws).
    static SUBS_URL: RefCell<String> = RefCell::new(format!("{HELIX_URL}/eventsub/subscriptions"));
}

/// The subscriptions endpoint that goes with an EventSub WebSocket URL.
fn subs_url(ws: &str) -> String {
    match ws.strip_prefix("ws://").or_else(|| ws.strip_prefix("wss://")) {
        Some(rest) if ws != DEFAULT_EVENTSUB_URL => {
            let host = rest.split('/').next().unwrap_or(rest);
            let scheme = if ws.starts_with("wss") { "https" } else { "http" };
            format!("{scheme}://{host}/eventsub/subscriptions")
        }
        _ => format!("{HELIX_URL}/eventsub/subscriptions"),
    }
}

/// False when the token was refused: every other subscription would be too.
async fn create_sub(
    sid: &str,
    token: &str,
    sub_type: &str,
    version: &str,
    condition: Value,
    auth_refused: &AtomicBool,
) -> bool {
    let url = SUBS_URL.with_borrow(Clone::clone);
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
            crate::error(format!("Twitch: create sub {sub_type}: {e}"));
            return true;
        }
    };
    if res.status == 401 {
        auth_refused.store(true, Ordering::Relaxed);
        crate::warn("Twitch: token rejected (HTTP 401); refreshing...".into());
        return false;
    }
    if res.status != 202 && res.status != 200 {
        crate::error(format!(
            "Twitch: create sub {sub_type} HTTP {}: {}",
            res.status,
            res.text()
        ));
    }
    true
}

fn unescape_irc_tag(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(':') => out.push(';'),
                Some('s') => out.push(' '),
                Some('\\') => out.push('\\'),
                Some('r') => out.push('\r'),
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse an IRCv3 `PRIVMSG` line (e.g. from `recent-messages.robotty.de`) into a historical chat `Line`.
pub fn parse_irc_recent_message(raw: &str, broadcaster_id: &str) -> Option<crate::engine::chatlog::Line> {
    let raw = raw.trim_end_matches(['\r', '\n']);
    let (tags_part, rest) = if let Some(r) = raw.strip_prefix('@') {
        r.split_once(' ')?
    } else {
        ("", raw)
    };
    let rest = rest.trim_start();
    let (prefix, cmd_rest) = if let Some(r) = rest.strip_prefix(':') {
        r.split_once(' ')?
    } else {
        ("", rest)
    };
    let login = prefix.split('!').next().unwrap_or("");
    let cmd_rest = cmd_rest.trim_start();
    let after_cmd = cmd_rest.strip_prefix("PRIVMSG ")?;
    // The text follows ` :`, except a one-word message, which IRC sends without the colon.
    let (_, trailing) = after_cmd.split_once(" :").or_else(|| after_cmd.split_once(' '))?;
    let mut text = trailing.to_string();
    if let Some(act) = text.strip_prefix("\x01ACTION ").and_then(|s| s.strip_suffix('\x01')) {
        text = act.to_string();
    }

    let mut msg_id = String::new();
    let mut user_id = String::new();
    let mut display_name = String::new();
    let mut color = String::new();
    let mut badges_raw = String::new();
    let mut emotes_raw = String::new();
    let mut sent_ts: Option<i64> = None;
    let mut reply = crate::engine::chatlog::Reply::default();

    for kv in tags_part.split(';') {
        let Some((k, v)) = kv.split_once('=') else { continue };
        match k {
            "id" => msg_id = v.to_string(),
            "user-id" => user_id = v.to_string(),
            "display-name" => display_name = unescape_irc_tag(v),
            "color" => color = v.to_string(),
            "badges" => badges_raw = v.to_string(),
            "emotes" => emotes_raw = v.to_string(),
            "tmi-sent-ts" => sent_ts = v.parse::<i64>().ok(),
            // Deleted by a moderator since: not shown again.
            "rm-deleted" if v == "1" => return None,
            "reply-parent-display-name" => reply.user = unescape_irc_tag(v),
            "reply-parent-msg-body" => reply.text = unescape_irc_tag(v),
            _ => {}
        }
    }

    let user = if display_name.is_empty() {
        login.to_string()
    } else {
        display_name
    };
    if user.is_empty() && text.is_empty() {
        return None;
    }
    if color.is_empty() {
        color = default_user_color(if !login.is_empty() { login } else { &user }).to_string();
    }
    let time = sent_ts.map_or_else(String::new, |ms| crate::engine::chatlog::local_hhmm(ms / 1000));

    let mut broadcaster = !broadcaster_id.is_empty() && user_id == broadcaster_id;
    let mut moderator = false;
    let mut vip = false;
    let mut subscriber = false;
    let mut badges = Vec::new();
    for b in badges_raw.split(',').filter(|s| !s.is_empty()) {
        let Some((set, ver)) = b.split_once('/') else { continue };
        match set {
            "broadcaster" => broadcaster = true,
            "moderator" => moderator = true,
            "vip" => vip = true,
            "subscriber" | "founder" => subscriber = true,
            _ => {}
        }
        let key = format!("{set}/{ver}");
        let url = SEEN.with_borrow(|s| s.badges.get(&key).cloned().unwrap_or_default());
        badges.push((key, url));
    }

    let chars: Vec<char> = text.chars().collect();
    let mut emotes = Vec::new();
    for spec in emotes_raw.split('/').filter(|s| !s.is_empty()) {
        let Some((id, ranges)) = spec.split_once(':') else {
            continue;
        };
        let Some(first_range) = ranges.split(',').next() else {
            continue;
        };
        let Some((a, b)) = first_range.split_once('-') else {
            continue;
        };
        let (Ok(start), Ok(end)) = (a.parse::<usize>(), b.parse::<usize>()) else {
            continue;
        };
        if start <= end && end < chars.len() {
            let name: String = chars[start..=end].iter().collect();
            let url = format!("https://static-cdn.jtvnw.net/emoticons/v2/{id}/default/dark/2.0");
            if !emotes.iter().any(|(n, _): &(String, String)| n == &name) {
                emotes.push((name, url));
            }
        }
    }

    let reply = (!reply.user.is_empty()).then_some(reply);
    let text = reply.as_ref().map_or(text.clone(), |r| r.strip(&text));
    Some(crate::engine::chatlog::Line {
        platform: "twitch".into(),
        reply,
        msg_id,
        time,
        ts: sent_ts.unwrap_or(0),
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

async fn fetch_recent_messages(login: &str, broadcaster_id: &str) -> Vec<crate::engine::chatlog::Line> {
    let ch = login.trim().to_ascii_lowercase();
    if ch.is_empty() {
        return Vec::new();
    }
    let url = format!(
        "https://recent-messages.robotty.de/api/v2/recent-messages/{}?limit=100&hide_moderation_messages=true&hide_moderated_messages=true",
        commands::url_encode(&ch)
    );
    let Ok(res) = crate::http::request("GET", &url, &[], b"").await else {
        return Vec::new();
    };
    if res.status != 200 {
        return Vec::new();
    }
    let Ok(val) = serde_json::from_slice::<Value>(&res.body) else {
        return Vec::new();
    };
    val["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| parse_irc_recent_message(m.as_str()?, broadcaster_id))
        .collect()
}

fn handle_notification(
    engine: &Rc<Engine>,
    broadcaster: &TwitchAccount,
    cfg: &Arc<watch::Sender<Config>>,
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
            set_badges(&mut args, badges);
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

            SEEN.with_borrow_mut(|s| {
                if s.messages.len() == 200 {
                    s.messages.pop_front();
                }
                s.messages.push_back((msg_id.to_string(), text.to_string()));
            });
            if let Some((ok, owner, word, width)) = pyramid((chatter_id, chatter_login, chatter_name), text) {
                let mut p = args.clone();
                p.insert("pyramidWidth".into(), width.into());
                p.insert("pyramidEmote".into(), word.into());
                if ok {
                    let n = SEEN.with_borrow_mut(|s| {
                        let c = s.pyramids.entry(owner.0.clone()).or_default();
                        c.0 += 1;
                        c.0
                    });
                    populate_user_vars(&mut p, &owner.0, &owner.1, &owner.2, 1, false);
                    p.insert("totalPyramids".into(), n.into());
                    go(
                        engine,
                        T_PYRAMID_SUCCESS,
                        "Pyramid Success",
                        "Twitch/Chat",
                        "PyramidSuccess",
                        &p,
                    );
                } else {
                    let n = SEEN.with_borrow_mut(|s| {
                        let c = s.pyramids.entry(chatter_id.to_string()).or_default();
                        c.1 += 1;
                        c.1
                    });
                    p.insert("totalPyramidsBroken".into(), n.into());
                    p.insert("pyramidOwnerId".into(), owner.0.into());
                    p.insert("pyramidOwnerUsername".into(), owner.1.into());
                    p.insert("pyramidOwnerDisplayName".into(), owner.2.into());
                    go(
                        engine,
                        T_PYRAMID_BROKEN,
                        "Pyramid Broken",
                        "Twitch/Chat",
                        "PyramidBroken",
                        &p,
                    );
                }
            }

            // First Words detection
            let norm_login = chatter_login.to_lowercase();
            if engine.first_words("twitch", &norm_login) {
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
                emotes: event
                    .pointer("/message/fragments")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|f| f["type"] == "emote")
                    .filter_map(|f| {
                        let url = format!(
                            "https://static-cdn.jtvnw.net/emoticons/v2/{}/default/dark/2.0",
                            f.pointer("/emote/id")?.as_str()?
                        );
                        Some((f["text"].as_str()?.to_string(), url))
                    })
                    .collect(),
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
            let ntype = es(event, "/notice_type");
            let chatter_id = es(event, "/chatter_user_id");
            let badges = event
                .pointer("/badges")
                .and_then(Value::as_array)
                .map_or(&[][..], |a| a.as_slice());
            let (role, sub) = parse_badges(badges, chatter_id == broadcaster.user_id);
            let sys_msg = es(event, "/system_message");
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            populate_user_vars(
                &mut args,
                chatter_id,
                es(event, "/chatter_user_login"),
                es(event, "/chatter_user_name"),
                role,
                sub,
            );
            populate_chat_vars(
                &mut args,
                &ChatContext {
                    text: es(event, "/message/text"),
                    role,
                    ..Default::default()
                },
            );
            args.insert("systemMessage".into(), sys_msg.into());
            let anonymous = event["chatter_is_anonymous"].as_bool().unwrap_or(false);
            args.insert("anonymous".into(), anonymous.into());
            // Shared chat: the same notices, from another channel in the session.
            let (shared, kind) = match ntype.strip_prefix("shared_chat_") {
                Some(k) => (true, k),
                None => (false, ntype),
            };
            if shared {
                args.insert("fromSharedChat".into(), true.into());
                event_by(&mut args, event, "source_broadcaster", "sourceBroadcaster");
            }
            let d = &event[ntype];
            let pick = |own: u32, other: u32| if shared { other } else { own };
            let gifter = |args: &mut Obj| {
                args.insert(
                    "anonymous".into(),
                    d["gifter_is_anonymous"].as_bool().unwrap_or(false).into(),
                );
                for (k, f) in [
                    ("gifterUser", "name"),
                    ("gifterUserName", "login"),
                    ("gifterUserId", "id"),
                ] {
                    args.insert(k.into(), d[format!("gifter_user_{f}")].clone());
                }
            };
            let months = |args: &mut Obj| {
                let duration = d["duration_months"].as_u64().unwrap_or(1);
                args.insert(
                    "tier".into(),
                    tier_name(es(d, "/sub_tier"), d["is_prime"].as_bool() == Some(true)).into(),
                );
                args.insert("isMultiMonth".into(), (duration > 1).into());
                args.insert("multiMonthDuration".into(), duration.into());
                // ponytail: EventSub doesn't say how far into a multi-month sub this is.
                args.insert("multiMonthTenure".into(), 0.into());
            };
            match kind {
                "announcement" => {
                    args.insert("announceColor".into(), d["color"].as_str().unwrap_or("PRIMARY").into());
                    let (code, name) = (pick(T_ANNOUNCEMENT, T_SHARED_CHAT_ANNOUNCEMENT), "Announcement");
                    go(engine, code, name, "Twitch/General", "Announcement", &args);
                }
                "sub" => {
                    months(&mut args);
                    if !shared {
                        count_subs(engine, cfg, broadcaster, 1);
                    }
                    let code = pick(T_SUB, T_SHARED_CHAT_SUB);
                    go(engine, code, "Subscription", "Twitch/Subscriptions", "Sub", &args);
                }
                "resub" => {
                    months(&mut args);
                    args.insert("cumulative".into(), d["cumulative_months"].as_u64().unwrap_or(1).into());
                    args.insert("monthStreak".into(), d["streak_months"].as_u64().unwrap_or(0).into());
                    args.insert("streakShared".into(), d["streak_months"].is_u64().into());
                    args.insert("isGift".into(), d["is_gift"].as_bool().unwrap_or(false).into());
                    if d["is_gift"] == true {
                        gifter(&mut args);
                    }
                    if !shared {
                        count_subs(engine, cfg, broadcaster, 1);
                    }
                    let code = pick(T_RESUB, T_SHARED_CHAT_RESUB);
                    go(engine, code, "Resubscription", "Twitch/Subscriptions", "ReSub", &args);
                }
                "sub_gift" => {
                    let bomb = es(d, "/community_gift_id").to_string();
                    let count = SEEN.with_borrow(|s| s.bombs.get(&bomb).copied().unwrap_or(0));
                    args.insert("tier".into(), tier_name(es(d, "/sub_tier"), false).into());
                    args.insert("recipientId".into(), d["recipient_user_id"].clone());
                    args.insert("recipientUserName".into(), d["recipient_user_login"].clone());
                    args.insert("recipientUser".into(), d["recipient_user_name"].clone());
                    args.insert("monthsGifted".into(), d["duration_months"].as_u64().unwrap_or(1).into());
                    args.insert(
                        "totalSubsGifted".into(),
                        d["cumulative_total"].as_u64().unwrap_or(0).into(),
                    );
                    args.insert("totalSubsGiftedShared".into(), d["cumulative_total"].is_u64().into());
                    args.insert("fromGiftBomb".into(), (!bomb.is_empty()).into());
                    args.insert("subBombCount".into(), count.into());
                    // ponytail: EventSub doesn't give the recipient's months; Streamer.bot reads IRC's.
                    args.insert("cumulativeMonths".into(), 0.into());
                    if !shared {
                        count_subs(engine, cfg, broadcaster, 1);
                    }
                    let code = pick(T_GIFT_SUB, T_SHARED_CHAT_SUB_GIFT);
                    go(
                        engine,
                        code,
                        "Gift Subscription",
                        "Twitch/Subscriptions",
                        "GiftSub",
                        &args,
                    );
                    let who = [
                        &d["recipient_user_name"],
                        &d["recipient_user_login"],
                        &d["recipient_user_id"],
                    ]
                    .map(Clone::clone);
                    let complete = SEEN.with_borrow_mut(|s| {
                        s.pending.get_mut(&bomb).is_some_and(|p| {
                            p.3.push(who);
                            p.3.len() as u64 >= p.2
                        })
                    });
                    if complete {
                        gift_bomb(engine, &bomb);
                    }
                }
                "community_sub_gift" => {
                    let total = d["total"].as_u64().unwrap_or(1);
                    SEEN.with_borrow_mut(|s| {
                        if s.bombs.len() > 50 {
                            s.bombs.clear();
                        }
                        s.bombs.insert(es(d, "/id").into(), total);
                    });
                    args.insert("tier".into(), tier_name(es(d, "/sub_tier"), false).into());
                    args.insert("gifts".into(), total.into());
                    args.insert("totalGifts".into(), d["cumulative_total"].as_u64().unwrap_or(0).into());
                    args.insert("totalGiftsShared".into(), d["cumulative_total"].is_u64().into());
                    args.insert("bonusGifts".into(), false.into());
                    let code = pick(T_GIFT_BOMB, T_SHARED_CHAT_COMMUNITY_SUB_GIFT);
                    // It fires once its gifts have come (each a sub_gift), to list who got them; or
                    // after 5 seconds with those that came.
                    let id = es(d, "/id").to_string();
                    SEEN.with_borrow_mut(|s| s.pending.insert(id.clone(), (code, args, total, Vec::new())));
                    let engine = engine.clone();
                    tokio::task::spawn_local(async move {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        gift_bomb(&engine, &id);
                    });
                }
                "gift_paid_upgrade" => {
                    gifter(&mut args);
                    let code = pick(T_GIFT_PAID_UPGRADE, T_SHARED_CHAT_GIFT_PAID_UPGRADE);
                    go(
                        engine,
                        code,
                        "Gift Paid Upgrade",
                        "Twitch/Subscriptions",
                        "GiftPaidUpgrade",
                        &args,
                    );
                }
                "prime_paid_upgrade" => {
                    args.insert("upgradeTier".into(), es(d, "/sub_tier").into());
                    args.insert("upgradeTierString".into(), tier_name(es(d, "/sub_tier"), false).into());
                    let code = pick(T_PRIME_PAID_UPGRADE, T_SHARED_CHAT_PRIME_PAID_UPGRADE);
                    go(
                        engine,
                        code,
                        "Prime Paid Upgrade",
                        "Twitch/Subscriptions",
                        "PrimePaidUpgrade",
                        &args,
                    );
                }
                "pay_it_forward" => {
                    gifter(&mut args);
                    let code = pick(T_PAY_IT_FORWARD, T_SHARED_CHAT_PAY_IT_FORWARD);
                    go(
                        engine,
                        code,
                        "Pay It Forward",
                        "Twitch/Subscriptions",
                        "PayItForward",
                        &args,
                    );
                }
                "raid" => {
                    populate_user_vars(
                        &mut args,
                        es(d, "/user_id"),
                        es(d, "/user_login"),
                        es(d, "/user_name"),
                        1,
                        false,
                    );
                    args.insert("viewers".into(), d["viewer_count"].as_u64().unwrap_or(0).into());
                    args.insert("profileImageUrl".into(), d["profile_image_url"].clone());
                    let code = pick(T_RAID, T_SHARED_CHAT_RAID);
                    go(engine, code, "Raid", "Twitch/Raid", "Raid", &args);
                }
                "bits_badge_tier" if !shared => {
                    args.insert("tier".into(), d["tier"].clone());
                    go(
                        engine,
                        T_BITS_BADGE_TIER,
                        "Bits Badge Tier",
                        "Twitch/Chat",
                        "BitsBadgeTier",
                        &args,
                    );
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
        "channel.raid" => {
            // Raids sent: incoming ones come as chat notifications (with more in them).
            if es(event, "/from_broadcaster_user_id") != broadcaster.user_id {
                return;
            }
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            args.insert("raidUser".into(), es(event, "/to_broadcaster_user_name").into());
            args.insert("raidUserName".into(), es(event, "/to_broadcaster_user_login").into());
            args.insert("raidUserId".into(), es(event, "/to_broadcaster_user_id").into());
            args.insert("viewers".into(), event["viewers"].as_u64().unwrap_or(0).into());
            let to = es(event, "/to_broadcaster_user_id").to_string();
            let known = SEEN
                .with_borrow_mut(|s| s.raid.take())
                .filter(|r| r["raidUserId"] == to.as_str());
            let engine = engine.clone();
            // The raided channel's picture: from Raid Start, else looked up.
            tokio::task::spawn_local(async move {
                let raided = match known {
                    Some(r) => r,
                    None => match engine.twitch() {
                        Some(tw) => tw
                            .helix("GET", "users", vec![("id", to.as_str())], None, false, false)
                            .await
                            .map(|v| raid_target(&v["data"][0]))
                            .unwrap_or_default(),
                        None => Obj::new(),
                    },
                };
                args.extend(raided);
                go(&engine, T_RAID_SEND, "Raid Send", "Twitch/Raid", "RaidSend", &args);
            });
        }
        "stream.online" => {
            if cfg.borrow().twitch.first_words_auto_reset {
                engine.reset_first_words(Some("twitch"));
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
            // A pin "until the stream ends" ends here.
            engine.chat_unpinned("twitch", "");
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
        "channel.channel_points_custom_reward_redemption.add"
        | "channel.channel_points_custom_reward_redemption.update" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            let input = es(event, "/user_input");
            let rid = es(event, "/reward/id");
            for (k, v) in [
                ("id", event["id"].clone()),
                ("redemptionId", event["id"].clone()),
                ("rewardId", rid.into()),
                ("rewardTitle", event["reward"]["title"].clone()),
                ("rewardName", event["reward"]["title"].clone()),
                ("rewardPrompt", event["reward"]["prompt"].clone()),
                ("rewardCost", event["reward"]["cost"].clone()),
                ("userInput", input.into()),
                ("rawInput", input.into()),
                ("rawInputEscaped", commands::escape(input).into()),
                ("rawInputUrlEncoded", commands::url_encode(input).into()),
                ("status", event["status"].clone()),
                ("rewardStatus", event["status"].clone()),
                ("redeemedAt", event["redeemed_at"].clone()),
            ] {
                args.insert(k.into(), v);
            }
            // The rest of the reward's settings, from its last known state.
            if let Some(r) = SEEN.with_borrow(|s| s.rewards.get(rid).cloned()) {
                for (k, v) in [
                    ("skipsQueue", r["should_redemptions_skip_request_queue"].clone()),
                    ("requiresUserInput", r["is_user_input_required"].clone()),
                    ("maxPerStream", r["max_per_stream_setting"]["max_per_stream"].clone()),
                    (
                        "maxPerUserPerStream",
                        r["max_per_user_per_stream_setting"]["max_per_user_per_stream"].clone(),
                    ),
                    (
                        "globalCooldown",
                        r["global_cooldown_setting"]["global_cooldown_seconds"].clone(),
                    ),
                    ("backgroundColor", r["background_color"].clone()),
                ] {
                    args.insert(k.into(), v);
                }
            }
            if sub_type.ends_with(".add") {
                go(
                    engine,
                    T_REWARD_REDEMPTION,
                    "Reward Redemption",
                    "Twitch/Channel Points",
                    "RewardRedemption",
                    &args,
                );
            } else {
                go(
                    engine,
                    T_REWARD_REDEMPTION_UPDATED,
                    "Reward Redemption Updated",
                    "Twitch/Channel Points",
                    "RewardRedemptionUpdated",
                    &args,
                );
            }
        }
        "channel.channel_points_custom_reward.add"
        | "channel.channel_points_custom_reward.update"
        | "channel.channel_points_custom_reward.remove" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let id = es(event, "/id").to_string();
            for (k, f) in [
                ("rewardId", "id"),
                ("rewardTitle", "title"),
                ("rewardCost", "cost"),
                ("rewardPrompt", "prompt"),
            ] {
                args.insert(k.into(), event[f].clone());
            }
            SEEN.with_borrow_mut(|s| {
                if sub_type.ends_with("remove") {
                    s.rewards.remove(&id);
                } else {
                    s.rewards.insert(id, event.clone());
                }
            });
            let (code, name, ev) = match sub_type {
                "channel.channel_points_custom_reward.add" => (T_REWARD_CREATED, "Reward Created", "RewardCreated"),
                "channel.channel_points_custom_reward.update" => (T_REWARD_UPDATED, "Reward Updated", "RewardUpdated"),
                _ => (T_REWARD_DELETED, "Reward Deleted", "RewardDeleted"),
            };
            go(engine, code, name, "Twitch/Channel Points", ev, &args);
        }
        "channel.poll.begin" | "channel.poll.progress" | "channel.poll.end" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            // EventSub has no duration: it's from the start to the end.
            let mut p = event.clone();
            let start = crate::engine::rest_parse_iso(es(event, "/started_at"));
            let end = crate::engine::rest_parse_iso(es(event, "/ends_at"))
                .or_else(|| crate::engine::rest_parse_iso(es(event, "/ended_at")));
            if let (Some(a), Some(b)) = (start, end) {
                p["duration"] = (b - a).into();
            }
            crate::engine::poll_args(&p, &mut args);
            args.insert("pollId".into(), event["id"].clone());
            args.insert("pollTitle".into(), event["title"].clone());
            let status = es(event, "/status");
            args.insert(
                "status".into(),
                if status.is_empty() { "active" } else { status }.into(),
            );
            if sub_type == "channel.poll.end" {
                args.insert("active".into(), false.into());
                args.insert("poll.endedAt".into(), event["ended_at"].clone());
                let choices = event["choices"].as_array().cloned().unwrap_or_default();
                let total = |c: &Value| c["votes"].as_i64().unwrap_or(0);
                if let Some((i, c)) = choices.iter().enumerate().max_by_key(|(_, c)| total(c)) {
                    let points = c["channel_points_votes"].as_i64().unwrap_or(0);
                    args.insert("poll.winningIndex".into(), i.into());
                    args.insert("poll.winningChoice.id".into(), c["id"].clone());
                    args.insert("poll.winningChoice.title".into(), c["title"].clone());
                    args.insert("poll.winningChoice.votes".into(), (total(c) - points).into());
                    args.insert("poll.winningChoice.rewardVotes".into(), points.into());
                    args.insert("poll.winningChoice.totalVotes".into(), total(c).into());
                }
            }
            args.insert("poll._json".into(), Value::Object(args.clone()).to_string().into());
            let (code, name, ev) = match (sub_type, status) {
                ("channel.poll.begin", _) => (T_POLL_CREATED, "Poll Created", "PollCreated"),
                ("channel.poll.progress", _) => (T_POLL_UPDATED, "Poll Updated", "PollUpdated"),
                (_, "terminated") => (T_POLL_TERMINATED, "Poll Terminated", "PollTerminated"),
                (_, "archived") => (T_POLL_ARCHIVED, "Poll Archived", "PollArchived"),
                _ => (T_POLL_COMPLETED, "Poll Completed", "PollCompleted"),
            };
            go(engine, code, name, "Twitch/Polls", ev, &args);
        }
        "channel.prediction.begin"
        | "channel.prediction.progress"
        | "channel.prediction.lock"
        | "channel.prediction.end" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let mut p = event.clone();
            p["created_at"] = event["started_at"].clone();
            if let (Some(a), Some(b)) = (
                crate::engine::rest_parse_iso(es(event, "/started_at")),
                crate::engine::rest_parse_iso(es(event, "/locks_at")),
            ) {
                p["prediction_window"] = (b - a).into();
            }
            let status = es(event, "/status");
            p["status"] = if status.is_empty() { "ACTIVE" } else { status }.into();
            crate::engine::prediction_args(&p, &mut args);
            args.insert("prediction.LockedAt".into(), event["locked_at"].clone());
            args.insert("prediction.EndedAt".into(), event["ended_at"].clone());
            args.insert("predictionId".into(), event["id"].clone());
            args.insert("predictionTitle".into(), event["title"].clone());
            args.insert("status".into(), status.into());
            let won = es(event, "/winning_outcome_id");
            args.insert("winningOutcomeId".into(), won.into());
            let outcomes = event["outcomes"].as_array().cloned().unwrap_or_default();
            if let Some((i, o)) = outcomes.iter().enumerate().find(|(_, o)| o["id"] == won) {
                args.insert("prediction.winningIndex".into(), i.into());
                for (k, f) in [
                    ("id", "id"),
                    ("title", "title"),
                    ("users", "users"),
                    ("points", "channel_points"),
                    ("color", "color"),
                ] {
                    args.insert(format!("prediction.winningOutcome.{k}"), o[f].clone());
                }
            }
            args.insert(
                "prediction._json".into(),
                Value::Object(args.clone()).to_string().into(),
            );
            let (code, name, ev) = match (sub_type, status) {
                ("channel.prediction.begin", _) => (T_PREDICTION_CREATED, "Prediction Created", "PredictionCreated"),
                ("channel.prediction.progress", _) => (T_PREDICTION_UPDATED, "Prediction Updated", "PredictionUpdated"),
                ("channel.prediction.lock", _) => (T_PREDICTION_LOCKED, "Prediction Locked", "PredictionLocked"),
                (_, "canceled") => (T_PREDICTION_CANCELED, "Prediction Canceled", "PredictionCanceled"),
                _ => (T_PREDICTION_COMPLETED, "Prediction Completed", "PredictionCompleted"),
            };
            go(engine, code, name, "Twitch/Predictions", ev, &args);
        }
        "channel.hype_train.begin" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            hype_args(&mut args, event);
            SEEN.with_borrow_mut(|s| s.hype_level = event["level"].as_u64().unwrap_or(1));
            go(
                engine,
                T_HYPE_TRAIN_START,
                "Hype Train Starts",
                "Twitch/Hype Train",
                "HypeTrainStart",
                &args,
            );
        }
        "channel.hype_train.progress" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            hype_args(&mut args, event);
            let level = event["level"].as_u64().unwrap_or(1);
            let prev = SEEN.with_borrow_mut(|s| std::mem::replace(&mut s.hype_level, level));
            go(
                engine,
                T_HYPE_TRAIN_UPDATE,
                "Hype Train Update",
                "Twitch/Hype Train",
                "HypeTrainUpdate",
                &args,
            );
            if level > prev && prev > 0 {
                args.insert("prevLevel".into(), prev.into());
                go(
                    engine,
                    T_HYPE_TRAIN_LEVEL_UP,
                    "Hype Train Level Up",
                    "Twitch/Hype Train",
                    "HypeTrainLevelUp",
                    &args,
                );
            }
        }
        "channel.hype_train.end" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            hype_args(&mut args, event);
            SEEN.with_borrow_mut(|s| s.hype_level = 0);
            go(
                engine,
                T_HYPE_TRAIN_END,
                "Hype Train End",
                "Twitch/Hype Train",
                "HypeTrainEnd",
                &args,
            );
        }
        "channel.goal.begin" | "channel.goal.progress" | "channel.goal.end" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            for (old, new, f) in [
                ("goalId", "id", "id"),
                ("goalType", "type", "type"),
                ("goalDescription", "description", "description"),
                ("currentAmount", "currentAmount", "current_amount"),
                ("targetAmount", "targetAmount", "target_amount"),
                ("startedAt", "startedAt", "started_at"),
                ("endedAt", "endedAt", "ended_at"),
            ] {
                if let Some(v) = event.get(f) {
                    args.insert(old.into(), v.clone());
                    args.insert(format!("goal.{new}"), v.clone());
                }
            }
            let (code, name, ev) = match sub_type {
                "channel.goal.begin" => (T_GOAL_BEGIN, "Goal Begin", "GoalBegin"),
                "channel.goal.progress" => (T_GOAL_PROGRESS, "Goal Progress", "GoalProgress"),
                _ => (T_GOAL_END, "Goal End", "GoalEnd"),
            };
            go(engine, code, name, "Twitch/Goals", ev, &args);
        }
        "channel.update" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let (title, game_id, game) = (
                es(event, "/title"),
                es(event, "/category_id"),
                es(event, "/category_name"),
            );
            let art = |id: &str| format!("https://static-cdn.jtvnw.net/ttv-boxart/{id}-144x192.jpg");
            let (old_title, (old_id, old_game)) = SEEN.with_borrow_mut(|s| {
                let old = (s.title.clone(), s.game.clone());
                s.title = title.into();
                s.game = (game_id.into(), game.into());
                old
            });
            for (k, v) in [
                ("status", title.to_string()),
                ("game", game.into()),
                ("gameName", game.into()),
                ("gameId", game_id.into()),
                ("gameBoxArt", art(game_id)),
                ("oldStatus", old_title.clone()),
                ("oldGameId", old_id.clone()),
                ("oldGameName", old_game),
                ("oldGameBoxArt", art(&old_id)),
            ] {
                args.insert(k.into(), v.into());
            }
            args.insert("statusUpdate".into(), (old_title != title).into());
            args.insert("gameUpdate".into(), (old_id != game_id).into());
            go(
                engine,
                T_STREAM_UPDATE,
                "Stream Update",
                "Twitch/Channel",
                "StreamUpdate",
                &args,
            );
        }
        "channel.ad_break.begin" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let length = event["duration_seconds"].as_u64().unwrap_or(0);
            let auto = event["is_automatic"].as_bool().unwrap_or(false);
            for (k, v) in [
                ("duration", length.into()),
                ("length", length.into()),
                ("adLength", length.into()),
                ("adLengthMs", (length * 1000).into()),
                ("isAutomatic", auto.into()),
                ("adScheduled", auto.into()),
                ("startedAt", event["started_at"].clone()),
            ] {
                args.insert(k.into(), v);
            }
            go(engine, T_AD_RUN, "Ad Run", "Twitch/Ads", "AdRun", &args);
        }
        "channel.ban" => {
            engine.chat_remove("twitch", "", es(event, "/user_id"));
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            event_by(&mut args, event, "moderator", "createdBy");
            let perm = event["is_permanent"].as_bool().unwrap_or(true);
            args.insert("reason".into(), es(event, "/reason").into());
            args.insert("isPermanent".into(), perm.into());
            if perm {
                go(
                    engine,
                    T_USER_BANNED,
                    "User Banned",
                    "Twitch/Moderation",
                    "UserBanned",
                    &args,
                );
            } else {
                let (from, to) = (es(event, "/banned_at"), es(event, "/ends_at"));
                args.insert("endsAt".into(), to.into());
                if let (Some(a), Some(b)) = (crate::engine::rest_parse_iso(from), crate::engine::rest_parse_iso(to)) {
                    args.insert("duration".into(), (b - a).into());
                }
                go(
                    engine,
                    T_USER_TIMED_OUT,
                    "User Timed Out",
                    "Twitch/Moderation",
                    "UserTimedOut",
                    &args,
                );
            }
        }
        "channel.unban" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            event_by(&mut args, event, "moderator", "createdBy");
            // EventSub has one unban for both; Streamer.bot has two triggers.
            go(
                engine,
                T_USER_UNBANNED,
                "User Unbanned",
                "Twitch/Moderation",
                "UserUnbanned",
                &args,
            );
            go(
                engine,
                T_USER_UNTIMED_OUT,
                "User Untimed Out",
                "Twitch/Moderation",
                "UserUntimedOut",
                &args,
            );
        }
        "channel.moderator.add" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let uid = event.pointer("/user_id").and_then(Value::as_str).unwrap_or("");
            let ulogin = event.pointer("/user_login").and_then(Value::as_str).unwrap_or("");
            let uname = event.pointer("/user_name").and_then(Value::as_str).unwrap_or("");
            populate_user_vars(&mut args, uid, ulogin, uname, 3, false);
            engine.fire(
                T_MODERATOR_ADDED,
                "Moderator Added",
                "Twitch/Moderation",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "ModeratorAdded", || json!(args));
        }
        "channel.moderator.remove" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let uid = event.pointer("/user_id").and_then(Value::as_str).unwrap_or("");
            let ulogin = event.pointer("/user_login").and_then(Value::as_str).unwrap_or("");
            let uname = event.pointer("/user_name").and_then(Value::as_str).unwrap_or("");
            populate_user_vars(&mut args, uid, ulogin, uname, 1, false);
            engine.fire(
                T_MODERATOR_REMOVED,
                "Moderator Removed",
                "Twitch/Moderation",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "ModeratorRemoved", || json!(args));
        }
        "channel.vip.add" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let uid = event.pointer("/user_id").and_then(Value::as_str).unwrap_or("");
            let ulogin = event.pointer("/user_login").and_then(Value::as_str).unwrap_or("");
            let uname = event.pointer("/user_name").and_then(Value::as_str).unwrap_or("");
            populate_user_vars(&mut args, uid, ulogin, uname, 2, false);
            engine.fire(T_VIP_ADDED, "VIP Added", "Twitch/Moderation", "twitch", &args, |_| true);
            engine.emit("Twitch", "VipAdded", || json!(args));
        }
        "channel.vip.remove" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let uid = event.pointer("/user_id").and_then(Value::as_str).unwrap_or("");
            let ulogin = event.pointer("/user_login").and_then(Value::as_str).unwrap_or("");
            let uname = event.pointer("/user_name").and_then(Value::as_str).unwrap_or("");
            populate_user_vars(&mut args, uid, ulogin, uname, 1, false);
            engine.fire(
                T_VIP_REMOVED,
                "VIP Removed",
                "Twitch/Moderation",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "VipRemoved", || json!(args));
        }
        "channel.shield_mode.begin" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            engine.fire(
                T_SHIELD_MODE_BEGIN,
                "Shield Mode Begin",
                "Twitch/Moderation",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "ShieldModeBegin", || json!(args));
        }
        "channel.shield_mode.end" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            engine.fire(
                T_SHIELD_MODE_END,
                "Shield Mode End",
                "Twitch/Moderation",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "ShieldModeEnd", || json!(args));
        }
        "channel.shoutout.create" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let (login, name, id) = (
                es(event, "/to_broadcaster_user_login"),
                es(event, "/to_broadcaster_user_name"),
                es(event, "/to_broadcaster_user_id"),
            );
            for (k, v) in [
                ("targetUser", name),
                ("targetUserName", login),
                ("targetUserLogin", login),
                ("targetUserDisplayName", name),
                ("targetUserId", id),
                ("cooldownEndsAt", es(event, "/cooldown_ends_at")),
                ("targetCooldownEndsAt", es(event, "/target_cooldown_ends_at")),
            ] {
                args.insert(k.into(), v.into());
            }
            args.insert("viewerCount".into(), event["viewer_count"].as_u64().unwrap_or(0).into());
            event_moderator(&mut args, event);
            go(
                engine,
                T_SHOUTOUT_CREATED,
                "Shoutout Created",
                "Twitch/Moderation",
                "ShoutoutCreated",
                &args,
            );
        }
        "channel.shoutout.receive" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let from_login = event
                .pointer("/from_broadcaster_user_login")
                .and_then(Value::as_str)
                .unwrap_or("");
            let from_name = event
                .pointer("/from_broadcaster_user_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let from_id = event
                .pointer("/from_broadcaster_user_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let viewers = event.pointer("/viewer_count").and_then(Value::as_u64).unwrap_or(0);
            args.insert("fromUser".into(), from_name.into());
            args.insert("fromUserName".into(), from_login.into());
            args.insert("fromUserId".into(), from_id.into());
            args.insert("viewerCount".into(), viewers.into());
            engine.fire(
                T_SHOUTOUT_RECEIVED,
                "Shoutout Received",
                "Twitch/General",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "ShoutoutReceived", || json!(args));
        }
        "channel.chat.clear_user_messages" => engine.chat_remove("twitch", "", es(event, "/target_user_id")),
        "channel.chat.clear" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            engine.fire(
                T_CHAT_CLEARED,
                "Chat Cleared",
                "Twitch/Moderation",
                "twitch",
                &args,
                |_| true,
            );
            engine.emit("Twitch", "ChatCleared", || json!(args));
        }
        "channel.chat_settings.update" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            let f = |k: &str| event[k].as_bool().unwrap_or(false);
            let n = |k: &str| event[k].as_u64().unwrap_or(0);
            for (k, v) in [
                ("emoteMode", f("emote_mode").into()),
                ("followerMode", f("follower_mode").into()),
                ("followerModeDuration", n("follower_mode_duration_minutes").into()),
                ("slowMode", f("slow_mode").into()),
                ("slowModeWaitTime", n("slow_mode_wait_time_seconds").into()),
                ("subscriberOnly", f("subscriber_mode").into()),
                ("uniqueMode", f("unique_chat_mode").into()),
            ] {
                args.insert(k.into(), v);
            }
            // Only what changed since the last update (or since connecting) fires.
            let old = SEEN
                .with_borrow_mut(|s| s.settings.replace(event.clone()))
                .unwrap_or_default();
            let was = |k: &str| old[k].as_bool().unwrap_or(false);
            let modes = [
                (
                    "emote_mode",
                    T_CHAT_MODE_EMOTE_ON,
                    T_CHAT_MODE_EMOTE_OFF,
                    "Emote Mode",
                    "EmoteMode",
                ),
                (
                    "subscriber_mode",
                    T_CHAT_MODE_SUBSCRIBER_ON,
                    T_CHAT_MODE_SUBSCRIBER_OFF,
                    "Subscriber Mode",
                    "SubscriberMode",
                ),
                (
                    "unique_chat_mode",
                    T_CHAT_MODE_UNIQUE_ON,
                    T_CHAT_MODE_UNIQUE_OFF,
                    "Unique Mode",
                    "UniqueMode",
                ),
                (
                    "slow_mode",
                    T_CHAT_MODE_SLOW_ON,
                    T_CHAT_MODE_SLOW_OFF,
                    "Slow Mode",
                    "SlowMode",
                ),
                (
                    "follower_mode",
                    T_CHAT_MODE_FOLLOWER_ON,
                    T_CHAT_MODE_FOLLOWER_OFF,
                    "Follower Mode",
                    "FollowerMode",
                ),
            ];
            for (k, on, off, name, ev) in modes {
                if f(k) != was(k) {
                    let (code, state) = if f(k) { (on, "On") } else { (off, "Off") };
                    go(
                        engine,
                        code,
                        &format!("{name} {state}"),
                        "Twitch/Chat Modes",
                        &format!("{ev}{state}"),
                        &args,
                    );
                }
            }
            for (k, length, code, name, ev) in [
                (
                    "slow_mode",
                    "slow_mode_wait_time_seconds",
                    T_CHAT_MODE_SLOW_CHANGED,
                    "Slow Mode Changed",
                    "SlowModeChanged",
                ),
                (
                    "follower_mode",
                    "follower_mode_duration_minutes",
                    T_CHAT_MODE_FOLLOWER_CHANGED,
                    "Follower Mode Changed",
                    "FollowerModeChanged",
                ),
            ] {
                if f(k) != was(k) || (f(k) && old[length] != event[length]) {
                    go(engine, code, name, "Twitch/Chat Modes", ev, &args);
                }
            }
        }
        "channel.charity_campaign.donate" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            charity_args(&mut args, event);
            args.insert("amount".into(), event["amount"]["value"].clone());
            go(
                engine,
                T_CHARITY_DONATION,
                "Charity Donation",
                "Twitch/Charity",
                "CharityDonation",
                &args,
            );
        }
        "channel.charity_campaign.start" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            charity_args(&mut args, event);
            args.insert("charityName".into(), es(event, "/charity_name").into());
            go(
                engine,
                T_CHARITY_STARTED,
                "Charity Started",
                "Twitch/Charity",
                "CharityStarted",
                &args,
            );
        }
        "channel.charity_campaign.progress" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            charity_args(&mut args, event);
            args.insert("currentAmount".into(), event["current_amount"]["value"].clone());
            args.insert("targetAmount".into(), event["target_amount"]["value"].clone());
            go(
                engine,
                T_CHARITY_PROGRESS,
                "Charity Progress",
                "Twitch/Charity",
                "CharityProgress",
                &args,
            );
        }
        "channel.charity_campaign.stop" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            charity_args(&mut args, event);
            go(
                engine,
                T_CHARITY_COMPLETED,
                "Charity Completed",
                "Twitch/Charity",
                "CharityCompleted",
                &args,
            );
        }
        "channel.guest_star_session.begin" | "channel.guest_star_session.end" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            args.insert("sessionId".into(), event["session_id"].clone());
            if sub_type.ends_with("begin") {
                go(
                    engine,
                    T_GUEST_STAR_SESSION_BEGIN,
                    "Guest Star Session Begin",
                    "Twitch/Guest Star",
                    "GuestStarSessionBegin",
                    &args,
                );
            } else {
                go(
                    engine,
                    T_GUEST_STAR_SESSION_END,
                    "Guest Star Session End",
                    "Twitch/Guest Star",
                    "GuestStarSessionEnd",
                    &args,
                );
            }
        }
        "channel.guest_star_guest.update" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            for (k, f) in [
                ("sessionId", "session_id"),
                ("moderatorUserName", "moderator_user_name"),
                ("moderatorUserLogin", "moderator_user_login"),
                ("moderatorUserId", "moderator_user_id"),
                ("guestUserName", "guest_user_name"),
                ("guestUserLogin", "guest_user_login"),
                ("guestUserId", "guest_user_id"),
                ("slotId", "slot_id"),
                ("state", "state"),
                ("hostVideoEnabled", "host_video_enabled"),
                ("hostAudioEnabled", "host_audio_enabled"),
                ("hostVolume", "host_volume"),
            ] {
                args.insert(k.into(), event[f].clone());
            }
            go(
                engine,
                T_GUEST_STAR_GUEST_UPDATE,
                "Guest Star Guest Update",
                "Twitch/Guest Star",
                "GuestStarGuestUpdate",
                &args,
            );
        }
        "channel.guest_star_settings.update" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            for (k, f) in [
                ("isModeratorSendLiveEnabled", "is_moderator_send_live_enabled"),
                ("slotCount", "slot_count"),
                ("isBrowserSourceAudioEnabled", "is_browser_source_audio_enabled"),
                ("groupLayout", "group_layout"),
            ] {
                args.insert(k.into(), event[f].clone());
            }
            go(
                engine,
                T_GUEST_STAR_SETTINGS_UPDATE,
                "Guest Star Settings Update",
                "Twitch/Guest Star",
                "GuestStarSettingsUpdate",
                &args,
            );
        }
        "channel.shared_chat.begin" | "channel.shared_chat.update" | "channel.shared_chat.end" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            for (k, f) in [("userId", "id"), ("userLogin", "login"), ("userName", "name")] {
                args.insert(
                    format!("host.{k}"),
                    es(event, &format!("/host_broadcaster_user_{f}")).into(),
                );
            }
            for k in ["isSubscribed", "isModerator", "isVip"] {
                args.insert(format!("host.{k}"), false.into());
            }
            args.insert("sessionId".into(), event["session_id"].clone());
            let (code, name, ev) = match sub_type {
                "channel.shared_chat.begin" => (
                    T_SHARED_CHAT_SESSION_BEGIN,
                    "Twitch Shared Chat Session Begin",
                    "SharedChatSessionBegin",
                ),
                "channel.shared_chat.update" => (
                    T_SHARED_CHAT_SESSION_UPDATE,
                    "Twitch Shared Chat Session Update",
                    "SharedChatSessionUpdate",
                ),
                _ => (
                    T_SHARED_CHAT_SESSION_END,
                    "Twitch Shared Chat Session End",
                    "SharedChatSessionEnd",
                ),
            };
            go(engine, code, name, "Twitch/Shared Chat", ev, &args);
        }
        "channel.chat.message_delete" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "target");
            let id = es(event, "/message_id");
            engine.chat_remove("twitch", id, "");
            let text = SEEN.with_borrow(|s| s.messages.iter().find(|m| m.0 == id).map(|m| m.1.clone()));
            args.insert("targetMessageId".into(), id.into());
            args.insert("message".into(), text.unwrap_or_default().into());
            go(
                engine,
                T_CHAT_MESSAGE_DELETED,
                "Chat Message Deleted",
                "Twitch/Moderation",
                "ChatMessageDeleted",
                &args,
            );
        }
        "channel.moderate" => {
            // Only shared chat's: this channel's own come as their own events.
            let source = es(event, "/source_broadcaster_user_id");
            if source.is_empty() || source == broadcaster.user_id {
                return;
            }
            let action = es(event, "/action");
            let d = &event[action.replace("shared_chat_", "")];
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_by(&mut args, event, "moderator", "createdBy");
            event_by(&mut args, event, "source_broadcaster", "sourceBroadcaster");
            populate_user_vars(
                &mut args,
                es(d, "/user_id"),
                es(d, "/user_login"),
                es(d, "/user_name"),
                1,
                false,
            );
            args.insert("reason".into(), d["reason"].clone());
            args.insert("fromSharedChat".into(), true.into());
            let (code, name, ev) = match action {
                "shared_chat_ban" => (
                    T_SHARED_CHAT_USER_BANNED,
                    "Shared Chat User Banned",
                    "SharedChatUserBanned",
                ),
                "shared_chat_timeout" => (
                    T_SHARED_CHAT_USER_TIMED_OUT,
                    "Shared Chat User Timed Out",
                    "SharedChatUserTimedOut",
                ),
                "shared_chat_unban" => (
                    T_SHARED_CHAT_USER_UNBANNED,
                    "Shared Chat User Unbanned",
                    "SharedChatUserUnbanned",
                ),
                "shared_chat_untimeout" => (
                    T_SHARED_CHAT_USER_UNTIMED_OUT,
                    "Shared Chat User Untimed Out",
                    "SharedChatUserUntimedOut",
                ),
                "shared_chat_delete" => {
                    args.insert("targetMessageId".into(), d["message_id"].clone());
                    args.insert("message".into(), d["message_body"].clone());
                    (
                        T_SHARED_CHAT_MESSAGE_DELETED,
                        "Shared Chat Message Deleted",
                        "SharedChatMessageDeleted",
                    )
                }
                _ => return,
            };
            if let Some(t) = d["expires_at"].as_str() {
                args.insert("endsAt".into(), t.into());
            }
            go(engine, code, name, "Twitch/Shared Chat", ev, &args);
        }
        "automod.message.hold" | "automod.message.update" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            let text = es(event, "/message/text");
            commands::input_args(text, text, &mut args);
            args.insert("messageId".into(), event["message_id"].clone());
            args.insert("rawInput".into(), text.into());
            args.insert("category".into(), event["automod"]["category"].clone());
            args.insert("level".into(), event["automod"]["level"].clone());
            args.insert("reason".into(), event["reason"].clone());
            args.insert("heldAt".into(), event["held_at"].clone());
            if sub_type.ends_with("hold") {
                go(
                    engine,
                    T_AUTOMOD_MESSAGE_HELD,
                    "AutoMod Message Held",
                    "Twitch/Moderation",
                    "AutomodMessageHeld",
                    &args,
                );
            } else {
                args.insert("status".into(), event["status"].clone());
                event_moderator(&mut args, event);
                go(
                    engine,
                    T_AUTOMOD_MESSAGE_UPDATED,
                    "AutoMod Message Updated",
                    "Twitch/Moderation",
                    "AutomodMessageUpdated",
                    &args,
                );
            }
        }
        "automod.terms.update" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_by(&mut args, event, "moderator", "moderator");
            // Streamer.bot's names: moderatorUsername is the display name, moderatorDisplayName the login.
            args.insert("moderatorUsername".into(), es(event, "/moderator_user_name").into());
            args.insert("moderatorDisplayName".into(), es(event, "/moderator_user_login").into());
            let terms = event["terms"].as_array().cloned().unwrap_or_default();
            args.insert("termCount".into(), terms.len().into());
            for (i, t) in terms.iter().enumerate() {
                args.insert(format!("term.[{i}]"), t.clone());
            }
            args.insert(
                "fromAutomod".into(),
                event["from_automod"].as_bool().unwrap_or(false).into(),
            );
            args.insert("createdAt".into(), crate::fmt::date(&crate::fmt::now(), "o").into());
            let (code, name, ev) = match es(event, "/action") {
                "add_blocked" => (T_BLOCKED_TERMS_ADDED, "Blocked Terms Added", "BlockedTermsAdded"),
                "remove_blocked" => (T_BLOCKED_TERMS_DELETED, "Blocked Terms Deleted", "BlockedTermsDeleted"),
                "add_permitted" => (T_PERMITTED_TERMS_ADDED, "Permitted Terms Added", "PermittedTermsAdded"),
                "remove_permitted" => (
                    T_PERMITTED_TERMS_DELETED,
                    "Permitted Terms Deleted",
                    "PermittedTermsDeleted",
                ),
                _ => return,
            };
            go(engine, code, name, "Twitch/Moderation", ev, &args);
        }
        "channel.unban_request.create" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            args.insert("requestId".into(), event["id"].clone());
            args.insert("text".into(), event["text"].clone());
            args.insert("createdAt".into(), event["created_at"].clone());
            go(
                engine,
                T_UNBAN_REQUEST_CREATED,
                "Unban Request Created",
                "Twitch/Moderation",
                "UnbanRequestCreated",
                &args,
            );
        }
        "channel.unban_request.resolve" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            event_moderator(&mut args, event);
            args.insert("requestId".into(), event["id"].clone());
            args.insert("message".into(), event["resolution_text"].clone());
            args.insert("createdAt".into(), crate::fmt::date(&crate::fmt::now(), "o").into());
            if es(event, "/status") == "approved" {
                go(
                    engine,
                    T_UNBAN_REQUEST_APPROVED,
                    "Unban Request Approved",
                    "Twitch/Moderation",
                    "UnbanRequestApproved",
                    &args,
                );
            } else {
                go(
                    engine,
                    T_UNBAN_REQUEST_DENIED,
                    "Unban Request Denied",
                    "Twitch/Moderation",
                    "UnbanRequestDenied",
                    &args,
                );
            }
        }
        "channel.warning.send" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            event_moderator(&mut args, event);
            args.insert("reason".into(), event["reason"].clone());
            go(
                engine,
                T_WARNED_USER,
                "Warned User",
                "Twitch/Moderation",
                "WarnedUser",
                &args,
            );
        }
        "channel.warning.acknowledge" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            go(
                engine,
                T_WARNING_ACKNOWLEDGED,
                "Warning Acknowledged",
                "Twitch/Moderation",
                "WarningAcknowledged",
                &args,
            );
        }
        "channel.suspicious_user.update" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            event_moderator(&mut args, event);
            args.insert("lowTrustStatus".into(), event["low_trust_status"].clone());
            go(
                engine,
                T_SUSPICIOUS_USER_UPDATE,
                "Suspicious User Update",
                "Twitch/Moderation",
                "SuspiciousUserUpdate",
                &args,
            );
        }
        "channel.suspicious_user.message" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            let text = es(event, "/message/text");
            populate_chat_vars(
                &mut args,
                &ChatContext {
                    text,
                    msg_id: es(event, "/message/message_id"),
                    ..Default::default()
                },
            );
            args.insert("lowTrustStatus".into(), event["low_trust_status"].clone());
            args.insert("banEvasionEvaluation".into(), event["ban_evasion_evaluation"].clone());
            args.insert("sharedBanChannelIds".into(), event["shared_ban_channel_ids"].clone());
            go(
                engine,
                T_SUSPICIOUS_USER_MESSAGE,
                "Suspicious User Message",
                "Twitch/Moderation",
                "SuspiciousUserMessage",
                &args,
            );
        }
        "channel.channel_points_automatic_reward_redemption.add" => {
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            let r = &event["reward"];
            let kind = es(r, "/type");
            args.insert("redemptionId".into(), event["id"].clone());
            args.insert("rewardType".into(), kind.into());
            args.insert("rewardCost".into(), r["channel_points"].clone());
            args.insert("userInput".into(), event["message"]["text"].clone());
            let (id, name) = (es(r, "/emote/id"), es(r, "/emote/name"));
            if kind == "gigantify_an_emote" {
                args.insert("gigantifiedEmoteId".into(), id.into());
                args.insert("gigantifiedEmoteName".into(), name.into());
                let url = format!("https://static-cdn.jtvnw.net/emoticons/v2/{id}/default/dark/3.0");
                args.insert("gigantifiedEmoteUrl".into(), url.into());
            } else if !id.is_empty() {
                args.insert("unlockedEmoteId".into(), id.into());
                args.insert("unlockedEmoteName".into(), name.into());
            }
            go(
                engine,
                T_AUTOMATIC_REWARD_REDEMPTION,
                "Automatic Reward Redemption",
                "Twitch/Channel Points",
                "AutomaticRewardRedemption",
                &args,
            );
        }
        "channel.bits.use" => {
            // Cheers come with the chat message; this is for power-ups.
            if es(event, "/type") != "power_up" {
                return;
            }
            let mut args = Obj::new();
            populate_broadcaster_vars(&mut args, broadcaster);
            event_user(&mut args, event, "user");
            let p = &event["power_up"];
            args.insert("bits".into(), event["bits"].clone());
            args.insert("powerUpType".into(), p["type"].clone());
            args.insert("emoteId".into(), p["emote"]["id"].clone());
            args.insert("emoteName".into(), p["emote"]["name"].clone());
            args.insert("messageEffectId".into(), p["message_effect_id"].clone());
            args.insert("message".into(), event["message"]["text"].clone());
            go(
                engine,
                T_POWER_UP_REDEMPTION,
                "Power Up Redemption",
                "Twitch/Channel Points",
                "PowerUpRedemption",
                &args,
            );
        }
        _ => {}
    }
}

/// A string field of an EventSub event ("" when missing).
fn es<'a>(e: &'a Value, p: &str) -> &'a str {
    e.pointer(p).and_then(Value::as_str).unwrap_or("")
}

/// Fire a Twitch trigger and send its WebSocket event, with the same arguments.
fn go(engine: &Rc<Engine>, code: u32, name: &str, cat: &str, ev: &str, args: &Obj) {
    engine.fire(code, name, cat, "twitch", args, |_| true);
    engine.emit("Twitch", ev, || json!(args));
}

/// The event's `user_*` (or `{who}_user_*`) as the user arguments.
fn event_user(args: &mut Obj, e: &Value, who: &str) {
    // The user is `user_id`; the others are `target_user_id`, `moderator_user_id`...
    let pre = if who == "user" {
        String::new()
    } else {
        format!("{who}_")
    };
    let f = |k: &str| es(e, &format!("/{pre}user_{k}")).to_string();
    populate_user_vars(args, &f("id"), &f("login"), &f("name"), 1, false);
}

/// The event's `{who}_user_*` as `{prefix}Id`, `{prefix}Username`, `{prefix}DisplayName` (createdBy,
/// moderator...).
fn event_by(args: &mut Obj, e: &Value, who: &str, prefix: &str) {
    for (k, f) in [("Id", "id"), ("Username", "login"), ("DisplayName", "name")] {
        args.insert(format!("{prefix}{k}"), es(e, &format!("/{who}_user_{f}")).into());
    }
}

/// The moderator as Streamer.bot's `moderator.userName/login/id`.
fn event_moderator(args: &mut Obj, e: &Value) {
    for (k, f) in [("userName", "name"), ("login", "login"), ("id", "id")] {
        args.insert(format!("moderator.{k}"), es(e, &format!("/moderator_user_{f}")).into());
    }
}

/// "tier 1", "tier 2", "tier 3" or "prime".
fn tier_name(tier: &str, prime: bool) -> &'static str {
    match tier {
        _ if prime => "prime",
        "2000" => "tier 2",
        "3000" => "tier 3",
        _ => "tier 1",
    }
}

/// A Twitch money amount (`value` in minor units, `decimal_places`) as `{key}.value/.valueMinor/.currency`.
fn money(args: &mut Obj, key: &str, m: &Value) {
    let minor = m["value"].as_i64().unwrap_or(0);
    let places = m["decimal_places"].as_i64().unwrap_or(2) as i32;
    args.insert(format!("{key}.value"), (minor as f64 / 10f64.powi(places)).into());
    args.insert(format!("{key}.valueMinor"), minor.into());
    args.insert(format!("{key}.currency"), m["currency"].clone());
}

/// A charity campaign event as `charity.*`.
fn charity_args(args: &mut Obj, e: &Value) {
    for (k, f) in [
        ("id", "id"),
        ("campaignId", "campaign_id"),
        ("name", "charity_name"),
        ("description", "charity_description"),
        ("logo", "charity_logo"),
        ("website", "charity_website"),
        ("startedAt", "started_at"),
        ("stoppedAt", "stopped_at"),
    ] {
        if let Some(v) = e.get(f) {
            args.insert(format!("charity.{k}"), v.clone());
        }
    }
    for (k, f) in [
        ("currentAmount", "current_amount"),
        ("targetAmount", "target_amount"),
        ("amount", "amount"),
    ] {
        if e.get(f).is_some() {
            money(args, &format!("charity.{k}"), &e[f]);
        }
    }
}

/// A hype train (EventSub v2) as Streamer.bot's arguments.
fn hype_args(args: &mut Obj, e: &Value) {
    let (level, total, progress, goal) = (
        e["level"].as_u64().unwrap_or(1),
        e["total"].as_u64().unwrap_or(0),
        e["progress"].as_u64().unwrap_or(0),
        e["goal"].as_u64().unwrap_or(0),
    );
    let pct = if goal > 0 { progress as f64 / goal as f64 } else { 0.0 };
    let kind = es(e, "/type");
    for (k, v) in [
        ("id", e["id"].clone()),
        ("level", level.into()),
        ("total", total.into()),
        ("progress", progress.into()),
        ("goal", goal.into()),
        ("percent", format!("{}%", (pct * 100.0).round()).into()),
        ("percentDecimal", ((pct * 100.0).round() / 100.0).into()),
        ("startedAt", e["started_at"].clone()),
        ("trainType", kind.into()),
        ("isGoldenKappaTrain", (kind == "golden_kappa").into()),
        ("isTreasureTrain", (kind == "treasure").into()),
        ("isSharedTrain", e["is_shared_train"].as_bool().unwrap_or(false).into()),
        ("isInSharedChat", e["is_shared_train"].as_bool().unwrap_or(false).into()),
        ("isSharedChatHost", false.into()),
    ] {
        args.insert(k.into(), v);
    }
    for (k, f) in [
        ("expiresAt", "expires_at"),
        ("endedAt", "ended_at"),
        ("cooldownEndsAt", "cooldown_ends_at"),
    ] {
        if let Some(v) = e.get(f) {
            args.insert(k.into(), v.clone());
        }
    }
    if let (Some(a), Some(b)) = (e["started_at"].as_str(), e["expires_at"].as_str()) {
        if let (Some(a), Some(b)) = (crate::engine::rest_parse_iso(a), crate::engine::rest_parse_iso(b)) {
            args.insert("duration".into(), (b - a).into());
        }
    }
    for (k, f) in [
        ("allTimeHighLevel", "all_time_high_level"),
        ("allTimeHighTotal", "all_time_high_total"),
    ] {
        if let Some(v) = e.get(f) {
            args.insert(k.into(), v.clone());
        }
    }
    for c in e["top_contributions"].as_array().into_iter().flatten() {
        let t = format!("top.{}", c["type"].as_str().unwrap_or("other"));
        for (k, f) in [
            ("user", "user_name"),
            ("userName", "user_login"),
            ("userId", "user_id"),
            ("total", "total"),
        ] {
            args.insert(format!("{t}.{k}"), c[f].clone());
        }
    }
}

/// Count `n` subs on the sub counter; at the rollover it starts again and Sub Counter Rollover fires.
fn count_subs(engine: &Rc<Engine>, cfg: &Arc<watch::Sender<Config>>, broadcaster: &TwitchAccount, n: u64) {
    let rollover = cfg.borrow().twitch.sub_counter_rollover;
    let get = |c: &Config, k: &str| c.counters.get(k).and_then(Value::as_u64).unwrap_or(0);
    let mut hit = None;
    engine.change(|c| {
        let (mut count, mut times) = (get(c, "twitch:subCounter") + n, get(c, "twitch:subCounterRollovers"));
        if rollover > 0 && count >= rollover {
            count %= rollover;
            times += 1;
            hit = Some((count, times));
        }
        c.counters.insert("twitch:subCounter".into(), count.into());
        c.counters.insert("twitch:subCounterRollovers".into(), times.into());
    });
    if let Some((count, times)) = hit {
        let mut args = Obj::new();
        populate_broadcaster_vars(&mut args, broadcaster);
        args.insert("subCounter".into(), count.into());
        args.insert("rollover".into(), rollover.into());
        args.insert("rolloverCount".into(), times.into());
        go(
            engine,
            T_SUB_COUNTER_ROLLOVER,
            "Sub Counter Rollover",
            "Twitch/Subscriptions",
            "SubCounterRollover",
            &args,
        );
    }
}

/// What the event stream remembers between events: the hype train's level, gift bombs (for each
/// gift's subBombCount), the chat settings, title and game (for the Changed triggers and old*
/// arguments), the custom rewards' settings, recent messages (for Chat Message Deleted) and the
/// pyramid being built.
#[derive(Default)]
struct Seen {
    hype_level: u64,
    bombs: HashMap<String, u64>,
    settings: Option<Value>,
    title: String,
    game: (String, String),
    rewards: HashMap<String, Value>,
    messages: std::collections::VecDeque<(String, String)>,
    pyramid: Pyramid,
    pyramids: HashMap<String, (u64, u64)>,
    raid: Option<Obj>,
    pending: HashMap<String, Bomb>,
    /// Badge pictures by "set/version" (the channel's over Twitch's global ones).
    badges: HashMap<String, String>,
    /// Their names ("Moderator", "1-Year Subscriber"), by "set/version".
    badge_titles: HashMap<String, String>,
}

/// A gift bomb waiting for its gifts: trigger, arguments, how many, recipients so far.
type Bomb = (u32, Obj, u64, Vec<[Value; 3]>);

/// Fire a waiting gift bomb (if it's still waiting) with its recipients as `gift.recipient*#`.
fn gift_bomb(engine: &Rc<Engine>, id: &str) {
    let Some((code, mut args, _, who)) = SEEN.with_borrow_mut(|s| s.pending.remove(id)) else {
        return;
    };
    for (i, [name, login, uid]) in who.into_iter().enumerate() {
        args.insert(format!("gift.recipientUser{i}"), name);
        args.insert(format!("gift.recipientUserName{i}"), login);
        args.insert(format!("gift.recipientId{i}"), uid);
    }
    go(engine, code, "Gift Bomb", "Twitch/Subscriptions", "GiftBomb", &args);
}

/// A Helix user as the raid triggers' raidUser* arguments.
fn raid_target(u: &Value) -> Obj {
    let pic = u["profile_image_url"].as_str().unwrap_or("");
    let mut a = Obj::new();
    for (k, v) in [
        ("raidUser", u["display_name"].clone()),
        ("raidUserName", u["login"].clone()),
        ("raidUserId", u["id"].clone()),
        ("raidUserProfileImageURL", pic.into()),
        ("raidUserProfileImageEscaped", commands::escape(pic).into()),
    ] {
        a.insert(k.into(), v);
    }
    a
}

#[derive(Default)]
struct Pyramid {
    owner: Who,
    word: String,
    at: usize,
    top: usize,
    falling: bool,
}

thread_local!(static SEEN: RefCell<Seen> = RefCell::default());

/// The channel's category (id, name), as the last channel update said.
pub fn current_game() -> (String, String) {
    SEEN.with_borrow(|s| s.game.clone())
}

/// A user's id, login and display name.
type Who = (String, String, String);

/// Pyramids: one user sends "X", "X X", "X X X"... then back down to "X" (at least 3 wide). Another
/// user's message in the middle breaks it. What happened, if anything: (success, owner, word, width).
fn pyramid(user: (&str, &str, &str), text: &str) -> Option<(bool, Who, String, usize)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let same = !words.is_empty() && words.iter().all(|w| *w == words[0]);
    SEEN.with_borrow_mut(|s| {
        let p = &mut s.pyramid;
        let mine = p.at > 0 && p.owner.0 == user.0;
        let mut out = None;
        if p.at > 0 && !mine {
            if p.top >= 2 {
                out = Some((false, p.owner.clone(), p.word.clone(), p.top));
            }
            *p = Pyramid::default();
        }
        let n = words.len();
        if p.at > 0 && same && words[0] == p.word {
            if !p.falling && n == p.at + 1 {
                p.at = n;
                p.top = n;
                return out;
            }
            if n + 1 == p.at && p.top >= 2 {
                p.falling = true;
                p.at = n;
                if n == 1 {
                    let done = (p.top >= 3).then(|| (true, p.owner.clone(), p.word.clone(), p.top));
                    *p = Pyramid::default();
                    return done.or(out);
                }
                return out;
            }
        }
        *p = Pyramid::default();
        if same && n == 1 {
            *p = Pyramid {
                owner: (user.0.into(), user.1.into(), user.2.into()),
                word: words[0].into(),
                at: 1,
                top: 1,
                falling: false,
            };
        }
        out
    })
}

/// A badge's name ("1-Year Subscriber") by its "set/version", once the badge sets are read.
pub fn badge_title(key: &str) -> Option<String> {
    SEEN.with_borrow(|s| s.badge_titles.get(key).cloned())
}

/// What twitch.tv's viewer card shows of a user, read without a login: their profile banner (the
/// picture behind the card, which the Helix API doesn't have), picture and account age. From Twitch's
/// public web API, asked anonymously with its web client id, as twitch.tv does.
pub async fn public_profile(user_id: &str) -> Option<Value> {
    if !user_id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let headers = [
        ("Client-Id".to_string(), "kimne78kx3ncx6brgo4mv6wki5h1ko".to_string()),
        ("Content-Type".to_string(), "application/json".to_string()),
    ];
    let q = json!({
        "query": format!("{{ user(id: \"{user_id}\") {{ bannerImageURL createdAt profileImageURL(width: 150) }} }}")
    });
    let r = crate::http::request("POST", "https://gql.twitch.tv/gql", &headers, q.to_string().as_bytes())
        .await
        .ok()?;
    let v: Value = serde_json::from_slice(&r.body).ok()?;
    Some(v["data"]["user"].clone()).filter(Value::is_object)
}

/// Remember what later events compare against (chat settings, title and game, the custom rewards).
async fn seed(engine: &Rc<Engine>) {
    let Some(tw) = engine.twitch() else { return };
    let b = vec![("broadcaster_id", "{broadcaster_id}")];
    let settings = tw.helix("GET", "chat/settings", b.clone(), None, false, false).await;
    let channel = tw.helix("GET", "channels", b.clone(), None, false, false).await;
    let rewards = tw
        .helix("GET", "channel_points/custom_rewards", b.clone(), None, false, false)
        .await;
    let global = tw.helix("GET", "chat/badges/global", vec![], None, false, false).await;
    let own = tw.helix("GET", "chat/badges", b, None, false, false).await;
    SEEN.with_borrow_mut(|s| {
        for sets in [global, own].into_iter().flatten() {
            for set in sets["data"].as_array().into_iter().flatten() {
                for v in set["versions"].as_array().into_iter().flatten() {
                    // 72 px: sharp once shrunk, on any screen scale.
                    let url = v["image_url_4x"].as_str().or(v["image_url_1x"].as_str()).unwrap_or("");
                    let key = format!(
                        "{}/{}",
                        set["set_id"].as_str().unwrap_or(""),
                        v["id"].as_str().unwrap_or("")
                    );
                    if let Some(t) = v["title"].as_str().filter(|t| !t.is_empty()) {
                        s.badge_titles.insert(key.clone(), t.into());
                    }
                    s.badges.insert(key, url.into());
                }
            }
        }
        if let Ok(v) = settings {
            s.settings = Some(v["data"][0].clone());
        }
        if let Ok(v) = channel {
            let c = &v["data"][0];
            s.title = c["title"].as_str().unwrap_or("").into();
            s.game = (
                c["game_id"].as_str().unwrap_or("").into(),
                c["game_name"].as_str().unwrap_or("").into(),
            );
        }
        if let Ok(v) = rewards {
            for r in v["data"].as_array().into_iter().flatten() {
                s.rewards.insert(r["id"].as_str().unwrap_or("").into(), r.clone());
            }
        }
    });
}

/// While connected: viewer count (every minute), present viewers (every 5) and the next ad (every
/// minute, once 5 minutes before it), each only when an action uses its trigger and the stream is live.
async fn poll(engine: Rc<Engine>, broadcaster: TwitchAccount) {
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    let (mut viewers, mut warned, mut n) = (None, String::new(), 0u64);
    loop {
        tick.tick().await;
        n += 1;
        let want = [T_VIEWER_COUNT_UPDATE, T_PRESENT_VIEWERS, T_UPCOMING_AD].map(|t| engine.uses_trigger(t));
        let Some(tw) = engine.twitch().filter(|_| want.iter().any(|w| *w)) else {
            continue;
        };
        let id = broadcaster.user_id.as_str();
        let Ok(s) = tw
            .helix("GET", "streams", vec![("user_id", id)], None, false, false)
            .await
        else {
            continue;
        };
        let live = s["data"][0]["type"] == "live";
        let mut args = Obj::new();
        populate_broadcaster_vars(&mut args, &broadcaster);
        if want[0] && live {
            let count = s["data"][0]["viewer_count"].as_u64().unwrap_or(0);
            if viewers != Some(count) {
                viewers = Some(count);
                let mut a = args.clone();
                a.insert("viewerCount".into(), count.into());
                go(
                    &engine,
                    T_VIEWER_COUNT_UPDATE,
                    "Viewer Count Update",
                    "Twitch/General",
                    "ViewerCountUpdate",
                    &a,
                );
            }
        }
        if want[1] && live && n % 5 == 1 {
            let q = vec![("broadcaster_id", id), ("moderator_id", id), ("first", "1000")];
            if let Ok(c) = tw.helix("GET", "chat/chatters", q, None, false, false).await {
                let users: Vec<Value> = c["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|u| json!({"id": u["user_id"], "login": u["user_login"], "display": u["user_name"]}))
                    .collect();
                let mut a = args.clone();
                a.insert("users".into(), users.into());
                a.insert("isLive".into(), live.into());
                a.insert("isTest".into(), false.into());
                go(
                    &engine,
                    T_PRESENT_VIEWERS,
                    "Present Viewers",
                    "Twitch/General",
                    "PresentViewers",
                    &a,
                );
            }
        }
        if want[2] && live {
            if let Ok(ad) = tw
                .helix("GET", "channels/ads", vec![("broadcaster_id", id)], None, false, false)
                .await
            {
                let d = &ad["data"][0];
                let next = d["next_ad_at"].as_str().unwrap_or("");
                let at = crate::engine::rest_parse_iso(next).unwrap_or(0);
                let left = (at - crate::fmt::unix_now().as_secs() as i64) / 60;
                if !next.is_empty() && (0..=5).contains(&left) && warned != next {
                    warned = next.to_string();
                    let mut a = args.clone();
                    a.insert("minutes".into(), left.into());
                    a.insert("nextAdAt".into(), next.into());
                    a.insert("snoozesLeft".into(), d["snooze_count"].clone());
                    a.insert("adLength".into(), d["duration"].clone());
                    go(&engine, T_UPCOMING_AD, "Upcoming Ad", "Twitch/Ads", "UpcomingAd", &a);
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[test]
    fn recent_messages_of_one_word_have_no_colon() {
        let one = "@display-name=Ana;user-id=7;tmi-sent-ts=1793151375247 :ana!ana@ana.tmi.twitch.tv PRIVMSG #pedro opa";
        assert_eq!(parse_irc_recent_message(one, "1").unwrap().text, "opa");
        let many = "@display-name=Ana;user-id=7 :ana!ana@ana.tmi.twitch.tv PRIVMSG #pedro :two words";
        assert_eq!(parse_irc_recent_message(many, "1").unwrap().text, "two words");
        let gone = "@display-name=Ana;rm-deleted=1 :ana!ana@ana.tmi.twitch.tv PRIVMSG #pedro :bye";
        assert!(parse_irc_recent_message(gone, "1").is_none());
    }
    use super::*;

    #[test]
    fn events_fire_once_with_their_variables() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap();
        let watch: &[(u32, &str)] = &[
            (T_SUB, "%tier% %isMultiMonth%"),
            (T_SHARED_CHAT_SUB, "%sourceBroadcasterUsername%"),
            (T_CHAT_MODE_EMOTE_ON, "%emoteMode%"),
            (T_CHAT_MODE_SLOW_ON, "%slowModeWaitTime%"),
            (T_CHAT_MODE_SLOW_CHANGED, "%slowModeWaitTime%"),
            (
                T_PYRAMID_SUCCESS,
                "%pyramidWidth% %pyramidEmote% %user% %totalPyramids%",
            ),
            (T_PYRAMID_BROKEN, "%pyramidOwnerUsername% %userName%"),
            (T_HYPE_TRAIN_LEVEL_UP, "%prevLevel% %level% %percent% %top.bits.user%"),
            (T_BLOCKED_TERMS_ADDED, "%termCount% %term.[0]%"),
            (T_USER_BANNED, "%createdByUsername%"),
            (T_STREAM_UPDATE, "%oldStatus%>%status% %statusUpdate% %gameUpdate%"),
            (T_SUB_COUNTER_ROLLOVER, "%subCounter% %rollover% %rolloverCount%"),
            (T_GIFT_BOMB, "%gifts% %gift.recipientUser0% %gift.recipientUser1%"),
            (T_GIFT_SUB, "%recipientUser% %subBombCount% %fromGiftBomb%"),
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
        let mut c = Config::default();
        c.twitch.sub_counter_rollover = 2;
        let cfg = Arc::new(watch::channel(c).0);
        let b = TwitchAccount {
            user_id: "1".into(),
            login: "me".into(),
            ..Default::default()
        };
        let out = run(async move {
            let go = |kind: &str, v: Value| handle_notification(&e, &b, &cfg, kind, &v);
            let note = |t: &str, d: Value| json!({"notice_type": t, "chatter_user_id": "5", "chatter_user_login": "ann", t: d});
            go(
                "channel.chat.notification",
                note("sub", json!({"sub_tier": "2000", "duration_months": 3})),
            );
            let mut shared = note("shared_chat_sub", json!({"sub_tier": "1000"}));
            shared["source_broadcaster_user_login"] = "other".into();
            go("channel.chat.notification", shared);
            go(
                "channel.chat.notification",
                note("resub", json!({"sub_tier": "1000", "cumulative_months": 4})),
            );
            let settings =
                |slow: u64| json!({"emote_mode": true, "slow_mode": slow > 0, "slow_mode_wait_time_seconds": slow});
            go(
                "channel.chat.notification",
                note("community_sub_gift", json!({"id": "b1", "total": 2})),
            );
            for r in ["R1", "R2"] {
                let gift = json!({"recipient_user_name": r, "community_gift_id": "b1"});
                go("channel.chat.notification", note("sub_gift", gift));
            }
            go("channel.chat_settings.update", settings(0));
            go("channel.chat_settings.update", settings(30));
            go("channel.chat_settings.update", settings(60));
            go("channel.chat_settings.update", settings(60));
            let say = |id: &str, text: &str| {
                json!({"chatter_user_id": id, "chatter_user_login": format!("u{id}"), "chatter_user_name": id,
                    "message_id": "m", "message": {"text": text}})
            };
            for t in ["Kappa", "Kappa Kappa", "Kappa Kappa Kappa", "Kappa Kappa", "Kappa"] {
                go("channel.chat.message", say("7", t));
            }
            go("channel.chat.message", say("7", "LUL"));
            go("channel.chat.message", say("7", "LUL LUL"));
            go("channel.chat.message", say("8", "nope"));
            let train = |level: u64| {
                json!({"level": level, "progress": 40, "goal": 100, "type": "regular",
                    "top_contributions": [{"type": "bits", "user_name": "Bitsy", "user_login": "bitsy", "user_id": "9", "total": 500}]})
            };
            go("channel.hype_train.begin", train(1));
            go("channel.hype_train.progress", train(1));
            go("channel.hype_train.progress", train(2));
            go(
                "automod.terms.update",
                json!({"action": "add_blocked", "terms": ["badword"], "moderator_user_login": "mod"}),
            );
            go(
                "channel.ban",
                json!({"user_id": "4", "is_permanent": true, "moderator_user_login": "boss"}),
            );
            SEEN.with_borrow_mut(|s| s.title = "old".into());
            go("channel.update", json!({"title": "new", "category_id": ""}));
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        let want = [
            "T 103 tier 2 True",
            "T 486 other",
            "T 451 True",
            "T 456 30",
            "T 458 30",
            "T 458 60",
            "T 162 3 Kappa 7 1",
            "T 163 u7 u8",
            "T 110 1 2 40% Bitsy",
            "T 465 1 badword",
            "T 137 boss",
            "T 118 old>new True False",
            "T 121 0 2 1",
            "T 121 0 2 2",
            "T 106 2 R1 R2",
            "T 105 R1 2 True",
            "T 105 R2 2 True",
        ];
        for w in want {
            assert_eq!(out.iter().filter(|x| *x == w).count(), 1, "{w} once: {out:?}");
        }
        assert_eq!(out.len(), want.len(), "nothing else: {out:?}");
    }

    #[test]
    fn refresh_timing() {
        let t = |rt: &str, exp: u64| TwitchToken {
            refresh_token: rt.into(),
            expires_at: exp,
            ..Default::default()
        };
        assert!(!needs_refresh(&t("", 0), 1000));
        assert!(needs_refresh(&t("r", 0), 1000));
        assert!(!needs_refresh(&t("r", 2000), 1000));
        assert!(needs_refresh(&t("r", 1200), 1000));
    }

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

    #[test]
    fn whisper_and_bot_whisper_notifications() {
        let mut cfg_data = Config::default();
        cfg_data.twitch.bot = Some(TwitchAccount {
            user_id: "999".into(),
            login: "botuser".into(),
            display_name: "BotUser".into(),
            ..Default::default()
        });
        let (tx, _rx) = watch::channel(cfg_data);
        let cfg = Arc::new(tx);
        let engine = Engine::new(cfg.clone(), |_| {});

        let broadcaster = TwitchAccount {
            user_id: "123".into(),
            login: "streamer".into(),
            display_name: "Streamer".into(),
            ..Default::default()
        };

        // Whisper to broadcaster
        let event = json!({
            "from_user_id": "456",
            "from_user_login": "chatter",
            "from_user_name": "Chatter",
            "to_user_id": "123",
            "whisper": { "text": "hello broadcaster" }
        });
        handle_notification(&engine, &broadcaster, &cfg, "user.whisper.message", &event);

        // Bot whisper to bot
        let bot_event = json!({
            "from_user_id": "456",
            "from_user_login": "chatter",
            "from_user_name": "Chatter",
            "to_user_id": "999",
            "whisper": { "text": "hello bot" }
        });
        handle_notification(&engine, &broadcaster, &cfg, "user.whisper.message", &bot_event);
    }

    #[test]
    fn raid_send_notification() {
        use crate::engine::tests::{action, engine, log, run};
        let _g = crate::TEST_LOCK.lock().unwrap();
        let mut a = action("a", "a", json!([log("T send %raidUserName% %viewers%")]));
        a["triggers"] = json!([{"id": "t", "type": T_RAID_SEND, "enabled": true}]);
        let e = engine(json!({ "actions": [a] }));
        let cfg = Arc::new(watch::channel(Config::default()).0);
        let b = TwitchAccount {
            user_id: "123".into(),
            ..Default::default()
        };
        let out = run(async move {
            let raid = |from: &str| {
                json!({"from_broadcaster_user_id": from, "to_broadcaster_user_id": "789",
                    "to_broadcaster_user_login": "target", "viewers": 42})
            };
            // Outgoing: Raid Send. Incoming ones come as chat notifications, so this one does nothing.
            handle_notification(&e, &b, &cfg, "channel.raid", &raid("123"));
            handle_notification(&e, &b, &cfg, "channel.raid", &raid("456"));
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        assert_eq!(out, ["T send target 42"]);
    }

    #[test]
    fn reward_redemption_notifications() {
        let (tx, _rx) = watch::channel(Config::default());
        let cfg = Arc::new(tx);
        let engine = Engine::new(cfg.clone(), |_| {});

        let broadcaster = TwitchAccount {
            user_id: "123".into(),
            login: "streamer".into(),
            display_name: "Streamer".into(),
            ..Default::default()
        };

        let redemption = json!({
            "id": "red-1",
            "user_id": "456",
            "user_login": "viewer",
            "user_name": "Viewer",
            "user_input": "hydrate!",
            "status": "unfulfilled",
            "reward": {
                "id": "rew-1",
                "title": "Drink Water",
                "cost": 500,
                "prompt": "Stay hydrated"
            }
        });
        handle_notification(
            &engine,
            &broadcaster,
            &cfg,
            "channel.channel_points_custom_reward_redemption.add",
            &redemption,
        );

        let updated = json!({
            "id": "red-1",
            "user_id": "456",
            "user_login": "viewer",
            "user_name": "Viewer",
            "user_input": "hydrate!",
            "status": "fulfilled",
            "reward": {
                "id": "rew-1",
                "title": "Drink Water",
                "cost": 500,
                "prompt": "Stay hydrated"
            }
        });
        handle_notification(
            &engine,
            &broadcaster,
            &cfg,
            "channel.channel_points_custom_reward_redemption.update",
            &updated,
        );
    }

    #[test]
    fn poll_and_prediction_notifications() {
        let (tx, _rx) = watch::channel(Config::default());
        let cfg = Arc::new(tx);
        let engine = Engine::new(cfg.clone(), |_| {});

        let broadcaster = TwitchAccount {
            user_id: "123".into(),
            login: "streamer".into(),
            display_name: "Streamer".into(),
            ..Default::default()
        };

        let poll = json!({
            "id": "poll-1",
            "title": "Favorite language?",
            "choices": [{"id": "c1", "title": "Rust", "votes": 10}, {"id": "c2", "title": "Other", "votes": 2}],
            "status": "completed"
        });
        handle_notification(&engine, &broadcaster, &cfg, "channel.poll.begin", &poll);
        handle_notification(&engine, &broadcaster, &cfg, "channel.poll.end", &poll);

        let pred = json!({
            "id": "pred-1",
            "title": "Will we win?",
            "winning_outcome_id": "o1",
            "outcomes": [{"id": "o1", "title": "Yes", "users": 5, "channel_points": 100}],
            "status": "resolved"
        });
        handle_notification(&engine, &broadcaster, &cfg, "channel.prediction.begin", &pred);
        handle_notification(&engine, &broadcaster, &cfg, "channel.prediction.end", &pred);
    }

    #[test]
    fn moderation_and_chat_modes_notifications() {
        let (tx, _rx) = watch::channel(Config::default());
        let cfg = Arc::new(tx);
        let engine = Engine::new(cfg.clone(), |_| {});

        let broadcaster = TwitchAccount {
            user_id: "123".into(),
            login: "streamer".into(),
            display_name: "Streamer".into(),
            ..Default::default()
        };

        // Ban
        let ban = json!({
            "user_id": "456",
            "user_login": "spammer",
            "user_name": "Spammer",
            "reason": "spamming",
            "is_permanent": true
        });
        handle_notification(&engine, &broadcaster, &cfg, "channel.ban", &ban);

        // Timeout
        let to = json!({
            "user_id": "456",
            "user_login": "spammer",
            "user_name": "Spammer",
            "reason": "caps",
            "is_permanent": false,
            "ends_at": "2026-10-12T22:00:00Z"
        });
        handle_notification(&engine, &broadcaster, &cfg, "channel.ban", &to);

        // Unban
        let unban = json!({
            "user_id": "456",
            "user_login": "spammer",
            "user_name": "Spammer"
        });
        handle_notification(&engine, &broadcaster, &cfg, "channel.unban", &unban);

        // Mod add / remove
        let mod_event = json!({
            "user_id": "789",
            "user_login": "moduser",
            "user_name": "ModUser"
        });
        handle_notification(&engine, &broadcaster, &cfg, "channel.moderator.add", &mod_event);
        handle_notification(&engine, &broadcaster, &cfg, "channel.moderator.remove", &mod_event);

        // VIP add / remove
        handle_notification(&engine, &broadcaster, &cfg, "channel.vip.add", &mod_event);
        handle_notification(&engine, &broadcaster, &cfg, "channel.vip.remove", &mod_event);

        // Shield mode
        handle_notification(&engine, &broadcaster, &cfg, "channel.shield_mode.begin", &json!({}));
        handle_notification(&engine, &broadcaster, &cfg, "channel.shield_mode.end", &json!({}));

        // Shoutout
        let shoutout = json!({
            "to_broadcaster_user_id": "999",
            "to_broadcaster_user_login": "friend",
            "to_broadcaster_user_name": "Friend"
        });
        handle_notification(&engine, &broadcaster, &cfg, "channel.shoutout.create", &shoutout);

        // Chat settings
        let settings = json!({
            "emote_mode": true,
            "subscriber_mode": true,
            "slow_mode": false,
            "follower_mode": false,
            "unique_chat_mode": true
        });
        handle_notification(&engine, &broadcaster, &cfg, "channel.chat_settings.update", &settings);
    }

    #[test]
    fn charity_and_guest_star_notifications() {
        let (tx, _rx) = watch::channel(Config::default());
        let cfg = Arc::new(tx);
        let engine = Engine::new(cfg.clone(), |_| {});

        let broadcaster = TwitchAccount {
            user_id: "123".into(),
            login: "streamer".into(),
            display_name: "Streamer".into(),
            ..Default::default()
        };

        // Charity
        let donation = json!({
            "user_id": "456",
            "user_login": "donor",
            "user_name": "Donor",
            "amount": { "value": 1000 }
        });
        handle_notification(
            &engine,
            &broadcaster,
            &cfg,
            "channel.charity_campaign.donate",
            &donation,
        );

        let charity_start = json!({ "charity_name": "Save the Whales" });
        handle_notification(
            &engine,
            &broadcaster,
            &cfg,
            "channel.charity_campaign.start",
            &charity_start,
        );
        handle_notification(&engine, &broadcaster, &cfg, "channel.charity_campaign.stop", &json!({}));

        // Guest star
        handle_notification(
            &engine,
            &broadcaster,
            &cfg,
            "channel.guest_star_session.begin",
            &json!({}),
        );
        handle_notification(
            &engine,
            &broadcaster,
            &cfg,
            "channel.guest_star_session.end",
            &json!({}),
        );

        // Shared chat
        handle_notification(&engine, &broadcaster, &cfg, "channel.shared_chat.begin", &json!({}));
        handle_notification(&engine, &broadcaster, &cfg, "channel.shared_chat.end", &json!({}));
    }

    #[test]
    fn default_palette_and_irc_history_parser() {
        let c1 = default_user_color("PedroZamecki");
        let c2 = default_user_color("pedrozamecki");
        assert_eq!(c1, c2);
        assert_ne!(c1, "#FFFFFF");
        assert!(DEFAULT_COLORS.contains(&c1));

        let raw = "@badge-info=;badges=broadcaster/1,subscriber/12;color=#00FF7F;display-name=PedroZamecki;emotes=25:4-8;id=abc-123;tmi-sent-ts=1700000000000;user-id=42 :pedrozamecki!pedrozamecki@pedrozamecki.tmi.twitch.tv PRIVMSG #pedrozamecki :hey Kappa";
        let line = parse_irc_recent_message(raw, "42").expect("valid PRIVMSG");
        assert_eq!(line.msg_id, "abc-123");
        assert_eq!(line.user, "PedroZamecki");
        assert_eq!(line.user_id, "42");
        assert_eq!(line.color, "#00FF7F");
        assert_eq!(line.text, "hey Kappa");
        assert!(line.broadcaster && line.subscriber && line.historical);
        assert_eq!(line.badges[0].0, "broadcaster/1");
        assert_eq!(line.emotes.len(), 1);
        assert_eq!(line.emotes[0].0, "Kappa");
    }
}
