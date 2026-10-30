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

use crate::roles::Role;

/// One command as the box suggests it.
pub struct Info {
    pub name: &'static str,
    /// What follows the name, as Twitch's own hint writes it: `[length]`, `<user>`.
    pub args: &'static str,
    /// The i18n key of what it does.
    pub about: &'static str,
    /// What it takes to use it (the box hides what our role can't).
    pub needs: Role,
    /// Values worth one click after the name: `/commercial 30`.
    pub presets: &'static [&'static str],
}

const fn info(
    name: &'static str,
    args: &'static str,
    about: &'static str,
    needs: Role,
    presets: &'static [&'static str],
) -> Info {
    Info {
        name,
        args,
        about,
        needs,
        presets,
    }
}

/// Every command, as the chat box lists them while a `/` is typed.
pub const COMMANDS: &[Info] = &[
    info(
        "commercial",
        "[length]",
        "slash.commercial",
        Role::Broadcaster,
        &["30", "60", "90", "120", "150", "180"],
    ),
    info("clear", "", "slash.clear", Role::Moderator, &[]),
    info("delete", "<message id>", "slash.delete", Role::Moderator, &[]),
    info(
        "timeout",
        "<user> [length] [reason]",
        "slash.timeout",
        Role::Moderator,
        &["10m", "1h", "1d", "1w"],
    ),
    info("ban", "<user> [reason]", "slash.ban", Role::Moderator, &[]),
    info("unban", "<user>", "slash.unban", Role::Moderator, &[]),
    info(
        "slow",
        "[seconds]",
        "slash.slow",
        Role::Moderator,
        &["10", "30", "60", "120"],
    ),
    info("slowoff", "", "slash.slowoff", Role::Moderator, &[]),
    info(
        "followers",
        "[length]",
        "slash.followers",
        Role::Moderator,
        &["0", "10m", "1h", "1d", "1w"],
    ),
    info("followersoff", "", "slash.followersoff", Role::Moderator, &[]),
    info("subscribers", "", "slash.subscribers", Role::Moderator, &[]),
    info("subscribersoff", "", "slash.subscribersoff", Role::Moderator, &[]),
    info("emoteonly", "", "slash.emoteonly", Role::Moderator, &[]),
    info("emoteonlyoff", "", "slash.emoteonlyoff", Role::Moderator, &[]),
    info("uniquechat", "", "slash.uniquechat", Role::Moderator, &[]),
    info("uniquechatoff", "", "slash.uniquechatoff", Role::Moderator, &[]),
    info("shoutout", "<channel>", "slash.shoutout", Role::Moderator, &[]),
    info("announce", "<message>", "slash.announce", Role::Moderator, &[]),
    info("announceblue", "<message>", "slash.announceblue", Role::Moderator, &[]),
    info(
        "announcegreen",
        "<message>",
        "slash.announcegreen",
        Role::Moderator,
        &[],
    ),
    info(
        "announceorange",
        "<message>",
        "slash.announceorange",
        Role::Moderator,
        &[],
    ),
    info(
        "announcepurple",
        "<message>",
        "slash.announcepurple",
        Role::Moderator,
        &[],
    ),
    info("raid", "<channel>", "slash.raid", Role::Broadcaster, &[]),
    info("unraid", "", "slash.unraid", Role::Broadcaster, &[]),
    info("mod", "<user>", "slash.mod", Role::Broadcaster, &[]),
    info("unmod", "<user>", "slash.unmod", Role::Broadcaster, &[]),
    info("vip", "<user>", "slash.vip", Role::Broadcaster, &[]),
    info("unvip", "<user>", "slash.unvip", Role::Broadcaster, &[]),
    info("marker", "[description]", "slash.marker", Role::Broadcaster, &[]),
    info("me", "<message>", "slash.me", Role::Anonymous, &[]),
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

/// The commands to suggest for what is typed so far (`typed` as in the box), for an account that is `me`:
/// those whose name starts with what follows the `/`, or, once a command with presets is typed in full
/// and a space after it, its presets.
pub fn suggestions(typed: &str, me: Role) -> Vec<Suggestion> {
    let Some(body) = typed.strip_prefix('/').filter(|b| !b.starts_with('/')) else {
        return Vec::new();
    };
    let usable = |c: &&Info| me >= c.needs || c.needs == Role::Anonymous;
    match body.split_once(char::is_whitespace) {
        None => COMMANDS
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
            let Some(c) = COMMANDS.iter().filter(usable).find(|c| c.name == name.to_lowercase()) else {
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

/// Runs a command typed for Twitch's chat: says in the chat what happened.
pub async fn run(e: &Rc<Engine>, text: &str, bot: bool, fallback: bool) -> Result<String, String> {
    let cmd = parse(text)?;
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
    fn the_box_suggests_by_prefix_and_role() {
        let names = |t: &str, me| suggestions(t, me).into_iter().map(|s| s.title).collect::<Vec<_>>();
        assert_eq!(names("/comm", Role::Broadcaster), ["/commercial"]);
        // A moderator isn't offered the owner's commercial.
        assert!(names("/comm", Role::Moderator).is_empty());
        assert!(names("/", Role::Viewer) == ["/me"]);
        assert!(names("hello", Role::Broadcaster).is_empty());
        assert!(names("//x", Role::Broadcaster).is_empty());
        assert_eq!(names("/commercial ", Role::Broadcaster).len(), 6);
        assert_eq!(
            names("/commercial 1", Role::Broadcaster),
            ["/commercial 120", "/commercial 150", "/commercial 180"]
        );
        // Past the first word, the usage stays as a hint.
        assert_eq!(names("/timeout bob 10m", Role::Moderator), ["/timeout"]);
        assert_eq!(names("/timeout ", Role::Moderator).len(), 4);
        let s = &suggestions("/ban", Role::Moderator)[0];
        assert!(s.more && s.fill == "/ban");
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

    #[test]
    fn the_roles_they_need_are_known() {
        use crate::roles::{twitch_needs, Role};
        for (m, p, want) in [
            ("POST", "channels/commercial", Role::Broadcaster),
            ("POST", "moderation/bans", Role::Moderator),
            ("PATCH", "chat/settings", Role::Moderator),
            ("POST", "raids", Role::Broadcaster),
            ("POST", "chat/shoutouts", Role::Moderator),
            ("POST", "streams/markers", Role::Broadcaster),
        ] {
            assert_eq!(twitch_needs(m, p), want, "{m} {p}");
        }
    }
}
