//! The connections that are set up, and how many are up: the status line every frontend shows.

use crate::{
    engine::Engine,
    i18n::{tr, trf},
};
use serde_json::{json, Value};
use std::rc::Rc;

/// Connection failures carry a fixed, translated reason rather than a raw transport error, which
/// may contain credentials or remote user data. At most 64 configured connections are retained.
#[derive(Clone, Copy)]
pub enum Failure {
    Connect,
    Handshake,
    Closed,
    Bind,
}

thread_local! {
    static FAILURES: std::cell::RefCell<std::collections::VecDeque<(String, Failure, i64)>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

pub fn record_failure(key: &str, reason: Failure) {
    if key.is_empty() || key.len() > 256 {
        return;
    }
    FAILURES.with_borrow_mut(|items| {
        items.retain(|(id, _, _)| id != key);
        if items.len() == 64 {
            items.pop_front();
        }
        items.push_back((key.to_owned(), reason, crate::fmt::unix_now().as_secs() as i64));
    });
    crate::host::changed();
}

pub fn clear_failure(key: &str) {
    FAILURES.with_borrow_mut(|items| items.retain(|(id, _, _)| id != key));
}

pub fn failure(key: &str) -> Option<Value> {
    FAILURES.with_borrow(|items| {
        items.iter().find(|(id, _, _)| id == key).map(|(_, reason, when)| {
            let reason = tr(match reason {
                Failure::Connect => "conns.failure.connect",
                Failure::Handshake => "conns.failure.handshake",
                Failure::Closed => "conns.failure.closed",
                Failure::Bind => "conns.failure.bind",
            });
            let time = crate::fmt::shown_clock(*when);
            json!({"reason": reason, "at": when, "text": trf("conns.failure.at", &[("reason", reason), ("time", &time)])})
        })
    })
}

struct Connection {
    name: String,
    brand: &'static str,
    up: bool,
    standby: bool,
    /// The page with its settings.
    page: &'static str,
    /// What `connections` `retry` takes to try it again.
    key: String,
}

/// Every connection that is set up, with whether it is up or standing by.
fn list(engine: &Rc<Engine>) -> Vec<Connection> {
    let config = engine.config();
    let mut out = Vec::new();
    let platforms = [
        (
            "Twitch",
            "twitch",
            config.twitch.broadcaster.is_some(),
            engine.twitch_connected(),
            false,
        ),
        (
            "YouTube",
            "youtube",
            config.youtube.broadcaster.is_some() || !config.youtube.channel_id.is_empty(),
            engine.youtube_connected(),
            engine.youtube_standby(),
        ),
        (
            "Kick",
            "kick",
            config.kick.broadcaster.is_some() || !config.kick.channel.is_empty(),
            engine.kick_connected(),
            false,
        ),
        (
            "TikTok",
            "tiktok",
            crate::tiktok_bridge::set_up(&config) || !config.tiktok.channel.is_empty(),
            engine.tiktok_connected(),
            engine.tiktok_standby(),
        ),
    ];
    for (name, brand, set_up, up, standby) in platforms {
        if set_up {
            out.push(Connection {
                name: name.into(),
                brand,
                up,
                standby,
                page: "Accounts",
                key: brand.into(),
            });
        }
    }
    let named =
        |item: &crate::model::Obj, key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
    for item in &config.obs.connections {
        let id = named(item, "id");
        out.push(Connection {
            name: format!("OBS: {}", named(item, "name")),
            brand: "obsstudio",
            up: engine.obs_connected(Some(&id)),
            standby: false,
            page: "Stream Apps",
            key: format!("obs:{id}"),
        });
    }
    for item in &config.slobs.connections {
        let id = named(item, "id");
        out.push(Connection {
            name: format!("Streamlabs Desktop: {}", named(item, "name")),
            brand: "streamlabs",
            up: engine.slobs_up(&id),
            standby: false,
            page: "Stream Apps",
            key: format!("slobs:{id}"),
        });
    }
    let donations = &config.donations;
    let live = engine.donations.borrow();
    for (set_up, up, name, brand) in [
        (
            !donations.streamelements_token.trim().is_empty(),
            live.streamelements.get(),
            "StreamElements",
            "streamelements",
        ),
        (
            !donations.streamlabs_token.trim().is_empty(),
            live.streamlabs.get(),
            "Streamlabs",
            "streamlabs",
        ),
    ] {
        if set_up {
            out.push(Connection {
                name: name.into(),
                brand,
                up,
                standby: false,
                page: "Donations",
                key: brand.into(),
            });
        }
    }
    out
}

/// "3 of 5 connected" (plus the standby count, or a locked keyring) with the health it deserves.
pub fn summary(engine: &Rc<Engine>) -> Value {
    if crate::secrets::is_locked() {
        return json!({"health": "locked", "text": tr("conns.keyring_locked"), "up": 0, "total": 0, "connections": []});
    }
    let connections = list(engine);
    let up = connections.iter().filter(|connection| connection.up).count();
    let standby = connections
        .iter()
        .filter(|connection| !connection.up && connection.standby)
        .count();
    let total = connections.len();
    let (health, text) = if total == 0 {
        ("unset", tr("conns.nothing_set_up").to_owned())
    } else {
        let health = match (up, standby) {
            (0, 0) => "off",
            (up, standby) if up + standby == total && up > 0 => "up",
            (0, standby) if standby == total => "standby",
            _ => "attention",
        };
        let numbers = [
            ("up", up.to_string()),
            ("total", total.to_string()),
            ("standby", standby.to_string()),
        ];
        let numbers: Vec<(&str, &str)> = numbers.iter().map(|(key, value)| (*key, value.as_str())).collect();
        (
            health,
            trf(
                if standby > 0 {
                    "conns.summary_standby"
                } else {
                    "conns.summary"
                },
                &numbers,
            ),
        )
    };
    json!({"health": health, "text": text, "up": up, "total": total,
        "connections": connections.iter().map(|connection| {
            // What it is doing, its health (the icon and colour), what hovering it explains, and what
            // its button does: a standby is connected and only waiting, so it checks; off connects.
            let (health, state, tip, button) = if connection.up {
                ("up", tr("conns.connected"), tr("conns.connected_tip"), "")
            } else if connection.standby {
                ("standby", tr("conns.standby"), tr("conns.standby_tip"), tr("conns.check"))
            } else {
                ("off", tr("conns.off"), tr("conns.off_tip"), tr("btn.connect"))
            };
            let failure = (!connection.up && !connection.standby).then(|| failure(&connection.key)).flatten();
            let health = if failure.is_some() { "attention" } else { health };
            let reason = failure.as_ref().and_then(|f| f["text"].as_str()).unwrap_or(tip);
            let state = failure.as_ref().and_then(|f| f["text"].as_str()).unwrap_or(state);
            json!({"name": connection.name, "brand": connection.brand, "up": connection.up,
                "standby": connection.standby, "page": connection.page, "key": connection.key,
                "health": health, "state": state, "tip": reason, "failure": failure, "button": button})
        }).collect::<Vec<_>>()})
}

/// Write the settings now instead of when the debounce fires.
pub fn save(engine: &Rc<Engine>) -> Result<Value, String> {
    crate::save(&engine.config()).map_err(|error| trf("ui.cant_save", &[("error", &error.to_string())]))?;
    Ok(json!({"saved": true, "text": tr("ui.saved")}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_history_is_bounded_and_contains_only_fixed_reasons() {
        FAILURES.with_borrow_mut(|items| items.clear());
        for n in 0..70 {
            record_failure(&format!("test:{n}"), Failure::Connect);
        }
        assert!(failure("test:0").is_none());
        let last = failure("test:69").unwrap();
        assert_eq!(last["reason"], tr("conns.failure.connect"));
        assert!(last["at"].is_i64());
        assert_eq!(FAILURES.with_borrow(|items| items.len()), 64);
        clear_failure("test:69");
        assert!(failure("test:69").is_none());
        FAILURES.with_borrow_mut(|items| items.clear());
    }

    #[test]
    fn nothing_set_up_says_so_and_saving_succeeds() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let status = summary(&engine);
            assert_eq!(status["total"], 0);
            assert_eq!(status["health"], "unset");
            assert_eq!(status["text"], tr("conns.nothing_set_up"));
            engine.change(|config| {
                config
                    .obs
                    .connections
                    .push(json!({"id":"o","name":"Main"}).as_object().unwrap().clone());
            });
            let status = summary(&engine);
            assert_eq!(status["total"], 1);
            assert_eq!(status["up"], 0);
            assert_eq!(status["connections"][0]["name"], "OBS: Main");
        });
    }
}
