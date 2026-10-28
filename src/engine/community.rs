//! Quotes, credits, users and first words: Streamer.bot's quotes.dat (kept in the config), credits.cache
//! and users.dat (kept in community.json next to it).

use super::{n, s, Ctx, Engine};
use crate::commands::Chat;
use crate::model::Obj;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

/// GetCredits' events, in its order.
const EVENTS: [&str; 12] = [
    "follows",
    "cheers",
    "subs",
    "resubs",
    "giftsubs",
    "giftbombs",
    "raided",
    "rewardredemptions",
    "goalcontributions",
    "gameupdates",
    "pyramids",
    "hypetrains",
];

/// Credits forget a stream 12 hours after it was last written, like Streamer.bot's default.
const STALE: u64 = 12 * 60 * 60;

pub const T_QUOTE_ADDED: u32 = 601;

/// The credits event a trigger adds to.
fn event_of(kind: u32) -> Option<&'static str> {
    Some(match kind {
        101 | 35011 | 41004 | 4018 => "follows",
        102 => "cheers",
        103 | 35014 | 4008 => "subs",
        104 | 35015 | 4009 => "resubs",
        105 | 35016 | 4014 => "giftsubs",
        106 => "giftbombs",
        107 => "raided",
        112 | 35024 => "rewardredemptions",
        149 => "goalcontributions",
        118 => "gameupdates",
        162 => "pyramids",
        111 => "hypetrains",
        _ => return None,
    })
}

/// What happened this stream, as display names.
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Credits {
    events: BTreeMap<String, Vec<String>>,
    /// Who spoke: "users", and by role "moderators", "vips", "subscribers".
    users: BTreeMap<String, Vec<String>>,
    /// Channel point redemptions per name, for top channelRewards.
    redeems: BTreeMap<String, u64>,
    /// Unix seconds.
    last_write: u64,
}

impl Credits {
    fn add(&mut self, list: &str, name: &str, users: bool) {
        let to = if users { &mut self.users } else { &mut self.events };
        let l = to.entry(list.into()).or_default();
        if !name.is_empty() && !l.iter().any(|x| x == name) {
            l.push(name.into());
        }
    }

