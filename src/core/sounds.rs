//! The Sounds page: what plays now (Playing), every sound Streamerz knows (Library), pads one press
//! away (Board, also a dock for OBS), the voices (Voices) and where sounds go (Output). Shown one at a
//! time with the page's Show selector.
use super::pages::{button, button_if, confirm, danger};
use super::view::{column, note};
use crate::automation::schema::{F, W};
use crate::engine::Engine;
use crate::i18n::{tr, trf};
use crate::model::Obj;
use crate::sound;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const DOMAIN: &str = "sounds";

pub fn supports(domain: &str) -> bool {
    domain == DOMAIN
}

/// The page was opened (or another one was): sounds that start and end say so only while it shows.
pub fn opened(domain: &str) {
    sound::watch(domain == DOMAIN);
}

fn m_ss(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The folder beside the settings where sounds of one's own are kept.
pub fn folder() -> PathBuf {
    crate::config_path().with_file_name("sounds")
}

/// A sound Streamerz knows: its file, where it comes from and what uses it.
#[derive(Debug, Clone, PartialEq)]
pub struct Known {
    pub path: PathBuf,
    pub from: String,
    pub used_by: Vec<String>,
}

fn is_sound(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| sound::EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Every sound file Streamerz knows: those the actions play, those of the overlays, and those of the
/// sounds folder. Each once, with everything that uses it.
pub fn library(config: &crate::Config) -> Vec<Known> {
    let mut out: Vec<Known> = Vec::new();
    let mut add = |path: PathBuf, from: String, user: Option<String>| match out.iter_mut().find(|k| k.path == path) {
        Some(k) => {
            if let Some(u) = user.filter(|u| !k.used_by.contains(u)) {
                k.used_by.push(u);
            }
        }
        None => out.push(Known {
            path,
            from,
            used_by: user.into_iter().collect(),
        }),
    };
    for action in &config.actions.actions {
        let mut found = Vec::new();
        crate::edit::walk(&action.sub_actions, &mut |o| {
            let kind = o.get("type").and_then(Value::as_u64).unwrap_or(0);
            let text = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or_default().trim().to_string();
            if kind == 1 && !text("soundFile").is_empty() && !text("soundFile").contains('%') {
                found.push(sound::resolve(&text("soundFile")));
            }
        });
        for path in found {
            add(path, tr("sounds.from_actions").into(), Some(action.name.clone()));
        }
    }
    for (group, file) in crate::groups::all(config).iter().flat_map(|g| {
        crate::groups::meta(config, g)
            .files
            .into_iter()
            .filter(|f| crate::groups::accepts_file("sound", &f.name))
            .map(move |f| (g.clone(), f.name))
    }) {
        if let Some(path) = crate::groups::file_path(config, &group, &file) {
            add(
                path,
                trf("sounds.from_overlay", &[("overlay", &group)]),
                Some(group.clone()),
            );
        }
    }
    if let Ok(dir) = std::fs::read_dir(folder()) {
        let mut files: Vec<PathBuf> = dir.flatten().map(|e| e.path()).filter(|p| is_sound(p)).collect();
        files.sort();
        for path in files {
            add(path, tr("sounds.from_folder").into(), None);
        }
    }
    for pad in &config.sounds.board {
        let path = sound::resolve(&pad.file);
        if let Some(k) = out.iter_mut().find(|k| k.path == path) {
            let user = trf("sounds.pad_user", &[("pad", &pad.label)]);
            if !k.used_by.contains(&user) {
                k.used_by.push(user);
            }
        }
    }
    out
}

fn device_name(device: &str, outputs: &[(String, String)]) -> String {
    if device.is_empty() {
        return tr("sounds.default").into();
    }
    outputs
        .iter()
        .find(|(id, _)| id == device)
        .map_or_else(|| device.to_string(), |(_, shown)| shown.clone())
}

fn top(mut b: Value) -> Value {
    b["top"] = true.into();
    b
}

fn table(
    kind: &str,
    title: &str,
    intro: &str,
    empty: &str,
    columns: Vec<Value>,
    rows: Vec<Value>,
    actions: Vec<Value>,
) -> Value {
    let mut t = super::pages::table(kind, title, empty, columns, rows, actions);
    t["domain"] = DOMAIN.into();
    t["intro"] = intro.into();
    t
}

pub fn read(engine: &Rc<Engine>) -> Result<Value, String> {
    let config = engine.config();
    let outputs = sound::outputs();
    crate::catalog::set("sound.devices", outputs.clone());
    // Playing: what plays now, on every device.
    let playing = sound::playing();
    let rows: Vec<Value> = playing
        .iter()
        .map(|p| {
            let file = Path::new(&p.file).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            let name = if p.name.is_empty() { file.clone() } else { p.name.clone() };
            let place = match p.duration_ms {
                Some(d) => format!("{} / {}", m_ss(p.position_ms), m_ss(d)),
                None => m_ss(p.position_ms),
            };
            json!({"id": p.id.to_string(), "name": name, "enabled": true,
                "cells": [note("music", name), device_name(&p.device, &outputs), place,
                    format!("{}%", (p.volume * 100.0).round()),
                    if p.paused { note("pause", tr("sounds.paused")) } else { note("play", tr("sounds.playing_now")) }]})
        })
        .collect();
    let mut volume = button("volume", "sounds.volume", "volume-2", true);
    volume["form"] = "sound_volume:".into();
    let playing = table(
        "sounds_playing",
        tr("sounds.show.playing"),
        tr("sounds.playing_intro"),
        tr("sounds.playing_empty"),
        vec![
            column("sound", "sounds.sound", 220),
            column("device", "sounds.device", 160),
            column("place", "sounds.place", 110),
            column("volume", "sounds.volume", 80),
            column("state", "sounds.state", 100),
        ],
        rows,
        vec![
            top(button_if(
                button("stop_all", "sounds.stop_all", "square", false),
                !playing.is_empty(),
            )),
            button("stop", "sounds.stop", "square", true),
            button("pause", "btn.pause_resume", "pause", true),
            volume,
        ],
    );
    // Library: every sound file Streamerz knows.
    let rows: Vec<Value> = library(&config)
        .iter()
        .map(|k| {
            let name = k
                .path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            let there = k.path.is_file();
            let mut row = json!({"id": k.path.display().to_string(), "name": name, "enabled": there,
                "cells": [note("music", name.clone()), k.from, k.used_by.join(", "),
                    if there { Value::from("") } else { note("triangle-alert", tr("sounds.missing")) }]});
            if !there {
                row["tip"] = trf("sounds.missing_tip", &[("path", &k.path.display().to_string())]).into();
            }
            row
        })
        .collect();
    let mut add = top(button("add", "sounds.add", "plus", false));
    add["form"] = "sound_add:".into();
    let library = table(
        "sounds_library",
        tr("sounds.show.library"),
        &trf("sounds.library_intro", &[("folder", &folder().display().to_string())]),
        tr("sounds.library_empty"),
        vec![
            column("sound", "sounds.sound", 220),
            column("from", "sounds.from", 180),
            column("used", "sounds.used_by", 220),
            column("state", "sounds.state", 100),
        ],
        rows,
        vec![
            add,
            top(button("open_folder", "btn.open_folder", "folder-open", false)),
            button("play", "btn.play", "play", true),
            button("stop_all", "sounds.stop", "square", false),
            button("copy_path", "sounds.copy_path", "copy", true),
            button("used_by", "sounds.used_by", "list", true),
            button("new_action", "sounds.new_action", "zap", true),
            button("to_board", "sounds.to_board", "layout-template", true),
        ],
    );
    // Board: pads one press away.
    let rows: Vec<Value> = config
        .sounds
        .board
        .iter()
        .map(|pad| {
            json!({"id": pad.id, "name": pad.label, "enabled": true,
                "cells": [note("music", pad.label.clone()), Path::new(&pad.file).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(),
                    format!("{}%", pad_volume(pad).round()), device_name(&pad.device, &outputs)]})
        })
        .collect();
    let mut new_pad = top(button("pad_new", "sounds.pad_new", "plus", false));
    new_pad["form"] = "sound_pad:".into();
    let mut edit_pad = button("pad_edit", "btn.edit", "pencil", true);
    edit_pad["form"] = "sound_pad:".into();
    let mut board = table(
        "sounds_board",
        tr("sounds.show.board"),
        tr("sounds.board_intro"),
        tr("sounds.board_empty"),
        vec![
            column("pad", "sounds.pad", 200),
            column("file", "sounds.file", 200),
            column("volume", "sounds.volume", 80),
            column("device", "sounds.device", 160),
        ],
        rows,
        vec![
            new_pad,
            top(button("board_dock", "sounds.board_dock", "panel-right-open", false)),
            button("pad_play", "btn.play", "play", true),
            button("stop_all", "sounds.stop", "square", false),
            edit_pad,
            button("pad_action", "sounds.new_action", "zap", true),
            button("pad_up", "btn.move_up", "arrow-up", true),
            danger(confirm(
                button("pad_remove", "btn.delete", "trash-2", true),
                tr("sounds.pad_remove_confirm").into(),
                "btn.delete",
            )),
        ],
    );
    board["open"] = "pad_play".into();
    // Voices: the engines and voices (system voices, then neural ones).
    let voices = super::voices::table(engine);
    // Output: where sounds go.
    let s = &config.sounds;
    let rows = vec![
        json!({"id": "device", "name": tr("sounds.output.device"), "enabled": true,
            "cells": [tr("sounds.output.device"), device_name(&s.device, &outputs)]}),
        json!({"id": "monitor", "name": tr("sounds.output.monitor"), "enabled": true,
            "cells": [tr("sounds.output.monitor"), if s.monitor.is_empty() { tr("sounds.output.monitor_same").into() } else { device_name(&s.monitor, &outputs) }]}),
        json!({"id": "master", "name": tr("sounds.output.master"), "enabled": true,
            "cells": [tr("sounds.output.master"), format!("{}%", s.master.round())]}),
        json!({"id": "max", "name": tr("sounds.output.max"), "enabled": true,
            "cells": [tr("sounds.output.max"), s.max_at_once.to_string()]}),
    ];
    let mut edit = top(button("output_edit", "btn.edit", "pencil", false));
    edit["form"] = "sounds_output:".into();
    let output = table(
        "sounds_output",
        tr("sounds.show.output"),
        tr("sounds.output_intro"),
        "",
        vec![
            column("what", "sounds.output.what", 220),
            column("value", "sounds.output.value", 260),
        ],
        rows,
        vec![
            edit,
            top(button("test_default", "sounds.test_default", "volume-2", false)),
            top(button("test_monitor", "sounds.test_monitor", "headphones", false)),
        ],
    );
    let tables = vec![playing, library, board, voices, output];
    let options: Vec<Value> = tables
        .iter()
        .map(|t| json!({"id": t["kind"], "label": t["title"]}))
        .collect();
    Ok(json!({"domain": DOMAIN, "tables": tables,
        "tabs": {"label": tr("lists.community.show"), "selected": "sounds_playing", "options": options}}))
}

fn pad_volume(pad: &sound::Pad) -> f64 {
    if pad.volume <= 0.0 {
        100.0
    } else {
        pad.volume.min(100.0)
    }
}

/// A short sound for Play a test sound, written once to the cache.
fn test_sound() -> Result<PathBuf, String> {
    const CHIME: &[u8] = include_bytes!("../../assets/examples/alert-box/files/sounds/chime.wav");
    let path = std::env::temp_dir().join("streamerz-test-sound.wav");
    if std::fs::metadata(&path).map(|m| m.len()).ok() != Some(CHIME.len() as u64) {
        std::fs::write(&path, CHIME).map_err(|e| e.to_string())?;
    }
    Ok(path)
}

fn pad<'a>(config: &'a crate::Config, id: &str) -> Result<&'a sound::Pad, String> {
    config
        .sounds
        .board
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| tr("sounds.pad_gone").to_string())
}

