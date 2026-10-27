//! The engine, continued: chat commands, timed actions, user groups, and the sub-actions that reach
//! outside (Fetch URL, UDP Broadcast, Run a Program).

use super::{b, n, new_state, s, Ctx, Engine};
use crate::commands::{self, Chat};
use crate::model::Obj;
use crate::vars::{self, auto_type, text};
use serde_json::{json, Value};
use std::rc::Rc;
use std::time::{Duration, Instant};

const T_COMMAND: u32 = 401;
const T_COOLDOWN: u32 = 402;
const T_TIMER: u32 = 701;
const T_GROUP_ADDED: u32 = 32001;
const T_GROUP_REMOVED: u32 = 32002;
const T_GROUP_CLEARED: u32 = 32003;
const T_GROUP_USERS_ADDED: u32 = 32004;
const T_GROUP_USERS_REMOVED: u32 = 32005;

/// Get Command(s) Group State and Get Commands: "" or "All" is every group.
fn in_group(c: &Obj, group: &str) -> bool {
    group.is_empty() || group == "All" || s(c, "group") == group
}

/// A color as Streamer.bot writes it ("#AARRGGBB" or "#RRGGBB") in OBS's order (0xAABBGGRR).
fn obs_color(c: &str) -> i64 {
    let h = c.trim().trim_start_matches('#');
    let argb = match u32::from_str_radix(h, 16) {
        Ok(v) if h.len() == 6 => 0xFF00_0000 | v,
        Ok(v) => v,
        Err(_) => 0xFF00_0000,
    };
    let (a, r, g, b2) = (argb >> 24, (argb >> 16) & 0xFF, (argb >> 8) & 0xFF, argb & 0xFF);
    i64::from(a << 24 | b2 << 16 | g << 8 | r)
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

pub(super) fn civil_pub(secs: i64) -> crate::fmt::Time {
    civil(secs)
}

/// A UTC civil date and time from Unix seconds.
fn civil(secs: i64) -> crate::fmt::Time {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    crate::fmt::Time {
        year,
        month,
        day,
        hour: (rem / 3600) as u32,
        min: (rem % 3600 / 60) as u32,
        sec: (rem % 60) as u32,
        nanos: 0,
        weekday: (days + 4).rem_euclid(7) as u32,
        offset: 0,
    }
}

pub(super) fn parse_iso_pub(t: &str) -> Option<i64> {
    parse_iso(t)
}

/// Unix seconds from an ISO 8601 UTC time as the platforms send it ("2024-08-17T12:34:56Z",
/// fractions ignored).
fn parse_iso(t: &str) -> Option<i64> {
    let n = |r: std::ops::Range<usize>| t.get(r)?.parse::<i64>().ok();
    let days = days_from_civil(n(0..4)?, n(5..7)?, n(8..10)?);
    // An offset after the seconds (and any fraction): "+02:00" is two hours ahead of UTC.
    let tail = t
        .get(19..)
        .unwrap_or("")
        .trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match (tail.get(..1), tail.get(1..3).and_then(|h| h.parse::<i64>().ok())) {
        (Some(sign @ ("+" | "-")), Some(h)) => {
            let m = tail.get(4..6).and_then(|m| m.parse::<i64>().ok()).unwrap_or(0);
            (h * 3600 + m * 60) * if sign == "+" { 1 } else { -1 }
        }
        _ => 0,
    };
    Some(days * 86_400 + n(11..13)? * 3600 + n(14..16)? * 60 + n(17..19)? - offset)
}

/// The calendar time between two Unix times: years, months, days, hours, minutes, seconds.
/// Whole months are added first (a day past the month's end clamps to its last day), then the
/// rest is counted.
fn age(from: i64, to: i64) -> [i64; 6] {
    let to = to.max(from);
    let (a, b2) = (civil(from), civil(to));
    let add = |months: i64| {
        let m0 = i64::from(a.year) * 12 + i64::from(a.month) - 1 + months;
        let (y, m) = (m0.div_euclid(12), m0.rem_euclid(12) + 1);
        let len =
            days_from_civil(y + i64::from(m == 12), if m == 12 { 1 } else { m + 1 }, 1) - days_from_civil(y, m, 1);
        let day = i64::from(a.day).min(len);
        days_from_civil(y, m, day) * 86_400 + from.rem_euclid(86_400)
    };
    let mut months = (i64::from(b2.year) - i64::from(a.year)) * 12 + i64::from(b2.month) - i64::from(a.month);
    while months > 0 && add(months) > to {
        months -= 1;
    }
    let rest = to - add(months);
    [
        months / 12,
        months % 12,
        rest / 86_400,
        rest % 86_400 / 3600,
        rest % 3600 / 60,
        rest % 60,
    ]
}

/// "1 year, 10 months, 5 days, ..." and "1y, 10m, 5d, ..." (zero units left out, as Streamer.bot).
fn age_text(v: &[i64; 6]) -> (String, String) {
    const LONG: [&str; 6] = ["year", "month", "day", "hour", "minute", "second"];
    const SHORT: [&str; 6] = ["y", "m", "d", "h", "m", "s"];
    let (mut l, mut sh) = (Vec::new(), Vec::new());
    for (i, x) in v.iter().enumerate().filter(|(_, x)| **x > 0) {
        l.push(format!("{x} {}{}", LONG[i], if *x == 1 { "" } else { "s" }));
        sh.push(format!("{x}{}", SHORT[i]));
    }
    (l.join(", "), sh.join(", "))
}

/// A list field: a JSON array (of strings or {title}), or text with one item per line or comma.
fn list(v: Option<&Value>) -> Vec<String> {
    let items: Vec<String> = match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| {
                x.as_str()
                    .or_else(|| x["title"].as_str())
                    .or_else(|| x["text"].as_str())
            })
            .map(String::from)
            .collect(),
        Some(Value::String(t)) => {
            let sep = if t.contains('\n') { '\n' } else { ',' };
            t.split(sep).map(|x| x.trim().to_string()).collect()
        }
        _ => Vec::new(),
    };
    items.into_iter().filter(|x| !x.trim().is_empty()).collect()
}

/// A Helix poll as Streamer.bot's poll.* arguments.
pub(crate) fn poll_args(p: &Value, args: &mut Obj) {
    let now = crate::fmt::unix_now().as_secs() as i64;
    let started = p["started_at"].as_str().and_then(parse_iso).unwrap_or(now);
    let duration = p["duration"].as_i64().unwrap_or(0);
    let choices = p["choices"].as_array().cloned().unwrap_or_default();
    let (mut votes, mut points) = (0, 0);
    for (i, c) in choices.iter().enumerate() {
        let (v, r) = (
            c["votes"].as_i64().unwrap_or(0),
            c["channel_points_votes"].as_i64().unwrap_or(0),
        );
        votes += v - r;
        points += r;
        for (k, x) in [("id", c["id"].clone()), ("title", c["title"].clone())] {
            args.insert(format!("poll.choice{i}.{k}"), x);
        }
        args.insert(format!("poll.choice{i}.votes"), (v - r).into());
        args.insert(format!("poll.choice{i}.rewardVotes"), r.into());
        args.insert(format!("poll.choice{i}.totalVotes"), v.into());
    }
    for (k, x) in [
        ("active", true.into()),
        ("poll.Id", p["id"].clone()),
        ("poll.Title", p["title"].clone()),
        ("poll.StartedAt", p["started_at"].clone()),
        ("poll.EndsAt", crate::fmt::date(&civil(started + duration), "o").into()),
        ("poll.Duration", duration.into()),
        ("poll.DurationRemaining", (started + duration - now).max(0).into()),
        ("poll.choices.count", choices.len().into()),
        ("poll.votes", votes.into()),
        ("poll.rewardVotes", points.into()),
        ("poll.totalVotes", (votes + points).into()),
    ] {
        args.insert(k.into(), x);
    }
}

/// A Helix prediction as Streamer.bot's prediction.* arguments.
pub(crate) fn prediction_args(p: &Value, args: &mut Obj) {
    args.insert("active".into(), (p["status"] == "ACTIVE").into());
    for (k, f) in [
        ("Id", "id"),
        ("Title", "title"),
        ("CreatedAt", "created_at"),
        ("PredictionWindow", "prediction_window"),
    ] {
        args.insert(format!("prediction.{k}"), p[f].clone());
    }
    for (i, c) in p["outcomes"].as_array().into_iter().flatten().enumerate() {
        for (k, f) in [
            ("id", "id"),
            ("title", "title"),
            ("color", "color"),
            ("users", "users"),
            ("points", "channel_points"),
        ] {
            args.insert(format!("prediction.outcome{i}.{k}"), c[f].clone());
        }
    }
}

/// A JSON value as arguments: `prefix.key.sub`, arrays as `prefix.list[0]` (as Streamer.bot puts
/// OBS Raw and Get Scene Item Properties results).
fn flatten(prefix: &str, v: &Value, args: &mut Obj) {
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                flatten(&format!("{prefix}.{k}"), x, args);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                flatten(&format!("{prefix}[{i}]"), x, args);
            }
        }
        x => {
            args.insert(prefix.into(), x.clone());
        }
    }
}

impl Engine {
    /// A chat message: run the commands it matches. How many ran.
    pub fn chat(self: &Rc<Self>, chat: &Chat) -> usize {
        self.st.borrow_mut().chat_lines += 1;
        self.log_chat(chat);
        self.spoke(chat);
        let cmds: Vec<Obj> = self.cfg.borrow().commands.clone();
        let mut ran = 0;
        for c in &cmds {
            let enabled = c.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            if !enabled || (n(c, "sources") as u64 & chat.source) == 0 {
                continue;
            }
            if (chat.bot && b(c, "ignoreBotAccount", true)) || (chat.internal && b(c, "ignoreInternal", true)) {
                continue;
            }
            let Some((alias, raw, matched)) = commands::matches(c, &chat.text) else {
                continue;
            };
            if !commands::permitted(c, chat, &|g| self.user_in_group(g, &chat.user_id)) {
                continue;
            }
            let id = s(c, "id").to_string();
            let user_key = format!("{id}:{}", chat.user_id);
            let mut args = chat.args.clone();
            args.insert("command".into(), alias.into());
            args.insert("commandId".into(), id.as_str().into());
            args.insert("commandName".into(), s(c, "name").into());
            args.insert("commandSource".into(), chat.platform.as_str().into());
            let kind = if chat.source == 2 || chat.source == 4 {
                "whisper"
            } else {
                "message"
            };
            args.insert("commandType".into(), kind.into());
            args.insert("msgId".into(), chat.msg_id.as_str().into());
            // Cooldowns (the broadcaster has none).
            let now = Instant::now();
            let left = |k: &str| {
                let st = self.st.borrow();
                st.cooldowns
                    .get(k)
                    .map_or(0.0, |t| t.saturating_duration_since(now).as_secs_f64())
            };
            let (global, user) = (left(&id), left(&user_key));
            if !chat.broadcaster && (global > 0.0 || user > 0.0) {
                let secs = |x: f64| Value::from(x.ceil() as i64);
                args.insert("cooldownLeft".into(), secs(global.max(user)));
                args.insert("globalCooldownLeft".into(), secs(global));
                args.insert("userCooldownLeft".into(), secs(user));
                self.fire(T_COOLDOWN, "Command Cooldown", "Core/Commands", "command", &args, |t| {
                    s(t, "commandId") == id
                });
                self.emit("Command", "Cooldown", || Value::Object(args));
                continue;
            }
            {
                let mut st = self.st.borrow_mut();
                for (k, secs) in [(&id, n(c, "globalCooldown")), (&user_key, n(c, "userCooldown"))] {
                    if secs > 0.0 {
                        st.cooldowns.insert(k.clone(), now + Duration::from_secs_f64(secs));
                    }
                }
            }
            let counter = self.count(&id, b(c, "persistCounter", false));
            let user_counter = self.count(&user_key, b(c, "persistUserCounter", false));
            args.insert("counter".into(), counter.into());
            args.insert("userCounter".into(), user_counter.into());
            // Regex commands: words from the message without its leading "!" (as Streamer.bot).
            let words = if n(c, "mode") as i64 == 1 {
                raw.trim_start_matches('!').to_string()
            } else {
                raw.clone()
            };
            commands::input_args(&raw, &words, &mut args);
            args.extend(matched);
            self.emit("Command", "Triggered", || Value::Object(args.clone()));
            ran += self.fire(T_COMMAND, "Command Triggered", "Core/Commands", "command", &args, |t| {
                s(t, "commandId") == id
            });
            ran += usize::from(self.command_reply(c, &args, chat));
        }
        ran
    }

    /// Dev mode (`STREAMERZ_DEV` set).
    pub fn dev(&self) -> bool {
        self.st.borrow().dev_out.is_some()
    }

    /// Dev mode: keep a chat line instead of sending it. Whether it was kept.
    pub fn dev_out(&self, platform: &str, text: &str, bot: bool) -> bool {
        let mut st = self.st.borrow_mut();
        let Some(out) = st.dev_out.as_mut() else {
            return false;
        };
        if out.len() >= 500 {
            out.remove(0); // ponytail: dev only, a VecDeque if this ever matters
        }
        out.push(json!({"platform": platform, "text": text, "bot": bot}));
        true
    }

    /// Dev mode: the chat lines kept since the last call. None outside dev mode.
    pub fn dev_take(&self) -> Option<Vec<Value>> {
        self.st.borrow_mut().dev_out.as_mut().map(std::mem::take)
    }

    /// Dev mode: a chat message as if it came from `platform`, run through the commands. How many
    /// ran. `role`: broadcaster, moderator, vip, subscriber or viewer.
    pub fn inject_chat(
        self: &Rc<Self>,
        platform: &str,
        user: &str,
        role: &str,
        text: &str,
        badges: Option<Value>,
    ) -> Result<usize, String> {
        let source = match platform {
            "twitch" => 1,
            "youtube" => 1024,
            "kick" => 2_097_152,
            "tiktok" => 4_194_304,
            _ => return Err(format!("unknown platform '{platform}'")),
        };
        let mut args = Obj::new();
        for k in ["user", "userName", "userId"] {
            args.insert(k.into(), user.into());
        }
        args.insert("message".into(), text.into());
        args.insert("platform".into(), platform.into());
        // As Streamer.bot passes them: [{name, version, imageUrl}].
        if let Some(b) = badges.filter(Value::is_array) {
            args.insert("badges".into(), b);
        }
        Ok(self.chat(&Chat {
            platform: platform.into(),
            source,
            user_id: user.into(),
            user: user.into(),
            broadcaster: role == "broadcaster",
            moderator: role == "moderator",
            vip: role == "vip",
            subscriber: role == "subscriber",
            msg_id: super::uuid(),
            text: text.into(),
            emotes: if platform == "kick" {
                crate::kick::kick_emotes(text)
            } else {
                Vec::new()
            },
            args,
            ..Default::default()
        }))
    }

    /// Add one to a command counter; the new count.
    fn count(self: &Rc<Self>, key: &str, persisted: bool) -> i64 {
        let bump = |m: &mut Obj| {
            let v = m.get(key).and_then(Value::as_i64).unwrap_or(0) + 1;
            m.insert(key.into(), v.into());
            v
        };
        if persisted {
            let mut v = 0;
            self.change(|c| v = bump(&mut c.counters));
            v
        } else {
            bump(&mut self.st.borrow_mut().counters)
        }
    }

    /// A user's id on a platform from their name, as chat reports it (and groups keep it); the
    /// name itself when it can't be looked up (an id already, or the platform not connected).
    async fn user_id_for(self: &Rc<Self>, platform: &str, login: &str) -> String {
        let looked = match platform {
            "twitch" if !login.bytes().all(|b| b.is_ascii_digit()) => match self.twitch() {
                Some(tw) => tw.resolve_user_id(login).await.ok(),
                None => None,
            },
            "kick" if !login.bytes().all(|b| b.is_ascii_digit()) && self.kick().is_some() => {
                let slug = crate::commands::url_encode(&login.to_lowercase());
                self.kick_start()
                    .api("GET", &format!("/channels?slug={slug}"), None)
                    .await
                    .ok()
                    .and_then(|v| v["data"][0]["broadcaster_user_id"].as_u64())
                    .map(|i| i.to_string())
            }
            _ => None,
        };
        looked.unwrap_or_else(|| login.to_string())
    }

    fn user_in_group(&self, group: &str, user_id: &str) -> bool {
        self.cfg.borrow().groups.iter().any(|g| {
            s(g, "name").eq_ignore_ascii_case(group)
                && g.get("userIds")
                    .and_then(Value::as_array)
                    .is_some_and(|u| u.iter().any(|x| x == user_id))
        })
    }

    /// (Re)start the timed actions: one task per enabled timer, none for disabled ones.
    pub fn timers(self: &Rc<Self>) {
        let old = std::mem::take(&mut self.st.borrow_mut().timers);
        for h in old {
            h.abort();
        }
        let timers: Vec<Obj> = self.cfg.borrow().timers.clone();
        for t in timers.into_iter().filter(|t| b(t, "enabled", false)) {
            let me = self.clone();
            let h = tokio::task::spawn_local(async move { me.timer(t).await });
            self.st.borrow_mut().timers.push(h.abort_handle());
        }
    }

    async fn timer(self: Rc<Self>, t: Obj) {
        let id = s(&t, "id").to_string();
        let (lo, hi) = (n(&t, "interval").max(1.0), n(&t, "upperInterval"));
        let lines = n(&t, "lines") as u64;
        let mut seen = self.st.borrow().chat_lines;
        loop {
            let secs = if b(&t, "randomInterval", false) && hi > lo {
                lo + crate::math::random() * (hi - lo)
            } else {
                lo
            };
            tokio::time::sleep(Duration::from_secs_f64(secs)).await;
            let now = self.st.borrow().chat_lines;
            if lines > 0 && now - seen < lines {
                continue;
            }
            seen = now;
            let mut counter = 0;
            self.change(|c| {
                if let Some(x) = c.timers.iter_mut().find(|x| s(x, "id") == id) {
                    counter = x.get("counter").and_then(Value::as_i64).unwrap_or(0) + 1;
                    x.insert("counter".into(), counter.into());
                }
            });
            let mut args = Obj::new();
            args.insert("timerId".into(), id.as_str().into());
            args.insert("timerName".into(), s(&t, "name").into());
            args.insert("counter".into(), counter.into());
            self.fire(T_TIMER, "Timed Actions", "Core", "misc", &args, |x| {
                s(x, "timerId") == id
            });
            self.emit("Misc", "TimedAction", || Value::Object(args));
            if !b(&t, "repeat", true) {
                self.change(|c| {
                    if let Some(x) = c.timers.iter_mut().find(|x| s(x, "id") == id) {
                        x.insert("enabled".into(), false.into());
                    }
                });
                return;
            }
        }
    }

