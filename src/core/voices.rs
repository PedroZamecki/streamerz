//! The Voices and Find voices sections of the Sounds page: system voices (espeak-ng on Linux,
//! WinRT speech on Windows), optional neural voices (`Streamerz-Voice` with Piper, Kokoro and
//! Pocket TTS), and user-made voices (Kokoro mixes, Pocket TTS recordings and audio effects).
use super::pages::{button, button_if, confirm, danger};
use super::view::{column, note};
use crate::automation::schema::{F, W};
use crate::engine::Engine;
use crate::i18n::{tr, trf};
use crate::model::Obj;
use crate::voice::neural;
use serde_json::{json, Value};
use std::rc::Rc;

/// System voices, asked of the OS once and kept until Refresh (espeak-ng is a program to start).
fn system_listed(refresh: bool) -> Result<Vec<crate::voice::Voice>, String> {
    thread_local! {
        static KNOWN: std::cell::RefCell<Option<Result<Vec<crate::voice::Voice>, String>>> =
            const { std::cell::RefCell::new(None) };
    }
    KNOWN.with_borrow_mut(|known| {
        if refresh || known.is_none() {
            *known = Some(crate::voice::system_voices());
        }
        known.clone().unwrap_or_else(|| Ok(Vec::new()))
    })
}

/// All currently installed voices (custom + installed neural + cached system voices).
fn listed(engine: &Rc<Engine>, refresh: bool) -> Result<Vec<crate::voice::Voice>, String> {
    let settings = engine.config().voices.clone();
    let mut out: Vec<crate::voice::Voice> = settings
        .custom
        .iter()
        .map(|c| {
            let tag = match c.kind {
                crate::voice::CustomKind::Mix => tr("voices.kind_mix"),
                crate::voice::CustomKind::Clone => tr("voices.kind_clone"),
                crate::voice::CustomKind::Effect => tr("voices.kind_effect"),
            };
            crate::voice::Voice {
                id: c.id.clone(),
                name: format!("{} ({tag})", c.name),
                language: c.language.clone(),
                gender: String::new(),
                engine: "custom".into(),
            }
        })
        .collect();
    out.extend(neural::installed_voices());
    match system_listed(refresh) {
        Ok(sys) => {
            out.extend(sys);
            Ok(out)
        }
        Err(why) => {
            if out.is_empty() {
                Err(why)
            } else {
                Ok(out)
            }
        }
    }
}

fn mb(bytes: u64) -> String {
    format!("{} MB", (bytes as f64 / (1024.0 * 1024.0)).round().max(1.0) as u64)
}

fn engine_summary(st: &neural::EngineState) -> String {
    if let Some(dl) = &st.download {
        return match dl.phase.as_str() {
            "verifying" => trf("voices.dl_verifying", &[("name", &dl.label)]),
            "unpacking" => trf("voices.dl_unpacking", &[("name", &dl.label)]),
            _ => trf(
                "voices.dl_progress",
                &[
                    ("name", &dl.label),
                    ("percent", &dl.percent.to_string()),
                    ("done", &mb(dl.downloaded)),
                    ("total", &mb(dl.total.max(dl.downloaded))),
                ],
            ),
        };
    }
    if st.running {
        let mem = mb(st.memory_bytes);
        if st.loaded.is_some() {
            trf("voices.engine_loaded", &[("ram", &mem)])
        } else {
            trf("voices.engine_running", &[("ram", &mem)])
        }
    } else if st.installed {
        trf("voices.engine_ready", &[("disk", &mb(st.disk_bytes))])
    } else {
        tr("voices.engine_not_installed").to_string()
    }
}

fn progress_model(st: &neural::EngineState) -> Option<Value> {
    let dl = st.download.as_ref()?;
    Some(json!({
        "percent": dl.percent,
        "label": engine_summary(st),
    }))
}

fn effect_index(effect: &str) -> u64 {
    match effect {
        "echo" => 1,
        "radio" => 2,
        "robot" => 3,
        _ => 0,
    }
}