/// Plays a pad of the Board (from the page, its dock or an action).
pub fn play_pad(engine: &Rc<Engine>, id: &str) -> Result<Value, String> {
    let config = engine.config();
    let p = pad(&config, id)?;
    let started = sound::play(
        &sound::resolve(&p.file),
        &format!("pad:{}", p.label),
        &p.device,
        sound::volume(pad_volume(p)),
    )?;
    Ok(json!({"accepted": true, "soundId": started.id}))
}

/// An action that plays this file, made in Actions (for a hotkey, a Stream Deck key, a command).
fn new_action(engine: &Rc<Engine>, name: &str, file: &str, volume: f64, device: &str) -> Result<Value, String> {
    let id = crate::engine::uuid();
    let mut action = crate::model::Action {
        id: id.clone(),
        name: trf("sounds.action_name", &[("name", name)]),
        enabled: true,
        ..Default::default()
    };
    let mut sub = Obj::new();
    sub.insert("id".into(), crate::engine::uuid().into());
    sub.insert("type".into(), 1.into());
    sub.insert("enabled".into(), true.into());
    sub.insert("soundFile".into(), file.into());
    sub.insert("volume".into(), (volume.round() as i64).into());
    sub.insert("device".into(), device.into());
    sub.insert("soundName".into(), name.into());
    action.sub_actions.push(sub);
    let made = action.name.clone();
    engine.change(|c| c.actions.actions.push(action));
    Ok(json!({"accepted": true, "saved": id, "message": trf("sounds.action_made", &[("name", &made)])}))
}

