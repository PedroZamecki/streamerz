//! How triggers and sub-actions read in a list, and which icon they wear. Toolkit independent.

use super::catalog as picker;
use crate::edit;
use crate::engine::plugins::{SUB_PLUGIN, T_PLUGIN};
use crate::engine::DEFAULT_QUEUE;
use crate::i18n::tr;
use crate::model::Obj;
use crate::types::{name, SUB_ACTION_INFO, TRIGGERS};
use crate::Config;
use serde_json::Value;

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

fn short(t: &str) -> String {
    let t = t.replace(['\r', '\n'], " ");
    if t.chars().count() > 60 {
        format!("{}...", t.chars().take(60).collect::<String>())
    } else {
        t
    }
}

/// Names by id (actions, queues, timers, commands, user groups, WebSocket clients and servers, OBS
/// and Streamlabs connections), so labels show names rather than ids.
pub type Names = std::collections::HashMap<String, String>;

pub fn names(c: &Config) -> Names {
    let objs = [
        &c.actions.queues,
        &c.timers,
        &c.commands,
        &c.groups,
        &c.websockets.connections,
        &c.websockets.servers,
        &c.obs.connections,
        &c.slobs.connections,
    ];
    let mut out: Names = objs
        .into_iter()
        .flatten()
        .map(|o| (s(o, "id").to_string(), s(o, "name").to_string()))
        .collect();
    out.extend(c.actions.actions.iter().map(|a| (a.id.clone(), a.name.clone())));
    out.insert(DEFAULT_QUEUE.into(), "Default".into());
    out
}

/// The title of a sub-action type.
pub fn sub_title(k: u64) -> String {
    if let Some(title) = picker::spotify_player_title(k as u32) {
        return title.to_string();
    }
    match k {
        99802 => tr("spotify.action_name").into(),
        99803 => tr("countdown.action_name").into(),
        99880 => tr("overlay.action_name").into(),
        _ => SUB_ACTION_INFO.binary_search_by_key(&(k as u32), |x| x.0).map_or_else(
            |_| {
                name(crate::types::SUB_ACTIONS, k as u32)
                    .unwrap_or("Sub-action")
                    .to_string()
            },
            |i| SUB_ACTION_INFO[i].1.to_string(),
        ),
    }
}

/// How a sub-action reads in the list.
pub fn sub_label(o: &Obj, names: &Names) -> String {
    let k = edit::kind(o);
    if k == SUB_PLUGIN {
        return format!("{}: {}", s(o, "plugin"), s(o, "name"));
    }
    let what = sub_title(k);
    let v = |k: &str| o.get(k).map(crate::vars::text).unwrap_or_default();
    let detail = match k {
        99901 => return "True".into(),
        99902 => return "False".into(),
        99904 => return "Default".into(),
        99903 => {
            let vals: Vec<String> = o
                .get("values")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(crate::vars::text).collect())
                .unwrap_or_default();
            return format!("Case: {}", short(&vals.join(", ")));
        }
        99900 => format!(
            "{}{}",
            v("name"),
            if o.get("random").and_then(Value::as_bool) == Some(true) {
                " (random)"
            } else {
                ""
            }
        ),
        120 | 128 => {
            let op = o.get("operation").and_then(Value::as_u64).unwrap_or(0) as usize;
            let op = if k == 120 {
                super::schema::OPS.get(op)
            } else {
                super::schema::WHILE_OPS.get(op)
            }
            .copied()
            .unwrap_or("?");
            format!("{} {op} {}", v("input"), v("value"))
        }
        123 | 122 => format!("{} = {}", v("variableName"), v("value")),
        121 => format!("{} -> {}", v("variableName"), v("destinationVariable")),
        1002 => match o.get("value").and_then(Value::as_u64) {
            Some(ms) => crate::groups::format_duration(ms),
            None => v("value"),
        },
        200..=202 | 204..=210 | 213..=215 => {
            let id = v("rewardId");
            crate::catalog::name_of("twitch.rewards", &id).unwrap_or(id)
        }
        35101..=35103 | 35105..=35110 | 35113 => {
            let id = v("rewardId");
            crate::catalog::name_of("kick.rewards", &id).unwrap_or(id)
        }
        // Its clock is an ID nobody reads: say what it does to the clock instead.
        99803 => match v("operation").as_str() {
            "add" => tr("countdown.op_add").to_string(),
            "reset" => tr("countdown.op_reset").to_string(),
            other => other.to_string(),
        },
        1009 => v("value"),
        1020 => v("message"),
        _ => o
            .iter()
            .filter(|(k, _)| {
                !["id", "weight", "type", "parentId", "enabled", "index", "subActions"].contains(&k.as_str())
            })
            .find_map(|(_, x)| x.as_str().filter(|t| !t.is_empty()))
            .map(|t| names.get(t).map_or(t, |n| n.as_str()).to_string())
            .unwrap_or_default(),
    };
    let mut out = if detail.is_empty() {
        what
    } else {
        format!("{what}: {}", short(&detail))
    };
    if !crate::engine::supported(k as u32) {
        out += tr("actions.not_supported");
    }
    out
}