/// The installed Voices table of the Sounds page.
pub fn table(engine: &Rc<Engine>) -> Value {
    let settings = engine.config().voices.clone();
    let neural_state = neural::state();
    let mut has_base_voice = false;
    let (rows, empty, intro) = match listed(engine, false) {
        Ok(voices) => {
            has_base_voice = voices.iter().any(|v| v.engine != "custom");
            let rows: Vec<Value> = voices
                .iter()
                .map(|v| {
                    let default = v.id == settings.voice;
                    let icon = match v.engine.as_str() {
                        "custom" => "sparkles",
                        "piper" | "kokoro" => "cpu",
                        _ => "mic",
                    };
                    let kind_label = match v.engine.as_str() {
                        "piper" => "Piper",
                        "kokoro" => "Kokoro",
                        "custom" => tr("voices.engine_custom"),
                        _ => tr("voices.engine_system"),
                    };
                    let gender_label = match v.gender.as_str() {
                        "male" => tr("voices.male"),
                        "female" => tr("voices.female"),
                        _ => "",
                    };
                    let remove_confirm = if v.engine == "kokoro" {
                        tr("voices.remove_kokoro_confirm").to_string()
                    } else {
                        trf("voices.remove_named_confirm", &[("name", &v.name)])
                    };
                    json!({
                        "id": v.id,
                        "name": v.name,
                        "enabled": true,
                        "can": {
                            "voice_edit": v.engine == "custom",
                            "voice_remove": v.engine != "system",
                        },
                        "confirm": {
                            "voice_remove": remove_confirm,
                        },
                        "cells": [
                            note(icon, v.name.clone()),
                            kind_label,
                            v.language,
                            gender_label,
                            if default { note("star-filled", tr("voices.default")) } else { Value::from("") },
                        ],
                    })
                })
                .collect();
            (rows, tr("voices.none").to_string(), tr("voices.intro").to_string())
        }
        Err(why) => (Vec::new(), why, tr("voices.intro").to_string()),
    };

    let mut how = button("voice_settings", "voices.how", "sliders-horizontal", false);
    how["form"] = "voice_settings:".into();
    how["top"] = true.into();

    let kokoro_installed = neural::is_model_installed("kokoro:multi-lang-v1");
    let mut mix_btn = button_if(
        button("voice_mix", "voices.make_mix", "sparkles", false),
        kokoro_installed,
    );
    mix_btn["form"] = "voice_mix:".into();
    mix_btn["top"] = true.into();
    if !kokoro_installed {
        mix_btn["tip"] = tr("voices.needs_kokoro").into();
    }

    let pocket_installed = neural::is_model_installed("pocket:en-v1");
    let mut clone_btn = button_if(
        button("voice_clone", "voices.make_clone", "mic", false),
        pocket_installed,
    );
    clone_btn["form"] = "voice_clone:".into();
    clone_btn["top"] = true.into();
    if !pocket_installed {
        clone_btn["tip"] = tr("voices.needs_pocket").into();
    }

    let mut effect_btn = button_if(
        button("voice_effect", "voices.make_effect", "radio", false),
        has_base_voice,
    );
    effect_btn["form"] = "voice_effect:".into();
    effect_btn["top"] = true.into();
    if !has_base_voice {
        effect_btn["tip"] = tr("voices.needs_any_voice").into();
    }

    let state = engine.speech_state();
    let paused = state["paused"] == true;
    let waiting = state["waiting"].as_u64().unwrap_or(0);
    let speaking = state["speaking"] == true;

    let mut pause = button(
        "speech_pause",
        if paused { "voices.resume" } else { "voices.pause" },
        if paused { "play" } else { "pause" },
        false,
    );
    pause["top"] = true.into();

    let mut stop = button_if(
        button("speech_stop", "voices.stop", "square", false),
        speaking || waiting > 0,
    );
    stop["top"] = true.into();
    stop["hideDisabled"] = true.into();

    let mut clear = button_if(button("speech_clear", "voices.clear", "eraser", false), waiting > 0);
    clear["top"] = true.into();
    clear["hideDisabled"] = true.into();

    let mut refresh = button("voices_refresh", "btn.refresh", "refresh-cw", false);
    refresh["top"] = true.into();

    let mut actions = vec![how, mix_btn, clone_btn, effect_btn];
    if neural_state.running {
        let mut unload = button("voice_unload", "voices.unload", "power", false);
        unload["top"] = true.into();
        actions.push(unload);
    }
    if neural_state.download.is_some() {
        let mut cancel = button("voice_dl_cancel", "voices.cancel_download", "x", false);
        cancel["top"] = true.into();
        actions.push(cancel);
    }
    if neural_state.installed {
        let mut remove_engine = danger(confirm(
            button("voice_engine_remove", "voices.engine_remove", "trash-2", false),
            tr("voices.engine_remove_confirm").into(),
            "btn.delete",
        ));
        remove_engine["top"] = true.into();
        actions.push(remove_engine);
    }

    let mut say = button("voice_say", "voices.say", "play", true);
    say["form"] = "voice_say:".into();
    let mut edit = button("voice_edit", "btn.edit", "pencil", true);
    edit["form"] = "voice_edit:".into();

    actions.extend([
        pause,
        stop,
        clear,
        refresh,
        say,
        edit,
        button("voice_default", "voices.make_default", "star", true),
        danger(confirm(
            button("voice_remove", "btn.delete", "trash-2", true),
            tr("voices.remove_confirm").into(),
            "btn.delete",
        )),
    ]);

    let mut t = super::pages::table(
        "sounds_voices",
        tr("sounds.show.voices"),
        &empty,
        vec![
            column("voice", "sounds.voice", 220),
            column("engine", "voices.engine", 90),
            column("language", "voices.language", 100),
            column("gender", "voices.gender", 80),
            column("default", "voices.default_column", 90),
        ],
        rows,
        actions,
    );
    t["domain"] = json!(super::sounds::DOMAIN);
    let mut said = format!("{intro} {}", engine_summary(&neural_state));
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
    if let Some(progress) = progress_model(&neural_state) {
        t["progress"] = progress;
    }
    t["search"] = true.into();
    t["open"] = "voice_say".into();
    t
}

/// The Find voices (neural voice catalog) table of the Sounds page, filtered by the view's active toggles.
#[cfg(test)]
pub fn catalog_table(engine: &Rc<Engine>) -> Value {
    catalog_table_with(engine, &Value::Null)
}