pub fn act(engine: &Rc<Engine>, action: &str, id: &str) -> Result<Value, String> {
    if let Some(done) = super::voices::act(engine, action, id) {
        return done;
    }
    let accepted = Ok(json!({"accepted": true}));
    let file_name = |path: &str| {
        Path::new(path)
            .file_stem()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    match action {
        "stop_all" => {
            sound::stop(None, 0);
            accepted
        }
        "stop" => {
            let id: u64 = id.parse().map_err(|_| tr("sounds.not_playing").to_string())?;
            let named = sound::playing()
                .into_iter()
                .find(|p| p.id == id)
                .ok_or_else(|| tr("sounds.not_playing").to_string())?;
            sound::stop_id(named.id, 0);
            accepted
        }
        "pause" => {
            let id: u64 = id.parse().map_err(|_| tr("sounds.not_playing").to_string())?;
            let now = sound::playing()
                .into_iter()
                .find(|p| p.id == id)
                .ok_or_else(|| tr("sounds.not_playing").to_string())?;
            sound::pause_id(now.id, !now.paused);
            accepted
        }
        "play" => {
            sound::play(Path::new(id), "", "Monitor", 1.0)?;
            accepted
        }
        "copy_path" => Ok(json!({"copy": id, "message": tr("sounds.copied")})),
        "used_by" => {
            let config = engine.config();
            let known = library(&config)
                .into_iter()
                .find(|k| k.path.display().to_string() == id);
            let users = known.map(|k| k.used_by).unwrap_or_default();
            let rows: Vec<Value> = if users.is_empty() {
                vec![json!({"name": tr("sounds.used_by_nothing"), "value": ""})]
            } else {
                users.iter().map(|u| json!({"name": u, "value": ""})).collect()
            };
            Ok(json!({"values": {"title": trf("sounds.used_by_title", &[("name", &file_name(id))]), "rows": rows}}))
        }
        "new_action" => new_action(engine, &file_name(id), id, 100.0, ""),
        "to_board" => {
            let pad = sound::Pad {
                id: crate::engine::uuid(),
                label: file_name(id),
                file: id.to_string(),
                volume: 100.0,
                device: String::new(),
            };
            let label = pad.label.clone();
            engine.change(|c| c.sounds.board.push(pad));
            Ok(json!({"accepted": true, "message": trf("sounds.added_to_board", &[("name", &label)])}))
        }
        "open_folder" => {
            let dir = folder();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            crate::oauth::open_browser(&dir.display().to_string());
            accepted
        }
        "pad_play" => play_pad(engine, id),
        "pad_action" => {
            let p = pad(&engine.config(), id)?.clone();
            new_action(engine, &p.label, &p.file, pad_volume(&p), &p.device)
        }
        "pad_up" => {
            engine.change(|c| {
                if let Some(at) = c.sounds.board.iter().position(|p| p.id == id).filter(|at| *at > 0) {
                    c.sounds.board.swap(at, at - 1);
                }
            });
            accepted
        }
        "pad_remove" => {
            engine.change(|c| c.sounds.board.retain(|p| p.id != id));
            accepted
        }
        "board_dock" => {
            let config = engine.config();
            Ok(json!({"copy": board_url(&config), "message": tr("sounds.board_dock_copied")}))
        }
        "test_default" | "test_monitor" => {
            let device = if action == "test_monitor" { "Monitor" } else { "" };
            sound::play(&test_sound()?, "test", device, 1.0)?;
            accepted
        }
        other => Err(format!("no sounds operation {other}")),
    }
}

/// Where the Board's dock is served, with its key.
pub fn board_url(config: &crate::Config) -> String {
    format!(
        "{}/sounds/board?token={}",
        crate::overlays::base(config),
        crate::overlays::token(config, BOARD)
    )
}

/// What the Board's dock signs its presses with (a name no group can have).
const BOARD: &str = "\u{1}sounds-board";

/// The Board as a page for an OBS dock: a pad for each, pressed to play.
pub fn board_page(config: &crate::Config, token: &str) -> Option<String> {
    if token != crate::overlays::token(config, BOARD) {
        return None;
    }
    let pads: Vec<Value> = config
        .sounds
        .board
        .iter()
        .map(|p| json!({"id": p.id, "label": p.label}))
        .collect();
    Some(
        BOARD_PAGE
            .replace(
                "__PADS__",
                &serde_json::to_string(&pads).unwrap_or_default().replace("</", "<\\/"),
            )
            .replace("__TOKEN__", token),
    )
}

/// A press of the Board's dock: `{id, token}`.
pub fn board_press(engine: &Rc<Engine>, body: &[u8]) -> (u16, Value) {
    let req: Value = serde_json::from_slice(body).unwrap_or_default();
    if req["token"].as_str() != Some(crate::overlays::token(&engine.config(), BOARD).as_str()) {
        return (403, json!({"ok": false, "error": "not the Board's dock"}));
    }
    match play_pad(engine, req["id"].as_str().unwrap_or_default()) {
        Ok(_) => (200, json!({"ok": true})),
        Err(error) => (400, json!({"ok": false, "error": error})),
    }
}

const BOARD_PAGE: &str = include_str!("../../assets/overlays/board.html");

// Forms: a playing sound's volume, a file to add to the folder, a pad, the Output settings.

const VOLUME_FIELDS: &[F] = &[("volume", "sounds.volume", W::Int)];
const ADD_FIELDS: &[F] = &[("path", "sounds.add_path", W::Text)];
const PAD_FIELDS: &[F] = &[
    ("label", "sounds.pad", W::Text),
    ("file", "sounds.file", W::Text),
    ("volume", "sounds.volume", W::Int),
    ("device", "sounds.device", W::Suggest("sound.devices")),
];
const OUTPUT_FIELDS: &[F] = &[
    ("device", "sounds.output.device", W::Suggest("sound.devices")),
    ("monitor", "sounds.output.monitor", W::Suggest("sound.devices")),
    ("master", "sounds.output.master", W::Int),
    ("maxAtOnce", "sounds.output.max", W::Int),
];

pub fn external_fields(kind: &str) -> Option<&'static [F]> {
    if kind.starts_with("sound_volume:") {
        Some(VOLUME_FIELDS)
    } else if kind.starts_with("sound_add:") {
        Some(ADD_FIELDS)
    } else if kind.starts_with("sound_pad:") {
        Some(PAD_FIELDS)
    } else if kind.starts_with("sounds_output:") {
        Some(OUTPUT_FIELDS)
    } else {
        None
    }
}

