//! The chat window's side of the engine: every platform's chat (through `Engine::chat`) kept in one
//! bounded log, recent history from the platform (memory only) merged before a divider,
//! the emotes seen per platform, and sending to one platform or several with each platform's emote format.

use super::Engine;
use crate::commands::Chat;
use crate::model::Obj;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

pub const PLATFORMS: [&str; 4] = ["twitch", "youtube", "kick", "tiktok"];

/// Historical lines kept per platform across restarts / on room join.
const KEEP_HISTORY: usize = 100;
/// Emotes remembered per platform (the most recent).
const EMOTES: usize = 300;

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(n: &u64) -> bool {
    *n == 0
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
    /// When it was sent, in ms since the epoch (0 when the platform didn't say): history from every
    /// platform is put in this order.
    #[serde(skip)]
    pub ts: i64,
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
    /// Not a chat message: a send that failed, dev-mode output, or the history divider.
    #[serde(skip_serializing_if = "is_false")]
    pub note: bool,
    /// The divider between history and live messages: rendered without timestamp.
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
    /// An event shown in the chat (a sub, a gift, a raid...): what happened, and its icon. `text` is
    /// what its sender wrote with it, if anything.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub event: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// What a reward or a power-up cost (0: nothing said), and the icon of what it was paid in
    /// (empty: channel points).
    #[serde(skip_serializing_if = "is_zero")]
    pub cost: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cost_icon: String,
    /// A picture that goes with an event (a reward's icon, an emote it unlocked, a gift), as a URL, and
    /// whether it is drawn big (a gigantified emote).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub picture: String,
    #[serde(skip_serializing_if = "is_false")]
    pub huge: bool,
    /// A colour that sets the line apart ("#RRGGBB"): events, highlighted and first messages.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub accent: String,
    /// What's special about a message, said above it ("Highlighted", "First message").
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tag: String,
    /// Ours, shown the moment it was sent: the platform's own copy (with its id) replaces it when it comes.
    #[serde(skip)]
    pub local: bool,
    /// A `/me` message: shown in italics, in the sender's colour.
    #[serde(skip_serializing_if = "is_false")]
    pub action: bool,
}

/// The words of a `/me` message sent as an action (`/me waves` or IRC's `\x01ACTION waves\x01`).
pub fn action_of(text: &str) -> Option<&str> {
    text.strip_prefix("\x01ACTION ")
        .and_then(|s| s.strip_suffix('\x01'))
        .or_else(|| text.strip_prefix("/me ").filter(|s| !s.is_empty()))
}

/// An event the chat shows: its platform, icon, colour, words (an i18n key, with the trigger's
/// %variables%), and the variable holding what its sender wrote (when the chat doesn't show it already).
struct Ev {
    platform: &'static str,
    icon: &'static str,
    accent: &'static str,
    says: &'static str,
    with: Option<&'static str>,
    /// The argument holding what it cost, and what it was paid in (an icon; "" for channel points).
    cost: Option<&'static str>,
    cost_icon: &'static str,
    /// The argument holding a picture for it, and whether that picture is drawn big.
    picture: Option<&'static str>,
    huge: bool,
    /// The argument holding a colour of its own ("#RRGGBB", a reward's), over `accent`.
    color: Option<&'static str>,
}

const TWITCH: &str = "#9146FF";
const KICK: &str = "#53FC18";
const YOUTUBE: &str = "#FF0033";
const MEMBERS: &str = "#0F9D58";
const TIKTOK: &str = "#FE2C55";
const QUIET: &str = "#808080";

/// What the chat shows for trigger `kind` (none for most triggers: only what the platforms' own chats
/// show). Twitch's cheers, hype chats and power-ups are chat messages already: marked, not repeated.
fn event_of(kind: u32, a: &Obj) -> Option<Ev> {
    let yes = |k: &str| a.get(k).and_then(Value::as_bool) == Some(true);
    let ev = |platform, icon, accent, says, with| Ev {
        platform,
        icon,
        accent,
        says,
        with,
        cost: None,
        cost_icon: "",
        picture: None,
        huge: false,
        color: None,
    };
    let text = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("");
    Some(match kind {
        102 => ev("twitch", "gem", TWITCH, "chat.ev.cheer", None),
        103 => ev("twitch", "star", TWITCH, "chat.ev.sub", None),
        104 => ev("twitch", "star", TWITCH, "chat.ev.resub", Some("message")),
        // A gift of a bomb: the bomb says it once.
        105 if yes("fromGiftBomb") => return None,
        105 => ev("twitch", "gift", TWITCH, "chat.ev.gift_sub", None),
        106 => ev("twitch", "gift", TWITCH, "chat.ev.gift_bomb", None),
        107 => ev("twitch", "swords", TWITCH, "chat.ev.raid", None),
        // One with words comes as a chat message too (marked with the reward): shown there.
        112 if !text("rawInput").is_empty() => return None,
        112 => Ev {
            cost: Some("rewardCost"),
            picture: None,
            color: Some("rewardColor"),
            ..ev("twitch", "", TWITCH, "chat.ev.reward", None)
        },
        // The rewards every channel has (highlight a message, unlock an emote...) and the power-ups.
        194 => {
            let (says, icon) = match text("rewardType") {
                "send_highlighted_message" => ("chat.ev.auto.highlight", "highlighter"),
                "single_message_bypass_sub_mode" => ("chat.ev.auto.bypass", "message-square"),
                "random_sub_emote_unlock" => ("chat.ev.auto.random_emote", "smile"),
                "chosen_sub_emote_unlock" => ("chat.ev.auto.chosen_emote", "smile"),
                "chosen_modified_sub_emote_unlock" => ("chat.ev.auto.modified_emote", "smile"),
                "message_effect" => ("chat.ev.auto.effect", "sparkles"),
                "gigantify_an_emote" => ("chat.ev.auto.gigantify", "maximize"),
                "celebration" => ("chat.ev.auto.celebration", "party-popper"),
                _ => ("chat.ev.reward_other", "sparkles"),
            };
            Ev {
                cost: Some("rewardCost"),
                picture: Some(if a.contains_key("gigantifiedEmoteUrl") {
                    "gigantifiedEmoteUrl"
                } else {
                    "unlockedEmoteUrl"
                }),
                huge: text("rewardType") == "gigantify_an_emote",
                ..ev("twitch", icon, TWITCH, says, None)
            }
        }
        196 | 496 => {
            let says = match text("powerUpType") {
                "message_effect" => "chat.ev.power.effect",
                "celebration" => "chat.ev.power.celebration",
                "gigantify_an_emote" => "chat.ev.power.gigantify",
                _ if kind == 496 => "chat.ev.power.custom",
                _ => "chat.ev.power.other",
            };
            Ev {
                cost: Some("bits"),
                cost_icon: "gem",
                picture: Some("emoteUrl"),
                huge: text("powerUpType") == "gigantify_an_emote",
                ..ev("twitch", "zap", TWITCH, says, None)
            }
        }
        472 => ev("twitch", "flame", TWITCH, "chat.ev.watch_streak", None),
        138 => {
            let accent = match a.get("announceColor").and_then(Value::as_str).unwrap_or("") {
                "BLUE" => "#1E69FF",
                "GREEN" => "#00C881",
                "ORANGE" => "#FF9100",
                _ => TWITCH,
            };
            ev("twitch", "megaphone", accent, "chat.ev.announcement", Some("message"))
        }
        136 => ev("twitch", "clock", QUIET, "chat.ev.timeout", None),
        137 => ev("twitch", "ban", QUIET, "chat.ev.ban", None),
        35014 => ev("kick", "star", KICK, "chat.ev.sub", None),
        35015 => ev("kick", "star", KICK, "chat.ev.resub_kick", None),
        35016 => ev("kick", "gift", KICK, "chat.ev.gift_sub_kick", None),
        35017 => ev("kick", "gift", KICK, "chat.ev.gift_bomb_kick", None),
        35018 => ev("kick", "clock", QUIET, "chat.ev.timeout", None),
        35019 => ev("kick", "ban", QUIET, "chat.ev.ban", None),
        35024 => Ev {
            cost: Some("rewardCost"),
            color: Some("reward.backgroundColor"),
            ..ev("kick", "sparkles", KICK, "chat.ev.reward_kick", Some("rawInput"))
        },
        35025 => ev("kick", "gem", KICK, "chat.ev.kicks", Some("rawInput")),
        4006 => ev(
            "youtube",
            "badge-dollar-sign",
            YOUTUBE,
            "chat.ev.superchat",
            Some("message"),
        ),
        4007 => ev("youtube", "sticker", YOUTUBE, "chat.ev.supersticker", None),
        4008 => ev("youtube", "star", MEMBERS, "chat.ev.member", None),
        4009 => ev("youtube", "star", MEMBERS, "chat.ev.milestone", Some("message")),
        4014 => ev("youtube", "gift", MEMBERS, "chat.ev.member_gift", None),
        // A combo gift repeats while it grows: shown once, when it ends.
        41002 if a.get("giftType").and_then(Value::as_u64) == Some(1) && !yes("repeatEnd") => return None,
        41002 => Ev {
            cost: Some("coins"),
            picture: Some("giftPictureUrl"),
            ..ev("tiktok", "gift", TIKTOK, "chat.ev.tiktok_gift", None)
        },
        41004 => ev("tiktok", "user-plus", TIKTOK, "chat.ev.follow", None),
        41005 => ev("tiktok", "share-2", TIKTOK, "chat.ev.share", None),
        41006 => ev("tiktok", "star", TIKTOK, "chat.ev.sub", None),
        _ => return None,
    })
}