pub fn catalog_table_with(engine: &Rc<Engine>, view: &Value) -> Value {
    let neural_state = neural::state();
    let ui_locale = {
        let loc = engine.config().locale.clone();
        if loc.is_empty() {
            crate::i18n::active().to_string()
        } else {
            loc
        }
    };
    let ui_lang = ui_locale.replace('_', "-").to_ascii_lowercase();
    let ui_prefix = ui_lang.split('-').next().unwrap_or("en").to_string();
    let active_filters: Vec<String> = view["sounds_voice_catalog"]["hidden"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let commercial_only = active_filters.iter().any(|f| f == "commercial");
    let my_language_only = active_filters.iter().any(|f| f == "my_language");
    let installed_only = active_filters.iter().any(|f| f == "installed");

    let mut entries: Vec<&neural::CatalogEntry> = neural::catalog()
        .iter()
        .filter(|e| !commercial_only || e.commercial)
        .filter(|e| {
            !my_language_only
                || e.language.eq_ignore_ascii_case("multi")
                || e.language.to_ascii_lowercase().starts_with(&ui_prefix)
        })
        .filter(|e| !installed_only || neural::is_model_installed(&e.id))
        .collect();
    // Sort UI language first, then Kokoro/Pocket packs, then remaining voices alphabetically.
    entries.sort_by_key(|e| {
        let lang = e.language.to_ascii_lowercase();
        let rank = if lang == ui_lang {
            0
        } else if lang.starts_with(&ui_prefix) {
            1
        } else if e.engine != "piper" {
            2
        } else {
            3
        };
        (rank, e.language.clone(), e.name.clone())
    });

    let busy = neural_state.download.is_some();
    let rows: Vec<Value> = entries
        .iter()
        .map(|e| {
            let installed = neural::is_model_installed(&e.id);
            let downloading = neural_state.download.as_ref().filter(|d| d.id == e.id);
            let state_cell = if let Some(dl) = downloading {
                note("download", format!("{}%", dl.percent))
            } else if installed {
                note("check", tr("voices.installed"))
            } else {
                Value::from("")
            };
            let license_cell = if e.commercial {
                Value::from(e.license.clone())
            } else {
                note(
                    "triangle-alert",
                    trf("voices.non_commercial", &[("license", &e.license)]),
                )
            };
            let license_detail = if e.commercial {
                e.license.clone()
            } else {
                trf("voices.non_commercial_detail", &[("license", &e.license)])
            };
            let dl_confirm = trf(
                "voices.download_confirm_detail",
                &[
                    ("name", &e.name),
                    ("language", &e.language),
                    ("size", &mb(e.disk_bytes)),
                    ("ram", &e.ram_mb.to_string()),
                    ("license", &license_detail),
                    ("attribution", &e.attribution),
                ],
            );
            let remove_confirm = if e.engine == "kokoro" {
                tr("voices.remove_kokoro_confirm").to_string()
            } else {
                trf("voices.remove_named_confirm", &[("name", &e.name)])
            };
            let icon = match e.engine.as_str() {
                "kokoro" => "sparkles",
                "pocket" => "mic",
                _ => "cpu",
            };
            let size_text = format!("{} / {} MB RAM", mb(e.disk_bytes), e.ram_mb);
            json!({
                "id": e.id,
                "name": e.name,
                "enabled": true,
                "can": {
                    "voice_sample": !e.sample_url.is_empty(),
                    "voice_download": !installed && !busy,
                    "voice_catalog_remove": installed,
                },
                "confirm": {
                    "voice_download": dl_confirm,
                    "voice_catalog_remove": remove_confirm,
                },
                "cells": [
                    note(icon, e.name.clone()),
                    e.language,
                    e.quality,
                    size_text,
                    license_cell,
                    state_cell,
                ],
                "tip": e.attribution,
            })
        })
        .collect();

    let mut actions = Vec::new();
    if neural_state.running {
        let mut unload = button("voice_unload", "voices.unload", "power", false);
        unload["top"] = true.into();
        actions.push(unload);
    }
    if busy {
        let mut cancel = button("voice_dl_cancel", "voices.cancel_download", "x", false);
        cancel["top"] = true.into();
        actions.push(cancel);
    }
    actions.extend([
        button("voice_sample", "voices.play_sample", "play", true),
        confirm(
            button("voice_download", "voices.download", "download", true),
            tr("voices.download_confirm").into(),
            "voices.download",
        ),
        danger(confirm(
            button("voice_catalog_remove", "btn.delete", "trash-2", true),
            tr("voices.remove_confirm").into(),
            "btn.delete",
        )),
    ]);

    let mut t = super::pages::table(
        "sounds_voice_catalog",
        tr("sounds.show.find_voices"),
        "",
        vec![
            column("voice", "sounds.voice", 235),
            column("language", "voices.language", 80),
            column("quality", "voices.quality", 75),
            column("size", "voices.size", 168),
            column("license", "voices.license", 262),
            column("state", "sounds.state", 85),
        ],
        rows,
        actions,
    );
    t["domain"] = json!(super::sounds::DOMAIN);
    t["intro"] = format!("{} {}", tr("voices.catalog_intro"), engine_summary(&neural_state)).into();
    if let Some(progress) = progress_model(&neural_state) {
        t["progress"] = progress;
    }
    let ui_tag = ui_locale.replace('_', "-");
    t["filter"] = json!({
        "mode": "toggles",
        "options": [
            {
                "id": "commercial",
                "label": tr("voices.filter_commercial"),
                "tip": tr("voices.filter_commercial_tip"),
                "icon": "shield",
                "shown": commercial_only,
            },
            {
                "id": "my_language",
                "label": trf("voices.filter_my_language", &[("language", &ui_tag)]),
                "tip": tr("voices.filter_my_language_tip"),
                "icon": "globe",
                "shown": my_language_only,
            },
            {
                "id": "installed",
                "label": tr("voices.filter_installed"),
                "tip": tr("voices.filter_installed_tip"),
                "icon": "check",
                "shown": installed_only,
            },
        ],
    });
    t["search"] = true.into();
    t["open"] = "voice_sample".into();
    t
}

pub fn act(engine: &Rc<Engine>, action: &str, id: &str) -> Option<Result<Value, String>> {
    Some(match action {
        "voices_refresh" => listed(engine, true).map(|v| json!({"accepted": true, "count": v.len()})),
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
        "voice_unload" => {
            tokio::task::spawn_local(async {
                neural::unload().await;
            });
            Ok(json!({"accepted": true, "message": tr("voices.unloaded")}))
        }
        "voice_dl_cancel" => {
            let cancelled = neural::cancel_download();
            Ok(json!({"accepted": true, "cancelled": cancelled}))
        }
        "voice_engine_remove" => {
            tokio::task::spawn_local(async {
                if let Err(e) = neural::remove_engine().await {
                    crate::warn(format!("Remove neural voice engine: {e}"));
                }
            });
            Ok(json!({"accepted": true}))
        }
        "voice_download" => {
            if neural::catalog_entry(id).is_none() {
                return Some(Err(format!("unknown voice {id}")));
            }
            if neural::active_download().is_some() {
                return Some(Err(tr("voices.download_busy").into()));
            }
            let model = id.to_string();
            tokio::task::spawn_local(async move {
                if let Err(e) = neural::install_model(&model).await {
                    if e != "cancelled" {
                        crate::warn(format!("Download voice {model}: {e}"));
                    }
                }
            });
            Ok(json!({"accepted": true, "message": tr("voices.downloading")}))
        }
        "voice_sample" => {
            let id = id.to_string();
            let entry = neural::catalog_entry(&id);
            if entry.is_none_or(|e| e.sample_url.is_empty()) {
                return Some(Err(tr("voices.no_sample").into()));
            }
            tokio::task::spawn_local(async move {
                if let Err(e) = neural::play_sample(&id).await {
                    crate::warn(format!("Voice sample: {e}"));
                }
            });
            Ok(json!({"accepted": true, "message": tr("voices.playing_sample")}))
        }
        "voice_remove" => {
            if id.starts_with("custom:") {
                engine.remove_custom_voice(id).map(|_| json!({"accepted": true}))
            } else if id.starts_with("piper:") {
                let model = id.to_string();
                let eng = engine.clone();
                tokio::task::spawn_local(async move {
                    if let Err(e) = neural::remove_model(&model).await {
                        crate::warn(format!("Remove voice: {e}"));
                    } else if eng.config().voices.voice == model {
                        eng.change(|c| c.voices.voice.clear());
                    }
                });
                Ok(json!({"accepted": true}))
            } else if id.starts_with("kokoro:") {
                let eng = engine.clone();
                tokio::task::spawn_local(async move {
                    if let Err(e) = neural::remove_model("kokoro:multi-lang-v1").await {
                        crate::warn(format!("Remove Kokoro: {e}"));
                    } else if eng.config().voices.voice.starts_with("kokoro:") {
                        eng.change(|c| c.voices.voice.clear());
                    }
                });
                Ok(json!({"accepted": true}))
            } else {
                Err(tr("voices.cannot_delete_system").into())
            }
        }
        "voice_catalog_remove" => {
            let model = id.to_string();
            if !neural::is_model_installed(&model) {
                return Some(Err(tr("voices.not_installed").into()));
            }
            tokio::task::spawn_local(async move {
                if let Err(e) = neural::remove_model(&model).await {
                    crate::warn(format!("Remove voice: {e}"));
                }
            });
            Ok(json!({"accepted": true}))
        }
        _ => return None,
    })
}

/// Speaks a short preview of an unsaved Mix, Recording or Effect form directly on the monitor device.
pub fn preview(engine: &Rc<Engine>, form_kind: &str, value: &Value) -> Result<Value, String> {
    let item = value.as_object().cloned().unwrap_or_default();
    let existing = form_kind
        .strip_prefix("voice_edit:")
        .and_then(|id| engine.config().voices.custom.iter().find(|c| c.id == id).cloned());
    let kind = if let Some(c) = &existing {
        match c.kind {
            crate::voice::CustomKind::Mix => "mix",
            crate::voice::CustomKind::Clone => "clone",
            crate::voice::CustomKind::Effect => "effect",
        }
    } else if form_kind.starts_with("voice_mix:") {
        "mix"
    } else if form_kind.starts_with("voice_clone:") {
        "clone"
    } else if form_kind.starts_with("voice_effect:") {
        "effect"
    } else {
        return Err(format!("cannot preview {form_kind}"));
    };

    let mut cleanup_preview_rec: Option<std::path::PathBuf> = None;
    let mut custom = match kind {
        "mix" => build_mix(&item, Some("custom:__preview__"))?,
        "clone" => {
            if item.get("consent").and_then(Value::as_bool) != Some(true) {
                return Err(tr("voices.needs_consent").into());
            }
            let path = text_field(&item, "recording");
            if path.is_empty() {
                return Err(tr("voices.needs_recording").into());
            }
            let existing_file = existing.as_ref().map(|c| c.recording.as_str());
            let filename = if existing_file == Some(path.as_str()) && neural::recordings_dir().join(&path).is_file() {
                path
            } else {
                let preview = crate::engine::uuid();
                let (file, _) = neural::import_recording(std::path::Path::new(&path), &preview)?;
                cleanup_preview_rec = Some(neural::recordings_dir().join(&file));
                file
            };
            crate::voice::CustomVoice {
                id: "custom:__preview__".into(),
                name: "Preview".into(),
                kind: crate::voice::CustomKind::Clone,
                language: "en-US".into(),
                mix: Vec::new(),
                kokoro_lang: String::new(),
                recording: filename,
                consent: true,
                base_voice: String::new(),
                speed: number(&item, "speed", 1.0).clamp(0.5, 3.0),
                pitch: number(&item, "pitch", 1.0).clamp(0.5, 1.5),
                effect: pick_effect(&item),
            }
        }
        _ => build_effect(engine, &item, Some("custom:__preview__"))?,
    };
    custom.id = "custom:__preview__".into();

    let mut cfg = engine.config().voices.clone();
    let volume = cfg.volume;
    cfg.custom.retain(|c| c.id != custom.id);
    cfg.custom.push(custom);
    let sample = tr("voices.sample").to_string();
    let speed = cfg.speed;
    let pitch = cfg.pitch;

    tokio::task::spawn_local(async move {
        let spoken = crate::voice::speak_with(&sample, "custom:__preview__", speed, pitch, &cfg).await;
        if let Some(temp_rec) = cleanup_preview_rec {
            let _ = std::fs::remove_file(temp_rec);
        }
        match spoken {
            Ok(wav) => match crate::voice::to_file(&wav) {
                Ok(file) => {
                    if let Ok(started) = crate::sound::play(&file, "speech", "Monitor", crate::sound::volume(volume)) {
                        let _ = started.ended.await;
                    }
                    let _ = std::fs::remove_file(&file);
                }
                Err(e) => crate::warn(format!("Voice preview: {e}")),
            },
            Err(e) => crate::warn(format!("Voice preview: {e}")),
        }
    });
    Ok(json!({"accepted": true, "message": tr("voices.previewing")}))
}

const SAY_FIELDS: &[F] = &[
    ("text", "voices.text", W::Multi),
    ("monitor", "voices.on_monitor", W::Bool),
];
const HOW_FIELDS: &[F] = &[
    ("speed", "voices.speed", W::Num),
    ("pitch", "voices.pitch", W::Num),
    ("volume", "sounds.volume", W::Int),
    ("keepLoaded", "voices.keep_loaded", W::Bool),
];
const MIX_FIELDS: &[F] = &[
    ("name", "voices.custom_name", W::Text),
    ("voice1", "voices.mix_voice_1", W::Suggest("kokoro.speakers")),
    ("share1", "voices.mix_share_1", W::Int),
    ("voice2", "voices.mix_voice_2", W::Suggest("kokoro.speakers")),
    ("share2", "voices.mix_share_2", W::Int),
    ("voice3", "voices.mix_voice_3", W::Suggest("kokoro.speakers")),
    ("share3", "voices.mix_share_3", W::Int),
    ("voice4", "voices.mix_voice_4", W::Suggest("kokoro.speakers")),
    ("share4", "voices.mix_share_4", W::Int),
    ("language", "voices.language", W::Text),
    ("speed", "voices.speed", W::Num),
    ("pitch", "voices.pitch", W::Num),
    (
        "effect",
        "voices.effect",
        W::Pick(&[
            "voices.effect_none",
            "voices.effect_echo",
            "voices.effect_radio",
            "voices.effect_robot",
        ]),
    ),
    ("preview", "voices.preview", W::Text),
];
const CLONE_FIELDS: &[F] = &[
    ("name", "voices.custom_name", W::Text),
    ("recording", "voices.recording_file", W::Text),
    ("record", "voices.record_mic", W::Text),
    ("consent", "voices.recording_consent", W::Bool),
    ("speed", "voices.speed", W::Num),
    ("pitch", "voices.pitch", W::Num),
    (
        "effect",
        "voices.effect",
        W::Pick(&[
            "voices.effect_none",
            "voices.effect_echo",
            "voices.effect_radio",
            "voices.effect_robot",
        ]),
    ),
    ("preview", "voices.preview", W::Text),
];
const EFFECT_FIELDS: &[F] = &[
    ("name", "voices.custom_name", W::Text),
    ("baseVoice", "voices.base_voice", W::Suggest("voices.base")),
    (
        "effect",
        "voices.effect",
        W::Pick(&[
            "voices.effect_none",
            "voices.effect_echo",
            "voices.effect_radio",
            "voices.effect_robot",
        ]),
    ),
    ("speed", "voices.speed", W::Num),
    ("pitch", "voices.pitch", W::Num),
    ("preview", "voices.preview", W::Text),
];

pub fn external_fields(engine: &Rc<Engine>, kind: &str) -> Option<&'static [F]> {
    if kind.starts_with("voice_say:") {
        Some(SAY_FIELDS)
    } else if kind.starts_with("voice_settings:") {
        Some(HOW_FIELDS)
    } else if kind.starts_with("voice_mix:") {
        Some(MIX_FIELDS)
    } else if kind.starts_with("voice_clone:") {
        Some(CLONE_FIELDS)
    } else if kind.starts_with("voice_effect:") {
        Some(EFFECT_FIELDS)
    } else if let Some(id) = kind.strip_prefix("voice_edit:") {
        engine
            .config()
            .voices
            .custom
            .iter()
            .find(|c| c.id == id)
            .map(|c| match c.kind {
                crate::voice::CustomKind::Mix => MIX_FIELDS,
                crate::voice::CustomKind::Clone => CLONE_FIELDS,
                crate::voice::CustomKind::Effect => EFFECT_FIELDS,
            })
    } else {
        None
    }
}