    /// Streamer.bot's GetCredits shape.
    pub fn json(&self) -> Value {
        let list = |m: &BTreeMap<String, Vec<String>>, k: &str| json!(m.get(k).cloned().unwrap_or_default());
        let events: serde_json::Map<String, Value> =
            EVENTS.iter().map(|k| ((*k).into(), list(&self.events, k))).collect();
        let mut top: Vec<(&String, &u64)> = self.redeems.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        json!({
            "events": events,
            "hypeTrain": {"conductors": [], "contributors": []},
            "users": {
                "editors": [],
                "moderators": list(&self.users, "moderators"),
                "subscribers": list(&self.users, "subscribers"),
                "vips": list(&self.users, "vips"),
                "groups": [],
                "users": list(&self.users, "users"),
            },
            "groups": {},
            // ponytail: bits tops need Helix bits/leaderboard; add when an overlay uses them.
            "top": {"allBits": [], "monthBits": [], "weekBits": [], "channelRewards": top.iter().map(|x| x.0).collect::<Vec<_>>()},
            "custom": {},
        })
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Community {
    /// users.dat's users: id, name, display, role, subscribed, type, firstMessage, present,
    /// previousActive, lastActive, exempt.
    pub users: Vec<Obj>,
    pub credits: Credits,
    /// First Words: "platform:id" of who spoke since the last reset.
    #[serde(skip)]
    speakers: HashSet<String>,
    /// Changes since the last save.
    #[serde(skip)]
    unsaved: u32,
}

fn file() -> std::path::PathBuf {
    crate::config_path().with_file_name("community.json")
}

fn unix() -> u64 {
    crate::fmt::unix_now().as_secs()
}

/// Streamer.bot's never.
const NEVER: &str = "0001-01-01T00:00:00";

/// "YYYY-MM-DDTHH:MM:SS..." as a time (the offset is kept as written, no conversion).
fn parse(iso: &str) -> Option<crate::fmt::Time> {
    let p = |a: usize, b: usize| iso.get(a..b)?.parse::<u32>().ok();
    Some(crate::fmt::Time {
        year: p(0, 4)? as i32,
        month: p(5, 7)?,
        day: p(8, 10)?,
        hour: p(11, 13)?,
        min: p(14, 16)?,
        sec: p(17, 19)?,
        nanos: 0,
        weekday: 0,
        offset: 0,
    })
}

impl Engine {
    pub fn load_community(&self) {
        let Ok(text) = std::fs::read_to_string(file()) else {
            return;
        };
        let Ok(mut c) = serde_json::from_str::<Community>(&text) else {
            return;
        };
        if unix().saturating_sub(c.credits.last_write) > STALE {
            c.credits = Credits::default();
        }
        self.st.borrow_mut().community = c;
    }

    /// Write community.json now.
    pub fn save_community(&self) {
        let text = serde_json::to_string(&self.st.borrow().community).unwrap_or_default();
        let (path, tmp) = (file(), file().with_extension("json.tmp"));
        if std::fs::write(&tmp, text)
            .and_then(|()| std::fs::rename(&tmp, &path))
            .is_err()
        {
            crate::say(format!("can't save {}", path.display()));
        }
    }

    /// Saved every 50 changes, and on quit.
    // ponytail: a crash loses up to 49 chat lines' worth of users and credits; a timer if that matters.
    fn community_changed(&self) {
        let n = {
            let c = &mut self.st.borrow_mut().community;
            c.unsaved = (c.unsaved + 1) % 50;
            c.unsaved
        };
        if n == 0 && !cfg!(test) {
            self.save_community();
        }
    }

    /// Whether this is `user`'s first message on `platform` since First Words were reset.
    pub fn first_words(&self, platform: &str, user: &str) -> bool {
        self.st
            .borrow_mut()
            .community
            .speakers
            .insert(format!("{platform}:{}", user.to_lowercase()))
    }

    /// Forget who spoke: everyone's (None) or one platform's (its stream started).
    pub fn reset_first_words(&self, platform: Option<&str>) {
        let speakers = &mut self.st.borrow_mut().community.speakers;
        match platform {
            Some(p) => speakers.retain(|k| !k.starts_with(&format!("{p}:"))),
            None => speakers.clear(),
        }
    }

    pub fn credits(&self) -> Value {
        self.st.borrow().community.credits.json()
    }

    pub fn reset_credits(self: &Rc<Self>) {
        self.st.borrow_mut().community.credits = Credits::default();
        self.community_changed();
        crate::ui::changed();
    }

    /// TestCredits: a few names in every list.
    pub fn test_credits(self: &Rc<Self>) -> Value {
        {
            let c = &mut self.st.borrow_mut().community.credits;
            for (i, k) in EVENTS.iter().enumerate() {
                c.add(k, &format!("TestUser{}", i + 1), false);
            }
            for k in ["users", "moderators", "vips", "subscribers"] {
                c.add(k, "TestUser1", true);
            }
            c.last_write = unix();
        }
        self.community_changed();
        self.credits()
    }

    pub fn users(&self) -> Vec<Obj> {
        self.st.borrow().community.users.clone()
    }

    /// One user of the Community tab.
    pub fn community_user(&self, platform: &str, id: &str) -> Option<Obj> {
        let st = self.st.borrow();
        st.community
            .users
            .iter()
            .find(|u| s(u, "type") == platform && s(u, "id") == id)
            .cloned()
    }

    /// Credits and users from a trigger that fired.
    pub(super) fn record(self: &Rc<Self>, kind: u32, args: &Obj) {
        let name = |k: &str| s(args, k).to_string();
        let who = [name("user"), name("userName"), name("gameName")]
            .into_iter()
            .find(|x| !x.is_empty())
            .unwrap_or_default();
        match kind {
            118 => self.credit("gameupdates", &name("gameName")),
            111 => self.credit("hypetrains", &format!("Level {}", n(args, "level"))),
            112 | 35024 => {
                self.credit("rewardredemptions", &who);
                *self.st.borrow_mut().community.credits.redeems.entry(who).or_default() += 1;
            }
            _ => {
                if let Some(ev) = event_of(kind) {
                    self.credit(ev, &who);
                }
            }
        }
    }

    fn credit(&self, list: &str, name: &str) {
        {
            let c = &mut self.st.borrow_mut().community.credits;
            c.add(list, name, false);
            c.last_write = unix();
        }
        self.community_changed();
    }

    /// A chat message: the users present for the credits, and the user's record.
    pub(super) fn spoke(&self, c: &Chat) {
        let (id, platform, display) = (c.user_id.as_str(), c.platform.as_str(), c.user.as_str());
        if id.is_empty() || c.bot || c.internal {
            return;
        }
        let role = if c.broadcaster {
            4
        } else if c.moderator {
            3
        } else if c.vip {
            2
        } else {
            1
        };
        let now = super::globals::iso_now();
        {
            let mut st = self.st.borrow_mut();
            let cm = &mut st.community;
            cm.credits.add("users", display, true);
            match role {
                3 => cm.credits.add("moderators", display, true),
                2 => cm.credits.add("vips", display, true),
                _ => {}
            }
            if c.subscriber {
                cm.credits.add("subscribers", display, true);
            }
            cm.credits.last_write = unix();
            let i = match cm
                .users
                .iter()
                .position(|u| s(u, "type") == platform && s(u, "id") == id)
            {
                Some(i) => i,
                None => {
                    let u = json!({"id": id, "name": "", "display": "", "role": 1, "subscribed": false,
                        "type": platform, "firstMessage": now, "present": true, "previousActive": NEVER,
                        "lastActive": NEVER, "exempt": false});
                    cm.users.push(u.as_object().cloned().unwrap_or_default());
                    cm.users.len() - 1
                }
            };
            let u = &mut cm.users[i];
            // A new visit (half an hour quiet): the last one becomes the previous.
            let last = s(u, "lastActive").to_string();
            let quiet = parse(&last).zip(parse(&now)).is_none_or(|(a, b)| {
                let mins = |t: crate::fmt::Time| {
                    ((t.year * 372 + t.month as i32 * 31 + t.day as i32) * 1440) + (t.hour * 60 + t.min) as i32
                };
                mins(b) - mins(a) > 30
            });
            if quiet && last != NEVER {
                u.insert("previousActive".into(), last.clone().into());
            }
            // What the Community tab shows: messages sent, and the time spent chatting (gaps under half an
            // hour between messages add up).
            let (messages, active) = (n(u, "messages") as u64, n(u, "activeSecs") as u64);
            let gap = super::rest_parse_iso(&last).map_or(u64::MAX, |t| unix().saturating_sub(t as u64));
            u.insert("messages".into(), json!(messages + 1));
            u.insert(
                "activeSecs".into(),
                json!(if gap <= 30 * 60 { active + gap } else { active }),
            );
            for (k, v) in [
                ("name", json!(s(&c.args, "userName"))),
                ("display", json!(display)),
                ("role", json!(role)),
                ("subscribed", json!(c.subscriber)),
                ("present", json!(true)),
                ("lastActive", json!(now)),
            ] {
                u.insert(k.into(), v);
            }
        }
        self.community_changed();
    }

    /// The quote sub-actions (1000, 1035-1037) and Reset First Words / Reset Credits (1026, 1027).
    pub(super) fn community_sub(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) {
        let quote_id = |ctx: &Ctx| {
            let id = o
                .get("quoteId")
                .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from));
            self.expand(ctx, &id.unwrap_or_default()).trim().parse::<u64>().ok()
        };
        match kind {
            1026 => self.reset_first_words(None),
            1027 => self.reset_credits(),
            1035 => drop(
                ctx.args
                    .insert("quoteCount".into(), self.cfg.borrow().quotes.len().into()),
            ),
            1000 => {
                let id = quote_id(ctx);
                let cfg = self.cfg.borrow();
                let quotes = &cfg.quotes;
                let q = if n(o, "quoteType") as u64 == 1 {
                    quotes.iter().find(|q| q.get("id").and_then(Value::as_u64) == id)
                } else if quotes.is_empty() {
                    None
                } else {
                    quotes.get((crate::math::random() * quotes.len() as f64) as usize % quotes.len())
                };
                if let Some(q) = q {
                    ctx.args.extend(quote_args(q));
                }
            }
            1036 => {
                let id = quote_id(ctx).unwrap_or(0);
                let found = self
                    .cfg
                    .borrow()
                    .quotes
                    .iter()
                    .any(|q| q.get("id").and_then(Value::as_u64) == Some(id));
                let a = &mut ctx.args;
                a.insert("success".into(), found.into());
                if found {
                    self.change(|c| c.quotes.retain(|q| q.get("id").and_then(Value::as_u64) != Some(id)));
                    a.insert("quoteId".into(), id.to_string().into());
                } else {
                    a.insert(
                        "error".into(),
                        format!("Unable to delete quote {id}, it does not exist").into(),
                    );
                }
            }
            1037 => {
                let text = self.expand(ctx, s(o, "quoteVariable"));
                // The caller, else the broadcaster.
                let a = &ctx.args;
                let (user, id, platform) = if s(a, "userId").is_empty() && !a.contains_key("userId") {
                    (s(a, "broadcastUser"), s(a, "broadcastUserId").to_string(), "twitch")
                } else {
                    let id = a
                        .get("userId")
                        .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from));
                    (s(a, "user"), id.unwrap_or_default(), s(a, "userType"))
                };
                let q = self.add_quote(&text, user, &id, if platform.is_empty() { "twitch" } else { platform });
                ctx.args.insert("success".into(), true.into());
                ctx.args.extend(quote_args(&q));
            }
            _ => {}
        }
    }

    /// Adds a quote (Quote Added fires) and gives it back.
    pub fn add_quote(self: &Rc<Self>, text: &str, user: &str, user_id: &str, platform: &str) -> Obj {
        let (game_id, game) = crate::twitch::current_game();
        let id = self
            .cfg
            .borrow()
            .quotes
            .iter()
            .filter_map(|q| q.get("id").and_then(Value::as_u64))
            .max()
            .unwrap_or(0)
            + 1;
        let q = json!({"id": id, "timestamp": super::globals::iso_now(), "userId": user_id, "user": user,
            "platform": platform, "gameId": game_id, "gameName": game, "quote": text});
        let q = q.as_object().cloned().unwrap_or_default();
        self.change(|c| c.quotes.push(q.clone()));
        let mut args = quote_args(&q);
        args.retain(|k, _| {
            matches!(
                k.as_str(),
                "quote" | "quoteId" | "quoteUser" | "quoteGame" | "quoteTime"
            )
        });
        self.fire(T_QUOTE_ADDED, "Quote Added", "Core/Quotes", "Quote", &args, |_| true);
        self.emit("Quote", "Added", || json!(args));
        q
    }
}

