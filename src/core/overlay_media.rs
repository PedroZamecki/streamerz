//! The Media tab of an overlay's page: its files seen and heard. Each file says what it is, its size,
//! and what uses it (settings and pages); pictures and GIFs show themselves, sounds play through
//! Streamerz's player. Add brings a file of this computer in; Rename takes the settings that point at
//! the file with it; Replace keeps the name; Delete says first what uses the file.

use crate::engine::Engine;
use crate::groups;
use crate::i18n::{tr, trf};
use serde_json::{json, Value};
use std::rc::Rc;

/// What a file is, by its extension: image, gif, video, sound, font, page, code or other.
pub fn kind_of(name: &str) -> &'static str {
    let extension = name.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "svg" | "bmp" | "avif" => "image",
        "gif" | "apng" => "gif",
        "mp4" | "webm" | "mov" | "mkv" => "video",
        "mp3" | "wav" | "ogg" | "oga" | "flac" | "m4a" | "aac" | "opus" => "sound",
        "ttf" | "otf" | "woff" | "woff2" => "font",
        "html" | "htm" => "page",
        "js" | "css" | "json" | "ts" => "code",
        _ => "other",
    }
}

fn icon_of(kind: &str) -> &'static str {
    match kind {
        "image" | "gif" => "image",
        "video" => "film",
        "sound" => "music",
        "font" => "type",
        "page" | "code" => "code",
        _ => "file",
    }
}

/// Whether a setting's value names the file: as itself, a part of a `|` or line list, or anywhere in a
/// list setting's rows.
fn names(value: &Value, name: &str) -> bool {
    match value {
        Value::String(text) => text.split(['|', '\n', ',']).any(|part| part.trim() == name),
        Value::Array(items) => items.iter().any(|v| names(v, name)),
        Value::Object(map) => map.values().any(|v| names(v, name)),
        _ => false,
    }
}

/// What uses a file: the settings that hold its name, and the pages and code that name it.
pub fn used_by(c: &crate::Config, group: &str, name: &str) -> Vec<String> {
    let meta = groups::meta(c, group);
    let values = groups::values(c, group);
    let mut users: Vec<String> = meta
        .variables
        .iter()
        .filter(|d| values.get(&d.name).is_some_and(|v| names(v, name)))
        .map(|d| {
            if d.label.is_empty() {
                d.name.clone()
            } else {
                d.label.clone()
            }
        })
        .collect();
    for f in &meta.files {
        if f.name == name || !matches!(kind_of(&f.name), "page" | "code") {
            continue;
        }
        let text = groups::read_file(c, group, &f.name)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default();
        if text.contains(name) {
            users.push(f.name.clone());
        }
    }
    users
}