/// The title of a trigger type, with its platform.
pub fn trigger_title(k: u64) -> String {
    match k {
        706 => tr("automation.started").to_string(),
        707 => tr("automation.exiting").to_string(),
        _ if name(TRIGGERS, k as u32).is_some() => {
            let raw = name(TRIGGERS, k as u32).unwrap_or_default();
            let cat = picker::trigger_categorize(raw);
            let platform = cat.split('/').next().unwrap_or(&cat);
            let clean = picker::trigger_item_title(k as u32, raw);
            format!("{platform}: {clean}")
        }
        _ => tr("actions.unknown").to_string(),
    }
}

pub fn trigger_label(t: &Obj, names: &Names) -> String {
    let k = t.get("type").and_then(Value::as_u64).unwrap_or(0) as u32;
    if k == T_PLUGIN {
        return format!("{}: {}", s(t, "plugin"), s(t, "event"));
    }
    let what = trigger_title(u64::from(k));
    let n = |f: &str| t.get(f).and_then(Value::as_i64).unwrap_or(0);
    let detail = if k == crate::hotkeys::T_KEY_PRESS {
        crate::hotkeys::label(n("keyCode"), n("modifierMask"))
    } else if k == crate::tiktok::T_TIKTOK_GIFT {
        let mut parts = Vec::new();
        let id = t.get("giftId").map(crate::vars::text).unwrap_or_default();
        if !id.trim().is_empty() {
            parts.push(crate::catalog::name_of("tiktok.gifts", &id).unwrap_or_else(|| format!("#{id}")));
        }
        if t.get("giftDelivery").and_then(Value::as_i64) == Some(1) {
            parts.push(tr("automation.completed_combo").into());
        } else if !parts.is_empty() {
            parts.push(tr("automation.every_update").into());
        }
        let min = t.get("minCoins").and_then(Value::as_u64).unwrap_or(0);
        let max = t.get("maxCoins").and_then(Value::as_u64).unwrap_or(0);
        match (min, max) {
            (0, 0) => {}
            (lo, 0) => parts.push(format!("{lo}+ coins")),
            (0, hi) => parts.push(format!("<= {hi} coins")),
            (lo, hi) if lo == hi => parts.push(format!("{lo} coins")),
            (lo, hi) => parts.push(format!("{lo}-{hi} coins")),
        }
        parts.join(", ")
    } else if matches!(k, 112..=115 | 171 | 35024) {
        let id = t.get("rewardId").map(crate::vars::text).unwrap_or_default();
        let cat = if k == 35024 { "kick.rewards" } else { "twitch.rewards" };
        if id.trim().is_empty() {
            String::new()
        } else {
            crate::catalog::name_of(cat, &id).unwrap_or(id)
        }
    } else {
        t.iter()
            .filter(|(k, _)| !["id", "type", "enabled", "exclusions"].contains(&k.as_str()))
            .filter_map(|(k, x)| {
                let x = crate::vars::text(x);
                let x = names.get(&x).cloned().unwrap_or(x);
                (!x.is_empty()).then(|| format!("{}: {x}", k.trim_end_matches("Id")))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    if detail.is_empty() {
        what
    } else {
        format!("{what} ({})", short(&detail))
    }
}

/// Whether a trigger is switched on.
pub fn trigger_on(t: &Obj) -> bool {
    t.get("enabled").and_then(Value::as_bool).unwrap_or(true)
}

/// The Lucide icon of a trigger (a plugin's, or an unknown one, wears the puzzle).
pub fn trigger_icon(code: u32) -> &'static str {
    match name(TRIGGERS, code) {
        Some(raw) => {
            let cat = picker::trigger_categorize(raw);
            let top = cat.rsplit('/').next().unwrap_or(&cat).trim().to_owned();
            super::pick_icons::item(&picker::trigger_clean_title(raw), super::pick_icons::group(&top))
        }
        None => "puzzle",
    }
}

/// The Lucide icon of a sub-action (a plugin's, or an unknown one, wears the puzzle).
pub fn sub_icon(code: u32) -> &'static str {
    let Ok(i) = SUB_ACTION_INFO.binary_search_by_key(&code, |x| x.0) else {
        return "puzzle";
    };
    let (_, name, category, _) = SUB_ACTION_INFO[i];
    let title = picker::spotify_player_title(code).unwrap_or(name);
    let category = if category.is_empty() { "Core/General" } else { category };
    let top = category.rsplit('/').next().unwrap_or(category).trim();
    super::pick_icons::item(title, super::pick_icons::group(top))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_read_like_the_old_editor_and_icons_exist() {
        let names = Names::new();
        let mut sub = edit::new_sub(1009);
        sub.insert("value".into(), "hello".into());
        assert!(sub_label(&sub, &names).ends_with("hello"));
        assert_eq!(sub_label(&edit::new_sub(99901), &names), "True");
        let trigger = edit::new_trigger(101);
        assert!(trigger_label(&trigger, &names).starts_with("Twitch: "));
        assert!(!trigger_icon(101).is_empty());
        assert_eq!(trigger_icon(u32::MAX), "puzzle");
        assert_eq!(sub_icon(u32::MAX), "puzzle");
    }
}
