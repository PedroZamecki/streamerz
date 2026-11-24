//! The Voices of the Sounds page: the system's voices (espeak-ng on Linux, the system's speech on
//! Windows), each to try and to make the default, and how they speak (speed, pitch, volume).
use super::pages::button;
use super::view::{column, note};
use crate::automation::schema::{F, W};
use crate::engine::Engine;
use crate::i18n::{tr, trf};
use crate::model::Obj;
use serde_json::{json, Value};
use std::rc::Rc;

/// The voices, asked of the system once and kept until Refresh (espeak-ng is a program to start).
fn listed(refresh: bool) -> Result<Vec<crate::voice::Voice>, String> {
    thread_local! {
        static KNOWN: std::cell::RefCell<Option<Result<Vec<crate::voice::Voice>, String>>> = const { std::cell::RefCell::new(None) };
    }
    KNOWN.with_borrow_mut(|known| {
        if refresh || known.is_none() {
            *known = Some(crate::voice::voices());
        }
        known.clone().unwrap_or_else(|| Ok(Vec::new()))
    })
}

/// The Voices table of the Sounds page.
pub fn table(engine: &Rc<Engine>) -> Value {
    let settings = engine.config().voices.clone();
    let (rows, empty, intro) = match listed(false) {
        Ok(voices) => {
            let rows: Vec<Value> = voices
                .iter()
                .map(|v| {
                    let default = v.id == settings.voice;
                    json!({"id": v.id, "name": v.name, "enabled": true,
                        "cells": [note("mic", v.name.clone()), v.language, match v.gender.as_str() {
                            "male" => tr("voices.male"), "female" => tr("voices.female"), _ => "" },
                            if default { note("star-filled", tr("voices.default")) } else { Value::from("") }]})
                })
                .collect();
            (rows, tr("voices.none").to_string(), tr("voices.intro").to_string())
        }
        Err(why) => (Vec::new(), why, tr("voices.intro").to_string()),
    };
    let mut say = button("voice_say", "voices.say", "play", true);
    say["form"] = "voice_say:".into();
    let mut how = button("voice_settings", "voices.how", "sliders-horizontal", false);
    how["form"] = "voice_settings:".into();
    how["top"] = true.into();
    let mut refresh = button("voices_refresh", "btn.refresh", "refresh-cw", false);
    refresh["top"] = true.into();
    let state = engine.speech_state();
    let paused = state["paused"] == true;
    let mut pause = button(
        "speech_pause",
        if paused { "voices.resume" } else { "voices.pause" },
        if paused { "play" } else { "pause" },
        false,
    );
    pause["top"] = true.into();
    let mut clear = button("speech_clear", "voices.clear", "eraser", false);
    clear["top"] = true.into();
    clear["enabled"] = (state["waiting"].as_u64().unwrap_or(0) > 0).into();
    let mut t = super::pages::table(
        "sounds_voices",
        tr("sounds.show.voices"),
        &empty,
        vec![
            column("voice", "sounds.voice", 260),
            column("language", "voices.language", 120),
            column("gender", "voices.gender", 100),
            column("default", "voices.default_column", 120),
        ],
        rows,
        vec![
            how,
            pause,
            clear,
            refresh,
            say,
            button("voice_default", "voices.make_default", "star", true),
            button("speech_stop", "voices.stop", "square", false),
        ],
    );
    t["domain"] = json!(super::sounds::DOMAIN);
    // What waits and whether speech is paused or off, with the voices' own intro.
    let mut said = intro;
    let waiting = state["waiting"].as_u64().unwrap_or(0);
    if waiting > 0 {
        said = format!("{said} {}", trf("voices.waiting", &[("count", &waiting.to_string())]));
    }
    if paused {
        said = format!("{said} {}", tr("voices.paused"));
    }
    if state["enabled"] == false {
        said = format!("{said} {}", tr("voices.off"));
    }
    t["intro"] = said.into();
    t["search"] = true.into();
    t["open"] = "voice_say".into();
    t
}

pub fn act(engine: &Rc<Engine>, action: &str, id: &str) -> Option<Result<Value, String>> {
    Some(match action {
        "voices_refresh" => listed(true).map(|v| json!({"accepted": true, "count": v.len()})),
        "speech_stop" => Ok(json!({"accepted": true, "stopped": engine.speech_stop()})),
        "speech_clear" => Ok(json!({"accepted": true, "cleared": engine.speech_clear()})),
        "speech_pause" => {
            let paused = engine.speech_state()["paused"] == true;
            engine.speech_pause(!paused);
            Ok(json!({"accepted": true}))
        }
        "voice_default" => {
            let id = id.to_string();
            engine.change(|c| c.voices.voice = id);
            Ok(json!({"accepted": true}))
        }
        _ => return None,
    })
}

const SAY_FIELDS: &[F] = &[
    ("text", "voices.text", W::Multi),
    ("monitor", "voices.on_monitor", W::Bool),
];
const HOW_FIELDS: &[F] = &[
    ("speed", "voices.speed", W::Num),
    ("pitch", "voices.pitch", W::Num),
    ("volume", "sounds.volume", W::Int),
];

pub fn external_fields(kind: &str) -> Option<&'static [F]> {
    if kind.starts_with("voice_say:") {
        Some(SAY_FIELDS)
    } else if kind.starts_with("voice_settings:") {
        Some(HOW_FIELDS)
    } else {
        None
    }
}