/// The tab: a list of the files, each with its picture (a local path) and its buttons.
pub fn section(engine: &Rc<Engine>, group: &str, id: &str) -> Value {
    let config = engine.config();
    let meta = groups::meta(&config, group);
    let rows: Vec<Value> = meta
        .files
        .iter()
        .map(|f| {
            let kind = kind_of(&f.name);
            let users = used_by(&config, group, &f.name);
            let used = if users.is_empty() {
                tr("overlays.media.unused").to_owned()
            } else {
                trf("overlays.media.used_by", &[("list", &users.join(", "))])
            };
            let kind_key = match kind {
                "image" => "overlays.media.kind.image",
                "gif" => "overlays.media.kind.gif",
                "video" => "overlays.media.kind.video",
                "sound" => "overlays.media.kind.sound",
                "font" => "overlays.media.kind.font",
                "page" => "overlays.media.kind.page",
                "code" => "overlays.media.kind.code",
                _ => "overlays.media.kind.other",
            };
            let mut notes = vec![tr(kind_key).to_owned(), super::pages::size(f.size), used];
            if f.size > 25 * 1024 * 1024 {
                notes.push(tr("overlays.media.too_big").to_owned());
            }
            if kind == "video" && !f.name.to_ascii_lowercase().ends_with(".webm") && !f.name.to_ascii_lowercase().ends_with(".mp4") {
                notes.push(tr("overlays.media.video_format").to_owned());
            }
            let picture = matches!(kind, "image" | "gif")
                .then(|| groups::file_path(&config, group, &f.name))
                .flatten()
                .map(|p| p.display().to_string());
            let name = &f.name;
            let mut buttons = Vec::new();
            if kind == "sound" {
                buttons.push(json!({"id": format!("media_play:{name}"), "label": tr("overlays.media.play"), "icon": "play"}));
                buttons.push(json!({"id": format!("media_stop:{name}"), "label": tr("overlays.media.stop"), "icon": "square"}));
            }
            buttons.push(json!({"id": format!("media_copy:{name}"), "label": tr("overlays.btn.copy_url"), "icon": "copy"}));
            buttons.push(json!({"form": format!("overlay:media_rename:{id}|{name}"), "label": tr("btn.rename"), "icon": "pencil"}));
            buttons.push(json!({"form": format!("overlay:media_replace:{id}|{name}"), "label": tr("overlays.media.replace"), "icon": "replace"}));
            if !groups::is_page(name) || meta.files.iter().filter(|f| groups::is_page(&f.name)).count() > 1 {
                let confirm = if users.is_empty() {
                    trf("overlays.media.delete_confirm", &[("name", name)])
                } else {
                    trf("overlays.media.delete_used", &[("name", name), ("list", &users.join(", "))])
                };
                buttons.push(json!({"id": format!("media_delete:{name}"), "label": tr("btn.delete"), "icon": "trash-2",
                    "confirm": confirm, "danger": true}));
            }
            json!({"text": name, "detail": notes.join(". "), "icon": icon_of(kind), "picture": picture, "buttons": buttons})
        })
        .collect();
    json!({"rows": rows, "empty": tr("overlays.media.none"), "buttons": [
        {"form": format!("overlay:media_add:{id}"), "label": tr("overlays.media.add"), "icon": "plus"},
        {"id": "open_folder", "label": tr("btn.open_folder"), "icon": "folder-open"},
    ]})
}

/// The operations of a file's buttons: `media_play:<file>`, `media_stop:`, `media_copy:`, `media_delete:`.
pub fn act(engine: &Rc<Engine>, group: &str, action: &str) -> Result<Value, String> {
    let (op, name) = action.split_once(':').ok_or("no file named")?;
    let config = engine.config();
    let path = groups::file_path(&config, group, name).ok_or_else(|| tr("overlays.media.gone").to_owned())?;
    match op {
        "media_play" => {
            crate::sound::play(&path, &format!("media:{group}"), "Monitor", 1.0)?;
            Ok(json!({"accepted": true}))
        }
        "media_stop" => {
            crate::sound::stop(Some(&format!("media:{group}")), 0);
            Ok(json!({"accepted": true}))
        }
        "media_copy" => {
            let address = format!(
                "{}/overlays/{}/{}",
                crate::overlays::base(&config),
                crate::commands::url_encode(group),
                name
            );
            Ok(json!({"copy": address, "message": tr("overlays.copied")}))
        }
        "media_delete" => {
            drop(config);
            let mut result = Ok(());
            engine.change(|c| {
                let mut meta = groups::meta(c, group);
                meta.files.retain(|f| f.name != name);
                result = groups::set_meta(c, group, meta);
            });
            result.map(|()| json!({"accepted": true}))
        }
        other => Err(format!("no media operation {other}")),
    }
}

/// Puts `to` wherever a value names `from` (see `names`).
fn follow(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::String(text) if text.split(['|', '\n', ',']).any(|part| part.trim() == from) => {
            *text = text.replace(from, to);
        }
        Value::Array(items) => items.iter_mut().for_each(|v| follow(v, from, to)),
        Value::Object(map) => map.values_mut().for_each(|v| follow(v, from, to)),
        _ => {}
    }
}

/// A file of this computer as one of the group's: its bytes and a free name like it.
fn read_in(meta: &groups::Meta, path: &str, keep: Option<&str>) -> Result<groups::File, String> {
    use base64::Engine as _;
    let path = std::path::Path::new(path.trim().trim_matches('"'));
    let bytes = std::fs::read(path).map_err(|e| {
        trf(
            "dialogs.import.cant_read",
            &[("file", &path.display().to_string()), ("error", &e.to_string())],
        )
    })?;
    let name = match keep {
        Some(name) => name.to_owned(),
        None => {
            let original = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            free_name(meta, original)?
        }
    };
    Ok(groups::File {
        name,
        content: base64::engine::general_purpose::STANDARD.encode(bytes),
        size: 0,
    })
}

