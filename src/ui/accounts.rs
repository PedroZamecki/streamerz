//! The Accounts tab: one row per platform (status, channel, Connect, broadcaster and bot logins),
//! with the rarely needed settings behind Advanced.

use super::fit::Ellipsis;
use super::lists::save;
use super::{button, dialogs, query, tell, ROW};
use crate::engine::community::Leftover;
use crate::engine::Engine;
use crate::i18n::{button_id, tr, trf};
use crate::model::Obj;
use crate::Config;
use fltk::button::{Button, CheckButton};
use fltk::enums::Align;
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
use fltk::input::Input;
use fltk::prelude::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc::UnboundedSender, watch};

type Cfg = Arc<watch::Sender<Config>>;
type Cmds = UnboundedSender<crate::engine::UiCmd>;

#[derive(Clone, Copy, PartialEq)]
enum P {
    Twitch,
    YouTube,
    Kick,
    TikTok,
}

const ALL: [P; 4] = [P::Twitch, P::YouTube, P::Kick, P::TikTok];

/// A Twitch channel from what was typed: its login, without a URL or @.
fn clean_twitch_channel(raw: &str) -> String {
    let s = raw.trim();
    let s = s.rsplit("twitch.tv/").next().unwrap_or(s);
    s.trim_start_matches('@').trim_end_matches('/').trim().to_lowercase()
}

impl P {
    fn from_slug(s: &str) -> Option<Self> {
        match s {
            "twitch" => Some(P::Twitch),
            "youtube" => Some(P::YouTube),
            "kick" => Some(P::Kick),
            "tiktok" => Some(P::TikTok),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            P::Twitch => "Twitch",
            P::YouTube => "YouTube",
            P::Kick => "Kick",
            P::TikTok => "TikTok",
        }
    }

    /// Raw display name of the logged-in broadcaster or bot (without status suffixes).
    fn raw_account_name(self, c: &Config, bot: bool) -> Option<String> {
        match self {
            P::Twitch => if bot { &c.twitch.bot } else { &c.twitch.broadcaster }
                .as_ref()
                .map(|a| a.display_name.clone()),
            P::YouTube => if bot { &c.youtube.bot } else { &c.youtube.broadcaster }
                .as_ref()
                .map(|a| a.name.clone()),
            P::Kick => if bot { &c.kick.bot } else { &c.kick.broadcaster }
                .as_ref()
                .map(|a| a.username.clone()),
            P::TikTok => None,
        }
        .filter(|s| !s.is_empty())
    }

    /// Optional status warning on the account (locked keyring, expired token, or new scopes).
    fn account_note(self, c: &Config, bot: bool) -> Option<&'static str> {
        let locked = crate::secrets::is_locked();
        match self {
            P::Twitch => {
                let a = if bot { &c.twitch.bot } else { &c.twitch.broadcaster }.as_ref()?;
                let want = if bot {
                    crate::twitch::BOT_SCOPES
                } else {
                    crate::twitch::BROADCASTER_SCOPES
                };
                if a.token.access_token.is_empty() {
                    Some(if locked {
                        tr("accounts.note.keyring_locked")
                    } else {
                        tr("accounts.note.login_expired")
                    })
                } else if want.iter().any(|w| !a.token.scope.iter().any(|g| g == w)) {
                    Some(tr("accounts.note.new_permissions"))
                } else {
                    None
                }
            }
            P::YouTube => {
                let a = if bot { &c.youtube.bot } else { &c.youtube.broadcaster }.as_ref()?;
                (a.token.access_token.is_empty() && a.token.refresh_token.is_empty() && locked)
                    .then_some(tr("accounts.note.keyring_locked"))
            }
            P::Kick => {
                let a = if bot { &c.kick.bot } else { &c.kick.broadcaster }.as_ref()?;
                (a.token.access_token.is_empty() && a.token.refresh_token.is_empty() && locked)
                    .then_some(tr("accounts.note.keyring_locked"))
            }
            P::TikTok => None,
        }
    }

    /// The logged-in broadcaster's or bot's name (with any warning note).
    fn account(self, c: &Config, bot: bool) -> Option<String> {
        let name = self.raw_account_name(c, bot)?;
        match self.account_note(c, bot) {
            Some(note) => Some(format!("{name} ({note})")),
            None => Some(name),
        }
    }

    /// Forget an account; its revoke URL and form (Kick and Google need the token in the URL).
    fn logout(self, c: &mut Config, bot: bool) -> Option<String> {
        let e = crate::commands::url_encode;
        let revoke = match self {
            P::Twitch => if bot { &c.twitch.bot } else { &c.twitch.broadcaster }
                .as_ref()
                .map(|a| {
                    format!(
                        "https://id.twitch.tv/oauth2/revoke?client_id={}&token={}",
                        crate::twitch::CLIENT_ID,
                        e(&a.token.access_token)
                    )
                }),
            P::YouTube => if bot { &c.youtube.bot } else { &c.youtube.broadcaster }
                .as_ref()
                .map(|a| {
                    // Revoking the refresh token ends the whole grant.
                    let t = if a.token.refresh_token.is_empty() {
                        &a.token.access_token
                    } else {
                        &a.token.refresh_token
                    };
                    format!("https://oauth2.googleapis.com/revoke?token={}", e(t))
                }),
            P::Kick => if bot { &c.kick.bot } else { &c.kick.broadcaster }.as_ref().map(|a| {
                format!(
                    "https://id.kick.com/oauth/revoke?token={}&token_hint_type=access_token",
                    e(&a.token.access_token)
                )
            }),
            P::TikTok => None,
        };
        match (self, bot) {
            (P::Twitch, false) => c.twitch.broadcaster = None,
            (P::Twitch, true) => c.twitch.bot = None,
            (P::YouTube, false) => c.youtube.broadcaster = None,
            (P::YouTube, true) => c.youtube.bot = None,
            (P::Kick, false) => c.kick.broadcaster = None,
            (P::Kick, true) => c.kick.bot = None,
            (P::TikTok, _) => {}
        }
        revoke
    }

    /// The channel to read (empty: the logged-in account's own).
    fn channel(self, c: &Config) -> &str {
        match self {
            P::Twitch => &c.twitch.channel,
            P::YouTube => &c.youtube.channel_id,
            P::Kick => &c.kick.channel,
            P::TikTok => &c.tiktok.channel,
        }
    }

    /// Whether the channel read is the broadcaster's own (by id, or by name ignoring case, spaces and @).
    fn own(self, c: &Config) -> bool {
        let norm = |s: &str| s.trim_start_matches('@').replace(' ', "").to_lowercase();
        let ch = norm(self.channel(c));
        let mine = match self {
            P::Twitch => return !crate::twitch::other_channel(c),
            P::YouTube => c
                .youtube
                .broadcaster
                .as_ref()
                .map(|b| [norm(&b.channel_id), norm(&b.name)]),
            P::Kick => c.kick.broadcaster.as_ref().map(|b| [norm(&b.username), String::new()]),
            P::TikTok => None,
        };
        ch.is_empty() || mine.is_some_and(|m| m.contains(&ch))
    }

    /// What the channel box shows: nothing while the channel read is the broadcaster's own (the name is
    /// kept, but it isn't something typed), the same on every platform.
    fn typed_channel(self, c: &Config) -> &str {
        if self.raw_account_name(c, false).is_some() && self.own(c) {
            ""
        } else {
            self.channel(c)
        }
    }

    /// Set the channel from what was typed (a URL or @handle works); what was kept.
    fn set_channel(self, c: &mut Config, raw: &str) -> String {
        let (v, slot) = match self {
            P::Twitch => (clean_twitch_channel(raw), &mut c.twitch.channel),
            P::YouTube => (crate::youtube::clean_youtube_channel(raw), &mut c.youtube.channel_id),
            P::Kick => (crate::kick::clean_kick_channel(raw), &mut c.kick.channel),
            P::TikTok => (crate::tiktok::clean_tiktok_channel(raw), &mut c.tiktok.channel),
        };
        if *slot != v {
            slot.clone_from(&v);
            // The ids looked up for the old channel.
            match self {
                P::Kick => {
                    c.kick.channel_id = None;
                    c.kick.chatroom_id = None;
                }
                P::Twitch => c.twitch.channel_id.clear(),
                _ => {}
            }
        }
        v
    }

    fn slug(self) -> &'static str {
        match self {
            P::Twitch => "twitch",
            P::YouTube => "youtube",
            P::Kick => "kick",
            P::TikTok => "tiktok",
        }
    }

    /// Whether it connects when Streamerz starts (it does, unless turned off).
    fn auto(self, c: &Config) -> bool {
        match self {
            P::Twitch => !c.twitch.manual_connect,
            P::YouTube => !c.youtube.manual_connect,
            P::Kick => !c.kick.manual_connect,
            P::TikTok => !c.tiktok.manual_connect,
        }
    }

    fn manual_mut(self, c: &mut Config) -> &mut bool {
        match self {
            P::Twitch => &mut c.twitch.manual_connect,
            P::YouTube => &mut c.youtube.manual_connect,
            P::Kick => &mut c.kick.manual_connect,
            P::TikTok => &mut c.tiktok.manual_connect,
        }
    }

    fn connect(self, cmds: &Cmds, on: bool) {
        tell(cmds, move |e| match (self, on) {
            (P::Twitch, true) => e.twitch_start().send(crate::twitch::TwitchCmd::Connect),
            (P::YouTube, true) => e.youtube_start().send(crate::youtube::YouTubeCmd::Connect),
            (P::Kick, true) => e.kick_start().send(crate::kick::KickCmd::Connect),
            (P::TikTok, true) => e.tiktok_start().send(crate::tiktok::TikTokCmd::Connect),
            (P::Twitch, false) => {
                if let Some(t) = e.twitch() {
                    t.send(crate::twitch::TwitchCmd::Disconnect);
                }
            }
            (P::YouTube, false) => {
                if let Some(y) = e.youtube() {
                    y.send(crate::youtube::YouTubeCmd::Disconnect);
                }
            }
            (P::Kick, false) => {
                if let Some(k) = e.kick() {
                    k.send(crate::kick::KickCmd::Disconnect);
                }
            }
            (P::TikTok, false) => {
                if let Some(t) = e.tiktok() {
                    t.send(crate::tiktok::TikTokCmd::Disconnect);
                }
            }
        });
    }
}

