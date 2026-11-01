//! Twitch's chat commands (`/commercial 60`, `/timeout name 600`, `/slow`...). Twitch's chat API sends
//! what it is given as text, so a command typed in the chat box is turned into the Helix call it stands
//! for. `parse` is pure (and tested); `run` makes the call.

use crate::engine::Engine;
use serde_json::{json, Value};
use std::rc::Rc;

/// Where a user's id goes once the login is looked up.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Put {
    /// A query parameter of that name.
    Query(&'static str),
    /// `data.<key>` of the body (`{"data": {...}}`, as the ban call wants it).
    Data(&'static str),
}

/// One command as a call.
#[derive(Debug, PartialEq)]
pub enum Cmd {
    Helix {
        method: &'static str,
        path: &'static str,
        query: Vec<(&'static str, String)>,
        body: Option<Value>,
        /// Logins to look up, and where each id goes.
        users: Vec<(String, Put)>,
        /// What the chat says when it worked.
        done: String,
    },
    Announce {
        text: String,
        color: &'static str,
    },
    /// Not a command: sent as the chat message it is (`/me waves`).
    Say(String),
}

/// One command as the box suggests it.
pub struct Info {
    pub name: &'static str,
    /// What follows the name, as Twitch's own hint writes it: `[length]`, `<user>`.
    pub args: &'static str,
    /// The i18n key of what it does.
    pub about: &'static str,
    /// Values worth one click after the name: `/commercial 30`.
    pub presets: &'static [&'static str],
}

const fn info(name: &'static str, args: &'static str, about: &'static str, presets: &'static [&'static str]) -> Info {
    Info {
        name,
        args,
        about,
        presets,
    }
}

/// Every command, as the chat box lists them while a `/` is typed.
pub const COMMANDS: &[Info] = &[
    info(
        "commercial",
        "[length]",
        "slash.commercial",
        &["30", "60", "90", "120", "150", "180"],
    ),
    info("clear", "", "slash.clear", &[]),
    info("delete", "<message id>", "slash.delete", &[]),
    info(
        "timeout",
        "<user> [length] [reason]",
        "slash.timeout",
        &["10m", "1h", "1d", "1w"],
    ),
    info("ban", "<user> [reason]", "slash.ban", &[]),
    info("unban", "<user>", "slash.unban", &[]),
    info("slow", "[seconds]", "slash.slow", &["10", "30", "60", "120"]),
    info("slowoff", "", "slash.slowoff", &[]),
    info(
        "followers",
        "[length]",
        "slash.followers",
        &["0", "10m", "1h", "1d", "1w"],
    ),
    info("followersoff", "", "slash.followersoff", &[]),
    info("subscribers", "", "slash.subscribers", &[]),
    info("subscribersoff", "", "slash.subscribersoff", &[]),
    info("emoteonly", "", "slash.emoteonly", &[]),
    info("emoteonlyoff", "", "slash.emoteonlyoff", &[]),
    info("uniquechat", "", "slash.uniquechat", &[]),
    info("uniquechatoff", "", "slash.uniquechatoff", &[]),
    info("shoutout", "<channel>", "slash.shoutout", &[]),
    info("announce", "<message>", "slash.announce", &[]),
    info("announceblue", "<message>", "slash.announceblue", &[]),
    info("announcegreen", "<message>", "slash.announcegreen", &[]),
    info("announceorange", "<message>", "slash.announceorange", &[]),
    info("announcepurple", "<message>", "slash.announcepurple", &[]),
    info("raid", "<channel>", "slash.raid", &[]),
    info("unraid", "", "slash.unraid", &[]),
    info("mod", "<user>", "slash.mod", &[]),
    info("unmod", "<user>", "slash.unmod", &[]),
    info("vip", "<user>", "slash.vip", &[]),
    info("unvip", "<user>", "slash.unvip", &[]),
    info("marker", "[description]", "slash.marker", &[]),
    info("me", "<message>", "slash.me", &[]),
];

/// One line of the box's list.
#[derive(Debug, PartialEq, Clone)]
pub struct Suggestion {
    /// What the box holds once it's picked.
    pub fill: String,
    /// `/commercial`, or `/commercial 30` for a preset.
    pub title: String,
    pub args: &'static str,
    pub about: &'static str,
    /// Typing on completes it (a space follows): it takes more words, or has presets.
    pub more: bool,
}

/// Kick's chat commands that its API can do (kick.com's help lists more: its chat modes have no API).
pub const KICK_COMMANDS: &[Info] = &[
    info("ban", "<user> [reason]", "slash.ban", &[]),
    info("unban", "<user>", "slash.unban_kick", &[]),
    info(
        "timeout",
        "<user> [seconds]",
        "slash.timeout_kick",
        &["60", "600", "3600", "86400"],
    ),
    info("title", "<new title>", "slash.title", &[]),
];

/// The commands to suggest for what is typed so far in `platform`'s box, those whose name starts with what
/// follows the `/`, or, once a command with presets is typed in full and a space after it, its presets.
/// Nothing is hidden for what our role may not do (Twitch and Kick answer that when it's tried), only
/// what is known to be impossible: `commercials` is false for a channel that isn't an affiliate or partner.
pub fn suggestions(typed: &str, platform: &str, commercials: bool) -> Vec<Suggestion> {
    let Some(body) = typed.strip_prefix('/').filter(|b| !b.starts_with('/')) else {
        return Vec::new();
    };
    let list = match platform {
        "twitch" => COMMANDS,
        "kick" => KICK_COMMANDS,
        _ => return Vec::new(),
    };
    let usable = |c: &&Info| commercials || c.name != "commercial";
    match body.split_once(char::is_whitespace) {
        None => list
            .iter()
            .filter(usable)
            .filter(|c| c.name.starts_with(&body.to_lowercase()))
            .map(|c| Suggestion {
                fill: format!("/{}", c.name),
                title: format!("/{}", c.name),
                args: c.args,
                about: c.about,
                more: !c.args.is_empty(),
            })
            .collect(),
        Some((name, rest)) => {
            let Some(c) = list.iter().filter(usable).find(|c| c.name == name.to_lowercase()) else {
                return Vec::new();
            };
            let rest = rest.trim_start();
            // Presets are for the first word only.
            let typing_first = !rest.contains(char::is_whitespace);
            if c.presets.is_empty() || !typing_first {
                return vec![Suggestion {
                    fill: typed.to_string(),
                    title: format!("/{}", c.name),
                    args: c.args,
                    about: c.about,
                    more: false,
                }];
            }
            c.presets
                .iter()
                .filter(|p| p.starts_with(rest))
                .map(|p| Suggestion {
                    fill: format!("/{} {p}", c.name),
                    title: format!("/{} {p}", c.name),
                    args: "",
                    about: c.about,
                    more: false,
                })
                .collect()
        }
    }
}

/// Whether `text` is a command this module handles (not a plain message, and not `/me`).
pub fn is_command(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with('/') && !t.starts_with("/me ") && t != "/me" && !t.starts_with("//")
}

/// A length as seconds: `90`, `90s`, `10m`, `2h`, `1d`, `1w` (a bare number is `default_unit` seconds).
fn seconds(word: &str, bare: u64) -> Option<u64> {
    let w = word.trim().to_lowercase();
    let (n, unit) = match w.find(|c: char| !c.is_ascii_digit()) {
        Some(i) => w.split_at(i),
        None => (w.as_str(), ""),
    };
    let n: u64 = n.parse().ok()?;
    let k = match unit {
        "" => bare,
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        "w" => 604_800,
        _ => return None,
    };
    Some(n * k)
}

fn login(word: &str) -> String {
    word.trim().trim_start_matches('@').to_lowercase()
}

fn settings(body: Value, done: &str) -> Cmd {
    Cmd::Helix {
        method: "PATCH",
        path: "chat/settings",
        query: vec![
            ("broadcaster_id", "{broadcaster_id}".into()),
            ("moderator_id", "{moderator_id}".into()),
        ],
        body: Some(body),
        users: vec![],
        done: done.into(),
    }
}

/// The call for a command typed in the chat box, or why it can't be (the usage, as the chat says it).
pub fn parse(text: &str) -> Result<Cmd, String> {
    let text = text.trim();
    if !is_command(text) {
        return Ok(Cmd::Say(text.to_string()));
    }
    let (name, rest) = text[1..].split_once(char::is_whitespace).unwrap_or((&text[1..], ""));
    let (name, rest) = (name.to_lowercase(), rest.trim());
    let words: Vec<&str> = rest.split_whitespace().collect();
    let need = |usage: &str| -> Result<(), String> {
        if words.is_empty() {
            Err(format!("usage: {usage}"))
        } else {
            Ok(())
        }
    };
    let owner = |method, path, query: Vec<(&'static str, String)>, body, done: String| Cmd::Helix {
        method,
        path,
        query,
        body,
        users: vec![],
        done,
    };
    Ok(match name.as_str() {
        "commercial" => {
            let n: u64 = words.first().and_then(|w| w.parse().ok()).unwrap_or(30);
            if ![30, 60, 90, 120, 150, 180].contains(&n) {
                return Err("usage: /commercial <30, 60, 90, 120, 150 or 180>".into());
            }
            owner(
                "POST",
                "channels/commercial",
                vec![],
                Some(json!({"broadcaster_id": "{broadcaster_id}", "length": n})),
                format!("a {n} second commercial is running"),
            )
        }
        "clear" => owner(
            "DELETE",
            "moderation/chat",
            vec![
                ("broadcaster_id", "{broadcaster_id}".into()),
                ("moderator_id", "{moderator_id}".into()),
            ],
            None,
            "chat cleared".into(),
        ),
        "delete" => {
            need("/delete <message id>")?;
            owner(
                "DELETE",
                "moderation/chat",
                vec![
                    ("broadcaster_id", "{broadcaster_id}".into()),
                    ("moderator_id", "{moderator_id}".into()),
                    ("message_id", words[0].into()),
                ],
                None,
                "message deleted".into(),
            )
        }
        "ban" | "timeout" => {
            let usage = if name == "ban" {
                "/ban <user> [reason]"
            } else {
                "/timeout <user> [length, 10m by default] [reason]"
            };
            need(usage)?;
            let mut data = serde_json::Map::new();
            let mut reason = &words[1..];
            if name == "timeout" {
                let secs = match words.get(1) {
                    Some(w) => match seconds(w, 1) {
                        Some(s) => {
                            reason = &words[2..];
                            s
                        }
                        None => 600,
                    },
                    None => 600,
                };
                if !(1..=1_209_600).contains(&secs) {
                    return Err("a timeout is from 1 second to 2 weeks".into());
                }
                data.insert("duration".into(), secs.into());
            }
            if !reason.is_empty() {
                data.insert("reason".into(), reason.join(" ").into());
            }
            Cmd::Helix {
                method: "POST",
                path: "moderation/bans",
                query: vec![
                    ("broadcaster_id", "{broadcaster_id}".into()),
                    ("moderator_id", "{moderator_id}".into()),
                ],
                body: Some(json!({ "data": data })),
                users: vec![(login(words[0]), Put::Data("user_id"))],
                done: format!(
                    "{} {}",
                    if name == "ban" { "banned" } else { "timed out" },
                    login(words[0])
                ),
            }
        }
        "unban" | "untimeout" => {
            need("/unban <user>")?;
            Cmd::Helix {
                method: "DELETE",
                path: "moderation/bans",
                query: vec![
                    ("broadcaster_id", "{broadcaster_id}".into()),
                    ("moderator_id", "{moderator_id}".into()),
                ],
                body: None,
                users: vec![(login(words[0]), Put::Query("user_id"))],
                done: format!("{} can chat again", login(words[0])),
            }
        }
        "slow" => {
            let s = match words.first() {
                Some(w) => seconds(w, 1).filter(|s| (3..=120).contains(s)),
                None => Some(30),
            };
            let Some(s) = s else {
                return Err("usage: /slow [3 to 120 seconds]".into());
            };
            settings(
                json!({"slow_mode": true, "slow_mode_wait_time": s}),
                &format!("slow mode on ({s}s)"),
            )
        }
        "slowoff" => settings(json!({"slow_mode": false}), "slow mode off"),
        "followers" => {
            // Minutes (Twitch's own unit) when bare: `/followers 10` is ten minutes.
            let secs = match words.first() {
                Some(w) => seconds(w, 60),
                None => Some(0),
            };
            let Some(m) = secs.map(|s| s / 60).filter(|m| *m <= 129_600) else {
                return Err("usage: /followers [length, like 10m, 1h, 1d, up to 3 months]".into());
            };
            settings(
                json!({"follower_mode": true, "follower_mode_duration": m}),
                &format!("followers-only on ({m} min)"),
            )
        }
        "followersoff" => settings(json!({"follower_mode": false}), "followers-only off"),
        "subscribers" => settings(json!({"subscriber_mode": true}), "subscribers-only on"),
        "subscribersoff" => settings(json!({"subscriber_mode": false}), "subscribers-only off"),
        "emoteonly" => settings(json!({"emote_mode": true}), "emote-only on"),
        "emoteonlyoff" => settings(json!({"emote_mode": false}), "emote-only off"),
        "uniquechat" => settings(json!({"unique_chat_mode": true}), "unique chat on"),
        "uniquechatoff" => settings(json!({"unique_chat_mode": false}), "unique chat off"),
        "raid" => {
            need("/raid <channel>")?;
            Cmd::Helix {
                method: "POST",
                path: "raids",
                query: vec![("from_broadcaster_id", "{broadcaster_id}".into())],
                body: None,
                users: vec![(login(words[0]), Put::Query("to_broadcaster_id"))],
                done: format!("raiding {}", login(words[0])),
            }
        }
        "unraid" => owner(
            "DELETE",
            "raids",
            vec![("broadcaster_id", "{broadcaster_id}".into())],
            None,
            "raid cancelled".into(),
        ),
        "shoutout" => {
            need("/shoutout <channel>")?;
            Cmd::Helix {
                method: "POST",
                path: "chat/shoutouts",
                query: vec![
                    ("from_broadcaster_id", "{broadcaster_id}".into()),
                    ("moderator_id", "{moderator_id}".into()),
                ],
                body: None,
                users: vec![(login(words[0]), Put::Query("to_broadcaster_id"))],
                done: format!("shouted out {}", login(words[0])),
            }
        }
        "mod" | "unmod" | "vip" | "unvip" => {
            need(&format!("/{name} <user>"))?;
            let (method, path) = match name.as_str() {
                "mod" => ("POST", "moderation/moderators"),
                "unmod" => ("DELETE", "moderation/moderators"),
                "vip" => ("POST", "channels/vips"),
                _ => ("DELETE", "channels/vips"),
            };
            Cmd::Helix {
                method,
                path,
                query: vec![("broadcaster_id", "{broadcaster_id}".into())],
                body: None,
                users: vec![(login(words[0]), Put::Query("user_id"))],
                done: format!("{name}: {}", login(words[0])),
            }
        }
        "marker" => owner(
            "POST",
            "streams/markers",
            vec![],
            Some(json!({"user_id": "{broadcaster_id}", "description": rest.chars().take(140).collect::<String>()})),
            "marker added".into(),
        ),
        "announce" | "announceblue" | "announcegreen" | "announceorange" | "announcepurple" => {
            need(&format!("/{name} <message>"))?;
            Cmd::Announce {
                text: rest.to_string(),
                color: match name.as_str() {
                    "announceblue" => "blue",
                    "announcegreen" => "green",
                    "announceorange" => "orange",
                    "announcepurple" => "purple",
                    _ => "primary",
                },
            }
        }
        _ => {
            return Err(format!(
                "/{name} isn't a command Streamerz knows (try one from the / menu)"
            ))
        }
    })
}

/// A Kick command as a call.
#[derive(Debug, PartialEq)]
pub enum KickCmd {
    Ban { user: String, reason: String },
    Unban(String),
    Timeout { user: String, secs: u64, reason: String },
    Title(String),
}

/// What a command typed in Kick's chat box stands for, or why it can't be.
pub fn parse_kick(text: &str) -> Result<KickCmd, String> {
    let text = text.trim();
    let (name, rest) = text
        .strip_prefix('/')
        .unwrap_or(text)
        .split_once(char::is_whitespace)
        .unwrap_or((text.trim_start_matches('/'), ""));
    let (name, rest) = (name.to_lowercase(), rest.trim());
    let words: Vec<&str> = rest.split_whitespace().collect();
    let user = || {
        words
            .first()
            .map(|w| login(w))
            .ok_or_else(|| format!("usage: /{name} <user>"))
    };
    Ok(match name.as_str() {
        "ban" => KickCmd::Ban {
            user: user()?,
            reason: words[1..].join(" "),
        },
        "unban" | "untimeout" => KickCmd::Unban(user()?),
        "timeout" => {
            let user = user()?;
            let (secs, reason) = match words.get(1).and_then(|w| seconds(w, 1)) {
                Some(s) => (s, words[2..].join(" ")),
                None => (600, words.get(1..).unwrap_or(&[]).join(" ")),
            };
            // Kick takes minutes, from 1 minute to 7 days.
            if !(60..=604_800).contains(&secs) {
                return Err("a timeout on Kick is from 1 minute (60) to 7 days".into());
            }
            KickCmd::Timeout { user, secs, reason }
        }
        "title" => {
            if rest.is_empty() {
                return Err("usage: /title <new title>".into());
            }
            KickCmd::Title(rest.to_string())
        }
        _ => {
            return Err(format!(
                "/{name} can't be run from here: Kick's own site does it, its API has no call for it"
            ))
        }
    })
}

/// Runs a command typed in Kick's chat box: says what happened.
pub async fn run_kick(e: &Rc<Engine>, text: &str) -> Result<String, String> {
    let k = e.kick_start();
    match parse_kick(text)? {
        KickCmd::Ban { user, reason } => k.ban_user(&user, &reason).await.map(|()| format!("banned {user}")),
        KickCmd::Unban(user) => k.unban_user(&user).await.map(|()| format!("{user} can chat again")),
        KickCmd::Timeout { user, secs, reason } => k
            .timeout_user(&user, secs, &reason)
            .await
            .map(|()| format!("timed out {user}")),
        KickCmd::Title(t) => k.set_channel_title(&t).await.map(|()| "title changed".into()),
    }
}

/// Runs a command typed for Twitch's chat: says in the chat what happened.
pub async fn run(e: &Rc<Engine>, text: &str, bot: bool, fallback: bool) -> Result<String, String> {
    let cmd = parse(text)?;
    if matches!(
        &cmd,
        Cmd::Helix {
            path: "channels/commercial",
            ..
        }
    ) && !crate::twitch::commercials_possible()
    {
        return Err("this channel isn't a Twitch affiliate or partner, so it can't run commercials".into());
    }
    let Some(t) = e.twitch() else {
        return Err("Twitch isn't connected".into());
    };
    match cmd {
        Cmd::Say(m) => t.send_message(m, None, bot, fallback).await.map(|_| String::new()),
        Cmd::Announce { text, color } => t
            .send_announcement(text, color.into(), bot, fallback)
            .await
            .map(|_| "announced".into()),
        Cmd::Helix {
            method,
            path,
            mut query,
            mut body,
            users,
            done,
        } => {
            for (who, put) in users {
                let id = t
                    .resolve_user_id(&who)
                    .await
                    .map_err(|_| format!("no Twitch user called {who}"))?;
                match put {
                    Put::Query(k) => query.push((k, id)),
                    Put::Data(k) => {
                        if let Some(d) = body
                            .as_mut()
                            .and_then(|b| b.get_mut("data"))
                            .and_then(Value::as_object_mut)
                        {
                            d.insert(k.into(), id.into());
                        }
                    }
                }
            }
            let q: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            // As the broadcaster account (the one that moderates), never the bot.
            t.helix(method, path, q, body, false, false).await.map(|_| done)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Call = (
        &'static str,
        &'static str,
        Vec<(&'static str, String)>,
        Option<Value>,
        Vec<(String, Put)>,
    );

    fn helix(text: &str) -> Call {
        match parse(text).unwrap() {
            Cmd::Helix {
                method,
                path,
                query,
                body,
                users,
                ..
            } => (method, path, query, body, users),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_commercial_takes_one_of_twitchs_lengths() {
        let (m, p, _, body, _) = helix("/commercial 90");
        assert_eq!((m, p), ("POST", "channels/commercial"));
        assert_eq!(body.unwrap()["length"], 90);
        assert_eq!(helix("/commercial").3.unwrap()["length"], 30);
        assert!(parse("/commercial 45").unwrap_err().starts_with("usage"));
    }

    #[test]
    fn timeouts_and_bans() {
        let (_, p, _, body, users) = helix("/timeout @Troll 10m spamming links");
        assert_eq!(p, "moderation/bans");
        assert_eq!(
            body.unwrap()["data"],
            json!({"duration": 600, "reason": "spamming links"})
        );
        assert_eq!(users, [("troll".to_string(), Put::Data("user_id"))]);
        // No length: ten minutes, and the rest is the reason.
        assert_eq!(helix("/timeout troll").3.unwrap()["data"]["duration"], 600);
        assert_eq!(
            helix("/timeout troll rude words").3.unwrap()["data"]["reason"],
            "rude words"
        );
        assert_eq!(helix("/ban troll").3.unwrap()["data"], json!({}));
        assert!(parse("/ban").unwrap_err().starts_with("usage"));
        assert!(parse("/timeout troll 99w").is_err());
    }

    #[test]
    fn chat_modes() {
        assert_eq!(
            helix("/slow").3.unwrap(),
            json!({"slow_mode": true, "slow_mode_wait_time": 30})
        );
        assert!(parse("/slow 1").is_err());
        assert_eq!(helix("/slowoff").3.unwrap(), json!({"slow_mode": false}));
        // A bare number of followers is minutes.
        assert_eq!(helix("/followers 10").3.unwrap()["follower_mode_duration"], 10);
        assert_eq!(helix("/followers 1h").3.unwrap()["follower_mode_duration"], 60);
        assert_eq!(helix("/followers").3.unwrap()["follower_mode_duration"], 0);
        assert_eq!(helix("/subscribers").3.unwrap(), json!({"subscriber_mode": true}));
        assert_eq!(helix("/emoteonlyoff").3.unwrap(), json!({"emote_mode": false}));
        assert_eq!(helix("/uniquechat").3.unwrap(), json!({"unique_chat_mode": true}));
        assert_eq!(helix("/slow").1, "chat/settings");
    }

    #[test]
    fn people_commands_look_the_user_up() {
        let (m, p, _, _, users) = helix("/raid @SomeStreamer");
        assert_eq!((m, p), ("POST", "raids"));
        assert_eq!(users, [("somestreamer".to_string(), Put::Query("to_broadcaster_id"))]);
        assert_eq!(helix("/unvip x").1, "channels/vips");
        assert_eq!(helix("/unvip x").0, "DELETE");
        assert_eq!(helix("/shoutout x").1, "chat/shoutouts");
    }

    #[test]
    fn announcements_and_plain_text() {
        assert_eq!(
            parse("/announceblue hello all").unwrap(),
            Cmd::Announce {
                text: "hello all".into(),
                color: "blue"
            }
        );
        assert_eq!(parse("/me waves").unwrap(), Cmd::Say("/me waves".into()));
        assert_eq!(parse("hello").unwrap(), Cmd::Say("hello".into()));
        assert_eq!(parse("//not a command").unwrap(), Cmd::Say("//not a command".into()));
        assert!(parse("/nonsense").unwrap_err().contains("isn't a command"));
    }

    #[test]
    fn every_listed_command_parses_with_its_arguments() {
        for c in COMMANDS.iter().filter(|c| c.name != "me") {
            // Those that take a number first, and those that take a name first.
            let numeric = matches!(c.name, "commercial" | "slow" | "followers");
            let lead = if numeric { "" } else { "someone " };
            for p in c.presets {
                let text = format!("/{} {lead}{p}", c.name);
                assert!(parse(&text).is_ok(), "{text}: {:?}", parse(&text));
            }
            if c.presets.is_empty() {
                let text = format!("/{} x", c.name);
                assert!(parse(&text).is_ok(), "{text}: {:?}", parse(&text));
            }
        }
    }

    #[test]
    fn the_box_suggests_by_prefix() {
        let names = |t: &str, p, c| suggestions(t, p, c).into_iter().map(|s| s.title).collect::<Vec<_>>();
        assert_eq!(names("/comm", "twitch", true), ["/commercial"]);
        // A channel that isn't an affiliate or partner can't run commercials: not offered.
        assert!(names("/comm", "twitch", false).is_empty());
        assert!(names("/co", "twitch", false).iter().all(|n| n != "/commercial"));
        // Nothing is hidden for a role: Twitch answers that when it's tried.
        assert!(names("/ban", "twitch", true).contains(&"/ban".to_string()));
        assert!(names("hello", "twitch", true).is_empty());
        assert!(names("//x", "twitch", true).is_empty());
        assert!(names("/", "youtube", true).is_empty());
        assert_eq!(names("/commercial ", "twitch", true).len(), 6);
        assert_eq!(
            names("/commercial 1", "twitch", true),
            ["/commercial 120", "/commercial 150", "/commercial 180"]
        );
        assert_eq!(names("/timeout bob 10m", "twitch", true), ["/timeout"]);
        assert_eq!(names("/timeout ", "twitch", true).len(), 4);
        let s = &suggestions("/ban", "twitch", true)[0];
        assert!(s.more && s.fill == "/ban");
        // Kick's own commands.
        assert_eq!(names("/ti", "kick", true), ["/timeout", "/title"]);
        assert_eq!(names("/timeout ", "kick", true).len(), 4);
    }

    #[test]
    fn kick_commands() {
        assert_eq!(
            parse_kick("/timeout @Troll 300 spam").unwrap(),
            KickCmd::Timeout {
                user: "troll".into(),
                secs: 300,
                reason: "spam".into()
            }
        );
        assert!(matches!(
            parse_kick("/timeout troll").unwrap(),
            KickCmd::Timeout { secs: 600, .. }
        ));
        assert!(parse_kick("/timeout troll 5").is_err());
        assert_eq!(parse_kick("/unban troll").unwrap(), KickCmd::Unban("troll".into()));
        assert_eq!(
            parse_kick("/title Hello there").unwrap(),
            KickCmd::Title("Hello there".into())
        );
        assert!(parse_kick("/slow on 5").unwrap_err().contains("can't be run from here"));
        assert!(parse_kick("/ban").unwrap_err().starts_with("usage"));
    }

    #[test]
    fn lengths() {
        assert_eq!(seconds("90", 1), Some(90));
        assert_eq!(seconds("10m", 1), Some(600));
        assert_eq!(seconds("2h", 1), Some(7200));
        assert_eq!(seconds("5", 60), Some(300));
        assert_eq!(seconds("abc", 1), None);
        assert_eq!(seconds("5x", 1), None);
    }
}
