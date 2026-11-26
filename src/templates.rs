//! Ready-made actions ("New from template"): a few common ones, made of ordinary triggers, sub-actions,
//! commands and timers, so they can be edited, exported and imported like any other. Nothing is added
//! until one is picked. One action per platform where the platforms differ (each sends its own way).

mod automation;
pub mod clock;
pub mod songs;
pub mod speech;

use crate::edit::{new_action, new_sub, new_trigger};
use crate::model::{Action, Obj};
use serde_json::{json, Value};

/// What a template adds.
#[derive(Default)]
pub struct Made {
    pub actions: Vec<Action>,
    pub commands: Vec<Obj>,
    pub timers: Vec<Obj>,
    /// Groups the template declares (settings, files), by name.
    pub groups: Vec<(String, crate::groups::Meta)>,
    /// Icons (PNG or JPEG bytes) of groups, by name.
    pub icons: Vec<(String, Vec<u8>)>,
}

impl Made {
    /// Add everything to the configuration. Settings start at their defaults, and countdowns the
    /// actions reset start now.
    pub fn add_to(self, c: &mut crate::Config) -> Result<(), String> {
        // Validate every group and icon before mutating the config so an error leaves it intact.
        for (_, meta) in &self.groups {
            meta.validate()?;
        }
        let mut encoded_icons = Vec::with_capacity(self.icons.len());
        for (group, bytes) in self.icons {
            encoded_icons.push((group, crate::group_icons::encode(&bytes)?));
        }
        for (group, meta) in self.groups {
            // Pages alone are an overlay: only what has actions is a group of them.
            let overlay = meta.files.iter().any(|f| crate::groups::is_page(&f.name));
            let used = self.actions.iter().any(|a| a.group == group);
            if used || !overlay {
                crate::groups::create(c, &group);
            }
            crate::groups::set_meta(c, &group, meta)?;
            // What is set stays; the rest starts at its default.
            let values = crate::groups::values(c, &group);
            crate::groups::set_values(c, &group, values)?;
        }
        for (group, icon) in encoded_icons {
            c.action_group_icons.insert(group, icon);
        }
        clock::start_for(c, &self.actions, clock::now_ms());
        c.actions.actions.extend(self.actions);
        c.commands.extend(self.commands);
        c.timers.extend(self.timers);
        Ok(())
    }
}

pub struct Template {
    pub name: &'static str,
    pub about: &'static str,
    pub make: fn() -> Made,
}

impl Template {
    pub fn title(&self) -> &'static str {
        let key = match self.name {
            "Song requests (Spotify)" => "automation.recipe_songs",
            "Read chat aloud" => "automation.recipe_tts",
            "Welcome first chatters" => "automation.recipe_welcome",
            "Thank follows" => "automation.recipe_follows",
            "Thank subscriptions" => "automation.recipe_subscriptions",
            "Thank gifted subs" => "automation.recipe_gifts",
            "Thank cheers and Super Chats" => "automation.recipe_cheers",
            "Raid welcome and shoutout" => "automation.recipe_raid",
            "OBS scene on raid" => "automation.recipe_raid_scene",
            "Socials every 20 minutes" => "automation.recipe_socials",
            "Hydrate reminder" => "automation.recipe_hydrate",
            "!lurk command" => "automation.recipe_lurk",
            "!discord command" => "automation.recipe_discord",
            name => name,
        };
        crate::i18n::tr(key)
    }
    pub fn description(&self) -> &'static str {
        if self.about.starts_with("automation.") {
            crate::i18n::tr(self.about)
        } else {
            self.about
        }
    }
}

/// A platform's send-message sub-action, as the bot (falling back to the broadcaster).
pub(super) fn say(platform: &str, text: &str) -> Obj {
    let kind = match platform {
        "twitch" => 10,
        "youtube" => 4001,
        "kick" => 35001,
        "tiktok" => 43001,
        _ => unreachable!("unsupported chat platform"),
    };
    if platform == "tiktok" {
        return with(new_sub(kind), json!({"text": text}));
    }
    with(new_sub(kind), json!({"text": text, "useBot": true, "fallback": true}))
}

fn with(mut o: Obj, more: Value) -> Obj {
    if let Value::Object(m) = more {
        o.extend(m);
    }
    o
}

