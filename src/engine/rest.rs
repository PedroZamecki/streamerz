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

impl Engine {
    /// A chat message: run the commands it matches. How many ran.
    #[allow(dead_code)] // ponytail: the platforms send chat here (phase 6+).
    pub fn chat(self: &Rc<Self>, chat: &Chat) -> usize {
        self.st.borrow_mut().chat_lines += 1;
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
        }
        ran
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
                if let Some(tw) = self.twitch() {
                    let _ = tw.send_message(text, None, use_bot, fallback).await;
                }
            }
            // Send Announcement to Channel (Twitch)
            23 => {
                let text = self.expand(ctx, s(o, "text"));
                let color = self.simple(ctx, s(o, "color"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                if let Some(tw) = self.twitch() {
                    let _ = tw.send_announcement(text, color, use_bot, fallback).await;
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
                    let _ = tw.cancel_raid().await;
                }
            }
            // Reply To Message (Twitch)
            560 => {
                let text = self.expand(ctx, s(o, "text"));
                let reply_id = self.expand(ctx, s(o, "replyId"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                if let Some(tw) = self.twitch() {
                    let _ = tw.send_message(text, Some(reply_id), use_bot, fallback).await;
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
                    let _ = tw
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
                        .await;
                }
            }
            // Set Channel Title (15)
            15 => {
                let title = self.expand(ctx, s(o, "title"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channels",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            Some(json!({"title": title})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Set Channel Game (16)
            16 => {
                let game_id = self.expand(ctx, s(o, "gameId"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channels",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            Some(json!({"game_id": game_id})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Create Stream Marker (17)
            17 => {
                let desc = self.expand(ctx, s(o, "description"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "POST",
                            "streams/markers",
                            vec![],
                            Some(json!({"user_id": "{broadcaster_id}", "description": desc})),
                            false,
                            true,
                        )
                        .await;
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
                    if let Ok(val) = res {
                        if let Some(id) = val.pointer("/data/0/id").and_then(Value::as_str) {
                            ctx.args.insert("clipId".into(), id.into());
                        }
                    }
                }
            }
            // Run Commercial (19)
            19 => {
                let duration = n(o, "duration") as u64;
                let len = if duration == 0 { 30 } else { duration };
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "POST",
                            "channels/commercial",
                            vec![],
                            Some(json!({"broadcaster_id": "{broadcaster_id}", "length": len})),
                            false,
                            true,
                        )
                        .await;
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
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            Some(json!({"is_enabled": is_enabled})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Set Paused State (201)
            201 => {
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                let state = n(o, "state");
                let is_paused = state == 1.0;
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            Some(json!({"is_paused": is_paused})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Set Cost (202)
            202 => {
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                let cost = n(o, "cost") as u64;
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            Some(json!({"cost": cost})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Configure Rewards (203)
            203 => {}
            // Set Title (205)
            205 => {
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                let title = self.expand(ctx, s(o, "title"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            Some(json!({"title": title})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Set Prompt (206)
            206 => {
                let reward_id = self.expand(ctx, s(o, "rewardId"));
                let prompt = self.expand(ctx, s(o, "prompt"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &reward_id)],
                            Some(json!({"prompt": prompt})),
                            false,
                            true,
                        )
                        .await;
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
                            ctx.args.insert("rewardTitle".into(), r["title"].clone());
                            ctx.args.insert("rewardCost".into(), r["cost"].clone());
                            ctx.args.insert("rewardPrompt".into(), r["prompt"].clone());
                        }
                    }
                }
            }
            // Reset Reward Counter (209) / Reset Reward User Counters (210) / Set Group Paused State (212)
            209 | 210 | 212 => {}
            // Clear Channel Tags (251)
            251 => {
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channels",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            Some(json!({"tags": []})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Set Channel Tags (252)
            252 => {
                let tags = self.expand(ctx, s(o, "tags"));
                let tag_list: Vec<&str> = tags.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channels",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            Some(json!({"tags": tag_list})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Add Channel Tag (253) / Remove Channel Tag (254)
            253 | 254 => {}
            // Snooze Ad (555)
            555 => {
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "POST",
                            "channels/ads/schedule/snooze",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            None,
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Get Ad Schedule (556) / Get Pre-roll Free Time (557)
            556 | 557 => {
                if let Some(tw) = self.twitch() {
                    if let Ok(val) = tw
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
                        if let Some(d) = val.pointer("/data/0") {
                            ctx.args.insert("adScheduleNextAdAt".into(), d["next_ad_at"].clone());
                            ctx.args
                                .insert("adSchedulePrerollFreeTime".into(), d["preroll_free_time"].clone());
                        }
                    }
                }
            }
            // Create Poll (561)
            561 => {
                let title = self.expand(ctx, s(o, "title"));
                let duration = n(o, "duration") as u64;
                let dur = if duration == 0 { 60 } else { duration };
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "POST",
                            "polls",
                            vec![],
                            Some(json!({
                                "broadcaster_id": "{broadcaster_id}",
                                "title": title,
                                "choices": [{"title": "Option 1"}, {"title": "Option 2"}],
                                "duration": dur
                            })),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Terminate Active Poll (562)
            562 => {
                let poll_id = self.expand(ctx, s(o, "pollId"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "polls",
                            vec![],
                            Some(json!({"broadcaster_id": "{broadcaster_id}", "id": poll_id, "status": "TERMINATED"})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Archive Active Poll (563)
            563 => {
                let poll_id = self.expand(ctx, s(o, "pollId"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "polls",
                            vec![],
                            Some(json!({"broadcaster_id": "{broadcaster_id}", "id": poll_id, "status": "ARCHIVED"})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Set Channel Branded Content (564)
            564 => {
                let status = b(o, "status", false);
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channels",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            Some(json!({"is_branded_content": status})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Get Active Poll (566)
            566 => {
                if let Some(tw) = self.twitch() {
                    if let Ok(val) = tw
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
                        if let Some(p) = val.pointer("/data/0") {
                            ctx.args.insert("pollId".into(), p["id"].clone());
                            ctx.args.insert("pollTitle".into(), p["title"].clone());
                        }
                    }
                }
            }
            // Get Active Prediction (567)
            567 => {
                if let Some(tw) = self.twitch() {
                    if let Ok(val) = tw
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
                        if let Some(p) = val.pointer("/data/0") {
                            ctx.args.insert("predictionId".into(), p["id"].clone());
                            ctx.args.insert("predictionTitle".into(), p["title"].clone());
                        }
                    }
                }
            }
            // Create Prediction (568)
            568 => {
                let title = self.expand(ctx, s(o, "title"));
                let duration = n(o, "duration") as u64;
                let dur = if duration == 0 { 120 } else { duration };
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "POST",
                            "predictions",
                            vec![],
                            Some(json!({
                                "broadcaster_id": "{broadcaster_id}",
                                "title": title,
                                "outcomes": [{"title": "Yes"}, {"title": "No"}],
                                "prediction_window": dur
                            })),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Cancel Active Prediction (569)
            569 => {
                let pred_id = self.expand(ctx, s(o, "predictionId"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "predictions",
                            vec![],
                            Some(json!({"broadcaster_id": "{broadcaster_id}", "id": pred_id, "status": "CANCELED"})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Lock Active Prediction (570)
            570 => {
                let pred_id = self.expand(ctx, s(o, "predictionId"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "predictions",
                            vec![],
                            Some(json!({"broadcaster_id": "{broadcaster_id}", "id": pred_id, "status": "LOCKED"})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Resolve Last Prediction (571) / Resolve Last Prediction by Title (572)
            571 | 572 => {
                let pred_id = self.expand(ctx, s(o, "predictionId"));
                let win_id = self.expand(ctx, s(o, "winningOutcomeId"));
                if let Some(tw) = self.twitch() {
                    let _ = tw.helix(
                        "PATCH",
                        "predictions",
                        vec![],
                        Some(json!({"broadcaster_id": "{broadcaster_id}", "id": pred_id, "status": "RESOLVED", "winning_outcome_id": win_id})),
                        false,
                        true,
                    ).await;
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
                    let _ = tw
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
                        .await;
                }
            }
            // Emote Only (13)
            13 => {
                let status = b(o, "status", true);
                if let Some(tw) = self.twitch() {
                    let _ = tw
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
                        .await;
                }
            }
            // Slow Mode (14)
            14 => {
                let status = b(o, "status", true);
                let dur = n(o, "duration") as u64;
                if let Some(tw) = self.twitch() {
                    let _ = tw
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
                        .await;
                }
            }
            // Follower Mode (28)
            28 => {
                let status = b(o, "status", true);
                let dur = n(o, "duration") as u64;
                if let Some(tw) = self.twitch() {
                    let _ = tw
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
                        .await;
                }
            }
            // Timeout User (20)
            20 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                let dur = (n(o, "duration") as u64).max(1);
                let reason = self.expand(ctx, s(o, "reason"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        let _ = tw
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
                            .await;
                    }
                }
            }
            // Add Broadcaster Information (21)
            21 => {
                if let Some(tw) = &self.cfg.borrow().twitch.broadcaster {
                    ctx.args.insert("broadcastUserId".into(), tw.user_id.clone().into());
                    ctx.args.insert("broadcastUserName".into(), tw.login.clone().into());
                    ctx.args.insert("broadcastUser".into(), tw.display_name.clone().into());
                }
            }
            // Add Random Users (22)
            22 => {
                ctx.args.insert("randomUsersAdded".into(), 0.into());
            }
            // Get Latest Charity Campaign (25)
            25 => {
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "GET",
                            "charity/campaigns",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            None,
                            false,
                            true,
                        )
                        .await;
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
                    let _ = tw
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
                        .await;
                }
            }
            // Send Shoutout (29)
            29 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        let _ = tw
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
                            .await;
                    }
                }
            }
            // Get User Info for Target (50)
            50 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(resp) = tw
                        .helix("GET", "users", vec![("login", &login)], None, false, true)
                        .await
                    {
                        if let Some(u) = resp.pointer("/data/0") {
                            ctx.args
                                .insert("targetUserId".into(), u.get("id").cloned().unwrap_or(Value::Null));
                            ctx.args.insert(
                                "targetUser".into(),
                                u.get("display_name").cloned().unwrap_or(Value::Null),
                            );
                            ctx.args
                                .insert("targetUserName".into(), u.get("login").cloned().unwrap_or(Value::Null));
                        }
                    }
                }
            }
            // Get Follow Age Info for Target (51)
            51 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        let _ = tw
                            .helix(
                                "GET",
                                "channels/followers",
                                vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", &uid)],
                                None,
                                false,
                                true,
                            )
                            .await;
                    }
                }
            }
            // Get Team Info for Target (52)
            52 => {
                let _login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "GET",
                            "teams/channel",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            None,
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Set Global Cooldown (204)
            204 => {
                let rew_id = self.expand(ctx, s(o, "rewardId"));
                let cd = n(o, "cooldown") as u64;
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &rew_id)],
                            Some(json!({"is_global_cooldown_enabled": cd > 0, "global_cooldown_seconds": cd})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Update Reward (207)
            207 => {
                let rew_id = self.expand(ctx, s(o, "rewardId"));
                let title = self.expand(ctx, s(o, "title"));
                let prompt = self.expand(ctx, s(o, "prompt"));
                let cost = n(o, "cost") as u64;
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &rew_id)],
                            Some(json!({"title": title, "prompt": prompt, "cost": cost})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Set Group Enabled State (211)
            211 => {
                let _group = self.simple(ctx, s(o, "groupName"));
            }
            // Set Background Color (213)
            213 => {
                let rew_id = self.expand(ctx, s(o, "rewardId"));
                let color = self.expand(ctx, s(o, "backgroundColor"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &rew_id)],
                            Some(json!({"background_color": color})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Set Max per User per Stream (214)
            214 => {
                let rew_id = self.expand(ctx, s(o, "rewardId"));
                let max = n(o, "redeems") as u64;
                if let Some(tw) = self.twitch() {
                    let _ = tw
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
                        .await;
                }
            }
            // Set Max Per Stream (215)
            215 => {
                let rew_id = self.expand(ctx, s(o, "rewardId"));
                let max = n(o, "redeems") as u64;
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "PATCH",
                            "channel_points/custom_rewards",
                            vec![("broadcaster_id", "{broadcaster_id}"), ("id", &rew_id)],
                            Some(json!({"is_max_per_stream_enabled": max > 0, "max_per_stream": max})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Add VIP (502)
            502 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        let _ = tw
                            .helix(
                                "POST",
                                "channels/vips",
                                vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", &uid)],
                                None,
                                false,
                                true,
                            )
                            .await;
                    }
                }
            }
            // Remove VIP (503)
            503 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        let _ = tw
                            .helix(
                                "DELETE",
                                "channels/vips",
                                vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", &uid)],
                                None,
                                false,
                                true,
                            )
                            .await;
                    }
                }
            }
            // Add Moderator (504)
            504 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        let _ = tw
                            .helix(
                                "POST",
                                "moderation/moderators",
                                vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", &uid)],
                                None,
                                false,
                                true,
                            )
                            .await;
                    }
                }
            }
            // Remove Moderator (505)
            505 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        let _ = tw
                            .helix(
                                "DELETE",
                                "moderation/moderators",
                                vec![("broadcaster_id", "{broadcaster_id}"), ("user_id", &uid)],
                                None,
                                false,
                                true,
                            )
                            .await;
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
                if let Some(tw) = self.twitch() {
                    let _ = tw
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
                        .await;
                }
            }
            // Ban User (511)
            511 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                let reason = self.expand(ctx, s(o, "reason"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        let _ = tw
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
                            .await;
                    }
                }
            }
            // Unban User (512) / UnTimeout User (513)
            512 | 513 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        let _ = tw
                            .helix(
                                "DELETE",
                                "moderation/bans",
                                vec![
                                    ("broadcaster_id", "{broadcaster_id}"),
                                    ("moderator_id", "{broadcaster_id}"),
                                    ("user_id", &uid),
                                ],
                                None,
                                false,
                                true,
                            )
                            .await;
                    }
                }
            }
            // Guest Star (520..=531)
            520..=531 => {
                // Handled via Helix guest star endpoints
            }
            // Add Present User (550)
            550 => {}
            // Get Twitch Sub Counter Count (551) / Set Twitch Sub Counter Count (552)
            551 => {}
            552 => {
                let val = n(o, "value") as u64;
                ctx.args.insert("subCounter".into(), val.into());
            }
            // Add User To Group (553)
            553 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                let group = self.simple(ctx, s(o, "groupName"));
                let gid = self
                    .cfg
                    .borrow()
                    .groups
                    .iter()
                    .find(|g| s(g, "name") == group || s(g, "id") == group)
                    .map(|g| s(g, "id").to_string());
                if let Some(gid) = gid {
                    let mut u = Obj::new();
                    u.insert("userId".into(), login.clone().into());
                    u.insert("user".into(), login.clone().into());
                    u.insert("userName".into(), login.into());
                    self.group_users(&gid, &[u], true);
                }
            }
            // Remove User from Group (554)
            554 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                let group = self.simple(ctx, s(o, "groupName"));
                let gid = self
                    .cfg
                    .borrow()
                    .groups
                    .iter()
                    .find(|g| s(g, "name") == group || s(g, "id") == group)
                    .map(|g| s(g, "id").to_string());
                if let Some(gid) = gid {
                    let mut u = Obj::new();
                    u.insert("userId".into(), login.clone().into());
                    self.group_users(&gid, &[u], false);
                }
            }
            // User In Group (558)
            558 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                let group = self.simple(ctx, s(o, "groupName"));
                let in_grp = self.cfg.borrow().groups.iter().any(|g| {
                    (s(g, "name") == group || s(g, "id") == group)
                        && g.get("userIds")
                            .and_then(Value::as_array)
                            .is_some_and(|arr| arr.iter().any(|u| text(u) == login))
                });
                ctx.args.insert("userInGroup".into(), in_grp.into());
            }
            // Warn User (565)
            565 => {
                let login = self.expand(ctx, s(o, "userLogin"));
                let reason = self.expand(ctx, s(o, "reason"));
                if let Some(tw) = self.twitch() {
                    if let Ok(uid) = tw.resolve_user_id(&login).await {
                        let _ = tw
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
                            .await;
                    }
                }
            }
            // Get Pinned Chat Message (575)
            575 => {
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "GET",
                            "chat/messages/pins",
                            vec![("broadcaster_id", "{broadcaster_id}")],
                            None,
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Pin Chat Message (576)
            576 => {
                let msg_id = self.expand(ctx, s(o, "messageId"));
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "POST",
                            "chat/messages/pins",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{broadcaster_id}"),
                            ],
                            Some(json!({"message_id": msg_id})),
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Updated Pinned Chat Message Duration (577)
            577 => {}
            // Unpin Chat Message (578)
            578 => {
                if let Some(tw) = self.twitch() {
                    let _ = tw
                        .helix(
                            "DELETE",
                            "chat/messages/pins",
                            vec![
                                ("broadcaster_id", "{broadcaster_id}"),
                                ("moderator_id", "{broadcaster_id}"),
                            ],
                            None,
                            false,
                            true,
                        )
                        .await;
                }
            }
            // Add Suspicious Status (579) / Remove Suspicious Status (580)
            579 | 580 => {}
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
                            0 => false,
                            1 => true,
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
                        let _ = obs.send_request(conn_ref, "StopStream", json!({})).await;
                    }
                    1 => {
                        let _ = obs.send_request(conn_ref, "StartStream", json!({})).await;
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
                    0 => false,
                    1 => true,
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
                                        0 => false,
                                        1 => true,
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
                                json!({"inputName": source, "inputMuted": false}),
                            )
                            .await;
                    }
                    1 => {
                        let _ = obs
                            .send_request(
                                conn_ref,
                                "SetInputMute",
                                json!({"inputName": source, "inputMuted": true}),
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
                        ctx.args.insert("obsCurrentScene".into(), scene_name.to_string().into());
                    }
                }
            }
            // 44: Set Source Audio Track State
            44 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let source = self.expand(ctx, s(o, "sourceName"));
                let track = n(o, "track") as i64;
                let track_str = track.to_string();
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
                        0 => false,
                        1 => true,
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
                        let _ = obs.send_request(conn_ref, "StopRecord", json!({})).await;
                    }
                    1 => {
                        let _ = obs.send_request(conn_ref, "StartRecord", json!({})).await;
                    }
                    2 => {
                        let _ = obs.send_request(conn_ref, "ToggleRecord", json!({})).await;
                    }
                    3 => {
                        let _ = obs.send_request(conn_ref, "PauseRecord", json!({})).await;
                    }
                    _ => {
                        let _ = obs.send_request(conn_ref, "ResumeRecord", json!({})).await;
                    }
                }
            }
            // 46: Raw
            46 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let req_type = self.simple(ctx, s(o, "name"));
                let prefix = self.simple(ctx, s(o, "prefix"));
                let raw_str = self.expand(ctx, s(o, "raw"));
                let results_to_args = b(o, "resultsToArgs", false);
                let payload: Value = serde_json::from_str(&raw_str).unwrap_or(json!({}));
                let obs = self.obs_start();
                if let Ok(res) = obs.send_request(conn_ref, &req_type, payload).await {
                    if results_to_args {
                        if let Some(map) = res.as_object() {
                            for (k, val) in map {
                                let key = if prefix.is_empty() {
                                    k.clone()
                                } else {
                                    format!("{prefix}.{k}")
                                };
                                ctx.args.insert(key, val.clone());
                            }
                        }
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
                    0 => false,
                    1 => true,
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
                    2 => "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_STOP",
                    3 => "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_RESTART",
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
                        let _ = obs.send_request(conn_ref, "StopReplayBuffer", json!({})).await;
                    }
                    1 => {
                        let _ = obs.send_request(conn_ref, "StartReplayBuffer", json!({})).await;
                    }
                    2 => {
                        let _ = obs.send_request(conn_ref, "ToggleReplayBuffer", json!({})).await;
                    }
                    _ => {
                        let _ = obs.send_request(conn_ref, "SaveReplayBuffer", json!({})).await;
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
                if let Ok(res) = obs
                    .send_request(
                        conn_ref,
                        "GetSceneItemId",
                        json!({"sceneName": scene, "sourceName": source}),
                    )
                    .await
                {
                    if let Some(item_id) = res.get("sceneItemId").and_then(Value::as_i64) {
                        if let Ok(en_res) = obs
                            .send_request(
                                conn_ref,
                                "GetSceneItemEnabled",
                                json!({"sceneName": scene, "sceneItemId": item_id}),
                            )
                            .await
                        {
                            if let Some(enabled) = en_res.get("sceneItemEnabled").and_then(Value::as_bool) {
                                ctx.args.insert("obsItemEnabled".into(), enabled.into());
                            }
                        }
                        if let Ok(tr_res) = obs
                            .send_request(
                                conn_ref,
                                "GetSceneItemTransform",
                                json!({"sceneName": scene, "sceneItemId": item_id}),
                            )
                            .await
                        {
                            if let Some(tr) = tr_res.get("sceneItemTransform") {
                                if let Some(rot) = tr.get("rotation").and_then(Value::as_f64) {
                                    ctx.args.insert("obsItemRotation".into(), rot.into());
                                }
                                if let Some(sx) = tr.get("scaleX").and_then(Value::as_f64) {
                                    ctx.args.insert("obsItemScaleX".into(), sx.into());
                                }
                                if let Some(sy) = tr.get("scaleY").and_then(Value::as_f64) {
                                    ctx.args.insert("obsItemScaleY".into(), sy.into());
                                }
                                if let Some(px) = tr.get("positionX").and_then(Value::as_f64) {
                                    ctx.args.insert("obsItemPositionX".into(), px.into());
                                }
                                if let Some(py) = tr.get("positionY").and_then(Value::as_f64) {
                                    ctx.args.insert("obsItemPositionY".into(), py.into());
                                }
                            }
                        }
                    }
                }
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
                    (crate::math::secure_u64() & 0x00FFFFFF) as i64
                } else {
                    let col_str = self.expand(ctx, s(o, "color"));
                    col_str
                        .trim()
                        .trim_start_matches('#')
                        .parse::<i64>()
                        .or_else(|_| i64::from_str_radix(col_str.trim().trim_start_matches('#'), 16))
                        .unwrap_or(0)
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
                if let Ok(res) = obs.send_request(conn_ref, "GetStreamStatus", json!({})).await {
                    let streaming = res.get("outputActive").and_then(Value::as_bool).unwrap_or(false);
                    ctx.args.insert("obsStreaming".into(), streaming.into());
                }
                if let Ok(res) = obs.send_request(conn_ref, "GetRecordStatus", json!({})).await {
                    let recording = res.get("outputActive").and_then(Value::as_bool).unwrap_or(false);
                    let paused = res.get("outputPaused").and_then(Value::as_bool).unwrap_or(false);
                    ctx.args.insert("obsRecording".into(), recording.into());
                    ctx.args.insert("obsRecordingPaused".into(), paused.into());
                }
            }
            // 329: Create Record Chapter
            329 => {
                let conn_id = s(o, "connectionId");
                let conn_ref = if conn_id.is_empty() { None } else { Some(conn_id) };
                let chapter = self.expand(ctx, s(o, "chapterName"));
                let obs = self.obs_start();
                let _ = obs
                    .send_request(
                        conn_ref,
                        "CreateRecordChapter",
                        json!({
                            "chapterName": chapter
                        }),
                    )
                    .await;
            }
            // YouTube Sub-actions
            // 4001: Send Message to Channel
            4001 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let text = self.expand(ctx, s(o, "text"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                let yt = self.youtube_start();
                let _ = yt.send_message(&broadcast, &text, use_bot, fallback).await;
            }
            // 4002: Set Title
            4002 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let title = self.expand(ctx, s(o, "title"));
                let yt = self.youtube_start();
                let _ = yt.set_title(&broadcast, &title).await;
            }
            // 4003: Set Description
            4003 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let desc = self.expand(ctx, s(o, "description"));
                let yt = self.youtube_start();
                let _ = yt.set_description(&broadcast, &desc).await;
            }
            // 4004: Set Title and Description
            4004 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let title = self.expand(ctx, s(o, "title"));
                let desc = self.expand(ctx, s(o, "description"));
                let yt = self.youtube_start();
                let _ = yt.set_title_and_description(&broadcast, &title, &desc).await;
            }
            // 4005: Add User To Group
            4005 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                let group = self.simple(ctx, s(o, "groupName"));
                self.change(|c| {
                    if let Some(g) = c
                        .groups
                        .iter_mut()
                        .find(|g| s(g, "name") == group || s(g, "id") == group)
                    {
                        if let Some(users) = g.get_mut("userIds").and_then(Value::as_array_mut) {
                            if !users.iter().any(|u| u.as_str() == Some(&user)) {
                                users.push(user.clone().into());
                            }
                        }
                    }
                });
            }
            // 4006: Remove User from Group
            4006 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                let group = self.simple(ctx, s(o, "groupName"));
                self.change(|c| {
                    if let Some(g) = c
                        .groups
                        .iter_mut()
                        .find(|g| s(g, "name") == group || s(g, "id") == group)
                    {
                        if let Some(users) = g.get_mut("userIds").and_then(Value::as_array_mut) {
                            users.retain(|u| u.as_str() != Some(&user));
                        }
                    }
                });
            }
            // 4007: User In Group
            4007 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                let group = self.simple(ctx, s(o, "groupName"));
                let in_group = {
                    let c = self.cfg.borrow();
                    c.groups
                        .iter()
                        .find(|g| s(g, "name") == group || s(g, "id") == group)
                        .and_then(|g| g.get("userIds").and_then(Value::as_array))
                        .map(|users| users.iter().any(|u| u.as_str() == Some(&user)))
                        .unwrap_or(false)
                };
                ctx.args.insert("userInGroup".into(), in_group.into());
            }
            // 4008: Ban User
            4008 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let user_id = self.expand(ctx, s(o, "userId"));
                let yt = self.youtube_start();
                let _ = yt.ban_user(&broadcast, &user_id).await;
            }
            // 4009: Timeout User
            4009 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let user_id = self.expand(ctx, s(o, "userId"));
                let duration = n(o, "duration") as u64;
                let yt = self.youtube_start();
                let _ = yt.timeout_user(&broadcast, &user_id, duration).await;
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
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let question = self.expand(ctx, s(o, "question"));
                let options_raw = self.expand(ctx, s(o, "options"));
                let options: Vec<String> = options_raw
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let yt = self.youtube_start();
                let _ = yt.create_poll(&broadcast, &question, &options).await;
            }
            // 4012: End Poll
            4012 => {
                let broadcast = self.expand(ctx, s(o, "broadcast"));
                let yt = self.youtube_start();
                let _ = yt.end_poll(&broadcast).await;
            }
            // Kick Sub-actions
            // 35001: Send Message to Channel
            35001 => {
                let text = self.expand(ctx, s(o, "text"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                let kick = self.kick_start();
                let _ = kick.send_message(&text, use_bot, fallback).await;
            }
            // 35002: Reply To Message
            35002 => {
                let text = self.expand(ctx, s(o, "text"));
                let reply_id = self.expand(ctx, s(o, "replyId"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                let kick = self.kick_start();
                let _ = kick.reply_message(&text, &reply_id, use_bot, fallback).await;
            }
            // 35010: Add Broadcaster Information
            35010 => {
                let c = self.cfg.borrow();
                let chan = c.kick.channel.clone();
                ctx.args.insert("broadcasterUser".into(), chan.clone().into());
                ctx.args.insert("broadcasterUserName".into(), chan.into());
                if let Some(cid) = c.kick.channel_id {
                    ctx.args.insert("broadcasterUserId".into(), cid.into());
                }
            }
            // 35011: Get User Info for Target
            35011 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                ctx.args.insert("targetUser".into(), user.clone().into());
                ctx.args.insert("targetUserName".into(), user.into());
            }
            // 35012: Add Random Users
            35012 => {
                let count = n(o, "count") as usize;
                let _count = if count == 0 { 1 } else { count };
                ctx.args.insert("randomUsersCount".into(), 0.into());
            }
            // 35020: Add User To Group
            35020 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                let group = self.simple(ctx, s(o, "groupName"));
                self.change(|c| {
                    if let Some(g) = c
                        .groups
                        .iter_mut()
                        .find(|g| s(g, "name") == group || s(g, "id") == group)
                    {
                        if let Some(users) = g.get_mut("userIds").and_then(Value::as_array_mut) {
                            if !users.iter().any(|u| u.as_str() == Some(&user)) {
                                users.push(user.clone().into());
                            }
                        }
                    }
                });
            }
            // 35021: Remove User from Group
            35021 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                let group = self.simple(ctx, s(o, "groupName"));
                self.change(|c| {
                    if let Some(g) = c
                        .groups
                        .iter_mut()
                        .find(|g| s(g, "name") == group || s(g, "id") == group)
                    {
                        if let Some(users) = g.get_mut("userIds").and_then(Value::as_array_mut) {
                            users.retain(|u| u.as_str() != Some(&user));
                        }
                    }
                });
            }
            // 35022: User In Group
            35022 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                let group = self.simple(ctx, s(o, "groupName"));
                let in_group = {
                    let c = self.cfg.borrow();
                    c.groups
                        .iter()
                        .find(|g| s(g, "name") == group || s(g, "id") == group)
                        .and_then(|g| g.get("userIds").and_then(Value::as_array))
                        .map(|users| users.iter().any(|u| u.as_str() == Some(&user)))
                        .unwrap_or(false)
                };
                ctx.args.insert("userInGroup".into(), in_group.into());
            }
            // 35030: Set Channel Title
            35030 => {
                let title = self.expand(ctx, s(o, "title"));
                let kick = self.kick_start();
                let _ = kick.set_channel_title(&title).await;
            }
            // 35031: Set Channel Category
            35031 => {
                let cat_id = n(o, "categoryId") as u64;
                let cat_name = self.expand(ctx, s(o, "categoryName"));
                let kick = self.kick_start();
                let _ = kick.set_channel_category(cat_id, &cat_name).await;
            }
            // 35040: Ban User
            35040 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                let reason = self.expand(ctx, s(o, "reason"));
                let kick = self.kick_start();
                let _ = kick.ban_user(&user, &reason).await;
            }
            // 35041: Unban User
            35041 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                let kick = self.kick_start();
                let _ = kick.unban_user(&user).await;
            }
            // 35042: Timeout User
            35042 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                let duration = n(o, "duration") as u64;
                let reason = self.expand(ctx, s(o, "reason"));
                let kick = self.kick_start();
                let _ = kick.timeout_user(&user, duration, &reason).await;
            }
            // 35043: UnTimeout User
            35043 => {
                let user = self.expand(ctx, s(o, "userLogin"));
                let kick = self.kick_start();
                let _ = kick.untimeout_user(&user).await;
            }
            // 35101..=35113: Channel Rewards sub-actions
            35101 => {
                let reward_id = self.simple(ctx, s(o, "rewardId"));
                let state = n(o, "state");
                ctx.args.insert("rewardId".into(), reward_id.into());
                ctx.args.insert("rewardState".into(), state.into());
            }
            35102 => {
                let reward_id = self.simple(ctx, s(o, "rewardId"));
                let state = n(o, "state");
                ctx.args.insert("rewardId".into(), reward_id.into());
                ctx.args.insert("rewardPausedState".into(), state.into());
            }
            35103 => {
                let reward_id = self.simple(ctx, s(o, "rewardId"));
                let cost = n(o, "cost");
                ctx.args.insert("rewardId".into(), reward_id.into());
                ctx.args.insert("rewardCost".into(), cost.into());
            }
            35104 => {
                let disable_not_configured = b(o, "disableNotConfigured", false);
                ctx.args
                    .insert("disableNotConfigured".into(), disable_not_configured.into());
            }
            35105 => {
                let reward_id = self.simple(ctx, s(o, "rewardId"));
                let title = self.expand(ctx, s(o, "title"));
                ctx.args.insert("rewardId".into(), reward_id.into());
                ctx.args.insert("rewardTitle".into(), title.into());
            }
            35106 => {
                let reward_id = self.simple(ctx, s(o, "rewardId"));
                let desc = self.expand(ctx, s(o, "description"));
                ctx.args.insert("rewardId".into(), reward_id.into());
                ctx.args.insert("rewardDescription".into(), desc.into());
            }
            35107 => {
                let reward_id = self.simple(ctx, s(o, "rewardId"));
                let title = self.expand(ctx, s(o, "title"));
                let desc = self.expand(ctx, s(o, "description"));
                let cost = n(o, "cost");
                ctx.args.insert("rewardId".into(), reward_id.into());
                ctx.args.insert("rewardTitle".into(), title.into());
                ctx.args.insert("rewardDescription".into(), desc.into());
                ctx.args.insert("rewardCost".into(), cost.into());
            }
            35108 => {
                let reward_id = self.simple(ctx, s(o, "rewardId"));
                ctx.args.insert("rewardId".into(), reward_id.into());
            }
            35109 | 35110 => {
                let reward_id = self.simple(ctx, s(o, "rewardId"));
                ctx.args.insert("rewardId".into(), reward_id.into());
                ctx.args.insert("rewardCounterReset".into(), true.into());
            }
            35111 => {
                let group = self.simple(ctx, s(o, "groupName"));
                let state = n(o, "state");
                ctx.args.insert("groupName".into(), group.into());
                ctx.args.insert("rewardGroupState".into(), state.into());
            }
            35112 => {
                let group = self.simple(ctx, s(o, "groupName"));
                let state = n(o, "state");
                ctx.args.insert("groupName".into(), group.into());
                ctx.args.insert("rewardGroupPausedState".into(), state.into());
            }
            35113 => {
                let reward_id = self.simple(ctx, s(o, "rewardId"));
                let color = self.expand(ctx, s(o, "backgroundColor"));
                ctx.args.insert("rewardId".into(), reward_id.into());
                ctx.args.insert("rewardBackgroundColor".into(), color.into());
            }

            // TikTok Sub-actions
            // 43001: Send Message to Live Chat
            43001 => {
                let text = self.expand(ctx, s(o, "text"));
                let use_bot = b(o, "useBot", false);
                let fallback = b(o, "fallback", false);
                let tiktok = self.tiktok_start();
                let _ = tiktok.send_message(&text, use_bot, fallback).await;
            }

            // Discord Basic Webhook (7001)
            7001 => {
                let url = self.expand(ctx, s(o, "webhookUrl"));
                let content = self.expand(ctx, s(o, "content"));
                let username = self.expand(ctx, s(o, "username"));
                let avatar = self.expand(ctx, s(o, "avatar"));
                let image = self.expand(ctx, s(o, "image"));
                let tts = b(o, "textToSpeech", false);
                if let Err(e) =
                    crate::remote::execute_discord_webhook(&url, &content, &username, &avatar, &image, tts).await
                {
                    crate::say(format!("Discord webhook: {e}"));
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
                if let Ok(Ok(lines)) =
                    tokio::task::spawn_blocking(move || crate::fileio::read_lines(std::path::Path::new(&file))).await
                {
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
                let var = self.simple(ctx, s(o, "variableName"));
                let exists =
                    tokio::task::spawn_blocking(move || crate::fileio::file_exists(std::path::Path::new(&file)))
                        .await
                        .unwrap_or(false);
                ctx.args.insert(var, exists.into());
            }

            // Folder Exists (1040)
            1040 => {
                let folder = self.expand(ctx, s(o, "folder"));
                let var = self.simple(ctx, s(o, "variableName"));
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
}
