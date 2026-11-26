//! The Rewards page: the channel rewards of Twitch and Kick in one list, with what Streamerz counts and
//! groups (Streamer.bot keeps both itself). A row is a reward by `platform:id`; Twitch and Kick let an
//! app change only the rewards it made, so the others are read only, with the reason.

use super::view::{column, note};
use crate::automation::schema::{F, W};
use crate::engine::Engine;
use crate::i18n::{tr, trf};
use crate::model::Obj;
use serde_json::{json, Value};
use std::rc::Rc;

pub fn supports(domain: &str) -> bool {
    domain == "rewards"
}

/// One reward as the page shows it.
struct Reward {
    key: String,
    platform: &'static str,
    id: String,
    title: String,
    cost: u64,
    prompt: String,
    color: String,
    enabled: bool,
    paused: bool,
    input: bool,
    /// Whether Streamerz may change it; None: not known (Kick doesn't say).
    ours: Option<bool>,
}

fn rewards(engine: &Rc<Engine>) -> Vec<Reward> {
    let made = engine.config().rewards.made.clone();
    let text = |r: &Value, k: &str| r[k].as_str().unwrap_or_default().to_string();
    let mut out: Vec<Reward> = crate::twitch::rewards_known()
        .iter()
        .filter_map(|r| {
            let id = r["id"].as_str()?.to_string();
            Some(Reward {
                key: crate::rewards::key("twitch", &id),
                platform: "twitch",
                ours: crate::twitch::reward_ours(&id),
                title: text(r, "title"),
                cost: r["cost"].as_u64().unwrap_or(0),
                prompt: text(r, "prompt"),
                color: text(r, "background_color"),
                enabled: r["is_enabled"].as_bool().unwrap_or(true),
                paused: r["is_paused"].as_bool().unwrap_or(false),
                input: r["is_user_input_required"].as_bool().unwrap_or(false),
                id,
            })
        })
        .collect();
    out.extend(crate::kick::rewards_known().iter().filter_map(|r| {
        let id = r["id"].as_str()?.to_string();
        let key = crate::rewards::key("kick", &id);
        Some(Reward {
            ours: made.contains(&key).then_some(true),
            key,
            platform: "kick",
            title: text(r, "title"),
            cost: r["cost"].as_u64().unwrap_or(0),
            prompt: text(r, "description"),
            color: text(r, "background_color"),
            enabled: r["is_enabled"].as_bool().unwrap_or(true),
            paused: r["is_paused"].as_bool().unwrap_or(false),
            input: r["is_user_input_required"].as_bool().unwrap_or(false),
            id,
        })
    }));
    out.sort_by(|a, b| (a.platform, a.title.to_lowercase()).cmp(&(b.platform, b.title.to_lowercase())));
    out
}

fn find(engine: &Rc<Engine>, key: &str) -> Result<Reward, String> {
    rewards(engine)
        .into_iter()
        .find(|r| r.key == key)
        .ok_or_else(|| tr("actions.item_gone").to_string())
}

/// The page opened: ask the platforms again (once per opening), in the background.
pub fn opened(engine: &Rc<Engine>) {
    reload(engine);
}

/// Platform validation limits for custom channel point rewards.
const TWITCH_TITLE_MAX: usize = 45;
const KICK_TITLE_MAX: usize = 50;
const PROMPT_MAX: usize = 200;
const TWITCH_COST_MAX: u64 = 1_000_000_000;

/// Syncs Kick's reward cache with a full `/channels/rewards` response, removing deleted rewards.
fn sync_kick_rewards(list: &Value) {
    let Some(items) = list["data"].as_array() else {
        return;
    };
    let mut seen = std::collections::HashSet::with_capacity(items.len());
    for r in items {
        if let Some(id) = r["id"].as_str() {
            seen.insert(id.to_owned());
            crate::kick::reward_saved(r);
        }
    }
    for old in crate::kick::rewards_known() {
        if let Some(id) = old["id"].as_str().filter(|id| !seen.contains(*id)) {
            crate::kick::reward_removed(id);
        }
    }
}

