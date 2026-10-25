//! Ready-made actions ("New from template"): a few common ones, made of ordinary triggers, sub-actions,
//! commands and timers, so they can be edited, exported and imported like any other. Nothing is added
//! until one is picked. One action per platform where the platforms differ (each sends its own way).

use crate::edit::{new_action, new_sub, new_trigger};
use crate::model::{Action, Obj};
use serde_json::{json, Value};

/// What a template adds.
#[derive(Default)]
pub struct Made {
    pub actions: Vec<Action>,
    pub commands: Vec<Obj>,
    pub timers: Vec<Obj>,
}

pub struct Template {
    pub name: &'static str,
    pub about: &'static str,
    pub make: fn() -> Made,
}

/// A platform's send-message sub-action, as the bot (falling back to the broadcaster).
fn say(platform: &str, text: &str) -> Obj {
    let kind = match platform {
        "twitch" => 10,
        "youtube" => 4001,
        "kick" => 35001,
        _ => 43001,
    };
    with(new_sub(kind), json!({"text": text, "useBot": true, "fallback": true}))
}

fn with(mut o: Obj, more: Value) -> Obj {
    if let Value::Object(m) = more {
        o.extend(m);
    }
    o
}

/// An action in the "Templates" group with `triggers` (type codes) and `subs`, in order.
fn action(name: &str, triggers: &[u32], subs: Vec<Obj>) -> Action {
    let mut a = new_action(name, "Templates");
    a.triggers = triggers.iter().map(|&t| new_trigger(t)).collect();
    a.sub_actions = subs
        .into_iter()
        .enumerate()
        .map(|(i, mut s)| {
            s.insert("index".into(), i.into());
            s
        })
        .collect();
    a
}

/// One action per platform: (platform, trigger codes, message).
fn per_platform(name: &str, each: &[(&str, &[u32], &str)]) -> Made {
    let title = |p: &str| match p {
        "twitch" => "Twitch",
        "youtube" => "YouTube",
        "kick" => "Kick",
        _ => "TikTok",
    };
    Made {
        actions: each
            .iter()
            .map(|(p, t, text)| action(&format!("{name} ({})", title(p)), t, vec![say(p, text)]))
            .collect(),
        ..Default::default()
    }
}

/// A timer and the action it runs, saying `text` on every platform (those not connected skip it).
fn timed(name: &str, minutes: u64, text: &str) -> Made {
    let timer_id = crate::engine::uuid();
    let timer = json!({"id": timer_id, "name": name, "enabled": true, "repeat": true, "interval": minutes * 60,
        "randomInterval": false, "upperInterval": 0, "lines": 0, "counter": 0});
    let mut a = action(
        name,
        &[],
        ["twitch", "youtube", "kick", "tiktok"]
            .iter()
            .map(|p| say(p, text))
            .collect(),
    );
    a.triggers = vec![with(new_trigger(701), json!({"timerId": timer_id}))];
    Made {
        actions: vec![a],
        timers: timer.as_object().cloned().into_iter().collect(),
        ..Default::default()
    }
}

/// A chat command on every platform answering with `response` (no action needed).
fn command(name: &str, alias: &str, response: &str, cooldown: u64) -> Made {
    let c = json!({"permittedUsers": [], "permittedGroups": [], "id": crate::engine::uuid(), "name": name,
        "enabled": true, "include": true, "mode": 0, "command": alias, "regexExplicitCapture": false,
        "location": 0, "ignoreBotAccount": true, "ignoreInternal": true,
        "sources": 1 | 1024 | 2_097_152 | 4_194_304, "persistCounter": false, "persistUserCounter": false,
        "caseSensitive": false, "globalCooldown": 0, "userCooldown": cooldown, "group": "Templates",
        "grantType": 0, "response": response, "replyAll": false});
    Made {
        commands: c.as_object().cloned().into_iter().collect(),
        ..Default::default()
    }
}