fn form_json(kind: &str, title: &str, fields: &[F], item: Value) -> Value {
    let object = item.as_object().cloned().unwrap_or_default();
    let outputs = sound::outputs();
    let fields: Vec<Value> = fields
        .iter()
        .map(|(key, label, widget)| {
            let mut field = json!({"key": key, "label": crate::automation::schema::field_label(label), "widget": widget,
                "holds": widget.about(), "value": crate::automation::edits::shown(widget, object.get(*key))});
            if matches!(widget, W::Suggest(_)) {
                // Default by a word of its own (an empty id is Other's, for a name typed).
                let mut choices = vec![json!({"id": "Default", "label": tr("sounds.default")})];
                choices.extend(outputs.iter().map(|(id, shown)| json!({"id": id, "label": shown})));
                field["choices"] = choices.into();
                field["other"] = true.into();
                if field["value"].as_str().is_none_or(str::is_empty) {
                    field["value"] = "Default".into();
                }
            }
            if *key == "volume" || *key == "master" {
                field["widget"] = json!({"kind": "slider", "min": 0, "max": 100, "step": 1});
                field["value"] = object.get(*key).cloned().unwrap_or(json!(100));
            }
            if *key == "file" || *key == "path" {
                field["browse"] = "file".into();
            }
            field
        })
        .collect();
    json!({"kind": kind, "title": title, "okLabel": tr("btn.save"), "json": false, "new": item.clone(), "fields": fields, "item": item})
}

