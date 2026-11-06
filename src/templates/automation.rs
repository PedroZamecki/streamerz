//! Small editable recipes. They stay off until the user chooses the missing gift/reward/source.
use super::{action, with, Made};
use crate::{
    edit::new_sub,
    groups::{Def, Kind, Meta},
    i18n::tr,
};
use serde_json::json;

fn setting(name: &str, label: &'static str, kind: Kind, default: serde_json::Value) -> Def {
    Def {
        name: name.into(),
        label: tr(label).into(),
        kind,
        default,
        ..Default::default()
    }
}

pub fn gift_thanks() -> Made {
    let group = tr("automation.template_gifts");
    let mut gift = action(
        group,
        &[41002],
        vec![with(new_sub(43001), json!({"text":"%group.reply%"}))],
    );
    gift.group = group.into();
    gift.enabled = false;
    let meta = Meta {
        description: tr("automation.template_gifts_setup").into(),
        variables: vec![setting(
            "reply",
            "automation.reply",
            Kind::Text,
            json!(tr("automation.default_gift_reply")),
        )],
        ..Default::default()
    };
    Made {
        actions: vec![gift],
        groups: vec![(group.into(), meta)],
        ..Default::default()
    }
}

pub fn reward_source() -> Made {
    let group = tr("automation.template_reward");
    let source = |state| {
        with(
            new_sub(30),
            json!({"sceneName":"%group.scene%","sourceName":"%group.source%","state":state}),
        )
    };
    let mut reward = action(
        group,
        &[112],
        vec![
            source(0),
            with(new_sub(1002), json!({"value":"%group.duration%"})),
            source(1),
        ],
    );
    reward.group = group.into();
    reward.enabled = false;
    let meta = Meta {
        description: tr("automation.template_reward_setup").into(),
        variables: vec![
            setting("scene", "automation.scene", Kind::Text, json!("")),
            setting("source", "automation.source", Kind::Text, json!("")),
            setting("duration", "automation.visible_for", Kind::Duration, json!(10000)),
        ],
        ..Default::default()
    };
    Made {
        actions: vec![reward],
        groups: vec![(group.into(), meta)],
        ..Default::default()
    }
}