/// Information about a logged-in account slot (Broadcaster or Bot) for Chat's account indicator & selector.
pub(super) struct AccountSlot {
    pub name: String,
    pub note: Option<&'static str>,
    pub pic: Option<fltk::image::RgbImage>,
}

/// The extension's single account and the existing image cache, shared by every account indicator.
pub(super) fn browser_account(cmds: &Cmds, size: i32) -> Option<AccountSlot> {
    let (name, url) = query(cmds, |e| (e.tiktok_bridge.user(), e.tiktok_bridge.avatar()))?;
    if name.is_empty() {
        return None;
    }
    let pic = (!url.is_empty())
        .then(|| super::img::get(cmds, &url, size, true))
        .flatten()
        .map(|p| p.first());
    Some(AccountSlot { name, note: None, pic })
}

/// Account slots and round pictures. TikTok uses only the first slot, for its browser account.
pub(super) fn platform_accounts(cmds: &Cmds, cfg: &Cfg, platform: &str, size: i32) -> [Option<AccountSlot>; 2] {
    if platform == "tiktok" {
        return [browser_account(cmds, size), None];
    }
    let Some(p) = P::from_slug(platform) else {
        return [None, None];
    };
    let raw = {
        let c = cfg.borrow();
        [
            p.raw_account_name(&c, false).map(|n| (n, p.account_note(&c, false))),
            p.raw_account_name(&c, true).map(|n| (n, p.account_note(&c, true))),
        ]
    };
    let build = |(bot, item): (usize, Option<(String, Option<&'static str>)>)| {
        let (name, note) = item?;
        let pic = picture(cmds, cfg, p, bot == 1, &name, size);
        Some(AccountSlot { name, note, pic })
    };
    let [b0, b1] = raw;
    [build((0, b0)), build((1, b1))]
}

/// The chat room a platform's column shows: the channel's name (the logged-in broadcaster's own when no
/// other is set) and their round picture (whether their own account or an outside channel).
pub(super) fn room(cmds: &Cmds, cfg: &Cfg, platform: &str, size: i32) -> (String, Option<fltk::image::RgbImage>) {
    let Some(p) = P::from_slug(platform) else {
        return (String::new(), None);
    };
    let browser = (platform == "tiktok").then(|| browser_account(cmds, size)).flatten();
    let (set, own, mine) = {
        let c = cfg.borrow();
        (
            p.channel(&c).trim().trim_start_matches('@').to_string(),
            browser
                .as_ref()
                .is_some_and(|b| p.channel(&c).is_empty() || b.name.eq_ignore_ascii_case(p.channel(&c)))
                || p.own(&c),
            browser
                .as_ref()
                .map(|b| b.name.clone())
                .or_else(|| p.raw_account_name(&c, false)),
        )
    };
    let name = if set.is_empty() {
        mine.clone().unwrap_or_default()
    } else {
        set
    };
    // The name as the platform writes it ("umaNanaAi"), once it has told; Twitch and YouTube accounts
    // already keep theirs.
    // The channel box may hold a typed lowercase login even when it is the account's own, so the
    // account's own display name wins there.
    let shown = match (&mine, own && matches!(p, P::Twitch | P::YouTube | P::TikTok)) {
        _ if name.is_empty() => name.clone(),
        (Some(m), true) if !m.is_empty() => m.clone(),
        _ => {
            channel_url(cmds, cfg, p, &name);
            channel_name(p, &name)
                .or_else(|| mine.clone().filter(|m| own && !m.is_empty()))
                .unwrap_or_else(|| name.clone())
        }
    };
    let pic = if own && platform == "tiktok" {
        browser.and_then(|b| b.pic)
    } else if own {
        mine.as_ref()
            .and_then(|m| picture(cmds, cfg, p, false, m, size))
            .or_else(|| {
                if !name.is_empty() {
                    channel_picture(cmds, cfg, p, &name, size)
                } else {
                    None
                }
            })
    } else if !name.is_empty() {
        channel_picture(cmds, cfg, p, &name, size)
    } else {
        None
    };
    (shown, pic)
}

/// Trigger an interactive keyring unlock on the engine thread and alert only if it fails.
pub(super) fn unlock_logins_interactive(cmds: &Cmds) {
    let (tx, rx) = std::sync::mpsc::channel();
    tell(cmds, move |e| {
        let e = e.clone();
        tokio::task::spawn_local(async move {
            let _ = tx.send(e.unlock_saved_logins(true).await);
        });
    });
    let mut waited = 0;
    loop {
        if let Ok(res) = rx.try_recv() {
            match res {
                Ok(true) => {
                    crate::ui::changed();
                }
                Ok(false) => {}
                Err(err) => {
                    super::dialogs::error(&trf("accounts.keyring_unlock_failed", &[("error", &err.to_string())]));
                }
            }
            break;
        }
        waited += 1;
        if waited > 300 {
            break;
        }
        super::nested_wait(Some(0.05));
    }
}

/// Each platform's state, read on the engine's thread: connected, standby, and TikTok's status text.
fn states(cmds: &Cmds) -> ([bool; 4], [bool; 4], String) {
    query(cmds, |e| {
        let tiktok = match e.tiktok().map(|t| t.current_status()) {
            Some(
                crate::tiktok::TikTokStatus::Error(m)
                | crate::tiktok::TikTokStatus::Connecting(m)
                | crate::tiktok::TikTokStatus::Standby(m),
            ) => m,
            _ => String::new(),
        };
        (
            [
                e.twitch_connected(),
                e.youtube_connected(),
                e.kick_connected(),
                e.tiktok_connected(),
            ],
            [false, e.youtube_standby(), false, e.tiktok_standby()],
            tiktok,
        )
    })
    .unwrap_or_default()
}

/// Profile picture URLs by "platform:bot:name", looked up once a run; "" while looking (or failed).
static AVATARS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// When each picture was last looked up this run (unix seconds): a failed one is tried again, not
/// more often than every `RETRY` seconds.
static LOOKED: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());
const RETRY: u64 = 120;

/// Whether a picture's saved address (found `at`, unix seconds) is to be looked up again `now`: none
/// was found, it's a day old, or it stops working soon (TikTok signs theirs for a few days).
fn stale(url: &str, at: u64, now: u64) -> bool {
    const FRESH: u64 = 24 * 60 * 60;
    url.is_empty() || now.saturating_sub(at) >= FRESH || expiring(url, now)
}

/// Whether a signed address (`x-expires=` in it) stops working within ten minutes of `now`.
fn expiring(url: &str, now: u64) -> bool {
    url.split(['?', '&'])
        .find_map(|kv| kv.strip_prefix("x-expires="))
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|at| at <= now + 600)
}

/// Whether the lookup of `key` may start `now` (once every `RETRY` seconds at most); it's noted.
fn may_look(key: &str, now: u64) -> bool {
    let Ok(mut looked) = LOOKED.lock() else {
        return false;
    };
    match looked.iter_mut().find(|(k, _)| k == key) {
        Some((_, at)) if now.saturating_sub(*at) < RETRY => false,
        Some((_, at)) => {
            *at = now;
            true
        }
        None => {
            looked.push((key.to_string(), now));
            true
        }
    }
}

/// The account's profile picture URL, from the platform (on the engine's thread).
async fn avatar(e: &Rc<Engine>, cfg: &Cfg, p: P, bot: bool) -> Option<String> {
    let get = |url: String, h: Vec<(String, String)>| async move {
        let r = crate::http::request("GET", &url, &h, b"").await.ok()?;
        serde_json::from_slice::<Value>(&r.body).ok()
    };
    let at = |v: Option<Value>, ptr: &str| v?.pointer(ptr).cloned();
    let bearer = |t: &str| ("Authorization".to_string(), format!("Bearer {t}"));
    let v = match p {
        P::Twitch => {
            let (id, t) = {
                let c = cfg.borrow();
                let a = if bot { &c.twitch.bot } else { &c.twitch.broadcaster }.clone()?;
                (a.user_id, a.token.access_token)
            };
            if t.is_empty() {
                return None;
            }
            let h = vec![("Client-Id".into(), crate::twitch::CLIENT_ID.into()), bearer(&t)];
            at(
                get(format!("{}/users?id={id}", crate::twitch::HELIX_URL), h).await,
                "/data/0/profile_image_url",
            )
        }
        P::Kick => {
            let t = crate::kick::token(e, cfg, bot, false).await.ok()?;
            at(
                get(format!("{}/users", crate::kick::KICK_API_BASE), vec![bearer(&t)]).await,
                "/data/0/profile_picture",
            )
        }
        P::YouTube => {
            let t = crate::youtube::token(e, cfg, bot, false).await.ok()?;
            let url = format!("{}/channels?part=snippet&mine=true", crate::youtube::YOUTUBE_API_BASE);
            at(
                get(url, vec![bearer(&t)]).await,
                "/items/0/snippet/thumbnails/default/url",
            )
        }
        P::TikTok => None,
    };
    v?.as_str().filter(|u| u.starts_with("http")).map(str::to_string)
}

