//! The `.streamerz` file: a group packed to share, as a template (actions, commands, timers) or an
//! overlay alone (pages only). It is a zip anyone can open:
//!
//! | In the file | What |
//! |---|---|
//! | `template.json` | `{format, name, streamerz, meta}`: the group's name and its [`Meta`] (description, settings, permissions, what it says about itself) with its files listed by name |
//! | `actions.json` | the group's actions, commands and timers, as an export (`edit::export_group`) |
//! | `files/...` | the group's files as they are on disk: pages, pictures, sounds |
//!
//! Nothing here touches a window: [`open`] and [`describe`] say what a file holds before anything
//! changes, [`install`] brings it in, [`pack`] and [`checks`] are for the person sharing.

use crate::groups::{self, Meta};
use crate::model::Obj;
use crate::Config;
use base64::Engine as _;
use serde_json::{json, Value};

/// The file's own version: a file made by a newer Streamerz says so instead of half working.
const FORMAT: u64 = 1;
/// The most a file may unpack to: what a zip can hold. A file that says less and holds more is refused.
const LIMIT: u64 = u32::MAX as u64;
pub const EXTENSION: &str = "streamerz";

/// A template file, read and checked, not yet installed.
#[derive(Debug)]
pub struct Package {
    /// The group's name where it was made.
    pub name: String,
    /// The group's meta, each file with its contents.
    pub meta: Meta,
    /// Its actions, commands and timers, with the meta inside, as `edit::import` takes it.
    pub export: Obj,
    /// The bytes of its files.
    pub size: u64,
}

fn b64() -> base64::engine::general_purpose::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Give the group the id that says "the same template" in every version of its file.
pub fn ensure_id(c: &mut Config, group: &str) -> Result<(), String> {
    let mut m = groups::meta(c, group);
    if m.template.id.is_empty() {
        m.template.id = crate::engine::uuid();
        groups::set_meta(c, group, m)?;
    }
    Ok(())
}

/// The group as a `.streamerz` file. The values set here stay out unless `values` (a template is
/// shared with its defaults, not with the sharer's own texts and numbers).
pub fn pack(c: &Config, group: &str, values: bool) -> Result<Vec<u8>, String> {
    if !groups::exists(c, group) {
        return Err(format!("no group named {group}"));
    }
    let mut meta = groups::meta_with_files(c, group);
    let mut files = Vec::new();
    for f in &mut meta.files {
        let bytes = b64()
            .decode(&f.content)
            .map_err(|_| format!("cannot read {}", f.name))?;
        f.size = bytes.len() as u64;
        f.content = String::new();
        files.push((format!("files/{}", f.name), bytes));
    }
    let t = &meta.template;
    let mut export = crate::edit::export_group(c, group, [group, &t.author, &t.version, &meta.description]);
    if let Some(ours) = export.get_mut("streamerz").and_then(Value::as_object_mut) {
        ours.remove("groups");
        if !values {
            ours.remove("groupVariables");
        }
    }
    let about = json!({"format": FORMAT, "name": group, "streamerz": crate::brand::VERSION, "meta": meta});
    let pretty = |v: &Value| serde_json::to_vec_pretty(v).map_err(|e| e.to_string());
    let mut all = vec![
        ("template.json".to_string(), pretty(&about)?),
        ("actions.json".to_string(), pretty(&Value::Object(export))?),
    ];
    all.extend(files);
    crate::zip::write(&all)
}

/// Read a `.streamerz` file and check all of it. Nothing is changed.
pub fn open(bytes: &[u8]) -> Result<Package, String> {
    let entries = crate::zip::read(bytes, LIMIT).map_err(|e| format!("this is not a template file ({e})"))?;
    unpacked(&entries)
}