fn reload(engine: &Rc<Engine>) {
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        let twitch = async {
            if engine.twitch().is_some() {
                if let Err(e) = crate::twitch::reload_rewards(&engine).await {
                    crate::warn(format!("Rewards: Twitch: {e}"));
                }
            }
        };
        let kick = async {
            if let Some(kick) = engine.kick().filter(|_| engine.config().kick.broadcaster.is_some()) {
                match kick.api("GET", "/channels/rewards", None).await {
                    Ok(list) => sync_kick_rewards(&list),
                    Err(e) => crate::warn(format!("Rewards: Kick: {e}")),
                }
            }
        };
        tokio::join!(twitch, kick);
        crate::host::changed();
    });
}

pub fn read(engine: &Rc<Engine>) -> Result<Value, String> {
    let list = rewards(engine);
    let config = engine.config();
    let book = &config.rewards;
    let rows: Vec<Value> = list
        .iter()
        .map(|r| {
            let (count, _) = book.count(&r.key, "");
            let group = book.group_of(&r.key).unwrap_or_default();
            let ours = r.ours != Some(false);
            let state = if r.paused {
                note("pause", tr("rewards.paused"))
            } else if r.enabled {
                note("power", tr("rewards.on"))
            } else {
                note("power-off", tr("rewards.off"))
            };
            let mut row = json!({"id": r.key, "name": r.title, "enabled": r.enabled,
                "can": {"edit": ours, "toggle": ours, "pause": ours && r.enabled, "delete": ours, "reset": count > 0},
                "cells": [{"brand": r.platform, "text": r.title}, r.cost.to_string(), note("folder", group),
                    note("hash", trf("rewards.redeemed", &[("count", &count.to_string())])), state]});
            if !ours {
                row["tip"] = tr("rewards.not_ours").into();
            }
            row
        })
        .collect();
    let logged_in = config.twitch.broadcaster.is_some() || config.kick.broadcaster.is_some();
    let top = |mut b: Value, primary: bool| {
        b["top"] = true.into();
        if primary {
            b["primary"] = true.into();
        }
        b
    };
    let mut new = super::pages::button_if(super::pages::button("new", "btn.new_reward", "plus", false), logged_in);
    new["form"] = "reward_new:".into();
    let mut edit = super::pages::button("edit", "btn.edit", "pencil", true);
    edit["form"] = "reward_edit:".into();
    let mut group = super::pages::button("group", "rewards.set_group", "folder", true);
    group["form"] = "reward_group:".into();
    let mut table = super::pages::table(
        "reward",
        "",
        if logged_in {
            tr("rewards.empty")
        } else {
            tr("rewards.empty_no_login")
        },
        vec![
            column("reward", "rewards.reward", 240),
            column("cost", "rewards.cost", 80),
            column("group", "rewards.group", 140),
            column("redeemed", "rewards.redeemed_column", 120),
            column("state", "rewards.state", 100),
        ],
        rows,
        vec![
            top(new, true),
            top(
                json!({"id": "refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "iconOnly": true}),
                false,
            ),
            edit,
            super::pages::button("toggle", "btn.on_off", "power", true),
            super::pages::button("pause", "btn.pause_resume", "pause", true),
            group,
            super::pages::confirm(
                super::pages::button("reset", "rewards.reset_counter", "rotate-ccw", true),
                tr("rewards.reset_confirm").into(),
                "rewards.reset_counter",
            ),
            super::pages::danger(super::pages::confirm(
                super::pages::button("delete", "btn.delete", "trash-2", true),
                tr("rewards.delete_confirm").into(),
                "btn.delete",
            )),
        ],
    );
    // Nothing to show without a login: say where to get one.
    if !logged_in {
        if let Some(actions) = table["actions"].as_array_mut() {
            actions.push(
                json!({"id": "accounts", "label": tr("nav.page.accounts"), "icon": "user", "top": true,
                "navigate": "Accounts", "needs": "none"}),
            );
        }
    }
    table["guide"] = tr("rewards.guide").into();
    table["guideBelow"] = true.into();
    table["titleless"] = true.into();
    table["strip"] = true.into();
    table["domain"] = "rewards".into();
    Ok(json!({"domain": "rewards", "tables": [table]}))
}

/// What a row's button does.
pub fn act(engine: &Rc<Engine>, action: &str, key: &str) -> Result<Value, String> {
    let accepted = Ok(json!({"accepted": true}));
    if action == "refresh" {
        reload(engine);
        return accepted;
    }
    let r = find(engine, key)?;
    match action {
        "reset" => {
            engine.change(|c| {
                c.rewards.reset(&r.key, false);
                c.rewards.reset(&r.key, true);
            });
            accepted
        }
        "toggle" | "pause" | "delete" => {
            if r.ours == Some(false) {
                return Err(tr("rewards.not_ours").into());
            }
            match action {
                "toggle" => change(engine, &r, json!({"is_enabled": !r.enabled}), None),
                "pause" => {
                    if !r.enabled {
                        return Err("Enable the reward before pausing or resuming it.".into());
                    }
                    change(engine, &r, json!({"is_paused": !r.paused}), None);
                }
                "delete" => {
                    let (code, field) = if r.platform == "twitch" {
                        (99523, "rewardId")
                    } else {
                        (99604, "rewardId")
                    };
                    run(engine, code, json!({ field: r.id }), &r.title);
                }
                _ => unreachable!(),
            }
            accepted
        }
        _ => Err(format!("rewards has no action {action}")),
    }
}

/// Changes a reward on its platform, and says plainly when it couldn't.
fn change(engine: &Rc<Engine>, r: &Reward, body: Value, group: Option<String>) {
    let (engine, platform, id, key, title) = (engine.clone(), r.platform, r.id.clone(), r.key.clone(), r.title.clone());
    tokio::task::spawn_local(async move {
        let result = if platform == "twitch" {
            match engine.twitch() {
                Some(tw) => {
                    let q = vec![("broadcaster_id", "{broadcaster_id}"), ("id", id.as_str())];
                    tw.helix("PATCH", "channel_points/custom_rewards", q, Some(body), false, false)
                        .await
                        .map(|v| crate::twitch::reward_saved(&v["data"][0]))
                        .map_err(|e| crate::twitch::plain_error(&e))
                }
                None => Err(tr("rewards.no_twitch").into()),
            }
        } else {
            match engine.kick() {
                Some(kick) => {
                    let path = format!("/channels/rewards/{}", crate::commands::url_encode(&id));
                    kick.api("PATCH", &path, Some(body))
                        .await
                        .map(|v| crate::kick::reward_saved(&v["data"]))
                }
                None => Err("Kick isn't connected".into()),
            }
        };
        match result {
            Ok(()) => {
                if let Some(group) = group {
                    engine.change(|c| c.rewards.set_group(&key, &group));
                }
            }
            Err(e) => {
                super::prompts::notice(title, trf("rewards.failed", &[("error", &e)]));
                crate::host::prompts_changed();
            }
        }
        crate::host::changed();
    });
}

/// Runs one of the reward sub-actions for the page, and says plainly when it failed.
fn run(engine: &Rc<Engine>, code: u64, settings: Value, title: &str) {
    let (engine, title) = (engine.clone(), title.to_string());
    let settings = settings.as_object().cloned().unwrap_or_default();
    tokio::task::spawn_local(async move {
        let (_, outcome) = engine.execute_one(code, settings).await;
        if let Some(error) = outcome.error {
            super::prompts::notice(title, trf("rewards.failed", &[("error", &error)]));
            crate::host::prompts_changed();
        }
        crate::host::changed();
    });
}

// The forms: a new reward, a reward's settings, its group.

const PLATFORM: &[&str] = &["Twitch", "Kick"];
const NEW_FIELDS: &[F] = &[
    ("platform", "rewards.platform", W::Pick(PLATFORM)),
    ("title", "tools.title", W::Text),
    ("cost", "tools.cost", W::Int),
    ("prompt", "tools.prompt", W::Multi),
    ("inputRequired", "tools.input_required", W::Bool),
    ("backgroundColor", "tools.color", W::Color),
    ("enabled", "tools.enabled", W::Bool),
    ("group", "rewards.group", W::Text),
];
const EDIT_FIELDS: &[F] = &[
    ("title", "tools.title", W::Text),
    ("cost", "tools.cost", W::Int),
    ("prompt", "tools.prompt", W::Multi),
    ("inputRequired", "tools.input_required", W::Bool),
    ("backgroundColor", "tools.color", W::Color),
    ("enabled", "tools.enabled", W::Bool),
    ("paused", "rewards.paused", W::Bool),
    ("group", "rewards.group", W::Text),
];
const GROUP_FIELDS: &[F] = &[("group", "rewards.group", W::Text)];

pub fn external_fields(kind: &str) -> Option<&'static [F]> {
    if kind.starts_with("reward_new:") {
        Some(NEW_FIELDS)
    } else if kind.starts_with("reward_edit:") {
        Some(EDIT_FIELDS)
    } else if kind.starts_with("reward_group:") {
        Some(GROUP_FIELDS)
    } else {
        None
    }
}

fn form_json(kind: &str, title: String, ok: &str, fields: &[F], item: Value, groups: &[String]) -> Value {
    let object = item.as_object().cloned().unwrap_or_default();
    let fields: Vec<Value> = fields
        .iter()
        .map(|(key, label, widget)| {
            let label = crate::automation::schema::field_label(label);
            let mut field = json!({"key": key, "label": label, "widget": widget, "holds": widget.about(),
                "value": crate::automation::edits::shown(widget, object.get(*key))});
            match widget {
                W::Pick(options) => field["widget"]["options"] = json!(options),
                W::Color => field["widget"] = json!({"kind": "color"}),
                _ => {}
            }
            // A group is typed, or one of those there are picked.
            if *key == "group" && !groups.is_empty() {
                field["choices"] = groups.iter().map(|g| json!({"id": g, "label": g})).collect();
                field["strict"] = false.into();
                field["other"] = true.into();
            }
            field
        })
        .collect();
    json!({"kind": kind, "title": title, "okLabel": ok, "json": false, "new": item.clone(), "fields": fields, "item": item})
}

pub fn external_form(engine: &Rc<Engine>, kind: &str) -> Option<Result<Value, String>> {
    let fields = external_fields(kind)?;
    let groups: Vec<String> = engine.config().rewards.groups.keys().cloned().collect();
    if kind.starts_with("reward_new:") {
        let default_platform = {
            let c = engine.config();
            usize::from(c.twitch.broadcaster.is_none() && c.kick.broadcaster.is_some())
        };
        let item = json!({"id": "", "platform": default_platform, "title": "", "cost": 100, "prompt": "", "inputRequired": false,
            "backgroundColor": "", "enabled": true, "group": ""});
        return Some(Ok(form_json(
            kind,
            tr("btn.new_reward").into(),
            tr("btn.create"),
            fields,
            item,
            &groups,
        )));
    }
    let key = kind.split_once(':').map_or("", |(_, key)| key);
    Some(find(engine, key).and_then(|r| {
        if kind.starts_with("reward_edit:") && r.ours == Some(false) {
            return Err(tr("rewards.not_ours").into());
        }
        let group = engine.config().rewards.group_of(&r.key).unwrap_or_default().to_string();
        let item = json!({"id": r.key, "title": r.title, "cost": r.cost, "prompt": r.prompt, "inputRequired": r.input,
            "backgroundColor": r.color, "enabled": r.enabled, "paused": r.paused, "group": group});
        Ok(form_json(kind, r.title.clone(), tr("btn.save"), fields, item, &groups))
    }))
}

fn get<'a>(item: &'a Obj, key: &str) -> &'a str {
    item.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn parse_cost(value: Option<&Value>, platform: &str) -> Result<u64, String> {
    let raw = value
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
        })
        .ok_or_else(|| "Reward cost must be a whole number of at least 1.".to_string())?;
    if raw < 1 {
        return Err("Reward cost must be at least 1.".into());
    }
    let cost = raw as u64;
    if platform == "twitch" && cost > TWITCH_COST_MAX {
        return Err(format!("Twitch reward cost cannot exceed {TWITCH_COST_MAX}."));
    }
    Ok(cost)
}