/// A channel's profile picture URL and display name (as the platform writes it, e.g. "umaNanaAi"), read from
/// public APIs or GQL (works for outside channels).
async fn channel_profile(cfg: &Cfg, p: P, channel: &str) -> (Option<String>, Option<String>) {
    let clean = channel.trim().trim_start_matches('@');
    if clean.is_empty() {
        return (None, None);
    }
    let text = |v: &Value, ptr: &str| {
        v.pointer(ptr)
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
            .map(str::to_string)
    };
    let link = |v: Option<String>| v.filter(|u| u.starts_with("http"));
    match p {
        P::Twitch => {
            let (cid, tok) = {
                let c = cfg.borrow();
                let a = c.twitch.broadcaster.as_ref().or(c.twitch.bot.as_ref()).cloned();
                (
                    crate::twitch::CLIENT_ID.to_string(),
                    a.map(|x| x.token.access_token).unwrap_or_default(),
                )
            };
            if !tok.is_empty() {
                let h = vec![
                    ("Client-Id".into(), cid),
                    ("Authorization".into(), format!("Bearer {tok}")),
                ];
                let url = format!(
                    "{}/users?login={}",
                    crate::twitch::HELIX_URL,
                    crate::commands::url_encode(clean)
                );
                if let Ok(r) = crate::http::request("GET", &url, &h, b"").await {
                    if let Ok(v) = serde_json::from_slice::<Value>(&r.body) {
                        let pic = link(text(&v, "/data/0/profile_image_url"));
                        let name = text(&v, "/data/0/display_name");
                        if pic.is_some() || name.is_some() {
                            return (pic, name);
                        }
                    }
                }
            }
            let headers = [
                ("Client-Id".to_string(), "kimne78kx3ncx6brgo4mv6wki5h1ko".to_string()),
                ("Content-Type".to_string(), "application/json".to_string()),
            ];
            let q = serde_json::json!({
                "query": format!("{{ user(login: \"{clean}\") {{ displayName profileImageURL(width: 150) }} }}")
            });
            if let Ok(r) =
                crate::http::request("POST", "https://gql.twitch.tv/gql", &headers, q.to_string().as_bytes()).await
            {
                if let Ok(v) = serde_json::from_slice::<Value>(&r.body) {
                    return (
                        link(text(&v, "/data/user/profileImageURL")),
                        text(&v, "/data/user/displayName"),
                    );
                }
            }
            (None, None)
        }
        P::YouTube => {
            let (_, pic, name) = crate::youtube::public_profile(clean).await;
            (pic, name)
        }
        P::Kick => {
            let h = [("Accept".to_string(), "application/json".to_string())];
            let url = format!("https://kick.com/api/v2/channels/{clean}");
            let Ok(r) = crate::http::request("GET", &url, &h, b"").await else {
                return (None, None);
            };
            let Ok(v) = serde_json::from_slice::<Value>(&r.body) else {
                return (None, None);
            };
            (
                link(text(&v, "/user/profile_pic").or_else(|| text(&v, "/user/profile_picture"))),
                text(&v, "/user/username"),
            )
        }
        P::TikTok => crate::webcast::profile(clean).await,
    }
}

/// Display names by "platform:chan:name", from the platform (empty until known).
static NAMES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// The channel's display name when the platform has told it (looked up with its picture).
fn channel_name(p: P, name: &str) -> Option<String> {
    let key = channel_key(p, name);
    NAMES
        .lock()
        .ok()?
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, n)| n.clone())
        .filter(|n| !n.is_empty())
}

/// What a channel's picture and name are kept by: one key for the channel, however it was looked up
/// (TikTok's through the browser account or not).
fn channel_key(p: P, name: &str) -> String {
    format!(
        "{}:chan:{}",
        p.slug(),
        name.trim().trim_start_matches('@').to_lowercase()
    )
}

/// The channel's profile picture URL (cached for 24h), and its display name once known. Looks both up in
/// the background when missing, old or about to stop working, and again later when that failed.
fn channel_url(cmds: &Cmds, cfg: &Cfg, p: P, name: &str) -> String {
    let norm = name.trim().trim_start_matches('@').to_lowercase();
    if norm.is_empty() {
        return String::new();
    }
    let key = channel_key(p, &norm);
    let now = crate::fmt::unix_now().as_secs();
    let Ok(mut a) = AVATARS.lock() else {
        return String::new();
    };
    let (url, look) = match a.iter().find(|(k, _)| *k == key) {
        Some((_, url)) => (url.clone(), url.is_empty() || expiring(url, now)),
        None => {
            let saved = super::img::saved("avatars").map(|v| v.1).unwrap_or_default();
            // TikTok's used to be kept apart when found through the browser account.
            let entry = [key.clone(), format!("{key}:browser")]
                .iter()
                .map(|k| saved[k].clone())
                .find(|v| v[0].as_str().is_some_and(|u| !u.is_empty()))
                .unwrap_or_default();
            let (url, at, known) = (
                entry[0].as_str().unwrap_or("").to_string(),
                entry[1].as_u64().unwrap_or(0),
                entry[2].as_str().unwrap_or("").to_string(),
            );
            a.push((key.clone(), url.clone()));
            if let Ok(mut n) = NAMES.lock() {
                n.push((key.clone(), known.clone()));
            }
            let look = stale(&url, at, now) || known.is_empty();
            (url, look)
        }
    };
    drop(a);
    if look && may_look(&key, now) {
        let cfg = cfg.clone();
        tell(cmds, move |e| {
            let e = e.clone();
            tokio::task::spawn_local(async move {
                let signed = if p == P::TikTok && e.tiktok_bridge.is_connected() {
                    crate::tiktok_bridge::search(&e, &norm)
                        .await
                        .ok()
                        .and_then(|found| found.into_iter().find(|f| f.handle.eq_ignore_ascii_case(&norm)))
                } else {
                    None
                };
                let (pic, shown) = match signed {
                    Some(f) if !f.picture.is_empty() => (Some(f.picture), Some(f.name)),
                    _ => channel_profile(&cfg, p, &norm).await,
                };
                let (url, shown) = (pic.unwrap_or_default(), shown.unwrap_or_default());
                // Nothing found (TikTok answered with a bot check...): what was known stays, and it's
                // asked again later.
                if url.is_empty() && shown.is_empty() {
                    return;
                }
                if let Ok(mut a) = AVATARS.lock() {
                    if let Some(slot) = a.iter_mut().find(|(k, _)| *k == key) {
                        if !url.is_empty() {
                            slot.1.clone_from(&url);
                        }
                    }
                }
                if let Ok(mut n) = NAMES.lock() {
                    if let Some(slot) = n.iter_mut().find(|(k, _)| *k == key) {
                        if !shown.is_empty() {
                            slot.1.clone_from(&shown);
                        }
                    }
                }
                let mut all = super::img::saved("avatars").map(|v| v.1).unwrap_or_default();
                if !all.is_object() {
                    all = serde_json::json!({});
                }
                let was = all[&key].clone();
                let keep = |new: &str, i: usize| {
                    if new.is_empty() {
                        was[i].as_str().unwrap_or("").to_string()
                    } else {
                        new.to_string()
                    }
                };
                all[&key] = serde_json::json!([keep(&url, 0), crate::fmt::unix_now().as_secs(), keep(&shown, 2)]);
                if let Some(o) = all.as_object_mut() {
                    o.remove(&format!("{key}:browser"));
                }
                super::img::save("avatars", &all);
                super::wake(super::Msg::Image);
            });
        });
    }
    url
}

/// The channel's profile picture, `size` px and round.
fn channel_picture(cmds: &Cmds, cfg: &Cfg, p: P, name: &str, size: i32) -> Option<fltk::image::RgbImage> {
    let url = channel_url(cmds, cfg, p, name);
    if url.is_empty() {
        return None;
    }
    super::img::get(cmds, &url, size, true).map(|p| p.first())
}

/// The account's profile picture, `size` px and round. Its URL is kept in the cache: used at once,
/// looked up again (in the background) when older than a day or not known yet.
fn picture(cmds: &Cmds, cfg: &Cfg, p: P, bot: bool, name: &str, size: i32) -> Option<fltk::image::RgbImage> {
    let key = format!("{}:{bot}:{name}", p.name());
    let now = crate::fmt::unix_now().as_secs();
    let mut a = AVATARS.lock().ok()?;
    let url = match a.iter().find(|(k, _)| *k == key) {
        Some((_, url)) if !url.is_empty() && !expiring(url, now) => url.clone(),
        found => {
            let (url, at) = match found {
                // Not found, or about to stop working: looked up again (not too often).
                Some((_, url)) => (url.clone(), 0),
                None => {
                    let saved = super::img::saved("avatars").map(|v| v.1).unwrap_or_default();
                    let (url, at) = (
                        saved[&key][0].as_str().unwrap_or("").to_string(),
                        saved[&key][1].as_u64().unwrap_or(0),
                    );
                    a.push((key.clone(), url.clone()));
                    (url, at)
                }
            };
            if stale(&url, at, now) && may_look(&key, now) {
                let (cfg, name) = (cfg.clone(), name.to_string());
                tell(cmds, move |e| {
                    let e = e.clone();
                    tokio::task::spawn_local(async move {
                        // With the login (expired, or the keyring locked) it can't ask as the account: the
                        // public profile has the same picture.
                        let url = match avatar(&e, &cfg, p, bot).await {
                            Some(url) => Some(url),
                            None if p != P::TikTok => channel_profile(&cfg, p, &name).await.0,
                            None => None,
                        };
                        let Some(url) = url else {
                            return;
                        };
                        if let Ok(mut a) = AVATARS.lock() {
                            if let Some(slot) = a.iter_mut().find(|(k, _)| *k == key) {
                                slot.1.clone_from(&url);
                            }
                        }
                        let mut all = super::img::saved("avatars").map(|v| v.1).unwrap_or_default();
                        if !all.is_object() {
                            all = serde_json::json!({});
                        }
                        all[&key] = serde_json::json!([url, crate::fmt::unix_now().as_secs()]);
                        super::img::save("avatars", &all);
                        super::wake(super::Msg::Image);
                    });
                });
            }
            url
        }
    };
    drop(a);
    super::img::get(cmds, &url, size, true).map(|p| p.first())
}

/// Connect platform `i` (Twitch, YouTube, Kick, TikTok) now.
pub(super) fn connect(cmds: &Cmds, i: usize) {
    ALL[i].connect(cmds, true);
}

struct Row {
    p: P,
    status: Frame,
    /// Whose chat to read and moderate (empty: your own), and whose it is.
    channel: Button,
    whose: Frame,
    connect: Button,
    adv: Button,
    /// Account, then bot: profile picture, name and Login/Logout.
    who: [(Frame, Frame, Button); 2],
    /// The rows they sit in: Login and Logout have different words, so the button's width follows them.
    rows: [Flex; 2],
    /// TikTok only: sending through the browser, its state and Set up.
    sending: Option<Frame>,
    browser_pic: Option<Frame>,
}

pub struct Accounts {
    pub tab: Flex,
    pub integrations_tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    lock_bar: Flex,
    lock_msg: Frame,
    rows: Vec<Row>,
    spotify_status: Frame,
}

