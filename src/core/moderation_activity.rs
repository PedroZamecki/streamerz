//! Recent moderation events observed from Twitch, kept only for the current session.
use super::moderation::{account, is_current_twitch_channel};
use crate::{engine::Engine, i18n::tr};
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct State {
    account: String,
    next_id: u64,
    rows: Vec<Value>,
}
thread_local! { static STATE: RefCell<State> = RefCell::default(); }

fn label(action: &str) -> Option<&'static str> {
    Some(match action {
        "ban" | "shared_chat_ban" => "moderation.activity_ban",
        "timeout" | "shared_chat_timeout" => "moderation.activity_timeout",
        "unban" | "shared_chat_unban" => "moderation.activity_unban",
        "untimeout" | "shared_chat_untimeout" => "moderation.activity_untimeout",
        "delete" | "shared_chat_delete" => "moderation.activity_delete",
        "clear" => "moderation.activity_clear",
        "warn" => "moderation.activity_warn",
        "add_blocked_term" => "moderation.activity_blocked_add",
        "add_permitted_term" => "moderation.activity_permitted_add",
        "remove_blocked_term" => "moderation.activity_blocked_remove",
        "remove_permitted_term" => "moderation.activity_permitted_remove",
        "emoteonly" => "slash.emoteonly",
        "emoteonlyoff" => "slash.emoteonlyoff",
        "followers" => "slash.followers",
        "followersoff" => "slash.followersoff",
        "uniquechat" => "slash.uniquechat",
        "uniquechatoff" => "slash.uniquechatoff",
        "slow" => "slash.slow",
        "slowoff" => "slash.slowoff",
        "subscribers" => "slash.subscribers",
        "subscribersoff" => "slash.subscribersoff",
        "mod" => "slash.mod",
        "unmod" => "slash.unmod",
        "vip" => "slash.vip",
        "unvip" => "slash.unvip",
        "approve_unban_request" => "moderation.approve",
        "deny_unban_request" => "moderation.deny",
        _ => return None,
    })
}

pub fn event(engine: &Rc<Engine>, event: &Value) {
    let Some(action) = event["action"].as_str() else {
        return;
    };
    if label(action).is_none() {
        return;
    }
    if !is_current_twitch_channel(engine, event) {
        return;
    }
    let key = account(engine);
    let detail = &event[action.strip_prefix("shared_chat_").unwrap_or(action)];
    STATE.with_borrow_mut(|s| {
        if s.account != key {
            *s = State {
                account: key,
                ..State::default()
            };
        }
        if s.rows.len() >= 100 {
            s.rows.remove(0);
        }
        s.next_id = s.next_id.wrapping_add(1);
        let id = s.next_id.to_string();
        s.rows.push(
            json!({"id": id, "action": action, "source": event["source_broadcaster_user_name"], "moderator": event["moderator_user_name"],
            "target": detail["user_name"], "reason": detail["reason"], "received": crate::oauth::now()}),
        );
    });
    crate::host::changed();
}

pub fn table(engine: &Rc<Engine>) -> Value {
    let key = account(engine);
    STATE.with_borrow_mut(|s| {
        if s.account != key {
            *s = State {
                account: key,
                ..State::default()
            };
        }
    });
    STATE.with_borrow(|s| {
        let rows: Vec<_> = s.rows.iter().rev().filter_map(|row| {
            let name = tr(label(row["action"].as_str()?)?);
            let when = crate::fmt::shown_when(row["received"].as_u64().unwrap_or_default().min(i64::MAX as u64) as i64);
            Some(json!({"id": row["id"], "name": name, "enabled": true, "cells": [when, name, row["moderator"], row["target"], row["reason"], row["source"]]}))
        }).collect();
        json!({"kind": "moderation_activity", "domain": "moderation", "title": tr("moderation.activity_title"), "showTitle": true, "icon": "history", "intro": tr("moderation.activity_note"),
            "columns": [super::view::column("received", "moderation.activity_received", 160), super::view::column("action", "moderation.cross_action", 160), super::view::column("moderator", "moderation.moderator", 140), super::view::column("target", "moderation.user", 140), super::view::column("reason", "moderation.reason", 200), super::view::column("source", "moderation.activity_source", 140)],
            "rows": rows, "empty": tr("moderation.activity_empty"), "search": true, "actions": []})
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moderation_activity_is_scoped_and_bounded() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            engine.change(|c| c.twitch.channel_id = "owner".into());
            STATE.with_borrow_mut(|s| *s = State::default());
            let mut entry = json!({"broadcaster_user_id": "other", "source_broadcaster_user_name": "Partner", "moderator_user_name": "Mod", "action": "ban", "ban": {"user_name": "User", "reason": "Spam"}});
            event(&engine, &entry);
            assert!(table(&engine)["rows"].as_array().unwrap().is_empty());
            entry["broadcaster_user_id"] = "owner".into();
            for _ in 0..101 {
                event(&engine, &entry);
            }
            let view = table(&engine);
            assert_eq!(view["rows"].as_array().unwrap().len(), 100);
            assert_eq!(view["rows"][0]["cells"][2], "Mod");
            assert_eq!(view["rows"][0]["cells"][3], "User");
            assert_eq!(view["rows"][0]["cells"][4], "Spam");
            assert_eq!(view["rows"][0]["cells"][5], "Partner");
            STATE.with_borrow_mut(|s| *s = State::default());
        });
    }
}
