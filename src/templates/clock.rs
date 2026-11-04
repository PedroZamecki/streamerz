//! Persisted countdowns for installed templates. No task is needed while they run.
use crate::Config;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

const KEY: &str = "templateCountdowns";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Countdown {
    remaining_ms: u64,
    max_ms: u64,
    started_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub remaining_ms: u64,
    pub max_ms: u64,
    pub running: bool,
    pub deadline_ms: Option<u64>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn load(config: &Config, id: &str) -> Option<Countdown> {
    serde_json::from_value(config.rest.get(KEY)?.get(id)?.clone()).ok()
}

fn store(config: &mut Config, id: &str, timer: &Countdown) -> bool {
    let timers = config
        .rest
        .entry(KEY)
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(timers) = timers.as_object_mut() else {
        return false;
    };
    let Ok(value) = serde_json::to_value(timer) else {
        return false;
    };
    timers.insert(id.to_string(), value);
    true
}

impl Countdown {
    fn remaining(&self, now: u64) -> u64 {
        self.remaining_ms
            .saturating_sub(self.started_ms.map_or(0, |start| now.saturating_sub(start)))
    }
    fn snapshot(&self, now: u64) -> Snapshot {
        let remaining_ms = self.remaining(now);
        Snapshot {
            remaining_ms,
            max_ms: self.max_ms,
            running: self.started_ms.is_some(),
            deadline_ms: self.started_ms.map(|_| now.saturating_add(remaining_ms)),
        }
    }
}

pub fn create(config: &mut Config, id: &str, start_ms: u64, max_ms: u64, now_ms: u64) -> bool {
    if id.is_empty()
        || config
            .rest
            .get(KEY)
            .and_then(Value::as_object)
            .is_some_and(|timers| timers.contains_key(id))
    {
        return false;
    }
    store(
        config,
        id,
        &Countdown {
            remaining_ms: start_ms.min(max_ms),
            max_ms,
            started_ms: Some(now_ms),
        },
    )
}

pub fn get(config: &Config, id: &str, now_ms: u64) -> Option<Snapshot> {
    Some(load(config, id)?.snapshot(now_ms))
}

pub fn add(config: &mut Config, id: &str, amount_ms: u64, now_ms: u64) -> bool {
    let Some(mut timer) = load(config, id) else {
        return false;
    };
    timer.remaining_ms = timer.remaining(now_ms).saturating_add(amount_ms).min(timer.max_ms);
    if timer.started_ms.is_some() {
        timer.started_ms = Some(now_ms);
    }
    store(config, id, &timer)
}

/// Change the cap without resetting elapsed time or changing whether the timer is running.
pub fn set_max(config: &mut Config, id: &str, max_ms: u64, now_ms: u64) -> bool {
    let Some(mut timer) = load(config, id) else {
        return false;
    };
    timer.remaining_ms = timer.remaining(now_ms).min(max_ms);
    timer.max_ms = max_ms;
    if timer.started_ms.is_some() {
        timer.started_ms = Some(now_ms);
    }
    store(config, id, &timer)
}

/// Restart a countdown, making it first if it was never made.
pub fn reset(config: &mut Config, id: &str, start_ms: u64, max_ms: u64, now_ms: u64) -> bool {
    store(
        config,
        id,
        &Countdown {
            remaining_ms: start_ms.min(max_ms),
            max_ms,
            started_ms: Some(now_ms),
        },
    )
}

/// A number of milliseconds typed in a sub-action field: a number, or text such as `%group.max%`
/// once the group's values are in it.
pub fn field_ms(v: Option<&Value>, text: impl FnOnce(&str) -> String) -> Option<u64> {
    let n = match v? {
        Value::Number(n) => n.as_f64()?,
        Value::String(t) => text(t).trim().parse::<f64>().ok()?,
        _ => return None,
    };
    (n.is_finite() && (0.0..1e15).contains(&n)).then(|| n.round() as u64)
}

/// Start the countdowns these actions reset, each with its starting and maximum time, unless they
/// already exist. This is what makes a countdown start when its group is installed.
pub fn start_for(config: &mut Config, actions: &[crate::model::Action], now_ms: u64) {
    let mut found = Vec::new();
    for a in actions {
        crate::edit::walk(&a.sub_actions, &mut |o| {
            if o.get("type").and_then(Value::as_u64) == Some(99803) && o.get("operation") == Some(&Value::from("reset"))
            {
                let text = |t: &str| crate::groups::resolve(config, &a.group, t);
                let id = text(o.get("clockId").and_then(Value::as_str).unwrap_or(""));
                if let (false, Some(start), Some(max)) = (
                    id.is_empty(),
                    field_ms(o.get("startMs"), text),
                    field_ms(o.get("maxMs"), text),
                ) {
                    found.push((id, start, max));
                }
            }
        });
    }
    for (id, start, max) in found {
        create(config, &id, start, max, now_ms);
    }
}

/// Snapshot on orderly shutdown, recording remaining time rather than a deadline.
pub fn pause_all(config: &mut Config, now_ms: u64) {
    let Some(timers) = config.rest.get_mut(KEY).and_then(Value::as_object_mut) else {
        return;
    };
    for value in timers.values_mut() {
        if let Ok(mut timer) = serde_json::from_value::<Countdown>(value.clone()) {
            timer.remaining_ms = timer.remaining(now_ms);
            timer.started_ms = None;
            if let Ok(updated) = serde_json::to_value(timer) {
                *value = updated;
            }
        }
    }
}

/// Resume saved snapshots on startup. After a crash, the last saved remaining value is used.
pub fn resume_all(config: &mut Config, now_ms: u64) {
    let Some(timers) = config.rest.get_mut(KEY).and_then(Value::as_object_mut) else {
        return;
    };
    for value in timers.values_mut() {
        if let Ok(mut timer) = serde_json::from_value::<Countdown>(value.clone()) {
            timer.started_ms = Some(now_ms);
            if let Ok(updated) = serde_json::to_value(timer) {
                *value = updated;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_caps_and_pauses_while_closed() {
        let mut c = Config::default();
        assert!(create(&mut c, "test", 10_000, 20_000, 100));
        assert!(!create(&mut c, "test", 1, 1, 100));
        assert_eq!(get(&c, "test", 2_100).unwrap().remaining_ms, 8_000);
        assert!(add(&mut c, "test", 15_000, 2_100));
        assert_eq!(get(&c, "test", 2_100).unwrap().remaining_ms, 20_000);
        assert!(set_max(&mut c, "test", 18_000, 2_100));
        assert_eq!(get(&c, "test", 2_100).unwrap().remaining_ms, 18_000);
        pause_all(&mut c, 5_100);
        assert_eq!(get(&c, "test", 100_000).unwrap().remaining_ms, 15_000);
        resume_all(&mut c, 100_000);
        assert_eq!(get(&c, "test", 101_250).unwrap().remaining_ms, 13_750);
        assert!(reset(&mut c, "test", u64::MAX, 20_000, 101_250));
        assert_eq!(get(&c, "test", 101_250).unwrap().remaining_ms, 20_000);
    }

    #[test]
    fn routine_save_does_not_pause_but_shutdown_does() {
        let mut config = Config::default();
        assert!(create(&mut config, "test", 60_000, 60_000, now_ms()));
        let cfg = std::sync::Arc::new(tokio::sync::watch::channel(config).0);
        let engine = crate::engine::Engine::new(cfg.clone(), |_| {});
        engine.save_all();
        assert!(get(&cfg.borrow(), "test", now_ms()).unwrap().running);
        engine.save_shutdown();
        assert!(!get(&cfg.borrow(), "test", now_ms()).unwrap().running);
        let restarted = crate::engine::Engine::new(cfg.clone(), |_| {});
        assert!(get(&cfg.borrow(), "test", now_ms()).unwrap().running);
        drop(restarted);
    }

    #[test]
    fn malformed_storage_is_not_overwritten() {
        let mut config = Config::default();
        config.rest.insert(KEY.into(), Value::Bool(true));
        assert!(!create(&mut config, "test", 1, 1, 1));
        assert_eq!(config.rest[KEY], true);
    }
}