impl Accounts {
    pub fn refresh(&mut self) {
        // Before borrowing the config: the engine may be waiting to write it.
        let (on, standby, tiktok) = states(&self.cmds);
        let bridge = query(&self.cmds, |e| {
            (
                e.tiktok_bridge.is_connected(),
                e.tiktok_bridge.user(),
                e.tiktok_bridge.ready(),
                e.tiktok_bridge.error(),
            )
        })
        .unwrap_or_default();
        let browser = browser_account(&self.cmds, ROW - 4);
        let locked = crate::secrets::is_locked();
        if locked {
            let detail = match crate::secrets::locked_reason() {
                Some(reason) if !reason.is_empty() => trf("accounts.locked_reason", &[("reason", &reason)]),
                _ => tr("accounts.locked_default").to_string(),
            };
            super::Health::Locked.show(&mut self.lock_msg, &detail);
            self.lock_bar.show();
        } else {
            self.lock_bar.hide();
        }

        let c = self.cfg.borrow();
        for (i, r) in self.rows.iter_mut().enumerate() {
            let active = on[i] || standby[i];
            use super::Health as H;
            let (h, text) = if on[i] {
                (H::Up, tr("conns.connected").to_string())
            } else if standby[i] {
                match r.p {
                    P::TikTok if !tiktok.is_empty() => (H::Standby, tiktok.clone()),
                    _ => (H::Standby, tr("accounts.standby_default").to_string()),
                }
            } else if locked && r.p.raw_account_name(&c, false).is_some() {
                (H::Locked, tr("accounts.keyring_locked_above").to_string())
            } else {
                let why = match r.p {
                    P::Twitch if c.twitch.broadcaster.is_none() && c.twitch.channel.trim().is_empty() => {
                        tr("accounts.login_broadcaster_first").into()
                    }
                    P::Twitch => String::new(),
                    P::TikTok if !tiktok.is_empty() => tiktok.clone(),
                    p if p.channel(&c).is_empty() => tr("accounts.login_broadcaster_first").into(),
                    _ => String::new(),
                };
                if why.is_empty() {
                    (H::Off, tr("conns.off").to_string())
                } else {
                    (H::Off, trf("accounts.off_why", &[("why", &why)]))
                }
            };
            h.show(&mut r.status, &text);
            r.status.set_tooltip(&text.replace('@', "@@"));
            if let Some(pic) = &mut r.browser_pic {
                pic.set_image(browser.as_ref().and_then(|b| b.pic.clone()));
                pic.redraw();
            }
            if let Some(status) = &mut r.sending {
                let (h, text) = match &bridge {
                    _ if !crate::tiktok_bridge::set_up(&c) => {
                        (H::Off, tr("accounts.tiktok_sending.not_set_up").to_string())
                    }
                    (false, _, _, _) => (H::Standby, tr("accounts.tiktok_sending.waiting").to_string()),
                    (_, _, _, error) if !error.is_empty() => (H::Off, error.clone()),
                    (_, _, false, _) => (H::Standby, tr("accounts.tiktok_sending.loading").to_string()),
                    (true, user, _, _) if user.is_empty() => {
                        (H::Standby, tr("accounts.tiktok_sending.login_needed").to_string())
                    }
                    (true, user, _, _) => (H::Up, trf("accounts.tiktok_sending.connected", &[("user", user)])),
                };
                h.show(status, &text);
                status.set_tooltip(&text.replace('@', "@@"));
            }
            r.connect.set_label(if active {
                tr("btn.disconnect")
            } else {
                tr("btn.connect")
            });
            super::with_icon(&mut r.connect);
            // Reading your own channel shows an empty box (the same on every platform): the name is kept,
            // but it isn't something typed.
            let shown = r.p.typed_channel(&c).to_string();
            // A channel set shows who it is: their picture and their name as the platform writes it. The
            // search icon says the button picks one.
            let (label, pic) = if shown.is_empty() {
                (tr("accounts.channel_placeholder").to_string(), None)
            } else {
                let (name, pic) = room(&self.cmds, &self.cfg, r.p.slug(), 18);
                (if name.is_empty() { shown } else { name }, pic)
            };
            match pic {
                Some(pic) => {
                    r.channel.set_image(Some(pic));
                    r.channel.set_label(&format!("  {}", label.replace('@', "@@")));
                }
                None => {
                    r.channel
                        .set_image(super::icons::before_label("search", super::button_ink(), 14));
                    r.channel.set_label(&label.replace('@', "@@"));
                }
            }
            r.channel.redraw();
            // Someone else's channel: what we are there, once the platform says (roles.rs).
            let (role, _) = crate::roles::get(&r.p.name().to_lowercase());
            let whose = if r.p.channel(&c).is_empty() || r.p.raw_account_name(&c, false).is_none() {
                String::new()
            } else if r.p.own(&c) {
                tr("accounts.channel_yours").into()
            } else if role.moderates() {
                trf("accounts.channel_moderator", &[("role", role.name())])
            } else if role >= crate::roles::Role::Viewer {
                tr("accounts.channel_viewer").into()
            } else {
                tr("accounts.channel_theirs").into()
            };
            r.whose.set_label(&whose);
            for (bot, (pic, name, b)) in r.who.iter_mut().enumerate() {
                let account = r.p.account(&c, bot == 1);
                match &account {
                    Some(n) => {
                        name.set_label(&n.replace('@', "@@"));
                        name.set_tooltip(&n.replace('@', "@@"));
                        name.set_label_color(fltk::enums::Color::Foreground);
                        b.set_label(tr("btn.logout"));
                        super::with_icon(b);
                        r.rows[bot].fixed(b, super::fit::grown(80, tr("btn.logout"), true));
                    }
                    None => {
                        name.set_label(tr("accounts.not_logged_in"));
                        name.set_tooltip("");
                        name.set_label_color(super::muted());
                        b.set_label(tr("btn.login"));
                        super::with_icon(b);
                        r.rows[bot].fixed(b, super::fit::grown(80, tr("btn.login"), true));
                    }
                }
                let raw_name = r.p.raw_account_name(&c, bot == 1);
                pic.set_image(raw_name.and_then(|n| picture(&self.cmds, &self.cfg, r.p, bot == 1, &n, ROW - 4)));
                // It comes in after the page was drawn (a download): show it when it does.
                pic.redraw();
            }
        }
        let spotify_on = c.spotify.enabled && !c.spotify.token.access_token.is_empty();
        let (health, text) = if spotify_on {
            (super::Health::Up, tr("conns.connected"))
        } else {
            (super::Health::Off, tr("conns.off"))
        };
        health.show(&mut self.spotify_status, text);
        drop(c);
        self.tab.layout();
        self.tab.redraw();
    }
}

