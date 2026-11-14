//! The Accounts page: one section per platform (state, Connect, Advanced, the account and bot logins,
//! the channel read), the TikTok sending setup dialog and the connections dialog's Retry.
//!
//! The page and the dialog are section models; every button is `act`, every login a cancellable prompt,
//! so MCP and any frontend reach exactly the same behaviour. Credentials never enter a model: it names
//! who is logged in, never their token.

use crate::{
    automation::schema::{F, W::*},
    engine::{community::Leftover, Engine},
    i18n::{tr, trf},
};
use serde_json::{json, Value};
use std::rc::Rc;
use std::sync::Mutex;

pub const DOMAINS: &[&str] = &["accounts", "tiktok_sending"];

pub fn supports(domain: &str) -> bool {
    DOMAINS.contains(&domain)
}

#[derive(Clone, Copy, PartialEq, Debug)]
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

/// The broadcaster's and the bot's names on `platform` (`None`: nobody logged in there).
pub fn account_names(config: &crate::Config, platform: &str) -> [Option<String>; 2] {
    match P::from_slug(platform) {
        Some(p) => [p.raw_account_name(config, false), p.raw_account_name(config, true)],
        None => [None, None],
    }
}

impl P {
    fn from_slug(slug: &str) -> Option<Self> {
        ALL.into_iter().find(|p| p.slug() == slug)
    }
    fn slug(self) -> &'static str {
        match self {
            P::Twitch => "twitch",
            P::YouTube => "youtube",
            P::Kick => "kick",
            P::TikTok => "tiktok",
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

    /// The logged-in broadcaster's or bot's display name (without status notes).
    fn raw_account_name(self, c: &crate::Config, bot: bool) -> Option<String> {
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

    /// A warning on the account: a locked keyring, an expired token or new scopes.
    fn account_note(self, c: &crate::Config, bot: bool) -> Option<&'static str> {
        let locked = crate::secrets::is_locked();
        // No token left: the keyring holds it, or the login ran out.
        let gone = |locked: bool| {
            tr(if locked {
                "accounts.note.keyring_locked"
            } else {
                "accounts.note.login_expired"
            })
        };
        match self {
            P::Twitch => {
                let a = if bot { &c.twitch.bot } else { &c.twitch.broadcaster }.as_ref()?;
                let want = if bot {
                    crate::twitch::BOT_SCOPES
                } else {
                    crate::twitch::BROADCASTER_SCOPES
                };
                if a.token.access_token.is_empty() {
                    Some(gone(locked))
                } else if want.iter().any(|w| !a.token.scope.iter().any(|g| g == w)) {
                    Some(tr("accounts.note.new_permissions"))
                } else {
                    None
                }
            }
            P::YouTube => {
                let a = if bot { &c.youtube.bot } else { &c.youtube.broadcaster }.as_ref()?;
                (a.token.access_token.is_empty() && a.token.refresh_token.is_empty()).then(|| gone(locked))
            }
            P::Kick => {
                let a = if bot { &c.kick.bot } else { &c.kick.broadcaster }.as_ref()?;
                (a.token.access_token.is_empty() && a.token.refresh_token.is_empty()).then(|| gone(locked))
            }
            P::TikTok => None,
        }
    }

    /// The account's login ran out (not a locked keyring): only a new login fixes it.
    fn expired(self, c: &crate::Config, bot: bool) -> bool {
        self.account_note(c, bot) == Some(tr("accounts.note.login_expired"))
    }

    /// Forget an account; its revoke URL (Kick and Google need the token in the URL).
    fn logout(self, c: &mut crate::Config, bot: bool) -> Option<String> {
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
    fn channel(self, c: &crate::Config) -> &str {
        match self {
            P::Twitch => &c.twitch.channel,
            P::YouTube => &c.youtube.channel_id,
            P::Kick => &c.kick.channel,
            P::TikTok => &c.tiktok.channel,
        }
    }

    /// Whether the channel read is the broadcaster's own (by id, or by name ignoring case, spaces and @).
    fn own(self, c: &crate::Config) -> bool {
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

    /// What the channel shows as typed: nothing while the channel read is the broadcaster's own.
    fn typed_channel(self, c: &crate::Config) -> &str {
        if self.raw_account_name(c, false).is_some() && self.own(c) {
            ""
        } else {
            self.channel(c)
        }
    }

    /// Set the channel from what was typed (a URL or @handle works); what was kept.
    fn set_channel(self, c: &mut crate::Config, raw: &str) -> String {
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

    /// Whether it connects when Streamerz starts (it does, unless turned off).
    fn auto(self, c: &crate::Config) -> bool {
        match self {
            P::Twitch => !c.twitch.manual_connect,
            P::YouTube => !c.youtube.manual_connect,
            P::Kick => !c.kick.manual_connect,
            P::TikTok => !c.tiktok.manual_connect,
        }
    }
    fn manual_mut(self, c: &mut crate::Config) -> &mut bool {
        match self {
            P::Twitch => &mut c.twitch.manual_connect,
            P::YouTube => &mut c.youtube.manual_connect,
            P::Kick => &mut c.kick.manual_connect,
            P::TikTok => &mut c.tiktok.manual_connect,
        }
    }

    fn connect(self, e: &Rc<Engine>, on: bool) {
        match (self, on) {
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
        }
    }
}

/// Each platform's state, read on the engine's thread: connected, standby, and TikTok's status text.
fn states(e: &Rc<Engine>) -> ([bool; 4], [bool; 4], String) {
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
}

// Pictures

/// Pictures looked up this run, by key and when: a lookup that found nothing waits before the next.
static LOOKED: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());
const FRESH: u64 = 24 * 60 * 60;
const RETRY: u64 = 5 * 60;

/// Whether a signed address (`x-expires=` in it) stops working within ten minutes of `now`.
fn expiring(url: &str, now: u64) -> bool {
    url.split(['?', '&'])
        .find_map(|kv| kv.strip_prefix("x-expires="))
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|at| at <= now + 600)
}

fn stale(url: &str, at: u64, now: u64) -> bool {
    url.is_empty() || now.saturating_sub(at) >= FRESH || expiring(url, now)
}

/// Whether the lookup of `key` may start `now` (once every `RETRY` seconds at most); it is noted.
fn may_look(key: &str, now: u64) -> bool {
    let Ok(mut looked) = LOOKED.lock() else { return false };
    match looked.iter_mut().find(|(have, _)| have == key) {
        Some((_, at)) if now.saturating_sub(*at) < RETRY => false,
        Some((_, at)) => {
            *at = now;
            true
        }
        None => {
            looked.push((key.to_owned(), now));
            true
        }
    }
}

/// The account's profile picture URL, as its platform says (the login is asked; TikTok has none).
async fn avatar(engine: &Rc<Engine>, p: P, bot: bool) -> Option<String> {
    let get = |url: String, headers: Vec<(String, String)>| async move {
        let reply = crate::http::request("GET", &url, &headers, b"").await.ok()?;
        serde_json::from_slice::<Value>(&reply.body).ok()
    };
    let bearer = |token: &str| ("Authorization".to_string(), format!("Bearer {token}"));
    let cfg = engine.cfg_tx().clone();
    let found = match p {
        P::Twitch => {
            let (id, token) = {
                let config = cfg.borrow();
                let account = if bot {
                    &config.twitch.bot
                } else {
                    &config.twitch.broadcaster
                }
                .clone()?;
                (account.user_id, account.token.access_token)
            };
            if token.is_empty() {
                return None;
            }
            let headers = vec![("Client-Id".into(), crate::twitch::CLIENT_ID.into()), bearer(&token)];
            get(format!("{}/users?id={id}", crate::twitch::HELIX_URL), headers)
                .await?
                .pointer("/data/0/profile_image_url")
                .cloned()
        }
        P::Kick => {
            let token = crate::kick::token(engine, &cfg, bot, false).await.ok()?;
            get(format!("{}/users", crate::kick::KICK_API_BASE), vec![bearer(&token)])
                .await?
                .pointer("/data/0/profile_picture")
                .cloned()
        }
        P::YouTube => {
            let token = crate::youtube::token(engine, &cfg, bot, false).await.ok()?;
            let url = format!("{}/channels?part=snippet&mine=true", crate::youtube::YOUTUBE_API_BASE);
            get(url, vec![bearer(&token)])
                .await?
                .pointer("/items/0/snippet/thumbnails/default/url")
                .cloned()
        }
        P::TikTok => None,
    };
    found?.as_str().filter(|url| url.starts_with("http")).map(str::to_owned)
}

/// The file holding the account's picture, when it is on disk. The address is kept (looked up again
/// after a day, or when it is about to stop working); the picture itself comes from the images cache,
/// and the page is told when either arrives.
fn picture(engine: &Rc<Engine>, p: P, bot: bool, name: &str) -> Option<String> {
    let key = format!("{}:{bot}:{name}", p.name());
    let now = crate::fmt::unix_now().as_secs();
    let saved = avatars();
    let url = saved[&key][0].as_str().unwrap_or_default().to_owned();
    // A new login looks again at once: the wait after a failed lookup was for the old one.
    let (login, token) = login_of(&engine.config(), p, bot);
    let tail: String = token.chars().rev().take(8).collect();
    if stale(&url, saved[&key][1].as_u64().unwrap_or(0), now) && may_look(&format!("{key}:{tail}"), now) {
        let engine = engine.clone();
        tokio::task::spawn_local(async move {
            // The account's own lookup, else its public profile (no login needed).
            let found = match avatar(&engine, p, bot).await {
                Some(url) => Some(url),
                None => channel_profile(&engine, p, &login).await.0,
            };
            let Some(url) = found else {
                return;
            };
            let mut all = avatars();
            if !all.is_object() {
                all = json!({});
            }
            all[&key] = json!([url, crate::fmt::unix_now().as_secs()]);
            save_avatars(&all);
            crate::host::images_changed();
        });
    }
    super::images::lookup(&url).map(|file| file.to_string_lossy().into_owned())
}

/// An account's public name (what its profile is found by) and its access token ("" when none).
fn login_of(c: &crate::Config, p: P, bot: bool) -> (String, String) {
    match p {
        P::Twitch => if bot { &c.twitch.bot } else { &c.twitch.broadcaster }
            .as_ref()
            .map(|a| (a.login.clone(), a.token.access_token.clone())),
        P::YouTube => if bot { &c.youtube.bot } else { &c.youtube.broadcaster }
            .as_ref()
            .map(|a| (a.channel_id.clone(), a.token.access_token.clone())),
        P::Kick => if bot { &c.kick.bot } else { &c.kick.broadcaster }
            .as_ref()
            .map(|a| (a.username.clone(), a.token.access_token.clone())),
        P::TikTok => None,
    }
    .unwrap_or_default()
}

thread_local! {
    /// The kept picture addresses, read from the cache folder once (the chat asks for them on every line).
    static AVATARS: std::cell::RefCell<Option<Value>> = const { std::cell::RefCell::new(None) };
}

/// The kept picture addresses by key: `[address, when, name]`.
fn avatars() -> Value {
    AVATARS.with_borrow_mut(|kept| {
        kept.get_or_insert_with(|| crate::cache::saved("avatars").map(|saved| saved.1).unwrap_or_default())
            .clone()
    })
}

fn save_avatars(all: &Value) {
    crate::cache::save("avatars", all);
    AVATARS.with_borrow_mut(|kept| *kept = Some(all.clone()));
}

/// A channel's profile picture address and display name ("umaNanaAi"), from public profiles (it works for
/// channels that are not ours).
async fn channel_profile(engine: &Rc<Engine>, p: P, channel: &str) -> (Option<String>, Option<String>) {
    let clean = channel.trim().trim_start_matches('@');
    if clean.is_empty() {
        return (None, None);
    }
    let text = |v: &Value, pointer: &str| {
        v.pointer(pointer)
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
            .map(str::to_owned)
    };
    let link = |v: Option<String>| v.filter(|url| url.starts_with("http"));
    match p {
        P::Twitch => {
            let token = {
                let config = engine.cfg_tx().borrow();
                config
                    .twitch
                    .broadcaster
                    .as_ref()
                    .or(config.twitch.bot.as_ref())
                    .map(|account| account.token.access_token.clone())
                    .unwrap_or_default()
            };
            if !token.is_empty() {
                let headers = vec![
                    ("Client-Id".to_owned(), crate::twitch::CLIENT_ID.to_owned()),
                    ("Authorization".to_owned(), format!("Bearer {token}")),
                ];
                let url = format!(
                    "{}/users?login={}",
                    crate::twitch::HELIX_URL,
                    crate::commands::url_encode(clean)
                );
                if let Ok(reply) = crate::http::request("GET", &url, &headers, b"").await {
                    if let Ok(v) = serde_json::from_slice::<Value>(&reply.body) {
                        let found = (
                            link(text(&v, "/data/0/profile_image_url")),
                            text(&v, "/data/0/display_name"),
                        );
                        if found.0.is_some() || found.1.is_some() {
                            return found;
                        }
                    }
                }
            }
            // Without a login: twitch.tv's own public query.
            let headers = [
                ("Client-Id".to_owned(), "kimne78kx3ncx6brgo4mv6wki5h1ko".to_owned()),
                ("Content-Type".to_owned(), "application/json".to_owned()),
            ];
            let query = json!({"query": format!("{{ user(login: \"{clean}\") {{ displayName profileImageURL(width: 150) }} }}")});
            if let Ok(reply) = crate::http::request(
                "POST",
                "https://gql.twitch.tv/gql",
                &headers,
                query.to_string().as_bytes(),
            )
            .await
            {
                if let Ok(v) = serde_json::from_slice::<Value>(&reply.body) {
                    return (
                        link(text(&v, "/data/user/profileImageURL")),
                        text(&v, "/data/user/displayName"),
                    );
                }
            }
            (None, None)
        }
        P::YouTube => {
            let (_, picture, name) = crate::youtube::public_profile(clean).await;
            (picture, name)
        }
        P::Kick => {
            let headers = [("Accept".to_owned(), "application/json".to_owned())];
            let url = format!("https://kick.com/api/v2/channels/{clean}");
            let Ok(reply) = crate::http::request("GET", &url, &headers, b"").await else {
                return (None, None);
            };
            let Ok(v) = serde_json::from_slice::<Value>(&reply.body) else {
                return (None, None);
            };
            (
                link(text(&v, "/user/profile_pic").or_else(|| text(&v, "/user/profile_picture"))),
                text(&v, "/user/username"),
            )
        }
        P::TikTok => (None, None),
    }
}

/// What a channel's picture and name are kept by, however it was looked up.
fn channel_key(p: P, name: &str) -> String {
    format!(
        "{}:chan:{}",
        p.slug(),
        name.trim().trim_start_matches('@').to_lowercase()
    )
}

/// A channel's display name and picture file: kept on disk (a day), looked up in the background when
/// missing, old or about to stop working. The frontend is told when either arrives.
fn channel_card(engine: &Rc<Engine>, p: P, name: &str) -> (Option<String>, Option<String>) {
    let norm = name.trim().trim_start_matches('@').to_lowercase();
    if norm.is_empty() {
        return (None, None);
    }
    let key = channel_key(p, &norm);
    let now = crate::fmt::unix_now().as_secs();
    let saved = avatars();
    let entry = &saved[&key];
    let (url, at, known) = (
        entry[0].as_str().unwrap_or_default().to_owned(),
        entry[1].as_u64().unwrap_or(0),
        entry[2].as_str().unwrap_or_default().to_owned(),
    );
    if (stale(&url, at, now) || known.is_empty()) && may_look(&key, now) {
        let engine = engine.clone();
        tokio::task::spawn_local(async move {
            let (url, shown) = channel_profile(&engine, p, &norm).await;
            let (url, shown) = (url.unwrap_or_default(), shown.unwrap_or_default());
            // Nothing found (a bot check, no network): what was known stays and it is asked again later.
            if url.is_empty() && shown.is_empty() {
                return;
            }
            let mut all = avatars();
            if !all.is_object() {
                all = json!({});
            }
            let was = all[&key].clone();
            let keep = |new: &str, at: usize| {
                if new.is_empty() {
                    was[at].as_str().unwrap_or_default().to_owned()
                } else {
                    new.to_owned()
                }
            };
            all[&key] = json!([keep(&url, 0), crate::fmt::unix_now().as_secs(), keep(&shown, 2)]);
            save_avatars(&all);
            crate::host::images_changed();
        });
    }
    (
        Some(known).filter(|known| !known.is_empty()),
        super::images::lookup(&url).map(|file| file.to_string_lossy().into_owned()),
    )
}

/// The chat room a platform's column shows: the channel's name (the logged-in broadcaster's own when no
/// other is set) and its picture file.
pub fn room(engine: &Rc<Engine>, platform: &str) -> Value {
    let Some(p) = P::from_slug(platform) else {
        return json!({"name": "", "picture": ""});
    };
    let config = engine.config();
    let set = p.channel(&config).trim().trim_start_matches('@').to_owned();
    let mine = p.raw_account_name(&config, false);
    let own = p.own(&config);
    let name = if set.is_empty() {
        mine.clone().unwrap_or_default()
    } else {
        set
    };
    if name.is_empty() {
        return json!({"name": "", "picture": ""});
    }
    if p == P::TikTok {
        let bridge = &engine.tiktok_bridge;
        let (user, avatar) = (bridge.user(), bridge.avatar());
        let ours =
            !user.is_empty() && (config.tiktok.channel.is_empty() || user.eq_ignore_ascii_case(&config.tiktok.channel));
        let picture = if ours {
            super::images::lookup(&avatar).map(|file| file.to_string_lossy().into_owned())
        } else {
            None
        };
        return json!({"name": if ours { user } else { name }, "picture": picture.unwrap_or_default()});
    }
    // The account's own display name when the channel is its own ("umaNanaAi"); else the platform's.
    let (known, file) = channel_card(engine, p, &name);
    let shown = match (&mine, own && matches!(p, P::Twitch | P::YouTube)) {
        (Some(mine), true) if !mine.is_empty() => mine.clone(),
        _ => known.unwrap_or_else(|| mine.clone().filter(|_| own).unwrap_or_else(|| name.clone())),
    };
    let picture = if own {
        mine.as_ref().and_then(|mine| picture(engine, p, false, mine)).or(file)
    } else {
        file
    };
    json!({"name": shown, "picture": picture.unwrap_or_default()})
}

/// An account's picture file for the chat's send box (`bot` picks the slot; TikTok has the browser account).
pub fn account_picture(engine: &Rc<Engine>, platform: &str, bot: bool) -> String {
    let Some(p) = P::from_slug(platform) else {
        return String::new();
    };
    if p == P::TikTok {
        return super::images::lookup(&engine.tiktok_bridge.avatar())
            .map(|file| file.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    let config = engine.config();
    p.raw_account_name(&config, bot)
        .and_then(|name| picture(engine, p, bot, &name))
        .unwrap_or_default()
}

// Model

fn button(id: &str, label: &str, icon: &str) -> Value {
    json!({"id": id, "label": label, "icon": icon})
}
fn buttons(list: Vec<Value>) -> Value {
    json!({"type": "buttons", "buttons": list})
}
fn section(title: &str, brand: &str, rows: Vec<Value>) -> Value {
    let mut section = json!({"title": title, "icon": "", "rows": rows});
    if !brand.is_empty() {
        section["brand"] = brand.into();
        // A platform's state and buttons sit on its heading's line, all of it in a card of its own.
        section["inline"] = true.into();
        section["card"] = true.into();
    }
    section
}
fn state(health: &str, text: &str) -> Value {
    json!({"health": health, "text": text})
}

pub fn read(engine: &Rc<Engine>, domain: &str) -> Result<Value, String> {
    let sections = match domain {
        "accounts" => accounts(engine),
        "tiktok_sending" => tiktok_sending(engine),
        _ => return Err(format!("no accounts page for {domain}")),
    };
    Ok(json!({"domain": domain, "sections": sections}))
}

fn accounts(engine: &Rc<Engine>) -> Vec<Value> {
    let (on, standby, tiktok) = states(engine);
    let bridge = &engine.tiktok_bridge;
    let locked = crate::secrets::is_locked();
    let config = engine.config();
    let mut sections = Vec::new();
    if locked {
        let detail = match crate::secrets::locked_reason() {
            Some(reason) if !reason.is_empty() => trf("accounts.locked_reason", &[("reason", &reason)]),
            _ => tr("accounts.locked_default").to_string(),
        };
        sections.push(section(
            "",
            "",
            vec![json!({"status": state("locked", &detail),
                "control": buttons(vec![button("unlock", tr("accounts.unlock_logins"), "lock-open")])})],
        ));
    }
    for (i, p) in ALL.into_iter().enumerate() {
        let slug = p.slug();
        let active = on[i] || standby[i];
        let (health, text) = if on[i] {
            ("up", tr("conns.connected").to_string())
        } else if standby[i] {
            match p {
                P::TikTok if !tiktok.is_empty() => ("standby", tiktok.clone()),
                _ => ("standby", tr("accounts.standby_default").to_string()),
            }
        } else if locked && p.raw_account_name(&config, false).is_some() {
            ("locked", tr("accounts.keyring_locked_above").to_string())
        } else {
            let why = match p {
                P::Twitch if config.twitch.broadcaster.is_none() && config.twitch.channel.trim().is_empty() => {
                    tr("accounts.login_broadcaster_first").into()
                }
                P::Twitch => String::new(),
                P::TikTok if !tiktok.is_empty() => tiktok.clone(),
                p if p.channel(&config).is_empty() => tr("accounts.login_broadcaster_first").into(),
                _ => String::new(),
            };
            if why.is_empty() {
                ("off", tr("conns.off").to_string())
            } else {
                ("off", trf("accounts.off_why", &[("why", &why)]))
            }
        };
        let mut line = vec![if active {
            button(&format!("{slug}.disconnect"), tr("btn.disconnect"), "unplug")
        } else {
            button(&format!("{slug}.connect"), tr("btn.connect"), "plug")
        }];
        // The rarely needed settings, behind the gear (TikTok has none).
        if external_fields(&format!("account_{slug}")).is_some() {
            let mut settings = button("settings", tr("accounts.settings_tooltip"), "settings");
            settings["form"] = format!("account_{slug}").into();
            settings["icon_only"] = true.into();
            line.push(settings);
        }
        let mut rows = vec![json!({"status": state(health, &text), "control": buttons(line)})];
        if p != P::TikTok {
            for bot in [false, true] {
                let name = p.raw_account_name(&config, bot);
                let key = format!("{slug}.{}", if bot { "bot" } else { "account" });
                let label = tr(if bot {
                    "accounts.row_bot"
                } else {
                    "accounts.row_account"
                });
                let prefix = if bot { "bot_" } else { "" };
                let label_button = match name {
                    Some(_) if p.expired(&config, bot) => {
                        button(&format!("{slug}.{prefix}login"), tr("btn.login"), "log-in")
                    }
                    Some(_) => button(&format!("{slug}.{prefix}logout"), tr("btn.logout"), "log-out"),
                    None => button(&format!("{slug}.{prefix}login"), tr("btn.login"), "log-in"),
                };
                let mut row = json!({"key": key, "label": label, "control": buttons(vec![label_button])});
                match &name {
                    Some(name) => {
                        // A warning goes with the name: "Streamer (log in again: ...)".
                        row["value"] = match p.account_note(&config, bot) {
                            Some(note) => format!("{name} ({note})"),
                            None => name.clone(),
                        }
                        .into();
                        if let Some(file) = picture(engine, p, bot, name) {
                            row["picture"] = file.into();
                        }
                    }
                    None => {
                        row["value"] = tr("accounts.not_logged_in").into();
                        row["muted"] = true.into();
                    }
                }
                rows.push(row);
                if !bot {
                    rows.push(channel_row(engine, p, &config));
                }
            }
        } else {
            rows.push(channel_row(engine, p, &config));
            let (health, text) = sending_state(&config, bridge);
            let mut set_up = button("tiktok.sending", tr("btn.set_up"), "puzzle");
            set_up["dialog"] = "tiktok_sending".into();
            let mut sending = json!({"key": "tiktok.sending", "label": tr("accounts.row_sending"),
                "status": state(health, &text), "control": buttons(vec![set_up])});
            // The browser account's picture, as the other accounts have theirs.
            let face = account_picture(engine, "tiktok", false);
            if !face.is_empty() && !bridge.user().is_empty() {
                sending["picture"] = face.into();
            }
            rows.push(sending);
        }
        rows.push(
            json!({"key": format!("{slug}.autostart"), "label": tr("accounts.autostart"),
            "value": tr("accounts.autostart_about"), "muted": true, "control": {"type": "switch", "value": p.auto(&config)}}),
        );
        sections.push(section(p.name(), slug, rows));
    }
    sections.push(section("", "", vec![json!({"about": tr("accounts.guide")})]));
    sections
}

/// Whose chat is read: the typed channel (or "your own"), and what the logged-in account is there.
fn channel_row(engine: &Rc<Engine>, p: P, config: &crate::Config) -> Value {
    let shown = typed_channel(engine, p, config);
    // A channel that was picked shows as the platform writes it ("umaNanaAi"), with its picture.
    let room = if shown.is_empty() {
        Value::Null
    } else {
        room(engine, p.slug())
    };
    let shown = room["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .map_or(shown, str::to_owned);
    let (role, _) = crate::roles::get(&p.name().to_lowercase());
    // "Your own channel" says whose it is; another one says what the account is there.
    let whose = if shown.is_empty() || p.raw_account_name(config, false).is_none() {
        String::new()
    } else if role.moderates() {
        trf("accounts.channel_moderator", &[("role", role.name())])
    } else if role >= crate::roles::Role::Viewer {
        tr("accounts.channel_viewer").into()
    } else {
        tr("accounts.channel_theirs").into()
    };
    let mut pick = button(&format!("{}.channel", p.slug()), tr("accounts.channel_pick"), "search");
    pick["tooltip"] = tr("accounts.channel_tooltip").into();
    let mut row = json!({"key": format!("{}.channel", p.slug()), "label": tr("accounts.channel"),
        "value": if shown.is_empty() { tr("accounts.channel_placeholder").to_owned() } else { shown.clone() },
        "control": buttons(vec![pick])});
    if shown.is_empty() {
        row["muted"] = true.into();
    }
    if let Some(picture) = room["picture"].as_str().filter(|file| !file.is_empty()) {
        row["picture"] = picture.into();
    }
    if !whose.is_empty() {
        row["about"] = whose.into();
    }
    row
}

/// What the channel shows as typed: nothing while it is the account's own (TikTok's: the browser
/// account's, the one it sends as).
fn typed_channel(engine: &Rc<Engine>, p: P, config: &crate::Config) -> String {
    if p == P::TikTok {
        let user = engine.tiktok_bridge.user();
        let channel = crate::tiktok::clean_tiktok_channel(&config.tiktok.channel);
        return if !user.is_empty() && user.eq_ignore_ascii_case(&channel) {
            String::new()
        } else {
            config.tiktok.channel.clone()
        };
    }
    p.typed_channel(config).to_owned()
}

use crate::tiktok_bridge::older;

/// TikTok sending through the browser: its health and a line saying so.
fn sending_state(config: &crate::Config, bridge: &crate::tiktok_bridge::Bridge) -> (&'static str, String) {
    let (connected, user, ready, error, outdated, version) = (
        bridge.is_connected(),
        bridge.user(),
        bridge.ready(),
        bridge.error(),
        bridge.is_outdated(),
        bridge.version(),
    );
    let refused = bridge.refused();
    if !crate::tiktok_bridge::set_up(config) {
        ("off", tr("accounts.tiktok_sending.not_set_up").to_string())
    } else if !connected && !refused.is_empty() {
        ("down", refused)
    } else if !connected {
        ("standby", tr("accounts.tiktok_sending.waiting").to_string())
    } else if !error.is_empty() {
        ("off", error)
    } else if !ready {
        ("standby", tr("accounts.tiktok_sending.loading").to_string())
    } else if outdated {
        (
            "standby",
            trf(
                "accounts.tiktok_sending.outdated",
                &[
                    ("installed", &version),
                    ("latest", crate::tiktok_bridge::EXTENSION_VERSION),
                ],
            ),
        )
    } else if user.is_empty() {
        ("standby", tr("accounts.tiktok_sending.login_needed").to_string())
    } else {
        ("up", trf("accounts.tiktok_sending.connected", &[("user", &user)]))
    }
}

/// The browser the setup was started in this run.
static PICKED: Mutex<Option<String>> = Mutex::new(None);

fn picked() -> Option<String> {
    PICKED.lock().ok().and_then(|picked| picked.clone())
}

static ADVANCED_SETUP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The browser whose signed Firefox add-on is being downloaded, while it is.
static DOWNLOADING: Mutex<Option<String>> = Mutex::new(None);

fn downloading() -> Option<String> {
    DOWNLOADING.lock().ok().and_then(|d| d.clone())
}

fn tiktok_sending(engine: &Rc<Engine>) -> Vec<Value> {
    let config = engine.config();
    let bridge = &engine.tiktok_bridge;
    let (connected, user, ready, login_required, error, outdated, version) = (
        bridge.is_connected(),
        bridge.user(),
        bridge.ready(),
        bridge.login_required(),
        bridge.error(),
        bridge.is_outdated(),
        bridge.version(),
    );
    let set_up = crate::tiktok_bridge::set_up(&config);
    // Not connected: the extension's files on disk follow this Streamerz again (its address and token),
    // whatever changed them. Unchanged files are left alone.
    if set_up && !connected {
        crate::tiktok_bridge::refresh(&config);
    }
    let is_advanced = ADVANCED_SETUP.load(std::sync::atomic::Ordering::Relaxed);
    let browsers = crate::browsers::installed();
    let chosen = picked().or_else(|| crate::browsers::running_browser().map(|b| b.name.to_owned()));
    let single = browsers.len() == 1;
    let folder = crate::tiktok_bridge::folder();
    let mut rows = Vec::new();
    let mut known_any = false;
    if browsers.is_empty() {
        rows.push(json!({"status": state("off", tr("accounts.tiktok_sending.no_browser"))}));
    }
    for (index, b) in browsers.iter().enumerate() {
        let known = b.has_extension_from(crate::tiktok_bridge::EXTENSION_ID, &folder)
            || (connected && (single || chosen.as_deref() == Some(b.name)));
        known_any |= known;
        let running = b.is_running();
        let (health, text) = match (known, running) {
            (true, true) => ("up", tr("accounts.tiktok_sending.installed_running")),
            (true, false) => ("standby", tr("accounts.tiktok_sending.installed_closed")),
            (false, true) => ("off", tr("accounts.tiktok_sending.running")),
            (false, false) => ("off", tr("accounts.tiktok_sending.not_installed")),
        };
        let (label, icon) = if connected && login_required && known {
            (tr("btn.login"), "log-in")
        } else if known && !running {
            (tr("btn.open_browser"), "external-link")
        } else if known {
            (tr("btn.open"), "external-link")
        } else {
            (tr("accounts.tiktok_sending.install"), "download")
        };
        let mut row_btns = vec![button(&format!("setup:{index}"), label, icon)];
        if is_advanced && !known {
            row_btns.push(button(
                &format!("manual_setup:{index}"),
                tr("accounts.tiktok_sending.load_unpacked"),
                "folder",
            ));
        }
        let mut row = json!({
            "key": format!("browser.{index}"),
            "label": b.name,
            "brand": b.icon(),
            "status": state(health, text),
            "control": buttons(row_btns),
        });
        if is_advanced {
            // The home folder as ~, so the path fits beside the buttons.
            let path = b.data.display().to_string();
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_default();
            row["about"] = match path.strip_prefix(&home).filter(|_| !home.is_empty()) {
                Some(rest) => format!("~{rest}"),
                None => path,
            }
            .into();
        }
        rows.push(row);
    }
    let installed = connected || known_any;
    let old_store_copy = if connected {
        None
    } else {
        browsers.iter().find_map(|b| {
            let version = b.store_copy_version()?;
            // Firefox's copy comes from Streamerz itself, never a store that can lag behind.
            (!b.is_firefox_based() && older(&version, "1.3.0")).then(|| (b.name.to_owned(), version))
        })
    };
    let any_running = browsers.iter().any(|b| b.is_running());
    let logged = connected && ready && !user.is_empty();
    let enabled = |mut button: Value, yes: bool| {
        button["enabled"] = yes.into();
        button
    };
    let mut copy = button("copy_path", tr("btn.copy_path"), "copy");
    copy["copy"] = folder.display().to_string().into();

    let (health, text) = if logged && outdated {
        (
            "standby",
            trf(
                "accounts.tiktok_sending.outdated_warning",
                &[
                    ("installed", &version),
                    ("latest", crate::tiktok_bridge::EXTENSION_VERSION),
                ],
            ),
        )
    } else if logged {
        (
            "up",
            trf(
                "accounts.tiktok_sending.complete_version",
                &[("version", crate::tiktok_bridge::EXTENSION_VERSION)],
            ),
        )
    } else if login_required {
        ("standby", tr("accounts.tiktok_sending.login_needed").to_string())
    } else if !error.is_empty() {
        ("down", error)
    } else if connected {
        ("busy", tr("accounts.tiktok_sending.loading").to_string())
    } else if let Some(browser) = downloading() {
        (
            "busy",
            trf("accounts.tiktok_sending.firefox_downloading", &[("browser", &browser)]),
        )
    } else if !bridge.refused().is_empty() {
        ("down", bridge.refused())
    } else if let Some((browser, version)) = old_store_copy.clone() {
        // A store copy older than 1.3.0 has no way to learn where Streamerz is: it waits forever.
        (
            "down",
            trf(
                "accounts.tiktok_sending.store_too_old",
                &[
                    ("browser", &browser),
                    ("version", &version),
                    ("latest", crate::tiktok_bridge::EXTENSION_VERSION),
                ],
            ),
        )
    } else if installed && !any_running {
        ("standby", tr("accounts.tiktok_sending.browser_closed").to_string())
    } else if is_advanced {
        ("standby", tr("accounts.tiktok_sending.waiting").to_string())
    } else {
        ("standby", tr("accounts.tiktok_sending.waiting_simple").to_string())
    };

    if is_advanced {
        let step = |done: bool, now: bool| match (done, now) {
            (true, _) => "up",
            (false, true) => "busy",
            _ => "off",
        };
        let browser = chosen.as_deref();
        let firefox = browser.is_some_and(|name| browsers.iter().any(|b| b.name == name && b.is_firefox_based()));
        let steps = [
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
                    if firefox {
                        "accounts.tiktok_sending.step_load_firefox"
                    } else {
                        "accounts.tiktok_sending.step_load"
                    },
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

        vec![
            section(
                tr("accounts.tiktok_sending.title"),
                "tiktok",
                vec![json!({"about": tr("accounts.tiktok_sending.why")})],
            ),
            section(tr("accounts.tiktok_sending.browsers"), "", rows),
            section(
                "",
                "",
                steps
                    .into_iter()
                    .map(|(health, text)| json!({"status": state(health, &text)}))
                    .chain([json!({"status": state(health, &text)})])
                    .collect(),
            ),
            section(
                "",
                "",
                vec![json!({"control": buttons(vec![
                    enabled(button("remove", tr("btn.remove_streamerz"), "trash-2"), set_up),
                    copy,
                    enabled(button("retry", tr("btn.retry"), "refresh-cw"), connected && (!ready || login_required)),
                    button("toggle_advanced", tr("accounts.tiktok_sending.simple_mode"), "arrow-left"),
                ])})],
            ),
        ]
    } else {
        vec![
            section(
                tr("accounts.tiktok_sending.title"),
                "tiktok",
                vec![json!({"about": tr("accounts.tiktok_sending.why")})],
            ),
            section(tr("accounts.tiktok_sending.browsers"), "", rows),
            section(
                "",
                "",
                vec![
                    json!({"control": buttons(vec![
                        enabled(button("remove", tr("btn.remove_streamerz"), "trash-2"), set_up),
                        enabled(button("retry", tr("btn.retry"), "refresh-cw"), connected && (!ready || login_required)),
                        button("toggle_advanced", tr("accounts.tiktok_sending.advanced_mode"), "settings"),
                    ])}),
                    json!({"status": state(health, &text)}),
                ],
            ),
        ]
    }
}

// Advanced forms

// The labels are locale keys.
const TWITCH_ADVANCED: &[F] = &[("subCounterRollover", "accounts.field.sub_counter_rollover", Int)];
const YOUTUBE_ADVANCED: &[F] = &[
    ("videoId", "accounts.field.video_id", Text),
    ("clientId", "accounts.field.google_client_id", Text),
    ("clientSecret", "accounts.field.google_client_secret", Secret),
];
const KICK_ADVANCED: &[F] = &[
    ("clientId", "accounts.field.client_id", Text),
    ("clientSecret", "accounts.field.client_secret", Secret),
];

fn advanced_platform(kind: &str) -> Option<P> {
    P::from_slug(kind.strip_prefix("account_")?)
}

/// The fields of an Advanced form, for the shared entity form requests.
pub fn external_fields(kind: &str) -> Option<&'static [F]> {
    Some(match advanced_platform(kind)? {
        P::Twitch => TWITCH_ADVANCED,
        P::YouTube => YOUTUBE_ADVANCED,
        P::Kick => KICK_ADVANCED,
        P::TikTok => return None,
    })
}

fn advanced_item(config: &crate::Config, p: P) -> Value {
    let mut item = json!({"id": ""});
    match p {
        P::Twitch => item["subCounterRollover"] = config.twitch.sub_counter_rollover.into(),
        P::YouTube => {
            item["videoId"] = config.youtube.video_id.clone().into();
            item["clientId"] = config.youtube.client_id.clone().into();
            item["clientSecret"] = config.youtube.client_secret.clone().into();
        }
        P::Kick => {
            item["clientId"] = config.kick.client_id.clone().into();
            item["clientSecret"] = config.kick.client_secret.clone().into();
        }
        P::TikTok => {}
    }
    item
}

fn hint(p: P) -> String {
    match p {
        P::Kick => trf("accounts.kick.help", &[("url", crate::kick::KICK_REDIRECT)]),
        P::YouTube => tr("accounts.youtube.help").into(),
        P::Twitch => tr("accounts.twitch.sub_counter_help").into(),
        P::TikTok => String::new(),
    }
}

/// The Advanced form for `kind` ("account_twitch"...) with the current values.
pub fn external_form(engine: &Rc<Engine>, kind: &str) -> Option<Result<Value, String>> {
    let p = advanced_platform(kind)?;
    let fields = external_fields(kind)?;
    let item = advanced_item(&engine.config(), p);
    let object = item.as_object()?;
    // A few plain fields: no JSON to edit.
    let mut form = json!({"kind": kind, "title": trf("accounts.settings_title", &[("platform", p.name())]),
        "hint": hint(p), "new": item.clone(), "json": false,
        "fields": fields.iter().map(|(key, label, widget)| json!({
            "key": key, "label": tr(label), "widget": widget,
            "holds": widget.about(), "value": crate::automation::edits::shown(widget, object.get(*key))})).collect::<Vec<_>>()});
    form["item"] = item;
    Some(Ok(form))
}

/// Whether `platform` connects when Streamerz starts (the Autostart switch on its card).
pub fn set_autostart(engine: &Rc<Engine>, platform: &str, on: bool) -> Result<(), String> {
    let p = P::from_slug(platform).ok_or_else(|| format!("accounts has no platform {platform}"))?;
    engine.change(|config| *p.manual_mut(config) = !on);
    Ok(())
}

/// Save an Advanced form's item.
pub fn save_external(engine: &Rc<Engine>, kind: &str, item: crate::model::Obj) -> Option<Result<Value, String>> {
    let p = advanced_platform(kind)?;
    let text = |key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let rollover = item.get("subCounterRollover").and_then(Value::as_u64).unwrap_or(0);
    engine.change(|config| match p {
        P::Kick => {
            config.kick.client_id = text("clientId");
            config.kick.client_secret = text("clientSecret");
        }
        P::YouTube => {
            config.youtube.video_id = crate::youtube::clean_youtube_video(&text("videoId"));
            config.youtube.client_id = text("clientId");
            config.youtube.client_secret = text("clientSecret");
        }
        P::Twitch => config.twitch.sub_counter_rollover = rollover,
        P::TikTok => {}
    });
    Some(Ok(json!({"saved": kind})))
}

// Operations

/// Run one button of the Accounts page or the TikTok sending dialog.
pub fn act(engine: &Rc<Engine>, domain: &str, action: &str, id: &str) -> Result<Value, String> {
    let accepted = Ok(json!({"accepted": true}));
    if domain == "connections" {
        return retry(engine, action, id);
    }
    if domain == "tiktok_sending" {
        return sending(engine, action);
    }
    if action == "unlock" {
        let engine = engine.clone();
        tokio::task::spawn_local(async move {
            match engine.unlock_saved_logins(true).await {
                Ok(_) => {}
                Err(error) => super::prompts::notice(
                    tr("accounts.unlock_logins").into(),
                    trf("accounts.keyring_unlock_failed", &[("error", &error.to_string())]),
                ),
            }
            crate::host::prompts_changed();
            crate::host::changed();
        });
        return accepted;
    }
    let (slug, what) = action
        .split_once('.')
        .ok_or_else(|| format!("accounts has no action {action}"))?;
    let p = P::from_slug(slug).ok_or_else(|| format!("accounts has no platform {slug}"))?;
    match what {
        // An expired login can't connect: Connect and Logout open the platform's login page instead.
        "connect" if p.expired(&engine.config(), false) => login(engine, p, false),
        "connect" | "disconnect" => p.connect(engine, what == "connect"),
        "login" | "bot_login" => login(engine, p, what == "bot_login"),
        "logout" | "bot_logout" if p.expired(&engine.config(), what == "bot_logout") => {
            login(engine, p, what == "bot_logout")
        }
        "logout" | "bot_logout" => logout(engine, p, what == "bot_logout"),
        "channel" => return Ok(finder(engine, p)),
        "set_channel" | "own_channel" => return choose_channel(engine, p, (what == "set_channel").then_some(id)),
        _ => return Err(format!("accounts has no action {action}")),
    }
    accepted
}

/// Try a connection again: a platform, an OBS or Streamlabs Desktop connection, or a donation service.
fn retry(engine: &Rc<Engine>, action: &str, key: &str) -> Result<Value, String> {
    if action != "retry" {
        return Err(format!("connections has no action {action}"));
    }
    if let Some(p) = P::from_slug(key) {
        p.connect(engine, true);
    } else if let Some(id) = key.strip_prefix("obs:") {
        super::connections::apply(engine, "obs_connection", id, "connect")?;
    } else if let Some(id) = key.strip_prefix("slobs:") {
        super::connections::apply(engine, "slobs_connection", id, "connect")?;
    } else if matches!(key, "streamelements" | "streamlabs") {
        engine.donations_sync();
    } else {
        return Err(format!("no connection {key}"));
    }
    crate::host::changed();
    Ok(json!({"accepted": true}))
}

fn logout(engine: &Rc<Engine>, p: P, bot: bool) {
    let mut revoke = None;
    engine.change(|config| revoke = p.logout(config, bot));
    crate::emotes::load_local(&engine.config());
    if let Some(url) = revoke {
        // Best effort: the token is gone here either way.
        tokio::task::spawn_local(async move {
            let headers = [(
                "Content-Type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            )];
            // Kick reads the token from the query; Twitch and Google from a form.
            let (target, query) = match url.split_once('?') {
                Some((target, query)) if !target.contains("kick.com") => (target, query),
                _ => (url.as_str(), ""),
            };
            let reply = crate::http::request("POST", target, &headers, query.as_bytes()).await;
            let ok = reply.as_ref().is_ok_and(|reply| (200..300).contains(&reply.status));
            crate::warn(format!(
                "{}: login revoked{}",
                p.name(),
                if ok { "" } else { " (the server didn't confirm)" }
            ));
        });
    }
    if p == P::Twitch && !bot {
        p.connect(engine, false);
    }
}

/// Look at every saved login once, whether or not its platform connects (at start, and when the keyring
/// opens): one that ran out, or lacks permissions Streamerz now needs, is asked about at once instead of
/// when something first uses it. `remote` also renews the tokens about to run out, which tells a login
/// the platform turned down (off in dev mode, which shares the logins: a renewal there would spend them).
pub fn check_logins(engine: &Rc<Engine>, remote: bool) {
    if crate::secrets::is_locked() {
        return;
    }
    let config = engine.config();
    let mut present = Vec::new();
    for p in [P::Twitch, P::YouTube, P::Kick] {
        for bot in [false, true] {
            if p.raw_account_name(&config, bot).is_none() {
                continue;
            }
            match p.account_note(&config, bot) {
                Some(note) if note == tr("accounts.note.new_permissions") => {
                    crate::notify::permissions_needed(p.name(), bot)
                }
                Some(note) if note == tr("accounts.note.login_expired") => crate::notify::login_needed(p.name(), bot),
                _ => present.push((p, bot)),
            }
        }
    }
    drop(config);
    if !remote || present.is_empty() {
        return;
    }
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        let cfg = engine.config_sender();
        // Each renewal says so itself when the platform refuses it.
        for (p, bot) in present {
            match p {
                P::Twitch => crate::twitch::renew_logins(&engine, &cfg).await,
                P::YouTube => drop(crate::youtube::token(&engine, &cfg, bot, false).await),
                P::Kick => drop(crate::kick::token(&engine, &cfg, bot, false).await),
                P::TikTok => {}
            }
        }
    });
}

/// A login ran out: ask, in a dialog, whether to log in again now. "Log in" runs the same
/// login as the Accounts button.
pub fn ask_relogin(engine: &Rc<Engine>, platform: &'static str, bot: bool) {
    let Some(p) = ALL.into_iter().find(|p| p.name() == platform) else {
        return;
    };
    // A login that works but lacks what Streamerz now asks for says so, not that it ran out.
    let permissions = p.account_note(&engine.config(), bot) == Some(tr("accounts.note.new_permissions"));
    let text = trf(
        match (permissions, bot) {
            (true, false) => "relogin.ask_permissions",
            (true, true) => "relogin.ask_permissions_bot",
            (false, false) => "relogin.ask",
            (false, true) => "relogin.ask_bot",
        },
        &[("platform", platform)],
    );
    let answer = crate::host::confirm(text, tr("relogin.later"), tr("relogin.now"));
    let engine = engine.clone();
    let task = tokio::task::spawn_local(async move {
        if answer.await == Ok(true) {
            login(&engine, p, bot);
        }
    });
    ASKS.with_borrow_mut(|asks| asks.push((p, bot, task.abort_handle())));
}

thread_local! {
    /// The "log in again" questions on screen, so a login saved by another way takes its question away.
    static ASKS: std::cell::RefCell<Vec<(P, bool, tokio::task::AbortHandle)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A new login was saved: the platform may ask about it again, and its open question is stale (aborting
/// the task drops the prompt's receiver, which closes it).
fn login_saved(p: P, bot: bool) {
    crate::host::relogin_done(p.name(), bot);
    ASKS.with_borrow_mut(|asks| {
        asks.retain(|(q, b, handle)| {
            let stale = *q == p && *b == bot;
            if stale {
                handle.abort();
            }
            !stale
        })
    });
    crate::host::prompts_changed();
}

fn who(bot: bool) -> String {
    tr(if bot {
        "accounts.who_bot"
    } else {
        "accounts.who_broadcaster"
    })
    .to_lowercase()
}

/// A login is under way: the old token stays empty until it ends, so the "log in again" question is
/// not asked again meanwhile (it is released only when the new login is saved).
fn login(engine: &Rc<Engine>, p: P, bot: bool) {
    match p {
        P::Twitch => twitch_login(engine, bot),
        P::TikTok => {
            let engine = engine.clone();
            tokio::task::spawn_local(async move {
                if let Err(error) = engine.tiktok_bridge.login().await {
                    crate::warn(format!("TikTok: {error}"));
                }
            });
        }
        P::Kick => {
            let config = engine.config_sender();
            super::login::spawn(
                engine,
                "Kick",
                &who(bot),
                async move { crate::kick::login(&config, bot).await },
                move |engine, account| {
                    login_saved(P::Kick, bot);
                    engine.change(|config| {
                        *if bot {
                            &mut config.kick.bot
                        } else {
                            &mut config.kick.broadcaster
                        } = Some(account)
                    });
                    crate::emotes::load_local(&engine.config());
                },
            );
        }
        P::YouTube => {
            let config = engine.config_sender();
            super::login::spawn(
                engine,
                "YouTube",
                &who(bot),
                async move { crate::youtube::login(&config).await },
                move |engine, account| {
                    login_saved(P::YouTube, bot);
                    engine.change(|config| {
                        *if bot {
                            &mut config.youtube.bot
                        } else {
                            &mut config.youtube.broadcaster
                        } = Some(account)
                    });
                },
            );
        }
    }
}

/// Twitch's device login: a code to type on Twitch's page, polled until it is approved or cancelled.
fn twitch_login(engine: &Rc<Engine>, bot: bool) {
    let who = if bot { "Bot" } else { "Broadcaster" };
    let scopes = if bot {
        crate::twitch::BOT_SCOPES
    } else {
        crate::twitch::BROADCASTER_SCOPES
    };
    let (handle_tx, handle_rx) = tokio::sync::oneshot::channel::<tokio::task::AbortHandle>();
    let engine = engine.clone();
    let task = tokio::task::spawn_local(async move {
        let title = trf("accounts.twitch_device.title", &[("who", who)]);
        let code = match crate::twitch::device_flow_start(crate::twitch::CLIENT_ID, scopes).await {
            Ok(code) => code,
            Err(error) => {
                super::prompts::notice(
                    title,
                    trf("accounts.twitch_device.start_failed", &[("error", &error.to_string())]),
                );
                return;
            }
        };
        crate::oauth::open_browser(&code.verification_uri);
        if let Ok(handle) = handle_rx.await {
            super::prompts::wait_with(
                title.clone(),
                trf(
                    "accounts.twitch_device.msg",
                    &[("code", &code.user_code), ("url", &code.verification_uri)],
                ),
                handle,
                Some(code.verification_uri.clone()),
                Some(code.user_code.clone()),
            );
        }
        let interval = code.interval.max(1);
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval));
        let outcome = async {
            for _ in 0..(code.expires_in / interval) {
                ticker.tick().await;
                if let Some(token) =
                    crate::twitch::device_flow_poll(crate::twitch::CLIENT_ID, &code.device_code).await?
                {
                    let mut account =
                        crate::twitch::fetch_user(crate::twitch::CLIENT_ID, &token.access_token, None).await?;
                    let expires_at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs())
                        + token.expires_in;
                    account.token = crate::twitch::TwitchToken {
                        access_token: token.access_token,
                        refresh_token: token.refresh_token,
                        scope: token.scope,
                        expires_at,
                    };
                    return Ok(Some(account));
                }
            }
            Ok::<_, String>(None)
        }
        .await;
        match outcome {
            Ok(Some(account)) => {
                let (name, user_id) = (account.display_name.clone(), account.user_id.clone());
                login_saved(P::Twitch, bot);
                engine.change(|config| {
                    if bot {
                        config.twitch.bot = Some(account);
                    } else {
                        config.twitch.broadcaster = Some(account);
                    }
                });
                crate::emotes::load_local(&engine.config());
                if !bot {
                    let mut args = crate::model::Obj::new();
                    args.insert("user".into(), name.clone().into());
                    args.insert("userId".into(), user_id.into());
                    engine.fire(
                        crate::twitch::T_AUTHED,
                        "Broadcaster Authenticated",
                        "Twitch/Connections",
                        "twitch",
                        &args,
                        |_| true,
                    );
                    engine.emit("Twitch", "BroadcasterAuthenticated", || json!({"user": name}));
                    engine.twitch_start().connect();
                }
                crate::say(format!("Twitch {who} logged in as {name}"));
                super::prompts::notice(
                    title,
                    trf("accounts.twitch_device.success", &[("who", who), ("name", &name)]),
                );
            }
            Ok(None) => {}
            Err(error) => super::prompts::notice(
                title,
                trf("accounts.twitch_device.failed", &[("error", &error.to_string())]),
            ),
        }
        crate::host::prompts_changed();
        crate::host::changed();
    });
    let _ = handle_tx.send(task.abort_handle());
}

// The channel read

/// The channel finder a frontend opens for the Channel button: a search box (results come from
/// `StreamerzSearchChannels`), the channels that match with their picture, followers and whether they are
/// live, and "Use my own channel". The pick is applied with `<platform>.set_channel` (the handle, or what
/// was typed: a link or an @name) or `<platform>.own_channel`.
fn finder(engine: &Rc<Engine>, p: P) -> Value {
    let searchable = p == P::TikTok || crate::search::can(p.slug());
    let note = if searchable {
        String::new()
    } else {
        trf("find.no_search", &[("platform", p.name())])
    };
    json!({"finder": {
        "platform": p.slug(), "title": trf("find.title", &[("platform", p.name())]),
        "placeholder": tr("find.placeholder"), "searchable": searchable, "note": note,
        "typed": typed_channel(engine, p, &engine.config()),
        "labels": {"channel": tr("find.channel"), "followers": tr("find.followers"), "live": tr("find.live"),
            "empty": tr("find.empty"), "searching": tr("find.searching"), "none": tr("find.none"),
            "own": tr("find.own"), "ok": tr("dialogs.ok"), "cancel": tr("dialogs.cancel")},
    }})
}

/// Read the channel picked in the finder (`None`: the account's own). Only Twitch reads its account's
/// channel from an empty one; Kick and YouTube need the channel written (the account's username or
/// channel id), and TikTok's is the browser account's.
fn choose_channel(engine: &Rc<Engine>, p: P, picked: Option<&str>) -> Result<Value, String> {
    let own = |name: Option<String>| {
        name.filter(|name| !name.trim().is_empty())
            .ok_or_else(|| trf("find.login_first", &[("platform", p.name())]))
    };
    let channel = match picked.map(str::trim) {
        Some(picked) => picked.to_owned(),
        None => match p {
            P::TikTok => {
                let user = engine.tiktok_bridge.user();
                if user.is_empty() {
                    return Err(tr("find.tiktok_login").into());
                }
                user
            }
            P::Kick => own(engine.config().kick.broadcaster.as_ref().map(|a| a.username.clone()))?,
            P::YouTube => own(engine
                .config()
                .youtube
                .broadcaster
                .as_ref()
                .map(|a| a.channel_id.clone()))?,
            P::Twitch => String::new(),
        },
    };
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        set_channel(&engine, p, &channel, None).await;
        crate::host::changed();
    });
    Ok(json!({"accepted": true}))
}

/// Read `raw` as the platform's channel. When there was another one with saved people, `leftover` (or
/// the person, asked) decides what happens to them; the new one is read at once if the old was.
async fn set_channel(engine: &Rc<Engine>, p: P, raw: &str, leftover: Option<Leftover>) {
    let slug = p.slug();
    let before = p.channel(&engine.config()).to_string();
    let old = engine.broadcaster(slug);
    let saved = engine.community_of(slug, &old);
    engine.change(|config| drop(p.set_channel(config, raw)));
    if p.channel(&engine.config()) == before {
        return;
    }
    crate::emotes::load_local(&engine.config());
    let what = match leftover {
        Some(what) => what,
        None if saved == 0 => Leftover::Keep,
        None => {
            let options = vec![
                super::prompts::Choice {
                    id: "keep".into(),
                    label: tr("room.keep").into(),
                },
                super::prompts::Choice {
                    id: "move".into(),
                    label: tr("room.move").into(),
                },
                super::prompts::Choice {
                    id: "delete".into(),
                    label: tr("room.delete").into(),
                },
            ];
            let question = trf(
                "room.ask",
                &[("platform", p.name()), ("channel", &old), ("count", &saved.to_string())],
            );
            match super::prompts::choice(question, options).await {
                Ok(Some(id)) if id == "move" => Leftover::Move,
                Ok(Some(id)) if id == "delete" => Leftover::Delete,
                _ => Leftover::Keep,
            }
        }
    };
    engine.channel_changed(slug, &old, what);
    engine.chat_forget(slug);
    engine.chat_polls_clear(slug);
    engine.chat_unpinned(slug, "");
    // Read the new one now, if it was reading.
    let (on, standby, _) = states(engine);
    let index = ALL.iter().position(|x| *x == p).unwrap_or(0);
    if on[index] || standby[index] {
        p.connect(engine, false);
        p.connect(engine, true);
    }
}

// TikTok sending

fn sending(engine: &Rc<Engine>, action: &str) -> Result<Value, String> {
    let accepted = Ok(json!({"accepted": true}));
    let bridge = &engine.tiktok_bridge;
    match action {
        "copy_path" => {
            crate::tiktok_bridge::install(engine)?;
            accepted
        }
        "retry" => {
            if bridge.login_required() {
                tiktok_login(engine);
            } else {
                let _ = crate::tiktok_bridge::retry(engine);
            }
            accepted
        }
        "remove" => {
            let engine = engine.clone();
            tokio::task::spawn_local(async move {
                let yes = crate::host::confirm(
                    tr("accounts.tiktok_sending.remove_confirm").into(),
                    tr("dialogs.cancel"),
                    tr("btn.remove_streamerz"),
                );
                if yes.await.unwrap_or(false) {
                    crate::tiktok_bridge::remove(&engine);
                    crate::host::changed();
                }
            });
            accepted
        }
        "toggle_advanced" => {
            let curr = ADVANCED_SETUP.load(std::sync::atomic::Ordering::Relaxed);
            ADVANCED_SETUP.store(!curr, std::sync::atomic::Ordering::Relaxed);
            crate::host::changed();
            accepted
        }
        action if action.starts_with("manual_setup:") => {
            let index: usize = action
                .strip_prefix("manual_setup:")
                .and_then(|index| index.parse().ok())
                .ok_or_else(|| format!("tiktok_sending has no action {action}"))?;
            let browsers = crate::browsers::installed();
            let b = browsers.get(index).ok_or("no such browser")?;
            let mut folder = crate::tiktok_bridge::install(engine)?;
            let setup = if b.is_firefox_based() {
                // Firefox loads a folder only as a temporary add-on, with its own manifest.
                folder = crate::tiktok_bridge::write_firefox_files(&engine.config())?;
                crate::tiktok_bridge::Setup::FirefoxTemporary
            } else {
                crate::tiktok_bridge::Setup::Chromium { store: b.store_url() }
            };
            let page = crate::tiktok_bridge::write_setup_page(b.name, b.extensions_page, &folder, &setup)?;
            b.open_setup(&page, &crate::tiktok_bridge::pairing(&engine.config()))
                .map_err(|error| {
                    trf(
                        "accounts.tiktok_sending.cant_open",
                        &[("browser", b.name), ("error", &error.to_string())],
                    )
                })?;
            if let Ok(mut picked) = PICKED.lock() {
                *picked = Some(b.name.to_owned());
            }
            crate::host::changed();
            accepted
        }
        _ => {
            let index: usize = action
                .strip_prefix("setup:")
                .and_then(|index| index.parse().ok())
                .ok_or_else(|| format!("tiktok_sending has no action {action}"))?;
            let browsers = crate::browsers::installed();
            let b = browsers.get(index).ok_or("no such browser")?;
            if bridge.is_connected() && bridge.login_required() {
                tiktok_login(engine);
                return accepted;
            }
            let cant_open = |error: std::io::Error| {
                trf(
                    "accounts.tiktok_sending.cant_open",
                    &[("browser", b.name), ("error", &error.to_string())],
                )
            };
            if bridge.is_connected() {
                b.command("https://www.tiktok.com/live/history/center")
                    .spawn()
                    .map_err(cant_open)?;
            } else if b.is_firefox_based() {
                // Firefox: the signed add-on of exactly this version, downloaded, then opened next to
                // the setup page (Firefox asks to add it, and the extension pairs from that page).
                let folder = crate::tiktok_bridge::install(engine)?;
                if downloading().is_some() {
                    return accepted;
                }
                if let Ok(mut d) = DOWNLOADING.lock() {
                    *d = Some(b.name.to_owned());
                }
                let (engine, b) = (engine.clone(), b.clone());
                tokio::task::spawn_local(async move {
                    let got = crate::tiktok_bridge::firefox_xpi().await;
                    if let Ok(mut d) = DOWNLOADING.lock() {
                        *d = None;
                    }
                    crate::host::changed();
                    let opened = got.and_then(|addon| {
                        let setup = crate::tiktok_bridge::Setup::Firefox { addon: &addon };
                        let page = crate::tiktok_bridge::write_setup_page(b.name, b.extensions_page, &folder, &setup)?;
                        b.open_setup_with_addon(&page, &crate::tiktok_bridge::pairing(&engine.config()), &addon)
                            .map_err(|error| {
                                trf(
                                    "accounts.tiktok_sending.cant_open",
                                    &[("browser", b.name), ("error", &error.to_string())],
                                )
                            })
                    });
                    if let Err(error) = opened {
                        crate::warn(format!("TikTok extension: {error}"));
                        let _ = crate::host::confirm(error, tr("dialogs.ok"), "").await;
                    }
                });
            } else {
                // Not connected yet (not installed, or installed but never paired): the setup page, with
                // the pairing in its address. It also offers the store and the steps to load the copy on
                // disk; the extension closes it once it connects.
                let folder = crate::tiktok_bridge::install(engine)?;
                let setup = crate::tiktok_bridge::Setup::Chromium { store: b.store_url() };
                let page = crate::tiktok_bridge::write_setup_page(b.name, b.extensions_page, &folder, &setup)?;
                b.open_setup(&page, &crate::tiktok_bridge::pairing(&engine.config()))
                    .map_err(cant_open)?;
            }
            if let Ok(mut picked) = PICKED.lock() {
                *picked = Some(b.name.to_owned());
            }
            crate::host::changed();
            accepted
        }
    }
}

fn tiktok_login(engine: &Rc<Engine>) {
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        if let Err(error) = engine.tiktok_bridge.login().await {
            crate::warn(format!("TikTok: {error}"));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Rc<Engine> {
        crate::engine::tests::engine(json!({"actions": []}))
    }

    fn rows(model: &Value) -> Vec<Value> {
        model["sections"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|section| section["rows"].as_array().unwrap().clone())
            .collect()
    }

    #[test]
    fn pictures_are_looked_up_again_when_missing_old_or_expiring_and_not_too_often() {
        let now = 1_800_000_000;
        let signed = |at: u64| format!("https://p16.tiktokcdn.com/a.jpeg?dr=1&x-expires={at}&x-signature=z");
        assert!(stale("", now, now), "a lookup that found nothing is tried again");
        assert!(!stale("https://x/a.png", now - 60, now));
        assert!(stale("https://x/a.png", now - 25 * 3600, now), "a day old");
        assert!(!stale(&signed(now + 2 * 86400), now - 60, now));
        assert!(stale(&signed(now + 60), now - 60, now), "it stops working in a minute");
        assert!(may_look("test:accounts:retry", now));
        assert!(!may_look("test:accounts:retry", now + 5));
        assert!(may_look("test:accounts:retry", now + RETRY));
    }

    #[test]
    fn a_platform_heads_its_section_with_its_state_and_an_account_row_may_carry_a_picture() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine();
            engine.change(|config| {
                config.kick.broadcaster = Some(crate::kick::KickAccount {
                    username: "Ana".into(),
                    ..Default::default()
                })
            });
            let model = read(&engine, "accounts").unwrap();
            let sections = model["sections"].as_array().unwrap();
            assert!(sections
                .iter()
                .filter(|section| section["brand"].is_string())
                .all(|section| section["inline"] == true));
            // No saved address and no network answer yet: the row names the account and has no picture.
            let kick = sections.iter().find(|section| section["brand"] == "kick").unwrap();
            let account = kick["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["value"].as_str().unwrap_or_default().starts_with("Ana"))
                .unwrap();
            assert!(account["picture"].is_null());
            // A saved account with no token left is a login that ran out, and says so.
            assert_eq!(account["value"], format!("Ana ({})", tr("accounts.note.login_expired")));
        });
    }

    #[test]
    fn every_platform_reads_without_credentials_and_offers_login_when_logged_out() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine();
            let model = read(&engine, "accounts").unwrap();
            let sections = model["sections"].as_array().unwrap();
            let brands: Vec<_> = sections.iter().filter_map(|s| s["brand"].as_str()).collect();
            assert!(brands.starts_with(&["twitch", "youtube", "kick", "tiktok"]));
            let all = rows(&model);
            let ids: Vec<_> = all
                .iter()
                .flat_map(|row| row["control"]["buttons"].as_array().cloned().unwrap_or_default())
                .filter_map(|button| button["id"].as_str().map(str::to_owned))
                .collect();
            for id in [
                "twitch.connect",
                "twitch.login",
                "twitch.bot_login",
                "twitch.channel",
                "kick.login",
                "youtube.bot_login",
                "tiktok.channel",
                "tiktok.sending",
            ] {
                assert!(ids.contains(&id.to_owned()), "{id} in {ids:?}");
            }
            assert!(
                !ids.contains(&"tiktok.login".to_owned()),
                "TikTok logs in through its browser account"
            );
            engine.change(|config| {
                let mut account = crate::twitch::TwitchAccount {
                    display_name: "Ana".into(),
                    login: "ana".into(),
                    ..Default::default()
                };
                account.token.access_token = "secret-access".into();
                account.token.refresh_token = "secret-refresh".into();
                config.twitch.broadcaster = Some(account);
                config.kick.client_secret = "kick-secret".into();
            });
            let model = read(&engine, "accounts").unwrap();
            let text = model.to_string();
            assert!(
                !text.contains("secret-access") && !text.contains("secret-refresh") && !text.contains("kick-secret")
            );
            let all = rows(&model);
            let account = all.iter().find(|row| row["key"] == "twitch.account").unwrap();
            // The name, with what it needs said beside it (this login lacks scopes).
            assert_eq!(
                account["value"],
                format!("Ana ({})", tr("accounts.note.new_permissions"))
            );
            assert!(account["about"].is_null());
            assert_eq!(account["control"]["buttons"][0]["id"], "twitch.logout");
            // The channel box is empty on the account's own channel.
            let channel = all.iter().find(|row| row["key"] == "twitch.channel").unwrap();
            assert_eq!(channel["value"], tr("accounts.channel_placeholder"));
        });
    }

    #[test]
    fn logging_out_forgets_the_account_and_the_bot_independently() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine();
            engine.change(|config| {
                let mut account = crate::kick::KickAccount {
                    username: "Ana".into(),
                    ..Default::default()
                };
                account.token.access_token = "token".into();
                config.kick.broadcaster = Some(account.clone());
                config.kick.bot = Some(account);
            });
            act(&engine, "accounts", "kick.bot_logout", "").unwrap();
            assert!(engine.config().kick.bot.is_none() && engine.config().kick.broadcaster.is_some());
            act(&engine, "accounts", "kick.logout", "").unwrap();
            assert!(engine.config().kick.broadcaster.is_none());
            assert!(act(&engine, "accounts", "nothing", "").is_err());
            assert!(act(&engine, "accounts", "orkut.login", "").is_err());
            assert!(act(&engine, "accounts", "kick.dance", "").is_err());
        });
    }

    #[test]
    fn the_channel_is_cleaned_changed_once_and_its_leftover_people_are_kept() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine();
            set_channel(&engine, P::Twitch, "https://www.twitch.tv/Umanana/", None).await;
            assert_eq!(engine.config().twitch.channel, "umanana");
            // The same channel again changes nothing and asks nothing.
            set_channel(&engine, P::Twitch, "@Umanana", None).await;
            assert_eq!(engine.config().twitch.channel, "umanana");
            set_channel(&engine, P::Twitch, "", Some(Leftover::Keep)).await;
            assert_eq!(engine.config().twitch.channel, "");
            engine.change(|config| {
                let account = crate::kick::KickAccount {
                    username: "Ana".into(),
                    ..Default::default()
                };
                config.kick.broadcaster = Some(account);
                config.kick.channel = "ana".into();
            });
            let model = read(&engine, "accounts").unwrap();
            let channel = rows(&model)
                .into_iter()
                .find(|row| row["key"] == "kick.channel")
                .unwrap();
            assert_eq!(channel["value"], tr("accounts.channel_placeholder"));
            // "Your own channel" says it all: no note beside it.
            assert!(channel["about"].is_null());
            set_channel(&engine, P::Kick, "Bia", None).await;
            let model = read(&engine, "accounts").unwrap();
            let channel = rows(&model)
                .into_iter()
                .find(|row| row["key"] == "kick.channel")
                .unwrap();
            assert_eq!(channel["value"], "Bia");
            // TikTok's own channel is the browser account's, and says so like the others.
            engine.tiktok_bridge.test_account("pedro", "1");
            engine.change(|config| config.tiktok.channel = "@Pedro".into());
            let model = read(&engine, "accounts").unwrap();
            let channel = rows(&model)
                .into_iter()
                .find(|row| row["key"] == "tiktok.channel")
                .unwrap();
            assert_eq!(channel["value"], tr("accounts.channel_placeholder"));
            engine.change(|config| config.tiktok.channel = "bia".into());
            let model = read(&engine, "accounts").unwrap();
            let channel = rows(&model)
                .into_iter()
                .find(|row| row["key"] == "tiktok.channel")
                .unwrap();
            assert_eq!(channel["value"], "bia");
        });
    }

    #[test]
    fn settings_forms_hold_only_the_rare_settings_and_autostart_is_a_switch_on_the_card() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine();
            for kind in ["account_twitch", "account_youtube", "account_kick"] {
                let form = external_form(&engine, kind).unwrap().unwrap();
                assert_eq!(form["json"], false, "no Edit JSON for a few plain fields");
                let fields = form["fields"].as_array().unwrap();
                assert_eq!(fields.len(), external_fields(kind).unwrap().len());
                assert!(fields
                    .iter()
                    .all(|field| field["key"] != "autoConnect"
                        && !field["label"].as_str().unwrap().starts_with("accounts.")));
            }
            // TikTok has nothing rare to set: no gear on its card.
            assert!(external_form(&engine, "account_tiktok").is_none());
            assert!(external_form(&engine, "account_orkut").is_none());
            let model = read(&engine, "accounts").unwrap();
            let gears = model["sections"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|section| section["rows"][0]["control"]["buttons"][1]["icon_only"] == true)
                .count();
            assert_eq!(gears, 3);
            // Autostart is on by default, and its switch turns it off.
            let autostart = rows(&model)
                .into_iter()
                .find(|row| row["key"] == "kick.autostart")
                .unwrap();
            assert_eq!(autostart["control"]["value"], true);
            crate::core::sections::set(&engine, "accounts", "kick.autostart", &json!(false)).unwrap();
            assert!(engine.config().kick.manual_connect);
            assert!(crate::core::sections::set(&engine, "accounts", "orkut.autostart", &json!(false)).is_err());
            let item = json!({"id": "", "videoId": " https://youtu.be/abcdefghijk ", "clientId": " id ", "clientSecret": "shh"});
            save_external(&engine, "account_youtube", item.as_object().unwrap().clone())
                .unwrap()
                .unwrap();
            let config = engine.config();
            assert!(config.youtube.client_id == "id" && config.youtube.client_secret == "shh");
            assert!(!config.youtube.manual_connect, "the form leaves autostart alone");
            assert_eq!(config.youtube.video_id, "abcdefghijk");
            drop(config);
            let item = json!({"id": "", "subCounterRollover": 50});
            save_external(&engine, "account_twitch", item.as_object().unwrap().clone())
                .unwrap()
                .unwrap();
            assert_eq!(engine.config().twitch.sub_counter_rollover, 50);
            let form = external_form(&engine, "account_twitch").unwrap().unwrap();
            assert_eq!(form["fields"][0]["value"], "50");
        });
    }

    #[test]
    fn retrying_a_connection_validates_its_key() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine();
            assert!(act(&engine, "connections", "retry", "nothing").is_err());
            assert!(act(&engine, "connections", "retry", "obs:missing").is_err());
            assert!(act(&engine, "connections", "explode", "kick").is_err());
            assert!(act(&engine, "connections", "retry", "streamelements").is_ok());
        });
    }

    #[test]
    fn the_tiktok_dialog_lists_steps_and_only_enables_what_can_work() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine();
            let model = read(&engine, "tiktok_sending").unwrap();
            let sections = model["sections"].as_array().unwrap();
            assert_eq!(sections[0]["title"], tr("accounts.tiktok_sending.title"));
            let last = sections.last().unwrap()["rows"][0]["control"]["buttons"].clone();
            let enabled =
                |id: &str| last.as_array().unwrap().iter().find(|b| b["id"] == id).unwrap()["enabled"].clone();
            assert_eq!(enabled("remove"), false, "nothing to remove before it is set up");
            assert_eq!(enabled("retry"), false, "nothing to retry while the extension is away");
            assert!(act(&engine, "tiktok_sending", "setup:999", "").is_err());
            assert!(act(&engine, "tiktok_sending", "nonsense", "").is_err());
            assert!(act(&engine, "tiktok_sending", "toggle_advanced", "").is_ok());
            let advanced_model = read(&engine, "tiktok_sending").unwrap();
            assert_eq!(advanced_model["sections"].as_array().unwrap().len(), 4);
            assert!(act(&engine, "tiktok_sending", "toggle_advanced", "").is_ok());
            let simple_model = read(&engine, "tiktok_sending").unwrap();
            assert_eq!(simple_model["sections"].as_array().unwrap().len(), 3);
            assert!(
                !model.to_string().contains(&engine.config().tiktok.bridge_token)
                    || engine.config().tiktok.bridge_token.is_empty()
            );
        });
    }

    #[test]
    fn a_login_that_ran_out_asks_in_a_dialog() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine();
            let before = super::super::prompts::list().len();
            ask_relogin(&engine, "Kick", true);
            let prompts = super::super::prompts::list();
            assert_eq!(prompts.len(), before + 1);
            let asked = serde_json::to_string(&prompts.last().unwrap().content).unwrap();
            assert!(asked.contains("Kick") && asked.contains(tr("relogin.now")), "{asked}");
            // "Later" closes it and nothing else happens.
            super::super::prompts::answer(prompts.last().unwrap().id, json!(false)).unwrap();
            tokio::task::yield_now().await;
            assert_eq!(super::super::prompts::list().len(), before);
            // A platform it doesn't know asks nothing.
            ask_relogin(&engine, "Nowhere", false);
            assert_eq!(super::super::prompts::list().len(), before);
        });
    }

    #[test]
    fn saved_logins_are_checked_at_start_whether_or_not_they_connect() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            // No desktop popups from a test.
            crate::notify::configure(crate::notify::Settings {
                login: false,
                ..Default::default()
            });
            let engine = engine();
            let asked = || {
                crate::host::RELOGINS
                    .lock()
                    .map(|mut queue| std::mem::take(&mut *queue))
                    .unwrap()
            };
            let forget = || {
                for p in ALL {
                    for bot in [false, true] {
                        crate::host::relogin_done(p.name(), bot);
                    }
                }
            };
            forget();
            asked();
            engine.change(|c| {
                // Twitch: a working token with every permission; Kick: its token is gone.
                c.twitch.broadcaster = Some(crate::twitch::TwitchAccount {
                    display_name: "Ana".into(),
                    token: crate::twitch::TwitchToken {
                        access_token: "a".into(),
                        scope: crate::twitch::BROADCASTER_SCOPES
                            .iter()
                            .map(|s| s.to_string())
                            .collect(),
                        ..Default::default()
                    },
                    ..Default::default()
                });
                c.twitch.manual_connect = true;
                c.kick.broadcaster = Some(crate::kick::KickAccount {
                    username: "Ana".into(),
                    ..Default::default()
                });
            });
            check_logins(&engine, false);
            assert_eq!(asked(), vec![("Kick", false)], "asked though Kick never connected");
            // Asked once: the next check doesn't ask again until it's done.
            check_logins(&engine, false);
            assert!(asked().is_empty());
            // Permissions Streamerz now needs are asked for as such.
            engine.change(|c| c.twitch.broadcaster.as_mut().unwrap().token.scope.clear());
            check_logins(&engine, false);
            assert_eq!(asked(), vec![("Twitch", false)]);
            let before = super::super::prompts::list().len();
            ask_relogin(&engine, "Twitch", false);
            let prompt = serde_json::to_string(&super::super::prompts::list().last().unwrap().content).unwrap();
            assert!(
                prompt.contains(&trf("relogin.ask_permissions", &[("platform", "Twitch")])),
                "{prompt}"
            );
            let id = super::super::prompts::list().last().unwrap().id;
            super::super::prompts::answer(id, json!(false)).unwrap();
            tokio::task::yield_now().await;
            assert_eq!(super::super::prompts::list().len(), before);
            forget();
            crate::notify::configure(crate::notify::Settings::default());
        });
    }

    #[test]
    fn a_store_copy_before_pairing_by_link_is_told_apart() {
        assert!(older("1.2.0", "1.3.0"));
        assert!(older("1.2", "1.3.0"));
        assert!(!older("1.3.0", "1.3.0"));
        assert!(!older("1.10.0", "1.3.0"));
    }

    #[test]
    fn the_channel_button_opens_the_finder_and_a_pick_is_read() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine();
            let before = super::super::prompts::list().len();
            // The button answers with the finder (nothing asked through prompts).
            let finder = act(&engine, "accounts", "kick.channel", "").unwrap();
            assert_eq!(finder["finder"]["platform"], "kick");
            assert_eq!(finder["finder"]["searchable"], true);
            assert!(!finder["finder"]["labels"]["own"].as_str().unwrap().is_empty());
            assert_eq!(super::super::prompts::list().len(), before);
            // YouTube can't be searched: the finder says to paste a link or an @name.
            let youtube = act(&engine, "accounts", "youtube.channel", "").unwrap();
            assert_eq!(youtube["finder"]["searchable"], crate::search::can("youtube"));
            // A pick is read as the channel; "own" goes back to the account's own.
            act(&engine, "accounts", "kick.set_channel", "someone").unwrap();
            for _ in 0..5 {
                tokio::task::yield_now().await;
            }
            assert_eq!(engine.config().kick.channel, "someone");
            // Kick and YouTube need a channel to connect: "own" with no login says so and keeps it.
            assert_eq!(
                act(&engine, "accounts", "kick.own_channel", "").unwrap_err(),
                trf("find.login_first", &[("platform", "Kick")])
            );
            assert!(act(&engine, "accounts", "youtube.own_channel", "").is_err());
            assert_eq!(engine.config().kick.channel, "someone");
            // Logged in: "own" writes the account's channel, which reads as its own.
            engine.change(|c| {
                c.kick.broadcaster = Some(crate::kick::KickAccount {
                    user_id: 7,
                    username: "Mine".into(),
                    ..Default::default()
                });
                c.youtube.broadcaster = Some(crate::youtube::YouTubeAccount {
                    channel_id: "UCmine".into(),
                    name: "Me".into(),
                    ..Default::default()
                });
            });
            act(&engine, "accounts", "kick.own_channel", "").unwrap();
            act(&engine, "accounts", "youtube.own_channel", "").unwrap();
            for _ in 0..5 {
                tokio::task::yield_now().await;
            }
            assert_eq!(engine.config().kick.channel, "Mine");
            assert_eq!(engine.config().youtube.channel_id, "UCmine");
            assert_eq!(P::Kick.typed_channel(&engine.config()), "", "shown as your own channel");
            // Twitch reads its account's channel from an empty one.
            act(&engine, "accounts", "twitch.set_channel", "someone").unwrap();
            for _ in 0..5 {
                tokio::task::yield_now().await;
            }
            act(&engine, "accounts", "twitch.own_channel", "").unwrap();
            for _ in 0..5 {
                tokio::task::yield_now().await;
            }
            assert_eq!(engine.config().twitch.channel, "");
            // TikTok's own channel needs the browser account.
            assert!(act(&engine, "accounts", "tiktok.own_channel", "").is_err());
        });
    }

    #[test]
    fn the_page_dialogs_and_retry_are_reachable_through_the_mcp_requests() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = engine();
            let ask = |request: Value| crate::servers::answer(&engine, &request, true).unwrap();
            assert_eq!(
                ask(json!({"request":"StreamerzCoreView","domain":"accounts"}))["status"],
                "ok"
            );
            assert_eq!(
                ask(json!({"request":"StreamerzCoreView","domain":"tiktok_sending"}))["status"],
                "ok"
            );
            assert_eq!(
                ask(json!({"request":"StreamerzCoreAction","domain":"accounts","action":"twitch.disconnect"}))
                    ["status"],
                "ok"
            );
            assert_eq!(
                ask(
                    json!({"request":"StreamerzCoreAction","domain":"connections","action":"retry","itemId":"nothing"})
                )["status"],
                "error"
            );
            let form = ask(json!({"request":"StreamerzEntityForm","kind":"account_kick"}));
            assert_eq!(form["status"], "ok");
            let saved = ask(
                json!({"request":"StreamerzEditEntity","kind":"account_kick","changes":[{"field":0,"value":"abc"}]}),
            );
            assert_eq!(saved["status"], "ok");
            assert_eq!(engine.config().kick.client_id, "abc");
            let set =
                ask(json!({"request":"StreamerzCoreSetting","domain":"accounts","key":"kick.autostart","value":false}));
            assert_eq!(set["status"], "ok");
            assert!(engine.config().kick.manual_connect);
            let status = crate::core::status::summary(&engine);
            assert!(status["connections"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c["key"].is_string()));
        });
    }
}