/// What sets a Twitch message apart (its EventSub `message_type`): the words above it and its colour.
fn special(message_type: &str) -> Option<(&'static str, &'static str)> {
    Some(match message_type {
        "channel_points_highlighted" => (crate::i18n::tr("chat.tag.highlighted"), TWITCH),
        "user_intro" => (crate::i18n::tr("chat.tag.first"), "#00C8AF"),
        "channel_points_sub_only" => (crate::i18n::tr("chat.tag.sub_only"), TWITCH),
        "power_ups_gigantified_emote" => (crate::i18n::tr("chat.tag.gigantified"), TWITCH),
        "power_ups_message_effect" => (crate::i18n::tr("chat.tag.effect"), TWITCH),
        _ => return None,
    })
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
    /// How many lines are kept, and for how long (the config's, taken in as lines come).
    limits: crate::ChatLimits,
    /// A sweep for lines past their lifetime is waiting to run.
    sweeping: bool,
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
    /// The message pinned in each platform's chat, if any (one each).
    pins: Vec<Pin>,
    /// The polls and predictions going on (and just over), per platform.
    polls: Vec<Poll>,
    /// The platforms whose stream has ended (until it starts again).
    ended: Vec<String>,
    /// When Twitch's pin was last asked for (it has no event for pins made elsewhere).
    twitch_pin_asked: Option<std::time::Instant>,
}

/// A poll or a prediction at the top of a chat: what is asked, and how the choices stand.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Poll {
    pub platform: String,
    pub id: String,
    pub title: String,
    /// A prediction (points on an outcome), not a poll (votes).
    pub prediction: bool,
    /// Each choice and its votes (a prediction's: the points put on it).
    pub choices: Vec<(String, u64)>,
    /// When voting closes (ms since the epoch), if said.
    pub ends: Option<u64>,
    /// When it ended or was locked (ms), if it has: the card stays a little, with the result.
    pub over: Option<u64>,
}

/// What a platform's chat is doing, for the notice over it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChatState {
    /// Connected, and the stream is on (or not said to be over).
    Live,
    /// The stream ended: the chat may still take messages.
    Ended,
    /// Watching for the stream to start.
    Standby,
    /// Not connected.
    Down,
}

/// How long a finished poll's card stays.
const POLL_AFTER: u64 = 20_000;

impl Poll {
    pub fn live(&self, now_ms: u64) -> bool {
        self.over.is_none_or(|o| now_ms < o + POLL_AFTER)
    }
}

/// A message pinned at the top of a chat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pin {
    pub platform: String,
    pub msg_id: String,
    /// Who wrote it, and what.
    pub user: String,
    pub text: String,
    /// Who pinned it (empty: not said).
    pub by: String,
    /// Until when (ms since the epoch); None: until it's unpinned or the stream ends.
    pub until: Option<u64>,
}

impl Pin {
    pub fn live(&self, now_ms: u64) -> bool {
        self.until.is_none_or(|u| u > now_ms)
    }
}

fn now_ms() -> u64 {
    crate::fmt::unix_now().as_millis() as u64
}

/// Label and value rows about a chatter.
type Facts = Vec<(String, String)>;

/// One thing a platform says about a chatter: what, and its value.
pub type Fact = (String, String);

/// How long a user card's lookup may take before it shows what it has.
const ASK_FOR: std::time::Duration = std::time::Duration::from_secs(20);

/// How long a user card's lookup is reused.
const CARD_FOR: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// One thing a chatter said: when, the words, and the emotes in them (name and picture).
#[derive(Clone, Default)]
pub struct Said {
    pub time: String,
    pub text: String,
    pub emotes: Vec<(String, String)>,
}

/// A chatter, as seen in chat this session.
#[derive(Clone, Default)]
pub struct Seen {
    /// Their latest message's arguments about them (userName, isSubscribed, pronouns...).
    pub args: Obj,
    pub messages: u32,
    /// When they first chatted (a time of day), and the log's seq of their latest message.
    pub first: String,
    last: u64,
}

