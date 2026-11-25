//! Recent suspicious-user activity received through Twitch EventSub.
use super::moderation::{account, is_current_twitch_channel};
use crate::{
    automation::schema::{F, W},
    engine::Engine,
    i18n::tr,
    model::Obj,
};
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct State {
    account: String,
    generation: u64,
    operation: u64,
    busy: bool,
    error: String,
    message: String,
    rows: Vec<Value>,
}
thread_local! { static STATE: RefCell<State> = RefCell::default(); }
const WARN_FIELDS: &[F] = &[("reason", "Reason", W::Text)];

pub fn external_fields(kind: &str) -> Option<&'static [F]> {
    kind.starts_with("moderation_warn:").then_some(WARN_FIELDS)
}

pub fn external_form(engine: &Rc<Engine>, kind: &str) -> Option<Result<Value, String>> {
    let rest = kind.strip_prefix("moderation_warn:")?;
    let (id, expected) = rest.split_once(':').map_or((rest, None), |(id, hash)| (id, Some(hash)));
    let key = account(engine);
    let hash = account_hash(&key);
    if expected.is_some_and(|old| old != hash) {
        return Some(Err(tr("moderation.account_changed").into()));
    }
    let row = STATE.with_borrow(|s| {
        if s.account == key && !s.busy {
            s.rows.iter().find(|row| row["user_id"].as_str() == Some(id)).cloned()
        } else {
            None
        }
    });
    let Some(row) = row else {
        return Some(Err(tr("moderation.suspicious_missing").into()));
    };
    let item = json!({"reason": ""});
    let widget = &WARN_FIELDS[0].2;
    Some(Ok(
        json!({"kind": format!("moderation_warn:{id}:{hash}"), "title": tr("moderation.warn_title"), "okLabel": tr("moderation.warn_title"), "json": false,
        "notes": [format!("{} ({id})", row["user_name"].as_str().unwrap_or_default()), tr("moderation.warn_note")],
        "item": item, "new": item, "fields": [{"key": "reason", "label": tr("moderation.reason"), "widget": widget, "holds": widget.about(), "value": ""}]}),
    ))
}

pub fn save_external(engine: &Rc<Engine>, kind: &str, item: Obj) -> Option<Result<Value, String>> {
    let rest = kind.strip_prefix("moderation_warn:")?;
    let Some((id, hash)) = rest.split_once(':') else {
        return Some(Err(tr("moderation.account_changed").into()));
    };
    if hash != account_hash(&account(engine)) {
        return Some(Err(tr("moderation.account_changed").into()));
    }
    Some(warn(engine, id, &item))
}