    /// Fire a group trigger, with group.* arguments (and group.user#.* for added/removed users).
    fn group_event(self: &Rc<Self>, kind: u32, name: &str, g: &Obj, users: &[Obj]) {
        let id = s(g, "id").to_string();
        let mut args = Obj::new();
        args.insert("group.id".into(), id.as_str().into());
        args.insert("group.name".into(), s(g, "name").into());
        args.insert("group.bots".into(), b(g, "bots", false).into());
        if kind == T_GROUP_USERS_ADDED || kind == T_GROUP_USERS_REMOVED {
            args.insert("group.user.count".into(), users.len().into());
            for (i, u) in users.iter().enumerate() {
                for k in ["userId", "user", "userName", "userType", "isSubscribed"] {
                    args.insert(format!("group.user{i}.{k}"), u.get(k).cloned().unwrap_or(Value::Null));
                }
            }
        }
        self.fire(kind, name, "Core/Groups", "streamerbot", &args, |t| {
            let want = s(t, "groupId");
            want.is_empty() || want == id
        });
        self.emit("Group", &name.replace(' ', ""), || {
            let mut d = serde_json::json!({"groupId": id, "groupName": s(g, "name"), "groupBots": b(g, "bots", false)});
            if !users.is_empty() {
                d["users"] = users.iter().cloned().map(Value::Object).collect();
            }
            d
        });
    }

    /// Create a user group (the editor and the platforms' Add User to Group use these).
    pub fn add_group(self: &Rc<Self>, name: &str, bots: bool) -> String {
        let mut g = Obj::new();
        let id = super::uuid();
        g.insert("id".into(), id.as_str().into());
        g.insert("name".into(), name.into());
        g.insert("bots".into(), bots.into());
        g.insert("userIds".into(), Value::Array(vec![]));
        self.change(|c| c.groups.push(g.clone()));
        self.group_event(T_GROUP_ADDED, "Added", &g, &[]);
        id
    }

    pub fn remove_group(self: &Rc<Self>, id: &str) {
        let Some(g) = self.cfg.borrow().groups.iter().find(|g| s(g, "id") == id).cloned() else {
            return;
        };
        self.change(|c| c.groups.retain(|g| s(g, "id") != id));
        self.group_event(T_GROUP_REMOVED, "Removed", &g, &[]);
    }

    /// Add (or remove) users to a group; each user is {userId, user, userName, userType,
    /// isSubscribed}. Only users that weren't (or were) in it count.
    #[allow(dead_code)] // ponytail: called by the platforms' user group sub-actions (phase 6+).
    pub fn group_users(self: &Rc<Self>, id: &str, users: &[Obj], add: bool) {
        let mut changed = Vec::new();
        let mut group = None;
        self.change(|c| {
            let Some(g) = c.groups.iter_mut().find(|g| s(g, "id") == id) else {
                return;
            };
            let ids = g.entry("userIds").or_insert_with(|| Value::Array(vec![]));
            if let Some(ids) = ids.as_array_mut() {
                for u in users {
                    let uid = u.get("userId").map(text).unwrap_or_default();
                    let has = ids.iter().any(|x| text(x) == uid);
                    if add && !has {
                        ids.push(uid.into());
                        changed.push(u.clone());
                    } else if !add && has {
                        ids.retain(|x| text(x) != uid);
                        changed.push(u.clone());
                    }
                }
            }
            group = Some(g.clone());
        });
        if let (Some(g), false) = (group, changed.is_empty()) {
            let (kind, name) = if add {
                (T_GROUP_USERS_ADDED, "Users Added")
            } else {
                (T_GROUP_USERS_REMOVED, "Users Removed")
            };
            self.group_event(kind, name, &g, &changed);
        }
    }

    /// Set a command's or a group of commands' state (Enabled / Disabled / Toggle).
    fn set_commands(self: &Rc<Self>, pick: impl Fn(&Obj) -> bool, state: f64) {
        self.change(|c| {
            for x in c.commands.iter_mut().filter(|x| pick(x)) {
                let on = x.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                x.insert("enabled".into(), new_state(state, on).into());
            }
        });
    }

