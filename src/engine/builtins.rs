//! Built-in chat commands and text replies, on every platform. They are ordinary commands (aliases,
//! permissions, cooldowns, platforms and on/off edited like any other) with two streamerz-only
//! fields, kept on export and ignored by Streamer.bot:
//! - `response`: a text reply (variables work), sent where the command came from, or to every
//!   connected platform with `replyAll`;
//! - `builtin`: `commands`, `command`, `so`, `uptime` or `title`, handled here.

use super::{Ctx, Engine};
use crate::commands::Chat;
use crate::model::Obj;
use serde_json::{json, Value};
use std::rc::Rc;

/// Command sources (Streamer.bot's bits) by platform.
pub const SOURCES: [(&str, u64); 4] = [
    ("twitch", 1),
    ("youtube", 1024),
    ("kick", 2_097_152),
    ("tiktok", 4_194_304),
];
const ALL: u64 = 1 | 1024 | 2_097_152 | 4_194_304;

fn platform_name(p: &str) -> &'static str {
    match p {
        "twitch" => "Twitch",
        "youtube" => "YouTube",
        "kick" => "Kick",
        _ => "TikTok",
    }
}

/// A channel's address on a platform.
fn channel_url(p: &str, user: &str) -> String {
    let u = user.trim_start_matches('@');
    match p {
        "twitch" => format!("https://twitch.tv/{}", u.to_lowercase()),
        "youtube" => format!("https://youtube.com/@{u}"),
        "kick" => format!("https://kick.com/{}", u.to_lowercase()),
        _ => format!("https://tiktok.com/@{u}"),
    }
}

/// "2h 5m", "45m", "30s".
pub fn duration(secs: i64) -> String {
    let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    match (h, m) {
        (0, 0) => format!("{s}s"),
        (0, _) => format!("{m}m"),
        _ => format!("{h}h {m}m"),
    }
}

/// Platform names ("twitch, kick", "all") as command source bits; None if one is unknown.
pub fn sources_from(list: &str) -> Option<u64> {
    let mut bits = 0;
    for w in list.split([',', ' ']).map(str::trim).filter(|w| !w.is_empty()) {
        let w = w.to_lowercase();
        bits |= match w.as_str() {
            "all" | "every" | "everywhere" => ALL,
            "yt" => 1024,
            "tt" => 4_194_304,
            _ => SOURCES.iter().find(|(p, _)| *p == w)?.1,
        };
    }
    (bits != 0).then_some(bits)
}

/// The platforms in source bits, as "twitch, kick".
pub fn sources_text(bits: u64) -> String {
    let v: Vec<&str> = SOURCES.iter().filter(|(_, b)| bits & b != 0).map(|(p, _)| *p).collect();
    if v.len() == SOURCES.len() {
        "all platforms".into()
    } else {
        v.join(", ")
    }
}

/// The commands made on first run.
pub fn defaults() -> Vec<Obj> {
    let make = |name: &str, alias: &str, builtin: &str, mods: bool, response: &str, cooldown: u64| {
        let groups: Vec<&str> = if mods { vec!["Moderators"] } else { vec![] };
        json!({"permittedUsers": [], "permittedGroups": groups, "id": super::uuid(), "name": name,
            "enabled": true, "include": true, "mode": 0, "command": alias, "regexExplicitCapture": false,
            "location": 0, "ignoreBotAccount": true, "ignoreInternal": true, "sources": ALL,
            "persistCounter": false, "persistUserCounter": false, "caseSensitive": false,
            "globalCooldown": 0, "userCooldown": cooldown, "group": "Built-in", "grantType": 0,
            "builtin": builtin, "response": response, "replyAll": false})
        .as_object()
        .cloned()
        .unwrap_or_default()
    };
    vec![
        make("Commands", "!commands", "commands", false, "Commands: %commands%", 10),
        make("Manage commands", "!command", "command", true, "", 0),
        make(
            "Shoutout",
            "!so",
            "so",
            true,
            "Go check out %targetUser% at %targetUrl% !",
            0,
        ),
        make("Uptime", "!uptime", "uptime", false, "Live for %uptime%.", 10),
        make("Title", "!title", "title", false, "Title: %title%", 10),
    ]
}