/// An action in the "Templates" group with `triggers` (type codes) and `subs`, in order.
fn localized_name(name: &str) -> &str {
    match name {
        "Song requests (Spotify)" => crate::i18n::tr("automation.recipe_songs"),
        "Welcome first chatters" => crate::i18n::tr("automation.recipe_welcome"),
        "Thank follows" => crate::i18n::tr("automation.recipe_follows"),
        "Thank subscriptions" => crate::i18n::tr("automation.recipe_subscriptions"),
        "Thank gifted subs" => crate::i18n::tr("automation.recipe_gifts"),
        "Thank cheers and Super Chats" => crate::i18n::tr("automation.recipe_cheers"),
        "Raid welcome and shoutout" => crate::i18n::tr("automation.recipe_raid"),
        "OBS scene on raid" => crate::i18n::tr("automation.recipe_raid_scene"),
        "Socials every 20 minutes" => crate::i18n::tr("automation.recipe_socials"),
        "Hydrate reminder" => crate::i18n::tr("automation.recipe_hydrate"),
        "!lurk command" => crate::i18n::tr("automation.recipe_lurk"),
        "!discord command" => crate::i18n::tr("automation.recipe_discord"),
        "Socials" => crate::i18n::tr("automation.recipe_socials"),
        "Hydrate" => crate::i18n::tr("automation.recipe_hydrate"),
        "Lurk" => crate::i18n::tr("automation.recipe_lurk"),
        "Discord" => crate::i18n::tr("automation.recipe_discord"),
        "Thank cheers" => crate::i18n::tr("automation.recipe_cheers"),
        "Thank gifted subs (Twitch community)" => crate::i18n::tr("automation.recipe_community_gifts"),
        name => name,
    }
}

fn action(name: &str, triggers: &[u32], subs: Vec<Obj>) -> Action {
    let mut a = new_action(localized_name(name), crate::i18n::tr("btn.templates"));
    a.triggers = triggers.iter().map(|&t| new_trigger(t)).collect();
    a.sub_actions = subs;
    crate::edit::renumber(&mut a.sub_actions);
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
            .map(|(p, t, text)| {
                action(
                    &format!("{} ({})", localized_name(name), title(p)),
                    t,
                    vec![say(p, text)],
                )
            })
            .collect(),
        ..Default::default()
    }
}