/// A template from the files of its archive (name, contents), wherever they came from: a
/// `.streamerz` file, or the example folders built into Streamerz (`examples.rs`).
pub fn unpacked(entries: &[(String, Vec<u8>)]) -> Result<Package, String> {
    let entry = |name: &str| entries.iter().find(|(n, _)| n == name).map(|(_, b)| b.as_slice());
    let about: Value = serde_json::from_slice(entry("template.json").ok_or("this is not a template file")?)
        .map_err(|e| format!("its template.json is damaged ({e})"))?;
    if about["format"].as_u64().is_none_or(|f| f > FORMAT) {
        return Err("this template was made by a newer Streamerz: update to import it".into());
    }
    let name = about["name"].as_str().unwrap_or("").trim().to_string();
    if name.is_empty() || name.len() > 200 {
        return Err("the template has no name".into());
    }
    let mut meta: Meta =
        serde_json::from_value(about["meta"].clone()).map_err(|e| format!("its template.json is damaged ({e})"))?;
    meta.store.clear();
    let mut size = 0;
    for f in &mut meta.files {
        let bytes = entry(&format!("files/{}", f.name)).ok_or_else(|| format!("the file {} is missing", f.name))?;
        size += bytes.len() as u64;
        f.size = 0;
        f.content = b64().encode(bytes);
    }
    meta.validate()?;
    let mut export: Obj = match entry("actions.json") {
        Some(bytes) => serde_json::from_slice(bytes).map_err(|e| format!("its actions.json is damaged ({e})"))?,
        None => Obj::new(),
    };
    export.entry("data").or_insert_with(|| json!({}));
    // Only this group comes in, under the name the file says: whatever else the export names is dropped.
    let ours = export.entry("streamerz").or_insert_with(|| json!({}));
    if !ours.is_object() {
        *ours = json!({});
    }
    ours["groups"] = json!({ name.clone(): meta });
    for key in ["groupVariables", "groupIcons"] {
        if let Some(map) = ours.get_mut(key).and_then(Value::as_object_mut) {
            map.retain(|group, _| *group == name);
        }
    }
    for list in ["actions", "commands", "timers"] {
        for item in export["data"][list].as_array_mut().into_iter().flatten() {
            if let Some(item) = item.as_object_mut() {
                item.insert("group".into(), name.clone().into());
            }
        }
    }
    Ok(Package {
        name,
        meta,
        export,
        size,
    })
}

/// The group here that a package would update: the one with its id (whatever it is named now).
fn installed(c: &Config, p: &Package) -> Option<String> {
    let id = &p.meta.template.id;
    if id.is_empty() {
        return None;
    }
    groups::all(c)
        .into_iter()
        .find(|g| groups::meta(c, g).template.id == *id)
}

/// The group a package goes into: the one it updates, else one of its own name.
pub fn target(c: &Config, p: &Package) -> String {
    installed(c, p).unwrap_or_else(|| p.name.clone())
}

/// What a package holds and what importing it would do, for whoever asks first (a dialog, an
/// assistant): its name and author, what is inside, what its pages may do, what it needs, the
/// programs it would run, and whether it is new here, an update, or replaces a group of the same name.
pub fn describe(c: &Config, p: &Package) -> Value {
    let count = |k: &str| p.export["data"][k].as_array().map_or(0, Vec::len);
    let t = &p.meta.template;
    let target = target(c, p);
    let state = match installed(c, p) {
        Some(group) => {
            let have = groups::meta(c, &group).template.version;
            json!({"state": if have == t.version { "same" } else { "update" }, "group": group, "version": have})
        }
        None if groups::exists(c, &p.name) => json!({"state": "replaces", "group": p.name}),
        None => json!({"state": "new", "group": p.name}),
    };
    let granted = groups::meta(c, &target).permissions;
    let permissions: Vec<Value> = crate::overlays::PERMISSIONS
        .iter()
        .filter(|(name, _)| p.meta.permissions.iter().any(|x| x == name))
        .map(|(name, says)| json!({"name": name, "says": says, "new": !granted.iter().any(|g| g == name)}))
        .collect();
    json!({
        "name": p.name, "about": p.meta.description, "author": t.author, "version": t.version, "link": t.link,
        "id": t.id, "picture": t.picture,
        "kind": if count("actions") + count("commands") + count("timers") == 0 { "overlay" } else { "template" },
        "actions": count("actions"), "commands": count("commands"), "timers": count("timers"),
        "settings": p.meta.variables.len(),
        "files": p.meta.files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        "pages": t.pages, "size": p.size,
        "permissions": permissions, "requires": t.requires,
        "programs": crate::edit::programs(&p.export),
        "installed": state,
    })
}

