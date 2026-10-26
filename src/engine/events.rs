//! Events for WebSocket clients: each connection subscribes to sources and types (or "*"); `emit`
//! sends `{timeStamp, event: {source, type}, data}` to the ones that want it, and builds nothing
//! when nobody does.

use super::globals::iso_now;
use super::Engine;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// A connection's subscriptions: source -> types ("*" for all).
pub type Subs = HashMap<String, HashSet<String>>;

pub struct Listener {
    pub id: u64,
    pub subs: Subs,
    pub tx: UnboundedSender<Arc<str>>,
}

fn wants(subs: &Subs, source: &str, kind: &str) -> bool {
    subs.get(source).is_some_and(|t| t.contains(kind) || t.contains("*"))
}

/// Merge (or remove) a Subscribe/UnSubscribe request's `events` object.
pub fn update(subs: &mut Subs, events: &Value, add: bool) {
    let Some(m) = events.as_object() else { return };
    for (source, kinds) in m {
        let kinds = kinds
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>());
        let set = subs.entry(source.clone()).or_default();
        for k in kinds.unwrap_or_default() {
            if add {
                set.insert(k.into());
            } else {
                set.remove(k);
            }
        }
        if set.is_empty() {
            subs.remove(source);
        }
    }
}

impl Engine {
    /// Send an event to the connections subscribed to it; `data` is only built if one is.
    pub fn emit(&self, source: &str, kind: &str, data: impl FnOnce() -> Value) {
        // A connection came or went: the window's status and chat follow.
        if kind.ends_with("Connected") || kind == "BroadcastMonitoringStarted" {
            crate::ui::changed();
        }
        let mut hub = self.hub.borrow_mut();
        if !hub.iter().any(|l| wants(&l.subs, source, kind)) {
            return;
        }
        let msg: Arc<str> = json!({"timeStamp": iso_now(), "event": {"source": source, "type": kind}, "data": data()})
            .to_string()
            .into();
        // Closed connections drop out here.
        hub.retain(|l| !wants(&l.subs, source, kind) || l.tx.send(msg.clone()).is_ok());
    }

    /// Whether anyone listens to this event (to skip work for Raw events).
    pub fn listening(&self, source: &str, kind: &str) -> bool {
        self.hub.borrow().iter().any(|l| wants(&l.subs, source, kind))
    }

    /// A connection's current subscriptions changed (an empty set removes it).
    pub fn listen(&self, id: u64, subs: &Subs, tx: &UnboundedSender<Arc<str>>) {
        let mut hub = self.hub.borrow_mut();
        hub.retain(|l| l.id != id);
        if !subs.is_empty() {
            hub.push(Listener {
                id,
                subs: subs.clone(),
                tx: tx.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscriptions() {
        let mut s = Subs::new();
        update(&mut s, &json!({"General": ["Custom"], "Raw": ["*"]}), true);
        assert!(wants(&s, "Raw", "SubAction") && wants(&s, "General", "Custom") && !wants(&s, "Twitch", "Follow"));
        update(&mut s, &json!({"General": ["Custom"]}), false);
        assert!(!s.contains_key("General"));
    }
}