pub fn external_form(engine: &Rc<Engine>, kind: &str) -> Option<Result<Value, String>> {
    let fields = external_fields(kind)?;
    let key = kind.split_once(':').map_or("", |(_, k)| k);
    let config = engine.config();
    Some(if kind.starts_with("sound_volume:") {
        let id: u64 = key.parse().unwrap_or(0);
        match sound::playing().into_iter().find(|p| p.id == id) {
            Some(p) => Ok(form_json(
                kind,
                tr("sounds.volume"),
                fields,
                json!({"id": key, "volume": (p.volume * 100.0).round()}),
            )),
            None => Err(tr("sounds.not_playing").to_string()),
        }
    } else if kind.starts_with("sound_add:") {
        Ok(form_json(kind, tr("sounds.add"), fields, json!({"id": "", "path": ""})))
    } else if kind.starts_with("sound_pad:") {
        if key.is_empty() {
            Ok(form_json(
                kind,
                tr("sounds.pad_new"),
                fields,
                json!({"id": "", "label": "", "file": "", "volume": 100, "device": ""}),
            ))
        } else {
            pad(&config, key).map(|p| {
                form_json(
                    kind,
                    &p.label,
                    fields,
                    json!({"id": p.id, "label": p.label, "file": p.file,
                    "volume": pad_volume(p), "device": p.device}),
                )
            })
        }
    } else {
        let s = &config.sounds;
        Ok(form_json(
            kind,
            tr("sounds.show.output"),
            fields,
            json!({"id": "", "device": s.device, "monitor": s.monitor,
            "master": s.master, "maxAtOnce": s.max_at_once}),
        ))
    })
}