pub fn tab(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Accounts>> {
    let (scroll, mut col) = super::scroll::page(tabs, "Accounts");

    let mut lock_bar = Flex::default().row();
    lock_bar.set_pad(8);
    let lock_msg = Frame::default()
        .with_align(Align::Left | Align::Inside | Align::Clip)
        .ellipsis();
    let mut unlock_btn = button(&mut lock_bar, tr("accounts.unlock_logins"), 120);
    lock_bar.end();
    col.fixed(&lock_bar, ROW);
    lock_bar.hide();

    let mut rows = Vec::new();
    for p in ALL {
        let mut top = Flex::default().row();
        top.set_pad(8);
        let mut name = Frame::default()
            .with_label(&format!("  {}", p.name()))
            .with_align(Align::Left | Align::Inside | Align::ImageNextToText)
            .ellipsis();
        name.set_label_font(fltk::enums::Font::HelveticaBold);
        name.set_image(super::img::logo(p.name(), ROW - 8));
        top.fixed(&name, 110);
        let status = Frame::default()
            .with_align(Align::Left | Align::Inside | Align::Clip)
            .ellipsis();
        let connect = button(&mut top, tr("btn.connect"), 100);
        let mut adv = button(&mut top, &format!("{}…", tr("btn.advanced")), 100);
        top.end();
        col.fixed(&top, ROW);
        // Narrow: the buttons keep their icons.
        top.resize_callback({
            let (connect, adv) = (connect.clone(), adv.clone());
            let connect_w = super::fit::grown(120, tr("btn.disconnect"), true);
            let adv_w = super::fit::grown(120, tr("btn.advanced"), true);
            move |t, _, _, w, _| {
                let narrow = w < 520;
                t.fixed(&connect, if narrow { 36 } else { connect_w });
                t.fixed(&adv, if narrow { 36 } else { adv_w });
            }
        });
        adv.set_tooltip(tr("accounts.advanced_tooltip"));

        // The account you log in with: it doesn't have to own the channel.
        let mut who = Vec::new();
        let mut people = Vec::new();
        let account_row = |label: &str, who: &mut Vec<(Frame, Frame, Button)>, people: &mut Vec<Flex>| {
            let mut row = Flex::default().row();
            row.set_pad(8);
            let mut l = Frame::default()
                .with_label(label)
                .with_align(Align::Right | Align::Inside);
            l.set_label_color(super::muted());
            row.fixed(&l, 110);
            let pic = Frame::default();
            row.fixed(&pic, ROW);
            let f = Frame::default()
                .with_align(Align::Left | Align::Inside | Align::Clip)
                .ellipsis();
            let b = button(&mut row, tr("btn.login"), 80);
            row.end();
            who.push((pic, f, b));
            people.push(row.clone());
            row
        };
        let row = account_row(tr("accounts.row_account"), &mut who, &mut people);
        col.fixed(&row, if p == P::TikTok { 0 } else { ROW });
        if p == P::TikTok {
            row.clone().hide();
        }

        // Whose chat: yours, or someone else's that you moderate with your account.
        let mut mid = Flex::default().row();
        mid.set_pad(8);
        let mut label = Frame::default()
            .with_label(tr("accounts.channel"))
            .with_align(Align::Right | Align::Inside);
        label.set_label_color(super::muted());
        mid.fixed(&label, 110);
        // A button: it opens the search (the channel is picked there, not typed here).
        let mut channel = Button::default().with_label(tr("accounts.channel_placeholder"));
        channel.set_tooltip(tr("accounts.channel_tooltip"));
        channel.set_image(super::icons::before_label("search", super::button_ink(), 14));
        channel.set_align(Align::ImageNextToText | Align::Inside);
        mid.fixed(&channel, 220);
        let mut whose = Frame::default()
            .with_align(Align::Left | Align::Inside | Align::Clip)
            .ellipsis();
        whose.set_label_color(super::muted());
        mid.end();
        col.fixed(&mid, ROW);

        let row = account_row(tr("accounts.row_bot"), &mut who, &mut people);
        col.fixed(&row, if p == P::TikTok { 0 } else { ROW });
        if p == P::TikTok {
            row.clone().hide();
        }
        // TikTok sends through the user's browser (tiktok_bridge.rs).
        let mut browser_pic = None;
        let sending = (p == P::TikTok).then(|| {
            let mut row = Flex::default().row();
            row.set_pad(8);
            let mut l = Frame::default()
                .with_label(tr("accounts.row_sending"))
                .with_align(Align::Right | Align::Inside);
            l.set_label_color(super::muted());
            row.fixed(&l, 110);
            let pic = Frame::default();
            row.fixed(&pic, ROW);
            browser_pic = Some(pic);
            let status = Frame::default()
                .with_align(Align::Left | Align::Inside | Align::Clip)
                .ellipsis();
            let mut set_up = button(&mut row, tr("btn.set_up"), 80);
            row.end();
            col.fixed(&row, ROW);
            row.resize_callback({
                let set_up = set_up.clone();
                move |r, _, _, w, _| r.fixed(&set_up, if w < 520 { 36 } else { 100 })
            });
            set_up.set_callback({
                let cmds = cmds.clone();
                move |_| tiktok_sending(&cmds)
            });
            status
        });
        let sep = Frame::default();
        col.fixed(&sep, 14);
        let who: [(Frame, Frame, Button); 2] = [who.remove(0), who.remove(0)];
        let rows_of: [Flex; 2] = [people.remove(0), people.remove(0)];
        rows.push(Row {
            p,
            status,
            channel,
            whose,
            adv,
            connect,
            who,
            rows: rows_of,
            sending,
            browser_pic,
        });
    }
    let guide = Frame::default()
        .with_label(tr("accounts.guide"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    guide.clone().set_label_color(super::muted());
    col.fixed(&guide, 110);
    Frame::default();
    col.end();
    scroll.end();

    let mut integrations_tab = super::tab(tabs, "Integrations");
    super::hint(&mut integrations_tab, tr("accounts.integrations.intro"), 36);
    let mut spotify_row = Flex::default().row();
    spotify_row.set_pad(8);
    let mut spotify_logo = Frame::default();
    spotify_logo.set_image(super::img::logo("Spotify", 18));
    spotify_row.fixed(&spotify_logo, 22);
    let mut spotify_name = Frame::default()
        .with_label(tr("accounts.spotify.name"))
        .with_align(Align::Left | Align::Inside);
    spotify_name.set_label_font(fltk::enums::Font::HelveticaBold);
    spotify_row.fixed(&spotify_name, 110);
    let spotify_status = Frame::default()
        .with_align(Align::Left | Align::Inside | Align::Clip)
        .ellipsis();
    let mut spotify_setup = button(&mut spotify_row, tr("btn.configure"), 100);
    spotify_row.end();
    integrations_tab.fixed(&spotify_row, ROW);
    super::hint(&mut integrations_tab, tr("accounts.spotify.about"), 36);
    Frame::default();
    integrations_tab.end();

    let me = Rc::new(RefCell::new(Accounts {
        tab: col,
        integrations_tab,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        lock_bar,
        lock_msg,
        rows: Vec::new(),
        spotify_status,
    }));
    spotify_setup.set_callback({
        let (cfg, cmds, me) = (cfg.clone(), cmds.clone(), me.clone());
        move |_| {
            spotify_configure(&cfg, &cmds);
            if let Ok(mut m) = me.try_borrow_mut() {
                m.refresh();
            }
        }
    });
    unlock_btn.set_callback({
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            unlock_logins_interactive(&cmds);
            if let Ok(mut m) = me.try_borrow_mut() {
                m.refresh();
            }
        }
    });
    for r in &mut rows {
        let p = r.p;
        let refresh = {
            let me = me.clone();
            move || {
                if let Ok(mut m) = me.try_borrow_mut() {
                    m.refresh();
                }
            }
        };
        r.adv.set_callback({
            let cfg = cfg.clone();
            let refresh = refresh.clone();
            move |_| {
                advanced(&cfg, p);
                refresh();
            }
        });
        r.channel.set_callback({
            let (cfg, cmds, refresh) = (cfg.clone(), cmds.clone(), refresh.clone());
            move |_| {
                let Some(raw) = super::find::channel(&cmds, p.slug()) else {
                    return;
                };
                let (before, slug) = (p.channel(&cfg.borrow()).to_string(), p.slug());
                // The channel read until now, as the community knows it, and how many people it saved.
                let old = query(&cmds, move |e| {
                    let ch = e.broadcaster(slug);
                    (e.community_of(slug, &ch), ch)
                });
                save(&cfg, |c| drop(p.set_channel(c, &raw)));
                if p.channel(&cfg.borrow()) != before {
                    // What the old channel showed goes; what to do with its people is asked.
                    let (saved, old) = old.unwrap_or_default();
                    let what = if saved == 0 {
                        Leftover::Keep
                    } else {
                        match dialogs::pick_one(
                            &trf(
                                "room.ask",
                                &[("platform", p.name()), ("channel", &old), ("count", &saved.to_string())],
                            ),
                            &[tr("room.keep"), tr("room.move"), tr("room.delete")],
                        ) {
                            Some(1) => Leftover::Move,
                            Some(2) => Leftover::Delete,
                            _ => Leftover::Keep,
                        }
                    };
                    tell(&cmds, move |e| {
                        e.channel_changed(slug, &old, what);
                        e.chat_forget(slug);
                        e.chat_polls_clear(slug);
                        e.chat_unpinned(slug, "");
                    });
                    // Read the new one now, if it was reading.
                    let (on, standby, _) = states(&cmds);
                    let i_p = ALL.iter().position(|x| *x == p).unwrap_or(0);
                    if on[i_p] || standby[i_p] {
                        p.connect(&cmds, false);
                        p.connect(&cmds, true);
                    }
                }
                refresh();
            }
        });
        r.connect.set_callback({
            let cmds = cmds.clone();
            let refresh = refresh.clone();
            move |b| {
                p.connect(&cmds, button_id(&b.label()) == Some("connect"));
                refresh();
            }
        });
        for (bot, (_, _, b)) in r.who.iter_mut().enumerate() {
            let bot = bot == 1;
            let cfg = cfg.clone();
            let cmds = cmds.clone();
            let refresh = refresh.clone();
            b.set_callback(move |b| {
                if button_id(&b.label()) == Some("logout") {
                    let mut revoke = None;
                    save(&cfg, |c| revoke = p.logout(c, bot));
                    if let Some(url) = revoke {
                        // Best effort: the token is gone here either way.
                        tell(&cmds, move |_| {
                            tokio::task::spawn_local(async move {
                                let h = [(
                                    "Content-Type".to_string(),
                                    "application/x-www-form-urlencoded".to_string(),
                                )];
                                // Kick reads the token from the query; Twitch and Google from a form.
                                let (u, q) = match url.split_once('?') {
                                    Some((u, q)) if !u.contains("kick.com") => (u, q),
                                    _ => (url.as_str(), ""),
                                };
                                let r = crate::http::request("POST", u, &h, q.as_bytes()).await;
                                let ok = r.as_ref().is_ok_and(|r| (200..300).contains(&r.status));
                                crate::warn(format!(
                                    "{}: login revoked{}",
                                    p.name(),
                                    if ok { "" } else { " (the server didn't confirm)" }
                                ));
                            });
                        });
                    }
                    if p == P::Twitch && !bot {
                        p.connect(&cmds, false);
                    }
                } else {
                    login(&cfg, &cmds, p, bot);
                }
                refresh();
            });
        }
    }
    me.borrow_mut().rows = rows;
    me
}

/// The login popup's "Log in": the same as the button in Accounts.
pub(super) fn login_again(cfg: &Cfg, cmds: &Cmds, platform: &str, bot: bool) {
    if let Some(p) = ALL.into_iter().find(|p| p.name() == platform) {
        login(cfg, cmds, p, bot);
    }
}

fn login(cfg: &Cfg, cmds: &Cmds, p: P, bot: bool) {
    crate::ui::relogin_done(p.name(), bot);
    match p {
        P::Twitch => device_login(cfg, cmds, bot),
        P::TikTok => tiktok_sending(cmds),
        P::Kick => {
            let c = cfg.clone();
            if let Some(a) = browser_login(cmds, "Kick", bot, async move { crate::kick::login(&c, bot).await }) {
                save(cfg, |c| {
                    *if bot { &mut c.kick.bot } else { &mut c.kick.broadcaster } = Some(a)
                });
            }
        }
        P::YouTube => {
            let c = cfg.clone();
            if let Some(a) = browser_login(cmds, "YouTube", bot, async move { crate::youtube::login(&c).await }) {
                save(cfg, |c| {
                    *if bot {
                        &mut c.youtube.bot
                    } else {
                        &mut c.youtube.broadcaster
                    } = Some(a)
                });
            }
        }
    }
}

/// Sending to TikTok goes through the user's own browser, with a small extension in it. Streamerz does
/// all it can: it finds the Chromium browsers here and whether the extension is in them, and on a click
/// (the consent) writes the extension, copies its folder, and opens a setup tab in that browser's
/// existing window. Each step is ticked off as it happens, up to logging in to TikTok.
fn tiktok_login(cmds: &Cmds) {
    tell(cmds, |e| {
        let e = e.clone();
        tokio::task::spawn_local(async move {
            if let Err(error) = e.tiktok_bridge.login().await {
                crate::warn(format!("TikTok: {error}"));
            }
        });
    });
}

fn tiktok_sending(cmds: &Cmds) {
    use super::Health as H;
    tell(cmds, |e| {
        let b = &e.tiktok_bridge;
        if b.is_connected() && !b.ready() && !b.login_required() && !b.error().is_empty() {
            let _ = crate::tiktok_bridge::retry(e);
        }
    });
    const W: i32 = 640;
    let browsers = crate::browsers::installed();
    let (mut win, answer) = dialogs::window(W, 440, tr("accounts.tiktok_sending.title"));
    let mut col = Flex::new(16, 0, W - 32, 432, None).column();
    col.set_pad(6);
    let mut why = Frame::default().with_align(Align::Left | Align::Inside | Align::Top);
    why.set_label_color(super::muted());
    col.fixed(&why, 56);
    let mut title = Frame::default()
        .with_label(tr("accounts.tiktok_sending.browsers"))
        .with_align(Align::Left | Align::Inside);
    title.set_label_font(fltk::enums::Font::HelveticaBold);
    col.fixed(&title, 22);
    let picked: Rc<RefCell<Option<crate::browsers::Browser>>> = Rc::default();
    let mut states = Vec::new();
    let mut none_row = if browsers.is_empty() {
        let f = Frame::default().with_align(Align::Left | Align::Inside | Align::ImageNextToText);
        col.fixed(&f, ROW);
        Some(f)
    } else {
        None
    };
    for b in &browsers {
        let mut row = Flex::default().row();
        row.set_pad(8);
        let mut logo = Frame::default();
        logo.set_image(super::icons::icon("globe", super::muted(), 16));
        row.fixed(&logo, 22);
        let mut name = Frame::default()
            .with_label(b.name)
            .with_align(Align::Left | Align::Inside);
        name.set_label_font(fltk::enums::Font::HelveticaBold);
        row.fixed(&name, 136);
        let state = Frame::default()
            .with_align(Align::Left | Align::Inside | Align::Clip)
            .ellipsis();
        let mut go = button(&mut row, tr("btn.set_up"), 100);
        go.set_tooltip(&b.data.display().to_string());
        row.end();
        col.fixed(&row, ROW);
        go.set_callback({
            let (cmds, b, picked) = (cmds.clone(), b.clone(), picked.clone());
            move |_| {
                if query(&cmds, |e| {
                    e.tiktok_bridge.is_connected() && e.tiktok_bridge.login_required()
                })
                .unwrap_or(false)
                {
                    tiktok_login(&cmds);
                    return;
                }
                let folder = match query(&cmds, crate::tiktok_bridge::install) {
                    Some(Ok(folder)) => folder,
                    Some(Err(error)) => return dialogs::error(&error),
                    None => return dialogs::error(tr("accounts.tiktok_sending.busy")),
                };
                fltk::app::copy(&folder.display().to_string());
                let setup_page = match crate::tiktok_bridge::write_setup_page(b.name, b.extensions_page, &folder) {
                    Ok(page) => page,
                    Err(error) => return dialogs::error(&error),
                };
                if let Err(error) = b.open_setup(&setup_page) {
                    return dialogs::error(&trf(
                        "accounts.tiktok_sending.cant_open",
                        &[("browser", b.name), ("error", &error.to_string())],
                    ));
                }
                *picked.borrow_mut() = Some(b.clone());
            }
        });
        let installed = b.has_extension_from(crate::tiktok_bridge::EXTENSION_ID, &crate::tiktok_bridge::folder());
        states.push((b.clone(), state, go, installed, row));
    }
    let sep = Frame::default();
    col.fixed(&sep, 6);
    let mut steps = Vec::new();
    for _ in 0..4 {
        let f = Frame::default().with_align(Align::Left | Align::Inside | Align::ImageNextToText);
        col.fixed(&f, 36);
        steps.push(f);
    }
    let mut note = Frame::default().with_align(Align::Left | Align::Inside | Align::ImageNextToText);
    note.set_label_size(12);
    col.fixed(&note, 28);
    let mut buttons = super::grid(
        &mut col,
        &[tr("btn.remove_streamerz"), tr("btn.copy_path"), tr("btn.retry")],
        1,
    )
    .into_iter();
    let mut remove = buttons.next().unwrap();
    let mut copy_btn = buttons.next().unwrap();
    let mut retry = buttons.next().unwrap();
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    Frame::default();
    let mut close = button(&mut bar, tr("dialogs.close"), 90);
    bar.end();
    col.fixed(&bar, ROW);
    col.end();
    win.end();
    close.set_shortcut(fltk::enums::Shortcut::from_key(fltk::enums::Key::Escape));
    close.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(true))
    });
    copy_btn.set_callback({
        let cmds = cmds.clone();
        move |_| {
            if let Some(Ok(folder)) = query(&cmds, crate::tiktok_bridge::install) {
                fltk::app::copy(&folder.display().to_string());
            }
        }
    });
    retry.set_callback({
        let cmds = cmds.clone();
        move |_| {
            if query(&cmds, |e| e.tiktok_bridge.login_required()).unwrap_or(false) {
                tiktok_login(&cmds);
            } else {
                tell(&cmds, |e| {
                    let _ = crate::tiktok_bridge::retry(e);
                });
            }
        }
    });
    remove.set_callback({
        let (cmds, a) = (cmds.clone(), answer.clone());
        move |_| {
            if dialogs::danger(
                tr("accounts.tiktok_sending.remove_confirm"),
                tr("dialogs.cancel"),
                tr("btn.remove_streamerz"),
            ) {
                tell(&cmds, crate::tiktok_bridge::remove);
                a.set(Some(false));
            }
        }
    });
    let mut asked_login = false;
    let mut last = None;
    let mut update = |win: &mut dialogs::Dialog, steps: &mut Vec<Frame>| {
        let Some((set_up, connected, user, ready, login_required, error)) = query(cmds, |e| {
            let b = &e.tiktok_bridge;
            (
                crate::tiktok_bridge::set_up(&e.config()),
                b.is_connected(),
                b.user(),
                b.ready(),
                b.login_required(),
                b.error(),
            )
        }) else {
            return;
        };
        let available = win
            .parent()
            .map(|p| (p.w(), p.h()))
            .or_else(|| fltk::app::first_window().map(|w| (w.w(), w.h())));
        let browser = picked.borrow().as_ref().map(|b| b.name);
        let snapshot = (
            set_up,
            connected,
            user.clone(),
            ready,
            login_required,
            error.clone(),
            browser,
            available,
        );
        if last.as_ref() == Some(&snapshot) {
            return;
        }
        last = Some(snapshot);
        let width = available.map_or(W, |(hw, _)| W.min((hw - 48).max(240)));
        let inner = width - 32;
        fltk::draw::set_font(why.label_font(), why.label_size());
        let why_lines = dialogs::wrap(tr("accounts.tiktok_sending.why"), inner.max(100), |t| {
            fltk::draw::width(t) as i32
        });
        let why_h = (why_lines.len() as i32 * 19 + 4).max(24);
        why.set_label(&why_lines.join("\n").replace('@', "@@"));
        col.fixed(&why, why_h);
        let mut browsers_h = states.len() as i32 * ROW;
        if let Some(none) = none_row.as_mut() {
            fltk::draw::set_font(none.label_font(), none.label_size());
            let wrapped = dialogs::wrap(tr("accounts.tiktok_sending.no_browser"), (inner - 30).max(100), |t| {
                fltk::draw::width(t) as i32
            });
            let nh = (wrapped.len() as i32 * 18 + 6).max(ROW);
            browsers_h = nh;
            col.fixed(none, nh);
            H::Off.show(none, &wrapped.join("\n"));
            none.set_align(Align::Left | Align::Inside | Align::ImageNextToText);
        }
        let single = states.len() == 1;
        let ext_folder = crate::tiktok_bridge::folder();
        for (b, state, go, known, _) in &mut states {
            if connected
                && !*known
                && (single
                    || browser == Some(b.name)
                    || b.has_extension_from(crate::tiktok_bridge::EXTENSION_ID, &ext_folder))
            {
                *known = true;
            }
            let installed = *known;
            H::show(
                if installed { H::Up } else { H::Off },
                state,
                tr(if installed {
                    "accounts.tiktok_sending.installed"
                } else {
                    "accounts.tiktok_sending.not_installed"
                }),
            );
            go.set_label(tr(if connected && login_required && installed {
                "btn.login"
            } else if installed {
                "btn.open"
            } else {
                "btn.set_up"
            }));
            super::with_icon(go);
        }
        let installed = connected || states.iter().any(|(_, _, _, known, _)| *known);
        let logged = connected && ready && !user.is_empty();
        let step = |done: bool, now: bool| match (done, now) {
            (true, _) => H::Up,
            (false, true) => H::Busy,
            _ => H::Off,
        };
        let lines = [
            (
                step(browser.is_some() || installed, browser.is_none() && !installed),
                match browser {
                    Some(b) => trf("accounts.tiktok_sending.step_open_done", &[("browser", b)]),
                    None => tr("accounts.tiktok_sending.step_open").to_string(),
                },
            ),
            (
                step(installed, browser.is_some() && !installed),
                trf(
                    "accounts.tiktok_sending.step_load",
                    &[("browser", browser.unwrap_or(tr("accounts.tiktok_sending.your_browser")))],
                ),
            ),
            (
                step(connected, installed && !connected),
                tr("accounts.tiktok_sending.step_connect").to_string(),
            ),
            (
                step(logged, connected && !logged),
                if logged {
                    trf("accounts.tiktok_sending.step_login_done", &[("user", &user)])
                } else {
                    tr("accounts.tiktok_sending.step_login").to_string()
                },
            ),
        ];
        let mut steps_h = 0;
        for ((health, text), f) in lines.into_iter().zip(steps.iter_mut()) {
            fltk::draw::set_font(f.label_font(), f.label_size());
            let wrapped = dialogs::wrap(&text, (inner - 30).max(100), |t| fltk::draw::width(t) as i32);
            let sh = (wrapped.len() as i32 * 18 + 8).max(28);
            steps_h += sh;
            col.fixed(f, sh);
            health.show(f, &wrapped.join("\n"));
            f.set_align(Align::Left | Align::Inside | Align::ImageNextToText);
            f.set_tooltip(&text.replace('@', "@@"));
        }
        let (health, text) = if logged {
            (H::Up, tr("accounts.tiktok_sending.complete").to_string())
        } else if login_required {
            (H::Standby, tr("accounts.tiktok_sending.login_needed").to_string())
        } else if !error.is_empty() {
            (H::Down, error)
        } else if connected {
            (H::Busy, tr("accounts.tiktok_sending.loading").to_string())
        } else {
            (H::Standby, tr("accounts.tiktok_sending.waiting").to_string())
        };
        fltk::draw::set_font(note.label_font(), note.label_size());
        let note_lines = dialogs::wrap(&text, (inner - 30).max(100), |t| fltk::draw::width(t) as i32);
        let note_h = (note_lines.len() as i32 * 17 + 8).max(26);
        col.fixed(&note, note_h);
        health.show(&mut note, &note_lines.join("\n"));
        note.set_align(Align::Left | Align::Inside | Align::ImageNextToText);
        let rows_in_col = states.len().max(1) as i32 + 9;
        let col_h = why_h + 22 + browsers_h + 6 + steps_h + note_h + (ROW + 2) + ROW + (rows_in_col - 1) * 6;
        let height = col_h + 8;
        let (x, y) = (win.x(), win.y());
        win.resize(x, y, width, height);
        col.resize(16, 0, inner, col_h);
        col.layout();
        for (_, _, _, _, row) in &mut states {
            row.layout();
        }
        win.resizer()(height);
        if set_up {
            remove.activate();
        } else {
            remove.deactivate();
        }
        if connected && (!ready || login_required) {
            retry.activate();
        } else {
            retry.deactivate();
        }
        if connected && login_required && !asked_login {
            asked_login = true;
            tiktok_login(cmds);
        }
    };
    update(&mut win, &mut steps);
    win.open();
    while win.shown() && answer.get().is_none() {
        if !super::nested_wait(None) {
            break;
        }
        update(&mut win, &mut steps);
    }
    win.close();
    crate::ui::changed();
}

