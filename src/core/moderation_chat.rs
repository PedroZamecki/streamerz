//! Twitch chat protections, edited against a freshly checked snapshot.
use super::moderation::account;
use crate::{
    automation::schema::{F, W},
    engine::Engine,
    i18n::tr,
    model::Obj,
};
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

const FIELDS: &[F] = &[
    ("emote_mode", "Emote-only", W::Bool),
    ("subscriber_mode", "Subscriber-only", W::Bool),
    ("unique_chat_mode", "Unique messages", W::Bool),
    ("follower_mode", "Followers-only", W::Bool),
    ("follower_mode_duration", "Follow minutes", W::Int),
    ("slow_mode", "Slow mode", W::Bool),
    ("slow_mode_wait_time", "Slow seconds", W::Int),
    ("non_moderator_chat_delay", "Chat delay", W::Bool),
    ("non_moderator_chat_delay_duration", "Delay seconds", W::Int),
];
const LABELS: [&str; 9] = [
    "moderation.chat_emote",
    "moderation.chat_subscriber",
    "moderation.chat_unique",
    "moderation.chat_follower",
    "moderation.chat_follower_minutes",
    "moderation.chat_slow",
    "moderation.chat_slow_seconds",
    "moderation.chat_delay",
    "moderation.chat_delay_seconds",
];

#[derive(Default)]
struct State {
    account: String,
    generation: u64,
    busy: bool,
    settings: Option<Value>,
    error: String,
}
thread_local! { static STATE: RefCell<State> = RefCell::default(); }