    pub(super) async fn rest(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) {
        match kind {
            // UDP Broadcast
            2 => {
                let payload = self.expand(ctx, s(o, "payload"));
                let port = n(o, "port") as u16;
                let sent = async {
                    let sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
                    sock.set_broadcast(true)?;
                    sock.send_to(payload.as_bytes(), ("255.255.255.255", port)).await
                };
                if let Err(e) = sent.await {
                    crate::say(format!("UDP Broadcast to port {port}: {e}"));
                }
            }
            6 => self.run_program(ctx, o).await,
            // Set Timer State
            8 => {
                let id = self.simple(ctx, s(o, "timerId"));
                let st = n(o, "state");
                self.change(|c| {
                    if let Some(t) = c.timers.iter_mut().find(|t| s(t, "id") == id) {
                        let on = b(t, "enabled", false);
                        t.insert("enabled".into(), new_state(st, on).into());
                    }
                });
                self.timers();
            }
            // Set Command State / Set Command Group State
            9 => {
                let id = self.simple(ctx, s(o, "commandId"));
                self.set_commands(|c| s(c, "id") == id, n(o, "state"));
            }
            // Send Message to Channel (Twitch)
            10 => {
                let text = self.expand(ctx, s(o, "text"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                if self.dev_out("twitch", &text, use_bot) {
                } else if let Some(tw) = self.twitch() {
                    if let Err(e) = tw.send_message(text, None, use_bot, fallback).await {
                        crate::say(e);
                    }
                }
            }
            // Send Announcement to Channel (Twitch)
            23 => {
                let text = self.expand(ctx, s(o, "text"));
                let color = self.simple(ctx, s(o, "color"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw.send_announcement(text, color, use_bot, fallback).await {
                        crate::say(e);
                    }
                }
            }
            // Start Raid (Twitch)
            500 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    let ok = tw.start_raid(login).await.unwrap_or(false);
                    ctx.args.insert("startRaidSuccess".into(), ok.into());
                }
            }
            // Cancel Raid (Twitch)
            501 => {
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw.cancel_raid().await {
                        crate::say(e);
                    }
                }
            }
            // Reply To Message (Twitch)
            560 => {
                let text = self.expand(ctx, s(o, "text"));
                let reply_id = self.expand(ctx, s(o, "replyId"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                if self.dev_out("twitch", &text, use_bot) {
                } else if let Some(tw) = self.twitch() {
                    if let Err(e) = tw.send_message(text, Some(reply_id), use_bot, fallback).await {
                        crate::say(e);
                    }
                }
            }
            // Update Redemption Status (11)
            11 => {
                let status = s(o, "status");
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                let reward_id = if reward_id.is_empty() {
                    ctx.args
                        .get("rewardId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                } else {
                    reward_id
                };
                let red_id = self.expand(ctx, s(o, "redemptionId"));
                let red_id = if red_id.is_empty() {
                    ctx.args
                        .get("redemptionId")
                        .or_else(|| ctx.args.get("id"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                } else {
                    red_id
                };
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards/redemptions",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("reward_id", &reward_id),
                                ("id", &red_id),
                            ],
                            Some(json!({"status": status})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Set Channel Title (15)
            15 => {
                ctx.args.insert("titleSuccess".into(), false.into());
                let title = self.expand(ctx, s(o, "title"));
                if let Some(tw) = self.twitch() {
                    match tw
                        .helix(
                            "PATCH",
                            "channels",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            Some(json!({"title": title})),
                            false,
                            true,
                        )
                        .await
                    {
                        Ok(_) => {
                            ctx.args.insert("titleSuccess".into(), true.into());
                        }
                        Err(e) => {
                            crate::say(format!("Twitch: {e}"));
                        }
                    }
                }
            }
            // Set Channel Game (16)
            16 => {
                ctx.args.insert("gameSuccess".into(), false.into());
                let Some(tw) = self.twitch() else { return };
                // Source 0 (String): a game name; 1 (Specific Game): the id picked.
                let (by_id, want) = if n(o, "source") as i64 == 1 && !s(o, "gameId").is_empty() {
                    (true, self.expand(ctx, s(o, "gameId")))
                } else {
                    (false, self.expand(ctx, s(o, "game")))
                };
                let r: Result<(), String> = async {
                    let q = vec![(if by_id { "id" } else { "name" }, want.as_str())];
                    let v = tw.helix("GET", "games", q, None, false, true).await?;
                    let g = v["data"][0].clone();
                    let id = g["id"].as_str().ok_or_else(|| format!("no game '{want}'"))?.to_string();
                    ctx.args.insert("gameId".into(), id.as_str().into());
                    ctx.args.insert("gameName".into(), g["name"].clone());
                    let art = g["box_art_url"]
                        .as_str()
                        .unwrap_or("")
                        .replace("{width}x{height}", "285x380");
                    ctx.args.insert("gameBoxArt".into(), art.into());
                    let who = vec![("broadcaster_id", "{broadcaster_id}")];
                    tw.helix("PATCH", "channels", who, Some(json!({"game_id": id})), false, true)
                        .await?;
                    Ok(())
                }
                .await;
                match r {
                    Ok(()) => drop(ctx.args.insert("gameSuccess".into(), true.into())),
                    Err(e) => crate::say(format!("Twitch: {e}")),
                }
            }

            // Create Stream Marker (17)
            17 => {
                ctx.args.insert("streamMarkerSuccess".into(), false.into());
                let desc = self.expand(ctx, s(o, "description"));
                let Some(tw) = self.twitch() else { return };
                let body = json!({"user_id": "{broadcaster_id}", "description": desc});
                match tw
                    .helix("POST", "streams/markers", vec![], Some(body), false, true)
                    .await
                {
                    Ok(v) => {
                        let m = &v["data"][0];
                        ctx.args.insert("streamMarkerSuccess".into(), true.into());
                        for (k, f) in [
                            ("Id", "id"),
                            ("CreatedAt", "created_at"),
                            ("Description", "description"),
                            ("Position", "position_seconds"),
                        ] {
                            ctx.args.insert(format!("streamMarker{k}"), m[f].clone());
                        }
                    }
                    Err(e) => crate::say(format!("Twitch: {e}")),
                }
            }

            // Create Clip (18)
            18 => {
                let delay = b(o, "hasDelay", false);
                if let Some(tw) = self.twitch() {
                    let res = tw
                        .helix(
                            "POST",
                            "clips",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("has_delay", if delay { "true" } else { "false" }),
                            ],
                            None,
                            false,
                            true,
                        )
                        .await;
                    ctx.args.insert("createClipSuccess".into(), res.is_ok().into());
                    match res {
                        Ok(val) => {
                            if let Some(id) = val.pointer("/data/0/id").and_then(Value::as_str) {
                                ctx.args.insert("clipId".into(), id.into());
                                ctx.args.insert("createClipId".into(), id.into());
                                ctx.args
                                    .insert("createClipUrl".into(), format!("https://clips.twitch.tv/{id}").into());
                                ctx.args
                                    .insert("createClipCreatedAt".into(), super::globals::iso_now().into());
                            }
                        }
                        Err(e) => crate::say(format!("Twitch: {e}")),
                    }
                }
            }
            // Run Commercial (19)
            19 => {
                let duration = n(o, "duration") as u64;
                let len = if duration == 0 { 30 } else { duration };
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "POST",
                            "channels/commercial",
                            vec![],
                            Some(json!({"broadcaster_id": "{broadcaster_id}", "length": len})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Add Viewer Count (24)
            24 => {
                if let Some(tw) = self.twitch() {
                    if let Ok(val) = tw
                        .helix(
                            "GET",
                            "streams",
                            vec![("user_id", "{broadcaster_id}")],
                            None,
                            false,
                            true,
                        )
                        .await
                    {
                        let count = val.pointer("/data/0/viewer_count").and_then(Value::as_u64).unwrap_or(0);
                        ctx.args.insert("viewerCount".into(), count.into());
                    }
                }
            }
            // Set Enabled State (200)
            200 => {
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                let state = n(o, "state");
                let is_enabled = state == 0.0;
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            Some(json!({"is_enabled": is_enabled})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Set Paused State (201)
            201 => {
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                let state = n(o, "state");
                let is_paused = state == 1.0;
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            Some(json!({"is_paused": is_paused})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Set Cost (202)
            202 => {
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                let cost = n(o, "cost") as u64;
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            Some(json!({"cost": cost})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Set Title (205)
            205 => {
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                let title = self.expand(ctx, s(o, "title"));
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            Some(json!({"title": title})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Set Prompt (206)
            206 => {
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                let prompt = self.expand(ctx, s(o, "prompt"));
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            Some(json!({"prompt": prompt})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Get Reward Info (208)
            208 => {
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                if let Some(tw) = self.twitch() {
                    if let Ok(val) = tw
                        .helix(
                            "GET",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            None,
                            false,
                            true,
                        )
                        .await
                    {
                        if let Some(r) = val.pointer("/data/0") {
                            let max = |k: &str| {
                                r[k][if k == "global_cooldown_setting" {
                                    "global_cooldown_seconds"
                                } else {
                                    "max_per_stream"
                                }]
                                .clone()
                            };
                            for (k, x) in [
                                ("rewardId", r["id"].clone()),
                                ("rewardTitle", r["title"].clone()),
                                ("rewardCost", r["cost"].clone()),
                                ("rewardPrompt", r["prompt"].clone()),
                                ("rewardBackgroundColor", r["background_color"].clone()),
                                ("rewardEnabled", r["is_enabled"].clone()),
                                ("rewardPaused", r["is_paused"].clone()),
                                (
                                    "rewardSkipRequestQueue",
                                    r["should_redemptions_skip_request_queue"].clone(),
                                ),
                                ("rewardGlobalCooldown", max("global_cooldown_setting")),
                                ("rewardMaxPerStream", max("max_per_stream_setting")),
                                (
                                    "rewardMaxPerUserPerStream",
                                    r["max_per_user_per_stream_setting"]["max_per_user_per_stream"].clone(),
                                ),
                            ] {
                                ctx.args.insert(k.into(), x);
                            }
                        }
                    }
                }
            }
            // Clear Channel Tags (251)
            251 => {
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channels",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            Some(json!({"tags": []})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Set Channel Tags (252)
            252 => {
                let tags = self.expand(ctx, s(o, "tags"));
                let tag_list: Vec<&str> = tags.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channels",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            Some(json!({"tags": tag_list})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Add Channel Tag (253) / Remove Channel Tag (254)
            253 | 254 => {
                let tag = self.expand(ctx, s(o, "tag"));
                let Some(tw) = self.twitch() else { return };
                let who = vec![("broadcaster_id", "{broadcaster_id}")];
                let r = async {
                    let cur = tw.helix("GET", "channels", who.clone(), None, false, true).await?;
                    let mut tags: Vec<String> = cur
                        .pointer("/data/0/tags")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|t| t.as_str().map(String::from))
                        .filter(|t| !t.eq_ignore_ascii_case(&tag))
                        .collect();
                    if kind == 253 {
                        tags.push(tag.clone());
                    }
                    tw.helix("PATCH", "channels", who, Some(json!({"tags": tags})), false, true)
                        .await
                }
                .await;
                if let Err(e) = r {
                    crate::say(format!("Twitch: {e}"));
                }
            }
            // Snooze Ad (555)
            555 => {
                ctx.args.insert("snooze.success".into(), false.into());
                if let Some(tw) = self.twitch() {
                    match tw
                        .helix(
                            "POST",
                            "channels/ads/schedule/snooze",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            None,
                            false,
                            true,
                        )
                        .await
                    {
                        Ok(_) => {
                            ctx.args.insert("snooze.success".into(), true.into());
                        }
                        Err(e) => {
                            crate::say(format!("Twitch: {e}"));
                        }
                    }
                }
            }
            // Get Ad Schedule (556) / Get Pre-roll Free Time (557)
            556 | 557 => {
                let Some(tw) = self.twitch() else { return };
                match tw
                    .helix(
                        "GET",
                        "channels/ads/schedule",
                        vec![("broadcaster_id", "{broadcaster_id}")],
                        None,
                        false,
                        true,
                    )
                    .await
                {
                    Ok(v) => {
                        let d = &v["data"][0];
                        let free = d["preroll_free_time"].as_i64().unwrap_or(0);
                        ctx.args.insert("prerollFree".into(), (free > 0).into());
                        ctx.args.insert("prerollFreeSeconds".into(), free.into());
                        if kind == 556 {
                            ctx.args.insert("snoozeCount".into(), d["snooze_count"].clone());
                            ctx.args.insert("adScheduleNextAdAt".into(), d["next_ad_at"].clone());
                        }
                    }
                    Err(e) => crate::say(format!("Twitch: {e}")),
                }
            }

            // Create Poll (561)
            561 => {
                ctx.args.insert("success".into(), false.into());
                let title = self.expand(ctx, s(o, "title"));
                let choices: Vec<Value> = list(o.get("choices"))
                    .iter()
                    .map(|c| json!({"title": self.expand(ctx, c)}))
                    .collect();
                let mut body = json!({"broadcaster_id": "{broadcaster_id}", "title": title, "choices": choices,
                    "duration": (n(o, "duration") as u64).clamp(15, 1800)});
                let points = n(o, "channelPointsPerVote") as u64;
                if points > 0 {
                    body["channel_points_voting_enabled"] = true.into();
                    body["channel_points_per_vote"] = points.into();
                }
                let Some(tw) = self.twitch() else { return };
                match tw.helix("POST", "polls", vec![], Some(body), false, true).await {
                    Ok(v) => {
                        ctx.args.insert("success".into(), true.into());
                        ctx.args.insert("pollId".into(), v["data"][0]["id"].clone());
                    }
                    Err(e) => crate::say(format!("Twitch: {e}")),
                }
            }

            // Terminate Active Poll (562)
            562 => {
                // The active poll (the newest one, still ACTIVE).
                let Some(tw) = self.twitch() else { return };
                let r = async {
                    let v = tw
                        .helix(
                            "GET",
                            "polls",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            None,
                            false,
                            true,
                        )
                        .await?;
                    let p = v["data"]
                        .as_array()
                        .and_then(|a| a.iter().find(|p| p["status"] == "ACTIVE"))
                        .cloned();
                    let p = p.ok_or("no active poll")?;
                    let body = json!({"broadcaster_id": "{broadcaster_id}", "id": p["id"], "status": "TERMINATED"});
                    tw.helix("PATCH", "polls", vec![], Some(body), false, true).await
                }
                .await;
                if let Err(e) = r {
                    crate::say(format!("Twitch: {e}"));
                }
            }

            // Archive Active Poll (563)
            563 => {
                // The active poll (the newest one, still ACTIVE).
                let Some(tw) = self.twitch() else { return };
                let r = async {
                    let v = tw
                        .helix(
                            "GET",
                            "polls",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            None,
                            false,
                            true,
                        )
                        .await?;
                    let p = v["data"]
                        .as_array()
                        .and_then(|a| a.iter().find(|p| p["status"] == "ACTIVE"))
                        .cloned();
                    let p = p.ok_or("no active poll")?;
                    let body = json!({"broadcaster_id": "{broadcaster_id}", "id": p["id"], "status": "ARCHIVED"});
                    tw.helix("PATCH", "polls", vec![], Some(body), false, true).await
                }
                .await;
                if let Err(e) = r {
                    crate::say(format!("Twitch: {e}"));
                }
            }

            // Set Channel Branded Content (564)
            564 => {
                ctx.args.insert("success".into(), false.into());
                let status = b(o, "status", false);
                if let Some(tw) = self.twitch() {
                    match tw
                        .helix(
                            "PATCH",
                            "channels",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            Some(json!({"is_branded_content": status})),
                            false,
                            true,
                        )
                        .await
                    {
                        Ok(_) => {
                            ctx.args.insert("success".into(), true.into());
                        }
                        Err(e) => {
                            crate::say(format!("Twitch: {e}"));
                        }
                    }
                }
            }
            // Get Active Poll (566)
            566 => {
                ctx.args.insert("active".into(), false.into());
                let Some(tw) = self.twitch() else { return };
                if let Ok(v) = tw
                    .helix(
                        "GET",
                        "polls",
                        vec![("broadcaster_id", "{broadcaster_id}")],
                        None,
                        false,
                        true,
                    )
                    .await
                {
                    if let Some(p) = v["data"]
                        .as_array()
                        .and_then(|a| a.iter().find(|p| p["status"] == "ACTIVE"))
                    {
                        poll_args(p, &mut ctx.args);
                    }
                }
            }

            // Get Active Prediction (567)
            567 => {
                ctx.args.insert("active".into(), false.into());
                let Some(tw) = self.twitch() else { return };
                if let Ok(v) = tw
                    .helix(
                        "GET",
                        "predictions",
                        vec![("broadcaster_id", "{broadcaster_id}")],
                        None,
                        false,
                        true,
                    )
                    .await
                {
                    if let Some(p) = v["data"]
                        .as_array()
                        .and_then(|a| a.iter().find(|p| p["status"] == "ACTIVE"))
                    {
                        prediction_args(p, &mut ctx.args);
                    }
                }
            }

            // Create Prediction (568)
            568 => {
                ctx.args.insert("success".into(), false.into());
                let title = self.expand(ctx, s(o, "title"));
                let outcomes: Vec<Value> = list(o.get("outcomes"))
                    .iter()
                    .map(|c| json!({"title": self.expand(ctx, c)}))
                    .collect();
                let window = (n(o, "predictionWindow") as u64).clamp(30, 1800);
                let body = json!({"broadcaster_id": "{broadcaster_id}", "title": title, "outcomes": outcomes,
                    "prediction_window": window});
                let Some(tw) = self.twitch() else { return };
                match tw.helix("POST", "predictions", vec![], Some(body), false, true).await {
                    Ok(v) => {
                        ctx.args.insert("success".into(), true.into());
                        ctx.args.insert("prediction.Id".into(), v["data"][0]["id"].clone());
                    }
                    Err(e) => crate::say(format!("Twitch: {e}")),
                }
            }

            // Cancel (569) / Lock (570) the active prediction; Resolve the last one by outcome
            // index (571) or title (572). Each adds the prediction's variables.
            569..=572 => {
                ctx.args.insert("active".into(), false.into());
                if kind >= 571 {
                    ctx.args.insert("success".into(), false.into());
                }
                let Some(tw) = self.twitch() else { return };
                let pick = match self.expand(ctx, s(o, if kind == 572 { "outcomeTitle" } else { "winningIndex" })) {
                    p if p.is_empty() => "0".to_string(),
                    p => p,
                };
                let r: Result<(), String> = async {
                    let v = tw
                        .helix(
                            "GET",
                            "predictions",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            None,
                            false,
                            true,
                        )
                        .await?;
                    // Cancel and lock act on an ACTIVE one; resolve also on a LOCKED one.
                    let p = v["data"]
                        .as_array()
                        .and_then(|a| {
                            a.iter()
                                .find(|p| p["status"] == "ACTIVE" || (kind != 570 && p["status"] == "LOCKED"))
                        })
                        .cloned()
                        .ok_or("no active prediction")?;
                    prediction_args(&p, &mut ctx.args);
                    let outcomes = p["outcomes"].as_array().cloned().unwrap_or_default();
                    let mut body = json!({"broadcaster_id": "{broadcaster_id}", "id": p["id"]});
                    body["status"] = match kind {
                        569 => "CANCELED",
                        570 => "LOCKED",
                        _ => "RESOLVED",
                    }
                    .into();
                    if kind >= 571 {
                        let win = if kind == 572 {
                            outcomes
                                .iter()
                                .position(|x| x["title"].as_str().is_some_and(|t| t.eq_ignore_ascii_case(&pick)))
                        } else {
                            pick.trim().parse::<usize>().ok().filter(|i| *i < outcomes.len())
                        }
                        .ok_or_else(|| format!("no outcome '{pick}'"))?;
                        body["winning_outcome_id"] = outcomes[win]["id"].clone();
                        ctx.args.insert("prediction.winningIndex".into(), win.into());
                        for (k, f) in [
                            ("id", "id"),
                            ("title", "title"),
                            ("color", "color"),
                            ("users", "users"),
                            ("points", "channel_points"),
                        ] {
                            ctx.args
                                .insert(format!("prediction.winningOutcome.{k}"), outcomes[win][f].clone());
                        }
                    }
                    tw.helix("PATCH", "predictions", vec![], Some(body), false, true)
                        .await?;
                    match kind {
                        569 => drop(ctx.args.insert("cancelled".into(), true.into())),
                        570 => drop(ctx.args.insert("locked".into(), true.into())),
                        _ => drop(ctx.args.insert("success".into(), true.into())),
                    }
                    Ok(())
                }
                .await;
                if let Err(e) = r {
                    crate::say(format!("Twitch prediction: {e}"));
                }
            }

            // Get Clip Download Urls (573)
            573 => {
                let clip_id = self.expand(ctx, s(o, "clipId"));
                if let Some(tw) = self.twitch() {
                    if let Ok(val) = tw
                        .helix("GET", "clips", vec![("id", &clip_id)], None, false, true)
                        .await
                    {
                        if let Some(c) = val.pointer("/data/0") {
                            ctx.args.insert("clipUrl".into(), c["url"].clone());
                            ctx.args.insert("clipThumbnailUrl".into(), c["thumbnail_url"].clone());
                        }
                    }
                }
            }
            // Subscriber Only (12)
            12 => {
                let status = b(o, "status", true);
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "chat/settings",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{broadcaster_id}"),
                            ],
                            Some(json!({"subscriber_mode": status})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Emote Only (13)
            13 => {
                let status = b(o, "status", true);
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "chat/settings",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{broadcaster_id}"),
                            ],
                            Some(json!({"emote_mode": status})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Slow Mode (14)
            14 => {
                let status = b(o, "status", true);
                let dur = n(o, "duration") as u64;
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "chat/settings",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{broadcaster_id}"),
                            ],
                            Some(json!({"slow_mode": status, "slow_mode_wait_time": dur})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Follower Mode (28)
            28 => {
                let status = b(o, "status", true);
                let dur = n(o, "duration") as u64;
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "chat/settings",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{broadcaster_id}"),
                            ],
                            Some(json!({"follower_mode": status, "follower_mode_duration": dur})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Timeout User (20)
            20 => {
                ctx.args.insert("timeoutResult".into(), false.into());
                let login = self.expand(ctx, s(o, "userLogin"));
                let dur = (n(o, "duration") as u64).max(1);
                let reason = self.expand(ctx, s(o, "reason"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        ctx.args.insert("timedOutUser".into(), login.clone().into());
                        ctx.args.insert("timedOutUserName".into(), login.clone().into());
                        ctx.args.insert("timedOutUserType".into(), "twitch".into());
                        ctx.args.insert("timedOutUserId".into(), uid.clone().into());
                        match tw
                            .helix(
                                "POST",
                                "moderation/bans",
                                vec![
                                    ("broadcaster_id", "{broadcaster_id}"),
                                    ("moderator_id", "{broadcaster_id}"),
                                ],
                                Some(json!({"data": {"user_id": uid, "duration": dur, "reason": reason}})),
                                false,
                                true,
                            )
                            .await
                        {
                            Ok(_) => {
                                ctx.args.insert("timeoutResult".into(), true.into());
                            }
                            Err(e) => {
                                crate::say(format!("Twitch: {e}"));
                            }
                        }
                    }
                }
            }
            // Add Broadcaster Information (21)
            21 => {
                let Some(tw) = self.twitch() else { return };
                let id = self.cfg.borrow().twitch.broadcaster.as_ref().map(|b| b.user_id.clone());
                let Some(id) = id else { return };
                if let Ok(v) = tw.helix("GET", "users", vec![("id", &id)], None, false, true).await {
                    let u = &v["data"][0];
                    let f = |k: &str| u[k].clone();
                    let kind = u["broadcaster_type"].as_str().unwrap_or("");
                    for (k, x) in [
                        ("broadcastUser", f("display_name")),
                        ("broadcastUserName", f("login")),
                        ("broadcastUserId", f("id")),
                        ("broadcastUserType", "twitch".into()),
                        ("broadcastIsAffiliate", (kind == "affiliate").into()),
                        ("broadcastIsPartner", (kind == "partner").into()),
                        ("broadcasterProfileUrl", f("profile_image_url")),
                        ("broadcasterOfflineUrl", f("offline_image_url")),
                        ("broadcasterCreatedAt", f("created_at")),
                        ("broadcasterDescription", f("description")),
                    ] {
                        ctx.args.insert(k.into(), x);
                    }
                }
            }

            // Add Random Users (22)
            22 => {
                // The users in chat now (Present Only, or not: Streamerz only knows who is present).
                let count = (n(o, "count") as usize).max(1);
                let Some(tw) = self.twitch() else { return };
                let who = vec![
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("moderator_id", "{broadcaster_id}"),
                    ("first", "1000"),
                ];
                let mut users: Vec<Value> = match tw.helix("GET", "chat/chatters", who, None, false, true).await {
                    Ok(v) => v["data"].as_array().cloned().unwrap_or_default(),
                    Err(e) => return crate::say(format!("Twitch: {e}")),
                };
                for i in 0..count.min(users.len()) {
                    let pick = i + (crate::math::secure_u64() as usize) % (users.len() - i);
                    users.swap(i, pick);
                    let u = &users[i];
                    ctx.args.insert(format!("randomUser{i}"), u["user_name"].clone());
                    ctx.args.insert(format!("randomUserName{i}"), u["user_login"].clone());
                    ctx.args.insert(format!("randomUserId{i}"), u["user_id"].clone());
                    ctx.args.insert(format!("randomUserType{i}"), "twitch".into());
                }
                ctx.args
                    .insert("randomUsersAdded".into(), count.min(users.len()).into());
            }

            // Get Latest Charity Campaign (25)
            25 => {
                let Some(tw) = self.twitch() else { return };
                match tw
                    .helix(
                        "GET",
                        "charity/campaigns",
                        vec![("broadcaster_id", "{broadcaster_id}")],
                        None,
                        false,
                        true,
                    )
                    .await
                {
                    Ok(v) => {
                        let c = &v["data"][0];
                        let f = |k: &str| c[k].clone();
                        for (k, x) in [
                            ("campaignId", f("id")),
                            ("charity.name", f("charity_name")),
                            ("charity.description", f("charity_description")),
                            ("charity.logoUrl", f("charity_logo")),
                            ("charity.websiteUrl", f("charity_website")),
                            ("currentAmount", c["current_amount"]["value"].clone()),
                            ("targetAmount", c["target_amount"]["value"].clone()),
                        ] {
                            ctx.args.insert(k.into(), x);
                        }
                    }
                    Err(e) => crate::say(format!("Twitch: {e}")),
                }
            }

            // Get Shield Mode Status (26)
            26 => {
                if let Some(tw) = self.twitch() {
                    if let Ok(resp) = tw
                        .helix(
                            "GET",
                            "moderation/shield_mode",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{broadcaster_id}"),
                            ],
                            None,
                            false,
                            true,
                        )
                        .await
                    {
                        let active = resp
                            .pointer("/data/0/is_active")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        ctx.args.insert("shieldMode".into(), active.into());
                    }
                }
            }
            // Update Shield Mode Status (27)
            27 => {
                let status = b(o, "status", true);
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PUT",
                            "moderation/shield_mode",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{broadcaster_id}"),
                            ],
                            Some(json!({"is_active": status})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Send Shoutout (29)
            29 => {
                ctx.args.insert("shoutoutSuccess".into(), false.into());
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        match tw
                            .helix(
                                "POST",
                                "chat/shoutouts",
                                vec![
                                    ("from_broadcaster_id", "{broadcaster_id}"),
                                    ("to_broadcaster_id", &uid),
                                    ("moderator_id", "{broadcaster_id}"),
                                ],
                                None,
                                false,
                                true,
                            )
                            .await
                        {
                            Ok(_) => {
                                ctx.args.insert("shoutoutSuccess".into(), true.into());
                            }
                            Err(e) => {
                                crate::say(format!("Twitch: {e}"));
                            }
                        }
                    }
                }
            }
            // Get User Info for Target (50)
            50 => {
                ctx.args.insert("addTargetResult".into(), false.into());
                let login = self.expand(ctx, s(o, "userLogin")).trim_start_matches('@').to_string();
                let Some(tw) = self.twitch() else { return };
                let Ok(v) = tw
                    .helix("GET", "users", vec![("login", &login)], None, false, true)
                    .await
                else {
                    return;
                };
                let u = v["data"][0].clone();
                let Some(uid) = u["id"].as_str().map(String::from) else {
                    return;
                };
                let desc = u["description"].as_str().unwrap_or("").to_string();
                let kind = u["broadcaster_type"].as_str().unwrap_or("");
                let mut put = |k: &str, x: Value| drop(ctx.args.insert(k.into(), x));
                put("addTargetResult", true.into());
                put("targetUser", u["display_name"].clone());
                put("targetUserName", u["login"].clone());
                put("targetUserId", uid.as_str().into());
                put("targetUserType", "twitch".into());
                put("targetDescription", desc.as_str().into());
                put("targetDescriptionEscaped", crate::commands::escape(&desc).into());
                put("targetUserProfileImageUrl", u["profile_image_url"].clone());
                put(
                    "targetUserProfileImageUrlEscaped",
                    crate::commands::url_encode(u["profile_image_url"].as_str().unwrap_or("")).into(),
                );
                put("targetIsAffiliate", (kind == "affiliate").into());
                put("targetIsPartner", (kind == "partner").into());
                put("createdAt", u["created_at"].clone());
                if let Some(t) = u["created_at"].as_str().and_then(parse_iso) {
                    let now = crate::fmt::unix_now().as_secs() as i64;
                    put("accountAge", ((now - t) / 86_400).into());
                }
                let me = vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", uid.as_str())];
                let has = |r: Result<Value, String>| {
                    r.ok()
                        .is_some_and(|v| v["data"].as_array().is_some_and(|a| !a.is_empty()))
                };
                let sub = tw.helix("GET", "subscriptions", me.clone(), None, false, true).await;
                let tier = sub
                    .as_ref()
                    .ok()
                    .and_then(|v| v["data"][0]["tier"].as_str().map(String::from));
                put("targetIsSubscribed", tier.is_some().into());
                put("targetSubscriptionTier", tier.unwrap_or_default().into());
                put(
                    "targetIsModerator",
                    has(tw
                        .helix("GET", "moderation/moderators", me.clone(), None, false, true)
                        .await)
                    .into(),
                );
                put(
                    "targetIsVip",
                    has(tw.helix("GET", "channels/vips", me, None, false, true).await).into(),
                );
                if let Ok(ch) = tw
                    .helix(
                        "GET",
                        "channels",
                        vec![("broadcaster_id", uid.as_str())],
                        None,
                        false,
                        true,
                    )
                    .await
                {
                    let c = &ch["data"][0];
                    put("targetChannelTitle", c["title"].clone());
                    put("game", c["game_name"].clone());
                    put("gameId", c["game_id"].clone());
                    let tags: Vec<String> = c["tags"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect();
                    put("tagCount", tags.len().into());
                    for (i, t) in tags.iter().enumerate() {
                        put(&format!("tag{i}"), t.as_str().into());
                    }
                    put("tagsDelimited", tags.join(", ").into());
                    put("tags", tags.into());
                }
            }

            // Get Follow Age Info for Target (51)
            51 => {
                ctx.args.insert("isFollowing".into(), false.into());
                let login = self.expand(ctx, s(o, "userLogin")).trim_start_matches('@').to_string();
                let Some(tw) = self.twitch() else { return };
                let Ok(uid) = tw.resolve_user_id(&login).await else {
                    return;
                };
                let q = vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", uid.as_str())];
                let v = match tw.helix("GET", "channels/followers", q, None, false, true).await {
                    Ok(v) => v,
                    Err(e) => return crate::say(format!("Twitch: {e}")),
                };
                let f = &v["data"][0];
                ctx.args.insert("followUser".into(), f["user_name"].clone());
                ctx.args.insert("followUserName".into(), f["user_login"].clone());
                ctx.args.insert("followUserId".into(), uid.as_str().into());
                let Some(t) = f["followed_at"].as_str().and_then(parse_iso) else {
                    return;
                };
                let now = crate::fmt::unix_now().as_secs() as i64;
                let a = age(t, now);
                let (long, short) = age_text(&a);
                ctx.args.insert("isFollowing".into(), true.into());
                ctx.args.insert(
                    "followDate".into(),
                    crate::fmt::date(&civil(t), "dddd, d MMMM yyyy").into(),
                );
                ctx.args.insert("followAgeLong".into(), long.into());
                ctx.args.insert("followAgeShort".into(), short.into());
                ctx.args.insert("followAgeDays".into(), ((now - t) / 86_400).into());
                ctx.args.insert("followAgeMinutes".into(), ((now - t) / 60).into());
                ctx.args.insert("followAgeSeconds".into(), (now - t).into());
                for (k, x) in ["years", "months", "days", "hours", "minutes", "seconds"].iter().zip(a) {
                    ctx.args.insert(format!("followAge.{k}"), x.into());
                }
            }

            // Get Team Info for Target (52)
            52 => {
                let login = self.expand(ctx, s(o, "userLogin")).trim_start_matches('@').to_string();
                let Some(tw) = self.twitch() else { return };
                let Ok(uid) = tw.resolve_user_id(&login).await else {
                    return;
                };
                match tw
                    .helix(
                        "GET",
                        "teams/channel",
                        vec![("broadcaster_id", uid.as_str())],
                        None,
                        false,
                        true,
                    )
                    .await
                {
                    Ok(v) => {
                        ctx.args.insert("_jsonTeamData".into(), v["data"].to_string().into());
                        for (i, t) in v["data"].as_array().into_iter().flatten().enumerate() {
                            for (k, f) in [
                                ("teamId", "id"),
                                ("teamName", "team_name"),
                                ("teamDisplayName", "team_display_name"),
                                ("teamBackgroundUrl", "background_image_url"),
                                ("teamBannerUrl", "banner"),
                            ] {
                                ctx.args.insert(format!("{k}{i}"), t[f].clone());
                            }
                        }
                    }
                    Err(e) => crate::say(format!("Twitch: {e}")),
                }
            }

            // Set Global Cooldown (204)
            204 => {
                let rew_id = self.expand(ctx, s(o, "rewardId"));
                let cd = n(o, "cooldown") as u64;
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &rew_id)],
                            Some(json!({"is_global_cooldown_enabled": cd > 0, "global_cooldown_seconds": cd})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Update Reward (207)
            207 => {
                let rew_id = self.expand(ctx, s(o, "rewardId"));
                let title = self.expand(ctx, s(o, "title"));
                let prompt = self.expand(ctx, s(o, "prompt"));
                let cost = n(o, "cost") as u64;
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &rew_id)],
                            Some(json!({"title": title, "prompt": prompt, "cost": cost})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Set Background Color (213)
            213 => {
                let rew_id = self.expand(ctx, s(o, "rewardId"));
                let color = self.expand(ctx, s(o, "backgroundColor"));
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &rew_id)],
                            Some(json!({"background_color": color})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Set Max per User per Stream (214)
            214 => {
                let rew_id = self.expand(ctx, s(o, "rewardId"));
                let max = n(o, "redeems") as u64;
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &rew_id)],
                            Some(
                                json!({"is_max_per_user_per_stream_enabled": max > 0, "max_per_user_per_stream": max}),
                            ),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Set Max Per Stream (215)
            215 => {
                let rew_id = self.expand(ctx, s(o, "rewardId"));
                let max = n(o, "redeems") as u64;
                if let Some(tw) = self.twitch() {
                    if let Err(e) = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &rew_id)],
                            Some(json!({"is_max_per_stream_enabled": max > 0, "max_per_stream": max})),
                            false,
                            true,
                        )
                        .await
                    {
                        crate::say(format!("Twitch: {e}"));
                    }
                }
            }
            // Add VIP (502)
            502 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        if let Err(e) = tw
                            .helix(
                                "POST",
                                "channels/vips",
                                vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", &uid)],
                                None,
                                false,
                                true,
                            )
                            .await
                        {
                            crate::say(format!("Twitch: {e}"));
                        }
                    }
                }
            }
            // Remove VIP (503)
            503 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        if let Err(e) = tw
                            .helix(
                                "DELETE",
                                "channels/vips",
                                vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", &uid)],
                                None,
                                false,
                                true,
                            )
                            .await
                        {
                            crate::say(format!("Twitch: {e}"));
                        }
                    }
                }
            }
            // Add Moderator (504)
            504 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        if let Err(e) = tw
                            .helix(
                                "POST",
                                "moderation/moderators",
                                vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", &uid)],
                                None,
                                false,
                                true,
                            )
                            .await
                        {
                            crate::say(format!("Twitch: {e}"));
                        }
                    }
                }
            }
            // Remove Moderator (505)
            505 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        if let Err(e) = tw
                            .helix(
                                "DELETE",
                                "moderation/moderators",
                                vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", &uid)],
                                None,
                                false,
                                true,
                            )
                            .await
                        {
                            crate::say(format!("Twitch: {e}"));
                        }
                    }
                }
            }
            // Get Follower Count (506) / Get Latest Follower (507)
            506 | 507 => {
                if let Some(tw) = self.twitch() {
                    if let Ok(resp) = tw
                        .helix(
                            "GET",
                            "channels/followers",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("first", "1")],
                            None,
                            false,
                            true,
                        )
                        .await
                    {
                        if let Some(total) = resp.pointer("/total") {
                            ctx.args.insert("totalFollowers".into(), total.clone());
                        }
                    }
                }
            }
            // Get Subscriber Count (508) / Get Latest Subscriber (509)
            508 | 509 => {
                if let Some(tw) = self.twitch() {
                    if let Ok(resp) = tw
                        .helix(
                            "GET",
                            "subscriptions",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("first", "1")],
                            None,
                            false,
                            true,
                        )
                        .await
                    {
                        if let Some(total) = resp.pointer("/total") {
                            ctx.args.insert("totalSubscribers".into(), total.clone());
                        }
                    }
                }
            }
            // Clear Chat Messages (510)
            510 => {
                ctx.args.insert("clearSuccess".into(), false.into());
                if let Some(tw) = self.twitch() {
                    match tw
                        .helix(
                            "DELETE",
                            "moderation/chat",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{broadcaster_id}"),
                            ],
                            None,
                            false,
                            true,
                        )
                        .await
                    {
                        Ok(_) => {
                            ctx.args.insert("clearSuccess".into(), true.into());
                        }
                        Err(e) => {
                            crate::say(format!("Twitch: {e}"));
                        }
                    }
                }
            }
            // Ban User (511)
            511 => {
                ctx.args.insert("banResult".into(), false.into());
                let login = self.expand(ctx, s(o, "userLogin"));
                let reason = self.expand(ctx, s(o, "reason"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        ctx.args.insert("bannedUser".into(), login.clone().into());
                        ctx.args.insert("bannedUserName".into(), login.clone().into());
                        ctx.args.insert("bannedUserType".into(), "twitch".into());
                        ctx.args.insert("bannedUserId".into(), uid.clone().into());
                        match tw
                            .helix(
                                "POST",
                                "moderation/bans",
                                vec![
                                    ("broadcaster_id", "{broadcaster_id}"),
                                    ("moderator_id", "{broadcaster_id}"),
                                ],
                                Some(json!({"data": {"user_id": uid, "reason": reason}})),
                                false,
                                true,
                            )
                            .await
                        {
                            Ok(_) => {
                                ctx.args.insert("banResult".into(), true.into());
                            }
                            Err(e) => {
                                crate::say(format!("Twitch: {e}"));
                            }
                        }
                    }
                }
            }
            // Unban User (512) / UnTimeout User (513)
            512 | 513 => {
                let (p, res) = if kind == 512 {
                    ("unbanned", "unbanResult")
                } else {
                    ("unTimedOut", "unTimedOutResult")
                };
                ctx.args.insert(res.into(), false.into());
                let login = self.expand(ctx, s(o, "userLogin")).trim_start_matches('@').to_string();
                let Some(tw) = self.twitch() else { return };
                let Ok(uid) = tw.resolve_user_id(&login).await else {
                    return;
                };
                for (k, x) in [("User", &login), ("UserName", &login), ("UserId", &uid)] {
                    ctx.args.insert(format!("{p}{k}"), x.as_str().into());
                }
                ctx.args.insert(format!("{p}UserType"), "twitch".into());
                let q = vec![
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("moderator_id", "{broadcaster_id}"),
                    ("user_id", uid.as_str()),
                ];
                match tw.helix("DELETE", "moderation/bans", q, None, false, true).await {
                    Ok(_) => drop(ctx.args.insert(res.into(), true.into())),
                    Err(e) => crate::say(format!("Twitch: {e}")),
                }
            }

            // Add Present User (550)
            550 => {
                ctx.args.insert("presentUser".into(), false.into());
                let Some(tw) = self.twitch() else { return };
                let who = vec![
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("moderator_id", "{broadcaster_id}"),
                    ("first", "1000"),
                ];
                match tw.helix("GET", "chat/chatters", who, None, false, true).await {
                    Ok(v) => {
                        ctx.args.insert("presentUsersCount".into(), v["total"].clone());
                        let i = self.expand(ctx, s(o, "userIndex")).trim().parse::<usize>().unwrap_or(0);
                        if let Some(u) = v["data"].as_array().and_then(|a| a.get(i)) {
                            ctx.args.insert("presentUser".into(), u["user_name"].clone());
                            ctx.args.insert("presentUserName".into(), u["user_login"].clone());
                            ctx.args.insert("presentUserId".into(), u["user_id"].clone());
                            ctx.args.insert("presentUserType".into(), "twitch".into());
                        }
                    }
                    Err(e) => crate::say(format!("Twitch: {e}")),
                }
            }
            // Get Twitch Sub Counter Count (551) / Set Twitch Sub Counter Count (552)
            // Twitch subs, resubs and gifted subs add to it (twitch.rs `count_subs`).
            551 => {
                let c = self.cfg.borrow();
                let get = |k: &str| c.counters.get(k).and_then(Value::as_i64).unwrap_or(0);
                let (count, times) = (get("twitch:subCounter"), get("twitch:subCounterRollovers"));
                let rollover = c.twitch.sub_counter_rollover;
                drop(c);
                ctx.args.insert("subCounter.count".into(), count.into());
                ctx.args.insert("subCounter.rollover".into(), rollover.into());
                ctx.args.insert("subCounter.rolloverCount".into(), times.into());
            }
            552 => {
                let v = self.expand(ctx, s(o, "value")).trim().parse::<i64>();
                ctx.args.insert("setResult".into(), v.is_ok().into());
                if let Ok(v) = v {
                    self.change(|c| drop(c.counters.insert("twitch:subCounter".into(), v.into())));
                }
            }
            // User groups from each platform: Add (553, 4005, 35020), Remove (554, 4006, 35021),
            // In Group (558, 4007, 35022). Groups hold user ids, so names are looked up first.
            553 | 554 | 558 | 4005..=4007 | 35020..=35022 => {
                let login = self
                    .expand(ctx, s(o, "userLogin"))
                    .trim()
                    .trim_start_matches('@')
                    .to_string();
                let group = self.simple(ctx, s(o, "groupName"));
                let (platform, op) = match kind {
                    553 | 554 | 558 => ("twitch", [553, 554, 558].iter().position(|k| *k == kind)),
                    4005..=4007 => ("youtube", Some(kind as usize - 4005)),
                    _ => ("kick", Some(kind as usize - 35020)),
                };
                let id = self.user_id_for(platform, &login).await;
                let gid = self
                    .cfg
                    .borrow()
                    .groups
                    .iter()
                    .find(|g| s(g, "name").eq_ignore_ascii_case(&group) || s(g, "id") == group)
                    .map(|g| s(g, "id").to_string());
                match op {
                    Some(2) => {
                        let found = gid.as_ref().is_some_and(|gid| {
                            self.cfg.borrow().groups.iter().any(|g| {
                                s(g, "id") == gid
                                    && g["userIds"].as_array().is_some_and(|u| u.iter().any(|x| text(x) == id))
                            })
                        });
                        ctx.args.insert("inGroup".into(), found.into());
                        if found {
                            for (k, x) in [("foundUser", &login), ("foundUserName", &login), ("foundUserId", &id)] {
                                ctx.args.insert(k.into(), x.as_str().into());
                            }
                            ctx.args.insert("foundUserType".into(), platform.into());
                        }
                    }
                    _ => {
                        let add = op == Some(0);
                        let key = if add { "addResult" } else { "removeResult" };
                        ctx.args.insert(key.into(), gid.is_some().into());
                        if let Some(gid) = gid {
                            let mut u = Obj::new();
                            u.insert("userId".into(), id.into());
                            u.insert("user".into(), login.as_str().into());
                            u.insert("userName".into(), login.as_str().into());
                            u.insert("platform".into(), platform.into());
                            self.group_users(&gid, &[u], add);
                        }
                    }
                }
            }
            // Warn User (565)
            565 => {
                ctx.args.insert("warnedResult".into(), false.into());
                let login = self.expand(ctx, s(o, "userLogin"));
                let reason = self.expand(ctx, s(o, "reason"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        ctx.args.insert("warnedUser".into(), login.clone().into());
                        ctx.args.insert("warnedUserName".into(), login.clone().into());
                        ctx.args.insert("warnedUserType".into(), "twitch".into());
                        ctx.args.insert("warnedUserId".into(), uid.clone().into());
                        match tw
                            .helix(
                                "POST",
                                "moderation/warnings",
                                vec![
                                    ("broadcaster_id", "{broadcaster_id}"),
                                    ("moderator_id", "{broadcaster_id}"),
                                ],
                                Some(json!({"data": {"user_id": uid, "reason": reason}})),
                                false,
                                true,
                            )
                            .await
                        {
                            Ok(_) => {
                                ctx.args.insert("warnedResult".into(), true.into());
                            }
                            Err(e) => {
                                crate::say(format!("Twitch: {e}"));
                            }
                        }
                    }
                }
            }
            // Pinned chat messages (575-578): Helix chat/pins. Update and Unpin act on the message
            // pinned now.
            575..=578 => {
                let Some(tw) = self.twitch() else { return };
                let who = vec![
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("moderator_id", "{broadcaster_id}"),
                ];
                let pinned = tw.helix("GET", "chat/pins", who.clone(), None, false, true).await;
                let current = pinned
                    .as_ref()
                    .ok()
                    .and_then(|v| v.pointer("/data/0").cloned())
                    .unwrap_or_default();
                let duration = n(o, "duration").max(0.0) as u64;
                let secs = duration.to_string();
                let with = |id: &str, dur: bool| {
                    let mut q: Vec<(&str, String)> = who.iter().map(|(k, v)| (*k, v.to_string())).collect();
                    q.push(("message_id", id.to_string()));
                    // 30 s to 30 min; none: until the stream ends.
                    if dur && duration > 0 {
                        q.push(("duration_seconds", secs.clone()));
                    }
                    q
                };
                let current_id = current["message_id"].as_str().unwrap_or("").to_string();
                let r = match kind {
                    575 => {
                        let f = |k: &str| current[k].clone();
                        ctx.args.insert("pinnedMessageId".into(), f("message_id"));
                        ctx.args.insert(
                            "pinnedMessage".into(),
                            current.pointer("/message/text").cloned().unwrap_or_default(),
                        );
                        ctx.args.insert("pinnedUserId".into(), f("sender_user_id"));
                        ctx.args.insert("pinnedUserName".into(), f("sender_user_name"));
                        ctx.args.insert("pinnedByUserName".into(), f("pinned_by_user_name"));
                        pinned.map(drop)
                    }
                    576 => {
                        let id = self.expand(ctx, s(o, "messageId"));
                        let q = with(&id, true);
                        tw.helix(
                            "PUT",
                            "chat/pins",
                            q.iter().map(|(k, v)| (*k, v.as_str())).collect(),
                            None,
                            false,
                            true,
                        )
                        .await
                        .map(drop)
                    }
                    _ if current_id.is_empty() => Err("Twitch: no pinned chat message".into()),
                    577 => {
                        let q = with(&current_id, true);
                        tw.helix(
                            "PATCH",
                            "chat/pins",
                            q.iter().map(|(k, v)| (*k, v.as_str())).collect(),
                            None,
                            false,
                            true,
                        )
                        .await
                        .map(drop)
                    }
                    _ => {
                        let q = with(&current_id, false);
                        tw.helix(
                            "DELETE",
                            "chat/pins",
                            q.iter().map(|(k, v)| (*k, v.as_str())).collect(),
                            None,
                            false,
                            true,
                        )
                        .await
                        .map(drop)
                    }
                };
                if let Err(e) = r {
                    crate::say(format!("Twitch pins: {e}"));
                }
            }
            // Add Suspicious Status (579) / Remove Suspicious Status (580)
            579 | 580 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                let Some(tw) = self.twitch() else { return };
                let r = async {
                    let uid = tw.resolve_user_id(&login).await?;
                    let mut q = vec![
                        ("broadcaster_id", "{broadcaster_id}"),
                        ("moderator_id", "{broadcaster_id}"),
                    ];
                    if kind == 579 {
                        // Monitored: messages show to mods; restricted needs mod approval.
                        let status = if n(o, "status") as i64 == 1 {
                            "RESTRICTED"
                        } else {
                            "ACTIVE_MONITORING"
                        };
                        let body = json!({"user_id": uid, "status": status});
                        tw.helix("POST", "moderation/suspicious_users", q, Some(body), false, true)
                            .await
                    } else {
                        q.push(("user_id", &uid));
                        tw.helix("DELETE", "moderation/suspicious_users", q, None, false, true)
                            .await
                    }
                }
                .await;
                if let Err(e) = r {
                    crate::say(format!("Twitch: {e}"));
                }
            }
            1010 => {
                let g = self.simple(ctx, s(o, "groupName"));
                self.set_commands(|c| s(c, "group") == g, n(o, "state"));
            }
            1007 => self.fetch(ctx, o).await,
            // Get Commands: the aliases, sorted, joined with ", ".
            1013 => {
                let g = self.simple(ctx, s(o, "groupName"));
                let var = self.simple(ctx, s(o, "variableName"));
                let chat = Chat {
                    user: ctx.args.get("user").map(text).unwrap_or_default(),
                    user_id: ctx.args.get("userId").map(text).unwrap_or_default(),
                    moderator: ctx.args.get("isModerator").and_then(Value::as_bool).unwrap_or(false),
                    vip: ctx.args.get("isVip").and_then(Value::as_bool).unwrap_or(false),
                    subscriber: ctx.args.get("isSubscribed").and_then(Value::as_bool).unwrap_or(false),
                    ..Default::default()
                };
                let mut names: Vec<String> = Vec::new();
                for c in self.cfg.borrow().commands.iter() {
                    let on = c.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                    let include = b(o, "includeAll", false) || b(c, "include", true);
                    if !on || !include || !in_group(c, &g) {
                        continue;
                    }
                    if b(o, "hasPermission", false)
                        && !commands::permitted(c, &chat, &|x| self.user_in_group(x, &chat.user_id))
                    {
                        continue;
                    }
                    let a = commands::aliases(c);
                    let take = if b(o, "ignoreAliases", false) { 1 } else { a.len() };
                    names.extend(a.into_iter().take(take).map(String::from));
                }
                names.sort_by_key(|x| x.to_lowercase());
                ctx.args.insert(var, names.join(", ").into());
            }
            // Get Command State
            1014 => {
                let id = self.simple(ctx, s(o, "commandId"));
                let on = self
                    .cfg
                    .borrow()
                    .commands
                    .iter()
                    .find(|c| s(c, "id") == id)
                    .is_some_and(|c| b(c, "enabled", true));
                ctx.args.insert("commandState".into(), on.into());
            }
            // Get Command Group State: 0 all disabled, 1 all enabled, 2 mixed.
            1015 => {
                let g = self.simple(ctx, s(o, "groupName"));
                let (on, off): (Vec<_>, Vec<_>) = {
                    let c = self.cfg.borrow();
                    c.commands
                        .iter()
                        .filter(|x| in_group(x, &g))
                        .map(|x| (s(x, "id").to_string(), b(x, "enabled", true)))
                        .partition(|(_, e)| *e)
                };
                let state = match (on.is_empty(), off.is_empty()) {
                    (false, true) => 1,
                    (true, _) => 0,
                    _ => 2,
                };
                let ids = |v: Vec<(String, bool)>| Value::from(v.into_iter().map(|(i, _)| i).collect::<Vec<_>>());
                ctx.args.insert("commandGroupState".into(), state.into());
                ctx.args.insert("commandsEnabled".into(), ids(on));
                ctx.args.insert("commandsDisabled".into(), ids(off));
            }
            // Clear Users From Group: fires only Cleared (as Streamer.bot).
            10001 => {
                let id = self.simple(ctx, s(o, "groupId"));
                let mut group = None;
                self.change(|c| {
                    if let Some(g) = c.groups.iter_mut().find(|g| s(g, "id") == id) {
                        g.insert("userIds".into(), Value::Array(vec![]));
                        group = Some(g.clone());
                    }
                });
                if let Some(g) = group {
                    self.group_event(T_GROUP_CLEARED, "Cleared", &g, &[]);
                }
            }
            // OBS Studio sub-actions (30..=49, 321..=329)
            // 30: Set Source Visibility State
            30 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let scene = self.expand(ctx, s(o, "sceneName"));
                let source = self.expand(ctx, s(o, "sourceName"));
                let state = n(o, "state") as i64;
                let obs = self.obs_start();
                if let Ok(res) = obs
                    .send_request(
                        conn_ref,
                        "GetSceneItemId",
                        json!({"sceneName": scene, "sourceName": source}),
                    )
                    .await
                {
                    if let Some(item_id) = res.get("sceneItemId").and_then(Value::as_i64) {
                        let target = match state {
                            0 => true,
                            1 => false,
                            _ => {
                                let cur = obs
                                    .send_request(
                                        conn_ref,
                                        "GetSceneItemEnabled",
                                        json!({"sceneName": scene, "sceneItemId": item_id}),
                                    )
                                    .await
                                    .ok()
                                    .and_then(|r| r.get("sceneItemEnabled").and_then(Value::as_bool))
                                    .unwrap_or(false);
                                !cur
                            }
                        };
                        let _ = obs
                            .send_request(
                                conn_ref,
                                "SetSceneItemEnabled",
                                json!({
                                    "sceneName": scene,
                                    "sceneItemId": item_id,
                                    "sceneItemEnabled": target
                                }),
                            )
                            .await;
                    }
                }
            }
            // 31: Set Random Group Source Visible
            31 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let group = self.expand(ctx, s(o, "groupName"));
                let obs = self.obs_start();
                if let Ok(res) = obs
                    .send_request(conn_ref, "GetGroupSceneItemList", json!({"sceneName": group}))
                    .await
                {
                    if let Some(items) = res.get("sceneItems").and_then(Value::as_array) {
                        if !items.is_empty() {
                            let chosen_idx = crate::math::secure_u64() as usize % items.len();
                            for (idx, item) in items.iter().enumerate() {
                                if let Some(item_id) = item.get("sceneItemId").and_then(Value::as_i64) {
                                    let _ = obs
                                        .send_request(
                                            conn_ref,
                                            "SetSceneItemEnabled",
                                            json!({
                                                "sceneName": group,
                                                "sceneItemId": item_id,
                                                "sceneItemEnabled": idx == chosen_idx,
                                            }),
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
            // 32: Hide Group's Sources
            32 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let group = self.expand(ctx, s(o, "groupName"));
                let obs = self.obs_start();
                if let Ok(res) = obs
                    .send_request(conn_ref, "GetGroupSceneItemList", json!({"sceneName": group}))
                    .await
                {
                    if let Some(items) = res.get("sceneItems").and_then(Value::as_array) {
                        for item in items {
                            if let Some(item_id) = item.get("sceneItemId").and_then(Value::as_i64) {
                                let _ = obs
                                    .send_request(
                                        conn_ref,
                                        "SetSceneItemEnabled",
                                        json!({
                                            "sceneName": group,
                                            "sceneItemId": item_id,
                                            "sceneItemEnabled": false,
                                        }),
                                    )
                                    .await;
                            }
                        }
                    }
                }
            }
            // 33: Streaming
            33 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let state = n(o, "state") as i64;
                let obs = self.obs_start();
                match state {
                    0 => {
                        let _ = obs.send_request(conn_ref, "StartStream", json!({})).await;
                    }
                    1 => {
                        let _ = obs.send_request(conn_ref, "StopStream", json!({})).await;
                    }
                    _ => {
                        let _ = obs.send_request(conn_ref, "ToggleStream", json!({})).await;
                    }
                }
            }
            // 34: Set Active Scene
            34 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let scene = self.expand(ctx, s(o, "sceneName"));
                let obs = self.obs_start();
                let _ = obs
                    .send_request(conn_ref, "SetCurrentProgramScene", json!({"sceneName": scene}))
                    .await;
            }
            // 35: Set Source Filter State
            35 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let filter = self.expand(ctx, s(o, "filterName"));
                let state = n(o, "state") as i64;
                let obs = self.obs_start();
                let target = match state {
                    0 => true,
                    1 => false,
                    _ => {
                        let cur = obs
                            .send_request(
                                conn_ref,
                                "GetSourceFilter",
                                json!({"sourceName": source, "filterName": filter}),
                            )
                            .await
                            .ok()
                            .and_then(|r| r.get("filterEnabled").and_then(Value::as_bool))
                            .unwrap_or(false);
                        !cur
                    }
                };
                let _ = obs
                    .send_request(
                        conn_ref,
                        "SetSourceFilterEnabled",
                        json!({
                            "sourceName": source,
                            "filterName": filter,
                            "filterEnabled": target
                        }),
                    )
                    .await;
            }
            // 36: Flip Source
            36 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let scene = self.expand(ctx, s(o, "sceneName"));
                let source = self.expand(ctx, s(o, "sourceName"));
                let flip_mode = n(o, "flipMode") as i64;
                let obs = self.obs_start();
                if let Ok(res) = obs
                    .send_request(
                        conn_ref,
                        "GetSceneItemId",
                        json!({"sceneName": scene, "sourceName": source}),
                    )
                    .await
                {
                    if let Some(item_id) = res.get("sceneItemId").and_then(Value::as_i64) {
                        if let Ok(tr_res) = obs
                            .send_request(
                                conn_ref,
                                "GetSceneItemTransform",
                                json!({"sceneName": scene, "sceneItemId": item_id}),
                            )
                            .await
                        {
                            if let Some(tr) = tr_res.get("sceneItemTransform") {
                                let mut scale_x = tr.get("scaleX").and_then(Value::as_f64).unwrap_or(1.0);
                                let mut scale_y = tr.get("scaleY").and_then(Value::as_f64).unwrap_or(1.0);
                                if flip_mode == 0 || flip_mode == 2 {
                                    scale_x = -scale_x;
                                }
                                if flip_mode == 1 || flip_mode == 2 {
                                    scale_y = -scale_y;
                                }
                                let _ = obs
                                    .send_request(
                                        conn_ref,
                                        "SetSceneItemTransform",
                                        json!({
                                            "sceneName": scene,
                                            "sceneItemId": item_id,
                                            "sceneItemTransform": {
                                                "scaleX": scale_x,
                                                "scaleY": scale_y,
                                            }
                                        }),
                                    )
                                    .await;
                            }
                        }
                    }
                }
            }
            // 37: Rotate Source
            37 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let scene = self.expand(ctx, s(o, "sceneName"));
                let source = self.expand(ctx, s(o, "sourceName"));
                let degrees = n(o, "degrees");
                let additive = b(o, "additive", false);
                let obs = self.obs_start();
                if let Ok(res) = obs
                    .send_request(
                        conn_ref,
                        "GetSceneItemId",
                        json!({"sceneName": scene, "sourceName": source}),
                    )
                    .await
                {
                    if let Some(item_id) = res.get("sceneItemId").and_then(Value::as_i64) {
                        let new_rot = if additive {
                            let cur_rot = obs
                                .send_request(
                                    conn_ref,
                                    "GetSceneItemTransform",
                                    json!({"sceneName": scene, "sceneItemId": item_id}),
                                )
                                .await
                                .ok()
                                .and_then(|r| r.pointer("/sceneItemTransform/rotation").and_then(Value::as_f64))
                                .unwrap_or(0.0);
                            (cur_rot + degrees) % 360.0
                        } else {
                            degrees % 360.0
                        };
                        let _ = obs
                            .send_request(
                                conn_ref,
                                "SetSceneItemTransform",
                                json!({
                                    "sceneName": scene,
                                    "sceneItemId": item_id,
                                    "sceneItemTransform": { "rotation": new_rot }
                                }),
                            )
                            .await;
                    }
                }
            }
            // 38: Set Browser Source URL
            38 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let url = self.expand(ctx, s(o, "url"));
                let obs = self.obs_start();
                let _ = obs
                    .send_request(
                        conn_ref,
                        "SetInputSettings",
                        json!({
                            "inputName": source,
                            "inputSettings": { "url": url },
                            "overlay": true
                        }),
                    )
                    .await;
            }
            // 39: Set GDI Text
            39 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let text_val = self.expand(ctx, s(o, "text"));
                let obs = self.obs_start();
                let _ = obs
                    .send_request(
                        conn_ref,
                        "SetInputSettings",
                        json!({
                            "inputName": source,
                            "inputSettings": { "text": text_val },
                            "overlay": true
                        }),
                    )
                    .await;
            }
            // 40: Set State of a Random Filter
            40 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let state = n(o, "state") as i64;
                let obs = self.obs_start();
                if let Ok(res) = obs
                    .send_request(conn_ref, "GetSourceFilterList", json!({"sourceName": source}))
                    .await
                {
                    if let Some(filters) = res.get("filters").and_then(Value::as_array) {
                        if !filters.is_empty() {
                            let chosen_idx = crate::math::secure_u64() as usize % filters.len();
                            if let Some(f) = filters.get(chosen_idx) {
                                if let Some(fname) = f.get("filterName").and_then(Value::as_str) {
                                    let cur_enabled = f.get("filterEnabled").and_then(Value::as_bool).unwrap_or(false);
                                    let target = match state {
                                        0 => true,
                                        1 => false,
                                        _ => !cur_enabled,
                                    };
                                    let _ = obs
                                        .send_request(
                                            conn_ref,
                                            "SetSourceFilterEnabled",
                                            json!({
                                                "sourceName": source,
                                                "filterName": fname,
                                                "filterEnabled": target
                                            }),
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
            // 41: Hide Source's Filters
            41 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let obs = self.obs_start();
                if let Ok(res) = obs
                    .send_request(conn_ref, "GetSourceFilterList", json!({"sourceName": source}))
                    .await
                {
                    if let Some(filters) = res.get("filters").and_then(Value::as_array) {
                        for f in filters {
                            if let Some(fname) = f.get("filterName").and_then(Value::as_str) {
                                let _ = obs
                                    .send_request(
                                        conn_ref,
                                        "SetSourceFilterEnabled",
                                        json!({
                                            "sourceName": source,
                                            "filterName": fname,
                                            "filterEnabled": false
                                        }),
                                    )
                                    .await;
                            }
                        }
                    }
                }
            }
            // 42: Set Source Mute State
            42 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let state = n(o, "state") as i64;
                let obs = self.obs_start();
                match state {
                    0 => {
                        let _ = obs
                            .send_request(
                                conn_ref,
                                "SetInputMute",
                                json!({"inputName": source, "inputMuted": true}),
                            )
                            .await;
                    }
                    1 => {
                        let _ = obs
                            .send_request(
                                conn_ref,
                                "SetInputMute",
                                json!({"inputName": source, "inputMuted": false}),
                            )
                            .await;
                    }
                    _ => {
                        let _ = obs
                            .send_request(conn_ref, "ToggleInputMute", json!({"inputName": source}))
                            .await;
                    }
                }
            }
            // 43: Get Current Scene
            43 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let obs = self.obs_start();
                if let Ok(res) = obs.send_request(conn_ref, "GetCurrentProgramScene", json!({})).await {
                    if let Some(scene_name) = res.get("currentProgramSceneName").and_then(Value::as_str) {
                        ctx.args.insert("currentScene".into(), scene_name.to_string().into());
                    }
                }
            }
            // 44: Set Source Audio Track State
            44 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let track = n(o, "track") as i64;
                // The dropdown position: Track 1 is 0.
                let track_str = (track + 1).to_string();
                let state = n(o, "state") as i64;
                let obs = self.obs_start();
                if let Ok(res) = obs
                    .send_request(conn_ref, "GetInputAudioTracks", json!({"inputName": source}))
                    .await
                {
                    let cur = res
                        .pointer("/inputAudioTracks")
                        .and_then(|t| t.get(&track_str))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let target = match state {
                        0 => true,
                        1 => false,
                        _ => !cur,
                    };
                    let _ = obs
                        .send_request(
                            conn_ref,
                            "SetInputAudioTracks",
                            json!({
                                "inputName": source,
                                "inputAudioTracks": {
                                    &track_str: target
                                }
                            }),
                        )
                        .await;
                }
            }
            // 45: Recording
            45 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let state = n(o, "state") as i64;
                let obs = self.obs_start();
                match state {
                    0 => {
                        let _ = obs.send_request(conn_ref, "StartRecord", json!({})).await;
                    }
                    1 => {
                        let _ = obs.send_request(conn_ref, "StopRecord", json!({})).await;
                    }
                    2 => {
                        let _ = obs.send_request(conn_ref, "PauseRecord", json!({})).await;
                    }
                    3 => {
                        let _ = obs.send_request(conn_ref, "ResumeRecord", json!({})).await;
                    }
                    _ => {
                        let _ = obs.send_request(conn_ref, "ToggleRecord", json!({})).await;
                    }
                }
            }
            // 46: Raw
            46 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                // `raw` is {"requestType", "requestData"} (obs-raw.streamer.bot's Raw JSON); older
                // ones kept the type in `name` and only the data in `raw`.
                let raw: Value = serde_json::from_str(&self.expand(ctx, s(o, "raw"))).unwrap_or(json!({}));
                let (req_type, payload) = match raw.get("requestType").and_then(Value::as_str) {
                    Some(t) => (t.to_string(), raw.get("requestData").cloned().unwrap_or(json!({}))),
                    None => (self.simple(ctx, s(o, "name")), raw),
                };
                let prefix = match self.simple(ctx, s(o, "prefix")) {
                    p if p.is_empty() => "obsRaw".to_string(),
                    p => p,
                };
                let obs = self.obs_start();
                if let Ok(res) = obs.send_request(conn_ref, &req_type, payload).await {
                    if b(o, "resultsToArgs", true) {
                        flatten(&prefix, &res, &mut ctx.args);
                    }
                }
            }

