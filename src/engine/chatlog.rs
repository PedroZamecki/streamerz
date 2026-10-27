//! The chat window's side of the engine: every platform's chat (through `Engine::chat`) kept in one
//! bounded log, persisted recent history + platform history merged before "Welcome to the chat room!",
//! the emotes seen per platform, and sending to one platform or several with each platform's emote format.

use super::Engine;
use crate::commands::Chat;
use crate::model::Obj;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

pub const PLATFORMS: [&str; 4] = ["twitch", "youtube", "kick", "tiktok"];

/// Lines kept (all platforms together).
const KEEP: usize = 500;
/// Historical lines kept per platform across restarts / on room join.
const KEEP_HISTORY: usize = 100;
/// Emotes remembered per platform (the most recent).
const EMOTES: usize = 300;

fn is_false(b: &bool) -> bool {
    !*b
}

/// What a chat message's actions do.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Moderate {
    Delete,
    /// Seconds.
    Timeout(u64),
    Ban,
    /// For that many seconds (30 to 1800 on Twitch), or until the stream ends.
    Pin(Option<u64>),
    Unpin,
}

/// A platform's error as the chat shows it: what it says, without the HTTP and JSON around it.
fn plain_error(err: &str) -> String {
    let said = err
        .find('{')
        .and_then(|i| serde_json::from_str::<Value>(&err[i..]).ok())
        .and_then(|v| v["message"].as_str().map(str::to_string));
    said.unwrap_or_else(|| err.to_string())
}

/// One chat line, as the window shows it: emotes by their display name.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Line {
    #[serde(skip)]
    pub seq: u64,
    pub platform: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub msg_id: String,
    pub time: String,
    pub user: String,
    pub user_id: String,
    pub text: String,
    /// The emotes in `text`: name and picture URL.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub emotes: Vec<(String, String)>,
    /// Roles, shown as badges when no explicit badge list is present.
    #[serde(skip_serializing_if = "is_false")]
    pub broadcaster: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub moderator: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub vip: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub subscriber: bool,
    /// The badges the chatter wears, directly from the platform: (badge key or name, image URL).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<(String, String)>,
    /// The name's colour ("#RRGGBB") from the platform.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub color: String,
    /// Not a chat message: a send that failed, dev-mode output, or "Welcome to the chat room!".
    #[serde(skip_serializing_if = "is_false")]
    pub note: bool,
    /// Room join marker ("Welcome to the chat room!"): rendered without timestamp.
    #[serde(skip_serializing_if = "is_false")]
    pub welcome: bool,
    /// Pre-join message (loaded from cache or platform history; never triggers commands/actions).
    #[serde(skip_serializing_if = "is_false")]
    pub historical: bool,
    /// The message this one answers: who wrote it and what it said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<Reply>,
    /// Deleted by a moderator (or its sender timed out or banned): shown struck through.
    #[serde(skip_serializing_if = "is_false")]
    pub deleted: bool,
    /// Pinned from here, until when (ms since the epoch; u64::MAX: until the stream ends).
    #[serde(skip)]
    pub pinned: Option<u64>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Reply {
    pub user: String,
    pub text: String,
}

impl Reply {
    /// From a chat's args: Twitch's `reply.*` or Kick's `replyTo`/`replyToMessage`.
    pub fn from_args(args: &crate::model::Obj) -> Option<Reply> {
        let s = |k: &str| args.get(k).and_then(Value::as_str).filter(|v| !v.is_empty());
        let (user, text) = match (s("reply.userName"), s("reply.msgBody")) {
            (Some(u), Some(t)) => (u, t.replace(r"\s", " ")),
            _ => (s("replyTo")?, s("replyToMessage").unwrap_or("").to_string()),
        };
        // Kick's emotes as their names.
        let text = text.split(' ').map(shown).collect::<Vec<_>>().join(" ");
        Some(Reply {
            user: user.to_string(),
            text,
        })
    }

    /// Twitch starts a reply with "@parent ": shown above it instead.
    pub fn strip(&self, text: &str) -> String {
        let t = text.trim_start();
        match t.strip_prefix('@').and_then(|r| r.split_once(' ')) {
            Some((who, rest)) if who.eq_ignore_ascii_case(&self.user) => rest.to_string(),
            _ => text.to_string(),
        }
    }
}