fn begin(engine: &Rc<Engine>) -> Result<(u64, crate::twitch::TwitchHandle), String> {
    let twitch = engine
        .twitch()
        .ok_or_else(|| tr("moderation.twitch_disconnected").to_string())?;
    let key = account(engine);
    if STATE.with_borrow(|s| s.account == key && s.busy) {
        return Err(tr("moderation.working").into());
    }
    let generation = STATE.with_borrow_mut(|s| {
        if s.account != key {
            let generation = s.generation;
            *s = State {
                account: key,
                generation,
                ..State::default()
            };
        }
        s.generation = s.generation.wrapping_add(1);
        s.busy = true;
        s.error.clear();
        s.generation
    });
    crate::host::changed();
    Ok((generation, twitch))
}
fn finish(engine: &Engine, generation: u64, result: Result<Value, String>) {
    let key = account(engine);
    STATE.with_borrow_mut(|s| {
        if s.account != key || s.generation != generation {
            return;
        }
        s.busy = false;
        match result {
            Ok(value) => {
                s.settings = Some(value);
                s.error.clear();
            }
            Err(error) => {
                s.settings = None;
                s.error = error;
            }
        }
    });
    crate::host::changed();
}
fn normalize(response: &Value) -> Result<Value, String> {
    let v = &response["data"][0];
    let mut out = serde_json::Map::new();
    for name in [
        "emote_mode",
        "subscriber_mode",
        "unique_chat_mode",
        "follower_mode",
        "slow_mode",
    ] {
        out.insert(
            name.into(),
            v[name]
                .as_bool()
                .ok_or_else(|| tr("moderation.chat_invalid_response").to_string())?
                .into(),
        );
    }
    let delay_available = v["non_moderator_chat_delay"].as_bool().is_some();
    out.insert("delay_available".into(), delay_available.into());
    out.insert(
        "non_moderator_chat_delay".into(),
        v["non_moderator_chat_delay"].as_bool().unwrap_or(false).into(),
    );
    for (field, active, default) in [
        ("follower_mode_duration", "follower_mode", 0),
        ("slow_mode_wait_time", "slow_mode", 30),
        ("non_moderator_chat_delay_duration", "non_moderator_chat_delay", 2),
    ] {
        let value = if out[active] == true {
            v[field]
                .as_u64()
                .ok_or_else(|| tr("moderation.chat_invalid_response").to_string())?
        } else {
            default
        };
        out.insert(field.into(), value.into());
    }
    Ok(Value::Object(out))
}
async fn get(twitch: &crate::twitch::TwitchHandle) -> Result<Value, String> {
    normalize(
        &twitch
            .helix(
                "GET",
                "chat/settings",
                vec![
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("moderator_id", "{moderator_id}"),
                ],
                None,
                false,
                true,
            )
            .await?,
    )
}
pub fn refresh(engine: &Rc<Engine>) -> Result<Value, String> {
    let (generation, twitch) = begin(engine)?;
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        finish(&engine, generation, get(&twitch).await);
    });
    Ok(json!({"accepted": true}))
}
pub fn external_fields(kind: &str) -> Option<&'static [F]> {
    (kind == "moderation_chat_settings" || kind.starts_with("moderation_chat_settings:")).then_some(FIELDS)
}
pub fn external_form(engine: &Rc<Engine>, kind: &str) -> Option<Result<Value, String>> {
    external_fields(kind)?;
    let key = account(engine);
    let state = STATE.with_borrow(|s| {
        if s.account == key && !s.busy {
            s.settings.clone().map(|v| (s.generation, v))
        } else {
            None
        }
    });
    let Some((generation, settings)) = state else {
        return Some(Err(tr("moderation.chat_refresh_first").into()));
    };
    if let Some(version) = kind.strip_prefix("moderation_chat_settings:") {
        if version.parse::<u64>().ok() != Some(generation) {
            return Some(Err(tr("moderation.chat_refresh_first").into()));
        }
    }
    let mut item = settings.clone();
    item["baseline"] = settings.clone();
    let delay = settings["delay_available"] == true;
    let fields: Vec<_> = FIELDS.iter().zip(LABELS).map(|((key, _, widget), label)| json!({"key": key, "label": tr(label), "widget": widget, "holds": widget.about(),
        "disabled": key.starts_with("non_moderator") && !delay, "value": crate::automation::edits::shown(widget, item.get(*key))})).collect();
    Some(Ok(
        json!({"kind": format!("moderation_chat_settings:{generation}"), "title": tr("moderation.chat_title"), "okLabel": tr("btn.save"), "json": false,
        "notes": [tr("moderation.chat_note")], "new": item, "item": item, "fields": fields}),
    ))
}
pub fn save_external(engine: &Rc<Engine>, kind: &str, item: Obj) -> Option<Result<Value, String>> {
    external_fields(kind)?;
    let Some(version) = kind
        .strip_prefix("moderation_chat_settings:")
        .and_then(|v| v.parse::<u64>().ok())
    else {
        return Some(Err(tr("moderation.chat_refresh_first").into()));
    };
    if !STATE.with_borrow(|s| s.generation == version) {
        return Some(Err(tr("moderation.chat_refresh_first").into()));
    }
    Some(save(engine, &item))
}
fn changes(item: &Obj, baseline: &Value) -> Result<Value, String> {
    let mut body = serde_json::Map::new();
    for name in ["emote_mode", "subscriber_mode", "unique_chat_mode"] {
        let value = item
            .get(name)
            .and_then(Value::as_bool)
            .ok_or_else(|| tr("moderation.chat_invalid").to_string())?;
        if baseline[name] != value {
            body.insert(name.into(), value.into());
        }
    }
    for (mode, duration, min, max) in [
        ("follower_mode", "follower_mode_duration", 0, 129600),
        ("slow_mode", "slow_mode_wait_time", 3, 120),
        ("non_moderator_chat_delay", "non_moderator_chat_delay_duration", 2, 6),
    ] {
        if mode == "non_moderator_chat_delay" && baseline["delay_available"] != true {
            continue;
        }
        let enabled = item
            .get(mode)
            .and_then(Value::as_bool)
            .ok_or_else(|| tr("moderation.chat_invalid").to_string())?;
        let seconds = item
            .get(duration)
            .and_then(Value::as_u64)
            .ok_or_else(|| tr("moderation.chat_invalid").to_string())?;
        if enabled
            && (seconds < min || seconds > max || (mode == "non_moderator_chat_delay" && ![2, 4, 6].contains(&seconds)))
        {
            return Err(tr("moderation.chat_invalid").into());
        }
        if baseline[mode] != enabled || (enabled && baseline[duration] != seconds) {
            body.insert(mode.into(), enabled.into());
            if enabled {
                body.insert(duration.into(), seconds.into());
            }
        }
    }
    Ok(Value::Object(body))
}
fn save(engine: &Rc<Engine>, item: &Obj) -> Result<Value, String> {
    let baseline = item
        .get("baseline")
        .cloned()
        .ok_or_else(|| tr("moderation.chat_refresh_first").to_string())?;
    let key = account(engine);
    if !STATE.with_borrow(|s| s.account == key && !s.busy && s.settings.as_ref() == Some(&baseline)) {
        return Err(tr("moderation.chat_refresh_first").into());
    }
    let body = changes(item, &baseline)?;
    if body.as_object().is_some_and(|body| body.is_empty()) {
        return Ok(json!({"unchanged": true}));
    }
    let (generation, twitch) = begin(engine)?;
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        let result = async {
            if get(&twitch).await? != baseline {
                return Err(tr("moderation.chat_changed").into());
            }
            if account(&engine) != key {
                return Err(tr("moderation.account_changed").into());
            }
            normalize(
                &twitch
                    .helix(
                        "PATCH",
                        "chat/settings",
                        vec![
                            ("broadcaster_id", "{broadcaster_id}"),
                            ("moderator_id", "{moderator_id}"),
                        ],
                        Some(body),
                        false,
                        true,
                    )
                    .await?,
            )
        }
        .await;
        finish(&engine, generation, result);
    });
    Ok(json!({"accepted": true}))
}
pub fn table(engine: &Rc<Engine>) -> Value {
    let key = account(engine);
    STATE.with_borrow_mut(|s| {
        if s.account != key {
            *s = State {
                account: key,
                generation: s.generation.wrapping_add(1),
                ..State::default()
            };
        }
    });
    STATE.with_borrow(|s| {
        let rows: Vec<Value> = s.settings.as_ref().into_iter().flat_map(|settings| FIELDS.iter().zip(LABELS).filter_map(|((key, _, _), label)| {
            if key.starts_with("non_moderator") && settings["delay_available"] != true { return None; }
            let value = &settings[*key];
            Some(json!({"id": key, "name": tr(label), "enabled": true, "cells": [tr(label), if let Some(value) = value.as_bool() { if value { tr("moderation.chat_on").to_string() } else { tr("moderation.chat_off").to_string() } } else { value.to_string() }]}))
        })).collect();
        json!({"kind": "moderation_chat_settings", "domain": "moderation", "title": tr("moderation.chat_title"), "showTitle": true, "icon": "message-square", "intro": tr("moderation.chat_note"),
            "status": {"health": if s.busy {"busy"} else if !s.error.is_empty() {"down"} else if s.settings.is_some() {"up"} else {"off"},
                "text": if s.busy {tr("moderation.working")} else if !s.error.is_empty() {&s.error} else if s.settings.is_some() {tr("moderation.chat_loaded")} else {tr("moderation.chat_refresh_first")}},
            "columns": [super::view::column("name", "moderation.settings_category", 230), super::view::column("value", "moderation.type", 100)], "rows": rows,
            "empty": tr("moderation.chat_refresh_first"), "actions": [
                {"id": "chat_refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "needs": "none", "top": true, "enabled": !s.busy},
                {"id": "chat_edit", "entity": "moderation_chat_settings", "label": tr("btn.edit"), "icon": "pencil", "needs": "none", "top": true, "enabled": !s.busy && s.settings.is_some()}]})
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_change_does_not_get_overwritten() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let initial = json!({"data": [{"emote_mode": false, "subscriber_mode": false,
                "unique_chat_mode": false, "follower_mode": false, "slow_mode": false}]});
            let mut changed = initial.clone();
            changed["data"][0]["subscriber_mode"] = true.into();
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({"twitch": {"GET chat/settings": {"$seq": [initial, changed]}}}),
                None,
            );
            STATE.with_borrow_mut(|s| *s = State::default());
            refresh(&engine).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            let form = external_form(&engine, "moderation_chat_settings").unwrap().unwrap();
            let mut item = form["item"].as_object().unwrap().clone();
            item.insert("emote_mode".into(), true.into());
            save(&engine, &item).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(!fakes
                .calls
                .borrow()
                .iter()
                .any(|call| call["call"] == "PATCH chat/settings"));
            assert_eq!(STATE.with_borrow(|s| s.error.clone()), tr("moderation.chat_changed"));
            crate::scenario::fakes::uninstall();
            STATE.with_borrow_mut(|s| *s = State::default());
        });
    }

    #[test]
    fn chat_changes_only_send_edited_modes_and_validate_durations() {
        let baseline = normalize(&json!({"data": [{"emote_mode": false, "subscriber_mode": false,
            "unique_chat_mode": false, "follower_mode": false, "slow_mode": false}]}))
        .unwrap();
        let mut item = baseline.as_object().unwrap().clone();
        assert_eq!(changes(&item, &baseline).unwrap(), json!({}));
        item.insert("slow_mode".into(), true.into());
        item.insert("slow_mode_wait_time".into(), 5.into());
        assert_eq!(
            changes(&item, &baseline).unwrap(),
            json!({"slow_mode": true, "slow_mode_wait_time": 5})
        );
        item.insert("slow_mode_wait_time".into(), 1.into());
        assert!(changes(&item, &baseline).is_err());
        item.insert("slow_mode_wait_time".into(), 5.into());
        item.insert("non_moderator_chat_delay".into(), true.into());
        assert_eq!(
            changes(&item, &baseline).unwrap(),
            json!({"slow_mode": true, "slow_mode_wait_time": 5})
        );
    }
}
