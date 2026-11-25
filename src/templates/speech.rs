//! Read chat aloud: `!tts <text>` says it with the viewer's voice, `!voice <name>` picks one, `!skip`
//! (moderators) stops what is said, and a channel point reward reads its message. The filters are the
//! group's settings, so they are changed in one place for every command.
use super::{action, command, with, Made};
use crate::edit::{new_sub, new_trigger};
use crate::groups::{Def, Kind, Meta};
use crate::model::Obj;
use serde_json::{json, Value};

pub const GROUP: &str = "Read chat aloud";

fn def(name: &str, label: &str, kind: Kind, default: Value, description: &str) -> Def {
    Def {
        name: name.into(),
        label: label.into(),
        kind,
        default,
        description: description.into(),
        ..Default::default()
    }
}

fn meta() -> Meta {
    let mut volume = def("volume", "Volume (%)", Kind::Slider, json!(100), "");
    (volume.min, volume.max, volume.step) = (Some(0.0), Some(100.0), Some(5.0));
    Meta {
        description: "Reads chat aloud with the voices of this computer: !tts <text>, !voice <name> for a voice of \
                      one's own, !skip for moderators, and a channel point reward."
            .into(),
        variables: vec![
            def(
                "voice",
                "Voice",
                Kind::Text,
                json!(""),
                "Empty: the default of Sounds, Voices",
            ),
            def(
                "prefix",
                "Said before",
                Kind::Text,
                json!("%user% says"),
                "Empty: nothing",
            ),
            volume,
            def("skipLinks", "Skip links", Kind::Boolean, json!(true), ""),
            def("skipEmotes", "Skip emote names", Kind::Boolean, json!(true), ""),
            def(
                "maxRepeat",
                "A letter at most this many times",
                Kind::Integer,
                json!(3),
                "0: as written",
            ),
            def(
                "maxLength",
                "At most this many characters",
                Kind::Integer,
                json!(200),
                "0: all",
            ),
            def("blockedWords", "Words never read", Kind::Lines, json!(""), "One a line"),
            def(
                "skipUsers",
                "Users never read",
                Kind::Lines,
                json!("nightbot\nstreamelements\nstreamlabs\nmoobot\nfossabot"),
                "One a line: bots",
            ),
        ],
        ..Default::default()
    }
}

/// Speak with the group's settings.
fn speak(text: &str) -> Obj {
    with(
        new_sub(99320),
        json!({"text": text, "voice": "%group.voice%", "prefix": "%group.prefix%", "volume": "%group.volume%",
            "skipLinks": "%group.skipLinks%", "skipEmotes": "%group.skipEmotes%", "skipCommands": true,
            "maxRepeat": "%group.maxRepeat%", "maxLength": "%group.maxLength%",
            "blockedWords": "%group.blockedWords%", "skipUsers": "%group.skipUsers%", "finishBeforeContinuing": false}),
    )
}

pub fn made() -> Made {
    let mut made = Made::default();
    let commands: [(&str, &str, u64, Vec<Obj>, bool); 3] = [
        ("tts", "!tts", 10, vec![speak("%rawInput%")], false),
        (
            "voice",
            "!voice",
            5,
            vec![with(new_sub(99326), json!({"source": 0, "voice": "%rawInput%"}))],
            false,
        ),
        ("skip", "!skip", 0, vec![new_sub(99322)], true),
    ];
    for (what, alias, cooldown, subs, mods) in commands {
        let mut c = command(&format!("Read chat aloud: {what}"), alias, "", cooldown);
        c.commands[0].insert("group".into(), GROUP.into());
        if mods {
            c.commands[0].insert("permittedGroups".into(), json!(["Moderators"]));
        }
        let id = c.commands[0]["id"].as_str().unwrap_or_default().to_string();
        let mut a = action(&format!("Read chat aloud: {what}"), &[], subs);
        a.group = GROUP.into();
        a.triggers = vec![with(new_trigger(401), json!({"commandId": id}))];
        made.actions.push(a);
        made.commands.extend(c.commands);
    }
    // A reward's message, read aloud: off until a reward is picked in its trigger.
    let mut reward = action("Read chat aloud: reward", &[], vec![speak("%rawInput%")]);
    reward.group = GROUP.into();
    reward.enabled = false;
    reward.triggers = vec![new_trigger(112)];
    made.actions.push(reward);
    made.groups.push((GROUP.into(), meta()));
    made
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_adds_its_commands_settings_and_a_reward_left_off() {
        let mut c = crate::Config::default();
        made().add_to(&mut c).unwrap();
        let names: Vec<&str> = c
            .commands
            .iter()
            .filter_map(|x| x.get("command").and_then(Value::as_str))
            .collect();
        assert_eq!(names, ["!tts", "!voice", "!skip"]);
        let skip = &c.commands[2];
        assert_eq!(skip["permittedGroups"], json!(["Moderators"]), "only moderators skip");
        assert_eq!(c.actions.actions.len(), 4);
        assert!(
            !c.actions.actions[3].enabled,
            "the reward waits for a reward to be picked"
        );
        assert!(crate::groups::meta(&c, GROUP).validate().is_ok());
        assert_eq!(crate::groups::values(&c, GROUP)["maxRepeat"], 3);
        // The commands' actions use the group's settings.
        assert_eq!(c.actions.actions[0].sub_actions[0]["skipLinks"], "%group.skipLinks%");
    }
}
