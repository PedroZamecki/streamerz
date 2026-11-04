//! Speech to Text Dictation (301) and Command (302): the recognizer (`speech.rs`) listens only while
//! an enabled action uses one of them. A command is matched in what was heard, by its Location:
//! the whole phrase (0), at its start (1, the rest is `spokenTextInput`) or anywhere in it (2).

use super::{s, Engine, Obj};
use crate::speech::Heard;
use serde_json::Value;
use std::rc::Rc;

pub const T_DICTATION: u32 = 301;
pub const T_COMMAND: u32 = 302;

fn on(o: &Obj, k: &str) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// The phrases of a voice command (its `command` field, one per line).
fn phrases(c: &Obj) -> Vec<String> {
    s(c, "command")
        .lines()
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty())
        .collect()
}

/// The commands `text` says, with what came after the phrase.
pub(crate) fn matches<'a>(commands: &'a [Obj], text: &str) -> Vec<(&'a Obj, String, String)> {
    let said = text.trim().to_lowercase();
    let mut out = Vec::new();
    for c in commands.iter().filter(|c| on(c, "enabled")) {
        let at = c.get("location").and_then(Value::as_u64).unwrap_or(0);
        for p in phrases(c) {
            let hit = match at {
                1 => said
                    .strip_prefix(&p)
                    .filter(|r| r.is_empty() || r.starts_with(' '))
                    .map(str::trim),
                2 => said.contains(&p).then_some(""),
                _ => (said == p).then_some(""),
            };
            if let Some(rest) = hit {
                // The rest as it was said (its case), for Start commands.
                let rest = text
                    .trim()
                    .get(text.trim().len() - rest.len()..)
                    .unwrap_or(rest)
                    .to_string();
                out.push((c, p, rest));
                break;
            }
        }
    }
    out
}

impl Engine {
    /// Listen while an action uses the speech triggers (called when the settings change).
    pub fn speech_sync(self: &Rc<Self>) {
        let want = self.uses_trigger(T_DICTATION) || self.uses_trigger(T_COMMAND);
        let on_now = self.st.borrow().speech.is_some();
        if want == on_now {
            return;
        }
        if !want {
            self.st.borrow_mut().speech = None;
            return;
        }
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        match crate::speech::listen(tx) {
            Ok(l) => {
                crate::say("Speech to Text: listening".into());
                let me = self.clone();
                let task = tokio::task::spawn_local(async move {
                    while let Some(h) = rx.recv().await {
                        me.speech_heard(&h);
                    }
                });
                self.st.borrow_mut().speech = Some((l, task.abort_handle()));
            }
            Err(e) => crate::warn(format!("Speech to Text: {e}")),
        }
    }

    /// A phrase was heard: Dictation, and Command for each voice command it says (above the
    /// confidence asked for).
    pub(crate) fn speech_heard(self: &Rc<Self>, h: &Heard) {
        if self.config().speech_log_spoken {
            crate::say(format!(
                "Speech to Text heard: {} ({:.0}%)",
                h.text,
                h.confidence * 100.0
            ));
        }
        let mut args = Obj::new();
        args.insert("spokenText".into(), h.text.as_str().into());
        args.insert("spokenTextConfidence".into(), h.confidence.into());
        args.insert(
            "spokenTextConfidencePercent".into(),
            ((h.confidence * 100.0).round() as i64).into(),
        );
        self.fire(
            T_DICTATION,
            "Dictation",
            "Core/Voice Control",
            "voicecontrol",
            &args,
            |_| true,
        );
        self.emit("VoiceControl", "Dictation", || Value::Object(args.clone()));
        let commands = self.config().voice_commands.clone();
        let global = self.config().speech_confidence;
        for (c, phrase, rest) in matches(&commands, &h.text) {
            let need = if on(c, "overrideGlobalConfidence") {
                c.get("confidenceThreshold").and_then(Value::as_f64).unwrap_or(0.0)
            } else {
                global
            };
            if h.confidence < need {
                continue;
            }
            let mut a = args.clone();
            a.insert("spokenCommand".into(), phrase.into());
            a.insert("spokenTextInput".into(), rest.into());
            a.insert("commandId".into(), s(c, "id").into());
            a.insert("commandName".into(), s(c, "name").into());
            let id = s(c, "id").to_string();
            self.fire(T_COMMAND, "Command", "Core/Voice Control", "voicecontrol", &a, |t| {
                let want = s(t, "commandId");
                want.is_empty() || want == super::DEFAULT_QUEUE || want == id
            });
            self.emit("VoiceControl", "Command", || Value::Object(a));
        }
    }

    /// Set Voice Control Command (1018): a command's phrase.
    pub fn set_voice_command(self: &Rc<Self>, id: &str, phrase: &str) {
        self.change(|c| {
            if let Some(x) = c.voice_commands.iter_mut().find(|x| s(x, "id") == id) {
                x.insert("command".into(), phrase.into());
            }
        });
    }

    /// Set Voice Control Command State (1019): 0 on, 1 off, 2 toggle.
    pub fn set_voice_command_state(self: &Rc<Self>, id: &str, state: f64) {
        self.change(|c| {
            if let Some(x) = c.voice_commands.iter_mut().find(|x| s(x, "id") == id) {
                let now = on(x, "enabled");
                x.insert("enabled".into(), super::new_state(state, now).into());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn commands_match_by_their_location() {
        let c = |id: &str, cmd: &str, at: u64| {
            json!({"id": id, "name": id, "enabled": true, "command": cmd, "location": at})
                .as_object()
                .unwrap()
                .clone()
        };
        let list = vec![
            c("exact", "lights on", 0),
            c("start", "play song\nput on", 1),
            c("any", "scene", 2),
            {
                let mut off = c("off", "lights on", 0);
                off.insert("enabled".into(), false.into());
                off
            },
        ];
        let ids = |t: &str| {
            matches(&list, t)
                .iter()
                .map(|(c, p, r)| format!("{}:{p}:{r}", s(c, "id")))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("Lights on"), ["exact:lights on:"]);
        assert_eq!(ids("play song Never Gonna"), ["start:play song:Never Gonna"]);
        assert_eq!(
            ids("put on the next scene"),
            ["start:put on:the next scene", "any:scene:"]
        );
        assert!(ids("lights on please").is_empty());
        assert!(ids("play songs").is_empty());
    }
}