/// Everything about the Spotify connection, in one place: the app's client ID (changing it forgets the
/// old login), connecting and disconnecting, and the local monitor that works without a login.
fn spotify_configure(cfg: &Cfg, cmds: &Cmds) {
    let (mut win, answer) = dialogs::window(640, 420, tr("accounts.spotify.setup"));
    let mut col = Flex::new(10, 10, 620, 400, None).column();
    col.set_pad(6);
    let help = Frame::default()
        .with_label(tr("accounts.spotify.help"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&help, 96);
    let row = dialogs::row(tr("accounts.spotify.client_id"));
    let mut client = Input::default();
    client.set_value(&cfg.borrow().spotify.client_id);
    row.end();
    col.fixed(&row, ROW);
    let mut links = Flex::default().row();
    links.set_pad(8);
    let mut dashboard = button(&mut links, tr("accounts.spotify.dashboard"), 140);
    dashboard.set_callback(|_| crate::oauth::open_browser("https://developer.spotify.com/dashboard"));
    let mut copy = button(&mut links, tr("btn.copy_redirect_url"), 150);
    copy.set_callback(|_| fltk::app::copy(crate::spotify::REDIRECT));
    Frame::default();
    links.end();
    col.fixed(&links, ROW);

    let mut link = Flex::default().row();
    link.set_pad(8);
    let status = Frame::default()
        .with_align(Align::Left | Align::Inside | Align::Clip)
        .ellipsis();
    let mut connect = button(&mut link, tr("btn.connect"), 110);
    let mut disconnect = button(&mut link, tr("btn.disconnect"), 110);
    link.end();
    col.fixed(&link, ROW);

    let mut monitor = CheckButton::default().with_label(tr("spotify.local_monitor"));
    col.fixed(&monitor, ROW);
    super::hint(&mut col, tr("spotify.limited_hint"), 80);
    let mut local = super::grid(&mut col, &[tr("spotify.local_check")], 1).remove(0);
    Frame::default();
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    Frame::default();
    dialogs::ok_cancel(&mut bar, &answer, true);
    bar.end();
    col.fixed(&bar, ROW);
    col.end();
    win.end();

    // What the connection is now; the buttons follow it.
    let show = {
        let cfg = cfg.clone();
        let (status, connect, disconnect, monitor) = (status, connect.clone(), disconnect.clone(), monitor.clone());
        Rc::new(move || {
            let (mut status, mut connect, mut disconnect, monitor) =
                (status.clone(), connect.clone(), disconnect.clone(), monitor.clone());
            let c = cfg.borrow();
            let on = c.spotify.enabled && !c.spotify.token.access_token.is_empty();
            let (health, text) = if on {
                (super::Health::Up, tr("conns.connected"))
            } else {
                (super::Health::Off, tr("conns.off"))
            };
            health.show(&mut status, text);
            monitor.set_checked(c.spotify.local_monitor);
            if on {
                connect.deactivate();
                disconnect.activate();
            } else {
                connect.activate();
                disconnect.deactivate();
            }
        })
    };
    show();
    monitor.set_callback({
        let (cfg, cmds) = (cfg.clone(), cmds.clone());
        move |monitor| {
            let on = monitor.is_checked();
            save(&cfg, |c| c.spotify.local_monitor = on);
            tell(&cmds, move |e| {
                if on {
                    e.spotify_local_start();
                } else {
                    e.spotify_local_stop();
                }
            });
        }
    });
    local.set_callback({
        let cmds = cmds.clone();
        move |_| {
            let (tx, rx) = std::sync::mpsc::channel();
            tell(&cmds, move |_| {
                tokio::task::spawn_local(async move {
                    let _ = tx.send(crate::spotify::local::current().await);
                });
            });
            let message = match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(Ok(Some(track))) => trf(
                    "spotify.local_found",
                    &[("artist", &track.artist), ("title", &track.title)],
                ),
                Ok(Ok(None)) => tr("spotify.local_missing").to_string(),
                Ok(Err(error)) => trf("spotify.local_error", &[("error", &error)]),
                Err(_) => tr("spotify.local_timeout").to_string(),
            };
            dialogs::message(&message);
        }
    });
    // A new client ID forgets the old login (and says so to the triggers).
    let set_client = {
        let (cfg, cmds, client) = (cfg.clone(), cmds.clone(), client.clone());
        Rc::new(move || {
            let id = client.value().trim().to_string();
            let disconnect = {
                let c = cfg.borrow();
                c.spotify.enabled && c.spotify.client_id != id
            };
            save(&cfg, |c| {
                if c.spotify.client_id != id {
                    c.spotify.client_id = id;
                    c.spotify.enabled = false;
                    c.spotify.token = Default::default();
                }
            });
            if disconnect {
                tell(&cmds, |e| crate::spotify::event(e, 99805, ""));
            }
        })
    };
    connect.set_callback({
        let (cfg, cmds, show, set_client) = (cfg.clone(), cmds.clone(), show.clone(), set_client.clone());
        move |_| {
            set_client();
            let id = cfg.borrow().spotify.client_id.clone();
            if id.trim().is_empty() {
                dialogs::message(tr("accounts.spotify.need_client_id"));
                return;
            }
            let request_id = id.clone();
            let events = cmds.clone();
            if let Some(token) = browser_login(&cmds, "Spotify", false, async move {
                let result = crate::spotify::login(&request_id).await;
                if let Err(error) = &result {
                    let error = error.clone();
                    tell(&events, move |e| crate::spotify::event(e, 99806, &error));
                }
                result
            }) {
                save(&cfg, |c| {
                    if c.spotify.client_id == id {
                        c.spotify.token = token;
                        c.spotify.enabled = true;
                    }
                });
                if cfg.borrow().spotify.enabled && cfg.borrow().spotify.client_id == id {
                    tell(&cmds, |e| crate::spotify::event(e, 99804, ""));
                }
            }
            show();
        }
    });
    disconnect.set_callback({
        let (cfg, cmds, show) = (cfg.clone(), cmds.clone(), show.clone());
        move |_| {
            save(&cfg, |c| {
                c.spotify.enabled = false;
                c.spotify.token = Default::default();
            });
            tell(&cmds, |e| crate::spotify::event(e, 99805, ""));
            show();
        }
    });
    let ok = dialogs::run(&mut win, &answer, || true);
    if ok {
        set_client();
    }
    drop(win);
}