/// Rename the group an export is about, everywhere it names it.
fn renamed(mut export: Obj, from: &str, to: &str) -> Obj {
    if from == to {
        return export;
    }
    for list in ["actions", "commands", "timers"] {
        for item in export["data"][list].as_array_mut().into_iter().flatten() {
            if item["group"].as_str() == Some(from) {
                item["group"] = to.into();
            }
        }
    }
    for key in ["groups", "groupVariables", "groupIcons"] {
        if let Some(map) = export["streamerz"].get_mut(key).and_then(Value::as_object_mut) {
            if let Some(v) = map.remove(from) {
                map.insert(to.into(), v);
            }
        }
    }
    export
}

/// Bring a package in: into the group it updates (kept under the name it has here) or a group of
/// its own name. The values set here stay; new settings start at their defaults. Gives the group
/// and the names of the sub-action types that won't run here.
pub fn install(c: &mut Config, p: &Package) -> Result<(String, Vec<&'static str>), String> {
    let target = target(c, p);
    install_as(c, p, &target)
}

/// Bring a package in under this group name, whatever it is called in the file.
pub fn install_as(c: &mut Config, p: &Package, group: &str) -> Result<(String, Vec<&'static str>), String> {
    let export = renamed(p.export.clone(), &p.name, group);
    let unsupported = crate::edit::import(c, &export)?;
    // What comes with actions, commands or timers is a group of actions, connected to its overlay.
    let count = |k: &str| p.export["data"][k].as_array().map_or(0, Vec::len);
    if count("actions") + count("commands") + count("timers") > 0 && !groups::listed(c, group) {
        c.actions.groups.push(group.into());
    }
    Ok((group.to_owned(), unsupported))
}

/// The links of a page to other files of its group: what `src=`, `href=` and `url()` name, without
/// the ones that leave the group (a web address, a `data:` picture, a `#` anchor).
fn links(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (open, close) in [
        ("src=\"", '"'),
        ("href=\"", '"'),
        ("src='", '\''),
        ("href='", '\''),
        ("url(", ')'),
    ] {
        for (at, _) in html.match_indices(open) {
            let rest = &html[at + open.len()..];
            let Some(end) = rest.find(close) else { continue };
            let link = rest[..end].trim().trim_matches(['"', '\'']);
            let link = link.split(['?', '#']).next().unwrap_or("");
            let outside = link.contains("://")
                || link.starts_with("//")
                || link.starts_with('/')
                || link.contains(['{', '$', '+', ' '])
                || ["data:", "mailto:", "javascript:", "blob:", "about:"]
                    .iter()
                    .any(|p| link.starts_with(p));
            if !link.is_empty() && !outside {
                out.push(link.to_string());
            }
        }
    }
    out
}