#[derive(Default)]
pub struct Log {
    lines: VecDeque<Line>,
    seq: u64,
    /// Incremented when room history / welcome lines are spliced in so UI panes resync cleanly.
    reset_gen: u64,
    /// Per platform, each emote's raw token (Twitch "Kappa", YouTube ":yt:", Kick "[emote:1:KEKW]") and
    /// picture URL.
    emotes: [VecDeque<(String, String)>; 4],
    /// What each chatter's messages said about them, by (platform, user id).
    users: HashMap<(String, String), Seen>,
    /// What the platforms said about chatters (user cards), and when: reopened at once for a while.
    cards: HashMap<(String, String), (std::time::Instant, Facts)>,
}

/// Label and value rows about a chatter.
type Facts = Vec<(String, String)>;

/// How long a user card's lookup is reused.
const CARD_FOR: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// A chatter, as seen in chat this session.
#[derive(Clone, Default)]
pub struct Seen {
    /// Their latest message's arguments about them (userName, isSubscribed, pronouns...).
    pub args: Obj,
    pub messages: u32,
    /// When they first chatted ("HH:mm"), and the log's seq of their latest message.
    pub first: String,
    last: u64,
}

/// Format Unix seconds into local `"HH:mm"`.
pub fn local_hhmm(unix_secs: i64) -> String {
    let off = i64::from(crate::fmt::now().offset);
    let t = super::civil_time(unix_secs.saturating_add(off));
    format!("{:02}:{:02}", t.hour, t.min)
}

/// Whether an argument is about the user rather than the message.
fn about_user(k: &str) -> bool {
    const ABOUT: [&str; 18] = [
        "user",
        "isSubscribed",
        "isModerator",
        "isVip",
        "isVerified",
        "isMember",
        "isSponsor",
        "isOwner",
        "isFollow",
        "pronoun",
        "follow",
        "gifter",
        "member",
        "months",
        "subscri",
        "tier",
        "profile",
        "team",
    ];
    ABOUT.iter().any(|p| k.starts_with(p))
}

fn index(platform: &str) -> Option<usize> {
    PLATFORMS.iter().position(|p| *p == platform)
}

/// What an emote token shows as: Kick's "[emote:37226:KEKW]" is "KEKW"; the others as they are.
pub fn shown(token: &str) -> &str {
    token
        .strip_prefix("[emote:")
        .and_then(|t| t.strip_suffix(']'))
        .and_then(|t| t.split_once(':'))
        .map_or(token, |x| x.1)
}

