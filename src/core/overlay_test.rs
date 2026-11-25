//! The Test tab of an overlay's page: one event of any platform (or any trigger) with made-up values
//! that can be changed, sent once or as a burst, to this overlay, to every overlay, or as a real event
//! (`isTest`, through the actions too); the ready-made test sets; a made-up song; any topic; the last
//! real events again; and what the pages logged.
//!
//! A burst is a task that ends with it: no timer is left behind.

use crate::engine::Engine;
use crate::i18n::{tr, trf};
use crate::model::Obj;
use serde_json::{json, Value};
use std::rc::Rc;

/// The events offered first, as chips: a trigger of each kind pages count with, on each platform.
pub const EVENTS: &[u32] = &[
    101, 103, 104, 105, 106, 102, 107, 112, // Twitch
    4006, 4007, 4008, 4018, 4014, // YouTube
    35011, 35014, 35016, 35017, 35025, // Kick
    41002, 41004, 41005, 41006, // TikTok
];

/// Who the made-up events come from, by platform.
fn person(code: u32, round: usize) -> (&'static str, &'static str) {
    let names: &[&str] = match code {
        4000..=4999 => &["Leo Martins", "Rafa Gomes"],
        35000..=35999 => &["duda", "kai_k"],
        41000..=41999 => &["bia.live", "joao.tt"],
        _ => &["ana_plays", "noobmaster69"],
    };
    (names[round % names.len()], ["test-0", "test-1"][round % 2])
}

/// Made-up arguments for a trigger, as its platform would send them.
pub fn made_up(code: u32, round: usize) -> Obj {
    let (user, id) = person(code, round);
    let mut a = json!({"user": user, "userName": user, "userId": id, "isTest": true});
    let extra = match code {
        102 | 35025 => json!({"bits": 500, "kicks": 500, "message": "Take my bits!"}),
        104 | 4009 => json!({"cumulative": 12, "monthStreak": 3, "tier": "tier 1", "message": "A year already!"}),
        105 | 35016 => json!({"recipientUser": "lucky_viewer", "recipientUserName": "lucky_viewer", "tier": "tier 1"}),
        106 | 35017 | 4014 => json!({"gifts": 20, "totalGifts": 20, "count": 20, "tier": "tier 1"}),
        107 => json!({"viewers": 500, "viewerCount": 500}),
        112 => json!({"rewardName": "Hydrate", "rewardCost": 500, "rawInput": "Drink some water!"}),
        4006 => {
            json!({"amount": "$10.00", "microAmount": 10_000_000, "currencyCode": "USD", "message": "Love the stream!"})
        }
        4007 => json!({"amount": "$5.00", "microAmount": 5_000_000, "currencyCode": "USD"}),
        4008 => json!({"levelName": "Fan", "months": 6}),
        41002 => {
            json!({"giftName": "Rose", "giftId": 5655, "repeatCount": 5, "coins": 5, "diamondCount": 1, "repeatEnd": true})
        }
        41005 => json!({"shares": 1}),
        _ => json!({}),
    };
    if let (Some(a), Some(extra)) = (a.as_object_mut(), extra.as_object()) {
        a.extend(extra.clone());
    }
    a.as_object().cloned().unwrap_or_default().into_iter().collect()
}

/// `name=value` lines over the made-up arguments; numbers and true or false as typed.
pub fn with_values(mut args: Obj, lines: &str) -> Obj {
    for line in lines.lines() {
        if let Some((name, value)) = line.split_once('=') {
            let name = name.trim();
            if !name.is_empty() {
                args.insert(name.to_owned(), crate::vars::auto_type(value.trim()));
            }
        }
    }
    args
}

