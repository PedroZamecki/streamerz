//! Platform tools: what Streamer.bot only offers as C# methods or not at all, and the sub-actions that
//! act on every platform at once (PLAN.md, Version 1.9.0). Streamerz-only codes: 99450-99499 TikTok,
//! 99500-99599 Twitch, 99600-99649 Kick, 99650-99699 YouTube, 99720-99749 every platform.

use super::{b, n, new_state, s, Ctx, Engine};
use crate::model::Obj;
use serde_json::{json, Value};
use std::rc::Rc;

thread_local! {
    /// What AutoMod holds now (see `Engine::note_held`).
    static HELD: std::cell::RefCell<Vec<Value>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The platforms a sub-action can reach, in the order they're tried.
pub const PLATFORMS: [&str; 4] = ["twitch", "youtube", "kick", "tiktok"];

/// The Helix query that says whose channel and who moderates it.
const MODERATES: [(&str, &str); 2] = [
    ("broadcaster_id", "{broadcaster_id}"),
    ("moderator_id", "{moderator_id}"),
];

/// Twitch permissions a few tools need beyond the login's own, asked only when the user reconnects
/// for them (Accounts, "More permissions available").
pub const TWITCH_EXTRA_SCOPES: &[&str] = &[
    "moderator:manage:blocked_terms",
    "moderator:manage:automod_settings",
    "user:manage:chat_color",
    "user:manage:blocked_users",
    "channel:manage:schedule",
    "channel:manage:clips",
];

/// The extra Twitch permission a sub-action needs, if any.
pub fn twitch_extra_scope(kind: u64) -> Option<&'static str> {
    Some(match kind {
        583 | 584 => "moderator:manage:blocked_terms",
        592 => "moderator:manage:automod_settings",
        99533 => "user:manage:chat_color",
        99534 | 99535 => "user:manage:blocked_users",
        99536..=99539 => "channel:manage:schedule",
        99545 => "channel:manage:clips",
        _ => return None,
    })
}

/// Content classification labels a channel can set itself (Twitch sets Mature-rated game on its own).
pub const CONTENT_LABELS: [&str; 6] = [
    "DebatedSocialIssuesAndPolitics",
    "DrugsIntoxication",
    "SexualThemes",
    "ViolentGraphic",
    "Gambling",
    "ProfanityVulgarity",
];

/// A YouTube playlist's id from the id itself or a link to it (`...?list=PL...`).
fn playlist_id(text: &str) -> String {
    let text = text.trim();
    match text.split_once("list=") {
        Some((_, rest)) => rest.split(['&', '#']).next().unwrap_or_default().to_string(),
        None => text.to_string(),
    }
}

/// A user as typed or filled in: no `@`, no spaces around.
fn login(text: &str) -> String {
    text.trim().trim_start_matches('@').trim().to_string()
}