pub fn external_form(engine: &Rc<Engine>, kind: &str) -> Option<Result<Value, String>> {
    let (fields, title, item) = if let Some(voice) = kind.strip_prefix("voice_say:") {
        let shown = listed(false)
            .ok()
            .and_then(|all| all.into_iter().find(|v| v.id == voice))
            .map_or_else(|| voice.to_string(), |v| v.name);
        (
            SAY_FIELDS,
            trf("voices.say_title", &[("voice", &shown)]),
            json!({"id": voice, "text": tr("voices.sample"), "monitor": true}),
        )
    } else if kind.starts_with("voice_settings:") {
        let s = engine.config().voices.clone();
        (
            HOW_FIELDS,
            tr("voices.how").to_string(),
            json!({"id": "", "speed": s.speed, "pitch": s.pitch, "volume": s.volume}),
        )
    } else {
        return None;
    };
    let object = item.as_object().cloned().unwrap_or_default();
    let fields: Vec<Value> = fields
        .iter()
        .map(|(key, label, widget)| {
            let mut field = json!({"key": key, "label": crate::automation::schema::field_label(label), "widget": widget, "holds": widget.about(),
                "value": crate::automation::edits::shown(widget, object.get(*key))});
            match *key {
                "speed" => field["widget"] = json!({"kind": "slider", "min": 0.5, "max": 3.0, "step": 0.1}),
                "pitch" => field["widget"] = json!({"kind": "slider", "min": 0.5, "max": 1.5, "step": 0.05}),
                "volume" => field["widget"] = json!({"kind": "slider", "min": 0, "max": 100, "step": 1}),
                _ => {}
            }
            if field["widget"]["kind"] == "slider" {
                field["value"] = object.get(*key).cloned().unwrap_or(json!(1));
            }
            field
        })
        .collect();
    let ok = if kind.starts_with("voice_say:") {
        tr("voices.speak")
    } else {
        tr("btn.save")
    };
    Some(Ok(
        json!({"kind": kind, "title": title, "okLabel": ok, "json": false, "new": item.clone(), "fields": fields, "item": item}),
    ))
}

fn number(item: &Obj, key: &str, or: f64) -> f64 {
    item.get(key)
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| crate::fmt::parse_number(s.trim())))
        })
        .filter(|n| n.is_finite())
        .unwrap_or(or)
}

pub fn save_external(engine: &Rc<Engine>, kind: &str, item: Obj) -> Option<Result<Value, String>> {
    if let Some(voice) = kind.strip_prefix("voice_say:") {
        let text = item
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if text.is_empty() {
            return Some(Err(tr("voices.needs_text").into()));
        }
        let monitor = item.get("monitor").and_then(Value::as_bool).unwrap_or(true);
        let s = engine.config().voices.clone();
        engine.say_now(
            &text,
            voice,
            s.speed,
            s.pitch,
            s.volume,
            if monitor { "Monitor" } else { "" },
        );
        return Some(Ok(json!({"saved": voice, "message": tr("voices.speaking")})));
    }
    if kind.starts_with("voice_settings:") {
        let (speed, pitch, volume) = (
            number(&item, "speed", 1.0).clamp(0.5, 3.0),
            number(&item, "pitch", 1.0).clamp(0.5, 1.5),
            number(&item, "volume", 100.0).clamp(0.0, 100.0),
        );
        engine.change(|c| {
            c.voices.speed = speed;
            c.voices.pitch = pitch;
            c.voices.volume = volume;
        });
        return Some(Ok(json!({"saved": "voices"})));
    }
    None
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn the_voices_are_listed_tried_and_one_made_the_default() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(json!({"actions": []}));
            // No espeak-ng: the table says how to install it.
            crate::voice::TEST_PROGRAM.with_borrow_mut(|p| *p = Some("/nowhere/espeak-ng".into()));
            listed(true).ok();
            let t = table(&e);
            assert!(t["empty"].as_str().unwrap().contains("espeak-ng"), "{}", t["empty"]);
            // A stand-in that lists two voices.
            let dir = std::env::temp_dir().join(format!("streamerz-voices-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let fake = dir.join("espeak-ng");
            std::fs::write(&fake, "#!/bin/sh\nprintf 'Pty Language Age/Gender VoiceName File Other\\n 5 en-US --/M English_(America) gmw/en-US\\n 5 pt-BR --/F Portuguese_(Brazil) roa/pt-BR\\n'\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
            crate::voice::TEST_PROGRAM.with_borrow_mut(|p| *p = Some(fake.display().to_string()));
            act(&e, "voices_refresh", "").unwrap().unwrap();
            act(&e, "voice_default", "pt-BR").unwrap().unwrap();
            let t = table(&e);
            let rows = t["rows"].as_array().unwrap();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[1]["cells"][0]["text"], "Portuguese (Brazil)");
            assert_eq!(rows[1]["cells"][3]["text"], "Default", "{}", rows[1]);
            // The forms' fields are known where a save is read (a save of an unknown kind is refused).
            for kind in [
                "voice_say:pt-BR",
                "voice_settings:",
                "sound_pad:",
                "sound_add:",
                "sound_volume:1",
                "sounds_output:",
            ] {
                assert!(super::super::pages::external_fields(&e, kind).is_some(), "{kind}");
            }
            let say = external_form(&e, "voice_say:pt-BR").unwrap().unwrap();
            assert_eq!(say["title"], "Say with Portuguese (Brazil)");
            let mut item = Obj::new();
            item.insert("speed".into(), json!(9));
            save_external(&e, "voice_settings:", item).unwrap().unwrap();
            assert_eq!(e.config().voices.speed, 3.0, "kept within its ends");
            let mut empty = Obj::new();
            empty.insert("text".into(), json!("  "));
            assert!(save_external(&e, "voice_say:pt-BR", empty).unwrap().is_err());
            crate::voice::TEST_PROGRAM.with_borrow_mut(|p| *p = None);
            listed(true).ok();
            let _ = std::fs::remove_dir_all(&dir);
        });
    }
}