/// Run a browser login on the engine's thread, with a dialog to cancel it; the account.
fn browser_login<T: Send + 'static>(
    cmds: &Cmds,
    what: &str,
    bot: bool,
    f: impl std::future::Future<Output = Result<T, String>> + 'static + Send,
) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    let (htx, hrx) = std::sync::mpsc::channel();
    tell(cmds, move |_| {
        let h = tokio::task::spawn_local(async move {
            let _ = tx.send(f.await);
        });
        let _ = htx.send(h.abort_handle());
    });
    let who_s = if bot {
        tr("accounts.who_bot").to_lowercase()
    } else {
        tr("accounts.who_broadcaster").to_lowercase()
    };
    let (mut win, answer) = dialogs::window(
        420,
        110,
        &trf("accounts.browser_login_title", &[("what", what), ("who", &who_s)]),
    );
    let mut col = Flex::new(10, 10, 400, 90, None).column();
    let msg = Frame::default()
        .with_label(&trf("accounts.browser_login_msg", &[("what", what), ("who", &who_s)]))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&msg, 50);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    Frame::default();
    let mut cancel = button(&mut bar, "Cancel", 80);
    cancel.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    bar.end();
    col.fixed(&bar, ROW);
    col.end();
    win.end();
    win.open();
    let mut out = None;
    while win.shown() && answer.get().is_none() {
        if let Ok(res) = rx.try_recv() {
            match res {
                Ok(a) => out = Some(a),
                Err(e) => super::dialogs::error(&trf(
                    "accounts.browser_login_failed",
                    &[("what", what), ("error", &e.to_string())],
                )),
            }
            break;
        }
        super::nested_wait(Some(0.1));
    }
    win.close();
    drop(win);
    if out.is_none() {
        // Cancelled: stop listening, so the port is free for the next try.
        if let Ok(h) = hrx.try_recv() {
            h.abort();
        }
    }
    out
}