/// Seconds as `h:mm:ss`, or `m:ss` under an hour.
pub fn clock(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// How long since an ISO 8601 time, in seconds (0 when it can't be read or is ahead).
fn since(iso: &str) -> u64 {
    let now = crate::fmt::unix_now().as_secs() as i64;
    super::rest_parse_iso(iso).map_or(0, |t| (now - t).max(0) as u64)
}

/// Unix seconds as an ISO 8601 UTC time (`2026-11-15T20:00:00Z`).
fn iso_utc(secs: i64) -> String {
    let t = super::civil_time(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        t.year, t.month, t.day, t.hour, t.min, t.sec
    )
}

/// A date and time as typed: ISO 8601 with its zone, or `2026-11-15 20:00` in local time.
fn parse_when(text: &str) -> Option<i64> {
    let mut t = text.trim().replacen(' ', "T", 1);
    if t.len() == 16 {
        t.push_str(":00");
    }
    let zoned = t.get(19..).is_some_and(|z| z.contains(['Z', 'z', '+', '-']));
    let secs = super::rest_parse_iso(&t)?;
    Some(if zoned {
        secs
    } else {
        secs - i64::from(crate::fmt::now().offset)
    })
}

/// The computer's time zone by its IANA name (Twitch's schedule wants one), else UTC.
fn local_zone() -> String {
    #[cfg(unix)]
    {
        let named = std::env::var("TZ")
            .ok()
            .filter(|z| !z.is_empty() && !z.starts_with(':') && !z.starts_with('/'))
            .or_else(|| {
                let link = std::fs::read_link("/etc/localtime").ok()?;
                let link = link.to_string_lossy();
                link.split_once("zoneinfo/").map(|(_, zone)| zone.to_string())
            });
        if let Some(zone) = named {
            return zone;
        }
    }
    "UTC".into()
}

/// A colour as `#RRGGBB` (Streamer.bot's pickers write `#AARRGGBB`); empty when it isn't one.
fn rgb(text: &str) -> String {
    let hex = text.trim().trim_start_matches('#');
    if !hex.bytes().all(|c| c.is_ascii_hexdigit()) || !matches!(hex.len(), 6 | 8) {
        return String::new();
    }
    format!("#{}", hex[hex.len() - 6..].to_uppercase())
}

impl Engine {
    /// A Helix call; the error is said plainly, ready for the log.
    async fn helix_call(
        &self,
        method: &'static str,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
        bot: bool,
    ) -> Result<Value, String> {
        let tw = self.twitch().ok_or("Twitch isn't connected")?;
        tw.helix(method, path, query.to_vec(), body, bot, true)
            .await
            .map_err(|e| crate::twitch::plain_error(&e))
    }

    /// A Helix call on the channel as its moderator, with more query after the ids.
    async fn helix_mod(
        &self,
        method: &'static str,
        path: &str,
        more: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut query = MODERATES.to_vec();
        query.extend_from_slice(more);
        self.helix_call(method, path, &query, body, false).await
    }

    /// The Twitch id of a login (or of an id typed as one).
    async fn twitch_id(&self, who: &str) -> Result<String, String> {
        let who = login(who);
        if who.is_empty() {
            return Err("no user given".into());
        }
        let tw = self.twitch().ok_or("Twitch isn't connected")?;
        tw.resolve_user_id(&who)
            .await
            .map_err(|_| format!("no Twitch user {who}"))
    }

    /// Whether the Twitch login was given `scope`.
    pub fn twitch_has_scope(&self, scope: &str) -> bool {
        self.cfg
            .borrow()
            .twitch
            .broadcaster
            .as_ref()
            .is_some_and(|a| a.token.scope.iter().any(|s| s == scope))
    }

    /// The id of a person seen in `platform`'s chat by name (YouTube and TikTok have no lookup by name).
    pub(super) fn seen_id(&self, platform: &str, who: &str) -> Option<String> {
        let who = login(who);
        self.chatters(platform)
            .into_iter()
            .find(|(name, id)| name.trim_start_matches('@').eq_ignore_ascii_case(&who) || *id == who)
            .map(|(_, id)| id)
    }

    /// A YouTube channel id for whoever `who` names: an id as it is, the person who started the action,
    /// or someone the chat has seen by that name.
    pub(super) fn youtube_channel_id(&self, ctx: &Ctx, who: &str) -> String {
        let who = login(who);
        if who.len() == 24 && who.starts_with("UC") {
            return who;
        }
        let arg = |k: &str| ctx.args.get(k).map(crate::vars::text).unwrap_or_default();
        if arg("userType") == "youtube"
            && (arg("userName").eq_ignore_ascii_case(&who) || arg("user").eq_ignore_ascii_case(&who))
        {
            return arg("userId");
        }
        self.seen_id("youtube", &who).unwrap_or(who)
    }

    /// Puts `items` (each an object of values) as `{prefix}Count` and `{prefix}0Name`... in the arguments.
    fn put_list(ctx: &mut Ctx, prefix: &str, items: &[Vec<(&str, Value)>]) {
        ctx.args.insert(format!("{prefix}Count"), items.len().into());
        for (i, item) in items.iter().enumerate() {
            for (key, value) in item {
                ctx.args.insert(format!("{prefix}{i}{key}"), value.clone());
            }
        }
    }

    /// Twitch's AutoMod held a message (463), or one held changed (464): the chat shows those waiting,
    /// with Allow and Deny. The newest 20 are kept, for this session only.
    pub(crate) fn note_held(&self, kind: u32, args: &Obj) {
        let get = |k: &str| args.get(k).map(crate::vars::text).unwrap_or_default();
        let id = get("messageId");
        if id.is_empty() {
            return;
        }
        HELD.with_borrow_mut(|held| {
            held.retain(|h| h["msgId"] != id.as_str());
            if kind == crate::twitch::T_AUTOMOD_MESSAGE_HELD {
                held.push(
                    json!({"msgId": id, "user": get("user"), "userId": get("userId"), "text": get("rawInput"),
                    "reason": get("category"), "level": get("level")}),
                );
                if held.len() > 20 {
                    held.remove(0);
                }
            }
        });
        crate::host::chat();
    }

    /// The messages AutoMod holds now, oldest first.
    pub fn held(&self) -> Vec<Value> {
        HELD.with_borrow(Clone::clone)
    }

    /// Lets a held message through, or keeps it out, and takes it off the list.
    pub fn decide_held(self: &Rc<Self>, id: &str, allow: bool) {
        HELD.with_borrow_mut(|held| held.retain(|h| h["msgId"] != id));
        crate::host::chat();
        let what = format!("/{} {id}", if allow { "allow" } else { "deny" });
        if self.dev_out("twitch", &what, false) {
            self.chat_note("twitch", format!("(dev, not sent) {what}"));
            return;
        }
        let settings = json!({"messageId": id, "action": if allow { 0 } else { 1 }})
            .as_object()
            .cloned()
            .unwrap_or_default();
        let engine = self.clone();
        tokio::task::spawn_local(async move {
            let (_, outcome) = engine.execute_one(582, settings).await;
            if let Some(error) = outcome.error {
                engine.chat_note("twitch", format!("{what}: {error}"));
            }
        });
    }

    /// A reward redemption (Twitch 112, Kick 35024) counts: the trigger's arguments with `counter` and
    /// `userCounter` after it. None for anything else.
    pub(super) fn count_redemption(self: &Rc<Self>, kind: u32, args: &Obj) -> Option<Obj> {
        let platform = match kind {
            112 => "twitch",
            35024 => "kick",
            _ => return None,
        };
        let get = |k: &str| args.get(k).map(crate::vars::text).unwrap_or_default();
        let reward = get("rewardId");
        if reward.is_empty() {
            return None;
        }
        let key = crate::rewards::key(platform, &reward);
        let mut counts = (0, 0);
        self.change(|c| counts = c.rewards.redeemed(&key, &get("userId")));
        let mut out = args.clone();
        out.insert("counter".into(), counts.0.into());
        out.insert("userCounter".into(), counts.1.into());
        Some(out)
    }

    /// The reward sub-actions that work on Streamerz's counters and groups (Streamer.bot's own):
    /// Configure Rewards, Reset Counter, Reset User Counters, Set Group Enabled and Paused State.
    async fn reward_tool(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64, platform: &str) -> Result<(), String> {
        let catalog = if platform == "twitch" {
            "twitch.rewards"
        } else {
            "kick.rewards"
        };
        match kind {
            // Reset Reward Counter / Reset Reward User Counters
            209 | 210 | 35109 | 35110 => {
                let id = crate::catalog::resolve(catalog, &self.expand(ctx, s(o, "rewardId")));
                let key = crate::rewards::key(platform, &id);
                let users = matches!(kind, 210 | 35110);
                self.change(|c| c.rewards.reset(&key, users));
            }
            // Set Group Enabled State / Set Group Paused State: each reward of the group.
            211 | 212 | 35111 | 35112 => {
                let group = self.simple(ctx, s(o, "groupName"));
                let ids = self.cfg.borrow().rewards.in_group(platform, &group);
                if ids.is_empty() {
                    return Err(format!("no {platform} rewards in the group {group}"));
                }
                let key = if matches!(kind, 211 | 35111) {
                    "is_enabled"
                } else {
                    "is_paused"
                };
                let now = self.reward_states(platform, &ids).await?;
                // Kick's state is on or off (true: on), Twitch's a choice (0 on, 1 off, 2 toggle).
                let state = super::rest::state_of(o);
                let changes: Vec<(String, Value)> = ids
                    .iter()
                    .map(|id| {
                        let on = now.get(id).and_then(|r| r[key].as_bool()).unwrap_or(false);
                        (id.clone(), json!({ key: new_state(state, on) }))
                    })
                    .collect();
                self.update_rewards(platform, changes).await?;
            }
            // Configure Rewards: those listed on, off or paused; the rest off when asked.
            _ => {
                let list = |key: &str| -> Vec<String> {
                    o.get(key)
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|v| crate::catalog::resolve(catalog, &self.expand(ctx, &crate::vars::text(v))))
                        .filter(|id| !id.is_empty())
                        .collect()
                };
                let (on, off, paused) = (list("enable"), list("disable"), list("pause"));
                let rest = b(o, "disableNotConfigured", false);
                let mine = self.reward_states(platform, &[]).await?;
                let changes: Vec<(String, Value)> = mine
                    .keys()
                    .filter_map(|id| {
                        let body = if on.contains(id) {
                            json!({"is_enabled": true, "is_paused": false})
                        } else if paused.contains(id) {
                            json!({"is_enabled": true, "is_paused": true})
                        } else if off.contains(id) || rest {
                            json!({"is_enabled": false})
                        } else {
                            return None;
                        };
                        Some((id.clone(), body))
                    })
                    .collect();
                self.update_rewards(platform, changes).await?;
            }
        }
        Ok(())
    }

    /// The rewards Streamerz may change on `platform` (an app changes only its own), by id; only those
    /// in `ids` when it isn't empty.
    async fn reward_states(
        self: &Rc<Self>,
        platform: &str,
        ids: &[String],
    ) -> Result<std::collections::BTreeMap<String, Value>, String> {
        let list = if platform == "twitch" {
            let mut q = vec![
                ("broadcaster_id", "{broadcaster_id}"),
                ("only_manageable_rewards", "true"),
            ];
            q.extend(ids.iter().map(|id| ("id", id.as_str())));
            self.helix_call("GET", "channel_points/custom_rewards", &q, None, false)
                .await?
        } else {
            self.kick_start().api("GET", "/channels/rewards", None).await?
        };
        Ok(list["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| Some((r["id"].as_str()?.to_string(), r.clone())))
            .filter(|(id, _)| ids.is_empty() || ids.contains(id))
            .collect())
    }

    /// Changes each reward; what failed is said together.
    async fn update_rewards(self: &Rc<Self>, platform: &str, changes: Vec<(String, Value)>) -> Result<(), String> {
        let mut failed = Vec::new();
        for (id, body) in changes {
            let r = if platform == "twitch" {
                let q = [("broadcaster_id", "{broadcaster_id}"), ("id", id.as_str())];
                self.helix_call("PATCH", "channel_points/custom_rewards", &q, Some(body), false)
                    .await
            } else {
                let path = format!("/channels/rewards/{}", crate::commands::url_encode(&id));
                self.kick_start().api("PATCH", &path, Some(body)).await
            };
            if let Err(e) = r {
                failed.push(format!("{id}: {e}"));
            }
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(failed.join("; "))
        }
    }

    /// The Countdown sub-action (99803): reset or add (what the templates use), and subtract, set,
    /// pause, resume, toggle and get. Gives `countdownRemainingMs`, `countdownRunning`, `countdownMaxMs`
    /// and `countdownLeft` (`1:30`) after any of them.
    pub(super) fn countdown(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj) {
        use crate::templates::clock;
        let id = self.expand(ctx, s(o, "clockId"));
        let expected = self.expand(ctx, s(o, "expectedCurrency"));
        let actual = self.expand(ctx, "%donationCurrency%");
        if !expected.is_empty() && !expected.eq_ignore_ascii_case(actual.trim()) {
            return;
        }
        let now = clock::now_ms();
        let ms = |key: &str| clock::field_ms(o.get(key), |t| self.expand(ctx, t));
        let operation = self.expand(ctx, s(o, "operation")).trim().to_lowercase();
        let units = crate::fmt::parse_number(&self.expand(ctx, s(o, "units"))).unwrap_or(-1.0);
        let amount = ((ms("perUnitMs").unwrap_or(0) as f64) * units).round() as u64;
        if matches!(operation.as_str(), "add" | "subtract") && (!units.is_finite() || units < 0.0) {
            return crate::warn("Countdown: invalid amount".to_string());
        }
        let known = ["reset", "add", "subtract", "set", "pause", "resume", "toggle", "get"];
        if !known.contains(&operation.as_str()) {
            return crate::warn("Countdown: invalid operation".to_string());
        }
        let running = clock::get(&self.config(), &id, now).is_some_and(|s| s.running);
        // Read the fields first: filling in variables reads the settings, which a change holds.
        let (start, max) = (ms("startMs"), ms("maxMs"));
        let mut changed = false;
        self.change(|config| {
            changed = match operation.as_str() {
                "reset" => clock::reset(config, &id, start.unwrap_or(0), max.unwrap_or(0), now),
                // Add keeps the cap the template gave it.
                "add" => clock::set_max(config, &id, max.unwrap_or(0), now) && clock::add(config, &id, amount, now),
                "subtract" => clock::subtract(config, &id, amount, now),
                "set" => {
                    max.is_none_or(|max| max == 0 || clock::set_max(config, &id, max, now))
                        && clock::set_remaining(config, &id, start.unwrap_or(0), now)
                }
                "pause" => clock::set_running(config, &id, false, now),
                "resume" => clock::set_running(config, &id, true, now),
                "toggle" => clock::set_running(config, &id, !running, now),
                _ => clock::get(config, &id, now).is_some(),
            };
        });
        if !changed {
            return crate::warn("Countdown: timer not found".to_string());
        }
        if operation != "get" {
            self.countdown_changed(&id);
        }
        if let Some(now) = clock::get(&self.config(), &id, clock::now_ms()) {
            ctx.args.insert("countdownRemainingMs".into(), now.remaining_ms.into());
            ctx.args.insert("countdownMaxMs".into(), now.max_ms.into());
            ctx.args.insert("countdownRunning".into(), now.running.into());
            ctx.args
                .insert("countdownLeft".into(), self::clock(now.remaining_ms / 1000).into());
        }
    }

    /// Keeps who subscribed last on Twitch (Sub 103, Resub 104, Gift Sub 105: the one who got it).
    pub(super) fn note_subscriber(self: &Rc<Self>, kind: u32, args: &Obj) {
        let get = |k: &str| args.get(k).map(crate::vars::text).unwrap_or_default();
        let (user, login, id) = if kind == 105 {
            (get("recipientUser"), get("recipientUserName"), get("recipientId"))
        } else {
            (get("user"), get("userName"), get("userId"))
        };
        if id.is_empty()
            || args
                .get("userType")
                .map(crate::vars::text)
                .is_some_and(|p| p != "twitch")
        {
            return;
        }
        let tier = get("tier").to_lowercase();
        let tier = match tier.as_str() {
            "prime" => "Prime".to_string(),
            t => format!("Tier{}", t.trim_start_matches("tier").trim()),
        };
        let latest = json!({"userName": user, "userLogin": login, "userId": id, "tier": tier, "isGift": kind == 105,
            "at": iso_utc(crate::fmt::unix_now().as_secs() as i64)});
        self.change(|c| c.twitch.latest_subscriber = Some(latest));
    }

    /// Runs a Streamerz platform tool; false when `kind` isn't one.
    pub(super) async fn tools(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) -> bool {
        let r = match kind {
            203 | 209..=212 => self.reward_tool(ctx, o, kind, "twitch").await,
            35104 | 35109..=35112 => self.reward_tool(ctx, o, kind, "kick").await,
            99450..=99454 => self.tiktok_tool(ctx, o, kind).await,
            99500..=99599 => self.twitch_tool(ctx, o, kind).await,
            520..=531 => self.guest_star(ctx, o, kind).await,
            99600..=99649 => self.kick_tool(ctx, o, kind).await,
            99650..=99699 => self.youtube_tool(ctx, o, kind).await,
            99740..=99749 => self.discord_tool(ctx, o, kind).await,
            99750..=99779 => self.obs_tool(ctx, o, kind).await,
            99720..=99733 => {
                self.every_platform(ctx, o, kind).await;
                Ok(())
            }
            _ => return false,
        };
        if let Err(e) = r {
            let platform = match kind {
                99740..=99749 => "Discord",
                99750..=99779 => "OBS",
                99500..=99599 | 203 | 209..=212 | 520..=531 => "Twitch",
                99600..=99649 | 35104 | 35109..=35112 => "Kick",
                99650..=99699 => "YouTube",
                99450..=99499 => "TikTok",
                _ => "Streamerz",
            };
            crate::error(format!("{platform}: {e}"));
        }
        true
    }

    async fn twitch_tool(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) -> Result<(), String> {
        if let Some(scope) = twitch_extra_scope(kind) {
            if !self.twitch_has_scope(scope) {
                return Err(crate::i18n::tr("twitch.error.more_permissions").into());
            }
        }
        let text = |key: &str| self.expand(ctx, s(o, key));
        match kind {
            // Purge User: a 1 second timeout takes their messages away.
            99501 => {
                let (who, reason) = (text("userLogin"), text("reason"));
                ctx.args.insert("purgeResult".into(), false.into());
                let uid = self.twitch_id(&who).await?;
                if self.dev_out("twitch", &format!("/timeout {who} 1"), false) {
                    ctx.args.insert("purgeResult".into(), true.into());
                    return Ok(());
                }
                let body = json!({"data": {"user_id": uid, "duration": 1, "reason": reason}});
                self.helix_mod("POST", "moderation/bans", &[], Some(body)).await?;
                ctx.args.insert("purgeResult".into(), true.into());
                ctx.args.insert("purgedUserId".into(), uid.into());
                ctx.args.insert("purgedUserName".into(), login(&who).into());
            }
            // Send and Pin Message
            99502 => {
                let message = text("text");
                let duration = self.duration_number(ctx, o, "duration").max(0.0) as u64;
                let (bot, fallback) = (b(o, "useBot", false), b(o, "fallback", true));
                ctx.args.insert("pinResult".into(), false.into());
                if self.dev_out("twitch", &format!("/pin {message}"), bot) {
                    ctx.args.insert("pinResult".into(), true.into());
                    return Ok(());
                }
                let body =
                    json!({"broadcaster_id": "{broadcaster_id}", "sender_id": "{sender_id}", "message": message});
                let tw = self.twitch().ok_or("Twitch isn't connected")?;
                let sent = tw
                    .helix("POST", "chat/messages", vec![], Some(body), bot, fallback)
                    .await
                    .map_err(|e| crate::twitch::plain_error(&e))?;
                let id = sent["data"][0]["message_id"].as_str().unwrap_or_default().to_string();
                if id.is_empty() {
                    let why = sent["data"][0]["drop_reason"]["message"].as_str().unwrap_or("not sent");
                    return Err(why.to_string());
                }
                ctx.args.insert("messageId".into(), id.as_str().into());
                let secs = duration.clamp(30, 1800).to_string();
                let mut q = vec![("message_id", id.as_str())];
                if duration > 0 {
                    q.push(("duration_seconds", secs.as_str()));
                }
                self.helix_mod("PUT", "chat/pins", &q, None).await?;
                ctx.args.insert("pinResult".into(), true.into());
            }
            // Send Whisper
            99503 => {
                let (who, message, bot) = (text("userLogin"), text("text"), b(o, "useBot", false));
                ctx.args.insert("whisperResult".into(), false.into());
                let uid = self.twitch_id(&who).await?;
                if self.dev_out("twitch", &format!("/w {who} {message}"), bot) {
                    ctx.args.insert("whisperResult".into(), true.into());
                    return Ok(());
                }
                let q = [("from_user_id", "{sender_id}"), ("to_user_id", uid.as_str())];
                self.helix_call("POST", "whispers", &q, Some(json!({"message": message})), bot)
                    .await?;
                ctx.args.insert("whisperResult".into(), true.into());
            }
            // Is Subscribed
            99513 => {
                let uid = self.twitch_id(&text("userLogin")).await?;
                let q = [("broadcaster_id", "{broadcaster_id}"), ("user_id", uid.as_str())];
                let got = self.helix_call("GET", "subscriptions", &q, None, false).await?;
                let sub = &got["data"][0];
                let tier = sub["tier"].as_str().and_then(|t| t.parse::<u64>().ok()).unwrap_or(0) / 1000;
                ctx.args.insert("isSubscribed".into(), (!sub.is_null()).into());
                ctx.args.insert("subscriptionTier".into(), tier.into());
                ctx.args
                    .insert("isGift".into(), sub["is_gift"].as_bool().unwrap_or(false).into());
                ctx.args.insert("gifterUser".into(), sub["gifter_name"].clone());
            }
            // Get Clips for Target: random, latest or most viewed of the last `days`.
            99514 => {
                let uid = self.twitch_id(&text("userLogin")).await?;
                let days = (n(o, "days") as i64).clamp(0, 3650);
                let started = (days > 0).then(|| {
                    let at = crate::fmt::unix_now().as_secs() as i64 - days * 86_400;
                    iso_utc(at)
                });
                let mut q = vec![("broadcaster_id", uid.as_str()), ("first", "100")];
                if let Some(at) = &started {
                    q.push(("started_at", at.as_str()));
                }
                if b(o, "featured", false) {
                    q.push(("is_featured", "true"));
                }
                let got = self.helix_call("GET", "clips", &q, None, false).await?;
                let clips: Vec<&Value> = got["data"].as_array().into_iter().flatten().collect();
                ctx.args.insert("clipCount".into(), clips.len().into());
                ctx.args.insert("clipFound".into(), (!clips.is_empty()).into());
                let chosen = match n(o, "pick") as u64 {
                    1 => clips
                        .iter()
                        .max_by_key(|c| c["created_at"].as_str().unwrap_or_default().to_string()),
                    2 => clips.iter().max_by_key(|c| c["view_count"].as_u64().unwrap_or(0)),
                    _ if clips.is_empty() => None,
                    _ => clips.get(crate::math::secure_u64() as usize % clips.len()),
                };
                if let Some(c) = chosen {
                    for (arg, key) in [
                        ("clipId", "id"),
                        ("clipUrl", "url"),
                        ("clipEmbedUrl", "embed_url"),
                        ("clipTitle", "title"),
                        ("clipViewCount", "view_count"),
                        ("clipCreatedAt", "created_at"),
                        ("clipDuration", "duration"),
                        ("clipCreatorName", "creator_name"),
                        ("clipGameId", "game_id"),
                        ("clipThumbnailUrl", "thumbnail_url"),
                    ] {
                        ctx.args.insert(arg.into(), c[key].clone());
                    }
                }
            }
            // Get Stream Info: ours, or the channel named.
            99515 => {
                let who = login(&text("userLogin"));
                let q = if who.is_empty() {
                    vec![("user_id", "{broadcaster_id}")]
                } else {
                    vec![("user_login", who.as_str())]
                };
                let got = self.helix_call("GET", "streams", &q, None, false).await?;
                let live = &got["data"][0];
                self.put_stream(
                    ctx,
                    "twitch",
                    !live.is_null(),
                    live["title"].clone(),
                    live["game_name"].clone(),
                    live["viewer_count"].clone(),
                    live["started_at"].as_str().unwrap_or_default(),
                );
                ctx.args.insert("gameId".into(), live["game_id"].clone());
                ctx.args.insert("language".into(), live["language"].clone());
                ctx.args.insert("thumbnailUrl".into(), live["thumbnail_url"].clone());
            }
            // Get Hype Train
            99516 => {
                let got = self
                    .helix_call(
                        "GET",
                        "hypetrain/status",
                        &[("broadcaster_id", "{broadcaster_id}")],
                        None,
                        false,
                    )
                    .await?;
                let now = &got["data"][0]["current"];
                ctx.args.insert("hypeTrainActive".into(), (!now.is_null()).into());
                for (arg, key) in [
                    ("hypeTrainLevel", "level"),
                    ("hypeTrainProgress", "progress"),
                    ("hypeTrainGoal", "goal"),
                    ("hypeTrainTotal", "total"),
                    ("hypeTrainStartedAt", "started_at"),
                    ("hypeTrainEndsAt", "expires_at"),
                ] {
                    ctx.args.insert(arg.into(), now[key].clone());
                }
                let best = &got["data"][0]["all_time_high"];
                ctx.args.insert("hypeTrainRecordLevel".into(), best["level"].clone());
                ctx.args.insert("hypeTrainRecordTotal".into(), best["total"].clone());
            }
            // Get Creator Goals
            99517 => {
                let got = self
                    .helix_call("GET", "goals", &[("broadcaster_id", "{broadcaster_id}")], None, false)
                    .await?;
                let items: Vec<Vec<(&str, Value)>> = got["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|g| {
                        vec![
                            ("Type", g["type"].clone()),
                            ("Description", g["description"].clone()),
                            ("Current", g["current_amount"].clone()),
                            ("Target", g["target_amount"].clone()),
                        ]
                    })
                    .collect();
                Self::put_list(ctx, "goal", &items);
            }
            // Get Bits Leaderboard
            99518 => {
                let period = ["all", "day", "week", "month", "year"]
                    .get(n(o, "period") as usize)
                    .copied()
                    .unwrap_or("all");
                let count = (n(o, "count") as u64).clamp(1, 100).to_string();
                let got = self
                    .helix_call(
                        "GET",
                        "bits/leaderboard",
                        &[("count", count.as_str()), ("period", period)],
                        None,
                        false,
                    )
                    .await?;
                let items: Vec<Vec<(&str, Value)>> = got["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|u| {
                        vec![
                            ("User", u["user_name"].clone()),
                            ("UserName", u["user_login"].clone()),
                            ("UserId", u["user_id"].clone()),
                            ("Bits", u["score"].clone()),
                            ("Rank", u["rank"].clone()),
                        ]
                    })
                    .collect();
                Self::put_list(ctx, "bitsLeader", &items);
            }
            // Set Content Classification: the ticked labels on, the rest off.
            99519 => {
                let on: Vec<String> = o
                    .get("labels")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect();
                let labels: Vec<Value> = CONTENT_LABELS
                    .iter()
                    .map(|id| json!({"id": id, "is_enabled": on.iter().any(|x| x == id)}))
                    .collect();
                let body = json!({"content_classification_labels": labels});
                self.helix_call(
                    "PATCH",
                    "channels",
                    &[("broadcaster_id", "{broadcaster_id}")],
                    Some(body),
                    false,
                )
                .await?;
            }
            // Set Stream Language / Set Stream Delay
            99520 | 99521 => {
                let body = if kind == 99520 {
                    let language = text("language").trim().to_lowercase();
                    if language.is_empty() {
                        return Err("no language given".into());
                    }
                    json!({"broadcaster_language": language})
                } else {
                    json!({"delay": (self.duration_number(ctx, o, "delay").max(0.0) as u64).min(900)})
                };
                self.helix_call(
                    "PATCH",
                    "channels",
                    &[("broadcaster_id", "{broadcaster_id}")],
                    Some(body),
                    false,
                )
                .await?;
            }
            // Create Reward
            99522 => {
                let title = text("title");
                if title.trim().is_empty() {
                    return Err("a reward needs a title".into());
                }
                let mut body = json!({
                    "title": title.trim(),
                    "cost": (n(o, "cost") as u64).max(1),
                    "is_enabled": b(o, "enabled", true),
                    "is_user_input_required": b(o, "inputRequired", false),
                });
                let prompt = text("prompt");
                if !prompt.is_empty() {
                    body["prompt"] = prompt.into();
                }
                let color = rgb(&text("backgroundColor"));
                if !color.is_empty() {
                    body["background_color"] = color.into();
                }
                let q = [("broadcaster_id", "{broadcaster_id}")];
                let got = self
                    .helix_call("POST", "channel_points/custom_rewards", &q, Some(body), false)
                    .await?;
                ctx.args.insert("rewardId".into(), got["data"][0]["id"].clone());
                crate::twitch::reward_saved(&got["data"][0]);
            }
            // Delete Reward
            99523 => {
                let id = crate::catalog::resolve("twitch.rewards", &text("rewardId"));
                let q = [("broadcaster_id", "{broadcaster_id}"), ("id", id.as_str())];
                self.helix_call("DELETE", "channel_points/custom_rewards", &q, None, false)
                    .await?;
                crate::twitch::reward_removed(&id);
            }
            // Get Shared Chat: whether the channel is in a shared chat, and with whom.
            99541 => {
                let q = [("broadcaster_id", "{broadcaster_id}")];
                let tw = self.twitch().ok_or("Twitch isn't connected")?;
                let session = match tw
                    .helix("GET", "shared_chat/session", q.to_vec(), None, false, true)
                    .await
                {
                    Ok(got) => got["data"][0].clone(),
                    // Twitch answers 404 when there is no session.
                    Err(e) if e.contains("HTTP 404") => Value::Null,
                    Err(e) => return Err(crate::twitch::plain_error(&e)),
                };
                ctx.args.insert("sharedChatActive".into(), (!session.is_null()).into());
                ctx.args
                    .insert("sharedChatSessionId".into(), session["session_id"].clone());
                ctx.args
                    .insert("sharedChatHostId".into(), session["host_broadcaster_id"].clone());
                let people: Vec<Vec<(&str, Value)>> = session["participants"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| vec![("Id", p["broadcaster_id"].clone())])
                    .collect();
                Self::put_list(ctx, "sharedChatParticipant", &people);
            }
            // Get Channel Info: any channel's title, game, language and tags.
            99542 => {
                let who = login(&text("userLogin"));
                let uid = if who.is_empty() {
                    "{broadcaster_id}".to_string()
                } else {
                    self.twitch_id(&who).await?
                };
                let got = self
                    .helix_call("GET", "channels", &[("broadcaster_id", uid.as_str())], None, false)
                    .await?;
                let c = &got["data"][0];
                if c.is_null() {
                    return Err(format!("no Twitch channel {who}"));
                }
                for (arg, key) in [
                    ("channelUser", "broadcaster_name"),
                    ("channelUserName", "broadcaster_login"),
                    ("channelUserId", "broadcaster_id"),
                    ("channelTitle", "title"),
                    ("channelGame", "game_name"),
                    ("channelGameId", "game_id"),
                    ("channelLanguage", "broadcaster_language"),
                ] {
                    ctx.args.insert(arg.into(), c[key].clone());
                }
                let tags: Vec<&str> = c["tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                ctx.args.insert("channelTags".into(), tags.join(", ").into());
            }
            // Get Latest Video: a channel's latest past broadcast, upload or highlight.
            99543 => {
                let who = login(&text("userLogin"));
                let uid = if who.is_empty() {
                    "{broadcaster_id}".to_string()
                } else {
                    self.twitch_id(&who).await?
                };
                let kind = ["archive", "upload", "highlight"][(n(o, "kind") as usize).min(2)];
                let q = [("user_id", uid.as_str()), ("type", kind), ("first", "1")];
                let got = self.helix_call("GET", "videos", &q, None, false).await?;
                let v = &got["data"][0];
                ctx.args.insert("videoFound".into(), (!v.is_null()).into());
                for (arg, key) in [
                    ("videoId", "id"),
                    ("videoTitle", "title"),
                    ("videoUrl", "url"),
                    ("videoDuration", "duration"),
                    ("videoCreatedAt", "created_at"),
                    ("videoViews", "view_count"),
                    ("videoThumbnail", "thumbnail_url"),
                ] {
                    ctx.args.insert(arg.into(), v[key].clone());
                }
            }
            // Create Clip From VOD: a past stream's moment, or an earlier one in this stream.
            99545 => {
                let given = text("videoId");
                // A link to the video works as well as its id.
                let given = given
                    .trim()
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let mut title = text("title").trim().to_string();
                ctx.args.insert("createClipSuccess".into(), false.into());
                let mut offset = self.duration_number(ctx, o, "offset").max(0.0) as u64;
                let length = super::clip_length(self.duration_number(ctx, o, "duration")).unwrap_or("30".into());
                let mut video = given.clone();
                if video.is_empty() || offset == 0 || title.is_empty() {
                    let q: &[(&str, &str)] = if given.is_empty() {
                        &[("user_id", "{broadcaster_id}"), ("type", "archive"), ("first", "1")]
                    } else {
                        &[("id", given.as_str())]
                    };
                    let got = self.helix_call("GET", "videos", q, None, false).await?;
                    let v = &got["data"][0];
                    video = v["id"].as_str().ok_or("no past stream to clip")?.to_string();
                    if offset == 0 {
                        // Its end: the latest moment, while the stream is still going.
                        offset = super::twitch_seconds(v["duration"].as_str().unwrap_or_default());
                    }
                    if title.is_empty() {
                        title = v["title"].as_str().unwrap_or_default().to_string();
                    }
                }
                // Twitch wants the end at least the clip's length in.
                let offset = offset
                    .max(length.parse::<f64>().unwrap_or(30.0).ceil() as u64)
                    .to_string();
                let q = [
                    ("editor_id", "{broadcaster_id}"),
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("vod_id", video.as_str()),
                    ("vod_offset", offset.as_str()),
                    ("duration", length.as_str()),
                    ("title", title.as_str()),
                ];
                let got = self.helix_call("POST", "videos/clips", &q, None, false).await?;
                let id = got["data"][0]["id"].as_str().ok_or("Twitch made no clip")?;
                ctx.args.insert("createClipSuccess".into(), true.into());
                ctx.args.insert("clipId".into(), id.into());
                ctx.args.insert("createClipId".into(), id.into());
                ctx.args
                    .insert("createClipUrl".into(), format!("https://clips.twitch.tv/{id}").into());
                ctx.args
                    .insert("createClipEditUrl".into(), got["data"][0]["edit_url"].clone());
                ctx.args
                    .insert("createClipCreatedAt".into(), super::globals::iso_now().into());
            }
            // Chat Mode for a While: turns a mode on, and puts the chat back as it was after the time.
            99544 => {
                let seconds = self.duration_number(ctx, o, "duration").max(0.0) as u64;
                if seconds == 0 {
                    return Err("give it a time: the mode would never end".into());
                }
                let value = crate::fmt::parse_number(&text("value"));
                let mode = (n(o, "mode") as usize).min(4);
                let (flag, extra, default) = [
                    ("slow_mode", Some("slow_mode_wait_time"), 30.0),
                    ("follower_mode", Some("follower_mode_duration"), 0.0),
                    ("subscriber_mode", None, 0.0),
                    ("emote_mode", None, 0.0),
                    ("unique_chat_mode", None, 0.0),
                ][mode];
                if self.dev_out("twitch", &format!("/{flag} for {seconds}s"), false) {
                    return Ok(());
                }
                let before = self.helix_mod("GET", "chat/settings", &[], None).await?;
                let (on, back) = chat_mode_patches(&before["data"][0], flag, extra, value.unwrap_or(default));
                self.helix_mod("PATCH", "chat/settings", &[], Some(on)).await?;
                let engine = self.clone();
                tokio::task::spawn_local(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
                    if let Err(e) = engine.helix_mod("PATCH", "chat/settings", &[], Some(back)).await {
                        crate::error(format!("Twitch: chat mode not put back: {e}"));
                    }
                });
            }
            // Set Bot Chat Colour: a named colour, or #rrggbb for Turbo and Prime accounts.
            99533 => {
                let color = text("color").trim().to_string();
                let color = if color.starts_with('#') { rgb(&color) } else { color };
                if color.is_empty() {
                    return Err("no colour given".into());
                }
                let q = [("user_id", "{sender_id}"), ("color", color.as_str())];
                self.helix_call("PUT", "chat/color", &q, None, true).await?;
            }
            // Block / Unblock User
            99534 | 99535 => {
                let uid = self.twitch_id(&text("userLogin")).await?;
                let method = if kind == 99534 { "PUT" } else { "DELETE" };
                self.helix_call(method, "users/blocks", &[("target_user_id", uid.as_str())], None, false)
                    .await?;
            }
            // Get Schedule
            99536 => {
                let got = self
                    .helix_call(
                        "GET",
                        "schedule",
                        &[("broadcaster_id", "{broadcaster_id}"), ("first", "10")],
                        None,
                        false,
                    )
                    .await
                    .or_else(|e| {
                        if e.contains("404") || e.to_lowercase().contains("not found") {
                            Ok(json!({}))
                        } else {
                            Err(e)
                        }
                    })?;
                let data = &got["data"];
                let items: Vec<Vec<(&str, Value)>> = data["segments"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|s| s["canceled_until"].is_null())
                    .map(|s| {
                        vec![
                            ("Id", s["id"].clone()),
                            ("Start", s["start_time"].clone()),
                            ("End", s["end_time"].clone()),
                            ("Title", s["title"].clone()),
                            ("Category", s["category"]["name"].clone()),
                            ("Recurring", s["is_recurring"].clone()),
                        ]
                    })
                    .collect();
                Self::put_list(ctx, "segment", &items);
                ctx.args
                    .insert("onVacation".into(), (!data["vacation"].is_null()).into());
            }
            // Add Schedule Segment
            99537 => {
                let start = parse_when(&text("start")).ok_or("the start isn't a date and time")?;
                let minutes = (self.duration_number(ctx, o, "duration") / 60.0)
                    .round()
                    .clamp(30.0, 1380.0) as u64;
                let mut body = json!({
                    "start_time": iso_utc(start),
                    "timezone": local_zone(),
                    "duration": minutes.to_string(),
                    "is_recurring": b(o, "recurring", false),
                });
                let title = text("title");
                if !title.is_empty() {
                    body["title"] = title.into();
                }
                let category = text("category");
                if !category.trim().is_empty() {
                    body["category_id"] = self.twitch_category_id(category.trim()).await?.into();
                }
                let got = self
                    .helix_call(
                        "POST",
                        "schedule/segment",
                        &[("broadcaster_id", "{broadcaster_id}")],
                        Some(body),
                        false,
                    )
                    .await?;
                ctx.args
                    .insert("segmentId".into(), got["data"]["segments"][0]["id"].clone());
            }
            // Cancel Next Schedule Segment
            99538 => {
                let got = self
                    .helix_call(
                        "GET",
                        "schedule",
                        &[("broadcaster_id", "{broadcaster_id}"), ("first", "1")],
                        None,
                        false,
                    )
                    .await?;
                let id = got["data"]["segments"][0]["id"]
                    .as_str()
                    .ok_or("nothing is scheduled")?
                    .to_string();
                let q = [("broadcaster_id", "{broadcaster_id}"), ("id", id.as_str())];
                self.helix_call(
                    "PATCH",
                    "schedule/segment",
                    &q,
                    Some(json!({"is_canceled": true})),
                    false,
                )
                .await?;
                ctx.args.insert("segmentId".into(), id.into());
            }
            // Set Vacation
            99539 => {
                let on = n(o, "state") as u64 != 1;
                let zone = local_zone();
                let (start, end);
                let mut q = vec![
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("is_vacation_enabled", if on { "true" } else { "false" }),
                ];
                if on {
                    let from = parse_when(&text("start")).ok_or("the start isn't a date and time")?;
                    let to = parse_when(&text("end")).ok_or("the end isn't a date and time")?;
                    (start, end) = (iso_utc(from), iso_utc(to));
                    q.extend([
                        ("vacation_start_time", start.as_str()),
                        ("vacation_end_time", end.as_str()),
                        ("timezone", zone.as_str()),
                    ]);
                }
                self.helix_call("PATCH", "schedule/settings", &q, None, false).await?;
            }
            _ => self.unsupported(kind),
        }
        Ok(())
    }

    /// Twitch's Guest Star (still a beta API at Twitch): settings, the session, invites and slots.
    async fn guest_star(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) -> Result<(), String> {
        let flag = |key: &str| if b(o, key, false) { "true" } else { "false" };
        // The live session's id, for what acts on it.
        let session = if matches!(kind, 524..=531) {
            let got = self.helix_mod("GET", "guest_star/session", &[], None).await?;
            let id = got["data"][0]["id"].as_str().unwrap_or_default().to_string();
            if id.is_empty() {
                return Err("no Guest Star session is running".into());
            }
            id
        } else {
            String::new()
        };
        let slot = (n(o, "slot") as u64).to_string();
        match kind {
            520 => {
                let got = self.helix_mod("GET", "guest_star/channel_settings", &[], None).await?;
                let s = &got["data"][0];
                for (arg, key) in [
                    ("moderatorSendLiveEnabled", "is_moderator_send_live_enabled"),
                    ("slotCount", "slot_count"),
                    ("browserSourceAudioEnabled", "is_browser_source_audio_enabled"),
                    ("groupLayout", "group_layout"),
                    ("browserSourceToken", "browser_source_token"),
                ] {
                    ctx.args.insert(arg.into(), s[key].clone());
                }
            }
            521 => {
                let mut body = json!({
                    "is_moderator_send_live_enabled": b(o, "moderatorSendLiveEnabled", false),
                    "is_browser_source_audio_enabled": b(o, "browserSourceAudioEnabled", true),
                    "regenerate_browser_sources": b(o, "regenerateBrowserSources", false),
                });
                if n(o, "slotCount") as u64 > 0 {
                    body["slot_count"] = (n(o, "slotCount") as u64).min(6).into();
                }
                let layout = self.expand(ctx, s(o, "groupLayout"));
                if !layout.trim().is_empty() {
                    body["group_layout"] = layout.trim().to_uppercase().into();
                }
                self.helix_call(
                    "PUT",
                    "guest_star/channel_settings",
                    &[("broadcaster_id", "{broadcaster_id}")],
                    Some(body),
                    false,
                )
                .await?;
            }
            522 | 523 => {
                let got = if kind == 522 {
                    self.helix_mod("GET", "guest_star/session", &[], None).await?
                } else {
                    self.helix_call(
                        "POST",
                        "guest_star/session",
                        &[("broadcaster_id", "{broadcaster_id}")],
                        None,
                        false,
                    )
                    .await?
                };
                let s = &got["data"][0];
                ctx.args.insert("guestStarSessionId".into(), s["id"].clone());
                let guests: Vec<Vec<(&str, Value)>> = s["guests"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|g| {
                        vec![
                            ("UserId", g["user_id"].clone()),
                            ("User", g["user_display_name"].clone()),
                            ("UserName", g["user_login"].clone()),
                            ("Slot", g["slot_id"].clone()),
                            ("IsLive", g["is_live"].clone()),
                            ("Volume", g["volume"].clone()),
                        ]
                    })
                    .collect();
                Self::put_list(ctx, "guest", &guests);
            }
            524 => {
                let q = [("broadcaster_id", "{broadcaster_id}"), ("session_id", session.as_str())];
                self.helix_call("DELETE", "guest_star/session", &q, None, false).await?;
            }
            525 => {
                let got = self
                    .helix_mod("GET", "guest_star/invites", &[("session_id", session.as_str())], None)
                    .await?;
                let invites: Vec<Vec<(&str, Value)>> = got["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|i| {
                        vec![
                            ("UserId", i["user_id"].clone()),
                            ("Status", i["status"].clone()),
                            ("InvitedAt", i["invited_at"].clone()),
                        ]
                    })
                    .collect();
                Self::put_list(ctx, "invite", &invites);
            }
            526..=528 | 530 => {
                let guest = self.twitch_id(&self.expand(ctx, s(o, "userLogin"))).await?;
                let mut q = vec![("session_id", session.as_str()), ("guest_id", guest.as_str())];
                let (method, path) = match kind {
                    526 => ("POST", "guest_star/invites"),
                    527 => ("DELETE", "guest_star/invites"),
                    528 => ("POST", "guest_star/slot"),
                    _ => ("DELETE", "guest_star/slot"),
                };
                if kind >= 528 {
                    q.push(("slot_id", slot.as_str()));
                }
                self.helix_mod(method, path, &q, None).await?;
            }
            529 => {
                let (from, to) = (
                    (n(o, "sourceSlot") as u64).to_string(),
                    (n(o, "destinationSlot") as u64).to_string(),
                );
                let q = [
                    ("session_id", session.as_str()),
                    ("source_slot_id", from.as_str()),
                    ("destination_slot_id", to.as_str()),
                ];
                self.helix_mod("PATCH", "guest_star/slot", &q, None).await?;
            }
            _ => {
                let volume = (n(o, "volume") as u64).min(100).to_string();
                let q = [
                    ("session_id", session.as_str()),
                    ("slot_id", slot.as_str()),
                    ("is_audio_enabled", flag("audioEnabled")),
                    ("is_video_enabled", flag("videoEnabled")),
                    ("is_live", flag("live")),
                    ("volume", volume.as_str()),
                ];
                self.helix_mod("PATCH", "guest_star/slot_settings", &q, None).await?;
            }
        }
        Ok(())
    }

    /// A Twitch category's id from its name (the best match Twitch finds).
    async fn twitch_category_id(&self, name: &str) -> Result<String, String> {
        let got = self
            .helix_call(
                "GET",
                "search/categories",
                &[("query", name), ("first", "1")],
                None,
                false,
            )
            .await?;
        got["data"][0]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("no category {name}"))
    }

    /// The same values for a stream on any platform.
    #[allow(clippy::too_many_arguments)]
    fn put_stream(
        &self,
        ctx: &mut Ctx,
        platform: &str,
        live: bool,
        title: Value,
        category: Value,
        viewers: Value,
        started: &str,
    ) {
        let up = if live && !started.is_empty() { since(started) } else { 0 };
        let mut put = |k: &str, v: Value| drop(ctx.args.insert(k.into(), v));
        put("isLive", live.into());
        put("streamPlatform", platform.into());
        put("streamTitle", title);
        put("streamCategory", category);
        put("viewerCount", if viewers.is_null() { 0.into() } else { viewers });
        put("startedAt", started.into());
        put("uptimeSeconds", up.into());
        put("uptime", clock(up).into());
    }

    async fn kick_tool(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) -> Result<(), String> {
        let text = |key: &str| self.expand(ctx, s(o, key));
        let kick = self.kick_start();
        match kind {
            // Accept / Reject Redemption: the one given, else the triggering reward's newest pending
            // redemption by the same user (Kick's chat feed doesn't say which redemption it was).
            99601 | 99602 => {
                let mut id = text("redemptionId").trim().to_string();
                ctx.args.insert("redemptionResult".into(), false.into());
                if id.is_empty() {
                    let reward = ctx
                        .args
                        .get("rewardId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let user = ctx.args.get("userId").map(crate::vars::text).unwrap_or_default();
                    if reward.is_empty() {
                        return Err("no redemption given, and no reward started this".into());
                    }
                    let path = format!(
                        "/channels/rewards/redemptions?reward_id={}",
                        crate::commands::url_encode(&reward)
                    );
                    let got = kick.api("GET", &path, None).await?;
                    id = got["data"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .flat_map(|group| group["redemptions"].as_array().into_iter().flatten())
                        .filter(|r| user.is_empty() || crate::vars::text(&r["redeemer"]["user_id"]) == user)
                        .max_by_key(|r| r["redeemed_at"].as_str().unwrap_or_default().to_string())
                        .and_then(|r| r["id"].as_str())
                        .map(str::to_string)
                        .ok_or("no pending redemption found")?;
                }
                let path = if kind == 99601 { "accept" } else { "reject" };
                let got = kick
                    .api(
                        "POST",
                        &format!("/channels/rewards/redemptions/{path}"),
                        Some(json!({"ids": [id]})),
                    )
                    .await?;
                if let Some(why) = got["data"][0]["reason"].as_str() {
                    return Err(match why {
                        "NOT_PENDING" => "that redemption isn't pending".into(),
                        "NOT_FOUND" => "no such redemption".into(),
                        "NOT_OWNED" => "only rewards Streamerz made can be accepted or rejected".into(),
                        other => other.to_string(),
                    });
                }
                ctx.args.insert("redemptionId".into(), id.into());
                ctx.args.insert("redemptionResult".into(), true.into());
            }
            // Create Reward
            99603 => {
                let title = text("title");
                if title.trim().is_empty() {
                    return Err("a reward needs a title".into());
                }
                let mut body = json!({
                    "title": title.trim().chars().take(50).collect::<String>(),
                    "cost": (n(o, "cost") as u64).max(1),
                    "is_enabled": b(o, "enabled", true),
                    "is_user_input_required": b(o, "inputRequired", false),
                });
                let description = text("description");
                if !description.is_empty() {
                    body["description"] = description.chars().take(200).collect::<String>().into();
                }
                let color = rgb(&text("backgroundColor"));
                if !color.is_empty() {
                    body["background_color"] = color.into();
                }
                let got = kick.api("POST", "/channels/rewards", Some(body)).await?;
                ctx.args.insert("rewardId".into(), got["data"]["id"].clone());
                crate::kick::reward_saved(&got["data"]);
                if let Some(id) = got["data"]["id"].as_str() {
                    let key = crate::rewards::key("kick", id);
                    self.change(|c| c.rewards.made.push(key));
                }
            }
            // Delete Reward
            99604 => {
                let id = crate::catalog::resolve("kick.rewards", &text("rewardId"));
                kick.api(
                    "DELETE",
                    &format!("/channels/rewards/{}", crate::commands::url_encode(&id)),
                    None,
                )
                .await?;
                crate::kick::reward_removed(&id);
            }
            // Get KICKs Leaderboard
            99605 => {
                let count = (n(o, "count") as u64).clamp(1, 100);
                let period = ["lifetime", "month", "week"]
                    .get(n(o, "period") as usize)
                    .copied()
                    .unwrap_or("lifetime");
                let got = kick
                    .api("GET", &format!("/kicks/leaderboard?top={count}"), None)
                    .await?;
                let items: Vec<Vec<(&str, Value)>> = got["data"][period]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|u| {
                        vec![
                            ("User", u["username"].clone()),
                            (
                                "UserName",
                                u["username"].as_str().unwrap_or_default().to_lowercase().into(),
                            ),
                            ("UserId", u["user_id"].clone()),
                            ("Kicks", u["gifted_amount"].clone()),
                            ("Rank", u["rank"].clone()),
                        ]
                    })
                    .collect();
                Self::put_list(ctx, "kicksLeader", &items);
            }
            // Get Stream Info
            99606 => {
                let id = self
                    .cfg
                    .borrow()
                    .kick
                    .broadcaster
                    .as_ref()
                    .map(|a| a.user_id)
                    .unwrap_or(0);
                if id == 0 {
                    return Err("log in to Kick in Accounts first".into());
                }
                let got = kick
                    .api("GET", &format!("/users/livestreams?user_id={id}"), None)
                    .await?;
                let live = &got["data"][0];
                self.put_stream(
                    ctx,
                    "kick",
                    !live.is_null(),
                    live["title"].clone(),
                    live["category"]["name"].clone(),
                    live["viewer_count"].clone(),
                    live["started_at"].as_str().unwrap_or_default(),
                );
                ctx.args.insert("thumbnailUrl".into(), live["thumbnail"].clone());
                ctx.args.insert("language".into(), live["language_code"].clone());
            }
            // Remove Channel Tag
            99607 => {
                let tag = text("tag").trim().to_string();
                let got = kick.api("GET", "/channels", None).await?;
                let tags: Vec<Value> = got["data"][0]["custom_tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|t| !t.as_str().is_some_and(|t| t.eq_ignore_ascii_case(&tag)))
                    .cloned()
                    .collect();
                kick.api("PATCH", "/channels", Some(json!({"custom_tags": tags})))
                    .await?;
            }
            _ => self.unsupported(kind),
        }
        Ok(())
    }

    async fn tiktok_tool(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) -> Result<(), String> {
        let who = login(&self.expand(ctx, s(o, "userLogin")));
        match kind {
            // User groups: the same as Twitch's, by the id the chat has seen for that name.
            99450..=99452 => {
                let id = self.seen_id("tiktok", &who).unwrap_or_else(|| who.clone());
                let group = self.simple(ctx, s(o, "groupName"));
                self.group_sub(ctx, "tiktok", kind - 99450, &who, &id, &group);
            }
            // Add Random Users: from the people who chatted this live.
            99453 => self.random_seen(ctx, "tiktok", (n(o, "count") as usize).max(1)),
            // Get User Info for Target: what the chat knows of them.
            99454 => self.seen_target(ctx, "tiktok", &who)?,
            _ => self.unsupported(kind),
        }
        Ok(())
    }

    /// Add to, remove from (`op` 0, 1) or look up in (2) a user group, as Streamer.bot's do.
    pub(super) fn group_sub(
        self: &Rc<Self>,
        ctx: &mut Ctx,
        platform: &str,
        op: u64,
        name: &str,
        id: &str,
        group: &str,
    ) {
        let gid = self
            .cfg
            .borrow()
            .groups
            .iter()
            .find(|g| s(g, "name").eq_ignore_ascii_case(group) || s(g, "id") == group)
            .map(|g| s(g, "id").to_string());
        if op == 2 {
            let member = crate::usergroups::key(platform, id);
            let found = gid.as_deref().is_some_and(|gid| {
                self.cfg
                    .borrow()
                    .groups
                    .iter()
                    .any(|g| s(g, "id") == gid && crate::usergroups::has(g, &member))
            });
            ctx.args.insert("inGroup".into(), found.into());
            if found {
                for (k, x) in [("foundUser", name), ("foundUserName", name), ("foundUserId", id)] {
                    ctx.args.insert(k.into(), x.into());
                }
                ctx.args.insert("foundUserType".into(), platform.into());
            }
            return;
        }
        let add = op == 0;
        ctx.args.insert(
            if add { "addResult" } else { "removeResult" }.into(),
            gid.is_some().into(),
        );
        if let Some(gid) = gid {
            let mut u = Obj::new();
            u.insert("userId".into(), id.into());
            u.insert("user".into(), name.into());
            u.insert("userName".into(), name.into());
            u.insert("userType".into(), platform.into());
            self.group_users(&gid, &[u], add);
        }
    }

    /// `count` people picked at random from those who chatted on `platform` this session.
    fn random_seen(&self, ctx: &mut Ctx, platform: &str, count: usize) {
        let mut people = self.chatters(platform);
        let take = count.min(people.len());
        for i in 0..take {
            let at = i + crate::math::secure_u64() as usize % (people.len() - i);
            people.swap(i, at);
            let (name, id) = &people[i];
            ctx.args.insert(format!("randomUser{i}"), name.as_str().into());
            ctx.args
                .insert(format!("randomUserName{i}"), name.to_lowercase().into());
            ctx.args.insert(format!("randomUserId{i}"), id.as_str().into());
            ctx.args.insert(format!("randomUserType{i}"), platform.into());
        }
        ctx.args.insert("randomUsersCount".into(), take.into());
    }

    /// The target values (`targetUser`...) for someone the chat has seen on `platform`.
    fn seen_target(&self, ctx: &mut Ctx, platform: &str, who: &str) -> Result<(), String> {
        ctx.args.insert("addTargetResult".into(), false.into());
        let id = self
            .seen_id(platform, who)
            .ok_or_else(|| format!("{who} hasn't chatted this session"))?;
        let seen = self.chatter(platform, &id);
        let get = |k: &str| seen.as_ref().and_then(|s| s.args.get(k).cloned()).unwrap_or_default();
        let name = seen
            .as_ref()
            .and_then(|s| s.args.get("user").map(crate::vars::text))
            .unwrap_or_else(|| who.to_string());
        let mut put = |k: &str, v: Value| drop(ctx.args.insert(k.into(), v));
        put("addTargetResult", true.into());
        put("targetUser", name.as_str().into());
        put("targetUserName", who.to_lowercase().into());
        put("targetUserId", id.as_str().into());
        put("targetUserType", platform.into());
        put("targetUserProfileImageUrl", get("userProfileUrl"));
        put("targetIsSubscribed", get("isSubscribed"));
        put("targetIsModerator", get("isModerator"));
        put("targetMessageCount", seen.as_ref().map_or(0, |s| s.messages).into());
        Ok(())
    }

    /// The platforms a sub-action of every platform acts on: those ticked, else the one its trigger
    /// came from.
    fn chosen_platforms(ctx: &Ctx, o: &Obj) -> Vec<String> {
        let ticked: Vec<String> = o
            .get("platforms")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|p| PLATFORMS.contains(p))
            .map(str::to_string)
            .collect();
        if !ticked.is_empty() {
            return ticked;
        }
        let from = ctx
            .args
            .get("userType")
            .or_else(|| ctx.args.get("platform"))
            .map(crate::vars::text)
            .unwrap_or_default()
            .to_lowercase();
        if PLATFORMS.contains(&from.as_str()) {
            vec![from]
        } else {
            Vec::new()
        }
    }

    /// A sub-action of every platform: the platform's own sub-action, run for each chosen platform.
    async fn every_platform(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) {
        let platforms = Self::chosen_platforms(ctx, o);
        if platforms.is_empty() {
            crate::warn(format!(
                "{}: no platform ticked, and its trigger isn't from one",
                crate::types::name(crate::types::SUB_ACTIONS, kind as u32).unwrap_or("Sub-action")
            ));
            return;
        }
        let multi = platforms.len() > 1;
        for platform in &platforms {
            let Some(sub) = self.platform_sub(ctx, o, kind, platform) else {
                crate::warn(format!(
                    "{}: {platform} can't do this, skipped",
                    crate::types::name(crate::types::SUB_ACTIONS, kind as u32).unwrap_or("Sub-action")
                ));
                continue;
            };
            let before: Vec<String> = ctx.args.keys().cloned().collect();
            let _ = self.one(ctx, &sub).await;
            // Several platforms: each one's values also under its name (twitchIsLive...).
            if multi {
                let added: Vec<(String, Value)> = ctx
                    .args
                    .iter()
                    .filter(|(k, _)| {
                        !before.contains(k)
                            || matches!(
                                k.as_str(),
                                "isLive"
                                    | "viewerCount"
                                    | "streamTitle"
                                    | "streamCategory"
                                    | "uptime"
                                    | "uptimeSeconds"
                                    | "startedAt"
                            )
                    })
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (k, v) in added {
                    let mut name = platform.clone();
                    name.extend(k.chars().take(1).flat_map(char::to_uppercase));
                    name.push_str(&k[k.chars().next().map_or(0, char::len_utf8)..]);
                    ctx.args.insert(name, v);
                }
            }
        }
        // Stream info for several platforms: live on any, viewers added up.
        if kind == 99731 && multi {
            let any = platforms
                .iter()
                .any(|p| ctx.args.get(&format!("{p}IsLive")).and_then(Value::as_bool) == Some(true));
            let viewers: u64 = platforms
                .iter()
                .filter_map(|p| ctx.args.get(&format!("{p}ViewerCount")).and_then(Value::as_u64))
                .sum();
            ctx.args.insert("isLive".into(), any.into());
            ctx.args.insert("viewerCount".into(), viewers.into());
        }
    }

    /// `platform`'s own sub-action for a sub-action of every platform, with the same settings; None
    /// when the platform can't do it.
    fn platform_sub(&self, ctx: &Ctx, o: &Obj, kind: u64, platform: &str) -> Option<Obj> {
        let field = |k: &str| o.get(k).cloned().unwrap_or(Value::String(String::new()));
        let mut sub = Obj::new();
        let mut set = |k: &str, v: Value| drop(sub.insert(k.into(), v));
        // YouTube acts on channel ids: the name is looked up among the people its chat has seen.
        let youtube_id = || self.youtube_channel_id(ctx, &self.expand(ctx, s(o, "userLogin")));
        let code: u64 = match (kind, platform) {
            (99720, _) | (99721, "youtube" | "tiktok") => {
                let mut text = field("text");
                // A reply where the platform has no replies: the name in front.
                if kind == 99721 {
                    let to = ctx.args.get("user").map(crate::vars::text).unwrap_or_default();
                    if !to.is_empty() {
                        text = format!("@{to} {}", crate::vars::text(&text)).into();
                    }
                }
                set("text", text);
                set("useBot", field("useBot"));
                set("fallback", field("fallback"));
                match platform {
                    "twitch" => 10,
                    "kick" => 35001,
                    "youtube" => 4001,
                    _ => 43001,
                }
            }
            (99721, "twitch" | "kick") => {
                set("text", field("text"));
                set("replyId", field("messageId"));
                set("useBot", field("useBot"));
                set("fallback", field("fallback"));
                if platform == "twitch" {
                    560
                } else {
                    35002
                }
            }
            (99723, "twitch") => {
                set("messageId", field("messageId"));
                set("duration", field("duration"));
                576
            }
            (99723, "tiktok") => {
                set("msgId", field("messageId"));
                43011
            }
            (99724, "twitch") => 578,
            (99724, "tiktok") => {
                set("msgId", Value::String("%msgId%".into()));
                43012
            }
            (99726, "twitch" | "kick") => {
                set("userLogin", field("userLogin"));
                if platform == "twitch" {
                    513
                } else {
                    35043
                }
            }
            // YouTube lifts a timeout like a ban, by the one Streamerz made.
            (99726, "youtube") => {
                set("userId", youtube_id().into());
                4015
            }
            (99731, "youtube") => 99661,
            (99733, "youtube") => {
                set("userLogin", field("userLogin"));
                99662
            }
            (99729, _) => {
                set("title", field("title"));
                match platform {
                    "twitch" => 15,
                    "kick" => 35030,
                    "youtube" => 4002,
                    _ => 43010,
                }
            }
            (99730, "twitch") => {
                set("source", 0.into());
                set("game", field("category"));
                16
            }
            (99730, "kick") => {
                set("source", 0.into());
                set("categoryName", field("category"));
                35031
            }
            (99731, "twitch") => 99515,
            (99731, "kick") => 99606,
            (99731, "tiktok") => 43003,
            (99732, "twitch") => {
                set("userLogin", field("userLogin"));
                29
            }
            (99732, _) => {
                // No shoutouts elsewhere: a message naming them.
                let text = crate::vars::text(&field("text"));
                if text.trim().is_empty() {
                    return None;
                }
                set("text", text.into());
                match platform {
                    "kick" => 35001,
                    "youtube" => 4001,
                    _ => 43001,
                }
            }
            (99733, "twitch" | "kick") => {
                set("userLogin", field("userLogin"));
                if platform == "twitch" {
                    50
                } else {
                    35011
                }
            }
            (99733, "tiktok") => {
                set("userLogin", field("userLogin"));
                99454
            }
            _ => return None,
        };
        set("type", code.into());
        set("enabled", true.into());
        Some(sub)
    }

    /// The embed a Discord sub-action describes, with variables filled in.
    fn discord_embed(&self, ctx: &Ctx, o: &Obj) -> crate::remote::Embed {
        let get = |k: &str| self.expand(ctx, s(o, k)).trim().to_string();
        let fields = match o.get("fields") {
            Some(Value::Object(map)) => map
                .iter()
                .map(|(k, v)| {
                    (
                        self.expand(ctx, k).trim().to_string(),
                        self.expand(ctx, &crate::vars::text(v)).trim().to_string(),
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        crate::remote::Embed {
            title: get("title"),
            description: self.expand(ctx, s(o, "description")),
            color: get("color"),
            url: get("url"),
            author: get("author"),
            footer: get("footer"),
            image: get("image"),
            thumbnail: get("thumbnail"),
            fields,
            inline: b(o, "inlineFields", false),
            timestamp: b(o, "timestamp", false).then(|| iso_utc(crate::fmt::unix_now().as_secs() as i64)),
        }
    }

    /// Discord webhooks: a rich embed, editing or deleting a message it posted, and a file.
    async fn discord_tool(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) -> Result<(), String> {
        let text = |ctx: &Ctx, key: &str| self.expand(ctx, s(o, key));
        let hook = text(ctx, "webhookUrl");
        let thread = text(ctx, "threadId").trim().to_string();
        match kind {
            99740 => {
                let body = crate::remote::discord_message(
                    &text(ctx, "content"),
                    &text(ctx, "username"),
                    &text(ctx, "avatar"),
                    &self.discord_embed(ctx, o),
                    &text(ctx, "threadName"),
                );
                if body.as_object().is_none_or(|m| {
                    m.keys()
                        .all(|k| matches!(k.as_str(), "username" | "avatar_url" | "thread_name"))
                }) {
                    return Err("nothing to post: give it some text or an embed".into());
                }
                let sent = crate::remote::discord_call("POST", &hook, "", &thread, Some(&body)).await?;
                ctx.args.insert("messageId".into(), sent["id"].clone());
                ctx.args.insert("discordChannelId".into(), sent["channel_id"].clone());
            }
            99741 => {
                let id = text(ctx, "messageId");
                if id.trim().is_empty() {
                    return Err("no message given".into());
                }
                let embed = self.discord_embed(ctx, o);
                // Only what is given changes: the text, and the embed when it has anything.
                let body = crate::remote::discord_message(&text(ctx, "content"), "", "", &embed, "");
                if body.as_object().is_none_or(|m| m.is_empty()) {
                    return Err("nothing to change: give it some text or an embed".into());
                }
                crate::remote::discord_call("PATCH", &hook, id.trim(), &thread, Some(&body)).await?;
            }
            99742 => {
                let id = text(ctx, "messageId");
                if id.trim().is_empty() {
                    return Err("no message given".into());
                }
                crate::remote::discord_call("DELETE", &hook, id.trim(), &thread, None).await?;
            }
            _ => {
                let payload = crate::remote::discord_message(
                    &text(ctx, "content"),
                    &text(ctx, "username"),
                    &text(ctx, "avatar"),
                    &Default::default(),
                    "",
                );
                let id = crate::remote::discord_upload(
                    &hook,
                    &thread,
                    &payload,
                    text(ctx, "file").trim(),
                    text(ctx, "fileName").trim(),
                )
                .await?;
                ctx.args.insert("messageId".into(), id.into());
            }
        }
        Ok(())
    }

    async fn youtube_tool(self: &Rc<Self>, ctx: &mut Ctx, o: &Obj, kind: u64) -> Result<(), String> {
        // Get Poll Results: the poll on the live now, as the chat shows it (no call to YouTube).
        if kind == 99665 {
            let poll = crate::youtube::current_poll();
            ctx.args.insert("pollFound".into(), (!poll.is_empty()).into());
            ctx.args.extend(poll);
            return Ok(());
        }
        let yt = self.youtube_start();
        let broadcast = self.expand(ctx, s(o, "broadcast"));
        let who = self.expand(ctx, s(o, "userLogin"));
        let enc = crate::commands::url_encode;
        match kind {
            // Add to Playlist: the broadcast's video at the end of one of the channel's playlists.
            99666 => {
                let given = self.expand(ctx, s(o, "playlist"));
                let playlist = playlist_id(&given);
                if playlist.is_empty() {
                    return Err("no playlist given".into());
                }
                let body = json!({"snippet": {"playlistId": playlist, "resourceId": {"kind": "youtube#video", "videoId": "{video}"}}});
                let got = yt
                    .data("POST", &broadcast, "playlistItems?part=snippet", Some(body))
                    .await?;
                ctx.args.insert("playlistItemId".into(), got["id"].clone());
            }
            // Go Live: moves the broadcast to testing, or live.
            99664 => {
                let stage = ["testing", "live"][(n(o, "stage") as usize).min(1)];
                if self.dev_out("youtube", &format!("/go {stage}"), false) {
                    return Ok(());
                }
                let path = format!("liveBroadcasts/transition?broadcastStatus={stage}&part=status&id={{video}}");
                yt.data("POST", &broadcast, &path, None).await?;
            }
            // Tags: Set, Add, Remove, Clear.
            99654..=99657 => {
                let mut tags: Vec<String> = if kind == 99655 || kind == 99656 {
                    let got = yt
                        .data("GET", &broadcast, "videos?part=snippet&id={video}", None)
                        .await?;
                    got["items"][0]["snippet"]["tags"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                } else {
                    Vec::new()
                };
                let tag = self.expand(ctx, s(o, "tag")).trim().to_string();
                match kind {
                    99654 => {
                        tags = o
                            .get("tags")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .map(|t| self.expand(ctx, &crate::vars::text(t)).trim().to_string())
                            .filter(|t| !t.is_empty())
                            .collect()
                    }
                    99655 if !tag.is_empty() && !tags.iter().any(|t| t.eq_ignore_ascii_case(&tag)) => tags.push(tag),
                    99656 => tags.retain(|t| !t.eq_ignore_ascii_case(&tag)),
                    _ => {}
                }
                yt.update_video(
                    &broadcast,
                    crate::youtube::VideoUpdate {
                        tags: Some(tags),
                        ..Default::default()
                    },
                )
                .await?;
            }
            // Set Category / Set Privacy: the video's snippet or status, as it is, with one value changed.
            99658 | 99659 => {
                let part = if kind == 99658 { "snippet" } else { "status" };
                let got = yt
                    .data("GET", &broadcast, &format!("videos?part={part}&id={{video}}"), None)
                    .await?;
                let mut now = got["items"][0][part].clone();
                let Some(fields) = now.as_object_mut() else {
                    return Err("YouTube didn't say what the video has".into());
                };
                if kind == 99658 {
                    let id = YOUTUBE_CATEGORIES.get(n(o, "category") as usize).map_or("20", |c| c.0);
                    fields.insert("categoryId".into(), id.into());
                    for k in [
                        "publishedAt",
                        "channelId",
                        "thumbnails",
                        "channelTitle",
                        "liveBroadcastContent",
                        "localized",
                    ] {
                        fields.remove(k);
                    }
                } else {
                    let privacy = ["public", "unlisted", "private"]
                        .get(n(o, "privacy") as usize)
                        .copied()
                        .unwrap_or("public");
                    fields.insert("privacyStatus".into(), privacy.into());
                    for k in ["uploadStatus", "failureReason", "rejectionReason", "madeForKids"] {
                        fields.remove(k);
                    }
                }
                let body = json!({"id": "{video}", part: now});
                yt.data("PUT", &broadcast, &format!("videos?part={part}"), Some(body))
                    .await?;
            }
            // End Stream
            99660 => {
                if self.dev_out("youtube", "/end stream", false) {
                    return Ok(());
                }
                yt.data(
                    "POST",
                    &broadcast,
                    "liveBroadcasts/transition?broadcastStatus=complete&part=status&id={video}",
                    None,
                )
                .await?;
            }
            // Get Broadcast Info
            99661 => {
                let got = yt
                    .data(
                        "GET",
                        &broadcast,
                        "videos?part=snippet,liveStreamingDetails,statistics&id={video}",
                        None,
                    )
                    .await?;
                let v = &got["items"][0];
                let live = v["snippet"]["liveBroadcastContent"] == "live";
                let viewers = v["liveStreamingDetails"]["concurrentViewers"]
                    .as_str()
                    .and_then(|c| c.parse::<u64>().ok())
                    .map_or(Value::Null, Value::from);
                let started = v["liveStreamingDetails"]["actualStartTime"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let category = YOUTUBE_CATEGORIES
                    .iter()
                    .find(|c| v["snippet"]["categoryId"] == c.0)
                    .map_or(Value::Null, |c| c.1.into());
                self.put_stream(
                    ctx,
                    "youtube",
                    live,
                    v["snippet"]["title"].clone(),
                    category,
                    viewers,
                    &started,
                );
                ctx.args.insert("broadcastId".into(), v["id"].clone());
                ctx.args
                    .insert("likeCount".into(), v["statistics"]["likeCount"].clone());
                ctx.args
                    .insert("viewCount".into(), v["statistics"]["viewCount"].clone());
                ctx.args.insert(
                    "scheduledStart".into(),
                    v["liveStreamingDetails"]["scheduledStartTime"].clone(),
                );
            }
            // Get User Info for Target
            99662 => {
                let id = self.youtube_channel_id(ctx, &who);
                ctx.args.insert("addTargetResult".into(), false.into());
                let got = yt
                    .data(
                        "GET",
                        "",
                        &format!("channels?part=snippet,statistics&id={}", enc(&id)),
                        None,
                    )
                    .await?;
                let c = &got["items"][0];
                if c.is_null() {
                    return Err(format!("no YouTube channel {id}"));
                }
                let mut put = |k: &str, v: Value| drop(ctx.args.insert(k.into(), v));
                put("addTargetResult", true.into());
                put("targetUser", c["snippet"]["title"].clone());
                put("targetUserName", c["snippet"]["customUrl"].clone());
                put("targetUserId", id.as_str().into());
                put("targetUserType", "youtube".into());
                put("targetDescription", c["snippet"]["description"].clone());
                put(
                    "targetUserProfileImageUrl",
                    c["snippet"]["thumbnails"]["high"]["url"].clone(),
                );
                put("targetCreatedAt", c["snippet"]["publishedAt"].clone());
                put("targetSubscriberCount", c["statistics"]["subscriberCount"].clone());
                put("targetVideoCount", c["statistics"]["videoCount"].clone());
            }
            // Add Random Users
            99663 => self.random_seen(ctx, "youtube", (n(o, "count") as usize).max(1)),
            _ => self.unsupported(kind),
        }
        Ok(())
    }
}

/// YouTube's video categories a stream can be in: id and name, the picker's order.
pub const YOUTUBE_CATEGORIES: [(&str, &str); 14] = [
    ("20", "Gaming"),
    ("22", "People & Blogs"),
    ("24", "Entertainment"),
    ("10", "Music"),
    ("23", "Comedy"),
    ("27", "Education"),
    ("28", "Science & Technology"),
    ("17", "Sports"),
    ("26", "Howto & Style"),
    ("25", "News & Politics"),
    ("1", "Film & Animation"),
    ("2", "Autos & Vehicles"),
    ("15", "Pets & Animals"),
    ("19", "Travel & Events"),
];

/// The two patches of Chat Mode for a While: turn the mode on, and put it back as it was (a mode that
/// was already on stays on with its old value; Twitch needs the value to turn it on again).
pub fn chat_mode_patches(was: &Value, flag: &str, extra: Option<&str>, value: f64) -> (Value, Value) {
    let mut on = json!({ flag: true });
    let mut back = json!({ flag: was[flag].as_bool().unwrap_or(false) });
    if let Some(extra) = extra {
        on[extra] = (value.max(0.0).round() as u64).into();
        if was[flag].as_bool() == Some(true) && !was[extra].is_null() {
            back[extra] = was[extra].clone();
        }
    }
    (on, back)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A step that went on but did nothing says why: Test and MCP's run_subaction show the warning.
    #[test]
    fn a_run_keeps_the_first_warning() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        super::super::tests::run(async {
            let engine = super::super::tests::engine(json!({"actions": []}));
            let settings = json!({"platforms": [], "userLogin": "ana"})
                .as_object()
                .cloned()
                .unwrap();
            let (_, outcome) = engine.execute_one(99726, settings).await;
            assert!(outcome.error.is_none());
            assert!(outcome.warning.unwrap().contains("no platform ticked"));
        });
    }

    #[test]
    fn a_chat_mode_goes_on_and_comes_back_as_it_was() {
        let was = json!({"slow_mode": true, "slow_mode_wait_time": 10, "emote_mode": false});
        let (on, back) = chat_mode_patches(&was, "slow_mode", Some("slow_mode_wait_time"), 45.4);
        assert_eq!(on, json!({"slow_mode": true, "slow_mode_wait_time": 45}));
        assert_eq!(
            back,
            json!({"slow_mode": true, "slow_mode_wait_time": 10}),
            "it was on: it stays on, as it was"
        );
        let (on, back) = chat_mode_patches(&was, "emote_mode", None, 0.0);
        assert_eq!((on, back), (json!({"emote_mode": true}), json!({"emote_mode": false})));
        let (_, back) = chat_mode_patches(&json!({}), "slow_mode", Some("slow_mode_wait_time"), 30.0);
        assert_eq!(
            back,
            json!({"slow_mode": false}),
            "off before: off after, with no value"
        );
    }
}