fn text(item: &Obj, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .trim_matches('"')
        .to_string()
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
    external_fields(kind)?;
    let key = kind.split_once(':').map_or("", |(_, k)| k).to_string();
    Some(if kind.starts_with("sound_volume:") {
        let id: u64 = key.parse().unwrap_or(0);
        sound::set_volume_id(id, sound::volume(number(&item, "volume", 100.0).clamp(0.0, 100.0)), 0);
        Ok(json!({"saved": key}))
    } else if kind.starts_with("sound_add:") {
        add_to_folder(&text(&item, "path"))
    } else if kind.starts_with("sound_pad:") {
        let label = text(&item, "label");
        let file = text(&item, "file");
        if file.is_empty() {
            return Some(Err(tr("sounds.pad_needs_file").into()));
        }
        let label = if label.is_empty() {
            Path::new(&file)
                .file_stem()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            label
        };
        let pad = sound::Pad {
            id: if key.is_empty() {
                crate::engine::uuid()
            } else {
                key.clone()
            },
            label,
            file,
            volume: number(&item, "volume", 100.0).clamp(1.0, 100.0),
            device: Some(text(&item, "device"))
                .filter(|d| !d.eq_ignore_ascii_case("Default"))
                .unwrap_or_default(),
        };
        let saved = pad.id.clone();
        engine.change(|c| match c.sounds.board.iter_mut().find(|p| p.id == pad.id) {
            Some(p) => *p = pad,
            None => c.sounds.board.push(pad),
        });
        Ok(json!({"saved": saved}))
    } else {
        let device = |k: &str| {
            let d = text(&item, k);
            if d.eq_ignore_ascii_case("Default") {
                String::new()
            } else {
                d
            }
        };
        let (d, m) = (device("device"), device("monitor"));
        let master = number(&item, "master", 100.0).clamp(0.0, 100.0);
        let max = number(&item, "maxAtOnce", sound::MAX_AT_ONCE as f64).clamp(1.0, 64.0) as usize;
        engine.change(|c| {
            c.sounds.device = d;
            c.sounds.monitor = m;
            c.sounds.master = master;
            c.sounds.max_at_once = max;
        });
        sound::configure(&engine.config().sounds);
        Ok(json!({"saved": "output"}))
    })
}