/// The test form of an overlay: which event, its values, how many, how often, and where it goes.
pub fn form(kind: String) -> Value {
    let mut events: Vec<String> = EVENTS
        .iter()
        .map(|code| crate::automation::labels::trigger_title(u64::from(*code)))
        .collect();
    events.push(tr("overlays.test.any").to_owned());
    let field = |key: &str, label: &str, widget: Value, value: Value| json!({"key": key, "label": label, "widget": widget, "value": value});
    json!({"kind": kind, "title": tr("overlays.tab.test"), "hint": tr("overlays.test.form_hint"),
    "okLabel": tr("overlays.test.send"), "json": false,
    "fields": [
        field("event", tr("overlays.test.event"), json!({"kind": "pick", "options": events}), json!(0)),
        field("code", tr("overlays.test.code"), json!({"kind": "text"}), json!("")),
        field("values", tr("overlays.test.values"), json!({"kind": "multi"}), json!("")),
        field("count", tr("overlays.test.count"), json!({"kind": "text"}), json!("1")),
        field("gap", tr("overlays.test.gap"), json!({"kind": "text"}), json!("1")),
        field("target", tr("overlays.test.target"), json!({"kind": "pick", "options": [
            tr("overlays.test.this"), tr("overlays.test.every"), tr("overlays.test.real")]}), json!(0)),
        field("confirm", tr("overlays.test.confirm"), json!({"kind": "bool"}), json!(false)),
    ]})
}

/// What the overlay's pages get for one event (`Chat.Alert`), if it is one a page shows.
fn alert_for(engine: &Rc<Engine>, code: u32, args: &Obj) -> Option<Value> {
    use crate::engine::chatlog::{alert_value, event_line};
    let line = event_line(code, args)?;
    let avatar = args.get("userProfileUrl").map(crate::vars::text).unwrap_or_default();
    Some(json!(["Chat.Alert", alert_value(engine, code, args, &line, &avatar)]))
}

/// Sends one made-up event to this overlay's pages (Play this rule).
pub fn play(engine: &Rc<Engine>, group: &str, code: u32, args: &Obj) -> Result<(), String> {
    send_one(engine, group, code, args, 0)
}

/// Sends `code` with `args` to `target` (0 this overlay, 1 every overlay, 2 a real event).
fn send_one(engine: &Rc<Engine>, group: &str, code: u32, args: &Obj, target: u64) -> Result<(), String> {
    if target == 2 {
        let name = crate::types::name(crate::types::TRIGGERS, code).unwrap_or("Test");
        engine.fire(code, name, "Test", "test", args, |_| true);
        return Ok(());
    }
    let alert = alert_for(engine, code, args).ok_or_else(|| tr("overlays.test.not_shown").to_owned())?;
    let groups: Vec<String> = if target == 1 {
        let mut all: Vec<String> = crate::overlays::list(&engine.config())
            .into_iter()
            .map(|(g, _)| g)
            .collect();
        all.dedup();
        all
    } else {
        vec![group.to_owned()]
    };
    for g in groups {
        let alert = alert.clone();
        engine.emit("Test", &g, || json!({"events": [alert]}));
    }
    Ok(())
}

