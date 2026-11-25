//! Twitch AutoMod settings: read before editing and reject changes made by another moderator.
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
    ("aggression", "Aggression", W::Int),
    ("bullying", "Bullying", W::Int),
    ("disability", "Disability", W::Int),
    ("misogyny", "Misogyny", W::Int),
    ("race_ethnicity_or_religion", "Race", W::Int),
    ("sex_based_terms", "Sexual content", W::Int),
    ("sexuality_sex_or_gender", "Sexuality", W::Int),
    ("swearing", "Swearing", W::Int),
];
const LABELS: [&str; 8] = [
    "moderation.aggression",
    "moderation.bullying",
    "moderation.disability",
    "moderation.misogyny",
    "moderation.race",
    "moderation.sexual_content",
    "moderation.sexuality",
    "moderation.swearing",
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

fn values(response: &Value) -> Result<Value, String> {
    let source = &response["data"][0];
    let mut out = serde_json::Map::new();
    for (key, _, _) in FIELDS {
        let n = source[*key]
            .as_u64()
            .filter(|v| *v <= 4)
            .ok_or_else(|| tr("moderation.settings_invalid_response").to_string())?;
        out.insert((*key).to_string(), n.into());
    }
    Ok(Value::Object(out))
}

async fn get(twitch: &crate::twitch::TwitchHandle) -> Result<Value, String> {
    values(
        &twitch
            .helix(
                "GET",
                "moderation/automod/settings",
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

pub fn refresh(engine: &Rc<Engine>) -> Result<Value, String> {
    let (generation, twitch) = begin(engine)?;
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        finish(&engine, generation, get(&twitch).await);
    });
    Ok(json!({"accepted": true}))
}

pub fn external_fields(kind: &str) -> Option<&'static [F]> {
    (kind == "moderation_automod_settings" || kind.starts_with("moderation_automod_settings:")).then_some(FIELDS)
}

pub fn external_form(engine: &Rc<Engine>, kind: &str) -> Option<Result<Value, String>> {
    external_fields(kind)?;
    let key = account(engine);
    let snapshot = STATE.with_borrow(|s| {
        if s.account == key && !s.busy {
            s.settings.clone().map(|v| (s.generation, v))
        } else {
            None
        }
    });
    let Some((generation, settings)) = snapshot else {
        return Some(Err(tr("moderation.settings_refresh_first").into()));
    };
    if let Some(version) = kind.strip_prefix("moderation_automod_settings:") {
        if version.parse::<u64>().ok() != Some(generation) {
            return Some(Err(tr("moderation.settings_refresh_first").into()));
        }
    }
    let form_kind = format!("moderation_automod_settings:{generation}");
    let mut item = settings.clone();
    item["baseline"] = settings;
    let fields: Vec<_> = FIELDS.iter().zip(LABELS).map(|((key, _, widget), label)| json!({"key": key, "label": tr(label), "widget": widget, "holds": widget.about(), "value": crate::automation::edits::shown(widget, item.get(*key))})).collect();
    Some(Ok(
        json!({"kind": form_kind, "title": tr("moderation.settings_title"), "okLabel": tr("btn.save"), "json": false,
        "notes": [tr("moderation.settings_note")], "new": item, "item": item, "fields": fields}),
    ))
}

pub fn save_external(engine: &Rc<Engine>, kind: &str, item: Obj) -> Option<Result<Value, String>> {
    external_fields(kind)?;
    let Some(version) = kind
        .strip_prefix("moderation_automod_settings:")
        .and_then(|s| s.parse::<u64>().ok())
    else {
        return Some(Err(tr("moderation.settings_refresh_first").into()));
    };
    if !STATE.with_borrow(|s| s.generation == version) {
        return Some(Err(tr("moderation.settings_refresh_first").into()));
    }
    Some(save(engine, &item))
}

fn save(engine: &Rc<Engine>, item: &Obj) -> Result<Value, String> {
    let baseline = item
        .get("baseline")
        .cloned()
        .ok_or_else(|| tr("moderation.settings_refresh_first").to_string())?;
    let key = account(engine);
    if !STATE.with_borrow(|s| s.account == key && !s.busy && s.settings.as_ref() == Some(&baseline)) {
        return Err(tr("moderation.settings_refresh_first").into());
    }
    let mut desired = serde_json::Map::new();
    for (field, _, _) in FIELDS {
        let value = item
            .get(*field)
            .and_then(Value::as_u64)
            .filter(|v| *v <= 4)
            .ok_or_else(|| tr("moderation.settings_invalid").to_string())?;
        desired.insert((*field).into(), value.into());
    }
    if Value::Object(desired.clone()) == baseline {
        return Ok(json!({"unchanged": true}));
    }
    let (generation, twitch) = begin(engine)?;
    let engine = engine.clone();
    tokio::task::spawn_local(async move {
        let result = async {
            let current = get(&twitch).await?;
            if current != baseline {
                return Err(tr("moderation.settings_changed").into());
            }
            let response = twitch
                .helix(
                    "PUT",
                    "moderation/automod/settings",
                    vec![
                        ("broadcaster_id", "{broadcaster_id}"),
                        ("moderator_id", "{moderator_id}"),
                    ],
                    Some(Value::Object(desired)),
                    false,
                    true,
                )
                .await?;
            values(&response)
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
        let rows: Vec<Value> = s.settings.as_ref().into_iter().flat_map(|settings| FIELDS.iter().zip(LABELS).map(|((key, _, _), label)| json!({"id": key, "name": tr(label), "cells": [tr(label), settings[*key]], "enabled": true}))).collect();
        json!({"kind": "moderation_settings", "domain": "moderation", "title": tr("moderation.settings_title"), "showTitle": true, "icon": "settings", "intro": tr("moderation.settings_note"),
            "status": {"health": if s.busy {"busy"} else if !s.error.is_empty() {"down"} else if s.settings.is_some() {"up"} else {"off"},
                "text": if s.busy {tr("moderation.working")} else if !s.error.is_empty() {&s.error} else if s.settings.is_some() {tr("moderation.settings_loaded")} else {tr("moderation.settings_refresh_first")}},
            "columns": [super::view::column("name", "moderation.settings_category", 200), super::view::column("level", "moderation.settings_level", 100)], "rows": rows,
            "empty": tr("moderation.settings_refresh_first"), "actions": [
                {"id": "settings_refresh", "label": tr("btn.refresh"), "icon": "refresh-cw", "needs": "none", "top": true, "enabled": !s.busy},
                {"id": "settings_edit", "entity": "moderation_automod_settings", "label": tr("btn.edit"), "icon": "pencil", "needs": "none", "top": true, "enabled": !s.busy && s.settings.is_some()}]})
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_complete_and_external_changes_are_not_overwritten() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let initial = json!({"data": [{"aggression": 0, "bullying": 1, "disability": 2, "misogyny": 3,
                "race_ethnicity_or_religion": 4, "sex_based_terms": 1, "sexuality_sex_or_gender": 2, "swearing": 3}]});
            let mut changed = initial.clone();
            changed["data"][0]["bullying"] = 4.into();
            let fakes = crate::scenario::fakes::install(
                &engine,
                &json!({"twitch": {"GET moderation/automod/settings": {"$seq": [initial, changed]}}}),
                None,
            );
            STATE.with_borrow_mut(|s| *s = State::default());
            refresh(&engine).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            let form = external_form(&engine, "moderation_automod_settings").unwrap().unwrap();
            let mut item = form["item"].as_object().unwrap().clone();
            item.insert("aggression".into(), 1.into());
            let form_kind = form["kind"].as_str().unwrap().to_string();
            save(&engine, &item).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(!fakes
                .calls
                .borrow()
                .iter()
                .any(|c| c["call"] == "PUT moderation/automod/settings"));
            assert_eq!(
                STATE.with_borrow(|s| s.error.clone()),
                tr("moderation.settings_changed")
            );
            let stale = crate::servers::answer(
                &engine,
                &json!({"request": "StreamerzEditEntity", "kind": form_kind, "changes": [{"field": 0, "value": 1}]}),
                true,
            )
            .unwrap();
            assert_eq!(stale["status"], "error");
            crate::scenario::fakes::uninstall();
            STATE.with_borrow_mut(|s| *s = State::default());
        });
    }
}