/// `text` written for `platform`: Kick emotes as Kick's tags (from `known` or Kick's loaded emote
/// catalog), Kick tags pasted for another platform as their names. Unicode emoji go as they are.
pub fn for_platform(text: &str, platform: &str, kick_known: &[String]) -> String {
    text.split(' ')
        .map(|w| {
            if platform == "kick" {
                kick_known
                    .iter()
                    .find(|k| shown(k) == w && *k != w)
                    .cloned()
                    .or_else(|| crate::ui::emotes::kick_tag(w))
                    .unwrap_or_else(|| w.to_string())
            } else {
                shown(w).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn same_msg(a: &Line, b: &Line) -> bool {
    if !a.msg_id.is_empty() && !b.msg_id.is_empty() {
        return a.msg_id == b.msg_id;
    }
    a.platform == b.platform
        && a.user.eq_ignore_ascii_case(&b.user)
        && a.text == b.text
        && (a.time == b.time || a.time.is_empty() || b.time.is_empty())
}

impl Log {
    fn push(&mut self, mut l: Line) {
        self.seq += 1;
        l.seq = self.seq;
        if self.lines.len() == KEEP {
            self.lines.pop_front();
        }
        self.lines.push_back(l);
    }

    fn remember_emote(&mut self, i: usize, token: String, url: String) {
        let seen = &mut self.emotes[i];
        if let Some(at) = seen.iter().position(|x| x.0 == token) {
            seen.remove(at);
        } else if seen.len() == EMOTES {
            seen.pop_front();
        }
        seen.push_back((token, url));
    }
}

/// Clean emote tokens inside `text` for non-Twitch platforms and convert `raw_emotes` to display names.
pub fn normalize_line_emotes(
    platform: &str,
    mut text: String,
    raw_emotes: &[(String, String)],
) -> (String, Vec<(String, String)>) {
    for e in raw_emotes {
        if platform != "twitch" {
            // Spaced, so "[emote:1:A][emote:1:A]" or ":a::b:" shows as separate emotes.
            text = text.replace(e.0.as_str(), &format!(" {} ", shown(&e.0)));
        }
    }
    if platform != "twitch" && !raw_emotes.is_empty() {
        text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    let emotes = raw_emotes
        .iter()
        .map(|e| (shown(&e.0).to_string(), e.1.clone()))
        .collect();
    (text, emotes)
}

impl Engine {
    /// Load persisted chat history from `cache/chat-history.json` at startup (marked historical).
    pub fn load_chat_history(&self) {
        let Some((_, v)) = crate::ui::img::saved("chat-history") else {
            return;
        };
        let Ok(saved) = serde_json::from_value::<Vec<Line>>(v) else {
            return;
        };
        let mut st = self.st.borrow_mut();
        let log = &mut st.chat_log;
        for mut l in saved {
            if l.note || l.welcome || index(&l.platform).is_none() {
                continue;
            }
            l.historical = true;
            if let Some(i) = index(&l.platform) {
                for (name, url) in &l.emotes {
                    let tok = if l.platform == "kick" {
                        crate::ui::emotes::kick_tag(name).unwrap_or_else(|| name.clone())
                    } else {
                        name.clone()
                    };
                    log.remember_emote(i, tok, url.clone());
                }
            }
            log.push(l);
        }
    }

    /// Persist real chat lines (excluding notes/welcome markers) to `cache/chat-history.json`.
    pub fn save_chat_history(&self) {
        let st = self.st.borrow();
        let mut out: Vec<&Line> = Vec::new();
        for p in PLATFORMS {
            let plat_lines: Vec<&Line> = st
                .chat_log
                .lines
                .iter()
                .filter(|l| l.platform == p && !l.note && !l.welcome)
                .collect();
            let start = plat_lines.len().saturating_sub(KEEP_HISTORY);
            out.extend(&plat_lines[start..]);
        }
        if let Ok(v) = serde_json::to_value(&out) {
            crate::ui::img::save("chat-history", &v);
        }
    }

    /// Merge `fetched` recent messages for `platform` (oldest-to-newest) with any cached historical
    /// lines, then place `"Welcome to the chat room!"` right before live session messages.
    pub fn chat_welcome(&self, platform: &str, fetched: Vec<Line>) {
        let Some(p_idx) = index(platform) else { return };
        {
            let mut st = self.st.borrow_mut();
            let log = &mut st.chat_log;

            let mut other_or_live_before: Vec<Line> = Vec::new();
            let mut old_hist: Vec<Line> = Vec::new();
            let mut live_current: Vec<Line> = Vec::new();

            for l in log.lines.drain(..) {
                if l.platform != platform {
                    other_or_live_before.push(l);
                } else if l.welcome {
                    // Replace previous welcome divider on reconnect; previous live lines become history.
                    old_hist.append(&mut live_current);
                } else if l.note {
                    live_current.push(l);
                } else if l.historical {
                    old_hist.push(l);
                } else {
                    live_current.push(l);
                }
            }

            // Merge old_hist + fetched (preferring fetched metadata like fresh badge URLs when same msg).
            let mut merged: Vec<Line> = Vec::new();
            if fetched.is_empty() {
                merged = old_hist;
            } else {
                // Keep older cached lines that precede fetched or aren't in fetched, then fetched in order.
                for old in old_hist {
                    if !fetched.iter().any(|f| same_msg(&old, f)) && !live_current.iter().any(|lv| same_msg(&old, lv)) {
                        merged.push(old);
                    }
                }
                for mut f in fetched {
                    f.platform = platform.to_string();
                    f.historical = true;
                    f.note = false;
                    f.welcome = false;
                    for (name, url) in &f.emotes {
                        let tok = if platform == "kick" {
                            crate::ui::emotes::kick_tag(name).unwrap_or_else(|| name.clone())
                        } else {
                            name.clone()
                        };
                        log.remember_emote(p_idx, tok, url.clone());
                    }
                    if !merged.iter().any(|m| same_msg(m, &f)) && !live_current.iter().any(|lv| same_msg(lv, &f)) {
                        merged.push(f);
                    }
                }
            }
            if merged.len() > KEEP_HISTORY {
                let drop_n = merged.len() - KEEP_HISTORY;
                merged.drain(..drop_n);
            }

            for l in other_or_live_before {
                log.push(l);
            }
            for mut l in merged {
                l.historical = true;
                log.push(l);
            }
            log.push(Line {
                platform: platform.to_string(),
                user: "streamerz".into(),
                text: "Welcome to the chat room!".into(),
                note: true,
                welcome: true,
                ..Default::default()
            });
            for l in live_current {
                log.push(l);
            }
            log.reset_gen += 1;
        }
        self.save_chat_history();
        crate::ui::chat();
    }

    /// Keep a chat message for the window (called by `chat`).
    pub(super) fn log_chat(&self, c: &Chat) {
        {
            let mut st = self.st.borrow_mut();
            let log = &mut st.chat_log;
            let Some(i) = index(&c.platform) else { return };
            for e in &c.emotes {
                log.remember_emote(i, e.0.clone(), e.1.clone());
            }
            let (text, emotes) = normalize_line_emotes(&c.platform, c.text.clone(), &c.emotes);
            let time = crate::fmt::date(&crate::fmt::now(), "HH:mm");
            let key = (c.platform.clone(), c.user_id.clone());
            if !log.users.contains_key(&key) && log.users.len() == KEEP {
                if let Some(old) = log.users.iter().min_by_key(|u| u.1.last).map(|u| u.0.clone()) {
                    log.users.remove(&old);
                }
            }
            let seen = log.users.entry(key).or_insert_with(|| Seen {
                first: time.clone(),
                ..Default::default()
            });
            seen.messages += 1;
            seen.last = log.seq + 1;
            seen.args = c
                .args
                .iter()
                .filter(|(k, v)| about_user(k) && !v.is_array() && !v.is_object())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let reply = Reply::from_args(&c.args);
            let text = reply.as_ref().map_or(text.clone(), |r| r.strip(&text));
            let mut line = Line {
                platform: c.platform.clone(),
                msg_id: c.msg_id.clone(),
                time,
                user: c.user.clone(),
                user_id: c.user_id.clone(),
                reply,
                text,
                emotes,
                broadcaster: c.broadcaster,
                moderator: c.moderator,
                vip: c.vip,
                subscriber: c.subscriber,
                color: c.args.get("color").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                badges: c
                    .args
                    .get("badges")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|b| {
                        let k = b["key"].as_str().or_else(|| b["name"].as_str())?;
                        Some((k.to_string(), b["imageUrl"].as_str().unwrap_or("").to_string()))
                    })
                    .collect(),
                ..Default::default()
            };
            // Deduplicate if a message arrived both in pre-join history and right as the live socket opened.
            if log
                .lines
                .iter()
                .rev()
                .take(20)
                .any(|prev| !prev.note && same_msg(prev, &line))
            {
                return;
            }
            line.historical = false;
            log.push(line);
        }
        self.save_chat_history();
        crate::ui::chat();
    }

    /// A line from streamerz itself (a failed send, a dev-mode send).
    fn chat_note(&self, platform: &str, text: String) {
        self.st.borrow_mut().chat_log.push(Line {
            platform: platform.into(),
            time: crate::fmt::date(&crate::fmt::now(), "HH:mm"),
            user: "streamerz".into(),
            text,
            note: true,
            ..Default::default()
        });
        crate::ui::chat();
    }

    /// Whether `platform`'s chat is in the log.
    pub fn chat_has(&self, platform: &str) -> bool {
        self.st
            .borrow()
            .chat_log
            .lines
            .iter()
            .any(|l| l.platform == platform && !l.note)
    }

    /// Current `(reset_gen, lines)` after `seq` (returns full log if `gen != client_gen`).
    pub fn chat_sync(&self, client_gen: u64, seq: u64) -> (u64, bool, Vec<Line>) {
        let st = self.st.borrow();
        let g = st.chat_log.reset_gen;
        if g != client_gen {
            (g, true, st.chat_log.lines.iter().cloned().collect())
        } else {
            (
                g,
                false,
                st.chat_log.lines.iter().filter(|l| l.seq > seq).cloned().collect(),
            )
        }
    }

    /// Re-resolve badge URLs on existing lines of `platform` (e.g. once Twitch Helix badge sets finish seeding).
    pub fn refresh_chat_badges(&self, platform: &str, mut resolve: impl FnMut(&str) -> Option<String>) {
        let mut changed = false;
        {
            let mut st = self.st.borrow_mut();
            for l in st
                .chat_log
                .lines
                .iter_mut()
                .filter(|l| l.platform == platform && !l.note)
            {
                for (name, url) in &mut l.badges {
                    if url.is_empty() {
                        if let Some(u) = resolve(name) {
                            *url = u;
                            changed = true;
                        }
                    }
                }
            }
            if changed {
                st.chat_log.reset_gen += 1;
            }
        }
        if changed {
            self.save_chat_history();
            crate::ui::chat();
        }
    }

    /// What chat said about a chatter this session.
    pub fn chatter(&self, platform: &str, user_id: &str) -> Option<Seen> {
        let st = self.st.borrow();
        st.chat_log
            .users
            .get(&(platform.to_string(), user_id.to_string()))
            .cloned()
    }

    /// What the platform says about a chatter (for the chat's user card): label and value. Twitch and
    /// Kick are asked; YouTube and TikTok tell only what their chat messages carry.
    pub async fn chatter_lookup(self: &Rc<Self>, platform: &str, user_id: &str) -> Vec<(String, String)> {
        let key = (platform.to_string(), user_id.to_string());
        if let Some((at, found)) = self.st.borrow().chat_log.cards.get(&key) {
            if at.elapsed() < CARD_FOR {
                return found.clone();
            }
        }
        let found = self.chatter_ask(platform, user_id).await;
        if !found.is_empty() {
            let cards = &mut self.st.borrow_mut().chat_log.cards;
            if cards.len() >= 200 {
                cards.clear();
            }
            cards.insert(key, (std::time::Instant::now(), found.clone()));
        }
        found
    }

    async fn chatter_ask(self: &Rc<Self>, platform: &str, user_id: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut add = |k: &str, v: &Value| {
            let v = match v {
                Value::String(s) => s.clone(),
                Value::Null => return,
                v => v.to_string(),
            };
            if !v.is_empty() {
                out.push((k.to_string(), v));
            }
        };
        match platform {
            "twitch" => {
                let Some(tw) = self.twitch() else {
                    return out;
                };
                let id = user_id;
                let get = |path: &'static str, q: Vec<(&'static str, String)>| {
                    let tw = tw.clone();
                    async move {
                        let q: Vec<(&str, &str)> = q.iter().map(|(k, v)| (*k, v.as_str())).collect();
                        tw.helix("GET", path, q, None, false, true)
                            .await
                            .map(|v| v["data"][0].clone())
                            .unwrap_or_default()
                    }
                };
                let me = || "{broadcaster_id}".to_string();
                let (u, f, s, c) = tokio::join!(
                    get("users", vec![("id", id.into())]),
                    get(
                        "channels/followers",
                        vec![("broadcaster_id", me()), ("user_id", id.into())]
                    ),
                    get("subscriptions", vec![("broadcaster_id", me()), ("user_id", id.into())]),
                    get("channels", vec![("broadcaster_id", id.into())]),
                );
                add("Picture", &u["profile_image_url"]);
                add("Account created", &u["created_at"]);
                add("Type", &u["broadcaster_type"]);
                add("About", &u["description"]);
                let since = if f.is_object() {
                    f["followed_at"].clone()
                } else {
                    "not following".into()
                };
                add("Following since", &since);
                if s.is_object() {
                    let tier = s["tier"].as_str().unwrap_or("1000").trim_end_matches("000").to_string();
                    add("Subscribed", &format!("tier {tier}").into());
                    if s["is_gift"] == true {
                        add("Gifted by", &s["gifter_name"]);
                    }
                }
                add("Their last category", &c["game_name"]);
                add("Their stream title", &c["title"]);
            }
            "kick" => {
                let Some(k) = self.kick() else { return out };
                let (users, channels) = (
                    format!("/users?id={user_id}"),
                    format!("/channels?broadcaster_user_id={user_id}"),
                );
                let (u, c) = tokio::join!(k.api("GET", &users, None), k.api("GET", &channels, None));
                let (u, c) = (u.unwrap_or_default(), c.unwrap_or_default());
                add("Picture", &u["data"][0]["profile_picture"]);
                let c = &c["data"][0];
                add("Channel", &c["slug"]);
                add("Their stream title", &c["stream_title"]);
                add("Their category", &c["category"]["name"]);
                if c["stream"]["is_live"] == true {
                    add("Live now", &c["stream"]["viewer_count"]);
                }
            }
            _ => {}
        }
        out
    }

    /// The people who chatted on `platform` this session (newest first): name and id.
    pub fn chatters(&self, platform: &str) -> Vec<(String, String)> {
        let st = self.st.borrow();
        let mut seen = Vec::new();
        for l in st
            .chat_log
            .lines
            .iter()
            .rev()
            .filter(|l| l.platform == platform && !l.note)
        {
            if !seen.iter().any(|(_, id): &(String, String)| *id == l.user_id) {
                seen.push((l.user.clone(), l.user_id.clone()));
            }
        }
        seen
    }

    /// The emotes seen on `platform`, most recent first: display name and picture URL.
    pub fn chat_emotes(&self, platform: &str) -> Vec<(String, String)> {
        let st = self.st.borrow();
        let i = index(platform).unwrap_or(0);
        let mut v: Vec<(String, String)> = st.chat_log.emotes[i]
            .iter()
            .rev()
            .map(|e| (shown(&e.0).to_string(), e.1.clone()))
            .collect();
        v.dedup();
        v
    }

    /// Whether a platform is reading chat.
    pub fn chat_connected(&self, platform: &str) -> bool {
        match platform {
            "twitch" => self.twitch_connected(),
            "youtube" => self.youtube_connected(),
            "kick" => self.kick_connected(),
            "tiktok" => self.tiktok_connected(),
            _ => false,
        }
    }

    /// Send `text` to `platform`, or to every connected one ("all"), as the bot (falling back to the
    /// broadcaster) or the broadcaster. Failures show up in the chat log. Where it went.
    pub fn send_chat(self: &Rc<Self>, platform: &str, text: &str, bot: bool) -> Vec<&'static str> {
        let to: Vec<&'static str> = PLATFORMS
            .into_iter()
            .filter(|p| {
                if platform == "all" {
                    self.chat_connected(p)
                } else {
                    *p == platform
                }
            })
            .collect();
        self.send_chat_list(&to, text, bot, true, None)
    }

    /// Drops `platform`'s line `msg_id`, or all of `user_id`'s lines: deleted, banned or timed out.
    pub fn chat_remove(&self, platform: &str, msg_id: &str, user_id: &str) {
        // Struck through where the broadcaster moderates (their own channel); elsewhere, like the
        // platforms show it to viewers, the text goes.
        let own = self.own_channel(platform);
        let mut st = self.st.borrow_mut();
        let log = &mut st.chat_log;
        let mut changed = false;
        for l in log.lines.iter_mut().filter(|l| {
            l.platform == platform
                && !l.note
                && !l.deleted
                && ((!msg_id.is_empty() && l.msg_id == msg_id) || (!user_id.is_empty() && l.user_id == user_id))
        }) {
            l.deleted = true;
            if !own {
                l.text = "message deleted".into();
                l.emotes.clear();
            }
            changed = true;
        }
        if changed {
            log.reset_gen += 1;
            drop(st);
            self.save_chat_history();
            crate::ui::chat();
        }
    }

    /// `msg_id` pinned for `secs` (none: until the stream ends); "" unpins. One pinned at a time.
    fn chat_pin(&self, platform: &str, msg_id: &str, secs: Option<u64>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let until = secs.map_or(u64::MAX, |s| now + s * 1000);
        let mut st = self.st.borrow_mut();
        for l in st.chat_log.lines.iter_mut().filter(|l| l.platform == platform) {
            l.pinned = (!msg_id.is_empty() && l.msg_id == msg_id).then_some(until);
        }
        st.chat_log.reset_gen += 1;
        drop(st);
        crate::ui::chat();
    }

    /// Whether `platform`'s chat is the broadcaster's own channel (not another one set in Advanced).
    fn own_channel(&self, platform: &str) -> bool {
        let c = self.cfg.borrow();
        match platform {
            "kick" => c
                .kick
                .broadcaster
                .as_ref()
                .is_none_or(|b| b.username.eq_ignore_ascii_case(&c.kick.channel)),
            "youtube" => c
                .youtube
                .broadcaster
                .as_ref()
                .is_none_or(|b| b.channel_id == c.youtube.channel_id),
            "tiktok" => c.tiktok.broadcaster.as_ref().is_none_or(|b| {
                b.username
                    .trim_start_matches('@')
                    .eq_ignore_ascii_case(&c.tiktok.channel)
            }),
            _ => true,
        }
    }

    /// Deletes `l`, or times out or bans its sender, as the broadcaster; what failed goes in the chat.
    pub fn moderate(self: &Rc<Self>, l: &Line, act: Moderate) {
        let what = match act {
            Moderate::Delete => format!("/delete {}", l.msg_id),
            Moderate::Timeout(s) => format!("/timeout {} {s}", l.user),
            Moderate::Ban => format!("/ban {}", l.user),
            Moderate::Pin(_) => format!("/pin {}", l.msg_id),
            Moderate::Unpin => format!("/unpin {}", l.msg_id),
        };
        if self.dev_out(&l.platform, &what, false) {
            self.chat_note(&l.platform, format!("(dev, not sent) {what}"));
            match act {
                Moderate::Delete => self.chat_remove(&l.platform, &l.msg_id, ""),
                Moderate::Pin(s) => self.chat_pin(&l.platform, &l.msg_id, s),
                Moderate::Unpin => self.chat_pin(&l.platform, "", None),
                _ => {}
            }
            return;
        }
        let (e, l) = (self.clone(), l.clone());
        tokio::task::spawn_local(async move {
            let ban = |secs: Option<u64>| {
                let mut b = serde_json::json!({"user_id": l.user_id});
                if let Some(s) = secs {
                    b["duration"] = s.into();
                }
                serde_json::json!({ "data": b })
            };
            let twitch = |method, path, q: Vec<(&'static str, String)>, body| {
                let t = e.twitch();
                async move {
                    let Some(t) = t else {
                        return Err("Twitch isn't connected".to_string());
                    };
                    let mut query = vec![
                        ("broadcaster_id", "{broadcaster_id}"),
                        ("moderator_id", "{broadcaster_id}"),
                    ];
                    query.extend(q.iter().map(|(k, v)| (*k, v.as_str())));
                    t.helix(method, path, query, body, false, true).await.map(drop)
                }
            };
            let id = || vec![("message_id", l.msg_id.clone())];
            let pin = |secs: Option<u64>| {
                let mut q = id();
                q.extend(secs.map(|s| ("duration_seconds", s.to_string())));
                q
            };
            let r = match (l.platform.as_str(), act) {
                ("twitch", Moderate::Delete) => twitch("DELETE", "moderation/chat", id(), None).await,
                ("twitch", Moderate::Timeout(s)) => twitch("POST", "moderation/bans", vec![], Some(ban(Some(s)))).await,
                ("twitch", Moderate::Ban) => twitch("POST", "moderation/bans", vec![], Some(ban(None))).await,
                ("twitch", Moderate::Pin(secs)) => match twitch("PUT", "chat/pins", pin(secs), None).await {
                    // Already pinned: its time changes instead.
                    Err(err) if err.contains("409") => twitch("PATCH", "chat/pins", pin(secs), None).await,
                    r => r,
                },
                ("twitch", Moderate::Unpin) => twitch("DELETE", "chat/pins", id(), None).await,
                ("kick", Moderate::Delete) => crate::kick::delete_message(&e, &e.cfg, &l.msg_id).await,
                ("kick", Moderate::Timeout(s)) => e.kick_start().timeout_user(&l.user, s, "").await,
                ("kick", Moderate::Ban) => e.kick_start().ban_user(&l.user, "").await,
                ("youtube", Moderate::Timeout(s)) => e.youtube_start().timeout_user("", &l.user_id, s).await,
                ("youtube", Moderate::Ban) => e.youtube_start().ban_user("", &l.user_id).await,
                _ => Err("not possible on this platform".into()),
            };
            match r {
                Ok(()) => match act {
                    Moderate::Delete => e.chat_remove(&l.platform, &l.msg_id, ""),
                    Moderate::Pin(s) => e.chat_pin(&l.platform, &l.msg_id, s),
                    Moderate::Unpin => e.chat_pin(&l.platform, "", None),
                    _ => e.chat_remove(&l.platform, "", &l.user_id),
                },
                Err(err) => e.chat_note(&l.platform, format!("{what}: {}", plain_error(&err))),
            }
        });
    }

    /// Send `text` to an explicit list of platforms with configurable account fallback.
    pub fn send_chat_list(
        self: &Rc<Self>,
        to: &[&'static str],
        text: &str,
        bot: bool,
        fallback: bool,
        reply: Option<&Line>,
    ) -> Vec<&'static str> {
        if to.is_empty() {
            self.chat_note("twitch", "nothing connected to send to".into());
            return Vec::new();
        }
        let kick_known: Vec<String> = self.st.borrow().chat_log.emotes[2]
            .iter()
            .map(|e| e.0.clone())
            .collect();
        for &p in to {
            let mut text = for_platform(text, p, &kick_known);
            // A reply where the platform has them; a mention elsewhere.
            let reply = reply.filter(|l| l.platform == p);
            let reply_id = reply.filter(|l| !l.msg_id.is_empty() && matches!(p, "twitch" | "kick"));
            if let (Some(l), None) = (reply, reply_id) {
                text = format!("@{} {text}", l.user);
            }
            let to = reply_id.map(|l| format!(", reply to @{}", l.user)).unwrap_or_default();
            let reply_id = reply_id.map(|l| l.msg_id.clone());
            if self.dev_out(p, &text, bot) {
                self.chat_note(p, format!("(dev, not sent{to}) {text}"));
                continue;
            }
            let e = self.clone();
            tokio::task::spawn_local(async move {
                let r = match p {
                    "twitch" => match e.twitch() {
                        Some(t) => t.send_message(text, reply_id.clone(), bot, fallback).await,
                        None => Err("Twitch isn't connected".into()),
                    },
                    "youtube" => e.youtube_start().send_message("", &text, bot, fallback).await,
                    "kick" => match &reply_id {
                        Some(r) => e.kick_start().reply_message(&text, r, bot, fallback).await,
                        None => e.kick_start().send_message(&text, bot, fallback).await,
                    },
                    _ => e.tiktok_start().send_message(&text, bot, fallback).await,
                };
                if let Err(err) = r {
                    e.chat_note(p, format!("not sent: {err}"));
                }
            });
        }
        to.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emotes_per_platform() {
        let known = vec!["[emote:37226:KEKW]".to_string()];
        assert_eq!(shown("[emote:37226:KEKW]"), "KEKW");
        assert_eq!(shown(":yt:"), ":yt:");
        assert_eq!(for_platform("hi KEKW 😀", "kick", &known), "hi [emote:37226:KEKW] 😀");
        assert_eq!(for_platform("hi [emote:37226:KEKW] 😀", "twitch", &known), "hi KEKW 😀");
    }

    #[test]
    fn strikes_deleted_and_banned_lines() {
        let cfg = std::sync::Arc::new(tokio::sync::watch::channel(crate::Config::default()).0);
        let e = Engine::new(cfg, |_| {});
        let line = |p: &str, id: &str, user: &str| Line {
            platform: p.into(),
            msg_id: id.into(),
            user_id: user.into(),
            text: id.into(),
            ..Default::default()
        };
        let lines = [
            line("twitch", "a", "1"),
            line("twitch", "b", "2"),
            line("twitch", "c", "2"),
            line("kick", "d", "2"),
        ];
        e.chat_welcome("twitch", lines[..3].to_vec());
        e.chat_welcome("kick", lines[3..].to_vec());
        let (g, _, _) = e.chat_sync(999, 0);
        e.chat_remove("twitch", "a", "");
        e.chat_remove("twitch", "", "2");
        let (g2, _, left) = e.chat_sync(g, 0);
        assert_ne!(g, g2);
        let deleted: Vec<(&str, bool)> = left
            .iter()
            .filter(|l| !l.note)
            .map(|l| (l.text.as_str(), l.deleted))
            .collect();
        assert_eq!(deleted, [("a", true), ("b", true), ("c", true), ("d", false)]);
    }

    #[test]
    fn replies_from_args() {
        let mut a = Obj::new();
        a.insert("reply.userName".into(), "Ana".into());
        a.insert("reply.msgBody".into(), r"hi\sthere".into());
        let r = Reply::from_args(&a).expect("a reply");
        assert_eq!((r.user.as_str(), r.text.as_str()), ("Ana", "hi there"));
        assert_eq!(r.strip("@ana yes"), "yes");
        assert_eq!(r.strip("@bob yes"), "@bob yes");
        let mut k = Obj::new();
        k.insert("replyTo".into(), "bob".into());
        k.insert("replyToMessage".into(), "[emote:1:KEKW] lol".into());
        assert_eq!(Reply::from_args(&k).expect("a reply").text, "KEKW lol");
        assert!(Reply::from_args(&Obj::new()).is_none());
    }

    #[test]
    fn welcome_merges_and_deduplicates_history() {
        let cfg = std::sync::Arc::new(tokio::sync::watch::channel(crate::Config::default()).0);
        let e = Engine::new(cfg, |_| {});
        let h1 = Line {
            platform: "twitch".into(),
            msg_id: "m1".into(),
            time: "12:40".into(),
            user: "PedroZamecki".into(),
            text: "hey".into(),
            ..Default::default()
        };
        e.chat_welcome("twitch", vec![h1.clone()]);
        // Reconnect with same message + a newer pre-join message: m1 stays once, welcome moves after m2.
        let h2 = Line {
            platform: "twitch".into(),
            msg_id: "m2".into(),
            time: "12:41".into(),
            user: "Ana".into(),
            text: "yo".into(),
            ..Default::default()
        };
        e.chat_welcome("twitch", vec![h1, h2]);
        let (_, _, lines) = e.chat_sync(999, 0);
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["hey", "yo", "Welcome to the chat room!"]);
        assert!(lines[2].welcome && lines[2].note);
    }
}
