//! The engine, continued: chat commands, timed actions, user groups, and the sub-actions that reach
//! outside (Fetch URL, UDP Broadcast, Run a Program).

use super::{b, n, new_state, s, Ctx, Engine};
use crate::commands::{self, Chat};
use crate::model::Obj;
use crate::vars::{self, auto_type, text};
use serde_json::Value;
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