fn account_hash(account: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(account.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn warn(engine: &Rc<Engine>, id: &str, item: &Obj) -> Result<Value, String> {
    let reason = item
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if reason.is_empty() || reason.chars().count() > 500 {
        return Err(tr("moderation.warn_invalid").into());
    }
    let key = account(engine);
    let valid = STATE
        .with_borrow(|s| s.account == key && !s.busy && s.rows.iter().any(|row| row["user_id"].as_str() == Some(id)));
    if !valid {
        return Err(tr("moderation.suspicious_missing").into());
    }
    let twitch = engine
        .twitch()
        .ok_or_else(|| tr("moderation.twitch_disconnected").to_string())?;
    let operation = STATE.with_borrow_mut(|s| {
        s.operation = s.operation.wrapping_add(1);
        s.busy = true;
        s.error.clear();
        s.message.clear();
        s.operation
    });
    let engine = engine.clone();
    let id = id.to_string();
    crate::host::changed();
    tokio::task::spawn_local(async move {
        let result = twitch
            .helix(
                "POST",
                "moderation/warnings",
                vec![
                    ("broadcaster_id", "{broadcaster_id}"),
                    ("moderator_id", "{moderator_id}"),
                ],
                Some(json!({"data": {"user_id": id, "reason": reason}})),
                false,
                true,
            )
            .await;
        STATE.with_borrow_mut(|s| {
            if s.account != key || s.operation != operation || account(&engine) != key {
                return;
            }
            s.busy = false;
            match result {
                Ok(_) => s.message = tr("moderation.action_done").into(),
                Err(error) => s.error = error,
            };
        });
        crate::host::changed();
    });
    Ok(json!({"accepted": true}))
}

pub fn event(engine: &Rc<Engine>, event: &Value, message: bool) {
    let Some(id) = event["user_id"].as_str().filter(|id| !id.is_empty()) else {
        return;
    };
    if !is_current_twitch_channel(engine, event) {
        return;
    }
    let key = account(engine);
    STATE.with_borrow_mut(|state| {
        if state.account != key {
            let generation = state.generation.wrapping_add(1);
            let operation = state.operation.wrapping_add(1);
            *state = State {
                account: key,
                generation,
                operation,
                ..State::default()
            };
        }
        state.generation = state.generation.wrapping_add(1);
        let previous = state
            .rows
            .iter()
            .find(|row| row["user_id"].as_str() == Some(id))
            .cloned();
        state.rows.retain(|row| row["user_id"].as_str() != Some(id));
        if event["low_trust_status"] == "none" && !message {
            return;
        }
        let mut next = event.clone();
        if !message {
            if let Some(old) = previous {
                next["message"] = old["message"].clone();
            }
        }
        if state.rows.len() >= 100 {
            state.rows.remove(0);
        }
        state.rows.push(next);
    });
    crate::host::changed();
}

pub fn act(engine: &Rc<Engine>, id: &str, status: &str) -> Result<Value, String> {
    if !matches!(status, "ACTIVE_MONITORING" | "RESTRICTED" | "none") {
        return Err(tr("moderation.suspicious_missing").into());
    }
    let key = account(engine);
    let valid = STATE
        .with_borrow(|s| s.account == key && !s.busy && s.rows.iter().any(|row| row["user_id"].as_str() == Some(id)));
    if !valid {
        return Err(tr("moderation.suspicious_missing").into());
    }
    let twitch = engine
        .twitch()
        .ok_or_else(|| tr("moderation.twitch_disconnected").to_string())?;
    let (generation, operation) = STATE.with_borrow_mut(|s| {
        s.operation = s.operation.wrapping_add(1);
        s.busy = true;
        s.error.clear();
        s.message.clear();
        (s.generation, s.operation)
    });
    let engine = engine.clone();
    let id = id.to_string();
    let status = status.to_string();
    crate::host::changed();
    tokio::task::spawn_local(async move {
        let mut query = vec![
            ("broadcaster_id", "{broadcaster_id}"),
            ("moderator_id", "{moderator_id}"),
        ];
        let (method, body) = if status == "none" {
            query.push(("user_id", id.as_str()));
            ("DELETE", None)
        } else {
            ("POST", Some(json!({"user_id": id, "status": status})))
        };
        let result = twitch
            .helix(method, "moderation/suspicious_users", query, body, false, true)
            .await;
        STATE.with_borrow_mut(|s| {
            if s.account != key || s.operation != operation || account(&engine) != key {
                return;
            }
            s.busy = false;
            match result {
                Ok(_) if s.generation == generation => {
                    if status == "none" {
                        s.rows.retain(|row| row["user_id"].as_str() != Some(id.as_str()));
                    } else if let Some(row) = s
                        .rows
                        .iter_mut()
                        .find(|row| row["user_id"].as_str() == Some(id.as_str()))
                    {
                        row["low_trust_status"] = status.to_ascii_lowercase().into();
                    }
                }
                Ok(_) => {}
                Err(error) => s.error = error,
            }
        });
        crate::host::changed();
    });
    Ok(json!({"accepted": true}))
}

pub fn table(engine: &Rc<Engine>) -> Value {
    let key = account(engine);
    STATE.with_borrow_mut(|s| {
        if s.account != key {
            let generation = s.generation.wrapping_add(1);
            let operation = s.operation.wrapping_add(1);
            *s = State {
                account: key,
                generation,
                operation,
                ..State::default()
            };
        }
    });
    STATE.with_borrow(|s| {
        let rows: Vec<Value> = s.rows.iter().filter_map(|row| {
            let id = row["user_id"].as_str()?;
            let status = match row["low_trust_status"].as_str().unwrap_or_default() {
                "active_monitoring" => tr("moderation.suspicious_monitoring"), "restricted" => tr("moderation.suspicious_restricted"), _ => tr("moderation.suspicious_unknown") };
            Some(json!({"id": id, "name": row["user_name"], "enabled": true, "cells": [row["user_name"], status, row["message"]["text"]]}))
        }).collect();
        let button = |id: &str, label: &'static str| json!({"id": id, "label": tr(label), "icon": "user", "needs": "row", "enabled": !s.busy});
        json!({"kind": "moderation_suspicious", "domain": "moderation", "title": tr("moderation.suspicious_title"), "showTitle": true, "icon": "shield", "intro": tr("moderation.suspicious_note"),
            "status": if s.busy || !s.error.is_empty() || !s.message.is_empty() { json!({"health": if s.busy {"busy"} else if !s.error.is_empty() {"down"} else {"up"}, "text": if s.busy {tr("moderation.working")} else if !s.error.is_empty() {&s.error} else {&s.message}}) } else { Value::Null },
            "columns": [super::view::column("user", "moderation.user", 160), super::view::column("status", "moderation.type", 150), super::view::column("message", "moderation.message", 300)],
            "rows": rows, "empty": tr("moderation.suspicious_empty"), "search": true,
            "actions": [button("suspicious_monitor", "moderation.suspicious_monitor"), button("suspicious_restrict", "moderation.suspicious_restrict"), button("suspicious_remove", "moderation.suspicious_remove"),
                {"id": "suspicious_warn", "form": "moderation_warn:", "label": tr("moderation.warn_title"), "icon": "message-square", "needs": "row", "enabled": !s.busy}]})
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_users_are_scoped_and_can_be_restricted() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|c| c.twitch.channel_id = "owner".into());
            let fakes = crate::scenario::fakes::install(&engine, &json!({"twitch": {}}), None);
            STATE.with_borrow_mut(|s| *s = State::default());
            let mut message = json!({"broadcaster_user_id": "other", "user_id": "5", "user_name": "Person", "low_trust_status": "active_monitoring", "message": {"text": "hello"}});
            event(&engine, &message, true);
            assert!(table(&engine)["rows"].as_array().unwrap().is_empty());
            message["broadcaster_user_id"] = "owner".into();
            event(&engine, &message, true);
            assert!(warn(&engine, "5", json!({"reason": ""}).as_object().unwrap()).is_err());
            warn(&engine, "5", json!({"reason": "Please stop"}).as_object().unwrap()).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(fakes
                .calls
                .borrow()
                .iter()
                .any(|call| call["call"] == "POST moderation/warnings"
                    && call["body"]["data"]["reason"] == "Please stop"));
            act(&engine, "5", "RESTRICTED").unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert!(fakes.calls.borrow().iter().any(
                |call| call["call"] == "POST moderation/suspicious_users" && call["body"]["status"] == "RESTRICTED"
            ));
            assert_eq!(
                table(&engine)["rows"][0]["cells"][1],
                tr("moderation.suspicious_restricted")
            );
            message["low_trust_status"] = "none".into();
            event(&engine, &message, false);
            assert!(table(&engine)["rows"].as_array().unwrap().is_empty());
            crate::scenario::fakes::uninstall();
            STATE.with_borrow_mut(|s| *s = State::default());
        });
    }
}
