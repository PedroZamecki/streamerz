//! The chat window's side of the engine: every platform's chat (through `Engine::chat`) kept in one
//! bounded log, the emotes seen per platform, and sending to one platform or all of them with each
//! platform's emote format.

use super::Engine;
use crate::commands::Chat;
use crate::model::Obj;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

pub const PLATFORMS: [&str; 4] = ["twitch", "youtube", "kick", "tiktok"];

/// Lines kept (all platforms together).
const KEEP: usize = 500;
/// Emotes remembered per platform (the most recent).
const EMOTES: usize = 300;

/// One chat line, as the window shows it: emotes by their display name.
#[derive(Clone, Default)]
pub struct Line {
    pub seq: u64,
    pub platform: String,
    pub time: String,
    pub user: String,
    pub user_id: String,
    pub text: String,
    /// The emotes in `text`: name and picture URL.
    pub emotes: Vec<(String, String)>,
    /// Roles, shown as badges.
    pub broadcaster: bool,
    pub moderator: bool,
    pub vip: bool,
    pub subscriber: bool,
    /// The name's colour ("#RRGGBB") when the platform has one.
    pub color: String,
    /// Not a chat message: a send that failed, or (dev mode) one kept instead of sent.
    pub note: bool,
}

#[derive(Default)]
pub struct Log {
    lines: VecDeque<Line>,
    seq: u64,
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

/// `text` written for `platform`: Kick emotes as Kick's tags (from the names in `known`, Kick's
/// emotes seen), Kick tags pasted for another platform as their names. Unicode emoji go as they are.
pub fn for_platform(text: &str, platform: &str, kick_known: &[String]) -> String {
    text.split(' ')
        .map(|w| {
            if platform == "kick" {
                kick_known
                    .iter()
                    .find(|k| shown(k) == w && *k != w)
                    .map_or(w.to_string(), Clone::clone)
            } else {
                shown(w).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
}

impl Engine {
    /// Keep a chat message for the window (called by `chat`).
    pub(super) fn log_chat(&self, c: &Chat) {
        let mut st = self.st.borrow_mut();
        let log = &mut st.chat_log;
        let Some(i) = index(&c.platform) else { return };
        let mut text = c.text.clone();
        for e in &c.emotes {
            if c.platform != "twitch" {
                // Spaced, so "[emote:1:A][emote:1:A]" or ":a::b:" shows as separate emotes.
                text = text.replace(e.0.as_str(), &format!(" {} ", shown(&e.0)));
            }
            let seen = &mut log.emotes[i];
            if let Some(at) = seen.iter().position(|x| x.0 == e.0) {
                seen.remove(at);
            } else if seen.len() == EMOTES {
                seen.pop_front();
            }
            seen.push_back(e.clone());
        }
        if c.platform != "twitch" && !c.emotes.is_empty() {
            text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        let time = crate::fmt::date(&crate::fmt::now(), "HH:mm");
        let key = (c.platform.clone(), c.user_id.clone());
        if !log.users.contains_key(&key) && log.users.len() == KEEP {
            // ponytail: O(n) at 500 chatters, once per new chatter; an ordered map if that shows up.
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
        log.push(Line {
            platform: c.platform.clone(),
            time,
            user: c.user.clone(),
            user_id: c.user_id.clone(),
            text,
            emotes: c
                .emotes
                .iter()
                .map(|e| (shown(&e.0).to_string(), e.1.clone()))
                .collect(),
            broadcaster: c.broadcaster,
            moderator: c.moderator,
            vip: c.vip,
            subscriber: c.subscriber,
            color: c.args.get("color").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            ..Default::default()
        });
        drop(st);
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

    /// The lines after `seq`.
    pub fn chat_since(&self, seq: u64) -> Vec<Line> {
        let st = self.st.borrow();
        st.chat_log.lines.iter().filter(|l| l.seq > seq).cloned().collect()
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
            // ponytail: cleared when full; an LRU if 200 cards in 10 minutes ever matters.
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
        if to.is_empty() {
            self.chat_note(platform, "nothing connected to send to".into());
        }
        let kick_known: Vec<String> = self.st.borrow().chat_log.emotes[2]
            .iter()
            .map(|e| e.0.clone())
            .collect();
        for &p in &to {
            let text = for_platform(text, p, &kick_known);
            if self.dev_out(p, &text, bot) {
                self.chat_note(p, format!("(dev, not sent) {text}"));
                continue;
            }
            let e = self.clone();
            tokio::task::spawn_local(async move {
                let r = match p {
                    "twitch" => match e.twitch() {
                        Some(t) => t.send_message(text, None, bot, true).await,
                        None => Err("Twitch isn't connected".into()),
                    },
                    "youtube" => e.youtube_start().send_message("", &text, bot, true).await,
                    "kick" => e.kick_start().send_message(&text, bot, true).await,
                    _ => e.tiktok_start().send_message(&text, bot, true).await,
                };
                if let Err(err) = r {
                    e.chat_note(p, format!("not sent: {err}"));
                }
            });
        }
        to
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
        assert_eq!(for_platform("hi KEKW", "youtube", &known), "hi KEKW");
        assert_eq!(for_platform("[emote:37226:KEKW]", "kick", &known), "[emote:37226:KEKW]");
        assert_eq!(
            crate::kick::kick_emotes("a [emote:1:A] b [emote:x:B] [emote:22:C]")
                .iter()
                .map(|e| e.0.as_str())
                .collect::<Vec<_>>(),
            ["[emote:1:A]", "[emote:22:C]"]
        );
    }
}