/// A timer and the action it runs, saying `text` on every platform (those not connected skip it).
fn timed(name: &str, minutes: u64, text: &str) -> Made {
    let name = localized_name(name);
    let timer_id = crate::engine::uuid();
    let timer = json!({"id": timer_id, "name": name, "enabled": true, "repeat": true, "interval": minutes * 60,
        "randomInterval": false, "upperInterval": 0, "lines": 0, "counter": 0});
    let mut a = action(
        name,
        &[],
        ["twitch", "youtube", "kick"].iter().map(|p| say(p, text)).collect(),
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
    let name = localized_name(name);
    let c = json!({"permittedUsers": [], "permittedGroups": [], "id": crate::engine::uuid(), "name": name,
        "enabled": true, "include": true, "mode": 0, "command": alias, "regexExplicitCapture": false,
        "location": 0, "ignoreBotAccount": true, "ignoreInternal": true,
        "sources": 1 | 1024 | 2_097_152 | 4_194_304, "persistCounter": false, "persistUserCounter": false,
        "caseSensitive": false, "globalCooldown": 0, "userCooldown": cooldown, "group": crate::i18n::tr("btn.templates"),
        "grantType": 0, "response": response, "replyAll": false});
    Made {
        commands: c.as_object().cloned().into_iter().collect(),
        ..Default::default()
    }
}

pub const TEMPLATES: &[Template] = &[
    Template {
        name: "automation.template_gifts",
        about: "automation.template_gifts_about",
        make: automation::gift_thanks,
    },
    Template {
        name: "automation.template_reward",
        about: "automation.template_reward_about",
        make: automation::reward_source,
    },
    Template {
        name: "Song requests (Spotify)",
        about: "automation.recipe_songs_about",
        make: songs::made,
    },
    Template {
        name: "Read chat aloud",
        about: "automation.recipe_tts_about",
        make: speech::made,
    },
    Template {
        name: "Welcome first chatters",
        about: "automation.recipe_welcome_about",
        make: || {
            per_platform(
                "Welcome first chatters",
                &[
                    ("twitch", &[120], crate::i18n::tr("automation.recipe_welcome_message")),
                    ("youtube", &[4016], crate::i18n::tr("automation.recipe_welcome_message")),
                    ("kick", &[35008], crate::i18n::tr("automation.recipe_welcome_message")),
                ],
            )
        },
    },
    Template {
        name: "Thank follows",
        about: "automation.recipe_follows_about",
        make: || {
            per_platform(
                "Thank follows",
                &[
                    ("twitch", &[101], crate::i18n::tr("automation.recipe_follow_message")),
                    ("kick", &[35011], crate::i18n::tr("automation.recipe_follow_message")),
                    (
                        "tiktok",
                        &[crate::tiktok::T_TIKTOK_FOLLOW],
                        crate::i18n::tr("automation.recipe_follow_message"),
                    ),
                ],
            )
        },
    },
    Template {
        name: "Thank subscriptions",
        about: "automation.recipe_subscriptions_about",
        make: || {
            per_platform(
                "Thank subscriptions",
                &[
                    (
                        "twitch",
                        &[103, 104],
                        crate::i18n::tr("automation.recipe_subscribe_message"),
                    ),
                    ("youtube", &[4008], crate::i18n::tr("automation.recipe_member_message")),
                    ("kick", &[35014], crate::i18n::tr("automation.recipe_subscribe_message")),
                ],
            )
        },
    },
    Template {
        name: "Thank gifted subs",
        about: "automation.recipe_gifts_about",
        make: || {
            let mut m = per_platform(
                "Thank gifted subs",
                &[
                    ("twitch", &[105], crate::i18n::tr("automation.recipe_gift_message")),
                    (
                        "kick",
                        &[35016, 35017],
                        crate::i18n::tr("automation.recipe_gifts_message"),
                    ),
                ],
            );
            m.actions.push(action(
                "Thank gifted subs (Twitch community)",
                &[106],
                vec![say(
                    "twitch",
                    crate::i18n::tr("automation.recipe_community_gifts_message"),
                )],
            ));
            m
        },
    },
    Template {
        name: "Thank cheers and Super Chats",
        about: "automation.recipe_cheers_about",
        make: || {
            per_platform(
                "Thank cheers",
                &[
                    ("twitch", &[102], crate::i18n::tr("automation.recipe_bits_message")),
                    (
                        "youtube",
                        &[4006],
                        crate::i18n::tr("automation.recipe_superchat_message"),
                    ),
                ],
            )
        },
    },
    Template {
        name: "Raid welcome and shoutout",
        about: "automation.recipe_raid_about",
        make: || Made {
            actions: vec![action(
                "Raid welcome and shoutout",
                &[107],
                vec![
                    say("twitch", crate::i18n::tr("automation.recipe_raid_message")),
                    with(new_sub(1002), json!({"value": 5000, "maxValue": 0, "random": false})),
                    with(new_sub(29), json!({"userLogin": "%userName%"})),
                ],
            )],
            ..Default::default()
        },
    },
    Template {
        name: "OBS scene on raid",
        about: "automation.recipe_raid_scene_about",
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
        about: "automation.recipe_socials_about",
        make: || timed("Socials", 20, crate::i18n::tr("automation.recipe_socials_message")),
    },
    Template {
        name: "Hydrate reminder",
        about: "automation.recipe_hydrate_about",
        make: || timed("Hydrate", 60, crate::i18n::tr("automation.recipe_hydrate_message")),
    },
    Template {
        name: "!lurk command",
        about: "automation.recipe_lurk_about",
        make: || command("Lurk", "!lurk", crate::i18n::tr("automation.recipe_lurk_message"), 30),
    },
    Template {
        name: "!discord command",
        about: "automation.recipe_discord_about",
        make: || {
            command(
                "Discord",
                "!discord",
                crate::i18n::tr("automation.recipe_discord_message"),
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
                !m.actions.is_empty() || !m.commands.is_empty() || !m.groups.is_empty(),
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

    #[test]
    fn add_to_validates_before_mutating_and_renumbers_nested_sub_actions() {
        // Invalid icon must fail before adding actions or groups.
        let mut c = crate::Config::default();
        let mut bad = automation::gift_thanks();
        bad.icons.push(("Bad".into(), b"not-an-image".to_vec()));
        assert!(bad.add_to(&mut c).is_err());
        assert!(c.actions.actions.is_empty());
        assert!(c.action_groups.is_empty());

        // Automation recipes add cleanly with disabled actions and valid settings.
        for recipe in [automation::gift_thanks(), automation::reward_source()] {
            let mut cfg = crate::Config::default();
            recipe.add_to(&mut cfg).unwrap();
            assert_eq!(cfg.actions.actions.len(), 1);
            assert!(!cfg.actions.actions[0].enabled);
        }

        // Nested sub-actions inside If/Else branches receive parentId and index.
        let songs = songs::made();
        let if_else = &songs.actions[0].sub_actions[1];
        let if_id = if_else["id"].as_str().unwrap();
        let branch = &if_else["subActions"][0];
        assert_eq!(branch["parentId"].as_str(), Some(if_id));
        assert_eq!(branch["index"], 0);
        assert_eq!(if_else["subActions"][1]["index"], 1);
    }
}