/// The actions a real `code` would run that send chat or change OBS, by name.
fn risky_actions(engine: &Rc<Engine>, code: u32) -> Vec<String> {
    fn risky(subs: &[Obj]) -> bool {
        subs.iter().any(|s| {
            let kind = s.get("type").and_then(Value::as_u64).unwrap_or(0) as u32;
            let category = crate::types::SUB_ACTION_INFO
                .binary_search_by_key(&kind, |e| e.0)
                .map(|at| crate::types::SUB_ACTION_INFO[at].2)
                .unwrap_or_default();
            let inner: Vec<Obj> = s
                .get("subActions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_object().cloned())
                .collect();
            category.ends_with("/Chat") || category.starts_with("OBS") || risky(&inner)
        })
    }
    let config = engine.config();
    config
        .actions
        .actions
        .iter()
        .filter(|a| {
            a.triggers
                .iter()
                .any(|t| t.get("type").and_then(Value::as_u64) == Some(u64::from(code)))
        })
        .filter(|a| risky(&a.sub_actions))
        .map(|a| a.name.clone())
        .collect()
}

/// Send from the test form: its `changes` over the form's starting values.
pub fn send(engine: &Rc<Engine>, group: &str, request: &Value) -> Result<Value, String> {
    let start = form(String::new());
    let mut values: Vec<Value> = start["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| f["value"].clone())
        .collect();
    for change in request["changes"].as_array().into_iter().flatten() {
        if let Some(slot) = change["field"].as_u64().and_then(|i| values.get_mut(i as usize)) {
            *slot = change["value"].clone();
        }
    }
    let text = |at: usize| crate::vars::text(&values[at]);
    let picked = values[0].as_u64().unwrap_or(0) as usize;
    let code = match EVENTS.get(picked) {
        Some(code) => *code,
        None => text(1)
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|c| crate::types::name(crate::types::TRIGGERS, *c).is_some())
            .ok_or_else(|| tr("overlays.test.no_code").to_owned())?,
    };
    let count = text(3).trim().parse::<u64>().unwrap_or(1).clamp(1, 200);
    let gap = crate::fmt::parse_number(&text(4)).unwrap_or(1.0).clamp(0.0, 60.0);
    let target = values[5].as_u64().unwrap_or(0);
    if target == 2 && values[6] != true {
        let risky = risky_actions(engine, code);
        if !risky.is_empty() {
            return Err(trf("overlays.test.real_asks", &[("actions", &risky.join(", "))]));
        }
    }
    let lines = text(2);
    let first = with_values(made_up(code, 0), &lines);
    send_one(engine, group, code, &first, target)?;
    if count > 1 {
        let (engine, group) = (engine.clone(), group.to_owned());
        tokio::task::spawn_local(async move {
            for round in 1..count as usize {
                tokio::time::sleep(std::time::Duration::from_secs_f64(gap)).await;
                let args = with_values(made_up(code, round), &lines);
                if send_one(&engine, &group, code, &args, target).is_err() {
                    break;
                }
            }
        });
    }
    Ok(json!({"accepted": true, "message": trf("overlays.test.sent_count", &[("count", &count.to_string())])}))
}

/// The ready-made test sets: (id, label).
pub const SETS: &[(&str, &str)] = &[
    ("set:each", "overlays.test.set_each"),
    ("set:giftbomb", "overlays.test.set_giftbomb"),
    ("set:raid", "overlays.test.set_raid"),
    ("set:combo", "overlays.test.set_combo"),
    ("set:flood", "overlays.test.set_flood"),
];

/// Runs a ready-made test set on the overlay's pages.
pub fn run_set(engine: &Rc<Engine>, group: &str, id: &str) -> Result<Value, String> {
    let (engine, group) = (engine.clone(), group.to_owned());
    let set = id.to_owned();
    tokio::task::spawn_local(async move {
        let pause = |ms| tokio::time::sleep(std::time::Duration::from_millis(ms));
        match set.as_str() {
            "set:each" => {
                for (round, code) in EVENTS.iter().enumerate() {
                    let _ = send_one(&engine, &group, *code, &made_up(*code, round), 0);
                    pause(1500).await;
                }
            }
            "set:giftbomb" => {
                let args = with_values(made_up(106, 0), "gifts=20\ntotalGifts=20\ncount=20");
                let _ = send_one(&engine, &group, 106, &args, 0);
            }
            "set:raid" => {
                let args = with_values(made_up(107, 0), "viewers=500\nviewerCount=500");
                let _ = send_one(&engine, &group, 107, &args, 0);
            }
            "set:combo" => {
                for press in 1..=10 {
                    let lines = format!("repeatCount={press}\ncoins={press}\nrepeatEnd={}", press == 10);
                    let _ = send_one(&engine, &group, 41002, &with_values(made_up(41002, 0), &lines), 0);
                    pause(300).await;
                }
            }
            _ => {
                for round in 0..50usize {
                    let line = crate::engine::chatlog::Line {
                        platform: ["twitch", "youtube", "kick", "tiktok"][round % 4].into(),
                        msg_id: crate::engine::uuid(),
                        time: crate::fmt::shown_time(&crate::fmt::now()),
                        user: person(round as u32 * 1000, round).0.into(),
                        user_id: format!("test-{}", round % 2),
                        text: format!("{} {}", tr("overlays.test.chat"), round + 1),
                        ..Default::default()
                    };
                    let value = crate::core::chat::overlay_line(&engine, &line, "");
                    engine.emit("Test", &group, || json!({"events": [["Chat.Line", value]]}));
                    pause(120).await;
                }
            }
        }
    });
    Ok(json!({"accepted": true, "message": tr("overlays.test.done")}))
}

/// A made-up song, playing or paused, for pages that show what plays.
pub fn song(engine: &Rc<Engine>, group: &str, playing: bool) -> Value {
    let now = crate::fmt::unix_now().as_millis() as u64;
    let song = json!({"title": tr("overlays.test.song"), "artist": "Streamerz", "album": tr("overlays.test.album"),
        "playing": playing, "durationMs": 215_000, "positionMs": 42_000, "at": now});
    engine.emit("Test", group, || json!({"events": [], "topics": {"song": song}}));
    json!({"accepted": true, "message": tr("overlays.test.done")})
}

thread_local! {
    /// The last real events that make an alert (code and arguments), newest last, for Replay.
    static RECENT: std::cell::RefCell<std::collections::VecDeque<(u32, Obj, u64)>> = Default::default();
}

/// Keeps a real event for Replay (made-up ones are left out). At most 50.
pub fn remember(code: u32, args: &Obj) {
    if args.get("isTest").is_some_and(|t| t == &Value::Bool(true)) {
        return;
    }
    RECENT.with_borrow_mut(|recent| {
        if recent.len() >= 50 {
            recent.pop_front();
        }
        recent.push_back((code, args.clone(), crate::fmt::unix_now().as_secs()));
    });
}

/// The last real events, newest first: `{id, title, who, at}`.
pub fn recent() -> Vec<Value> {
    RECENT.with_borrow(|recent| {
        recent
            .iter()
            .enumerate()
            .rev()
            .map(|(at, (code, args, when))| {
                json!({"id": format!("replay:{at}"), "title": crate::automation::labels::trigger_title(u64::from(*code)),
                    "who": args.get("user").map(crate::vars::text).unwrap_or_default(),
                    "at": crate::fmt::shown_clock(*when as i64)})
            })
            .collect()
    })
}

/// Shows a remembered real event again on this overlay's pages.
pub fn replay(engine: &Rc<Engine>, group: &str, id: &str) -> Result<Value, String> {
    let at = id
        .strip_prefix("replay:")
        .and_then(|n| n.parse::<usize>().ok())
        .ok_or("no such event")?;
    let (code, mut args) = RECENT
        .with_borrow(|recent| recent.get(at).map(|(c, a, _)| (*c, a.clone())))
        .ok_or_else(|| tr("overlays.test.replay_gone").to_owned())?;
    args.insert("isTest".into(), true.into());
    send_one(engine, group, code, &args, 0)?;
    Ok(json!({"accepted": true, "message": tr("overlays.test.done")}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_typed_in_go_over_the_made_up_ones() {
        let args = with_values(made_up(107, 0), "viewers=12\nuser = Zed\nbad line\nlive=true");
        assert_eq!(args["viewers"], json!(12));
        assert_eq!(args["user"], json!("Zed"));
        assert_eq!(args["live"], json!(true));
        assert_eq!(args["isTest"], json!(true));
    }

    #[test]
    fn every_offered_event_has_a_title() {
        for code in EVENTS {
            assert!(crate::types::name(crate::types::TRIGGERS, *code).is_some(), "{code}");
        }
    }

    #[test]
    fn replay_keeps_the_last_fifty_real_events_only() {
        for n in 0..60u32 {
            remember(101, &with_values(Obj::new(), &format!("user=u{n}")));
        }
        remember(101, &with_values(Obj::new(), "isTest=true"));
        let list = recent();
        assert_eq!(list.len(), 50);
        assert_eq!(list[0]["who"], "u59");
    }
}