/// Unix seconds as a chat line's time, as the user reads times.
pub fn local_hhmm(unix_secs: i64) -> String {
    crate::fmt::shown_clock(unix_secs)
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
        if !self.limits.save_history {
            return;
        }
        self.seq += 1;
        l.seq = self.seq;
        self.lines.push_back(l);
        self.trim();
    }

    /// Lines over the limits go, the oldest first.
    fn trim(&mut self) {
        if !self.limits.save_history {
            self.lines.clear();
            return;
        }
        let gone = self.limits.excess(self.lines.iter().map(|l| index(&l.platform)));
        for at in gone.into_iter().rev() {
            self.lines.remove(at);
        }
    }

    /// Lines older than their lifetime go (none when it's off). The age is of when each was said.
    fn age_out(&mut self, now: u64) -> usize {
        let life = self.limits.lifetime * 1000;
        let before = self.lines.len();
        if life > 0 {
            self.lines.retain(|l| l.ts <= 0 || now < l.ts as u64 + life);
        }
        before - self.lines.len()
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

/// Twitch's Get Pinned Chat Message answer as a pin (none when nothing is pinned).
fn twitch_pin(d: &Value) -> Option<Pin> {
    let id = d["message_id"].as_str().filter(|s| !s.is_empty())?;
    let until = d["ends_at"]
        .as_str()
        .and_then(super::rest_parse_iso)
        .map(|secs| secs as u64 * 1000);
    Some(Pin {
        platform: "twitch".into(),
        msg_id: id.into(),
        user: d["sender_user_name"].as_str().unwrap_or("").into(),
        text: d["message"]["text"].as_str().unwrap_or("").into(),
        by: d["pinned_by_user_name"].as_str().unwrap_or("").into(),
        until,
    })
}

/// A Kick username as its page's address has it: "Some_Name" is "some-name".
fn kick_slug(name: &str) -> String {
    name.trim().to_lowercase().replace('_', "-")
}

/// A badge's name from its key, when the platform didn't give one: "kick:sub_gifter" is "Sub gifter",
/// "subscriber/12" is "Subscriber".
fn badge_name(key: &str) -> String {
    let k = key.rsplit(':').next().unwrap_or(key);
    let k = k.split('/').next().unwrap_or(k).replace('_', " ");
    let mut c = k.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

impl Engine {
    /// The chat log, with the config's limits in it, so what goes in is trimmed to them.
    fn chat_log_mut(&self) -> std::cell::RefMut<'_, Log> {
        let limits = self.config().window.chat.clamped();
        let mut log = std::cell::RefMut::map(self.st.borrow_mut(), |s| &mut s.chat_log);
        log.limits = limits;
        log
    }

    /// A line came: when lines have a lifetime and no sweep waits, one is set.
    fn chat_sweep_soon(self: &Rc<Self>) {
        if self.config().window.chat.lifetime > 0 && !self.st.borrow().chat_log.sweeping {
            self.chat_trim();
        }
    }

    /// The limits changed (or lines came): lines over them go now, and the ones that will be past their
    /// lifetime are taken out when it is up. Idle, with no lifetime, nothing is waiting.
    pub fn chat_trim(self: &Rc<Self>) {
        let (life, due) = {
            let mut guard = self.chat_log_mut();
            let log = &mut *guard;
            log.trim();
            let now = now_ms();
            let gone = log.age_out(now);
            let life = log.limits.lifetime * 1000;
            let oldest = log.lines.iter().filter(|l| l.ts > 0).map(|l| l.ts as u64).min();
            if gone > 0 {
                log.reset_gen += 1;
            }
            // Sweeps come in batches (a tenth of the lifetime, at least a second), not for every line.
            let due = oldest.map(|t| (t + life).saturating_sub(now).max((life / 10).max(1000)));
            (life, due.filter(|_| life > 0 && !log.sweeping))
        };
        if let (true, Some(wait)) = (life > 0, due) {
            self.chat_log_mut().sweeping = true;
            let me = self.clone();
            tokio::task::spawn_local(async move {
                tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                me.chat_log_mut().sweeping = false;
                me.chat_trim();
            });
        }
    }

    /// Chat history is never kept on disk: the platforms give it back when we join. This removes the file
    /// older versions wrote.
    pub fn forget_saved_chat_history() {
        crate::ui::img::forget_saved("chat-history");
    }

    /// Merge `fetched` recent messages for `platform` (oldest-to-newest) with any cached historical
    /// lines, then a divider right before live session messages (kept in memory only).
    /// Forget what `platform`'s chat showed (its channel changed): lines, pins and who was seen.
    pub fn chat_forget(&self, platform: &str) {
        let log = &mut self.st.borrow_mut().chat_log;
        log.lines.retain(|l| l.platform != platform);
        log.pins.retain(|p| p.platform != platform);
        log.users.retain(|k, _| k.0 != platform);
        log.cards.retain(|k, _| k.0 != platform);
        log.reset_gen += 1;
    }

    pub fn chat_welcome(self: &Rc<Self>, platform: &str, fetched: Vec<Line>) {
        let Some(p_idx) = index(platform) else { return };
        self.role_from_history(platform, &fetched);
        {
            let mut guard = self.chat_log_mut();
            let log = &mut *guard;

            // What there is: history (of any platform) and live lines, in the order they came.
            let mut history: Vec<Line> = Vec::new();
            let mut live: Vec<Line> = Vec::new();
            let mut seen: Vec<Line> = Vec::new();
            for l in log.lines.drain(..) {
                if l.welcome {
                    // The divider is made again below.
                } else if l.historical && !l.note {
                    history.push(l);
                } else {
                    live.push(l);
                }
            }
            // The platform's history: what it says is fresher than what's kept, and what's already here,
            // as history or live, isn't added twice.
            history.retain(|h| h.platform != platform || !fetched.iter().any(|f| same_msg(h, f)));
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
                if !history.iter().any(|m| same_msg(m, &f)) && !live.iter().any(|lv| same_msg(lv, &f)) {
                    seen.push(f.clone());
                    history.push(f);
                }
            }
            // One timeline: by when each was sent, not by which platform answered first.
            history.sort_by_key(|l| l.ts);
            // At most the last KEEP_HISTORY of each platform.
            let mut left = [0usize; 4];
            for l in &history {
                if let Some(i) = index(&l.platform) {
                    left[i] += 1;
                }
            }
            history.retain(|l| match index(&l.platform) {
                Some(i) if left[i] > KEEP_HISTORY => {
                    left[i] -= 1;
                    false
                }
                _ => true,
            });
            let has_history = !history.is_empty();
            for mut l in history {
                l.historical = true;
                log.push(l);
            }
            // One divider for all of them, where the history ends and live messages begin.
            if has_history {
                log.push(Line {
                    text: crate::i18n::tr("chat.live_below").into(),
                    note: true,
                    welcome: true,
                    ..Default::default()
                });
            }
            for l in live {
                log.push(l);
            }
            log.reset_gen += 1;
            drop(guard);
            for l in &seen {
                self.seen_in_history(l);
            }
        }
        self.chat_sweep_soon();
        crate::ui::chat();
    }

    /// Keep a chat message for the window (called by `chat`).
    pub(super) fn log_chat(self: &Rc<Self>, c: &Chat) {
        // Our own message: its badges say our role now (modded or unmodded since we connected).
        let lead = c.args.get("badges").and_then(Value::as_array).is_some_and(|b| {
            b.iter().any(|b| {
                b["name"]
                    .as_str()
                    .or(b["key"].as_str())
                    .is_some_and(|n| n.contains("lead_moderator"))
            })
        });
        let shown = crate::roles::Role::from_flags(c.broadcaster, lead, c.moderator, c.vip);
        if crate::roles::seen_self(&c.platform, &c.user_id, shown) {
            crate::say(format!("{}: this account is now a {} here", c.platform, shown.name()));
            crate::ui::changed();
        }
        {
            let mut guard = self.chat_log_mut();
            let log = &mut *guard;
            let Some(i) = index(&c.platform) else { return };
            for e in &c.emotes {
                log.remember_emote(i, e.0.clone(), e.1.clone());
            }
            let was_action = c.args.get("isAction").and_then(Value::as_bool).unwrap_or(false);
            let (plain, action) = match action_of(&c.text) {
                Some(a) => (a.to_string(), true),
                None => (c.text.clone(), was_action),
            };
            let (text, emotes) = normalize_line_emotes(&c.platform, plain, &c.emotes);
            let time = crate::fmt::shown_time(&crate::fmt::now());
            let key = (c.platform.clone(), c.user_id.clone());
            if !log.users.contains_key(&key) && log.users.len() >= log.limits.total {
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
                ts: crate::fmt::unix_now().as_millis() as i64,
                user: c.user.clone(),
                user_id: c.user_id.clone(),
                reply,
                text,
                action,
                emotes,
                broadcaster: c.broadcaster,
                moderator: c.moderator,
                vip: c.vip,
                subscriber: c.subscriber,
                color: c.args.get("color").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                tag: special(c.args.get("messageType").and_then(Value::as_str).unwrap_or(""))
                    .map(|s| s.0.to_string())
                    .unwrap_or_default(),
                accent: special(c.args.get("messageType").and_then(Value::as_str).unwrap_or(""))
                    .map(|s| s.1.to_string())
                    .unwrap_or_default(),
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
            // A message sent with a channel reward: the reward over it, with its picture and price.
            let arg = |k: &str| c.args.get(k).and_then(Value::as_str).unwrap_or("");
            if !arg("messageRewardId").is_empty() {
                line.tag = match arg("messageRewardTitle") {
                    "" => crate::i18n::tr("chat.tag.reward_other").to_string(),
                    title => crate::i18n::tr("chat.tag.reward").replace("%title%", title),
                };
                line.cost = c.args.get("messageRewardCost").and_then(Value::as_u64).unwrap_or(0);
                line.cost_icon = arg("messageRewardPoints").into();
                line.picture = arg("messageRewardImage").into();
                line.accent = Some(arg("messageRewardColor"))
                    .filter(|c| c.len() == 7 && c.starts_with('#'))
                    .unwrap_or(TWITCH)
                    .into();
            }
            // A gigantified emote is the message's last emote, drawn big.
            line.huge = arg("messageType") == "power_ups_gigantified_emote";
            // The platform's copy of what we sent from here: it takes the place of the line shown at send.
            if let Some(mine) = log.lines.iter_mut().rev().take(30).find(|m| {
                m.local && m.platform == line.platform && m.user.eq_ignore_ascii_case(&line.user) && m.text == line.text
            }) {
                mine.local = false;
                mine.msg_id = line.msg_id;
                mine.badges = line.badges;
                mine.emotes = line.emotes;
                mine.color = line.color;
                (mine.broadcaster, mine.moderator, mine.vip, mine.subscriber) =
                    (line.broadcaster, line.moderator, line.vip, line.subscriber);
                log.reset_gen += 1;
                drop(guard);
                crate::ui::chat();
                return;
            }
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
        self.chat_sweep_soon();
        crate::ui::chat();
    }

    /// What a trigger shows in the chat, if anything (see `event_of`).
    pub(super) fn chat_event(self: &Rc<Self>, kind: u32, a: &Obj) {
        let Some(ev) = event_of(kind, a) else { return };
        let get = |k: &str| a.get(k).map(crate::vars::text).unwrap_or_default();
        let says = crate::vars::simple(crate::i18n::tr(ev.says), a, &Obj::new());
        let said = ev.with.map(get).unwrap_or_default();
        let (text, emotes) = normalize_line_emotes(ev.platform, said, &[]);
        let user = Some(get("user"))
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| get("userName"));
        let cost_icon = if !ev.cost_icon.is_empty() {
            ev.cost_icon.to_string()
        } else {
            get("rewardPoints")
        };
        self.chat_log_mut().push(Line {
            platform: ev.platform.into(),
            time: crate::fmt::shown_time(&crate::fmt::now()),
            ts: crate::fmt::unix_now().as_millis() as i64,
            user,
            user_id: get("userId"),
            text,
            emotes,
            event: says,
            cost: ev.cost.and_then(|k| a.get(k)).and_then(Value::as_u64).unwrap_or(0),
            cost_icon,
            picture: ev.picture.map(get).unwrap_or_default(),
            huge: ev.huge,
            icon: ev.icon.into(),
            accent: ev
                .color
                .map(get)
                .filter(|c| c.len() == 7 && c.starts_with('#'))
                .unwrap_or_else(|| ev.accent.into()),
            ..Default::default()
        });
        self.chat_sweep_soon();
        crate::ui::chat();
    }

    /// A line from Streamerz itself (a failed send, a dev-mode send).
    fn chat_note(&self, platform: &str, text: String) {
        self.chat_log_mut().push(Line {
            platform: platform.into(),
            time: crate::fmt::shown_time(&crate::fmt::now()),
            user: "Streamerz".into(),
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
            crate::ui::chat();
        }
    }

    /// What chat said about a chatter this session.
    /// What a chatter said, oldest first: time and text (this session's, and the history read when joining).
    pub fn chatter_lines(&self, platform: &str, user_id: &str) -> Vec<Said> {
        let st = self.st.borrow();
        let mut v: Vec<Said> = st
            .chat_log
            .lines
            .iter()
            .filter(|l| l.platform == platform && l.user_id == user_id && !l.note && !l.deleted)
            .map(|l| Said {
                time: l.time.clone(),
                text: l.text.clone(),
                emotes: l.emotes.clone(),
            })
            .collect();
        let drop = v.len().saturating_sub(100);
        v.drain(..drop);
        v
    }

    /// Like `chat_sync`, for one chatter's messages (a user card's).
    pub fn chatter_sync(&self, platform: &str, user_id: &str, client_gen: u64, seq: u64) -> (u64, bool, Vec<Line>) {
        let st = self.st.borrow();
        let g = st.chat_log.reset_gen;
        let full = g != client_gen;
        let lines = st
            .chat_log
            .lines
            .iter()
            .filter(|l| (full || l.seq > seq) && l.platform == platform && l.user_id == user_id && !l.note)
            .cloned()
            .collect();
        (g, full, lines)
    }

    /// The badges a chatter wore in their last message here, and their names.
    pub fn chatter_badges(&self, platform: &str, user_id: &str) -> Vec<(String, String, String)> {
        let st = self.st.borrow();
        let Some(l) = st
            .chat_log
            .lines
            .iter()
            .rev()
            .find(|l| l.platform == platform && l.user_id == user_id && !l.note)
        else {
            return Vec::new();
        };
        // No badge list from the platform: their roles, drawn as the chat draws them.
        let roles: Vec<(String, String)> = [
            ("broadcaster", l.broadcaster),
            ("moderator", l.moderator && !l.broadcaster),
            ("vip", l.vip),
            ("subscriber", l.subscriber),
        ]
        .into_iter()
        .filter(|r| r.1)
        .map(|r| (r.0.to_string(), String::new()))
        .collect();
        let badges = if l.badges.is_empty() { &roles } else { &l.badges };
        badges
            .iter()
            .map(|(k, url)| {
                // Twitch's pictures come from its badge sets, by set and version.
                let url = if url.is_empty() && platform == "twitch" {
                    crate::twitch::badge_url(k)
                        .or_else(|| crate::twitch::badge_url(&format!("{k}/1")))
                        .unwrap_or_default()
                } else {
                    url.clone()
                };
                let title = match platform {
                    "twitch" => crate::twitch::badge_title(k),
                    _ => None,
                }
                .unwrap_or_else(|| badge_name(k));
                (k.clone(), url, title)
            })
            .collect()
    }

    /// A badge's name as the user reads it ("1-Year Subscriber" where the platform says it).
    pub fn badge_label(&self, platform: &str, key: &str) -> String {
        match platform {
            "twitch" => crate::twitch::badge_title(key),
            _ => None,
        }
        .unwrap_or_else(|| badge_name(key))
    }

    pub fn chatter(&self, platform: &str, user_id: &str) -> Option<Seen> {
        let st = self.st.borrow();
        st.chat_log
            .users
            .get(&(platform.to_string(), user_id.to_string()))
            .cloned()
    }

    /// What the platform says about a chatter (for the chat's user card): label and value. Twitch,
    /// Kick and YouTube are asked; TikTok tells only what its chat messages carry.
    pub async fn chatter_lookup(
        self: &Rc<Self>,
        platform: &str,
        user_id: &str,
        progress: &dyn Fn(&[Fact]),
    ) -> Vec<(String, String)> {
        let key = (platform.to_string(), user_id.to_string());
        if let Some((at, found)) = self.st.borrow().chat_log.cards.get(&key) {
            if at.elapsed() < CARD_FOR {
                return found.clone();
            }
        }
        // Whatever it got is shown, even if a service never answers.
        let out = RefCell::new(Vec::new());
        let done = tokio::time::timeout(ASK_FOR, self.chatter_ask(platform, user_id, &out, progress))
            .await
            .is_ok();
        let found = out.into_inner();
        if done && !found.is_empty() {
            let cards = &mut self.st.borrow_mut().chat_log.cards;
            if cards.len() >= 200 {
                cards.clear();
            }
            cards.insert(key, (std::time::Instant::now(), found.clone()));
        }
        found
    }

    async fn chatter_ask(
        self: &Rc<Self>,
        platform: &str,
        user_id: &str,
        out: &RefCell<Vec<Fact>>,
        progress: &dyn Fn(&[Fact]),
    ) {
        let add = |k: &str, v: &Value| {
            let v = match v {
                Value::String(s) => s.clone(),
                Value::Null => return,
                v => v.to_string(),
            };
            // The first answer for a fact stands (kick.com's, then the API's).
            let mut out = out.borrow_mut();
            if !v.is_empty() && !out.iter().any(|f: &(String, String)| f.0 == k) {
                out.push((k.to_string(), v));
                progress(&out);
            }
        };
        match platform {
            "twitch" => {
                if let Some(u) = crate::twitch::public_profile(user_id).await {
                    add("Picture", &u["profileImageURL"]);
                    add("Banner", &u["bannerImageURL"]);
                    add("Account created", &u["createdAt"]);
                }
                let Some(tw) = self.twitch() else {
                    return;
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
                // Without a profile banner, the offline picture.
                add("Banner", &u["offline_image_url"]);
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
                // What kick.com's own chat shows when a name is clicked: no login needed.
                let channel = self.cfg.borrow().kick.channel.trim().to_lowercase();
                let name = {
                    let st = self.st.borrow();
                    st.chat_log
                        .lines
                        .iter()
                        .rev()
                        .find(|l| l.platform == "kick" && l.user_id == user_id)
                        .map(|l| kick_slug(&l.user))
                };
                if let Some(name) = name.clone().filter(|n| !n.is_empty() && !channel.is_empty()) {
                    let url = format!("https://kick.com/api/v2/channels/{channel}/users/{name}");
                    let h = [("Accept".to_string(), "application/json".to_string())];
                    let card = crate::http::request("GET", &url, &h, b"")
                        .await
                        .ok()
                        .filter(|r| r.status == 200)
                        .and_then(|r| serde_json::from_slice::<Value>(&r.body).ok())
                        .unwrap_or_default();
                    add("Picture", &card["profile_pic"]);
                    add("Channel", &card["slug"]);
                    add("Account created", &card["created_at"]);
                    if card.is_object() {
                        let since = if card["following_since"].is_string() {
                            card["following_since"].clone()
                        } else {
                            "not following".into()
                        };
                        add("Following since", &since);
                    }
                    if let Some(m) = card["subscribed_for"].as_u64().filter(|m| *m > 0) {
                        add("Subscribed", &format!("{m} months").into());
                    }
                    if !card["banned"].is_null() && card.is_object() {
                        add("Banned", &"yes".into());
                    }
                }
                // Their own channel's page, which has the banner (the API only has it for the logged-in channel).
                if let Some(name) = name.filter(|n| !n.is_empty()) {
                    let url = format!("https://kick.com/api/v2/channels/{name}");
                    let h = [("Accept".to_string(), "application/json".to_string())];
                    let ch = crate::http::request("GET", &url, &h, b"")
                        .await
                        .ok()
                        .filter(|r| r.status == 200)
                        .and_then(|r| serde_json::from_slice::<Value>(&r.body).ok())
                        .unwrap_or_default();
                    add("Picture", &ch["user"]["profile_pic"]);
                    add("Banner", &ch["banner_image"]["url"]);
                    add("Banner", &ch["offline_banner_image"]["src"]);
                    add("About", &ch["user"]["bio"]);
                    if let Some(n) = ch["followers_count"].as_u64() {
                        add("Followers", &n.to_string().into());
                    }
                }
                let Some(k) = self.kick() else { return };
                let (users, channels) = (
                    format!("/users?id={user_id}"),
                    format!("/channels?broadcaster_user_id={user_id}"),
                );
                let (u, c) = tokio::join!(k.api("GET", &users, None), k.api("GET", &channels, None));
                let (u, c) = (u.unwrap_or_default(), c.unwrap_or_default());
                add("Picture", &u["data"][0]["profile_picture"]);
                let c = &c["data"][0];
                add("Channel", &c["slug"]);
                add("Banner", &c["banner_picture"]);
                add("Their stream title", &c["stream_title"]);
                add("Their category", &c["category"]["name"]);
                if c["stream"]["is_live"] == true {
                    add("Live now", &c["stream"]["viewer_count"]);
                }
            }
            "youtube" => {
                // What youtube.com's channel card shows: no login, no quota.
                let (banner, picture) = crate::youtube::public_channel(user_id).await;
                add("Picture", &picture.into());
                add("Banner", &banner.into());
            }
            _ => {}
        }
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
                    self.chat_connected(p) && can_send(&self.cfg.borrow(), p)
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
            crate::ui::chat();
        }
    }

    /// `msg_id` pinned from here for `secs` (none: until the stream ends); "" unpins. One pinned at a time.
    fn chat_pin(&self, platform: &str, msg_id: &str, secs: Option<u64>) {
        if msg_id.is_empty() {
            return self.chat_unpinned(platform, "");
        }
        let line = {
            let st = self.st.borrow();
            st.chat_log
                .lines
                .iter()
                .find(|l| l.platform == platform && l.msg_id == msg_id)
                .map(|l| (l.user.clone(), l.text.clone()))
                .unwrap_or_default()
        };
        self.chat_pinned(Pin {
            platform: platform.into(),
            msg_id: msg_id.into(),
            user: line.0,
            text: line.1,
            by: String::new(),
            until: secs.map(|s| now_ms() + s * 1000),
        });
    }

    /// A message got pinned in `pin.platform`'s chat (here or anywhere): it replaces the one there.
    pub fn chat_pinned(&self, pin: Pin) {
        let mut st = self.st.borrow_mut();
        let until = pin.until.unwrap_or(u64::MAX);
        for l in st.chat_log.lines.iter_mut().filter(|l| l.platform == pin.platform) {
            l.pinned = (l.msg_id == pin.msg_id).then_some(until);
        }
        let pins = &mut st.chat_log.pins;
        if pins.contains(&pin) {
            return;
        }
        pins.retain(|p| p.platform != pin.platform);
        pins.push(pin);
        st.chat_log.reset_gen += 1;
        drop(st);
        crate::ui::chat();
    }

    /// `platform`'s pin was taken down (`msg_id`: only if it's that one; "" any).
    pub fn chat_unpinned(&self, platform: &str, msg_id: &str) {
        let mut st = self.st.borrow_mut();
        let before = st.chat_log.pins.len();
        st.chat_log
            .pins
            .retain(|p| p.platform != platform || !(msg_id.is_empty() || p.msg_id == msg_id));
        if st.chat_log.pins.len() == before {
            return;
        }
        for l in st.chat_log.lines.iter_mut().filter(|l| l.platform == platform) {
            l.pinned = None;
        }
        st.chat_log.reset_gen += 1;
        drop(st);
        crate::ui::chat();
    }

    /// The messages pinned now, one per platform at most.
    /// A poll or prediction started, moved or ended (one per platform and id).
    pub fn chat_poll(&self, p: Poll) {
        {
            let mut st = self.st.borrow_mut();
            let polls = &mut st.chat_log.polls;
            let now = now_ms();
            polls.retain(|x| x.live(now));
            match polls.iter_mut().find(|x| x.platform == p.platform && x.id == p.id) {
                Some(x) if *x == p => return,
                Some(x) => *x = p,
                None => polls.push(p),
            }
        }
        crate::ui::chat();
    }

    /// A platform's stream started (`live`) or ended: the chat's notice follows. Ended streams take their
    /// pins and polls with them.
    pub fn chat_stream(&self, platform: &str, live: bool) {
        let changed = {
            let mut st = self.st.borrow_mut();
            let ended = &mut st.chat_log.ended;
            let has = ended.iter().any(|p| p == platform);
            match (live, has) {
                (true, true) => {
                    ended.retain(|p| p != platform);
                    true
                }
                (false, false) => {
                    ended.push(platform.to_string());
                    true
                }
                _ => false,
            }
        };
        if !live {
            self.chat_polls_clear(platform);
        }
        if changed {
            crate::ui::changed();
        }
    }

    /// What `platform`'s chat is doing now.
    pub fn chat_state(&self, platform: &str) -> ChatState {
        let ended = self.st.borrow().chat_log.ended.iter().any(|p| p == platform);
        let standby = match platform {
            "youtube" => self.youtube_standby(),
            "tiktok" => self.tiktok_standby(),
            _ => false,
        };
        if ended {
            ChatState::Ended
        } else if self.chat_connected(platform) {
            ChatState::Live
        } else if standby {
            ChatState::Standby
        } else {
            ChatState::Down
        }
    }

    /// Connects `platform`'s chat (the notice's Reconnect).
    pub fn chat_reconnect(self: &Rc<Self>, platform: &str) {
        match platform {
            "twitch" => self.twitch_start().connect(),
            "youtube" => self.youtube_start().connect(),
            "kick" => self.kick_start().connect(),
            _ => self.tiktok_start().send(crate::tiktok::TikTokCmd::Connect),
        }
    }

    /// The polls and predictions to show now.
    pub fn chat_polls(&self) -> Vec<Poll> {
        let now = now_ms();
        self.st
            .borrow()
            .chat_log
            .polls
            .iter()
            .filter(|p| p.live(now))
            .cloned()
            .collect()
    }

    /// A platform's chat ended or left (a stream over): its polls go with it.
    pub fn chat_polls_clear(&self, platform: &str) {
        let before = {
            let mut st = self.st.borrow_mut();
            let n = st.chat_log.polls.len();
            st.chat_log.polls.retain(|p| p.platform != platform);
            n != st.chat_log.polls.len()
        };
        if before {
            crate::ui::chat();
        }
    }

    pub fn chat_pins(&self) -> Vec<Pin> {
        let now = now_ms();
        self.st
            .borrow()
            .chat_log
            .pins
            .iter()
            .filter(|p| p.live(now))
            .cloned()
            .collect()
    }

    /// Asks Twitch what's pinned (it has no event for pins made on its site), at most once a minute:
    /// on connecting, and when the chat is looked at.
    pub fn twitch_check_pin(self: &Rc<Self>) {
        {
            let mut st = self.st.borrow_mut();
            let asked = &mut st.chat_log.twitch_pin_asked;
            if asked.is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(60)) {
                return;
            }
            *asked = Some(std::time::Instant::now());
        }
        let Some(tw) = self.twitch() else { return };
        let e = self.clone();
        tokio::task::spawn_local(async move {
            let who = vec![
                ("broadcaster_id", "{broadcaster_id}"),
                ("moderator_id", "{moderator_id}"),
            ];
            let Ok(v) = tw.helix("GET", "chat/pins", who, None, false, false).await else {
                return;
            };
            match twitch_pin(&v["data"][0]) {
                Some(p) => e.chat_pinned(p),
                None => e.chat_unpinned("twitch", ""),
            }
        });
    }

    /// Our role when `platform`'s chat connects (Twitch asks instead, twitch.rs): not logged in, the owner on
    /// our own channel, else a viewer until the badges on our messages say more (Kick and YouTube have no
    /// call that tells).
    pub fn settle_role(self: &Rc<Self>, platform: &str) {
        use crate::roles::Role;
        let me = self.my_id(platform);
        let role = if me.is_empty() {
            Role::Anonymous
        } else if self.own_channel(platform) {
            Role::Broadcaster
        } else {
            Role::Viewer
        };
        let (known, id) = crate::roles::get(platform);
        if role == Role::Viewer && id == me && known > Role::Viewer {
            return;
        }
        if crate::roles::set(platform, role, &me) {
            crate::ui::changed();
        }
        // Not the owner: Kick says what we are in a channel; elsewhere our earlier messages in the history
        // do (see `chat_welcome`), and our next message.
        if role == Role::Viewer && platform == "kick" {
            let e = self.clone();
            tokio::task::spawn_local(async move { e.kick_role().await });
        }
    }

    /// Our user id on `platform` ("" when not logged in as the broadcaster there).
    fn my_id(&self, platform: &str) -> String {
        let c = self.cfg.borrow();
        match platform {
            "kick" => c.kick.broadcaster.as_ref().map(|b| b.user_id.to_string()),
            "youtube" => c.youtube.broadcaster.as_ref().map(|b| b.channel_id.clone()),
            "tiktok" => c.tiktok.broadcaster.as_ref().map(|b| b.username.clone()),
            _ => None,
        }
        .unwrap_or_default()
    }

    /// Kick's own answer to "what is this account in this channel" (what kick.com's chat uses; no login).
    async fn kick_role(self: &Rc<Self>) {
        use crate::roles::Role;
        let (channel, name, me) = {
            let c = self.cfg.borrow();
            (
                c.kick.channel.trim().to_lowercase(),
                c.kick
                    .broadcaster
                    .as_ref()
                    .map(|b| kick_slug(&b.username))
                    .unwrap_or_default(),
                self.my_id("kick"),
            )
        };
        if channel.is_empty() || name.is_empty() {
            return;
        }
        let url = format!("https://kick.com/api/v2/channels/{channel}/users/{name}");
        let h = [("Accept".to_string(), "application/json".to_string())];
        let Some(card) = crate::http::request("GET", &url, &h, b"")
            .await
            .ok()
            .filter(|r| r.status == 200)
            .and_then(|r| serde_json::from_slice::<Value>(&r.body).ok())
        else {
            return;
        };
        let role = if card["is_channel_owner"].as_bool() == Some(true) {
            Role::Broadcaster
        } else if card["is_moderator"].as_bool() == Some(true) {
            Role::Moderator
        } else {
            Role::Viewer
        };
        if crate::roles::get("kick").1 == me && crate::roles::set("kick", role, &me) {
            if role.moderates() {
                crate::say(format!("kick: this account is a {} here", role.name()));
            }
            crate::ui::changed();
        }
    }

    /// Our own earlier messages in a chat's history say what we are there: a moderator's badge on one
    /// is known now, not at our next message.
    fn role_from_history(&self, platform: &str, lines: &[Line]) {
        let me = self.my_id(platform);
        if me.is_empty() {
            return;
        }
        // The newest of ours.
        let Some(l) = lines.iter().rev().find(|l| l.user_id == me && !l.note) else {
            return;
        };
        let lead = l.badges.iter().any(|b| b.0.contains("lead_moderator"));
        let role = crate::roles::Role::from_flags(l.broadcaster, lead, l.moderator, l.vip);
        if crate::roles::seen_self(platform, &me, role) {
            crate::say(format!("{platform}: this account is a {} here", role.name()));
            crate::ui::changed();
        }
    }

    /// A line for what we are sending, shown at once (before the platform has taken it, and its own copy
    /// has come back); `chat_unlocal` takes it away if the send fails. The sequence number to ask that with.
    fn chat_local(&self, platform: &str, text: &str, bot: bool) -> Option<u64> {
        let (name, id) = {
            let c = self.cfg.borrow();
            let acct = |b: bool| -> Option<(String, String)> {
                match platform {
                    "kick" => (if b { &c.kick.bot } else { &c.kick.broadcaster })
                        .as_ref()
                        .map(|a| (a.username.clone(), a.user_id.to_string())),
                    "youtube" => (if b { &c.youtube.bot } else { &c.youtube.broadcaster })
                        .as_ref()
                        .map(|a| (a.name.clone(), a.channel_id.clone())),
                    "tiktok" => (if b { &c.tiktok.bot } else { &c.tiktok.broadcaster })
                        .as_ref()
                        .map(|a| (a.username.trim_start_matches('@').to_string(), a.username.clone())),
                    _ => None,
                }
            };
            acct(bot).or_else(|| acct(!bot))?
        };
        let (role, mine) = crate::roles::get(platform);
        let own = mine == id;
        let (action, plain) = match action_of(text) {
            Some(a) => (true, a.to_string()),
            None => (false, text.to_string()),
        };
        let (plain, emotes) = normalize_line_emotes(platform, plain, &[]);
        let mut guard = self.chat_log_mut();
        let log = &mut *guard;
        // Our name's colour and badges as the chat showed them last, until the platform's own copy says.
        let (color, badges) = log
            .lines
            .iter()
            .rev()
            .find(|l| l.platform == platform && l.user_id == id && !l.note && !l.local)
            .map(|l| (l.color.clone(), l.badges.clone()))
            .unwrap_or_default();
        log.push(Line {
            platform: platform.into(),
            time: crate::fmt::shown_time(&crate::fmt::now()),
            ts: crate::fmt::unix_now().as_millis() as i64,
            user: name,
            user_id: id,
            text: plain,
            emotes,
            action,
            color,
            badges,
            local: true,
            broadcaster: own && role == crate::roles::Role::Broadcaster,
            moderator: own && role.moderates(),
            ..Default::default()
        });
        let seq = log.seq;
        drop(guard);
        crate::ui::chat();
        Some(seq)
    }

    /// Takes away the line `chat_local` showed, if it is still waiting for the platform's copy.
    fn chat_unlocal(&self, seq: u64) {
        let mut st = self.st.borrow_mut();
        let log = &mut st.chat_log;
        let before = log.lines.len();
        log.lines.retain(|l| !(l.seq == seq && l.local));
        if log.lines.len() != before {
            log.reset_gen += 1;
            drop(st);
            crate::ui::chat();
        }
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
            "twitch" => !crate::twitch::other_channel(&c),
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
                        ("moderator_id", "{moderator_id}"),
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

    /// Whether the chat sends on `platform` as the broadcaster (else as the bot), as picked in the chat.
    pub fn sends_as_broadcaster(&self, platform: &str) -> bool {
        self.cfg
            .borrow()
            .window
            .chat_as_broadcaster
            .iter()
            .any(|p| p == platform)
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
            // Elsewhere than Twitch (whose copy comes back at once) the line shows as it is sent.
            let command = crate::slash::is_command(&text);
            let local = (p != "twitch" && !(p == "kick" && command))
                .then(|| self.chat_local(p, &text, bot))
                .flatten();
            let started = std::time::Instant::now();
            // Twitch's chat commands (/commercial, /timeout...) are calls, not text.
            if p == "twitch" && crate::slash::is_command(&text) {
                tokio::task::spawn_local(async move {
                    match crate::slash::run(&e, &text, bot, fallback).await {
                        Ok(done) if !done.is_empty() => e.chat_note(p, done),
                        Ok(_) => {}
                        Err(err) => e.chat_note(p, format!("{text}: {}", plain_error(&err))),
                    }
                });
                continue;
            }
            if p == "kick" && crate::slash::is_command(&text) {
                tokio::task::spawn_local(async move {
                    match crate::slash::run_kick(&e, &text).await {
                        Ok(done) => e.chat_note(p, done),
                        Err(err) => e.chat_note(p, format!("{text}: {}", plain_error(&err))),
                    }
                });
                continue;
            }
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
                match r {
                    Ok(()) => crate::debug(format!("{p}: message sent in {} ms", started.elapsed().as_millis())),
                    Err(err) => {
                        if let Some(seq) = local {
                            e.chat_unlocal(seq);
                        }
                        e.chat_note(p, format!("not sent: {err}"));
                    }
                }
            });
        }
        to.to_vec()
    }
}