/// A portable name like `wanted` that the group doesn't have yet.
fn free_name(meta: &groups::Meta, wanted: &str) -> Result<String, String> {
    let clean: String = wanted
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let (stem, extension) = clean.rsplit_once('.').unwrap_or((&clean, "bin"));
    (1..1000)
        .map(|n| {
            if n == 1 {
                format!("{stem}.{extension}")
            } else {
                format!("{stem}-{n}.{extension}")
            }
        })
        .find(|name| groups::valid_file_path(name) && !meta.files.iter().any(|f| f.name.eq_ignore_ascii_case(name)))
        .ok_or_else(|| tr("overlays.media.bad_name").to_owned())
}

/// Add (a file of this computer), Rename (the settings that name it follow) and Replace.
pub fn edit(engine: &Rc<Engine>, group: &str, form: &str, file: &str, value: &str) -> Result<Value, String> {
    let mut meta = groups::meta(&engine.config(), group);
    let mut renamed: Option<(String, String)> = None;
    match form {
        "media_add" => {
            let added = read_in(&meta, value, None)?;
            meta.files.push(added);
        }
        "media_replace" => {
            let at = meta
                .files
                .iter()
                .position(|f| f.name == file)
                .ok_or_else(|| tr("overlays.media.gone").to_owned())?;
            meta.files[at] = read_in(&meta, value, Some(file))?;
        }
        _ => {
            let wanted = value.trim();
            if wanted == file {
                return Ok(json!({"accepted": true}));
            }
            if !groups::valid_file_path(wanted) || meta.files.iter().any(|f| f.name.eq_ignore_ascii_case(wanted)) {
                return Err(tr("overlays.media.bad_name").into());
            }
            let path =
                groups::file_path(&engine.config(), group, file).ok_or_else(|| tr("overlays.media.gone").to_owned())?;
            let at = meta
                .files
                .iter()
                .position(|f| f.name == file)
                .ok_or_else(|| tr("overlays.media.gone").to_owned())?;
            meta.files[at] = read_in(&meta, &path.display().to_string(), Some(wanted))?;
            renamed = Some((file.to_owned(), wanted.to_owned()));
        }
    }
    let mut result = Ok(());
    engine.change(|c| {
        result = groups::set_meta(c, group, meta).and_then(|()| {
            let Some((from, to)) = &renamed else { return Ok(()) };
            // The settings that held the old name hold the new one.
            let mut values = groups::values(c, group);
            for value in values.values_mut() {
                follow(value, from, to);
            }
            groups::set_values(c, group, values)
        });
    });
    result.map(|()| json!({"accepted": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_found_and_renamed_inside_list_settings() {
        let mut rules = json!([{"media": "img/star.svg", "text": "hi"}, {"media": "other.png"}]);
        assert!(names(&rules, "img/star.svg"));
        assert!(!names(&rules, "img/star"));
        follow(&mut rules, "img/star.svg", "img/new.svg");
        assert_eq!(rules[0]["media"], "img/new.svg");
        assert!(names(&json!("a.png|img/star.svg"), "img/star.svg"));
    }

    #[test]
    fn a_file_is_known_by_its_extension() {
        assert_eq!(kind_of("img/star.SVG"), "image");
        assert_eq!(kind_of("party.gif"), "gif");
        assert_eq!(kind_of("sounds/chime.wav"), "sound");
        assert_eq!(kind_of("clip.webm"), "video");
        assert_eq!(kind_of("Inter.woff2"), "font");
        assert_eq!(kind_of("index.html"), "page");
        assert_eq!(kind_of("notes"), "other");
    }

    #[test]
    fn a_free_name_gets_a_number_when_taken() {
        let meta = groups::Meta {
            files: vec![groups::File {
                name: "a.png".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(free_name(&meta, "a.png").unwrap(), "a-2.png");
        assert_eq!(free_name(&meta, "my pic.png").unwrap(), "my-pic.png");
    }
}