impl Engine {
    /// Add the built-in commands, once (deleted ones stay deleted).
    pub fn add_builtins(self: &Rc<Self>) {
        if self.cfg.borrow().builtins {
            return;
        }
        self.change(|c| {
            for d in defaults() {
                let alias = crate::commands::aliases(&d).first().map(|a| a.to_lowercase());
                let taken = c.commands.iter().any(|x| {
                    crate::commands::aliases(x)
                        .iter()
                        .any(|a| Some(a.to_lowercase()) == alias)
                });
                if !taken {
                    c.commands.push(d);
                }
            }
            c.builtins = true;
        });
    }

    /// After a command ran: its built-in behaviour or text reply. `args` are the command's.
    pub(super) fn command_reply(self: &Rc<Self>, c: &Obj, args: &Obj, chat: &Chat) -> bool {
        let builtin = c.get("builtin").and_then(Value::as_str).unwrap_or("").to_string();
        let response = c.get("response").and_then(Value::as_str).unwrap_or("").to_string();
        if builtin.is_empty() && response.trim().is_empty() {
            return false;
        }
        let all = c.get("replyAll").and_then(Value::as_bool).unwrap_or(false);
        let (e, chat, mut args) = (self.clone(), chat.clone(), args.clone());
        tokio::task::spawn_local(async move {
            let reply = match builtin.as_str() {
                "" => Some(response),
                b => e.builtin(b, &response, &chat, &mut args).await,
            };
            if let Some(r) = reply.filter(|r| !r.trim().is_empty()) {
                let text = e.expand(&Ctx { args }, &r);
                e.send_chat(if all { "all" } else { &chat.platform }, &text, true);
            }
        });
        true
    }

    /// Run a built-in; the reply (the command's response with the built-in's variables set).
    async fn builtin(self: &Rc<Self>, which: &str, response: &str, chat: &Chat, args: &mut Obj) -> Option<String> {
        let input = args
            .get("rawInput")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let staff = chat.broadcaster || chat.moderator;
        let p = chat.platform.as_str();
        match which {
            "commands" => {
                let bit = SOURCES.iter().find(|(x, _)| *x == p).map_or(0, |x| x.1);
                let mut list: Vec<String> = self
                    .cfg
                    .borrow()
                    .commands
                    .iter()
                    .filter(|c| c.get("enabled").and_then(Value::as_bool).unwrap_or(true))
                    .filter(|c| c.get("include").and_then(Value::as_bool).unwrap_or(true))
                    .filter(|c| c.get("sources").and_then(Value::as_u64).unwrap_or(1) & bit != 0)
                    .filter(|c| crate::commands::permitted(c, chat, &|_| false))
                    .filter_map(|c| crate::commands::aliases(c).first().map(|a| a.to_string()))
                    .collect();
                list.sort_by_key(|a| a.to_lowercase());
                args.insert("commands".into(), list.join(", ").into());
                Some(response.to_string())
            }
            "command" => Some(self.manage(&input, staff)),
            "so" => {
                let target = input
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_start_matches('@')
                    .to_string();
                if target.is_empty() {
                    return Some("Usage: !so <user>".into());
                }
                if p == "twitch" {
                    if let Some(tw) = self.twitch() {
                        // Twitch's own shoutout too (a cooldown or offline stream may refuse it).
                        if let Ok(id) = tw.resolve_user_id(&target).await {
                            let q = vec![
                                ("from_broadcaster_id", "{broadcaster_id}"),
                                ("to_broadcaster_id", id.as_str()),
                                ("moderator_id", "{moderator_id}"),
                            ];
                            if let Err(err) = tw.helix("POST", "chat/shoutouts", q, None, false, true).await {
                                crate::error(format!("Twitch shoutout: {err}"));
                            }
                        }
                    }
                }
                args.insert("targetUser".into(), target.as_str().into());
                args.insert("targetUrl".into(), channel_url(p, &target).into());
                Some(response.to_string())
            }
            "uptime" => {
                let since = self.live_since(p).await;
                args.insert("platformName".into(), platform_name(p).into());
                match since {
                    Ok(Some(t)) => {
                        let now = crate::fmt::unix_now().as_secs() as i64;
                        args.insert("uptime".into(), duration(now - t).into());
                        Some(response.to_string())
                    }
                    Ok(None) => Some(format!("The {} stream is offline.", platform_name(p))),
                    Err(err) => Some(format!("Can't tell the uptime: {err}")),
                }
            }
            "title" => {
                if !input.is_empty() && staff {
                    return Some(match self.set_title(p, &input).await {
                        Ok(()) => format!("Title set to: {input}"),
                        Err(err) => format!("Couldn't set the title: {err}"),
                    });
                }
                match self.title(p).await {
                    Ok(t) => {
                        args.insert("title".into(), t.into());
                        Some(response.to_string())
                    }
                    Err(err) => Some(format!("Can't read the title: {err}")),
                }
            }
            _ => None,
        }
    }