/// The settings few people need, per platform.
fn advanced(cfg: &Cfg, p: P) {
    let c = cfg.borrow();
    // (label, value, secret)
    let fields: Vec<(&str, String, bool)> = match p {
        P::TikTok => Vec::new(),
        P::Twitch => vec![(
            tr("accounts.twitch.sub_counter_rollover"),
            c.twitch.sub_counter_rollover.to_string(),
            false,
        )],
        P::Kick => vec![
            (tr("accounts.kick.client_id"), c.kick.client_id.clone(), false),
            (tr("accounts.kick.client_secret"), c.kick.client_secret.clone(), true),
        ],
        P::YouTube => vec![
            (tr("accounts.youtube.video_id"), c.youtube.video_id.clone(), false),
            (tr("accounts.youtube.client_id"), c.youtube.client_id.clone(), false),
            (
                tr("accounts.youtube.client_secret"),
                c.youtube.client_secret.clone(),
                true,
            ),
        ],
    };
    let help = match p {
        P::Kick => trf("accounts.kick.help", &[("url", crate::kick::KICK_REDIRECT)]),
        P::YouTube => tr("accounts.youtube.help").into(),
        P::Twitch => tr("accounts.twitch.sub_counter_help").into(),
        P::TikTok => String::new(),
    };
    let auto = p.auto(&c);
    drop(c);
    let note_h = if help.is_empty() { 0 } else { 76 };
    let h = 80 + note_h + fields.len() as i32 * (ROW + 6);
    let (mut win, answer) = dialogs::window(620, h, &trf("accounts.advanced_title", &[("platform", p.name())]));
    let mut col = Flex::new(10, 10, 600, h - 20, None).column();
    col.set_pad(6);
    let f = Frame::default()
        .with_label(&help)
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&f, note_h);
    let mut start = CheckButton::default().with_label(tr("accounts.connect_on_start"));
    start.set_checked(auto);
    start.set_tooltip(tr("accounts.connect_on_start_tooltip"));
    col.fixed(&start, ROW);
    let mut inputs: Vec<Box<dyn Fn() -> String>> = Vec::new();
    for (label, value, secret) in &fields {
        let mut r = dialogs::row(label);
        if *secret {
            let i = super::fields::secret(&mut r, value);
            inputs.push(Box::new(move || i.value()));
        } else {
            let mut i = Input::default();
            i.set_value(value);
            inputs.push(Box::new(move || i.value()));
        }
        r.end();
        col.fixed(&r, ROW);
    }
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    if p == P::Kick {
        let mut b = button(&mut bar, tr("btn.copy_redirect_url"), 150);
        b.set_callback(|_| fltk::app::copy(crate::kick::KICK_REDIRECT));
    }
    dialogs::ok_cancel(&mut bar, &answer, true);
    bar.end();
    col.fixed(&bar, ROW);
    col.end();
    win.end();
    win.open();
    while win.shown() && answer.get().is_none() {
        if !super::nested_wait(None) {
            break;
        }
    }
    win.close();
    let ok = answer.get() == Some(true);
    let v: Vec<String> = if ok {
        inputs.iter().map(|i| i().trim().to_string()).collect()
    } else {
        Vec::new()
    };
    let on_start = start.is_checked();
    drop(win);
    if !ok {
        return;
    }
    save(cfg, |c| *p.manual_mut(c) = !on_start);
    save(cfg, |c| match p {
        P::Kick => {
            c.kick.client_id.clone_from(&v[0]);
            c.kick.client_secret.clone_from(&v[1]);
        }
        P::YouTube => {
            c.youtube.video_id = crate::youtube::clean_youtube_video(&v[0]);
            c.youtube.client_id.clone_from(&v[1]);
            c.youtube.client_secret.clone_from(&v[2]);
        }
        P::Twitch => c.twitch.sub_counter_rollover = v[0].parse().unwrap_or(0),
        P::TikTok => {}
    });
}

fn device_login(cfg: &Cfg, cmds: &Cmds, as_bot: bool) {
    let who = if as_bot { "Bot" } else { "Broadcaster" };
    let scopes = if as_bot {
        crate::twitch::BOT_SCOPES
    } else {
        crate::twitch::BROADCASTER_SCOPES
    };

    let (tx, rx) = std::sync::mpsc::channel();
    tell(cmds, move |_| {
        tokio::task::spawn_local(async move {
            let res = crate::twitch::device_flow_start(crate::twitch::CLIENT_ID, scopes).await;
            let _ = tx.send(res);
        });
    });

    let code_resp = match rx.recv() {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            super::dialogs::error(&trf(
                "accounts.twitch_device.start_failed",
                &[("error", &e.to_string())],
            ));
            return;
        }
        Err(_) => return,
    };

    let user_code = code_resp.user_code.clone();
    let device_code = code_resp.device_code.clone();
    let interval = code_resp.interval.max(1);

    crate::oauth::open_browser(&code_resp.verification_uri);

    let (mut win, answer) = dialogs::window(460, 220, &trf("accounts.twitch_device.title", &[("who", who)]));
    let mut col = Flex::new(10, 10, 440, 200, None).column();
    col.set_pad(8);

    let msg = trf(
        "accounts.twitch_device.msg",
        &[("code", &user_code), ("url", &code_resp.verification_uri)],
    );
    let msg_frame = Frame::default()
        .with_label(&msg)
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&msg_frame, 140);

    let mut bar = Flex::default().row();
    bar.set_pad(8);
    let mut open_btn = button(&mut bar, tr("btn.open_browser"), 110);
    open_btn.set_callback({
        let uri = code_resp.verification_uri.clone();
        move |_| crate::oauth::open_browser(&uri)
    });
    let mut copy_btn = button(&mut bar, tr("btn.copy_code"), 90);
    copy_btn.set_callback({
        let code = user_code.clone();
        move |_| fltk::app::copy(&code)
    });
    Frame::default();
    let mut cancel_btn = button(&mut bar, tr("dialogs.cancel"), 80);
    cancel_btn.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let is_cancelled = cancelled.clone();
    let cfg_clone = cfg.clone();

    tell(cmds, move |e| {
        let e = e.clone();
        tokio::task::spawn_local(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval));
            for _ in 0..(code_resp.expires_in / interval) {
                ticker.tick().await;
                if is_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match crate::twitch::device_flow_poll(crate::twitch::CLIENT_ID, &device_code).await {
                    Ok(Some(token)) => {
                        match crate::twitch::fetch_user(crate::twitch::CLIENT_ID, &token.access_token, None).await {
                            Ok(mut acct) => {
                                let expires_at = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map_or(0, |d| d.as_secs())
                                    + token.expires_in;
                                acct.token = crate::twitch::TwitchToken {
                                    access_token: token.access_token,
                                    refresh_token: token.refresh_token,
                                    scope: token.scope,
                                    expires_at,
                                };
                                let (name, user_id) = (acct.display_name.clone(), acct.user_id.clone());
                                save(&cfg_clone, |c| {
                                    if as_bot {
                                        c.twitch.bot = Some(acct.clone());
                                    } else {
                                        c.twitch.broadcaster = Some(acct.clone());
                                    }
                                });
                                if !as_bot {
                                    let mut args = Obj::new();
                                    args.insert("user".into(), name.clone().into());
                                    args.insert("userId".into(), user_id.into());
                                    e.fire(
                                        crate::twitch::T_AUTHED,
                                        "Broadcaster Authenticated",
                                        "Twitch/Connections",
                                        "twitch",
                                        &args,
                                        |_| true,
                                    );
                                    e.emit("Twitch", "BroadcasterAuthenticated", || json!({"user": name}));
                                    e.twitch_start().connect();
                                }
                                crate::say(format!("Twitch {who} logged in as {name}"));
                                let _ = done_tx.send(Ok(name));
                            }
                            Err(e) => {
                                let _ = done_tx.send(Err(e));
                            }
                        }
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        let _ = done_tx.send(Err(e));
                        break;
                    }
                }
            }
        });
    });

    win.open();
    while win.shown() && answer.get().is_none() {
        if let Ok(res) = done_rx.try_recv() {
            match res {
                Ok(name) => {
                    win.close();
                    super::dialogs::message(&trf("accounts.twitch_device.success", &[("who", who), ("name", &name)]));
                    break;
                }
                Err(e) => {
                    win.close();
                    super::dialogs::error(&trf("accounts.twitch_device.failed", &[("error", &e.to_string())]));
                    break;
                }
            }
        }
        super::nested_wait(None);
    }
    cancelled.store(true, Ordering::Relaxed);
    win.close();
    drop(win);
}

#[cfg(test)]
mod tests {
    #[test]
    fn pictures_are_looked_up_again_when_missing_old_or_expiring() {
        use super::{expiring, stale};
        let now = 1_800_000_000;
        let signed = |at: u64| format!("https://p16.tiktokcdn.com/a.jpeg?dr=1&x-expires={at}&x-signature=z");
        assert!(stale("", now, now), "a lookup that found nothing is tried again");
        assert!(!stale("https://x/a.png", now - 60, now));
        assert!(stale("https://x/a.png", now - 25 * 3600, now), "a day old");
        assert!(!stale(&signed(now + 2 * 86400), now - 60, now));
        assert!(stale(&signed(now + 60), now - 60, now), "it stops working in a minute");
        assert!(expiring(&signed(now - 1), now) && !expiring("https://x/a.png?x-expiresnot=1", now));
    }

    #[test]
    fn a_failed_lookup_is_tried_again_but_not_too_often() {
        use super::{may_look, RETRY};
        let now = 1_800_000_000;
        assert!(may_look("test:chan:retry", now));
        assert!(!may_look("test:chan:retry", now + 5));
        assert!(may_look("test:chan:retry", now + RETRY));
    }

    #[test]
    fn a_channel_has_one_picture_however_it_was_found() {
        use super::{channel_key, P};
        assert_eq!(channel_key(P::TikTok, "@UmaNanaAi "), "tiktok:chan:umananaai");
        assert_eq!(
            channel_key(P::TikTok, "umananaai"),
            channel_key(P::TikTok, "@umananaai")
        );
    }

    #[test]
    fn a_twitch_channel_from_what_was_typed() {
        assert_eq!(super::clean_twitch_channel("https://www.twitch.tv/Umanana/"), "umanana");
        assert_eq!(super::clean_twitch_channel(" @Umanana "), "umanana");
        assert_eq!(super::clean_twitch_channel(""), "");
    }

    #[test]
    fn the_channel_box_is_empty_on_your_own_channel() {
        use super::P;
        let mut c = crate::Config::default();
        c.kick.broadcaster = Some(crate::kick::KickAccount {
            username: "Ana".into(),
            ..Default::default()
        });
        // The own channel is filled in after a save, yet the box stays empty (and can be emptied).
        c.own_channels();
        assert_eq!(c.kick.channel, "ana");
        assert_eq!(P::Kick.typed_channel(&c), "");
        c.kick.channel = "bia".into();
        assert_eq!(P::Kick.typed_channel(&c), "bia");
    }
}