            // 47: Set Scene Filter State
            47 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let scene = self.expand(ctx, s(o, "sceneName"));
                let filter = self.expand(ctx, s(o, "filterName"));
                let state = n(o, "state") as i64;
                let obs = self.obs_start();
                let target = match state {
                    0 => true,
                    1 => false,
                    _ => {
                        let cur = obs
                            .send_request(
                                conn_ref,
                                "GetSourceFilter",
                                json!({"sourceName": scene, "filterName": filter}),
                            )
                            .await
                            .ok()
                            .and_then(|r| r.get("filterEnabled").and_then(Value::as_bool))
                            .unwrap_or(false);
                        !cur
                    }
                };
                let _ = obs
                    .send_request(
                        conn_ref,
                        "SetSourceFilterEnabled",
                        json!({
                            "sourceName": scene,
                            "filterName": filter,
                            "filterEnabled": target
                        }),
                    )
                    .await;
            }
            // 48: Take Screenshot
            48 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let scene = self.expand(ctx, s(o, "sceneName"));
                let source = self.expand(ctx, s(o, "sourceName"));
                let path = self.expand(ctx, s(o, "path"));
                let quality = n(o, "quality") as i64;
                let target_name = if !source.is_empty() { source } else { scene };
                let ext = std::path::Path::new(&path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png");
                let obs = self.obs_start();
                let _ = obs
                    .send_request(
                        conn_ref,
                        "SaveSourceScreenshot",
                        json!({
                            "sourceName": target_name,
                            "imageFilePath": path,
                            "imageFormat": ext,
                            "imageCompressionQuality": quality
                        }),
                    )
                    .await;
            }
            // 49: Set Media State
            49 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let state = n(o, "state") as i64;
                let action = match state {
                    0 => "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_PLAY",
                    1 => "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_PAUSE",
                    2 => "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_RESTART",
                    3 => "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_STOP",
                    4 => "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_NEXT",
                    _ => "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_PREVIOUS",
                };
                let obs = self.obs_start();
                let _ = obs
                    .send_request(
                        conn_ref,
                        "TriggerMediaInputAction",
                        json!({
                            "inputName": source,
                            "mediaAction": action
                        }),
                    )
                    .await;
            }
            // 321: Set Replay Buffer State
            321 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let state = n(o, "state") as i64;
                let obs = self.obs_start();
                match state {
                    0 => {
                        let _ = obs.send_request(conn_ref, "StartReplayBuffer", json!({})).await;
                    }
                    1 => {
                        let _ = obs.send_request(conn_ref, "StopReplayBuffer", json!({})).await;
                    }
                    2 => {
                        let _ = obs.send_request(conn_ref, "SaveReplayBuffer", json!({})).await;
                    }
                    _ => {
                        let _ = obs.send_request(conn_ref, "ToggleReplayBuffer", json!({})).await;
                    }
                }
            }
            // 322: Set Media Source File
            322 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let file_name = self.expand(ctx, s(o, "fileName"));
                let obs = self.obs_start();
                let _ = obs
                    .send_request(
                        conn_ref,
                        "SetInputSettings",
                        json!({
                            "inputName": source,
                            "inputSettings": { "local_file": file_name },
                            "overlay": true
                        }),
                    )
                    .await;
            }
            // 323: Set Image Source File
            323 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let file_name = self.expand(ctx, s(o, "fileName"));
                let obs = self.obs_start();
                let _ = obs
                    .send_request(
                        conn_ref,
                        "SetInputSettings",
                        json!({
                            "inputName": source,
                            "inputSettings": { "file": file_name },
                            "overlay": true
                        }),
                    )
                    .await;
            }
            // 324: Get Scene Item Properties
            324 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let scene = self.expand(ctx, s(o, "sceneName"));
                let source = self.expand(ctx, s(o, "sourceName"));
                let obs = self.obs_start();
                let get = |t: &'static str, d: Value| {
                    let obs = obs.clone();
                    async move { obs.send_request(conn_ref, t, d).await.unwrap_or_default() }
                };
                let id = get("GetSceneItemId", json!({"sceneName": scene, "sourceName": source})).await["sceneItemId"]
                    .clone();
                if id.is_null() {
                    return;
                }
                let item = json!({"sceneName": scene, "sceneItemId": id});
                let t = get("GetSceneItemTransform", item.clone()).await["sceneItemTransform"].clone();
                let visible = get("GetSceneItemEnabled", item.clone()).await["sceneItemEnabled"].clone();
                let locked = get("GetSceneItemLocked", item).await["sceneItemLocked"].clone();
                let muted = get("GetInputMute", json!({"inputName": source})).await["inputMuted"]
                    .as_bool()
                    .unwrap_or(false);
                // obs-websocket 4's GetSceneItemProperties, which Streamer.bot's props.* follow.
                let props = json!({
                    "message-id": "", "status": "ok", "name": source, "itemId": id,
                    "position": {"x": t["positionX"], "y": t["positionY"], "alignment": t["alignment"]},
                    "rotation": t["rotation"],
                    "scale": {"x": t["scaleX"], "y": t["scaleY"], "filter": "OBS_SCALE_DISABLE"},
                    "crop": {"top": t["cropTop"], "right": t["cropRight"], "bottom": t["cropBottom"], "left": t["cropLeft"]},
                    "visible": visible, "muted": muted, "locked": locked,
                    "bounds": {"type": t["boundsType"], "alignment": t["boundsAlignment"], "x": t["boundsWidth"], "y": t["boundsHeight"]},
                    "sourceWidth": t["sourceWidth"], "sourceHeight": t["sourceHeight"],
                    "width": t["width"], "height": t["height"],
                });
                ctx.args.insert("props._json".into(), props.to_string().into());
                flatten("props", &props, &mut ctx.args);
            }

            // 325: Hide Scene's Sources
            325 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let scene = self.expand(ctx, s(o, "sceneName"));
                let obs = self.obs_start();
                if let Ok(res) = obs
                    .send_request(conn_ref, "GetSceneItemList", json!({"sceneName": scene}))
                    .await
                {
                    if let Some(items) = res.get("sceneItems").and_then(Value::as_array) {
                        for item in items {
                            if let Some(item_id) = item.get("sceneItemId").and_then(Value::as_i64) {
                                let _ = obs
                                    .send_request(
                                        conn_ref,
                                        "SetSceneItemEnabled",
                                        json!({
                                            "sceneName": scene,
                                            "sceneItemId": item_id,
                                            "sceneItemEnabled": false
                                        }),
                                    )
                                    .await;
                            }
                        }
                    }
                }
            }
            // 326: Set Random Scene Source Visible
            326 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let scene = self.expand(ctx, s(o, "sceneName"));
                let obs = self.obs_start();
                if let Ok(res) = obs
                    .send_request(conn_ref, "GetSceneItemList", json!({"sceneName": scene}))
                    .await
                {
                    if let Some(items) = res.get("sceneItems").and_then(Value::as_array) {
                        if !items.is_empty() {
                            let chosen_idx = crate::math::secure_u64() as usize % items.len();
                            if let Some(item) = items.get(chosen_idx) {
                                if let Some(item_id) = item.get("sceneItemId").and_then(Value::as_i64) {
                                    let _ = obs
                                        .send_request(
                                            conn_ref,
                                            "SetSceneItemEnabled",
                                            json!({
                                                "sceneName": scene,
                                                "sceneItemId": item_id,
                                                "sceneItemEnabled": true
                                            }),
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
            // 327: Set Color Source Color
            327 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let random = b(o, "random", false);
                let color_val = if random {
                    0xFF00_0000 | (crate::math::secure_u64() & 0x00FF_FFFF) as i64
                } else {
                    obs_color(&self.expand(ctx, s(o, "color")))
                };
                let obs = self.obs_start();
                let _ = obs
                    .send_request(
                        conn_ref,
                        "SetInputSettings",
                        json!({
                            "inputName": source,
                            "inputSettings": { "color": color_val },
                            "overlay": true
                        }),
                    )
                    .await;
            }
            // 328: Get Status
            328 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let obs = self.obs_start();
                let active = |r: Result<Value, String>| r.ok().and_then(|v| v["outputActive"].as_bool());
                let streaming = active(obs.send_request(conn_ref, "GetStreamStatus", json!({})).await);
                let recording = active(obs.send_request(conn_ref, "GetRecordStatus", json!({})).await);
                ctx.args.insert("obs.isConnected".into(), streaming.is_some().into());
                ctx.args
                    .insert("obs.isStreaming".into(), streaming.unwrap_or(false).into());
                ctx.args
                    .insert("obs.isRecording".into(), recording.unwrap_or(false).into());
            }

            // 329: Create Record Chapter
            329 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let chapter = self.expand(ctx, s(o, "chapterName"));
                let obs = self.obs_start();
                let ok = obs
                    .send_request(
                        conn_ref,
                        "CreateRecordChapter",
                        json!({
                            "chapterName": chapter
                        }),
                    )
                    .await
                    .is_ok();
                ctx.args.insert("success".into(), ok.into());
            }
            // YouTube Sub-actions
            // 4001: Send Message to Channel
            4001 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let text = self.expand(ctx, s(o, "text"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                if !self.dev_out("youtube", &text, use_bot) {
                    let yt = self.youtube_start();
                    if let Err(e) = yt.send_message(&broadcast, &text, use_bot, fallback).await {
                        crate::say(e);
                    }
                }
            }
            // 4002: Set Title
            4002 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let title = self.expand(ctx, s(o, "title"));
                let yt = self.youtube_start();
                if let Err(e) = yt.set_title(&broadcast, &title).await {
                    crate::say(e);
                }
            }
            // 4003: Set Description
            4003 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let desc = self.expand(ctx, s(o, "description"));
                let yt = self.youtube_start();
                if let Err(e) = yt.set_description(&broadcast, &desc).await {
                    crate::say(e);
                }
            }
            // 4004: Set Title and Description
            4004 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let title = self.expand(ctx, s(o, "title"));
                let desc = self.expand(ctx, s(o, "description"));
                let yt = self.youtube_start();
                if let Err(e) = yt.set_title_and_description(&broadcast, &title, &desc).await {
                    crate::say(e);
                }
            }
            // 4008: Ban User
            4008 => {
                ctx.args.insert("banResult".into(), false.into());
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let user_id = self.expand(ctx, s(o, "userId"));
                let yt = self.youtube_start();
                ctx.args.insert("bannedUser".into(), user_id.clone().into());
                ctx.args.insert("bannedUserName".into(), user_id.clone().into());
                ctx.args.insert("bannedUserType".into(), "youtube".into());
                ctx.args.insert("bannedUserId".into(), user_id.clone().into());
                match yt.ban_user(&broadcast, &user_id).await {
                    Ok(_) => {
                        ctx.args.insert("banResult".into(), true.into());
                    }
                    Err(e) => {
                        crate::say(e);
                    }
                }
            }
            // 4009: Timeout User
            4009 => {
                ctx.args.insert("timeoutResult".into(), false.into());
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let user_id = self.expand(ctx, s(o, "userId"));
                let duration = n(o, "duration") as u64;
                let yt = self.youtube_start();
                ctx.args.insert("bannedUser".into(), user_id.clone().into());
                ctx.args.insert("bannedUserName".into(), user_id.clone().into());
                ctx.args.insert("bannedUserType".into(), "youtube".into());
                ctx.args.insert("bannedUserId".into(), user_id.clone().into());
                match yt.timeout_user(&broadcast, &user_id, duration).await {
                    Ok(_) => {
                        ctx.args.insert("timeoutResult".into(), true.into());
                    }
                    Err(e) => {
                        crate::say(e);
                    }
                }
            }
            // 4010: Add Broadcaster Information
            4010 => {
                let c = self.cfg.borrow();
                let chan_id = c.youtube.channel_id.clone();
                ctx.args.insert("broadcasterUserId".into(), chan_id.into());
                if let Some(b) = &c.youtube.broadcaster {
                    ctx.args.insert("broadcasterUser".into(), b.name.clone().into());
                    ctx.args.insert("broadcasterUserName".into(), b.name.clone().into());
                }
            }
            // 4011: Create Poll
            4011 => {
                ctx.args.insert("success".into(), false.into());
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let question = self.expand(ctx, s(o, "question"));
                let options: Vec<String> = list(o.get("options")).iter().map(|x| self.expand(ctx, x)).collect();
                let yt = self.youtube_start();
                match yt.create_poll(&broadcast, &question, &options).await {
                    Ok(id) => {
                        ctx.args.insert("success".into(), true.into());
                        ctx.args.insert("pollId".into(), id.into());
                    }
                    Err(e) => {
                        crate::say(e);
                    }
                }
            }
            // 4012: End Poll
            4012 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let yt = self.youtube_start();
                if let Err(e) = yt.end_poll(&broadcast).await {
                    crate::say(e);
                }
            }
            // Kick Sub-actions
            // 35001: Send Message to Channel
            35001 => {
                let text = self.expand(ctx, s(o, "text"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                if !self.dev_out("kick", &text, use_bot) {
                    let kick = self.kick_start();
                    if let Err(e) = kick.send_message(&text, use_bot, fallback).await {
                        crate::say(e);
                    }
                }
            }
            // 35002: Reply To Message
            35002 => {
                let text = self.expand(ctx, s(o, "text"));
                let reply_id = self.expand(ctx, s(o, "replyId"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                if !self.dev_out("kick", &text, use_bot) {
                    let kick = self.kick_start();
                    if let Err(e) = kick.reply_message(&text, &reply_id, use_bot, fallback).await {
                        crate::say(e);
                    }
                }
            }
            // 35010: Add Broadcaster Information
            35010 => {
                let chan = self.cfg.borrow().kick.channel.clone();
                ctx.args.insert("broadcastUser".into(), chan.as_str().into());
                ctx.args.insert("broadcastUserName".into(), chan.to_lowercase().into());
                ctx.args.insert("broadcastUserType".into(), "kick".into());
                ctx.args.insert(
                    "broadcasterProfileUrl".into(),
                    format!("https://kick.com/{}", chan.to_lowercase()).into(),
                );
                let id = self.user_id_for("kick", &chan).await;
                ctx.args.insert("broadcastUserId".into(), id.into());
            }

            // 35011: Get User Info for Target
            35011 => {
                ctx.args.insert("addTargetResult".into(), false.into());
                let login = self
                    .expand(ctx, s(o, "userLogin"))
                    .trim()
                    .trim_start_matches('@')
                    .to_lowercase();
                let kick = self.kick_start();
                let slug = crate::commands::url_encode(&login);
                match kick.api("GET", &format!("/channels?slug={slug}"), None).await {
                    Ok(v) if v["data"][0].is_object() => {
                        let c = &v["data"][0];
                        let tags: Vec<String> = c["stream"]["custom_tags"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|t| t.as_str().map(String::from))
                            .collect();
                        let desc = c["channel_description"].as_str().unwrap_or("").to_string();
                        let mut put = |k: &str, x: Value| drop(ctx.args.insert(k.into(), x));
                        put("addTargetResult", true.into());
                        put("targetUser", c["slug"].clone());
                        put("targetUserName", c["slug"].clone());
                        put("targetUserId", c["broadcaster_user_id"].clone());
                        put("targetUserType", "kick".into());
                        put("targetDescription", desc.as_str().into());
                        put("targetDescriptionEscaped", crate::commands::escape(&desc).into());
                        put("targetChannelTitle", c["stream_title"].clone());
                        put("game", c["category"]["name"].clone());
                        put("gameId", c["category"]["id"].clone());
                        put("tagCount", tags.len().into());
                        for (i, t) in tags.iter().enumerate() {
                            put(&format!("tag{i}"), t.as_str().into());
                        }
                        put("tagsDelimited", tags.join(", ").into());
                        put("tags", tags.into());
                    }
                    Ok(_) => crate::say(format!("Kick: no user '{login}'")),
                    Err(e) => crate::say(e),
                }
            }

            // 35012: Add Random Users
            // Kick has no list of who is in chat: the people who chatted this session.
            35012 => {
                let count = (n(o, "count") as usize).max(1);
                let mut users = self.chatters("kick");
                let take = count.min(users.len());
                for i in 0..take {
                    let pick = i + (crate::math::secure_u64() as usize) % (users.len() - i);
                    users.swap(i, pick);
                    let (name, id) = &users[i];
                    ctx.args.insert(format!("randomUser{i}"), name.as_str().into());
                    ctx.args
                        .insert(format!("randomUserName{i}"), name.to_lowercase().into());
                    ctx.args.insert(format!("randomUserId{i}"), id.as_str().into());
                    ctx.args.insert(format!("randomUserType{i}"), "kick".into());
                }
                ctx.args.insert("randomUsersCount".into(), take.into());
            }

            // 35030: Set Channel Title
            35030 => {
                ctx.args.insert("titleSuccess".into(), false.into());
                let title = self.expand(ctx, s(o, "title"));
                let kick = self.kick_start();
                match kick.set_channel_title(&title).await {
                    Ok(_) => {
                        ctx.args.insert("titleSuccess".into(), true.into());
                    }
                    Err(e) => {
                        crate::say(e);
                    }
                }
            }
            // 35031: Set Channel Category
            35031 => {
                ctx.args.insert("category.success".into(), false.into());
                let id = n(o, "categoryId") as u64;
                let name = self.expand(ctx, s(o, "categoryName"));
                let kick = self.kick_start();
                let r: Result<(), String> = async {
                    // By id, else by name (Kick's v2 search, exact name first).
                    let q = if id > 0 {
                        format!("id={id}")
                    } else {
                        format!("name={}", crate::commands::url_encode(&name))
                    };
                    let v = kick
                        .api("GET", &format!("https://api.kick.com/public/v2/categories?{q}"), None)
                        .await?;
                    let list = v["data"].as_array().cloned().unwrap_or_default();
                    let c = list
                        .iter()
                        .find(|c| id > 0 || c["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(&name)))
                        .or(list.first())
                        .cloned()
                        .ok_or_else(|| format!("Kick: no category '{name}'"))?;
                    for (k, f) in [("id", "id"), ("name", "name"), ("thumbnail", "thumbnail")] {
                        ctx.args.insert(format!("category.{k}"), c[f].clone());
                    }
                    kick.set_channel_category(c["id"].as_u64().unwrap_or(0), &name).await
                }
                .await;
                match r {
                    Ok(()) => drop(ctx.args.insert("category.success".into(), true.into())),
                    Err(e) => crate::say(e),
                }
            }

            // 35040: Ban User
            35040 => {
                ctx.args.insert("banResult".into(), false.into());
                let user = self.expand(ctx, s(o, "userLogin"));
                let reason = self.expand(ctx, s(o, "reason"));
                let kick = self.kick_start();
                ctx.args.insert("bannedUser".into(), user.clone().into());
                ctx.args.insert("bannedUserName".into(), user.clone().into());
                ctx.args.insert("bannedUserType".into(), "kick".into());
                match kick.ban_user(&user, &reason).await {
                    Ok(_) => {
                        ctx.args.insert("banResult".into(), true.into());
                    }
                    Err(e) => {
                        crate::say(e);
                    }
                }
            }
            // 35041: Unban User
            35041 => {
                ctx.args.insert("unbanResult".into(), false.into());
                let user = self.expand(ctx, s(o, "userLogin"));
                let kick = self.kick_start();
                ctx.args.insert("unbannedUser".into(), user.clone().into());
                ctx.args.insert("unbannedUserName".into(), user.clone().into());
                ctx.args.insert("unbannedUserType".into(), "kick".into());
                match kick.unban_user(&user).await {
                    Ok(_) => {
                        ctx.args.insert("unbanResult".into(), true.into());
                    }
                    Err(e) => {
                        crate::say(e);
                    }
                }
            }
            // 35042: Timeout User
            35042 => {
                ctx.args.insert("timeoutResult".into(), false.into());
                let user = self.expand(ctx, s(o, "userLogin"));
                let duration = n(o, "duration") as u64;
                let reason = self.expand(ctx, s(o, "reason"));
                let kick = self.kick_start();
                ctx.args.insert("timedOutUser".into(), user.clone().into());
                ctx.args.insert("timedOutUserName".into(), user.clone().into());
                ctx.args.insert("timedOutUserType".into(), "kick".into());
                match kick.timeout_user(&user, duration, &reason).await {
                    Ok(_) => {
                        ctx.args.insert("timeoutResult".into(), true.into());
                    }
                    Err(e) => {
                        crate::say(e);
                    }
                }
            }
            // 35043: UnTimeout User
            35043 => {
                ctx.args.insert("unTimedOutResult".into(), false.into());
                let user = self.expand(ctx, s(o, "userLogin"));
                let kick = self.kick_start();
                ctx.args.insert("unTimedOutUser".into(), user.clone().into());
                ctx.args.insert("unTimedOutUserName".into(), user.clone().into());
                ctx.args.insert("unTimedOutUserType".into(), "kick".into());
                match kick.untimeout_user(&user).await {
                    Ok(_) => {
                        ctx.args.insert("unTimedOutResult".into(), true.into());
                    }
                    Err(e) => {
                        crate::say(e);
                    }
                }
            }
            // 35101..=35113: Channel Rewards sub-actions
            // Kick channel rewards (official API; only rewards made by this app's Kick app can be
            // changed, as on Twitch).
            35101..=35103 | 35105..=35108 | 35113 => {
                let id = self.expand(ctx, s(o, "rewardId"));
                ctx.args.insert("rewardId".into(), id.as_str().into());
                let kick = self.kick_start();
                let path = format!("/channels/rewards/{}", crate::commands::url_encode(&id));
                let current = || async {
                    kick.api("GET", "/channels/rewards", None)
                        .await?
                        .get("data")
                        .and_then(Value::as_array)
                        .and_then(|a| a.iter().find(|r| r["id"] == id.as_str()).cloned())
                        .ok_or_else(|| format!("Kick: no reward {id}"))
                };
                let cost = |cur: &Value| {
                    // Operator: 0 set, 1 add, 2 subtract (as Twitch's Set Cost).
                    let v = n(o, "cost") as i64;
                    let now = cur["cost"].as_i64().unwrap_or(0);
                    match n(o, "operator") as i64 {
                        1 => now + v,
                        2 => now - v,
                        _ => v,
                    }
                    .max(1)
                };
                let r: Result<(), String> = async {
                    let body = match kind {
                        35101 | 35102 => {
                            let key = if kind == 35101 { "is_enabled" } else { "is_paused" };
                            let on = new_state(n(o, "state"), current().await?[key].as_bool().unwrap_or(false));
                            // Enabled: 0 enable; Paused: 0 pause (Streamer.bot's order).
                            json!({ key: on })
                        }
                        35103 => json!({"cost": cost(&current().await?)}),
                        35105 => json!({"title": self.expand(ctx, s(o, "title"))}),
                        35106 => json!({"description": self.expand(ctx, s(o, "description"))}),
                        35107 => {
                            let cur = current().await?;
                            let mut b = json!({"cost": cost(&cur)});
                            for (f, k) in [("title", "title"), ("description", "description")] {
                                let v = self.expand(ctx, s(o, f));
                                if !v.is_empty() {
                                    b[k] = v.into();
                                }
                            }
                            b
                        }
                        35113 => {
                            let c = if b(o, "random", false) {
                                format!("#{:06X}", crate::math::secure_u64() & 0xFF_FFFF)
                            } else {
                                // "#AARRGGBB" from the color picker: Kick takes #RRGGBB.
                                let h = self.expand(ctx, s(o, "backgroundColor"));
                                let h = h.trim_start_matches('#');
                                format!("#{}", &h[h.len().saturating_sub(6)..])
                            };
                            json!({"background_color": c})
                        }
                        _ => {
                            let r = current().await?;
                            let f = |k: &str| r[k].clone();
                            for (arg, k) in [
                                ("rewardTitle", "title"),
                                ("rewardPrompt", "description"),
                                ("rewardCost", "cost"),
                                ("rewardBackgroundColor", "background_color"),
                                ("rewardEnabled", "is_enabled"),
                                ("rewardPaused", "is_paused"),
                                ("rewardSkipRequestQueue", "should_redemptions_skip_request_queue"),
                            ] {
                                ctx.args.insert(arg.into(), f(k));
                            }
                            ctx.args.insert("rewardIsOurs".into(), true.into());
                            return Ok(());
                        }
                    };
                    kick.api("PATCH", &path, Some(body)).await.map(drop)
                }
                .await;
                if let Err(e) = r {
                    crate::say(e);
                }
            }
            // TikTok Sub-actions
            // 43001: Send Message to Live Chat
            43001 => {
                let text = self.expand(ctx, s(o, "text"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                if !self.dev_out("tiktok", &text, use_bot) {
                    let tiktok = self.tiktok_start();
                    if let Err(e) = tiktok.send_message(&text, use_bot, fallback).await {
                        crate::say(e);
                    }
                }
            }

            // 43002: Get Top Gifters, 43003: Get Stream Info (this live's, as read)
            43002 => ctx
                .args
                .extend(crate::tiktok::top_gifters(n(o, "count").max(1.0) as usize)),
            43003 => {
                let live = self.tiktok().is_some_and(|t| t.is_connected());
                ctx.args.extend(crate::tiktok::stream_info(live));
            }

            // Discord Basic Webhook (7001)
            7001 => {
                let url = self.expand(ctx, s(o, "webhookUrl"));
                let content = self.expand(ctx, s(o, "content"));
                let username = self.expand(ctx, s(o, "username"));
                let avatar = self.expand(ctx, s(o, "avatar"));
                let image = self.expand(ctx, s(o, "image"));
                let tts = b(o, "textToSpeech", false);
                match crate::remote::execute_discord_webhook(&url, &content, &username, &avatar, &image, tts).await {
                    Ok(id) => drop(ctx.args.insert("messageId".into(), id.into())),
                    Err(e) => crate::say(format!("Discord webhook: {e}")),
                }
            }

            // IFTTT Notification Basic (8001)
            8001 => {
                let event = self.expand(ctx, s(o, "eventName"));
                let text_val = self.expand(ctx, s(o, "text"));
                let key = ctx
                    .args
                    .get("iftttKey")
                    .or_else(|| ctx.args.get("key"))
                    .and_then(Value::as_str);
                let payload = crate::remote::build_ifttt_basic_payload(&text_val);
                if let Err(e) = crate::remote::execute_ifttt_notification(&event, &payload, key).await {
                    crate::say(format!("IFTTT: {e}"));
                }
            }

            // IFTTT Notification Advanced (8002)
            8002 => {
                let event = self.expand(ctx, s(o, "eventName"));
                let custom_args = o.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
                let mut expanded_args = Obj::new();
                for (k, v) in &custom_args {
                    let val_str = self.expand(ctx, &text(v));
                    expanded_args.insert(k.clone(), val_str.into());
                }
                let key = ctx
                    .args
                    .get("iftttKey")
                    .or_else(|| ctx.args.get("key"))
                    .and_then(Value::as_str);
                let payload = crate::remote::build_ifttt_advanced_payload(&expanded_args);
                if let Err(e) = crate::remote::execute_ifttt_notification(&event, &payload, key).await {
                    crate::say(format!("IFTTT: {e}"));
                }
            }

            // Pronouns: Add Pronouns for User (15001)
            15001 => {
                let mut login = self.expand(ctx, s(o, "userLogin"));
                if login.is_empty() {
                    login = ctx
                        .args
                        .get("userLogin")
                        .or_else(|| ctx.args.get("user"))
                        .or_else(|| ctx.args.get("userName"))
                        .map(text)
                        .unwrap_or_default();
                }
                let prefix = self.expand(ctx, s(o, "prefix"));
                let info = crate::remote::get_user_pronouns(&login).await;
                crate::remote::populate_pronoun_vars(&mut ctx.args, &prefix, info.as_ref());
            }

            // Send Remote Trigger (20001)
            20001 => {
                let inst = self.expand(ctx, s(o, "instanceId"));
                let event = self.expand(ctx, s(o, "eventName"));
                let mut payload = if b(o, "useArge", false) {
                    ctx.args.clone()
                } else {
                    Obj::new()
                };
                if let Some(m) = o.get("customArgs").and_then(Value::as_object) {
                    for (k, v) in m {
                        payload.insert(k.clone(), self.expand(ctx, &text(v)).into());
                    }
                }
                let cfg = self.config();
                payload.insert("instanceId".into(), cfg.instance_id.clone().into());
                payload.insert("instanceName".into(), cfg.instance_name.clone().into());
                payload.insert(
                    "instanceOs".into(),
                    (if cfg!(windows) { "windows" } else { "linux" }).into(),
                );
                self.fire_remote_trigger(&inst, &event, payload);
            }

            // Send Remote Signal (20002)
            20002 => {
                let inst = self.expand(ctx, s(o, "instanceId"));
                let sig = self.expand(ctx, s(o, "signalName"));
                let mut payload = if b(o, "useArge", false) {
                    ctx.args.clone()
                } else {
                    Obj::new()
                };
                if let Some(m) = o.get("customArgs").and_then(Value::as_object) {
                    for (k, v) in m {
                        payload.insert(k.clone(), self.expand(ctx, &text(v)).into());
                    }
                }
                let cfg = self.config();
                payload.insert("instanceId".into(), cfg.instance_id.clone().into());
                payload.insert("instanceName".into(), cfg.instance_name.clone().into());
                payload.insert(
                    "instanceOs".into(),
                    (if cfg!(windows) { "windows" } else { "linux" }).into(),
                );
                self.deliver_remote_signal(&inst, &sig, payload);
            }

            // Wait For Remote Signal (20003)
            20003 => {
                let inst = self.expand(ctx, s(o, "instanceId"));
                let sig = self.expand(ctx, s(o, "signalName"));
                let timeout = n(o, "timeout") as u64;
                let overwrite = b(o, "overwrite", false);
                let got = self.wait_remote_signal(&inst, &sig, timeout).await;
                match got {
                    Some(payload) => {
                        for (k, v) in payload {
                            if overwrite || !ctx.args.contains_key(&k) {
                                ctx.args.insert(k, v);
                            }
                        }
                        ctx.args.insert("signalTimeout".into(), false.into());
                        ctx.args.insert("hasResult".into(), true.into());
                        ctx.args.insert("instanceId".into(), inst.as_str().into());
                        ctx.args.insert("signalName".into(), sig.as_str().into());
                    }
                    None => {
                        ctx.args.insert("signalTimeout".into(), true.into());
                        ctx.args.insert("hasResult".into(), false.into());
                    }
                }
            }

            // Keyboard Press (1012)
            1012 => {
                let key = self.expand(ctx, s(o, "key"));
                let modifiers = n(o, "modifiers") as u32;
                let _ = crate::inputs::simulate_keyboard_press(&key, modifiers);
            }

            // Set Voice Control Command (1018)
            1018 => {
                let cmd_id = self.expand(ctx, s(o, "commandId"));
                let cmd_phrase = self.expand(ctx, s(o, "command"));
                self.set_voice_command(&cmd_id, &cmd_phrase);
            }

            // Set Voice Control Command State (1019)
            1019 => {
                let cmd_id = self.expand(ctx, s(o, "commandId"));
                let state = n(o, "state");
                self.set_voice_command_state(&cmd_id, state);
            }

            // Set Voice Control Input (1025)
            1025 => {
                let dev_id = self.expand(ctx, s(o, "audioDeviceId"));
                let dev_name = self.expand(ctx, s(o, "audioDeviceName"));
                self.set_voice_input(&dev_id, &dev_name);
            }

            // MIDI Note On (9001)
            9001 => {
                let device = self.expand(ctx, s(o, "device"));
                let channel = n(o, "channel") as u8;
                let note = n(o, "note") as u8;
                let velocity = n(o, "velocity") as u8;
                let send_off = b(o, "sendNoteOff", false);
                let duration = n(o, "duration") as u64;
                crate::inputs::midi_note_on(&device, channel, note, velocity, send_off, duration);
            }

            // MIDI Control Change (9002)
            9002 => {
                let device = self.expand(ctx, s(o, "device"));
                let channel = n(o, "channel") as u8;
                let controller = n(o, "controller") as u8;
                let value = n(o, "value") as u8;
                crate::inputs::midi_control_change(&device, channel, controller, value);
            }

            // MIDI Generic Event (9003)
            9003 => {
                let device = self.expand(ctx, s(o, "device"));
                let cmd = self.expand(ctx, s(o, "command"));
                let channel = n(o, "channel") as u8;
                let data1 = n(o, "data1") as u8;
                let data2 = n(o, "data2") as u8;
                crate::inputs::midi_generic(&device, &cmd, channel, data1, data2);
            }

            // Elgato Stream Deck Set Title (11001)
            11001 => {
                let button_id = self.expand(ctx, s(o, "buttonId"));
                let title = self.expand(ctx, s(o, "title"));
                let state = n(o, "state");
                self.set_stream_deck_title(&button_id, &title, state);
            }

            // Elgato Stream Deck Set Background (11002)
            11002 => {
                let button_id = self.expand(ctx, s(o, "buttonId"));
                let image = self.expand(ctx, s(o, "image"));
                let color = self.expand(ctx, s(o, "color"));
                let state = n(o, "state");
                self.set_stream_deck_background(&button_id, &image, &color, state);
            }

            // Elgato Stream Deck Set State (11004)
            11004 => {
                let button_id = self.expand(ctx, s(o, "buttonId"));
                let state = n(o, "state");
                self.set_stream_deck_state(&button_id, state);
            }

            // Elgato Stream Deck Show Alert (11005)
            11005 => {
                let button_id = self.expand(ctx, s(o, "buttonId"));
                self.show_stream_deck_alert(&button_id);
            }

            // Elgato Stream Deck Show Ok (11006)
            11006 => {
                let button_id = self.expand(ctx, s(o, "buttonId"));
                self.show_stream_deck_ok(&button_id);
            }

            // Elgato Stream Deck Toggle State (11007)
            11007 => {
                let button_id = self.expand(ctx, s(o, "buttonId"));
                self.toggle_stream_deck_state(&button_id);
            }

            // Elgato Stream Deck Set Value (11008)
            11008 => {
                let button_id = self.expand(ctx, s(o, "buttonId"));
                let value = self.expand(ctx, s(o, "value"));
                self.set_stream_deck_value(&button_id, &value);
            }

            // Streamer.bot Deck Set Item State (21001)
            21001 => {
                let deck_id = self.expand(ctx, s(o, "deckId"));
                let deck_name = self.expand(ctx, s(o, "deckName"));
                let item_id = self.expand(ctx, s(o, "itemId"));
                let item_name = self.expand(ctx, s(o, "itemName"));
                let state = n(o, "state");
                self.set_deck_item_state(&deck_id, &deck_name, &item_id, &item_name, state);
            }

            // Streamer.bot Deck Toggle Item State (21002)
            21002 => {
                let deck_id = self.expand(ctx, s(o, "deckId"));
                let deck_name = self.expand(ctx, s(o, "deckName"));
                let item_id = self.expand(ctx, s(o, "itemId"));
                let item_name = self.expand(ctx, s(o, "itemName"));
                self.toggle_deck_item_state(&deck_id, &deck_name, &item_id, &item_name);
            }

            // Streamer.bot Deck Set Item Title (21003)
            21003 => {
                let deck_id = self.expand(ctx, s(o, "deckId"));
                let deck_name = self.expand(ctx, s(o, "deckName"));
                let item_id = self.expand(ctx, s(o, "itemId"));
                let item_name = self.expand(ctx, s(o, "itemName"));
                let color = self.expand(ctx, s(o, "color"));
                let title = self.expand(ctx, s(o, "title"));
                let state = n(o, "state");
                self.set_deck_item_title(&deck_id, &deck_name, &item_id, &item_name, &color, &title, state);
            }

            // Streamer.bot Deck Set Item Background (21004)
            21004 => {
                let deck_id = self.expand(ctx, s(o, "deckId"));
                let deck_name = self.expand(ctx, s(o, "deckName"));
                let item_id = self.expand(ctx, s(o, "itemId"));
                let item_name = self.expand(ctx, s(o, "itemName"));
                let color = self.expand(ctx, s(o, "color"));
                let url = self.expand(ctx, s(o, "url"));
                let file_id = self.expand(ctx, s(o, "fileId"));
                let state = n(o, "state");
                self.set_deck_item_background(
                    &deck_id, &deck_name, &item_id, &item_name, &color, &url, &file_id, state,
                );
            }

            // Streamer.bot Deck Set Item Icon (21005)
            21005 => {
                let deck_id = self.expand(ctx, s(o, "deckId"));
                let deck_name = self.expand(ctx, s(o, "deckName"));
                let item_id = self.expand(ctx, s(o, "itemId"));
                let item_name = self.expand(ctx, s(o, "itemName"));
                let color = self.expand(ctx, s(o, "color"));
                let name = self.expand(ctx, s(o, "name"));
                let url = self.expand(ctx, s(o, "url"));
                let file_id = self.expand(ctx, s(o, "fileId"));
                let state = n(o, "state");
                self.set_deck_item_icon(
                    &deck_id, &deck_name, &item_id, &item_name, &color, &name, &url, &file_id, state,
                );
            }

            // Streamer.bot Deck Set Item Value (21006)
            21006 => {
                let deck_id = self.expand(ctx, s(o, "deckId"));
                let deck_name = self.expand(ctx, s(o, "deckName"));
                let item_id = self.expand(ctx, s(o, "itemId"));
                let item_name = self.expand(ctx, s(o, "itemName"));
                let value = self.expand(ctx, s(o, "value"));
                let state = n(o, "state");
                self.set_deck_item_value(&deck_id, &deck_name, &item_id, &item_name, &value, state);
            }

            // Streamer.bot Deck Notify (21007)
            21007 => {
                let deck_id = self.expand(ctx, s(o, "deckId"));
                let deck_name = self.expand(ctx, s(o, "deckName"));
                let title = self.expand(ctx, s(o, "title"));
                let description = self.expand(ctx, s(o, "description"));
                let color = self.expand(ctx, s(o, "color"));
                let sound_id = self.expand(ctx, s(o, "soundId"));
                self.deck_notify(&deck_id, &deck_name, &title, &description, &color, &sound_id);
            }

            // Streamer.bot Deck Page Set (21008)
            21008 => {
                let deck_id = self.expand(ctx, s(o, "deckId"));
                let deck_name = self.expand(ctx, s(o, "deckName"));
                let page = n(o, "page") as i64;
                self.set_deck_page(&deck_id, &deck_name, page);
            }

            // Streamer.bot Deck Page Next (21009)
            21009 => {
                let deck_id = self.expand(ctx, s(o, "deckId"));
                let deck_name = self.expand(ctx, s(o, "deckName"));
                self.next_deck_page(&deck_id, &deck_name);
            }

            // Streamer.bot Deck Page Previous (21010)
            21010 => {
                let deck_id = self.expand(ctx, s(o, "deckId"));
                let deck_name = self.expand(ctx, s(o, "deckName"));
                self.prev_deck_page(&deck_id, &deck_name);
            }

            // Write To File (3)
            3 => {
                let file = self.expand(ctx, s(o, "file"));
                let text = self.expand(ctx, s(o, "text"));
                let append = b(o, "append", false);
                let _ = tokio::task::spawn_blocking(move || {
                    crate::fileio::write_to_file(std::path::Path::new(&file), &text, append)
                })
                .await;
            }

            // Read Random Line From File (5)
            5 => {
                let file = self.expand(ctx, s(o, "file"));
                let var = self.simple(ctx, s(o, "variableName"));
                let parse = b(o, "parseVariables", false);
                let auto = b(o, "autoType", false);
                if let Ok(Ok(Some(mut line))) =
                    tokio::task::spawn_blocking(move || crate::fileio::read_random_line(std::path::Path::new(&file)))
                        .await
                {
                    if parse {
                        line = self.expand(ctx, &line);
                    }
                    let val = if auto { auto_type(&line) } else { line.into() };
                    ctx.args.insert(var, val);
                }
            }

            // Read Lines From File (1006)
            1006 => {
                let file = self.expand(ctx, s(o, "file"));
                let var = self.simple(ctx, s(o, "variableName"));
                let parse = b(o, "parseVariables", false);
                let auto = b(o, "autoType", false);
                let read =
                    tokio::task::spawn_blocking(move || crate::fileio::read_lines(std::path::Path::new(&file))).await;
                ctx.args.insert("fileFound".into(), matches!(read, Ok(Ok(_))).into());
                if let Ok(Ok(lines)) = read {
                    // The documented lineCount and line0..N, and under the variable name if one is set.
                    ctx.args.insert("lineCount".into(), lines.len().into());
                    for (i, l) in lines.iter().enumerate() {
                        ctx.args.insert(format!("line{i}"), l.as_str().into());
                    }
                    if var.is_empty() {
                        return;
                    }
                    let processed: Vec<Value> = lines
                        .into_iter()
                        .map(|l| {
                            let l = if parse { self.expand(ctx, &l) } else { l };
                            if auto {
                                auto_type(&l)
                            } else {
                                l.into()
                            }
                        })
                        .collect();
                    ctx.args.insert(format!("{var}.count"), (processed.len() as u64).into());
                    for (i, v) in processed.iter().enumerate() {
                        ctx.args.insert(format!("{var}[{i}]"), v.clone());
                    }
                    ctx.args.insert(var, Value::Array(processed));
                }
            }

            // Read Specific Line From File (1021)
            1021 => {
                let file = self.expand(ctx, s(o, "file"));
                let var = self.simple(ctx, s(o, "variableName"));
                let line_num = n(o, "count") as usize;
                let parse = b(o, "parseVariables", false);
                let auto = b(o, "autoType", false);
                let found = std::path::Path::new(&file).is_file();
                ctx.args.insert("fileFound".into(), found.into());
                let var = if var.is_empty() { "line".to_string() } else { var };
                if let Ok(Ok(Some(mut line))) = tokio::task::spawn_blocking(move || {
                    crate::fileio::read_specific_line(std::path::Path::new(&file), line_num)
                })
                .await
                {
                    if parse {
                        line = self.expand(ctx, &line);
                    }
                    let val = if auto { auto_type(&line) } else { line.into() };
                    ctx.args.insert(var, val);
                }
            }

            // Set File Tail State (1030)
            1030 => {
                let tail_id = self.expand(ctx, s(o, "fileTailId"));
                let state = n(o, "state");
                self.set_file_tail_state(&tail_id, state);
            }

            // File Exists (1039)
            1039 => {
                let file = self.expand(ctx, s(o, "file"));
                let var = match self.simple(ctx, s(o, "variableName")) {
                    v if v.is_empty() => "fileExists".to_string(),
                    v => v,
                };
                let exists =
                    tokio::task::spawn_blocking(move || crate::fileio::file_exists(std::path::Path::new(&file)))
                        .await
                        .unwrap_or(false);
                ctx.args.insert(var, exists.into());
            }

            // Folder Exists (1040)
            1040 => {
                let folder = self.expand(ctx, s(o, "folder"));
                let var = match self.simple(ctx, s(o, "variableName")) {
                    v if v.is_empty() => "folderExists".to_string(),
                    v => v,
                };
                let exists =
                    tokio::task::spawn_blocking(move || crate::fileio::folder_exists(std::path::Path::new(&folder)))
                        .await
                        .unwrap_or(false);
                ctx.args.insert(var, exists.into());
            }

            _ => {}
        }
    }

    /// Fetch URL: the body into the variable, or with Parse as JSON, every value into
    /// `variable.key`, `variable.key.inner`, `variable.list[0]` (null as empty). Nothing on errors.
    async fn fetch(&self, ctx: &mut Ctx, o: &Obj) {
        let url = self.expand(ctx, s(o, "url"));
        let var = self.simple(ctx, s(o, "variableName"));
        let headers: Vec<(String, String)> = o
            .get("headers")
            .and_then(Value::as_object)
            .map(|h| h.iter().map(|(k, v)| (k.clone(), self.expand(ctx, &text(v)))).collect())
            .unwrap_or_default();
        let r = match crate::http::request("GET", &url, &headers, b"").await {
            Ok(r) if (200..300).contains(&r.status) => r,
            Ok(r) => return crate::say(format!("Fetch URL {url}: HTTP {}", r.status)),
            Err(e) => return crate::say(format!("Fetch URL: {e}")),
        };
        let body = r.text();
        let auto = b(o, "autoType", false);
        if !b(o, "parseAsJson", false) {
            ctx.args.insert(var, if auto { auto_type(&body) } else { body.into() });
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(&body) else {
            return crate::say(format!("Fetch URL {url}: not JSON"));
        };
        fn flat(prefix: &str, v: &Value, out: &mut Obj) {
            match v {
                Value::Object(m) => {
                    for (k, x) in m {
                        flat(&format!("{prefix}.{k}"), x, out);
                    }
                }
                Value::Array(a) => {
                    for (i, x) in a.iter().enumerate() {
                        flat(&format!("{prefix}[{i}]"), x, out);
                    }
                }
                Value::Null => {
                    out.insert(prefix.into(), "".into());
                }
                x => {
                    out.insert(prefix.into(), x.clone());
                }
            }
        }
        flat(&var, &v, &mut ctx.args);
    }

    /// Run a Program, extended: the action's arguments go to its stdin as JSON; its output comes
    /// back as output0..outputN (lines, as Streamer.bot) and outputCount, and, our extension, a JSON
    /// object or `key=value` lines on stdout become arguments. Wait for Exit is in seconds; 0 runs
    /// it and moves on.
    async fn run_program(&self, ctx: &mut Ctx, o: &Obj) {
        let prog = self.expand(ctx, s(o, "command"));
        let args = self.expand(ctx, s(o, "arguments"));
        let dir = self.expand(ctx, s(o, "workingDir"));
        let wait = n(o, "waitForExit");
        let mut cmd = tokio::process::Command::new(&prog);
        #[cfg(windows)]
        {
            cmd.raw_arg(&args);
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        #[cfg(not(windows))]
        cmd.args(split_args(&args));
        if !dir.is_empty() {
            cmd.current_dir(&dir);
        } else if let Some(d) = std::path::Path::new(&prog).parent().filter(|d| d.is_dir()) {
            cmd.current_dir(d);
        }
        if let Some(env) = o.get("envVars").and_then(Value::as_object) {
            for (k, v) in env {
                cmd.env(k, self.expand(ctx, &text(v)));
            }
        }
        cmd.stdin(std::process::Stdio::piped()).kill_on_drop(false);
        if wait > 0.0 {
            cmd.stdout(std::process::Stdio::piped());
        } else {
            cmd.stdout(std::process::Stdio::null());
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return crate::say(format!("Run a Program {prog}: {e}")),
        };
        let input = serde_json::to_vec(&ctx.args).unwrap_or_default();
        let stdin = child.stdin.take();
        let feed = async move {
            use tokio::io::AsyncWriteExt;
            if let Some(mut i) = stdin {
                let _ = i.write_all(&input).await;
            }
        };
        if wait <= 0.0 {
            tokio::task::spawn_local(async move {
                feed.await;
                let _ = child.wait().await;
            });
            return;
        }
        feed.await;
        let out = tokio::time::timeout(Duration::from_secs_f64(wait), child.wait_with_output()).await;
        let Ok(Ok(out)) = out else {
            return crate::say(format!("Run a Program {prog}: didn't finish in {wait} s"));
        };
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let lines: Vec<&str> = stdout.split('\n').collect();
        ctx.args.insert("outputCount".into(), lines.len().into());
        for (i, l) in lines.iter().enumerate() {
            ctx.args.insert(format!("output{i}"), (*l).into());
        }
        match serde_json::from_str::<Value>(stdout.trim()) {
            Ok(Value::Object(m)) => ctx.args.extend(m),
            _ => {
                for l in &lines {
                    if let Some((k, v)) = l.trim_end_matches('\r').split_once('=') {
                        let k = k.trim();
                        if !k.is_empty() && !k.contains(char::is_whitespace) {
                            ctx.args.insert(k.into(), vars::auto_type(v.trim()));
                        }
                    }
                }
            }
        }
    }
}

/// Split arguments like a shell: spaces separate, double quotes group, backslash escapes a quote.
#[cfg_attr(windows, allow(dead_code))]
fn split_args(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' if it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::tests::{action, engine, log, run};
    use super::*;
    use serde_json::json;

    #[test]
    fn dates_and_ages() {
        assert_eq!(
            parse_iso("2024-08-17T14:34:56+02:00"),
            parse_iso("2024-08-17T12:34:56Z")
        );
        assert_eq!(
            parse_iso("2024-08-17T09:34:56.5-03:00"),
            parse_iso("2024-08-17T12:34:56Z")
        );
        let t = parse_iso("2024-08-17T12:34:56.123Z").unwrap();
        assert_eq!(t, 1_723_898_096);
        let c = civil(t);
        assert_eq!((c.year, c.month, c.day, c.weekday, c.hour), (2024, 8, 17, 6, 12));
        let later = parse_iso("2026-06-22T10:38:21Z").unwrap();
        let a = age(t, later);
        assert_eq!(a, [1, 10, 4, 22, 3, 25]);
        assert_eq!(age_text(&a).1, "1y, 10m, 4d, 22h, 3m, 25s");
        assert_eq!(
            age(
                parse_iso("2024-01-31T00:00:00Z").unwrap(),
                parse_iso("2024-03-01T00:00:00Z").unwrap()
            ),
            [0, 1, 1, 0, 0, 0]
        );
    }

    #[test]
    fn lists_and_polls() {
        assert_eq!(list(Some(&json!(["a", {"title": "b"}, " "]))), ["a", "b"]);
        assert_eq!(list(Some(&json!("a\nb, c"))), ["a", "b, c"]);
        assert_eq!(list(Some(&json!("a, b"))), ["a", "b"]);
        let mut args = Obj::new();
        poll_args(
            &json!({"id": "p1", "title": "T", "duration": 60, "started_at": "2024-01-01T00:00:00Z",
                "choices": [{"id": "c", "title": "A", "votes": 10, "channel_points_votes": 3}]}),
            &mut args,
        );
        assert_eq!(args["poll.choice0.votes"], 7);
        assert_eq!(args["poll.totalVotes"], 10);
        assert_eq!(
            args["poll.EndsAt"].as_str().unwrap().get(..19),
            Some("2024-01-01T00:01:00")
        );
    }

    #[test]
    fn platform_user_groups() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        let a = action(
            "a1",
            "groups",
            json!([
                {"type": 4005, "userLogin": "UCabc", "groupName": "regulars"},
                log("T add %addResult%"),
                {"type": 4007, "userLogin": "UCabc", "groupName": "Regulars"},
                log("T in %inGroup% %foundUserId% %foundUserType%"),
                {"type": 35021, "userLogin": "123", "groupName": "Regulars"},
                {"type": 35022, "userLogin": "123", "groupName": "Regulars"},
                log("T kick %inGroup%"),
                {"type": 4006, "userLogin": "UCabc", "groupName": "nope"},
                log("T remove %removeResult%")
            ]),
        );
        let e = engine(json!({"actions": [a]}));
        e.cfg.send_modify(|c| {
            c.groups = vec![json!({"id": "g1", "name": "Regulars", "userIds": ["123"]})
                .as_object()
                .unwrap()
                .clone()];
        });
        let out = run(async move {
            e.queue("a1", Obj::new());
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert_eq!(e.cfg.borrow().groups[0]["userIds"], json!(["UCabc"]));
        });
        for w in [
            "T add True",
            "T in True UCabc youtube",
            "T kick False",
            "T remove False",
        ] {
            assert!(out.iter().any(|x| x == w), "{w} missing: {out:?}");
        }
    }

    #[test]
    fn file_read_variables() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("szt-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("l.txt");
        std::fs::write(&f, "alpha\nbeta\n").unwrap();
        let path = f.display().to_string();
        let a = action(
            "a1",
            "files",
            json!([
                {"type": 1006, "file": path, "variableName": ""},
                log("T lines %lineCount% %line1% %fileFound%"),
                {"type": 1021, "file": path, "count": 1, "variableName": ""},
                log("T line %line% %fileFound%"),
                {"type": 1039, "file": path, "variableName": ""},
                {"type": 1040, "folder": dir.display().to_string(), "variableName": ""},
                log("T exists %fileExists% %folderExists%"),
                {"type": 1006, "file": format!("{path}.nope"), "variableName": ""},
                log("T missing %fileFound%")
            ]),
        );
        let e = engine(json!({"actions": [a]}));
        let out = run(async move {
            e.queue("a1", Obj::new());
            tokio::time::sleep(Duration::from_millis(300)).await;
        });
        std::fs::remove_dir_all(&dir).unwrap();
        for w in ["T lines 2 beta True", "T exists True True", "T missing False"] {
            assert!(out.iter().any(|x| x == w), "{w} missing: {out:?}");
        }
        assert!(
            out.iter().any(|x| x.starts_with("T line ") && x.ends_with(" True")),
            "{out:?}"
        );
    }

    #[test]
    fn obs_colors() {
        assert_eq!(obs_color("#FF112233"), 0xFF33_2211);
        assert_eq!(obs_color("112233"), 0xFF33_2211);
        assert_eq!(obs_color("#80FF0000"), 0x8000_00FF);
    }

    #[test]
    fn args_split() {
        assert_eq!(split_args(r#"-a "b c" d\"e """#), ["-a", "b c", "d\"e", ""]);
    }

    #[test]
    fn commands_timers_groups_programs() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        let mut hello = action(
            "a1",
            "hello",
            json!([log(
                "T hello %command% [%rawInput%] %input0% %counter% %userCounter% %user%"
            )]),
        );
        hello["triggers"] = json!([{"commandId": "c1", "id": "t1", "type": 401, "enabled": true}]);
        let mut cool = action("a2", "cool", json!([log("T cooldown %globalCooldownLeft%")]));
        cool["triggers"] = json!([{"commandId": "c1", "id": "t2", "type": 402, "enabled": true}]);
        let mut tick = action("a3", "tick", json!([log("T tick %timerName% %counter%")]));
        tick["triggers"] = json!([{"timerId": "tm", "id": "t3", "type": 701, "enabled": true}]);
        let mut cleared = action("a4", "cleared", json!([log("T cleared %group.name%")]));
        cleared["triggers"] = json!([{"groupId": "", "id": "t4", "type": 32003, "enabled": true}]);
        let tools = action(
            "a5",
            "tools",
            json!([
                {"type": 1013, "groupName": "", "variableName": "list"},
                log("T list %list%"),
                {"type": 9, "commandId": "c1", "state": 1},
                {"type": 1014, "commandId": "c1"},
                log("T state %commandState%"),
                {"type": 9, "commandId": "c1", "state": 2},
                {"type": 10001, "groupId": "g1"},
                {"type": 6, "command": "sh", "arguments": "-c \"cat >/dev/null; echo first; echo a=1\"", "waitForExit": 5},
                log("T run %outputCount% %output0% %a%")
            ]),
        );
        let e = engine(json!({"actions": [hello, cool, tick, cleared, tools]}));
        e.cfg.send_modify(|c| {
            c.commands = vec![
                json!({"id": "c1", "name": "hi", "enabled": true, "include": true, "mode": 0,
                "command": "!hi\r\n!hey", "location": 0, "sources": 1, "globalCooldown": 100, "userCooldown": 0,
                "group": "", "grantType": 0, "permittedUsers": [], "permittedGroups": []})
                .as_object()
                .unwrap()
                .clone(),
            ];
            c.timers = vec![
                json!({"id": "tm", "name": "tock", "enabled": true, "repeat": false, "interval": 1})
                    .as_object()
                    .unwrap()
                    .clone(),
            ];
            c.groups = vec![json!({"id": "g1", "name": "Regulars", "bots": false, "userIds": ["1"]})
                .as_object()
                .unwrap()
                .clone()];
        });
        let out = run(async move {
            let viewer = |text: &str| Chat {
                platform: "twitch".into(),
                source: 1,
                user: "ana".into(),
                user_id: "1".into(),
                text: text.into(),
                args: json!({"user": "ana"}).as_object().unwrap().clone(),
                ..Default::default()
            };
            assert_eq!(e.chat(&viewer("!hi there friend")), 1);
            assert_eq!(e.chat(&viewer("!hey")), 0); // global cooldown
            assert_eq!(
                e.chat(&Chat {
                    broadcaster: true,
                    ..viewer("!HEY")
                }),
                1
            );
            assert_eq!(e.chat(&viewer("!hiya")), 0);
            e.timers();
            e.queue("a5", Obj::new());
            tokio::time::sleep(Duration::from_millis(1300)).await;
            assert!(e.cfg.borrow().groups[0]["userIds"].as_array().unwrap().is_empty());
            assert!(!b(&e.cfg.borrow().timers[0], "enabled", true));
        });
        let want = [
            "T hello !hi [there friend] there 1 1 ana",
            "T cooldown 100",
            "T hello !hey []  2 2 ana",
            "T list !hey, !hi",
            "T state False",
            "T cleared Regulars",
            "T run 3 first 1",
            "T tick tock 1",
        ];
        for w in want {
            assert!(out.iter().any(|x| x == w), "{w} missing: {out:?}");
        }
    }

    #[test]
    fn per_platform_commands_dev_outbox() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        let mut reply = action(
            "a1",
            "reply",
            json!([{"type": 35001, "text": "hi %user%"}, {"type": 43001, "text": "tt %user%"}]),
        );
        reply["triggers"] = json!([{"commandId": "c1", "id": "t1", "type": 401, "enabled": true}]);
        let e = engine(json!({"actions": [reply]}));
        e.st.borrow_mut().dev_out = Some(Vec::new());
        e.cfg.send_modify(|c| {
            c.commands = vec![
                json!({"id": "c1", "name": "hi", "enabled": true, "mode": 0, "command": "!hi",
                "location": 0, "sources": 2_097_152 | 4_194_304})
                .as_object()
                .unwrap()
                .clone(),
            ];
        });
        run(async move {
            assert_eq!(e.inject_chat("twitch", "ana", "viewer", "!hi", None).unwrap(), 0);
            assert_eq!(e.inject_chat("kick", "bo", "viewer", "!hi", None).unwrap(), 1);
            assert!(e.inject_chat("nope", "bo", "viewer", "!hi", None).is_err());
            tokio::time::sleep(Duration::from_millis(100)).await;
            let out = e.dev_take().unwrap();
            assert_eq!(out[0], json!({"platform": "kick", "text": "hi bo", "bot": false}));
            assert_eq!(out[1], json!({"platform": "tiktok", "text": "tt bo", "bot": false}));
            assert!(e.dev_take().unwrap().is_empty());
        });
    }

    #[test]
    fn templates_run() {
        let _g = crate::TEST_LOCK.lock().unwrap();
        let e = engine(json!({"actions": []}));
        e.st.borrow_mut().dev_out = Some(Vec::new());
        e.cfg.send_modify(|c| {
            for t in crate::templates::TEMPLATES {
                let m = (t.make)();
                c.actions.actions.extend(m.actions);
                c.commands.extend(m.commands);
                c.timers.extend(m.timers);
            }
        });
        run(async move {
            let args = |v: Value| v.as_object().cloned().unwrap();
            e.fire(101, "Follow", "Twitch", "twitch", &args(json!({"user": "ana"})), |_| {
                true
            });
            e.fire(
                41002,
                "Gift",
                "TikTok",
                "tiktok",
                &args(json!({"user": "bo", "giftName": "Rose", "repeatCount": 5})),
                |_| true,
            );
            assert_eq!(e.inject_chat("kick", "cy", "viewer", "!lurk", None).unwrap(), 1);
            tokio::time::sleep(Duration::from_millis(200)).await;
            let out = e.dev_take().unwrap();
            let said = |p: &str, t: &str| out.iter().any(|o| o["platform"] == p && o["text"] == t);
            assert!(said("twitch", "Thanks for the follow, ana! 💜"), "{out:?}");
            assert!(said("tiktok", "Thanks for the 5x Rose, bo! 🌹"), "{out:?}");
            assert!(said("kick", "cy is now lurking. Enjoy the stream! 👀"), "{out:?}");
        });
    }
}