fn validate_reward(platform: &str, title: &str, prompt: &str, cost: Option<&Value>) -> Result<u64, String> {
    if title.is_empty() {
        return Err(tr("rewards.needs_title").into());
    }
    let max_title = if platform == "twitch" {
        TWITCH_TITLE_MAX
    } else {
        KICK_TITLE_MAX
    };
    if title.chars().count() > max_title {
        let where_name = if platform == "twitch" { "Twitch" } else { "Kick" };
        return Err(format!("{where_name} reward titles take up to {max_title} characters."));
    }
    if prompt.chars().count() > PROMPT_MAX {
        return Err(format!("Reward prompt takes up to {PROMPT_MAX} characters."));
    }
    parse_cost(cost, platform)
}

fn normalize_color(color: &str) -> Option<String> {
    let rgb = color.trim().trim_start_matches('#');
    (rgb.len() >= 6 && rgb.bytes().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", &rgb[rgb.len() - 6..]).to_uppercase())
}

pub fn save_external(engine: &Rc<Engine>, kind: &str, item: Obj) -> Option<Result<Value, String>> {
    external_fields(kind)?;
    let key = kind.split_once(':').map_or("", |(_, key)| key).to_string();
    let group = get(&item, "group").trim().to_string();
    if kind.starts_with("reward_group:") {
        return Some(find(engine, &key).map(|r| {
            engine.change(|c| c.rewards.set_group(&r.key, &group));
            json!({"saved": r.key})
        }));
    }
    let title = get(&item, "title").trim().to_string();
    let flag = |k: &str| item.get(k).and_then(Value::as_bool).unwrap_or(false);
    let prompt = get(&item, "prompt").to_string();
    let color = get(&item, "backgroundColor").to_string();
    if kind.starts_with("reward_new:") {
        let kick = item.get("platform").and_then(Value::as_u64) == Some(1);
        let platform = if kick { "kick" } else { "twitch" };
        let cost = match validate_reward(platform, &title, &prompt, item.get("cost")) {
            Ok(cost) => cost,
            Err(error) => return Some(Err(error)),
        };
        let (code, text) = if kick {
            (99603, "description")
        } else {
            (99522, "prompt")
        };
        let settings = json!({"title": title, "cost": cost, text: prompt, "inputRequired": flag("inputRequired"),
            "backgroundColor": color, "enabled": flag("enabled")});
        // The group, once the reward exists.
        let engine = engine.clone();
        tokio::task::spawn_local(async move {
            let settings = settings.as_object().cloned().unwrap_or_default();
            let (after, outcome) = engine.execute_one(code, settings).await;
            match (outcome.error, after.get("rewardId").and_then(Value::as_str)) {
                (Some(error), _) => {
                    super::prompts::notice(title, trf("rewards.failed", &[("error", &error)]));
                    crate::host::prompts_changed();
                }
                (None, Some(id)) if !group.is_empty() => {
                    let key = crate::rewards::key(platform, id);
                    engine.change(|c| c.rewards.set_group(&key, &group));
                }
                _ => {}
            }
            crate::host::changed();
        });
        return Some(Ok(json!({"accepted": true})));
    }
    Some(find(engine, &key).and_then(|r| {
        if r.ours == Some(false) {
            return Err(tr("rewards.not_ours").into());
        }
        let cost = validate_reward(r.platform, &title, &prompt, item.get("cost"))?;
        let text = if r.platform == "twitch" { "prompt" } else { "description" };
        let mut body = json!({"title": title, "cost": cost, text: prompt, "is_user_input_required": flag("inputRequired"),
            "is_enabled": flag("enabled"), "is_paused": flag("paused")});
        if let Some(hex) = normalize_color(&color) {
            body["background_color"] = hex.into();
        }
        change(engine, &r, body, Some(group));
        Ok(json!({"saved": r.key}))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(value: Value) -> Obj {
        value.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn rewards_made_elsewhere_are_read_only_and_counts_and_groups_show() {
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            crate::twitch::rewards_for_test(
                vec![
                    json!({"id": "a", "title": "Hydrate", "cost": 100, "is_enabled": true}),
                    json!({"id": "b", "title": "Emote", "cost": 50, "is_enabled": false}),
                ],
                &["a"],
            );
            engine.change(|c| {
                c.rewards.redeemed("twitch:a", "5");
                c.rewards.set_group("twitch:a", "Drinks");
            });
            let page = read(&engine).unwrap();
            // Logged in (the test has rewards, and the config has no login): the page offers Accounts.
            assert!(page["tables"][0]["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|action| action["navigate"] == "Accounts"));
            let rows = page["tables"][0]["rows"].as_array().unwrap();
            let row = |id: &str| rows.iter().find(|r| r["id"] == id).unwrap().clone();
            let (a, b) = (row("twitch:a"), row("twitch:b"));
            assert_eq!((&a["can"]["edit"], &a["can"]["reset"]), (&json!(true), &json!(true)));
            assert_eq!(a["cells"][0], json!({"brand": "twitch", "text": "Hydrate"}));
            assert_eq!(a["cells"][2]["text"], "Drinks");
            assert!(a["cells"][3]["text"].as_str().unwrap().starts_with('1'), "{a}");
            // Made elsewhere: shown, read only, with why.
            assert_eq!(
                (&b["can"]["edit"], &b["can"]["delete"], &b["enabled"]),
                (&json!(false), &json!(false), &json!(false))
            );
            assert!(b["tip"].as_str().is_some_and(|tip| !tip.is_empty()));
            // A group may hold any reward; the count starts again.
            save_external(&engine, "reward_group:twitch:b", obj(json!({"group": "drinks"})))
                .unwrap()
                .unwrap();
            assert_eq!(engine.config().rewards.in_group("twitch", "Drinks").len(), 2);
            act(&engine, "reset", "twitch:a").unwrap();
            assert_eq!(engine.config().rewards.count("twitch:a", "5"), (0, 0));
            // The forms: a reward's own values, and a new one needs a title.
            let form = external_form(&engine, "reward_edit:twitch:a").unwrap().unwrap();
            assert_eq!(
                (&form["item"]["title"], &form["item"]["group"]),
                (&json!("Hydrate"), &json!("Drinks"))
            );
            let new = external_form(&engine, "reward_new:").unwrap().unwrap();
            assert_eq!(new["fields"][0]["widget"]["options"], json!(["Twitch", "Kick"]));
            assert!(save_external(&engine, "reward_new:", obj(json!({"title": " "})))
                .unwrap()
                .is_err());
            assert!(act(&engine, "reset", "twitch:gone").is_err());
        });
    }

    #[test]
    fn reward_validation_enforces_platform_limits_and_cost_parsing() {
        assert_eq!(
            validate_reward("twitch", "Hydrate", "", Some(&json!(100))).unwrap(),
            100
        );
        assert_eq!(
            validate_reward("twitch", "Hydrate", "", Some(&json!(" 250 "))).unwrap(),
            250
        );
        assert!(validate_reward("twitch", "", "", Some(&json!(10))).is_err());
        assert!(validate_reward("twitch", "Hydrate", "", Some(&json!(0))).is_err());
        assert!(validate_reward("twitch", "Hydrate", "", Some(&json!(-5))).is_err());
        assert!(validate_reward("twitch", "Hydrate", "", Some(&json!("abc"))).is_err());
        assert!(validate_reward("twitch", "Hydrate", "", None).is_err());
        assert!(validate_reward("twitch", "Hydrate", "", Some(&json!(TWITCH_COST_MAX + 1))).is_err());
        assert_eq!(
            validate_reward("twitch", "Hydrate", "", Some(&json!(TWITCH_COST_MAX))).unwrap(),
            TWITCH_COST_MAX
        );
        // Title limits: 45 on Twitch, 50 on Kick (counted in Unicode chars).
        let t45 = "é".repeat(TWITCH_TITLE_MAX);
        let t46 = "é".repeat(TWITCH_TITLE_MAX + 1);
        let k50 = "é".repeat(KICK_TITLE_MAX);
        let k51 = "é".repeat(KICK_TITLE_MAX + 1);
        assert!(validate_reward("twitch", &t45, "", Some(&json!(1))).is_ok());
        assert!(validate_reward("twitch", &t46, "", Some(&json!(1))).is_err());
        assert!(validate_reward("kick", &k50, "", Some(&json!(1))).is_ok());
        assert!(validate_reward("kick", &k51, "", Some(&json!(1))).is_err());
        // Prompt / description limit: 200 chars.
        assert!(validate_reward("twitch", "Ok", &"a".repeat(PROMPT_MAX), Some(&json!(1))).is_ok());
        assert!(validate_reward("twitch", "Ok", &"a".repeat(PROMPT_MAX + 1), Some(&json!(1))).is_err());
        assert!(validate_reward("kick", "Ok", &"a".repeat(PROMPT_MAX + 1), Some(&json!(1))).is_err());
    }

    #[test]
    fn normalize_color_accepts_rgb_and_argb_hex_only() {
        assert_eq!(normalize_color("#9146ff"), Some("#9146FF".into()));
        assert_eq!(normalize_color("ff112233"), Some("#112233".into()));
        assert_eq!(normalize_color(" #00aaff "), Some("#00AAFF".into()));
        assert_eq!(normalize_color("#12345"), None);
        assert_eq!(normalize_color("#zzzzzz"), None);
        assert_eq!(normalize_color(""), None);
    }

    #[test]
    fn read_only_and_disabled_guards_block_invalid_actions_and_edits() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            crate::twitch::rewards_for_test(
                vec![
                    json!({"id": "ours_off", "title": "Off", "cost": 50, "is_enabled": false}),
                    json!({"id": "theirs", "title": "External", "cost": 50, "is_enabled": true}),
                ],
                &["ours_off"],
            );
            // Non-owned rewards cannot be toggled, paused, deleted or edited, only grouped.
            for action in ["toggle", "pause", "delete"] {
                assert_eq!(
                    act(&engine, action, "twitch:theirs").unwrap_err(),
                    tr("rewards.not_ours"),
                    "{action}"
                );
            }
            assert_eq!(
                external_form(&engine, "reward_edit:twitch:theirs")
                    .unwrap()
                    .unwrap_err(),
                tr("rewards.not_ours")
            );
            assert_eq!(
                save_external(
                    &engine,
                    "reward_edit:twitch:theirs",
                    obj(json!({"title": "X", "cost": 10}))
                )
                .unwrap()
                .unwrap_err(),
                tr("rewards.not_ours")
            );
            // Disabled owned reward cannot be paused.
            assert!(act(&engine, "pause", "twitch:ours_off").is_err());
            // Group edit on failed platform edit does not mutate local config prematurely.
            engine.change(|c| c.rewards.set_group("twitch:ours_off", "Original"));
            save_external(
                &engine,
                "reward_edit:twitch:ours_off",
                obj(json!({"title": "Off", "cost": 50, "group": "Changed", "enabled": false})),
            )
            .unwrap()
            .unwrap();
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            if let Some(prompt) = super::super::prompts::list().last() {
                super::super::prompts::answer(prompt.id, Value::Null).ok();
            }
            assert_eq!(
                engine.config().rewards.group_of("twitch:ours_off"),
                Some("Original"),
                "failed PATCH must not overwrite the local group"
            );
        });
    }

    #[test]
    fn new_reward_form_defaults_to_kick_when_only_kick_is_logged_in_and_sync_removes_deleted() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|c| {
                c.kick.broadcaster = Some(crate::kick::KickAccount {
                    username: "streamer".into(),
                    ..Default::default()
                });
            });
            let form = external_form(&engine, "reward_new:").unwrap().unwrap();
            assert_eq!(form["item"]["platform"], 1, "defaults to Kick when Twitch has no login");
            sync_kick_rewards(&json!({"data": [
                {"id": "k1", "title": "First", "cost": 10, "is_enabled": true},
                {"id": "k2", "title": "Second", "cost": 20, "is_enabled": true}
            ]}));
            assert_eq!(crate::kick::rewards_known().len(), 2);
            sync_kick_rewards(&json!({"data": [
                {"id": "k2", "title": "Second", "cost": 25, "is_enabled": true}
            ]}));
            let known = crate::kick::rewards_known();
            assert_eq!(known.len(), 1);
            assert_eq!(known[0]["id"], "k2");
            assert_eq!(known[0]["cost"], 25);
            // Non-array response does not wipe the cache.
            sync_kick_rewards(&json!({"data": null}));
            assert_eq!(crate::kick::rewards_known().len(), 1);
            // Deleting a reward when Twitch is disconnected reports the reward's title, not its key.
            crate::twitch::rewards_for_test(
                vec![json!({"id": "del1", "title": "Stretch", "cost": 100, "is_enabled": true})],
                &["del1"],
            );
            let before = super::super::prompts::list().len();
            act(&engine, "delete", "twitch:del1").unwrap();
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            let prompts = super::super::prompts::list();
            assert!(prompts.len() > before);
            let prompt = prompts.last().unwrap();
            let super::super::prompts::Content::Notice { title, .. } = &prompt.content else {
                panic!("expected notice prompt");
            };
            assert_eq!(title, "Stretch");
            super::super::prompts::answer(prompt.id, Value::Null).ok();
            // Clean up Kick cache so other tests start clean.
            sync_kick_rewards(&json!({"data": []}));
        });
    }
}