/// A file of this computer copied into the sounds folder (a number added when the name is taken).
fn add_to_folder(path: &str) -> Result<Value, String> {
    let from = Path::new(path);
    if !from.is_file() {
        return Err(trf("sounds.add_missing", &[("path", path)]));
    }
    if !is_sound(from) {
        return Err(trf("sounds.add_not_sound", &[("kinds", &sound::EXTENSIONS.join(", "))]));
    }
    let dir = folder();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = from
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, ""));
    let to = (1..1000)
        .map(|n| {
            if n == 1 {
                dir.join(&name)
            } else {
                dir.join(format!("{stem}-{n}.{ext}"))
            }
        })
        .find(|p| !p.exists())
        .ok_or("no name left for it")?;
    std::fs::copy(from, &to).map_err(|e| e.to_string())?;
    Ok(
        json!({"saved": to.display().to_string(), "message": trf("sounds.added", &[("name", &to.file_name().unwrap_or_default().to_string_lossy())])}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_shows_what_plays_what_is_known_the_board_and_the_output() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::engine::tests::run(async {
            let e = crate::engine::tests::engine(json!({"actions": [{"id": "a", "name": "Ding", "subActions": [
                {"type": 1, "soundFile": "/nowhere/ding.wav"}, {"type": 1, "soundFile": "%file%"}]}]}));
            let page = read(&e).unwrap();
            let kinds: Vec<&str> = page["tables"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["kind"].as_str().unwrap())
                .collect();
            assert_eq!(
                kinds,
                [
                    "sounds_playing",
                    "sounds_library",
                    "sounds_board",
                    "sounds_voices",
                    "sounds_output"
                ]
            );
            let library = &page["tables"][1]["rows"];
            assert_eq!(library.as_array().unwrap().len(), 1, "a variable is no file: {library}");
            assert_eq!(library[0]["cells"][2], "Ding");
            assert_eq!(library[0]["enabled"], false, "a file that isn't there says so");
            // Add to Board, its form, play it, make an action from it, move and remove it.
            act(&e, "to_board", "/nowhere/ding.wav").unwrap();
            let id = e.config().sounds.board[0].id.clone();
            let form = external_form(&e, &format!("sound_pad:{id}")).unwrap().unwrap();
            assert_eq!(form["fields"][0]["value"], "ding");
            assert_eq!(form["fields"][0]["label"], "Pad", "a label, not its key");
            assert_eq!(form["fields"][3]["value"], "Default", "Default is a choice of its own");
            assert!(act(&e, "pad_play", &id).is_err(), "a pad whose file is gone says so");
            let made = act(&e, "pad_action", &id).unwrap();
            assert!(e
                .config()
                .actions
                .actions
                .iter()
                .any(|a| a.id == made["saved"] && a.sub_actions[0]["type"] == 1));
            let mut item = Obj::new();
            item.insert("label".into(), "Airhorn".into());
            item.insert("file".into(), "/nowhere/horn.wav".into());
            save_external(&e, "sound_pad:", item).unwrap().unwrap();
            let second = e.config().sounds.board[1].id.clone();
            act(&e, "pad_up", &second).unwrap();
            assert_eq!(e.config().sounds.board[0].label, "Airhorn");
            act(&e, "pad_remove", &second).unwrap();
            assert_eq!(e.config().sounds.board.len(), 1);
            // The Output settings: kept, and the player follows them.
            let mut item = Obj::new();
            item.insert("device".into(), "Speakers".into());
            item.insert("monitor".into(), "Headphones".into());
            item.insert("master".into(), json!(80));
            item.insert("maxAtOnce".into(), json!(4));
            save_external(&e, "sounds_output:", item).unwrap().unwrap();
            assert_eq!(e.config().sounds.max_at_once, 4);
            assert_eq!(sound::device_key("Monitor"), "Headphones");
            assert!((sound::master() - 0.8).abs() < 1e-6);
            // The dock: its page only with its key, and a press plays (or says why not).
            let config = e.config();
            let token = board_url(&config).split("token=").nth(1).unwrap().to_string();
            assert!(board_page(&config, "wrong").is_none());
            assert!(board_page(&config, &token).unwrap().contains("\"label\":\"ding\""));
            drop(config);
            let (status, _) = board_press(&e, json!({"id": id, "token": "x"}).to_string().as_bytes());
            assert_eq!(status, 403);
            let (status, answer) = board_press(&e, json!({"id": id, "token": token}).to_string().as_bytes());
            assert_eq!(status, 400, "{answer}");
            sound::configure(&sound::Settings::default());
        });
    }
}