fn speaker_id(sid: i32) -> String {
    neural::kokoro_speakers()
        .iter()
        .find(|s| s.sid == sid)
        .map(|s| s.id.clone())
        .unwrap_or_default()
}

pub fn external_form(engine: &Rc<Engine>, kind: &str) -> Option<Result<Value, String>> {
    let (fields, title, item, ok_label) = if let Some(voice) = kind.strip_prefix("voice_say:") {
        let shown = listed(engine, false)
            .ok()
            .and_then(|all| all.into_iter().find(|v| v.id == voice))
            .map_or_else(|| voice.to_string(), |v| v.name);
        (
            SAY_FIELDS,
            trf("voices.say_title", &[("voice", &shown)]),
            json!({"id": voice, "text": tr("voices.sample"), "monitor": true}),
            tr("voices.speak"),
        )
    } else if kind.starts_with("voice_settings:") {
        let s = engine.config().voices.clone();
        (
            HOW_FIELDS,
            tr("voices.how").to_string(),
            json!({"id": "", "speed": s.speed, "pitch": s.pitch, "volume": s.volume, "keepLoaded": s.keep_loaded}),
            tr("btn.save"),
        )
    } else if kind.starts_with("voice_mix:") {
        if !neural::is_model_installed("kokoro:multi-lang-v1") {
            return Some(Err(tr("voices.needs_kokoro").into()));
        }
        (
            MIX_FIELDS,
            tr("voices.make_mix").to_string(),
            json!({
                "id": "",
                "name": "",
                "voice1": "af_heart",
                "share1": 50,
                "voice2": "am_adam",
                "share2": 50,
                "voice3": "",
                "share3": 0,
                "voice4": "",
                "share4": 0,
                "language": "en-US",
                "speed": 1.0,
                "pitch": 1.0,
                "effect": 0,
            }),
            tr("btn.save"),
        )
    } else if kind.starts_with("voice_clone:") {
        if !neural::is_model_installed("pocket:en-v1") {
            return Some(Err(tr("voices.needs_pocket").into()));
        }
        (
            CLONE_FIELDS,
            tr("voices.make_clone").to_string(),
            json!({
                "id": "",
                "name": "",
                "recording": "",
                "consent": false,
                "speed": 1.0,
                "pitch": 1.0,
                "effect": 0,
            }),
            tr("btn.save"),
        )
    } else if kind.starts_with("voice_effect:") {
        let first_base = listed(engine, false)
            .ok()
            .and_then(|all| all.into_iter().find(|v| v.engine != "custom"))
            .map(|v| v.id)
            .unwrap_or_default();
        (
            EFFECT_FIELDS,
            tr("voices.make_effect").to_string(),
            json!({
                "id": "",
                "name": "",
                "baseVoice": first_base,
                "effect": 1,
                "speed": 1.0,
                "pitch": 1.0,
            }),
            tr("btn.save"),
        )
    } else {
        let id = kind.strip_prefix("voice_edit:")?;
        let Some(c) = engine.config().voices.custom.iter().find(|c| c.id == id).cloned() else {
            return Some(Err(format!("unknown custom voice {id}")));
        };
        let title = trf("voices.edit_custom", &[("name", &c.name)]);
        match c.kind {
            crate::voice::CustomKind::Mix => {
                let slot = |i: usize| {
                    c.mix
                        .get(i)
                        .map(|(sid, w)| (speaker_id(*sid), w.round().clamp(0.0, 100.0) as i64))
                        .unwrap_or_else(|| (String::new(), 0))
                };
                let (v1, s1) = slot(0);
                let (v2, s2) = slot(1);
                let (v3, s3) = slot(2);
                let (v4, s4) = slot(3);
                (
                    MIX_FIELDS,
                    title,
                    json!({
                        "id": c.id,
                        "name": c.name,
                        "voice1": v1,
                        "share1": s1,
                        "voice2": v2,
                        "share2": s2,
                        "voice3": v3,
                        "share3": s3,
                        "voice4": v4,
                        "share4": s4,
                        "language": c.language,
                        "speed": c.speed,
                        "pitch": c.pitch,
                        "effect": effect_index(&c.effect),
                    }),
                    tr("btn.save"),
                )
            }
            crate::voice::CustomKind::Clone => (
                CLONE_FIELDS,
                title,
                json!({
                    "id": c.id,
                    "name": c.name,
                    "recording": c.recording,
                    "consent": c.consent,
                    "speed": c.speed,
                    "pitch": c.pitch,
                    "effect": effect_index(&c.effect),
                }),
                tr("btn.save"),
            ),
            crate::voice::CustomKind::Effect => (
                EFFECT_FIELDS,
                title,
                json!({
                    "id": c.id,
                    "name": c.name,
                    "baseVoice": c.base_voice,
                    "effect": effect_index(&c.effect),
                    "speed": c.speed,
                    "pitch": c.pitch,
                }),
                tr("btn.save"),
            ),
        }
    };

    let object = item.as_object().cloned().unwrap_or_default();
    let kokoro_choices: Vec<Value> = neural::kokoro_speakers()
        .iter()
        .map(|s| json!({"id": s.id, "label": format!("{} ({})", s.name, s.language)}))
        .collect();
    let base_choices: Vec<Value> = listed(engine, false)
        .unwrap_or_default()
        .into_iter()
        .filter(|v| v.engine != "custom")
        .map(|v| json!({"id": v.id, "label": format!("{} ({})", v.name, v.language)}))
        .collect();

    let fields: Vec<Value> = fields
        .iter()
        .map(|(key, label, widget)| {
            let mut field = json!({
                "key": key,
                "label": crate::automation::schema::field_label(label),
                "widget": widget,
                "holds": widget.about(),
                "value": crate::automation::edits::shown(widget, object.get(*key)),
            });
            if let W::Pick(options) = widget {
                field["widget"]["options"] = json!(options
                    .iter()
                    .map(|o| crate::automation::schema::field_label(o))
                    .collect::<Vec<_>>());
                field["value"] = object.get(*key).cloned().unwrap_or(json!(0));
            }
            match *key {
                "speed" => field["widget"] = json!({"kind": "slider", "min": 0.5, "max": 3.0, "step": 0.1}),
                "pitch" => field["widget"] = json!({"kind": "slider", "min": 0.5, "max": 1.5, "step": 0.05}),
                "volume" | "share1" | "share2" | "share3" | "share4" => {
                    field["widget"] = json!({"kind": "slider", "min": 0, "max": 100, "step": 1});
                }
                "recording" => {
                    field["browse"] = "file".into();
                    field["tip"] = tr("voices.recording_tip").into();
                }
                "record" => {
                    field["label"] = "".into();
                    field["buttonLabel"] = tr("voices.record_mic").into();
                    field["busyLabel"] = tr("voices.recording_now").into();
                    field["widget"] = json!({"kind": "button", "icon": "mic"});
                    field["press"] = json!({
                        "request": {
                            "request": "StreamerzVoice",
                            "action": "record_mic",
                            "seconds": 6,
                        },
                        "fillField": "recording",
                        "replyKey": "path",
                    });
                }
                "voice1" | "voice2" | "voice3" | "voice4" => {
                    let mut choices = Vec::new();
                    if matches!(*key, "voice3" | "voice4") {
                        choices.push(json!({"id": "", "label": tr("voices.mix_none")}));
                    }
                    choices.extend(kokoro_choices.iter().cloned());
                    field["choices"] = choices.into();
                }
                "baseVoice" => {
                    field["choices"] = base_choices.clone().into();
                }
                "preview" => {
                    field["label"] = "".into();
                    field["buttonLabel"] = tr("voices.preview").into();
                    field["widget"] = json!({"kind": "button", "icon": "play"});
                    field["press"] = json!({
                        "request": {
                            "request": "StreamerzCoreAction",
                            "domain": super::sounds::DOMAIN,
                            "table": "sounds_voices",
                            "action": "voice_preview",
                            "itemId": kind,
                            "id": "voices:preview",
                        },
                        "withValue": true,
                    });
                }
                _ => {}
            }
            if field["widget"]["kind"] == "slider" {
                field["value"] = object.get(*key).cloned().unwrap_or(json!(1));
            }
            field
        })
        .collect();
    Some(Ok(json!({
        "kind": kind,
        "title": title,
        "okLabel": ok_label,
        "json": false,
        "new": item.clone(),
        "fields": fields,
        "item": item,
    })))
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

fn text_field(item: &Obj, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .trim_matches('"')
        .to_string()
}

fn pick_effect(item: &Obj) -> String {
    match item.get("effect") {
        Some(Value::Number(n)) => match n.as_u64().unwrap_or(0) {
            1 => "echo",
            2 => "radio",
            3 => "robot",
            _ => "",
        }
        .into(),
        Some(Value::String(s)) => crate::engine::speech::validate_effect(s).unwrap_or_default(),
        _ => String::new(),
    }
}

fn build_mix(item: &Obj, existing_id: Option<&str>) -> Result<crate::voice::CustomVoice, String> {
    let name = text_field(item, "name");
    if existing_id.is_none() && name.is_empty() {
        return Err(tr("voices.needs_name").into());
    }
    let mut mix = Vec::new();
    for (vk, sk) in [
        ("voice1", "share1"),
        ("voice2", "share2"),
        ("voice3", "share3"),
        ("voice4", "share4"),
    ] {
        let v = text_field(item, vk);
        let share = number(item, sk, 0.0);
        if v.is_empty() || share <= 0.0 {
            continue;
        }
        let Some(sp) = neural::kokoro_speakers()
            .iter()
            .find(|s| s.id.eq_ignore_ascii_case(&v) || s.name.eq_ignore_ascii_case(&v))
        else {
            return Err(format!("unknown Kokoro speaker {v}"));
        };
        mix.push((sp.sid, share));
    }
    if mix.len() < 2 {
        return Err(tr("voices.mix_needs_two").into());
    }
    let lang = {
        let l = text_field(item, "language");
        if l.is_empty() {
            "en-US".into()
        } else {
            l
        }
    };
    let id = existing_id
        .map(str::to_owned)
        .unwrap_or_else(|| crate::engine::speech::custom_voice_id(&name, ""));
    Ok(crate::voice::CustomVoice {
        id,
        name: if name.is_empty() { "Preview".into() } else { name },
        kind: crate::voice::CustomKind::Mix,
        language: lang.clone(),
        mix,
        kokoro_lang: crate::engine::speech::kokoro_lang_for(&lang),
        recording: String::new(),
        consent: false,
        base_voice: String::new(),
        speed: number(item, "speed", 1.0).clamp(0.5, 3.0),
        pitch: number(item, "pitch", 1.0).clamp(0.5, 1.5),
        effect: pick_effect(item),
    })
}

fn build_effect(
    engine: &Rc<Engine>,
    item: &Obj,
    existing_id: Option<&str>,
) -> Result<crate::voice::CustomVoice, String> {
    let name = text_field(item, "name");
    if existing_id.is_none() && name.is_empty() {
        return Err(tr("voices.needs_name").into());
    }
    let base_voice = text_field(item, "baseVoice");
    if base_voice.is_empty() {
        return Err(tr("voices.needs_base").into());
    }
    let lang = listed(engine, false)
        .ok()
        .and_then(|all| all.into_iter().find(|v| v.id == base_voice))
        .map(|v| v.language)
        .unwrap_or_default();
    let id = existing_id
        .map(str::to_owned)
        .unwrap_or_else(|| crate::engine::speech::custom_voice_id(&name, ""));
    Ok(crate::voice::CustomVoice {
        id,
        name: if name.is_empty() { "Preview".into() } else { name },
        kind: crate::voice::CustomKind::Effect,
        language: lang,
        mix: Vec::new(),
        kokoro_lang: String::new(),
        recording: String::new(),
        consent: false,
        base_voice,
        speed: number(item, "speed", 1.0).clamp(0.5, 3.0),
        pitch: number(item, "pitch", 1.0).clamp(0.5, 1.5),
        effect: pick_effect(item),
    })
}

fn save_clone(engine: &Rc<Engine>, item: &Obj, existing: Option<&crate::voice::CustomVoice>) -> Result<Value, String> {
    let name = text_field(item, "name");
    if name.is_empty() {
        return Err(tr("voices.needs_name").into());
    }
    if item.get("consent").and_then(Value::as_bool) != Some(true) {
        return Err(tr("voices.needs_consent").into());
    }
    let path = text_field(item, "recording");
    if path.is_empty() {
        return Err(tr("voices.needs_recording").into());
    }
    let id = existing
        .map(|c| c.id.clone())
        .unwrap_or_else(|| crate::engine::speech::custom_voice_id(&name, ""));
    let filename = if existing.is_some_and(|c| c.recording == path) && neural::recordings_dir().join(&path).is_file() {
        path
    } else {
        let (file, _) = neural::import_recording(std::path::Path::new(&path), &id)?;
        file
    };
    let custom = crate::voice::CustomVoice {
        id: id.clone(),
        name,
        kind: crate::voice::CustomKind::Clone,
        language: "en-US".into(),
        mix: Vec::new(),
        kokoro_lang: String::new(),
        recording: filename,
        consent: true,
        base_voice: String::new(),
        speed: number(item, "speed", 1.0).clamp(0.5, 3.0),
        pitch: number(item, "pitch", 1.0).clamp(0.5, 1.5),
        effect: pick_effect(item),
    };
    engine.save_custom_voice(custom);
    Ok(json!({"saved": id}))
}

pub fn save_external(engine: &Rc<Engine>, kind: &str, item: Obj) -> Option<Result<Value, String>> {
    if let Some(voice) = kind.strip_prefix("voice_say:") {
        let text = text_field(&item, "text");
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
        let keep_loaded = item.get("keepLoaded").and_then(Value::as_bool).unwrap_or(true);
        engine.change(|c| {
            c.voices.speed = speed;
            c.voices.pitch = pitch;
            c.voices.volume = volume;
            c.voices.keep_loaded = keep_loaded;
        });
        if !keep_loaded {
            tokio::task::spawn_local(async {
                neural::maybe_unload_idle(false).await;
            });
        }
        return Some(Ok(json!({"saved": "voices"})));
    }
    if kind.starts_with("voice_mix:") {
        return Some(build_mix(&item, None).map(|custom| {
            let saved = custom.id.clone();
            engine.save_custom_voice(custom);
            json!({"saved": saved})
        }));
    }
    if kind.starts_with("voice_clone:") {
        return Some(save_clone(engine, &item, None));
    }
    if kind.starts_with("voice_effect:") {
        return Some(build_effect(engine, &item, None).map(|custom| {
            let saved = custom.id.clone();
            engine.save_custom_voice(custom);
            json!({"saved": saved})
        }));
    }
    if let Some(id) = kind.strip_prefix("voice_edit:") {
        let Some(existing) = engine.config().voices.custom.iter().find(|c| c.id == id).cloned() else {
            return Some(Err(format!("unknown custom voice {id}")));
        };
        return Some(match existing.kind {
            crate::voice::CustomKind::Mix => {
                if text_field(&item, "name").is_empty() {
                    return Some(Err(tr("voices.needs_name").into()));
                }
                build_mix(&item, Some(&existing.id)).map(|custom| {
                    let saved = custom.id.clone();
                    engine.save_custom_voice(custom);
                    json!({"saved": saved})
                })
            }
            crate::voice::CustomKind::Clone => save_clone(engine, &item, Some(&existing)),
            crate::voice::CustomKind::Effect => {
                if text_field(&item, "name").is_empty() {
                    return Some(Err(tr("voices.needs_name").into()));
                }
                build_effect(engine, &item, Some(&existing.id)).map(|custom| {
                    let saved = custom.id.clone();
                    engine.save_custom_voice(custom);
                    json!({"saved": saved})
                })
            }
        });
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
            // No espeak-ng: the table says how to install it, and Mix/Clone/Effect stay off with tips.
            crate::voice::TEST_PROGRAM.with_borrow_mut(|p| *p = Some("/nowhere/espeak-ng".into()));
            system_listed(true).ok();
            let t = table(&e);
            assert!(t["empty"].as_str().unwrap().contains("espeak-ng"), "{}", t["empty"]);
            let actions = t["actions"].as_array().unwrap();
            for id in ["voice_mix", "voice_clone", "voice_effect"] {
                let btn = actions.iter().find(|a| a["id"] == id).unwrap();
                assert_eq!(
                    btn["enabled"], false,
                    "{id} should be disabled until prerequisites exist"
                );
                assert!(btn["tip"].as_str().is_some_and(|s| !s.is_empty()), "{id} needs a tip");
            }
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
            assert_eq!(rows[1]["cells"][4]["text"], "Default", "{}", rows[1]);
            // System voices cannot be edited or deleted.
            assert_eq!(rows[0]["can"]["voice_edit"], false);
            assert_eq!(rows[0]["can"]["voice_remove"], false);
            // Voice effect becomes enabled once a base voice is available.
            let fx_btn = t["actions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == "voice_effect")
                .unwrap();
            assert_eq!(fx_btn["enabled"], true);

            // Find voices catalog lists the bundled neural voices with per-row confirm text and can flags.
            let cat = catalog_table(&e);
            let cat_rows = cat["rows"].as_array().unwrap();
            assert!(cat_rows.len() >= 20);
            let lessac = cat_rows.iter().find(|r| r["id"] == "piper:en_US-lessac").unwrap();
            assert_eq!(lessac["can"]["voice_sample"], true);
            assert_eq!(lessac["can"]["voice_catalog_remove"], false);
            let dl_q = lessac["confirm"]["voice_download"].as_str().unwrap();
            assert!(
                dl_q.contains("Blizzard 2013") && dl_q.contains("Non-commercial"),
                "{dl_q}"
            );
            let pocket = cat_rows.iter().find(|r| r["id"] == "pocket:en-v1").unwrap();
            assert_eq!(pocket["can"]["voice_sample"], false);
            let kokoro = cat_rows.iter().find(|r| r["id"] == "kokoro:multi-lang-v1").unwrap();
            assert!(kokoro["confirm"]["voice_catalog_remove"]
                .as_str()
                .unwrap()
                .contains("54"));

            // Find voices filter toggles narrow by license, UI language, and installation state.
            let commercial = catalog_table_with(&e, &json!({"sounds_voice_catalog": {"hidden": ["commercial"]}}));
            let comm_rows = commercial["rows"].as_array().unwrap();
            assert!(comm_rows.len() < cat_rows.len());
            assert!(!comm_rows.iter().any(|r| r["id"] == "piper:en_US-lessac"));
            assert!(!comm_rows.iter().any(|r| r["id"] == "piper:en_US-ryan"));
            assert_eq!(commercial["filter"]["mode"], "toggles");
            assert_eq!(commercial["filter"]["options"][0]["shown"], true);

            let my_lang = catalog_table_with(&e, &json!({"sounds_voice_catalog": {"hidden": ["my_language"]}}));
            let lang_rows = my_lang["rows"].as_array().unwrap();
            assert!(lang_rows.len() < cat_rows.len());
            assert!(lang_rows.iter().all(|r| {
                let l = r["cells"][1].as_str().unwrap_or_default().to_ascii_lowercase();
                l == "multi" || l.starts_with("en")
            }));

            let installed_only = catalog_table_with(&e, &json!({"sounds_voice_catalog": {"hidden": ["installed"]}}));
            assert!(installed_only["rows"].as_array().unwrap().len() <= cat_rows.len());

            // Live progress bar payload mirrors active download state and stays absent when idle.
            let idle = neural::EngineState {
                installed: false,
                version: String::new(),
                running: false,
                loaded: None,
                memory_bytes: 0,
                disk_bytes: 0,
                kokoro_installed: false,
                pocket_installed: false,
                download: None,
            };
            assert!(progress_model(&idle).is_none());
            let active = neural::EngineState {
                download: Some(neural::DownloadStatus {
                    id: "piper:en_US-amy".into(),
                    label: "Amy".into(),
                    phase: "downloading".into(),
                    downloaded: 10 << 20,
                    total: 20 << 20,
                    percent: 50,
                }),
                ..idle
            };
            let bar = progress_model(&active).unwrap();
            assert_eq!(bar["percent"], 50);
            assert!(bar["label"].as_str().unwrap().contains("Amy"));

            // In-dialog mic recording button fills the recording field via async StreamerzVoice.
            let tmp_voice_dir = dir.join("voices");
            *neural::TEST_DIR.lock().unwrap() = Some(tmp_voice_dir);
            let pocket_dir = neural::model_dir("pocket:en-v1");
            std::fs::create_dir_all(&pocket_dir).unwrap();
            std::fs::write(pocket_dir.join("flow_lm_main.int8.onnx"), b"ok").unwrap();
            let clone_form = external_form(&e, "voice_clone:").unwrap().unwrap();
            let rec_btn = clone_form["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["key"] == "record")
                .unwrap();
            assert_eq!(rec_btn["widget"]["kind"], "button");
            assert_eq!(rec_btn["press"]["fillField"], "recording");
            assert_eq!(rec_btn["press"]["replyKey"], "path");
            assert_eq!(rec_btn["press"]["request"]["request"], "StreamerzVoice");
            assert_eq!(rec_btn["press"]["request"]["action"], "record_mic");
            *neural::TEST_DIR.lock().unwrap() = None;

            // The forms' fields are known where a save is read (a save of an unknown kind is refused).
            for kind in [
                "voice_say:pt-BR",
                "voice_settings:",
                "voice_mix:",
                "voice_clone:",
                "voice_effect:",
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
            item.insert("keepLoaded".into(), json!(false));
            save_external(&e, "voice_settings:", item).unwrap().unwrap();
            assert_eq!(e.config().voices.speed, 3.0, "kept within its ends");
            assert!(!e.config().voices.keep_loaded);

            // Creating an effect voice adds it to the installed voices table, can be edited in place,
            // previewed unsaved, and removed cleanly.
            let mut fx = Obj::new();
            fx.insert("name".into(), json!("Radio Bot"));
            fx.insert("baseVoice".into(), json!("pt-BR"));
            fx.insert("effect".into(), json!(2));
            let saved = save_external(&e, "voice_effect:", fx).unwrap().unwrap();
            let cid = saved["saved"].as_str().unwrap().to_string();
            assert!(cid.starts_with("custom:"));
            let t = table(&e);
            let custom_row = t["rows"].as_array().unwrap().iter().find(|r| r["id"] == cid).unwrap();
            assert_eq!(custom_row["can"]["voice_edit"], true);
            assert_eq!(custom_row["can"]["voice_remove"], true);

            let edit_kind = format!("voice_edit:{cid}");
            assert!(super::super::pages::external_fields(&e, &edit_kind).is_some());
            let edit_form = external_form(&e, &edit_kind).unwrap().unwrap();
            assert_eq!(edit_form["title"], "Edit Radio Bot");
            assert!(edit_form["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["key"] == "preview" && f["widget"]["kind"] == "button"));
            let mut edited = Obj::new();
            edited.insert("name".into(), json!("Robot Bot"));
            edited.insert("baseVoice".into(), json!("en-US"));
            edited.insert("effect".into(), json!(3));
            edited.insert("pitch".into(), json!(0.8));
            let resaved = save_external(&e, &edit_kind, edited).unwrap().unwrap();
            assert_eq!(resaved["saved"], cid, "editing preserves custom voice id");
            let updated = e.config().voices.custom.iter().find(|c| c.id == cid).unwrap().clone();
            assert_eq!(updated.name, "Robot Bot");
            assert_eq!(updated.effect, "robot");
            assert_eq!(updated.base_voice, "en-US");
            assert_eq!(
                preview(&e, "voice_clone:", &json!({"recording": "a.wav"})).unwrap_err(),
                tr("voices.needs_consent")
            );
            // Unsaved preview validates inputs cleanly.
            assert!(preview(&e, "voice_effect:", &json!({"baseVoice": ""})).is_err());
            assert!(preview(&e, "voice_effect:", &json!({"baseVoice": "en-US", "effect": 3})).is_ok());

            act(&e, "voice_remove", &cid).unwrap().unwrap();
            assert!(!table(&e)["rows"].as_array().unwrap().iter().any(|r| r["id"] == cid));
            let mut empty = Obj::new();
            empty.insert("text".into(), json!("  "));
            assert!(save_external(&e, "voice_say:pt-BR", empty).unwrap().is_err());
            crate::voice::TEST_PROGRAM.with_borrow_mut(|p| *p = None);
            system_listed(true).ok();
            let _ = std::fs::remove_dir_all(&dir);
        });
    }
}
