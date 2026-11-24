//! What overlays are told: topics with a last value (so an overlay that opens late shows the current
//! state), sent to the pages that listen as `Overlay` events. Built-in topics: `viewers`, `ad` and
//! `songrequests.queue` (read from request state, with no cached copy).
//! Anything else is published by the Overlay sub-action. A topic nobody listens to costs a map entry.

use super::Engine;
use crate::model::Obj;
use serde_json::{json, Map, Value};
use std::rc::Rc;

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
        if topic == "songrequests.queue" {
            return Some(self.song_queue_value());
        }
        self.overlay_state.borrow().get(topic).cloned()
    }

    /// Every topic that has a value.
    pub fn overlay_topics(&self) -> Vec<String> {
        let mut t: Vec<String> = self.overlay_state.borrow().keys().cloned().collect();
        t.push("viewers".into());
        t.push("songrequests.queue".into());
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

/// Overlay Control Changed: a live control of an overlay was set (Streamerz only).
pub const T_OVERLAY_CONTROL: u32 = 99881;

/// Drafts: the settings an overlay's editor changed and has not saved. They reach only the pages opened
/// as a preview (`?preview` in their address): OBS keeps showing the saved ones until Save.
impl Engine {
    /// The unsaved values of an overlay (empty: none).
    pub fn overlay_draft(&self, group: &str) -> Map<String, Value> {
        self.overlay_drafts.borrow().get(group).cloned().unwrap_or_default()
    }

    /// The settings a preview page of `group` shows: the saved ones with the draft over them.
    pub fn overlay_preview_settings(&self, group: &str) -> Value {
        let draft = self.overlay_draft(group);
        crate::overlays::settings_with(&self.config(), group, &draft)
    }

    /// Sets a live control of an overlay (`meta.template.controls`): checked, kept (a button is only
    /// pressed), sent to the overlay's pages and told to actions (Overlay Control Changed, unless
    /// `quiet`: an action that trigger ran setting a control doesn't run it again). `from` says who:
    /// `window`, `dock`, `action`, `page` or `mcp`.
    pub fn overlay_control_set(
        self: &Rc<Self>,
        group: &str,
        name: &str,
        value: &Value,
        from: &str,
        quiet: bool,
    ) -> Result<Value, String> {
        let (control, old) = {
            let config = self.config();
            if !crate::groups::exists(&config, group) {
                return Err(format!("there is no overlay {group}"));
            }
            crate::groups::controls(&config, group)
                .into_iter()
                .find(|(c, _)| c.name == name)
                .ok_or_else(|| format!("{group} has no control {name}"))?
        };
        let value = control.check(value)?;
        let value = if control.kind == "button" && value.is_null() {
            json!(crate::fmt::unix_now().as_millis() as u64)
        } else {
            value
        };
        if control.kind != "button" {
            self.change(|c| {
                let all = c.overlay_controls.entry(group.to_string()).or_insert_with(|| json!({}));
                if !all.is_object() {
                    *all = json!({});
                }
                all[name] = value.clone();
            });
        }
        let old = if control.kind == "button" { Value::Null } else { old };
        let data = json!({"name": name, "value": value, "old": old, "from": from});
        self.emit("Control", group, || data.clone());
        if !quiet {
            let mut args = Obj::new();
            args.insert("overlay".into(), group.into());
            args.insert("control".into(), name.into());
            args.insert("value".into(), value.clone());
            args.insert("old".into(), old.clone());
            args.insert("from".into(), from.into());
            self.fire(
                T_OVERLAY_CONTROL,
                "Overlay Control Changed",
                "Core/Overlays",
                "overlays",
                &args,
                |t| {
                    let wants = |k: &str, have: &str| {
                        t.get(k)
                            .and_then(Value::as_str)
                            .is_none_or(|w| w.is_empty() || w == have)
                    };
                    wants("overlay", group) && wants("control", name)
                },
            );
        }
        Ok(json!({"name": name, "value": value, "old": old}))
    }

    /// Plays an `animation` setting again on the pages of `group` that use it (`streamerz.animate`).
    pub fn overlay_play(&self, group: &str, setting: &str) {
        self.emit("Play", group, || json!({"name": setting}));
    }

    fn draft_changed(&self, group: &str) {
        let settings = self.overlay_preview_settings(group);
        self.emit("Draft", group, || settings);
    }

    /// Change some settings of an overlay's draft, each checked against its setting. The preview pages
    /// follow at once.
    pub fn overlay_draft_set(&self, group: &str, values: &Map<String, Value>) -> Result<(), String> {
        let defs = crate::groups::settings_defs(&crate::groups::meta(&self.config(), group));
        for (name, value) in values {
            let def = defs
                .iter()
                .find(|d| &d.name == name)
                .ok_or_else(|| format!("{group} has no setting {name}"))?;
            if !def.accepts(value) {
                return Err(format!("{} is not a {}", def.label, def.kind.name()));
            }
        }
        {
            let mut drafts = self.overlay_drafts.borrow_mut();
            let draft = drafts.entry(group.to_string()).or_default();
            let saved = crate::groups::values(&self.config(), group);
            for (name, value) in values {
                // A value put back as it is saved is no change.
                if saved.get(name) == Some(value) {
                    draft.remove(name);
                } else {
                    draft.insert(name.clone(), value.clone());
                }
            }
            if draft.is_empty() {
                drafts.remove(group);
            }
        }
        self.draft_changed(group);
        Ok(())
    }

    /// Keep the draft: its values are saved and every page, OBS too, shows them.
    pub fn overlay_draft_save(self: &Rc<Self>, group: &str) -> Result<usize, String> {
        let Some(draft) = self.overlay_drafts.borrow_mut().remove(group) else {
            return Ok(0);
        };
        let mut values = crate::groups::values(&self.config(), group);
        let changed = draft.len();
        values.extend(draft);
        let mut result = Ok(());
        self.change(|c| result = crate::groups::set_values(c, group, values));
        self.draft_changed(group);
        result.map(|_| changed)
    }

    /// Drop the draft (or one setting of it): the preview pages show the saved values again.
    pub fn overlay_draft_revert(&self, group: &str, only: Option<&str>) {
        {
            let mut drafts = self.overlay_drafts.borrow_mut();
            match only {
                Some(name) => {
                    if let Some(draft) = drafts.get_mut(group) {
                        draft.remove(name);
                        if draft.is_empty() {
                            drafts.remove(group);
                        }
                    }
                }
                None => {
                    drafts.remove(group);
                }
            }
        }
        self.draft_changed(group);
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
            assert_eq!(topics.len(), super::MAX_TOPICS + 2, "the topics kept and viewers");
            // A topic already kept still changes when the map is full.
            e.overlay_publish("t1", json!(3));
            assert_eq!(e.overlay_value("t1"), Some(json!(3)));
            assert_eq!(e.overlay_value("n99"), None);
        });
    }

    #[test]
    fn viewers_add_up_the_connected_platforms_and_polls_follow_listeners() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::host::set_viewers_visible(false);
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
