//! What overlays are told: topics with a last value (so an overlay that opens late shows the current
//! state), sent to the pages that listen as `Overlay` events. Built-in topics: `viewers` and `ad`.
//! Anything else is published by the Overlay sub-action. A topic nobody listens to costs a map entry.

use super::Engine;
use serde_json::{json, Value};

/// The most topics kept, so a loop that publishes endless names can't grow the memory.
const MAX_TOPICS: usize = 64;

impl Engine {
    /// Set a topic's value and send it to the overlays listening.
    pub fn overlay_publish(&self, topic: &str, value: Value) {
        if topic.is_empty() || topic.len() > 100 {
            return;
        }
        {
            let mut seen = self.overlay_state.borrow_mut();
            if !seen.contains_key(topic) && seen.len() >= MAX_TOPICS {
                return;
            }
            seen.insert(topic.into(), value.clone());
        }
        self.emit("Overlay", topic, || value);
    }

    /// The value a new listener of `topic` starts with.
    pub fn overlay_value(&self, topic: &str) -> Option<Value> {
        if topic == "viewers" {
            return Some(self.viewers_value());
        }
        self.overlay_state.borrow().get(topic).cloned()
    }

    /// Every topic that has a value.
    pub fn overlay_topics(&self) -> Vec<String> {
        let mut t: Vec<String> = self.overlay_state.borrow().keys().cloned().collect();
        t.push("viewers".into());
        t.sort();
        t
    }

    /// Live viewers of each connected platform, and their sum.
    pub fn viewers_value(&self) -> Value {
        let counts = self.chat_viewers();
        let mut v = serde_json::Map::new();
        for (p, c) in super::chatlog::PLATFORMS.iter().zip(counts) {
            v.insert((*p).into(), c.map_or(Value::Null, Value::from));
        }
        v.insert("total".into(), counts.iter().flatten().sum::<u64>().into());
        Value::Object(v)
    }

    /// The live count is polled when the window shows it (the Chat page), an action uses its trigger, or an
    /// overlay listens to `viewers`.
    pub fn viewers_wanted(&self) -> bool {
        crate::host::viewers_visible() || self.listening("Overlay", "viewers")
    }

    /// An ad break started: tell overlays, and that it ended when its time is up.
    pub fn overlay_ad(self: &std::rc::Rc<Self>, seconds: u64, automatic: bool, started: &str) {
        let ends = crate::templates::clock::now_ms() + seconds * 1000;
        self.overlay_publish(
            "ad",
            json!({"running": true, "duration": seconds, "isAutomatic": automatic, "startedAt": started,
                "endsAt": ends}),
        );
        let engine = self.clone();
        tokio::task::spawn_local(async move {
            tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
            // A newer break replaced this one: leave its state.
            if engine
                .overlay_value("ad")
                .is_some_and(|v| v["endsAt"].as_u64() == Some(ends))
            {
                engine.overlay_publish("ad", json!({"running": false, "duration": seconds}));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::events::Subs;
    use super::super::tests::{engine, run};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn an_ad_break_starts_and_ends_unless_a_newer_one_replaced_it() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            e.overlay_ad(0, true, "2026-01-01T00:00:00Z");
            let v = e.overlay_value("ad").unwrap();
            assert_eq!(
                (v["running"].clone(), v["isAutomatic"].clone()),
                (json!(true), json!(true))
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
            assert_eq!(e.overlay_value("ad").unwrap()["running"], false);
            // The first break's timer must not end the second one.
            e.overlay_ad(0, false, "");
            e.overlay_ad(60, false, "");
            tokio::time::sleep(Duration::from_millis(20)).await;
            let v = e.overlay_value("ad").unwrap();
            assert_eq!((v["running"].clone(), v["duration"].clone()), (json!(true), json!(60)));
        });
    }

    #[test]
    fn topics_are_limited_and_sent_to_listeners() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut subs = Subs::new();
            subs.entry("Overlay".into()).or_default().insert("t1".into());
            e.listen(1, &subs, &tx);
            e.overlay_publish("t1", json!({"a": 1}));
            e.overlay_publish("t2", json!(2));
            let m: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(m["data"], json!({"a": 1}));
            assert!(rx.try_recv().is_err(), "t2 isn't listened to");
            e.overlay_publish("", json!(1));
            e.overlay_publish(&"x".repeat(101), json!(1));
            for i in 0..100 {
                e.overlay_publish(&format!("n{i}"), json!(i));
            }
            let topics = e.overlay_topics();
            assert_eq!(topics.len(), super::MAX_TOPICS + 1, "the topics kept and viewers");
            // A topic already kept still changes when the map is full.
            e.overlay_publish("t1", json!(3));
            assert_eq!(e.overlay_value("t1"), Some(json!(3)));
            assert_eq!(e.overlay_value("n99"), None);
        });
    }

    #[test]
    fn viewers_add_up_the_connected_platforms_and_polls_follow_listeners() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        run(async {
            let e = engine(json!({"actions": []}));
            e.st.borrow_mut().dev_out = Some(Vec::new());
            assert!(!e.viewers_wanted());
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut subs = Subs::new();
            subs.entry("Overlay".into()).or_default().insert("viewers".into());
            e.listen(7, &subs, &tx);
            assert!(e.viewers_wanted());
            for p in ["twitch", "kick"] {
                e.simulate_chat_connection(p, Some(true));
            }
            e.set_chat_viewers("twitch", Some(10));
            e.set_chat_viewers("kick", Some(5));
            e.set_chat_viewers("youtube", Some(99));
            let v = e.overlay_value("viewers").unwrap();
            assert_eq!(v["total"], 15, "{v}");
            assert_eq!((v["twitch"].clone(), v["youtube"].clone()), (json!(10), json!(null)));
            let mut last = serde_json::Value::Null;
            while let Ok(m) = rx.try_recv() {
                last = serde_json::from_str(&m).unwrap();
            }
            assert_eq!(last["data"]["total"], 15);
            e.listen(7, &Subs::new(), &tx);
            assert!(!e.viewers_wanted());
        });
    }
}