/// `link`, as a page in `folder` names it, as a file name of the group. None: it leaves the group.
fn resolve(folder: &str, link: &str) -> Option<String> {
    let mut parts: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
    for part in link.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// What the person sharing a group should look at first, as sentences. Empty: nothing found.
pub fn checks(c: &Config, group: &str) -> Vec<String> {
    use crate::i18n::trf;
    let meta = groups::meta(c, group);
    let mut out = Vec::new();
    let t = &meta.template;
    if t.version.is_empty() {
        out.push(trf("templates.check.version", &[]));
    }
    if t.author.is_empty() {
        out.push(trf("templates.check.author", &[]));
    }
    if meta.description.trim().is_empty() {
        out.push(trf("templates.check.about", &[]));
    }
    let is_page = |name: &str| name.to_ascii_lowercase().ends_with(".html");
    for f in meta.files.iter().filter(|f| is_page(&f.name)) {
        if !t.pages.iter().any(|p| p.file == f.name) {
            out.push(trf("templates.check.unlisted", &[("file", &f.name)]));
        }
    }
    // What the pages call, against what the group lets them.
    let uses = [
        ("chat.read", "streamerz.chat("),
        ("alerts.read", "streamerz.alerts("),
        ("chat.send", "streamerz.say("),
        ("actions.run", "streamerz.run("),
        ("values.write", "streamerz.set("),
    ];
    let mut used = std::collections::HashSet::new();
    let secrets = [
        "oauth:",
        "client_secret",
        "access_token",
        "refresh_token",
        "Bearer ",
        "password=",
    ];
    for f in &meta.files {
        let lower = f.name.to_ascii_lowercase();
        if ![".html", ".htm", ".js", ".mjs", ".css", ".json", ".txt", ".md", ".svg"]
            .iter()
            .any(|e| lower.ends_with(e))
        {
            continue;
        }
        let Some(text) = groups::read_file(c, group, &f.name).and_then(|b| String::from_utf8(b).ok()) else {
            continue;
        };
        // A call in a comment (the starter page lists what to try) is not a call.
        let code: String = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for (permission, call) in uses {
            if code.contains(call) {
                used.insert(permission);
            }
        }
        if let Some(word) = secrets.iter().find(|s| text.contains(**s)) {
            out.push(trf(
                "templates.check.secret",
                &[("file", &f.name), ("word", word.trim())],
            ));
        }
        if is_page(&f.name) || lower.ends_with(".css") {
            let folder = f.name.rsplit_once('/').map_or("", |x| x.0);
            let mut missing: Vec<String> = links(&text)
                .into_iter()
                .filter(|l| resolve(folder, l).is_none_or(|name| !meta.files.iter().any(|f| f.name == name)))
                .collect();
            missing.sort();
            missing.dedup();
            for link in missing {
                out.push(trf("templates.check.missing", &[("file", &f.name), ("link", &link)]));
            }
        }
    }
    for (permission, _) in uses {
        let granted = meta.permissions.iter().any(|p| p == permission);
        if used.contains(permission) && !granted {
            out.push(trf("templates.check.needs", &[("permission", permission)]));
        } else if granted && !used.contains(permission) {
            out.push(trf("templates.check.unused", &[("permission", permission)]));
        }
    }
    let export = crate::edit::export_group(c, group, [group, "", "", ""]);
    for program in crate::edit::programs(&export) {
        out.push(trf("templates.check.program", &[("program", &program)]));
    }
    out
}

/// The bytes of the template file a request names: `path` (a file here), `content` (base64) or
/// `url` (a web address).
async fn source(req: &Value) -> Result<Vec<u8>, String> {
    let text = |k: &str| req.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
    if let Some(path) = text("path") {
        return std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"));
    }
    if let Some(content) = text("content") {
        return b64().decode(content).map_err(|_| "content is not base64".into());
    }
    match text("url") {
        Some(url) => download(url).await,
        None => Err("give the file as path, content (base64) or url".into()),
    }
}

/// The template requests, for a page, an assistant or a script:
///
/// | Request | Does |
/// |---|---|
/// | `StreamerzTemplateCheck` `{group}` | what to look at before sharing the group: `{checks}` |
/// | `StreamerzTemplateExport` `{group, path?, values?}` | packs the group into a `.streamerz` file at `path` (or gives it as base64 `content`), with `{size, checks}` |
/// | `StreamerzTemplateInspect` `{path \| content \| url}` | what a file holds and what importing it would do, changing nothing: `{template}` |
/// | `StreamerzTemplateImport` `{path \| content \| url, allowPrograms?}` | installs it: `{group, unsupported, template}` |
pub(crate) async fn request(e: &std::rc::Rc<crate::engine::Engine>, req: &Value) -> Result<Value, String> {
    let text = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("");
    let group = text("group");
    let known = || {
        if groups::exists(&e.config(), group) {
            Ok(())
        } else {
            Err(format!("no group named {group}"))
        }
    };
    match text("request") {
        "StreamerzTemplateCheck" => {
            known()?;
            Ok(json!({"checks": checks(&e.config(), group)}))
        }
        "StreamerzTemplateExport" => {
            known()?;
            let mut result = Ok(());
            e.change(|c| result = ensure_id(c, group));
            result?;
            let c = e.config();
            let file = pack(&c, group, req["values"].as_bool().unwrap_or(false))?;
            let mut out = json!({"size": file.len(), "checks": checks(&c, group)});
            if text("path").is_empty() {
                out["content"] = b64().encode(&file).into();
            } else {
                let mut path = std::path::PathBuf::from(text("path"));
                if path.extension().is_none_or(|x| x != EXTENSION) {
                    path.as_mut_os_string().push(format!(".{EXTENSION}"));
                }
                std::fs::write(&path, &file).map_err(|err| format!("cannot write {}: {err}", path.display()))?;
                out["path"] = path.to_string_lossy().into_owned().into();
            }
            Ok(out)
        }
        "StreamerzTemplateInspect" => {
            let p = open(&source(req).await?)?;
            Ok(json!({"template": describe(&e.config(), &p)}))
        }
        "StreamerzTemplateImport" => {
            let p = open(&source(req).await?)?;
            let about = describe(&e.config(), &p);
            if about["programs"].as_array().is_some_and(|a| !a.is_empty()) && req["allowPrograms"] != true {
                return Err("The template runs programs; confirm them before setting allowPrograms".into());
            }
            let mut result = Err("not imported".to_string());
            e.change(|c| result = install(c, &p));
            let (group, unsupported) = result?;
            Ok(json!({"group": group, "unsupported": unsupported, "template": about}))
        }
        _ => Err("unknown template request".into()),
    }
}

/// A template file from a web address, whole. Only `https` (and `http` on this computer).
pub async fn download(url: &str) -> Result<Vec<u8>, String> {
    let local = url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost");
    if !url.starts_with("https://") && !local {
        return Err("a template is downloaded from an https address".into());
    }
    let answer = crate::http::request("GET", url, &[], &[]).await?;
    if answer.status != 200 {
        return Err(format!("the address answered {}", answer.status));
    }
    Ok(answer.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::{text_file, Def, Kind, Page, Template};

    type Files = Vec<(String, Vec<u8>)>;

    fn wheel() -> Config {
        let mut c = Config::default();
        let mut a = crate::edit::new_action("Spin", "Wheel");
        a.sub_actions.push(crate::edit::new_sub(1020));
        c.actions.actions.push(a);
        let page = r#"<link rel=stylesheet href="css/look.css"><img src="img/wheel.png"><img src="https://x.example/a.png">
            <script>streamerz.run("Spin"); streamerz.alerts(a => a);</script>"#;
        let meta = Meta {
            description: "A wheel".into(),
            variables: vec![Def {
                name: "slices".into(),
                label: "Slices".into(),
                kind: Kind::Integer,
                default: json!(8),
                ..Default::default()
            }],
            files: vec![
                text_file("wheel.html", page),
                text_file("css/look.css", "body{background:url(../img/wheel.png)}"),
                text_file("img/wheel.png", "PNG"),
            ],
            permissions: vec!["actions.run".into(), "alerts.read".into()],
            template: Template {
                version: "1.0.0".into(),
                author: "ana".into(),
                pages: vec![Page {
                    file: "wheel.html".into(),
                    title: "Wheel".into(),
                    width: 800,
                    height: 800,
                    dock: false,
                }],
                requires: vec!["twitch".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        groups::set_meta(&mut c, "Wheel", meta).unwrap();
        let mut v = groups::values(&c, "Wheel");
        v.insert("slices".into(), json!(12));
        groups::set_values(&mut c, "Wheel", v).unwrap();
        ensure_id(&mut c, "Wheel").unwrap();
        c
    }

    #[test]
    fn a_group_packs_into_a_file_that_installs_elsewhere() {
        let c = wheel();
        assert!(checks(&c, "Wheel").is_empty(), "{:?}", checks(&c, "Wheel"));
        let file = pack(&c, "Wheel", false).unwrap();
        let names: Vec<String> = crate::zip::read(&file, LIMIT)
            .unwrap()
            .into_iter()
            .map(|f| f.0)
            .collect();
        assert_eq!(
            names,
            [
                "template.json",
                "actions.json",
                "files/wheel.html",
                "files/css/look.css",
                "files/img/wheel.png"
            ]
        );
        let p = open(&file).unwrap();
        assert_eq!(
            (p.name.as_str(), p.size),
            ("Wheel", groups::meta(&c, "Wheel").files.iter().map(|f| f.size).sum())
        );
        let mut d = Config::default();
        let about = describe(&d, &p);
        assert_eq!(about["installed"]["state"], "new");
        assert_eq!(
            (about["kind"].as_str(), about["actions"].as_u64()),
            (Some("template"), Some(1))
        );
        assert_eq!(
            about["permissions"][0],
            json!({"name": "alerts.read", "says": "See follows, subscriptions, gifts, donations and raids", "new": true})
        );
        assert_eq!(about["requires"], json!(["twitch"]));
        assert_eq!(about["pages"][0]["width"], 800);
        let (group, unsupported) = install(&mut d, &p).unwrap();
        assert!(group == "Wheel" && unsupported.is_empty());
        assert_eq!(
            groups::meta_with_files(&d, "Wheel"),
            groups::meta_with_files(&c, "Wheel")
        );
        assert_eq!(
            groups::read_file(&d, "Wheel", "css/look.css").unwrap(),
            b"body{background:url(../img/wheel.png)}"
        );
        assert_eq!(d.actions.actions[0].name, "Spin");
        // The sharer's own values stay behind unless asked for.
        assert_eq!(groups::values(&d, "Wheel")["slices"], 8);
        let mut with = Config::default();
        install(&mut with, &open(&pack(&c, "Wheel", true).unwrap()).unwrap()).unwrap();
        assert_eq!(groups::values(&with, "Wheel")["slices"], 12);
        for config in [c, d, with] {
            let mut config = config;
            groups::remove(&mut config, "Wheel");
        }
    }

    #[test]
    fn a_newer_file_updates_the_group_it_came_from_and_keeps_what_was_set() {
        let mut maker = wheel();
        let mut user = Config::default();
        install(&mut user, &open(&pack(&maker, "Wheel", false).unwrap()).unwrap()).unwrap();
        // The user renames the group and sets a value; the maker ships version 2 with a new setting.
        assert!(groups::rename(&mut user, "Wheel", "My wheel"));
        let mut v = groups::values(&user, "My wheel");
        v.insert("slices".into(), json!(5));
        groups::set_values(&mut user, "My wheel", v).unwrap();
        let mut m = groups::meta(&maker, "Wheel");
        m.template.version = "2.0.0".into();
        m.permissions.push("chat.send".into());
        m.variables.push(Def {
            name: "sound".into(),
            label: "Sound".into(),
            kind: Kind::Boolean,
            default: json!(true),
            ..Default::default()
        });
        m.files.retain(|f| f.name != "img/wheel.png");
        m.files.push(text_file("wheel.html", "<p>v2</p>"));
        m.files.remove(0);
        m.template.pages[0].file = "wheel.html".into();
        groups::set_meta(&mut maker, "Wheel", m).unwrap();
        let p = open(&pack(&maker, "Wheel", false).unwrap()).unwrap();
        let about = describe(&user, &p);
        assert_eq!(
            about["installed"],
            json!({"state": "update", "group": "My wheel", "version": "1.0.0"})
        );
        let new: Vec<&str> = about["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["new"] == true)
            .filter_map(|p| p["name"].as_str())
            .collect();
        assert_eq!(new, ["chat.send"], "only what it did not have before is new");
        let (group, _) = install(&mut user, &p).unwrap();
        assert_eq!(group, "My wheel");
        assert!(!groups::exists(&user, "Wheel"), "no second group appears");
        let v = groups::values(&user, "My wheel");
        assert_eq!((v["slices"].clone(), v["sound"].clone()), (json!(5), json!(true)));
        assert_eq!(
            groups::read_file(&user, "My wheel", "wheel.html").unwrap(),
            b"<p>v2</p>"
        );
        assert_eq!(
            groups::read_file(&user, "My wheel", "img/wheel.png"),
            None,
            "a file it dropped is gone"
        );
        assert_eq!(user.actions.actions.len(), 1);
        assert_eq!(user.actions.actions[0].group, "My wheel");
        assert_eq!(describe(&user, &p)["installed"]["state"], "same");
        // A group of the same name that isn't this template is said to be replaced.
        let mut other = Config::default();
        groups::create(&mut other, "Wheel");
        assert_eq!(
            describe(&other, &p)["installed"],
            json!({"state": "replaces", "group": "Wheel"})
        );
        groups::remove(&mut maker, "Wheel");
        groups::remove(&mut user, "My wheel");
    }

    #[test]
    fn an_overlay_alone_is_a_package_too() {
        let mut c = Config::default();
        install(&mut c, &crate::examples::package("viewers").unwrap()).unwrap();
        assert!(
            groups::names(&c).is_empty(),
            "an overlay alone makes no group of actions"
        );
        let p = open(&pack(&c, "Viewer counter", false).unwrap()).unwrap();
        let about = describe(&Config::default(), &p);
        assert_eq!(
            (about["kind"].as_str(), about["actions"].as_u64()),
            (Some("overlay"), Some(0))
        );
        assert_eq!(about["files"], json!(["index.html"]));
        groups::remove(&mut c, "Viewer counter");
    }

    #[test]
    fn bad_files_are_refused_whole() {
        let c = wheel();
        let good = crate::zip::read(&pack(&c, "Wheel", false).unwrap(), LIMIT).unwrap();
        let with = |change: &dyn Fn(&mut Files)| {
            let mut files = good.clone();
            change(&mut files);
            open(&crate::zip::write(&files).unwrap()).unwrap_err()
        };
        assert!(open(b"just text").unwrap_err().contains("not a template file"));
        assert!(with(&|f| f.retain(|x| x.0 != "template.json")).contains("not a template file"));
        assert!(with(&|f| f.retain(|x| x.0 != "files/img/wheel.png")).contains("img/wheel.png is missing"));
        let edit = |f: &mut Files, change: &dyn Fn(&mut Value)| {
            let mut v: Value = serde_json::from_slice(&f[0].1).unwrap();
            change(&mut v);
            f[0].1 = v.to_string().into_bytes();
        };
        assert!(with(&|f| edit(f, &|v| v["format"] = 99.into())).contains("newer Streamerz"));
        assert!(with(&|f| edit(f, &|v| v["name"] = "".into())).contains("no name"));
        assert!(
            with(&|f| edit(f, &|v| v["meta"]["permissions"] = json!(["everything"]))).contains("unknown permission")
        );
        assert!(with(&|f| edit(f, &|v| v["meta"]["files"][0]["name"] = "../../x.html".into())).contains("missing"));
        assert!(with(&|f| f[1].1 = b"[".to_vec()).contains("actions.json"));
        // What the export names besides this group is dropped: a file can't write into another group.
        let mut files = good.clone();
        let mut x: Value = serde_json::from_slice(&files[1].1).unwrap();
        x["data"]["actions"][0]["group"] = "Someone else's".into();
        x["streamerz"]["groupVariables"] = json!({"Someone else's": {"a": 1}});
        files[1].1 = x.to_string().into_bytes();
        let p = open(&crate::zip::write(&files).unwrap()).unwrap();
        assert_eq!(p.export["data"]["actions"][0]["group"], "Wheel");
        assert_eq!(p.export["streamerz"]["groupVariables"], json!({}));
        let mut c = c;
        groups::remove(&mut c, "Wheel");
    }

    #[test]
    fn the_checks_say_what_to_fix_before_sharing() {
        let mut c = wheel();
        let mut m = groups::meta(&c, "Wheel");
        m.template.version.clear();
        m.template.pages.clear();
        m.permissions = vec!["chat.send".into()];
        m.files.retain(|f| f.name != "img/wheel.png");
        m.files.push(text_file("notes.txt", "token oauth:abcdef"));
        groups::set_meta(&mut c, "Wheel", m).unwrap();
        c.actions.actions[0].sub_actions =
            vec![serde_json::from_value(json!({"type": 6, "command": "calc.exe"})).unwrap()];
        let found = checks(&c, "Wheel").join("\n");
        for part in [
            "version",
            "wheel.html is not listed",
            "wheel.html links img/wheel.png",
            "css/look.css links ../img/wheel.png",
            "notes.txt",
            "actions.run",
            "alerts.read",
            "chat.send",
            "calc.exe",
        ] {
            assert!(found.contains(part), "{part} in:\n{found}");
        }
        assert!(!found.contains("x.example"), "a web address is not a file of the group");
        // The starter page only mentions calls in comments: nothing to ask for, nothing to report.
        let mut fresh = Config::default();
        let group = crate::core::overlays::new_overlay(&mut fresh, "Fresh", false).unwrap();
        let found = checks(&fresh, &group).join("\n");
        assert!(!found.contains("chat.") && !found.contains("actions.run"), "{found}");
        groups::remove(&mut fresh, &group);
        groups::remove(&mut c, "Wheel");
    }

    #[test]
    fn the_requests_export_inspect_and_import_a_file() {
        use crate::engine::tests::{engine, run};
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join(format!("streamerz-template-{}", std::process::id()));
        run(async {
            let e = engine(json!({"actions": []}));
            e.change(|c| *c = wheel());
            let id_before = groups::meta(&e.config(), "Wheel").template.id;
            let ask = async |req: Value| request(&e, &req).await;
            let out = ask(json!({"request": "StreamerzTemplateExport", "group": "Wheel", "path": path}))
                .await
                .unwrap();
            let file = out["path"].as_str().unwrap().to_string();
            assert!(file.ends_with(".streamerz") && out["checks"] == json!([]), "{out}");
            assert_eq!(std::fs::metadata(&file).unwrap().len(), out["size"].as_u64().unwrap());
            assert!(ask(json!({"request": "StreamerzTemplateExport", "group": "Nope"}))
                .await
                .is_err());
            let content = ask(json!({"request": "StreamerzTemplateExport", "group": "Wheel"}))
                .await
                .unwrap();
            assert_eq!(
                b64().decode(content["content"].as_str().unwrap()).unwrap(),
                std::fs::read(&file).unwrap()
            );
            // The same installation sees its own file as the template it already has.
            let seen = ask(json!({"request": "StreamerzTemplateInspect", "path": file}))
                .await
                .unwrap();
            assert_eq!(seen["template"]["installed"]["state"], "same");
            assert_eq!(seen["template"]["id"], id_before.as_str());
            // A program inside has to be confirmed.
            e.change(|c| {
                c.actions.actions[0].sub_actions =
                    vec![serde_json::from_value(json!({"type": 6, "command": "calc.exe"})).unwrap()]
            });
            let risky = ask(json!({"request": "StreamerzTemplateExport", "group": "Wheel"}))
                .await
                .unwrap();
            let risky = risky["content"].as_str().unwrap();
            e.change(|c| groups::remove(c, "Wheel"));
            let refused = ask(json!({"request": "StreamerzTemplateImport", "content": risky})).await;
            assert!(refused.unwrap_err().contains("allowPrograms") && !groups::exists(&e.config(), "Wheel"));
            let done = ask(json!({"request": "StreamerzTemplateImport", "content": risky, "allowPrograms": true}))
                .await
                .unwrap();
            assert_eq!(
                (done["group"].as_str(), done["template"]["installed"]["state"].as_str()),
                (Some("Wheel"), Some("new"))
            );
            assert_eq!(
                groups::read_file(&e.config(), "Wheel", "img/wheel.png").unwrap(),
                b"PNG"
            );
            assert!(ask(json!({"request": "StreamerzTemplateImport"})).await.is_err());
            assert!(
                ask(json!({"request": "StreamerzTemplateImport", "url": "http://example.com/x.streamerz"}))
                    .await
                    .unwrap_err()
                    .contains("https")
            );
            e.change(|c| groups::remove(c, "Wheel"));
            let _ = std::fs::remove_file(file);
        });
    }

    #[test]
    fn links_inside_the_group() {
        assert_eq!(resolve("a/b", "../c.png").as_deref(), Some("a/c.png"));
        assert_eq!(resolve("", "./x.css").as_deref(), Some("x.css"));
        assert_eq!(resolve("a", "../../x"), None);
        let html = r#"<a href="page2.html#top"></a><script src='js/a.js?v=2'></script><img src="data:image/png;base64,AA">
            <a href="/overlays"></a><a href="mailto:a@b.c"></a><div style="background:url('img/b.png')"></div>
            <img src="${dynamic}"><a href="//cdn.example/x.js"></a>"#;
        assert_eq!(links(html), ["page2.html", "js/a.js", "img/b.png"]);
    }
}
