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
            } else if k == "*" {
                set.clear();
            } else {
                set.remove(k);
            }
        }
        if set.is_empty() {
            subs.remove(source);
        }
    }
}

fn connection_event(kind: &str) -> bool {
    kind.ends_with("Connected")
        || kind.ends_with("Disconnected")
        || kind == "BroadcastMonitoringStarted"
        || kind == "BroadcastMonitoringEnded"
}

impl Engine {
    /// Send an event to the connections subscribed to it; `data` is only built if one is.
    pub fn emit(&self, source: &str, kind: &str, data: impl FnOnce() -> Value) {
        // A connection came or went: the window's status and chat follow.
        if connection_event(kind) {
            crate::host::changed();
        }
        if !self.hub.borrow().iter().any(|l| wants(&l.subs, source, kind)) {
            return;
        }
        let payload = data();
        let msg: Arc<str> = json!({"timeStamp": iso_now(), "event": {"source": source, "type": kind}, "data": payload})
            .to_string()
            .into();
        // Closed connections drop out here.
        self.hub
            .borrow_mut()
            .retain(|l| !wants(&l.subs, source, kind) || l.tx.send(msg.clone()).is_ok());
    }

    /// Whether anyone listens to this event (to skip work for Raw events).
    pub fn listening(&self, source: &str, kind: &str) -> bool {
        self.hub.borrow().iter().any(|l| wants(&l.subs, source, kind))
    }

    /// Whether anyone listens to any event of this source.
    pub fn listening_to(&self, source: &str) -> bool {
        self.hub
            .borrow()
            .iter()
            .any(|l| l.subs.get(source).is_some_and(|t| !t.is_empty()))
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

    #[test]
    fn unsubscribe_wildcard_clears_specific_kinds() {
        let mut s = Subs::new();
        update(&mut s, &json!({"Twitch": ["ChatMessage", "Follow"]}), true);
        assert!(wants(&s, "Twitch", "ChatMessage"));
        assert!(wants(&s, "Twitch", "Follow"));
        update(&mut s, &json!({"Twitch": ["*"]}), false);
        assert!(!s.contains_key("Twitch"));
        assert!(!wants(&s, "Twitch", "ChatMessage"));
    }

    #[test]
    fn connection_events_cover_connects_and_disconnects() {
        for kind in [
            "Connected",
            "Disconnected",
            "BroadcasterChatConnected",
            "BroadcasterChatDisconnected",
            "InstanceConnected",
            "InstanceDisconnected",
            "BroadcastMonitoringStarted",
            "BroadcastMonitoringEnded",
        ] {
            assert!(connection_event(kind), "{kind}");
        }
        assert!(!connection_event("ChatMessage"));
    }

    #[test]
    fn emit_allows_reentrant_queries_and_drops_closed_listeners() {
        let cfg = std::sync::Arc::new(tokio::sync::watch::channel(crate::Config::default()).0);
        let e = Engine::new(cfg, |_| {});
        let (tx1, mut rx1) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, rx2) = tokio::sync::mpsc::unbounded_channel();
        let mut subs = Subs::new();
        update(&mut subs, &json!({"Twitch": ["Follow"]}), true);
        e.listen(1, &subs, &tx1);
        e.listen(2, &subs, &tx2);
        drop(rx2);

        // Querying the hub inside the lazy data builder must not double-borrow self.hub.
        e.emit("Twitch", "Follow", || {
            assert!(e.listening("Twitch", "Follow"));
            assert!(e.listening_to("Twitch"));
            json!({"user": "Ana"})
        });

        let msg: Value = serde_json::from_str(&rx1.try_recv().unwrap()).unwrap();
        assert_eq!(msg["event"]["source"], "Twitch");
        assert_eq!(msg["event"]["type"], "Follow");
        assert_eq!(msg["data"]["user"], "Ana");
        assert_eq!(e.hub.borrow().len(), 1, "closed listener was pruned");

        // Unsubscribing with an empty map removes the remaining listener.
        e.listen(1, &Subs::new(), &tx1);
        assert!(!e.listening_to("Twitch"));
        let mut built = false;
        e.emit("Twitch", "Follow", || {
            built = true;
            json!({})
        });
        assert!(!built, "data is not built when nobody listens");
    }
}