    /// `!command add|edit|remove|platforms|show !name ...`: text commands from chat.
    fn manage(self: &Rc<Self>, input: &str, staff: bool) -> String {
        const USAGE: &str =
            "Usage: !command add|edit|remove|platforms|show !name [text | twitch,kick,youtube,tiktok,all]";
        if !staff {
            return "Only moderators can manage commands.".into();
        }
        let mut words = input.splitn(3, ' ');
        let (op, name, rest) = (
            words.next().unwrap_or("").to_lowercase(),
            words.next().unwrap_or("").to_string(),
            words.next().unwrap_or("").trim().to_string(),
        );
        if name.is_empty() {
            return USAGE.into();
        }
        let name = if name.starts_with('!') {
            name
        } else {
            format!("!{name}")
        };
        let find = |cs: &[Obj]| {
            cs.iter().position(|c| {
                crate::commands::aliases(c)
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(&name))
            })
        };
        let at = find(&self.cfg.borrow().commands);
        let is_builtin = |c: &Obj| c.get("builtin").and_then(Value::as_str).is_some_and(|b| !b.is_empty());
        match (op.as_str(), at) {
            ("add", Some(_)) => format!("{name} already exists (use !command edit)."),
            ("add", None) if rest.is_empty() => USAGE.into(),
            ("add", None) => {
                let mut c = defaults().remove(0);
                for (k, v) in [
                    ("name", json!(name.trim_start_matches('!'))),
                    ("command", json!(name)),
                    ("builtin", json!("")),
                    ("response", json!(rest)),
                    ("group", json!("")),
                    ("userCooldown", json!(5)),
                ] {
                    c.insert(k.into(), v);
                }
                self.change(|cfg| cfg.commands.push(c));
                format!("Added {name} on all platforms.")
            }
            (_, None) => format!("There's no {name}."),
            ("edit", Some(_)) if rest.is_empty() => USAGE.into(),
            ("edit", Some(i)) => {
                self.change(|cfg| drop(cfg.commands[i].insert("response".into(), rest.into())));
                format!("Updated {name}.")
            }
            ("remove" | "delete", Some(i)) => {
                if is_builtin(&self.cfg.borrow().commands[i]) {
                    return format!("{name} is built in: turn it off in the Commands tab instead.");
                }
                self.change(|cfg| drop(cfg.commands.remove(i)));
                format!("Removed {name}.")
            }
            ("platforms", Some(i)) => match sources_from(&rest) {
                Some(bits) => {
                    self.change(|cfg| drop(cfg.commands[i].insert("sources".into(), bits.into())));
                    format!("{name} now works on {}.", sources_text(bits))
                }
                None => USAGE.into(),
            },
            ("show", Some(i)) => {
                let c = self.cfg.borrow().commands[i].clone();
                let bits = c.get("sources").and_then(Value::as_u64).unwrap_or(1);
                let reply = c.get("response").and_then(Value::as_str).unwrap_or("");
                format!(
                    "{name} ({}): {}",
                    sources_text(bits),
                    if reply.is_empty() { "(an action)" } else { reply }
                )
            }
            _ => USAGE.into(),
        }
    }

    /// When the stream on `platform` went live (Unix seconds); None if offline.
    async fn live_since(self: &Rc<Self>, platform: &str) -> Result<Option<i64>, String> {
        match platform {
            "twitch" => {
                let tw = self.twitch().ok_or("Twitch isn't connected")?;
                let v = tw
                    .helix(
                        "GET",
                        "streams",
                        vec![("user_id", "{broadcaster_id}")],
                        None,
                        false,
                        true,
                    )
                    .await?;
                Ok(v["data"][0]["started_at"].as_str().and_then(super::rest::parse_iso_pub))
            }
            "kick" => {
                let chan = self.cfg.borrow().kick.channel.to_lowercase();
                let r =
                    crate::http::request("GET", &format!("https://kick.com/api/v2/channels/{chan}"), &[], b"").await?;
                let v: Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
                // "2026-10-24 13:26:21" (UTC).
                Ok(v["livestream"]["created_at"]
                    .as_str()
                    .or_else(|| v["livestream"]["start_time"].as_str())
                    .and_then(|t| super::rest::parse_iso_pub(&t.replacen(' ', "T", 1))))
            }
            "youtube" => {
                let yt = self.youtube().ok_or("YouTube isn't connected")?;
                yt.live_started().await
            }
            _ => Err("not available on TikTok".into()),
        }
    }

    async fn title(self: &Rc<Self>, platform: &str) -> Result<String, String> {
        match platform {
            "twitch" => {
                let tw = self.twitch().ok_or("Twitch isn't connected")?;
                let v = tw
                    .helix(
                        "GET",
                        "channels",
                        vec![("broadcaster_id", "{broadcaster_id}")],
                        None,
                        false,
                        true,
                    )
                    .await?;
                Ok(v["data"][0]["title"].as_str().unwrap_or("").into())
            }
            "kick" => {
                let chan = self.cfg.borrow().kick.channel.to_lowercase();
                let r =
                    crate::http::request("GET", &format!("https://kick.com/api/v2/channels/{chan}"), &[], b"").await?;
                let v: Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
                Ok(v["livestream"]["session_title"].as_str().unwrap_or("").into())
            }
            "youtube" => self.youtube().ok_or("YouTube isn't connected")?.live_title().await,
            _ => Err("not available on TikTok".into()),
        }
    }

    async fn set_title(self: &Rc<Self>, platform: &str, title: &str) -> Result<(), String> {
        match platform {
            "twitch" => {
                let tw = self.twitch().ok_or("Twitch isn't connected")?;
                let q = vec![("broadcaster_id", "{broadcaster_id}")];
                tw.helix("PATCH", "channels", q, Some(json!({"title": title})), false, true)
                    .await
                    .map(drop)
            }
            "kick" => self.kick_start().set_channel_title(title).await,
            "youtube" => self.youtube_start().set_title("", title).await,
            _ => Err("not available on TikTok".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(sources_from("twitch, Kick"), Some(1 | 2_097_152));
        assert_eq!(sources_from("all"), Some(ALL));
        assert_eq!(sources_from("myspace"), None);
        assert_eq!(sources_text(ALL), "all platforms");
        assert_eq!(sources_text(1024 | 4_194_304), "youtube, tiktok");
        assert_eq!(duration(7500), "2h 5m");
        assert_eq!(duration(59), "59s");
        assert_eq!(channel_url("kick", "@Absi"), "https://kick.com/absi");
    }
}