/// A quote's variables, as Get Quote sets them.
pub fn quote_args(q: &Obj) -> Obj {
    let ts = s(q, "timestamp");
    let day = parse(ts).map(|t| crate::fmt::date(&t, "d")).unwrap_or_default();
    let mut a = Obj::new();
    for (k, v) in [
        ("quoteTimestamp", json!(ts)),
        ("quoteTime", json!(day)),
        (
            "quoteId",
            json!(q.get("id").and_then(Value::as_u64).unwrap_or(0).to_string()),
        ),
        ("quoteUserId", json!(s(q, "userId"))),
        ("quoteUser", json!(s(q, "user"))),
        ("quotePlatform", json!(s(q, "platform"))),
        ("quoteGame", json!(s(q, "gameName"))),
        ("quote", json!(s(q, "quote"))),
    ] {
        a.insert(k.into(), v);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Rc<Engine> {
        let cfg = std::sync::Arc::new(tokio::sync::watch::channel(crate::Config::default()).0);
        Engine::new(cfg, |_| {})
    }

    #[test]
    fn credits_and_users_from_triggers() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let e = engine();
            let args = |v: Value| v.as_object().cloned().unwrap();
            e.record(101, &args(json!({"user": "Ana"})));
            e.record(101, &args(json!({"user": "Ana"})));
            e.record(112, &args(json!({"user": "Bo"})));
            e.spoke(&Chat {
                platform: "twitch".into(),
                user_id: "7".into(),
                user: "Cy".into(),
                moderator: true,
                ..Default::default()
            });
            let c = e.credits();
            assert_eq!(c["events"]["follows"], json!(["Ana"]));
            assert_eq!(c["top"]["channelRewards"], json!(["Bo"]));
            assert_eq!(c["users"]["moderators"], json!(["Cy"]));
            let u = &e.users()[0];
            assert_eq!((s(u, "display"), u["role"].as_u64()), ("Cy", Some(3)));
            assert!(e.first_words("twitch", "Cy") && !e.first_words("twitch", "cy"));
            e.reset_first_words(Some("kick"));
            assert!(!e.first_words("twitch", "cy"));
            e.reset_first_words(None);
            assert!(e.first_words("twitch", "cy"));
        });
    }

    #[test]
    fn quotes_add_get_delete() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let e = engine();
            let q = e.add_quote("hi there", "Ana", "1", "twitch");
            assert_eq!(q["id"], 1);
            e.add_quote("second", "Bo", "2", "kick");
            let mut ctx = Ctx { args: Obj::new() };
            let o = |v: Value| v.as_object().cloned().unwrap();
            e.community_sub(&mut ctx, &o(json!({"quoteType": 1, "quoteId": 2})), 1000);
            assert_eq!((s(&ctx.args, "quote"), s(&ctx.args, "quoteUser")), ("second", "Bo"));
            e.community_sub(&mut ctx, &o(json!({"quoteId": 1})), 1036);
            assert_eq!(ctx.args["success"], true);
            e.community_sub(&mut ctx, &o(json!({})), 1035);
            assert_eq!(ctx.args["quoteCount"], 1);
        });
    }
}