/// Whether `platform` has an account logged in to send with (a chat read without one is read only).
pub fn can_send(c: &crate::Config, platform: &str) -> bool {
    match platform {
        "twitch" => c.twitch.broadcaster.is_some() || c.twitch.bot.is_some(),
        "youtube" => c.youtube.broadcaster.is_some() || c.youtube.bot.is_some(),
        "kick" => c.kick.broadcaster.is_some() || c.kick.bot.is_some(),
        "tiktok" => c.tiktok.broadcaster.is_some() || c.tiktok.bot.is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_we_send_shows_at_once_and_its_copy_takes_its_place() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let mut c = crate::Config::default();
            c.kick.broadcaster = Some(crate::kick::KickAccount {
                user_id: 7,
                username: "Umanana".into(),
                ..Default::default()
            });
            let e = crate::engine::Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(c).0), |_| {});
            let seq = e.chat_local("kick", "hello there", false).unwrap();
            let (_, _, lines) = e.chat_sync(999, 0);
            assert_eq!(
                (lines.len(), lines[0].local, lines[0].user.as_str()),
                (1, true, "Umanana")
            );
            // Kick's copy: same words, its id.
            e.log_chat(&crate::commands::Chat {
                platform: "kick".into(),
                user_id: "7".into(),
                user: "umanana".into(),
                msg_id: "k-1".into(),
                text: "hello there".into(),
                args: [("color".to_string(), serde_json::json!("#B27CFF"))]
                    .into_iter()
                    .collect(),
                ..Default::default()
            });
            let (_, _, lines) = e.chat_sync(999, 0);
            assert_eq!(lines.len(), 1, "no second line");
            assert_eq!((lines[0].local, lines[0].msg_id.as_str()), (false, "k-1"));
            assert_eq!(lines[0].color, "#B27CFF", "Kick's colour, not a guess");
            // A failed send takes its line away.
            let seq2 = e.chat_local("kick", "never arrives", false).unwrap();
            e.chat_unlocal(seq2);
            assert_eq!(e.chat_sync(999, 0).2.len(), 1);
            let _ = seq;
        });
    }

    #[test]
    fn rewards_and_power_ups_show_in_the_chat() {
        let a = |v: Value| v.as_object().cloned().unwrap();
        let line = |kind: u32, args: Value| {
            let args = a(args);
            let ev = event_of(kind, &args)?;
            Some((
                crate::i18n::tr(ev.says),
                ev.cost.and_then(|k| args.get(k)).and_then(Value::as_u64),
                ev.huge,
            ))
        };
        // The rewards every channel has, by what they do.
        let (says, cost, huge) = line(
            194,
            serde_json::json!({"rewardType": "send_highlighted_message", "rewardCost": 40000}),
        )
        .unwrap();
        assert_eq!(
            (says, cost, huge),
            ("redeemed Highlight My Message", Some(40000), false)
        );
        let (says, _, huge) = line(
            194,
            serde_json::json!({"rewardType": "gigantify_an_emote", "gigantifiedEmoteUrl": "u"}),
        )
        .unwrap();
        assert!(says.contains("Gigantify") && huge);
        assert!(line(194, serde_json::json!({"rewardType": "chosen_sub_emote_unlock"}))
            .unwrap()
            .0
            .contains("Unlock"));
        // A power-up is paid in Bits.
        let ev = event_of(196, &a(serde_json::json!({"powerUpType": "celebration", "bits": 50}))).unwrap();
        assert_eq!((ev.cost, ev.cost_icon), (Some("bits"), "gem"));
        assert!(crate::i18n::tr(ev.says).contains("Celebration"));
        let ev = event_of(496, &a(serde_json::json!({"powerUpTitle": "Dance"}))).unwrap();
        assert!(crate::i18n::tr(ev.says).contains("%powerUpTitle%"));
        assert!(event_of(472, &a(serde_json::json!({"streak": 3}))).is_some());
        // A reward with words is a chat message with the reward over it: not shown twice.
        assert!(event_of(112, &a(serde_json::json!({"rawInput": "drink!"}))).is_none());
        let ev = event_of(112, &a(serde_json::json!({"rawInput": "", "rewardImage": "i"}))).unwrap();
        assert_eq!((ev.picture, ev.color), (None, Some("rewardColor")));
    }

    #[test]
    fn a_reward_redemption_without_text_shows_in_chat() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(serde_json::json!({"actions": []}));
            let mut args = crate::model::Obj::new();
            args.insert("user".into(), "PedroZamecki".into());
            args.insert("userName".into(), "pedrozamecki".into());
            args.insert("userId".into(), "123".into());
            args.insert("rewardTitle".into(), "Alongameeeeeento".into());
            args.insert("rewardName".into(), "Alongameeeeeento".into());
            args.insert("rewardCost".into(), 350.into());
            args.insert("rewardPoints".into(), "https://x/coin.png".into());
            args.insert("rewardColor".into(), "#00E5FF".into());
            args.insert("rawInput".into(), "".into());
            e.fire(
                crate::twitch::T_REWARD_REDEMPTION,
                "Reward Redemption",
                "Twitch/Channel Points",
                "twitch",
                &args,
                |_| true,
            );
            let (_, _, lines) = e.chat_sync(u64::MAX, 0);
            assert_eq!(lines.len(), 1);
            let l = &lines[0];
            assert_eq!(l.user, "PedroZamecki");
            assert_eq!(l.event, "redeemed Alongameeeeeento");
            assert_eq!(l.cost, 350);
            assert_eq!(l.cost_icon, "https://x/coin.png");
            assert_eq!(l.accent, "#00E5FF");
            assert_eq!(l.icon, "");
            assert_eq!(l.picture, "");
        });
    }

    #[test]
    fn chat_with_save_history_disabled_does_not_keep_lines() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(serde_json::json!({"actions": []}));
            e.change(|c| c.window.chat.save_history = false);
            e.chat(&crate::commands::Chat {
                platform: "twitch".into(),
                user: "Viewer".into(),
                user_id: "1".into(),
                text: "hello".into(),
                msg_id: "m-1".into(),
                ..Default::default()
            });
            let (_, _, lines) = e.chat_sync(u64::MAX, 0);
            assert_eq!(lines.len(), 0);
        });
    }

    #[test]
    #[ignore]
    fn bench_chat_performance() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        crate::engine::tests::run(async {
            fn get_rss_kb() -> usize {
                std::fs::read_to_string("/proc/self/statm")
                    .ok()
                    .and_then(|s| {
                        let pages: usize = s.split_whitespace().nth(1)?.parse().ok()?;
                        Some(pages * 4)
                    })
                    .unwrap_or(0)
            }

            println!("\n=================== CHAT PERFORMANCE BENCHMARK ===================");
            {
                println!("\n>>> Raw In-Memory Chat Retention (100, 1,000, 10,000 lines stored without clamp)");
                for &count in &[100, 1_000, 10_000] {
                    let rss_before = get_rss_kb();
                    let t0 = std::time::Instant::now();
                    let mut lines: std::collections::VecDeque<Line> = std::collections::VecDeque::with_capacity(count);

                    for i in 0..count {
                        lines.push_back(Line {
                            platform: "twitch".into(),
                            time: "12:34:56".into(),
                            ts: 1730000000000 + (i as i64),
                            user: format!("ChatterUser{}", i % 200),
                            user_id: format!("id_{}", i % 200),
                            text: format!(
                                "Hello world! This is chat message number {} testing heap usage PogChamp Kappa",
                                i
                            ),
                            emotes: vec![
                                (
                                    "PogChamp".into(),
                                    "https://static-cdn.jtvnw.net/emotes/v1/305954156/1.0".into(),
                                ),
                                ("Kappa".into(), "https://static-cdn.jtvnw.net/emotes/v1/25/1.0".into()),
                            ],
                            badges: vec![
                                (
                                    "subscriber".into(),
                                    "https://static-cdn.jtvnw.net/badges/v1/sub.png".into(),
                                ),
                                ("vip".into(), "https://static-cdn.jtvnw.net/badges/v1/vip.png".into()),
                            ],
                            msg_id: format!("msg_id_{}", i),
                            color: "#1E90FF".into(),
                            ..Default::default()
                        });
                    }

                    let elapsed = t0.elapsed();
                    let rss_after = get_rss_kb();
                    let rss_diff = (rss_after as isize) - (rss_before as isize);
                    let bytes_est = lines.len() * std::mem::size_of::<Line>();

                    println!(
                        "Stored: {:>5} lines | Time: {:>7.2?} | Stack size: {:>6.2} KB | RSS delta: {:>+6.2} MB ({:.1} KB/msg)",
                        count,
                        elapsed,
                        (bytes_est as f64) / 1024.0,
                        (rss_diff as f64) / 1024.0,
                        (rss_diff as f64) / (count as f64)
                    );
                }
            }
            for &save_history in &[true, false] {
                println!("\n>>> Mode: save_history = {}", save_history);
                for &count in &[100, 1_000, 10_000] {
                    let e = crate::engine::tests::engine(serde_json::json!({"actions": []}));
                    e.change(|c| {
                        c.window.chat.save_history = save_history;
                    });

                    let rss_before = get_rss_kb();
                    let t0 = std::time::Instant::now();

                    for i in 0..count {
                        let user_id = format!("user_{}", i % 50);
                        let msg_id = format!("msg_{}_{}", count, i);
                        let mut args = crate::model::Obj::new();
                        args.insert("color".into(), "#1E90FF".into());
                        args.insert(
                            "badges".into(),
                            serde_json::json!([{"name": "subscriber", "imageUrl": "https://static-cdn.jtvnw.net/badges/v1/sub.png"}]),
                        );
                        e.chat(&crate::commands::Chat {
                            platform: "twitch".into(),
                            user: format!("User{}", i % 50),
                            user_id,
                            text: format!("Message #{} PogChamp Kappa testing chat throughput!", i),
                            msg_id,
                            emotes: vec![
                                (
                                    "PogChamp".into(),
                                    "https://static-cdn.jtvnw.net/emotes/v1/305954156/1.0".into(),
                                ),
                                ("Kappa".into(), "https://static-cdn.jtvnw.net/emotes/v1/25/1.0".into()),
                            ],
                            args,
                            ..Default::default()
                        });
                    }

                    let elapsed = t0.elapsed();
                    let rss_after = get_rss_kb();
                    let rss_diff = (rss_after as isize) - (rss_before as isize);
                    let msgs_per_sec = (count as f64) / elapsed.as_secs_f64();
                    let (_, _, lines) = e.chat_sync(u64::MAX, 0);

                    println!(
                        "Count: {:>5} | Time: {:>7.2?} | Speed: {:>8.1} msg/s | RSS: {:>6.2} MB ({:>+6.2} MB) | Retained lines: {}",
                        count,
                        elapsed,
                        msgs_per_sec,
                        (rss_after as f64) / 1024.0,
                        (rss_diff as f64) / 1024.0,
                        lines.len()
                    );
                }
            }
            println!("==================================================================\n");
        });
    }

    #[test]
    fn a_message_sent_with_a_reward_has_it_over_it() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(serde_json::json!({"actions": []}));
            let args = |v: Value| v.as_object().cloned().unwrap();
            let chat = |id: &str, extra: Value| {
                let mut a = args(serde_json::json!({"messageRewardId": id, "messageType": "text"}));
                a.extend(args(extra));
                crate::commands::Chat {
                    platform: "twitch".into(),
                    user: "Ana".into(),
                    user_id: "9".into(),
                    text: "drink water".into(),
                    msg_id: id.into(),
                    args: a,
                    ..Default::default()
                }
            };
            e.chat(&chat(
                "m1",
                serde_json::json!({"messageRewardTitle": "Hydrate", "messageRewardCost": 500,
                    "messageRewardImage": "https://x/i.png", "messageRewardColor": "#112233",
                    "messageRewardPoints": "https://x/p.png"}),
            ));
            // The reward is not known (not read yet): still marked as one.
            e.chat(&chat("m2", serde_json::json!({})));
            let mut big = chat("m3", serde_json::json!({"messageType": "power_ups_gigantified_emote"}));
            big.args.remove("messageRewardId");
            e.chat(&big);
            let (_, _, lines) = e.chat_sync(u64::MAX, 0);
            let by = |id: &str| lines.iter().find(|l| l.msg_id == id).unwrap();
            let (one, two, three) = (by("m1"), by("m2"), by("m3"));
            assert_eq!(one.tag, "Redeemed Hydrate");
            assert_eq!(
                (one.cost, one.picture.as_str(), one.accent.as_str()),
                (500, "https://x/i.png", "#112233")
            );
            assert_eq!(one.cost_icon, "https://x/p.png");
            assert_eq!((two.tag.as_str(), two.cost), ("Redeemed a reward", 0));
            assert!(three.huge && three.tag.contains("Gigantified"));
        });
    }

    #[test]
    fn a_me_message_is_an_action() {
        assert_eq!(action_of("/me waves"), Some("waves"));
        assert_eq!(action_of("\x01ACTION waves\x01"), Some("waves"));
        assert_eq!(action_of("hello /me"), None);
        assert_eq!(action_of("/me "), None);
        assert_eq!(action_of("/mention"), None);
    }

    #[test]
    fn a_chat_without_a_login_is_read_only() {
        let mut c = crate::Config::default();
        c.kick.channel = "someone".into();
        assert!(!can_send(&c, "kick"));
        c.kick.bot = Some(Default::default());
        assert!(can_send(&c, "kick"));
        assert!(!can_send(&c, "twitch"));
    }

    #[test]
    fn events_show_in_the_chat_once_and_in_words() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(serde_json::json!({"actions": []}));
            let a = |v: Value| v.as_object().cloned().unwrap();
            e.fire(
                104,
                "ReSub",
                "",
                "twitch",
                &a(serde_json::json!({"user": "Caio", "cumulative": 14, "message": "hi"})),
                |_| true,
            );
            // One gift of a bomb, and a combo gift still growing: the chat says neither.
            e.fire(
                105,
                "GiftSub",
                "",
                "twitch",
                &a(serde_json::json!({"user": "D", "fromGiftBomb": true})),
                |_| true,
            );
            e.fire(
                41002,
                "Gift",
                "",
                "tiktok",
                &a(serde_json::json!({"user": "T", "giftType": 1, "repeatEnd": false})),
                |_| true,
            );
            // Not an event the chat shows.
            e.fire(
                101,
                "Follow",
                "",
                "twitch",
                &a(serde_json::json!({"user": "F"})),
                |_| true,
            );
            let (_, _, lines) = e.chat_sync(u64::MAX, 0);
            let events: Vec<&Line> = lines.iter().filter(|l| !l.event.is_empty()).collect();
            assert_eq!(events.len(), 1);
            assert_eq!((events[0].user.as_str(), events[0].text.as_str()), ("Caio", "hi"));
            assert!(events[0].event.contains("14"), "{}", events[0].event);
        });
    }

    #[test]
    fn set_apart_twitch_messages_are_named() {
        assert!(special("channel_points_highlighted").is_some());
        assert!(special("user_intro").is_some());
        assert!(special("text").is_none());
    }

    #[test]
    fn twitch_pins_read_from_helix() {
        let d = serde_json::json!({"message_id": "m1", "sender_user_name": "Ana", "message": {"text": "hi"},
            "pinned_by_user_name": "Mod", "ends_at": "2026-01-01T00:00:00Z"});
        let p = twitch_pin(&d).unwrap();
        assert_eq!((p.user.as_str(), p.text.as_str(), p.by.as_str()), ("Ana", "hi", "Mod"));
        assert_eq!(p.until, Some(1_767_225_600_000));
        let forever = twitch_pin(&serde_json::json!({"message_id": "m2", "ends_at": null})).unwrap();
        assert!(forever.until.is_none() && forever.live(u64::MAX - 1));
        assert!(twitch_pin(&serde_json::json!({})).is_none());
    }

    #[test]
    fn kick_names_as_their_page_has_them() {
        assert_eq!(kick_slug("AlphaWujuu"), "alphawujuu");
        assert_eq!(kick_slug("Some_Name"), "some-name");
    }

    #[test]
    fn badges_without_a_title_get_a_name() {
        assert_eq!(badge_name("kick:sub_gifter"), "Sub gifter");
        assert_eq!(badge_name("subscriber/12"), "Subscriber");
        assert_eq!(badge_name("tiktok:moderator"), "Moderator");
    }

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
    fn history_of_every_platform_is_one_timeline_with_one_divider() {
        let cfg = std::sync::Arc::new(tokio::sync::watch::channel(crate::Config::default()).0);
        let e = Engine::new(cfg, |_| {});
        let line = |platform: &str, id: &str, ts: i64, text: &str| Line {
            platform: platform.into(),
            msg_id: id.into(),
            user: "u".into(),
            text: text.into(),
            ts,
            ..Default::default()
        };
        // Twitch answers first, Kick a moment later, with messages from between Twitch's.
        e.chat_welcome(
            "twitch",
            vec![line("twitch", "t1", 1000, "a"), line("twitch", "t2", 3000, "c")],
        );
        e.chat_welcome(
            "kick",
            vec![line("kick", "k1", 2000, "b"), line("kick", "k2", 4000, "d")],
        );
        let (_, _, lines) = e.chat_sync(999, 0);
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["a", "b", "c", "d", "Live messages below"]);
        assert_eq!(lines.iter().filter(|l| l.welcome).count(), 1);
        // A reconnect makes no second divider and no duplicates.
        e.chat_welcome(
            "kick",
            vec![line("kick", "k2", 4000, "d"), line("kick", "k3", 5000, "e")],
        );
        let (_, _, lines) = e.chat_sync(999, 0);
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["a", "b", "c", "d", "e", "Live messages below"]);
    }

    #[test]
    fn nothing_to_catch_up_on_means_no_divider() {
        let cfg = std::sync::Arc::new(tokio::sync::watch::channel(crate::Config::default()).0);
        let e = Engine::new(cfg, |_| {});
        e.chat_welcome("twitch", Vec::new());
        assert!(e.chat_sync(999, 0).2.is_empty());
    }

    #[test]
    fn people_in_the_history_are_in_the_community_once() {
        let cfg = std::sync::Arc::new(tokio::sync::watch::channel(crate::Config::default()).0);
        let e = Engine::new(cfg, |_| {});
        let line = |id: &str, user: &str| Line {
            platform: "kick".into(),
            msg_id: id.into(),
            user: user.into(),
            user_id: format!("id-{user}"),
            text: "hi".into(),
            ..Default::default()
        };
        e.chat_welcome("kick", vec![line("1", "Ana"), line("2", "Ana"), line("3", "Bo")]);
        let names: Vec<String> = e
            .users()
            .iter()
            .map(|u| u["display"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["Ana", "Bo"]);
    }

    fn said(e: &Rc<Engine>, platform: &str, text: &str) {
        e.log_chat(&crate::commands::Chat {
            platform: platform.into(),
            user_id: "1".into(),
            user: "Ana".into(),
            msg_id: format!("{platform}-{text}"),
            text: text.into(),
            ..Default::default()
        });
    }

    fn engine_with(limits: crate::ChatLimits) -> Rc<Engine> {
        let mut c = crate::Config::default();
        c.window.chat = limits;
        Engine::new(std::sync::Arc::new(tokio::sync::watch::channel(c).0), |_| {})
    }

    #[test]
    fn the_oldest_lines_go_past_each_platforms_limit_and_the_total() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let e = engine_with(crate::ChatLimits {
                per_platform: 10,
                total: 15,
                ..Default::default()
            });
            for n in 0..12 {
                said(&e, "kick", &format!("k{n}"));
            }
            let texts = |p: &str| {
                e.chat_sync(999, 0)
                    .2
                    .into_iter()
                    .filter(|l| l.platform == p)
                    .map(|l| l.text)
                    .collect::<Vec<_>>()
            };
            assert_eq!(texts("kick").len(), 10, "a platform keeps its limit");
            assert_eq!(texts("kick")[0], "k2", "the oldest went");
            for n in 0..12 {
                said(&e, "twitch", &format!("t{n}"));
            }
            // 10 and 10 are 20: 5 over the total go, the oldest of all (Kick's).
            assert_eq!((texts("kick").len(), texts("twitch").len()), (5, 10));
            assert_eq!(texts("kick")[0], "k7");
            assert_eq!(texts("twitch")[0], "t2");
        });
    }

    #[test]
    fn limits_in_a_file_out_of_range_are_brought_in() {
        let l = crate::ChatLimits {
            save_history: true,
            per_platform: 0,
            total: 5000,
            lifetime: u64::MAX,
            fade_in: u32::MAX,
            fade_out: 0,
        }
        .clamped();
        assert_eq!(
            (l.per_platform, l.total),
            (crate::ChatLimits::LEAST, crate::ChatLimits::MOST)
        );
        assert!(l.lifetime < u64::MAX && l.fade_in <= 3000);
        // Only what differs from the defaults is saved.
        let json = serde_json::to_value(crate::WindowState::default()).unwrap();
        assert!(json.get("chat").is_none());
    }

    #[test]
    fn lines_past_their_lifetime_leave_the_memory() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let e = engine_with(crate::ChatLimits {
                lifetime: 300,
                ..Default::default()
            });
            said(&e, "kick", "old");
            let at = e.chat_sync(999, 0).2[0].ts as u64;
            // Not yet, then past it.
            assert_eq!(e.chat_log_mut().age_out(at + 299_000), 0);
            said(&e, "kick", "new");
            assert_eq!(
                e.chat_log_mut().age_out(at + 301_000),
                2,
                "both are older than five minutes by now"
            );
            assert!(e.chat_sync(999, 0).2.is_empty());
            // The sweep itself, when a line is old by the clock.
            let e = engine_with(crate::ChatLimits {
                lifetime: 300,
                ..Default::default()
            });
            said(&e, "kick", "old");
            said(&e, "kick", "fresh");
            e.chat_log_mut().lines[0].ts -= 301_000;
            e.chat_trim();
            let left: Vec<String> = e.chat_sync(999, 0).2.into_iter().map(|l| l.text).collect();
            assert_eq!(left, ["fresh"]);
            // With no lifetime, nothing ages out.
            let e = engine_with(crate::ChatLimits::default());
            said(&e, "kick", "stays");
            assert_eq!(e.chat_log_mut().age_out(u64::MAX / 2), 0);
        });
    }
}