pub const TEMPLATES: &[Template] = &[
    Template {
        name: "Welcome first chatters",
        about: "Greets someone the first time they chat in a stream (Twitch, YouTube, Kick).",
        make: || {
            per_platform(
                "Welcome first chatters",
                &[
                    ("twitch", &[120], "Welcome to the stream, %user%! 👋"),
                    ("youtube", &[4016], "Welcome to the stream, %user%! 👋"),
                    ("kick", &[35008], "Welcome to the stream, %user%! 👋"),
                ],
            )
        },
    },
    Template {
        name: "Thank follows",
        about: "Thanks new followers in chat (Twitch, Kick, TikTok).",
        make: || {
            per_platform(
                "Thank follows",
                &[
                    ("twitch", &[101], "Thanks for the follow, %user%! 💜"),
                    ("kick", &[35011], "Thanks for the follow, %user%! 💚"),
                    ("tiktok", &[41004], "Thanks for the follow, %user%! ❤️"),
                ],
            )
        },
    },
    Template {
        name: "Thank subscriptions",
        about: "Thanks new subscribers, resubs and members (Twitch, YouTube, Kick, TikTok).",
        make: || {
            per_platform(
                "Thank subscriptions",
                &[
                    ("twitch", &[103, 104], "Thanks for subscribing, %user%! 🎉"),
                    ("youtube", &[4008], "Welcome to the members, %user%! 🎉"),
                    ("kick", &[35014], "Thanks for subscribing, %user%! 🎉"),
                    ("tiktok", &[41006], "Thanks for subscribing, %user%! 🎉"),
                ],
            )
        },
    },
    Template {
        name: "Thank gifted subs",
        about: "Thanks people who gift subscriptions (Twitch single and community gifts, Kick).",
        make: || {
            let mut m = per_platform(
                "Thank gifted subs",
                &[
                    (
                        "twitch",
                        &[105],
                        "%user% gifted a sub to %recipientUser%! Thank you! 🎁",
                    ),
                    ("kick", &[35016, 35017], "Thanks for the gifted subs, %user%! 🎁"),
                ],
            );
            m.actions.push(action(
                "Thank gifted subs (Twitch community)",
                &[106],
                vec![say(
                    "twitch",
                    "%user% just gifted %gifts% subs to the community! Thank you! 🎁",
                )],
            ));
            m
        },
    },
    Template {
        name: "Thank cheers and Super Chats",
        about: "Thanks Twitch cheers and YouTube Super Chats.",
        make: || {
            per_platform(
                "Thank cheers",
                &[
                    ("twitch", &[102], "Thanks for the %bits% bits, %user%! 💎"),
                    ("youtube", &[4006], "Thanks for the Super Chat, %user%! 💛"),
                ],
            )
        },
    },
    Template {
        name: "Thank TikTok gifts",
        about: "Thanks TikTok gifts with the gift's name and count.",
        make: || {
            per_platform(
                "Thank gifts",
                &[(
                    "tiktok",
                    &[41002],
                    "Thanks for the %repeatCount%x %giftName%, %user%! 🌹",
                )],
            )
        },
    },
    Template {
        name: "Raid welcome and shoutout",
        about: "Welcomes a Twitch raid and gives the raider a shoutout a few seconds later.",
        make: || Made {
            actions: vec![action(
                "Raid welcome and shoutout",
                &[107],
                vec![
                    say(
                        "twitch",
                        "Welcome raiders! Thanks for the raid with %viewers% viewers, %user%! 🚀",
                    ),
                    with(new_sub(1002), json!({"value": 5000, "maxValue": 0, "random": false})),
                    with(new_sub(29), json!({"userLogin": "%userName%"})),
                ],
            )],
            ..Default::default()
        },
    },
    Template {
        name: "OBS scene on raid",
        about: "Switches OBS to a \"Raid\" scene for 15 seconds when raided, then back to \"Main\" (edit the names).",
        make: || {
            let scene = |name: &str| with(new_sub(34), json!({"connectionId": "", "sceneName": name}));
            Made {
                actions: vec![action(
                    "OBS scene on raid",
                    &[107],
                    vec![
                        scene("Raid"),
                        with(new_sub(1002), json!({"value": 15000, "maxValue": 0, "random": false})),
                        scene("Main"),
                    ],
                )],
                ..Default::default()
            }
        },
    },
    Template {
        name: "Socials every 20 minutes",
        about: "A timer that posts your links on every connected platform (edit the text).",
        make: || {
            timed(
                "Socials",
                20,
                "Follow me everywhere: twitch.tv/you · youtube.com/@you · tiktok.com/@you",
            )
        },
    },
    Template {
        name: "Hydrate reminder",
        about: "A timer that reminds you (and chat) to drink water every hour.",
        make: || timed("Hydrate", 60, "Hydration check! Time for some water 💧"),
    },
    Template {
        name: "!lurk command",
        about: "A !lurk command on every platform with a friendly reply.",
        make: || command("Lurk", "!lurk", "%user% is now lurking. Enjoy the stream! 👀", 30),
    },
    Template {
        name: "!discord command",
        about: "A !discord command on every platform that posts your invite (edit the link).",
        make: || {
            command(
                "Discord",
                "!discord",
                "Join the Discord: https://discord.gg/your-invite",
                30,
            )
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SUB_ACTION_INFO, TRIGGERS};

    #[test]
    fn templates_use_known_types_and_fields() {
        for t in TEMPLATES {
            let m = (t.make)();
            assert!(
                !m.actions.is_empty() || !m.commands.is_empty(),
                "{} makes nothing",
                t.name
            );
            for a in &m.actions {
                for tr in &a.triggers {
                    let code = tr["type"].as_u64().unwrap() as u32;
                    assert!(TRIGGERS.iter().any(|x| x.0 == code), "{}: trigger {code}", t.name);
                }
                for s in &a.sub_actions {
                    let code = s["type"].as_u64().unwrap() as u32;
                    let info = SUB_ACTION_INFO.iter().find(|x| x.0 == code);
                    let info = info.unwrap_or_else(|| panic!("{}: sub-action {code}", t.name));
                    let base = ["id", "weight", "type", "parentId", "enabled", "index"];
                    for k in s.keys().filter(|k| !base.contains(&k.as_str())) {
                        assert!(info.3.contains(&k.as_str()), "{}: {} has no field {k}", t.name, info.1);
                    }
                }
            }
            for a in &m.actions {
                if a.triggers.iter().any(|tr| tr["type"] == 701) {
                    let id = &a.triggers[0]["timerId"];
                    assert!(m.timers.iter().any(|x| &x["id"] == id), "{}: timer", t.name);
                }
            }
        }
    }
}
