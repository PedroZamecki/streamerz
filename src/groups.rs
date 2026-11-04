//! Reusable action groups: a name that holds actions, and with them typed variables (the group's
//! settings), the commands and timers it uses, and files such as an overlay. Values live in
//! `Config.action_group_variables` (read by actions as `%group.name%`); everything else about a group
//! is its [`Meta`] in `Config.action_groups`. Both travel in an export, so a group is shared whole.
use crate::model::Obj;
use crate::Config;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// What a variable holds. A duration is whole milliseconds, shown and typed as `h:mm:ss`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    #[default]
    Text,
    Integer,
    Number,
    Boolean,
    Duration,
    /// One of a list of words the creator gives.
    Choice,
}

pub const KINDS: &[Kind] = &[
    Kind::Text,
    Kind::Integer,
    Kind::Number,
    Kind::Boolean,
    Kind::Duration,
    Kind::Choice,
];

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Integer => "integer",
            Kind::Number => "number",
            Kind::Boolean => "boolean",
            Kind::Duration => "duration",
            Kind::Choice => "choice",
        }
    }

    pub fn accepts(self, v: &Value) -> bool {
        match self {
            Kind::Text | Kind::Choice => v.is_string(),
            Kind::Integer => v.as_i64().is_some(),
            Kind::Number => v.is_number(),
            Kind::Boolean => v.is_boolean(),
            Kind::Duration => v.as_u64().is_some(),
        }
    }

    /// A value typed as text (in a form), as this kind.
    pub fn parse(self, text: &str) -> Result<Value, String> {
        let t = text.trim();
        match self {
            Kind::Text | Kind::Choice => Ok(Value::String(text.to_string())),
            Kind::Integer => t
                .parse::<i64>()
                .map(Value::from)
                .map_err(|_| "not a whole number".into()),
            Kind::Number => t
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .map(|n| {
                    if n.fract() == 0.0 && n.abs() < 1e15 {
                        json!(n as i64)
                    } else {
                        json!(n)
                    }
                })
                .ok_or_else(|| "not a number".into()),
            Kind::Boolean => match t.to_ascii_lowercase().as_str() {
                "true" | "yes" | "on" | "1" => Ok(json!(true)),
                "false" | "no" | "off" | "0" => Ok(json!(false)),
                _ => Err("not true or false".into()),
            },
            Kind::Duration => parse_duration(t).map(Value::from).ok_or_else(|| "not a time".into()),
        }
    }

    /// A value as the text a form shows.
    pub fn show(self, v: &Value) -> String {
        match (self, v) {
            (_, Value::String(s)) => s.clone(),
            (Kind::Duration, v) => format_duration(v.as_u64().unwrap_or(0)),
            (_, v) => v.to_string(),
        }
    }
}

/// `h:mm:ss`, with `.mmm` when there are milliseconds.
pub fn format_duration(ms: u64) -> String {
    let base = format!("{}:{:02}:{:02}", ms / 3_600_000, ms / 60_000 % 60, ms / 1_000 % 60);
    if ms.is_multiple_of(1_000) {
        base
    } else {
        format!("{base}.{:03}", ms % 1_000)
    }
}

/// A time as `h:mm:ss[.mmm]`, `m:ss`, a plain number of seconds, or units such as `1h 30m 5s 200ms`.
pub fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.contains(':') {
        let (clock, frac) = s.split_once('.').unwrap_or((s, ""));
        if frac.len() > 3 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let parts: Vec<u64> = clock.split(':').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
        let (h, m, sec) = match parts[..] {
            [m, s] => (0, m, s),
            [h, m, s] => (h, m, s),
            _ => return None,
        };
        if (parts.len() == 3 && m >= 60) || sec >= 60 {
            return None;
        }
        let frac = if frac.is_empty() {
            0
        } else {
            frac.parse::<u64>().ok()? * 10u64.pow(3 - frac.len() as u32)
        };
        return h
            .checked_mul(3_600_000)?
            .checked_add(m.checked_mul(60_000)?)?
            .checked_add(sec * 1_000 + frac);
    }
    if let Ok(seconds) = s.parse::<f64>() {
        return (seconds.is_finite() && (0.0..1e12).contains(&seconds)).then(|| (seconds * 1000.0).round() as u64);
    }
    let (mut total, mut number, mut seen) = (0u64, String::new(), false);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() || c == '.' {
            number.push(c);
        } else if c.is_whitespace() {
            continue;
        } else {
            let mut unit = c.to_string();
            while let Some(n) = chars.peek().filter(|n| n.is_ascii_alphabetic()) {
                unit.push(*n);
                chars.next();
            }
            let per = match unit.to_ascii_lowercase().as_str() {
                "h" => 3_600_000.0,
                "m" => 60_000.0,
                "s" => 1_000.0,
                "ms" => 1.0,
                _ => return None,
            };
            let n: f64 = number.parse().ok()?;
            number.clear();
            total = total.checked_add((n * per).round() as u64)?;
            seen = true;
        }
    }
    (number.is_empty() && seen).then_some(total)
}

/// A setting of a group, as its creator declared it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Def {
    /// The name actions use: `%group.name%`.
    pub name: String,
    pub label: String,
    pub kind: Kind,
    pub default: Value,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// What a choice can be.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

impl Def {
    /// Whether `v` is a value of this setting: of its kind, and for a choice one of its options.
    pub fn accepts(&self, v: &Value) -> bool {
        self.kind.accepts(v)
            && (self.kind != Kind::Choice || v.as_str().is_some_and(|s| self.options.iter().any(|o| o == s)))
    }

    /// A value typed as text (in a form), as this setting's. A choice is matched to its option.
    pub fn parse(&self, text: &str) -> Result<Value, String> {
        if self.kind == Kind::Choice {
            return self
                .options
                .iter()
                .find(|o| o.eq_ignore_ascii_case(text.trim()))
                .map(|o| Value::String(o.clone()))
                .ok_or_else(|| format!("one of: {}", self.options.join(", ")));
        }
        self.kind.parse(text)
    }
}

/// A file that belongs to a group (an overlay, say). Content is base64, so any file round-trips.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct File {
    pub name: String,
    pub content: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Def>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<File>,
    /// Commands and timers (by id) the group uses besides those that name it as their group.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timers: Vec<String>,
}

/// Portable file names only: no path, no traversal, no Windows device names.
pub fn valid_file_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 255
        || name.ends_with('.')
        || name.contains("..")
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return false;
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return false;
    }
    !["COM", "LPT"].iter().any(|p| {
        stem.strip_prefix(p)
            .is_some_and(|n| n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9'))
    })
}

pub fn valid_variable_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl Meta {
    pub fn validate(&self) -> Result<(), String> {
        let mut names = HashSet::new();
        for d in &self.variables {
            if !valid_variable_name(&d.name) {
                return Err(format!("invalid variable name: {}", d.name));
            }
            if !names.insert(d.name.as_str()) {
                return Err(format!("duplicate variable: {}", d.name));
            }
            if d.label.trim().is_empty() {
                return Err(format!("variable has no label: {}", d.name));
            }
            if d.kind == Kind::Choice && d.options.is_empty() {
                return Err(format!("{} has no options to choose from", d.name));
            }
            if !d.accepts(&d.default) {
                return Err(format!("default of {} is not a {}", d.name, d.kind.name()));
            }
        }
        let mut files = HashSet::new();
        for f in &self.files {
            if !valid_file_name(&f.name) || !files.insert(f.name.to_ascii_lowercase()) {
                return Err(format!("invalid or duplicate file name: {}", f.name));
            }
            if base64::engine::general_purpose::STANDARD.decode(&f.content).is_err() {
                return Err(format!("invalid file content: {}", f.name));
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        *self == Meta::default()
    }
}

pub fn meta(c: &Config, group: &str) -> Meta {
    c.action_groups
        .get(group)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

pub fn set_meta(c: &mut Config, group: &str, meta: Meta) -> Result<(), String> {
    meta.validate()?;
    if meta.is_empty() {
        c.action_groups.remove(group);
    } else {
        c.action_groups
            .insert(group.into(), serde_json::to_value(&meta).map_err(|e| e.to_string())?);
    }
    Ok(())
}

/// Every setting of the group: the declared ones (their default until set), then the extra
/// variables actions have stored in it.
pub fn values(c: &Config, group: &str) -> Obj {
    let stored = c.action_group_variables.get(group).and_then(Value::as_object);
    let mut out = Obj::new();
    for d in meta(c, group).variables {
        let v = stored.and_then(|s| s.get(&d.name)).filter(|v| d.accepts(v));
        out.insert(d.name.clone(), v.cloned().unwrap_or(d.default));
    }
    for (k, v) in stored.into_iter().flatten() {
        out.entry(k.clone()).or_insert_with(|| v.clone());
    }
    out
}

/// Replace a group's stored values. A declared variable must keep its type.
pub fn set_values(c: &mut Config, group: &str, values: Obj) -> Result<(), String> {
    for d in meta(c, group).variables {
        if let Some(v) = values.get(&d.name).filter(|v| !d.accepts(v)) {
            return Err(format!("{} must be a {}, not {v}", d.name, d.kind.name()));
        }
    }
    c.action_group_variables.insert(group.into(), Value::Object(values));
    Ok(())
}

/// Ids of the group's commands and timers: those it lists, and commands whose group it is.
pub fn members(c: &Config, group: &str) -> (Vec<String>, Vec<String>) {
    let m = meta(c, group);
    let id = |o: &Obj| o.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    let mut commands = m.commands;
    commands.extend(
        c.commands
            .iter()
            .filter(|o| o.get("group").and_then(Value::as_str) == Some(group))
            .map(id),
    );
    let mut timers = m.timers;
    timers.extend(
        c.timers
            .iter()
            .filter(|o| o.get("group").and_then(Value::as_str) == Some(group))
            .map(id),
    );
    for list in [&mut commands, &mut timers] {
        let mut seen = HashSet::new();
        list.retain(|i| !i.is_empty() && seen.insert(i.clone()));
    }
    (commands, timers)
}

/// A new, empty group. False if the name is taken or empty.
pub fn create(c: &mut Config, group: &str) -> bool {
    if group.trim().is_empty() || exists(c, group) {
        return false;
    }
    c.actions.groups.push(group.into());
    true
}

pub fn exists(c: &Config, group: &str) -> bool {
    c.actions.groups.iter().any(|v| v.as_str() == Some(group)) || c.actions.actions.iter().any(|a| a.group == group)
}

/// Every group, by name.
pub fn names(c: &Config) -> Vec<String> {
    let mut all: Vec<String> = c
        .actions
        .groups
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    all.extend(
        c.actions
            .actions
            .iter()
            .map(|a| a.group.clone())
            .filter(|g| !g.is_empty()),
    );
    all.sort();
    all.dedup();
    all
}

/// Rename a group everywhere it is named. False if `old` is unknown or `new` is empty or taken.
pub fn rename(c: &mut Config, old: &str, new: &str) -> bool {
    if new.trim().is_empty() || old == new || !exists(c, old) || exists(c, new) {
        return false;
    }
    for a in &mut c.actions.actions {
        if a.group == old {
            a.group = new.into();
        }
    }
    for o in c.commands.iter_mut().chain(c.timers.iter_mut()) {
        if o.get("group").and_then(Value::as_str) == Some(old) {
            o.insert("group".into(), new.into());
        }
    }
    c.actions.groups.retain(|g| g.as_str() != Some(old));
    c.actions.groups.push(new.into());
    for map in [
        &mut c.action_group_variables,
        &mut c.action_group_icons,
        &mut c.action_groups,
    ] {
        if let Some(v) = map.remove(old) {
            map.insert(new.into(), v);
        }
    }
    true
}

/// Delete a group: its actions, commands and timers stay, without a group. Its settings, files
/// and icon go.
pub fn remove(c: &mut Config, group: &str) {
    for a in &mut c.actions.actions {
        if a.group == group {
            a.group.clear();
        }
    }
    for o in c.commands.iter_mut().chain(c.timers.iter_mut()) {
        if o.get("group").and_then(Value::as_str) == Some(group) {
            o.insert("group".into(), "".into());
        }
    }
    c.actions.groups.retain(|g| g.as_str() != Some(group));
    c.action_group_variables.remove(group);
    c.action_group_icons.remove(group);
    c.action_groups.remove(group);
}

/// Write a group's files into an existing folder. A file that is already there is never replaced.
pub fn save_files(files: &[File], folder: &Path) -> Result<Vec<PathBuf>, String> {
    use std::io::Write;
    let mut written = Vec::new();
    for f in files {
        if !valid_file_name(&f.name) {
            return Err(format!("invalid file name: {}", f.name));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&f.content)
            .map_err(|_| format!("invalid file content: {}", f.name))?;
        let path = folder.join(&f.name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("cannot create {}: {e}", f.name))?;
        file.write_all(&bytes)
            .map_err(|e| format!("cannot write {}: {e}", f.name))?;
        written.push(path);
    }
    Ok(written)
}

/// A file made from text.
pub fn text_file(name: &str, text: &str) -> File {
    File {
        name: name.into(),
        content: base64::engine::general_purpose::STANDARD.encode(text),
    }
}

/// `%group.name%` replaced by the group's values, nothing else expanded.
pub fn resolve(c: &Config, group: &str, text: &str) -> String {
    if !text.contains("%group.") {
        return text.to_string();
    }
    let mut out = text.to_string();
    for (k, v) in values(c, group) {
        let s = match v {
            Value::String(s) => s,
            v => v.to_string(),
        };
        out = out.replace(&format!("%group.{k}%"), &s);
    }
    out
}

/// What an export carries about the groups its actions are in (see `edit::export`).
pub fn export_meta(c: &Config, groups: &[String]) -> Obj {
    groups
        .iter()
        .filter_map(|g| c.action_groups.get(g).map(|m| (g.clone(), m.clone())))
        .collect()
}

/// Take in what an export carries: each imported group's description, settings, files and
/// members replace the ones here; the values already set stay unless the import has the same name.
pub fn import_meta(c: &mut Config, groups: &Obj) -> Result<(), String> {
    let mut parsed = Vec::new();
    for (g, v) in groups {
        let m: Meta = serde_json::from_value(v.clone()).map_err(|e| format!("invalid group {g}: {e}"))?;
        m.validate().map_err(|e| format!("group {g}: {e}"))?;
        parsed.push((g.clone(), m));
    }
    for (g, m) in parsed {
        if !c.actions.groups.iter().any(|v| v.as_str() == Some(&g)) {
            c.actions.groups.push(g.clone().into());
        }
        let stored = c.action_group_variables.entry(g.clone()).or_insert_with(|| json!({}));
        if let Some(stored) = stored.as_object_mut() {
            for d in &m.variables {
                if !stored.get(&d.name).is_some_and(|v| d.accepts(v)) {
                    stored.insert(d.name.clone(), d.default.clone());
                }
            }
        }
        set_meta(c, &g, m)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, kind: Kind, default: Value) -> Def {
        Def {
            name: name.into(),
            label: name.into(),
            kind,
            default,
            ..Default::default()
        }
    }

    fn config() -> Config {
        let mut c = Config::default();
        assert!(create(&mut c, "Timer"));
        let m = Meta {
            variables: vec![
                def("start", Kind::Duration, json!(60_000)),
                def("label", Kind::Text, json!("hi")),
            ],
            files: vec![text_file("a.html", "<p>x</p>")],
            ..Default::default()
        };
        set_meta(&mut c, "Timer", m).unwrap();
        c
    }

    #[test]
    fn durations_parse_and_show() {
        for (text, ms) in [
            ("1:30:00", 5_400_000),
            ("2:05", 125_000),
            ("90", 90_000),
            ("1h 30m", 5_400_000),
            ("5s 200ms", 5_200),
            ("0:00:01.5", 1_500),
            ("1.5s", 1_500),
        ] {
            assert_eq!(parse_duration(text), Some(ms), "{text}");
        }
        for bad in ["", "abc", "1:75:00", "1:2:3:4", "-5", "1x", "1h30"] {
            assert_eq!(parse_duration(bad), None, "{bad}");
        }
        assert_eq!(format_duration(5_400_000), "1:30:00");
        assert_eq!(format_duration(1_500), "0:00:01.500");
        assert_eq!(parse_duration(&format_duration(123_456)), Some(123_456));
    }

    #[test]
    fn kinds_parse_typed_text() {
        assert_eq!(Kind::Integer.parse(" 7 "), Ok(json!(7)));
        assert!(Kind::Integer.parse("1.5").is_err());
        assert_eq!(Kind::Number.parse("2.5"), Ok(json!(2.5)));
        assert_eq!(Kind::Boolean.parse("Yes"), Ok(json!(true)));
        assert_eq!(Kind::Duration.parse("1:00"), Ok(json!(60_000)));
        assert!(Kind::Duration.parse("x").is_err());
        assert_eq!(Kind::Duration.show(&json!(61_000)), "0:01:01");
    }

    #[test]
    fn values_default_until_set_and_keep_types() {
        let mut c = config();
        assert_eq!(values(&c, "Timer")["start"], 60_000);
        let mut v = Obj::new();
        v.insert("start".into(), json!(5_000));
        v.insert("extra".into(), json!(1));
        set_values(&mut c, "Timer", v).unwrap();
        let got = values(&c, "Timer");
        assert_eq!(
            (got["start"].clone(), got["label"].clone(), got["extra"].clone()),
            (json!(5_000), json!("hi"), json!(1))
        );
        let mut bad = Obj::new();
        bad.insert("start".into(), json!("soon"));
        assert!(set_values(&mut c, "Timer", bad).is_err());
        // A stored value of the wrong type falls back to the default.
        c.action_group_variables
            .insert("Timer".into(), json!({"start": "soon"}));
        assert_eq!(values(&c, "Timer")["start"], 60_000);
        assert_eq!(
            resolve(&c, "Timer", "%group.start%/%group.label%/%group.none%"),
            "60000/hi/%group.none%"
        );
    }

    #[test]
    fn validation_refuses_bad_names_defaults_and_files() {
        let mut m = config().action_groups["Timer"].clone();
        let ok: Meta = serde_json::from_value(m.take()).unwrap();
        ok.validate().unwrap();
        let mut bad = ok.clone();
        bad.variables.push(def("start", Kind::Text, json!("")));
        assert!(bad.validate().is_err());
        let mut bad = ok.clone();
        bad.variables[0].default = json!("x");
        assert!(bad.validate().is_err());
        let mut bad = ok.clone();
        bad.variables[0].name = "a b".into();
        assert!(bad.validate().is_err());
        for name in [
            "../x", "/x", "C:x", "a\\b", "a/b", "NUL.html", "COM1.txt", "x.", "..", "",
        ] {
            let mut bad = ok.clone();
            bad.files[0].name = name.into();
            assert!(bad.validate().is_err(), "{name}");
        }
        let mut bad = ok.clone();
        bad.files.push(text_file("A.HTML", ""));
        assert!(bad.validate().is_err());
        let mut bad = ok;
        bad.files[0].content = "not base64!".into();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn rename_and_remove_move_everything() {
        let mut c = config();
        c.commands.push(
            json!({"id": "c1", "name": "n", "group": "Timer"})
                .as_object()
                .unwrap()
                .clone(),
        );
        c.timers
            .push(json!({"id": "t1", "name": "n"}).as_object().unwrap().clone());
        let mut m = meta(&c, "Timer");
        m.timers = vec!["t1".into()];
        set_meta(&mut c, "Timer", m).unwrap();
        assert_eq!(members(&c, "Timer"), (vec!["c1".to_string()], vec!["t1".to_string()]));
        c.action_group_variables.insert("Timer".into(), json!({"start": 1}));
        assert!(!rename(&mut c, "Timer", "Timer"));
        assert!(rename(&mut c, "Timer", "Clock"));
        assert!(!exists(&c, "Timer"));
        assert_eq!(values(&c, "Clock")["start"], 1);
        assert_eq!(meta(&c, "Clock").files.len(), 1);
        assert_eq!(c.commands[0]["group"], "Clock");
        remove(&mut c, "Clock");
        assert!(!exists(&c, "Clock") && c.action_groups.is_empty() && c.action_group_variables.is_empty());
        assert_eq!(c.commands[0]["group"], "");
        assert_eq!(c.timers.len(), 1);
    }

    #[test]
    fn import_keeps_set_values_and_fills_the_rest() {
        let src = config();
        let exported = export_meta(&src, &["Timer".to_string()]);
        let mut dst = Config::default();
        dst.action_group_variables
            .insert("Timer".into(), json!({"start": 9_000}));
        import_meta(&mut dst, &exported).unwrap();
        let v = values(&dst, "Timer");
        assert_eq!((v["start"].clone(), v["label"].clone()), (json!(9_000), json!("hi")));
        assert!(exists(&dst, "Timer"));
        assert_eq!(meta(&dst, "Timer"), meta(&src, "Timer"));
        let mut bad = exported;
        bad["Timer"]["variables"][0]["default"] = json!("x");
        assert!(import_meta(&mut dst, &bad).is_err());
    }

    #[test]
    fn files_are_saved_once_inside_the_folder() {
        let dir = std::env::temp_dir().join(format!("streamerz-groups-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let files = meta(&config(), "Timer").files;
        assert_eq!(save_files(&files, &dir).unwrap(), vec![dir.join("a.html")]);
        assert_eq!(std::fs::read(dir.join("a.html")).unwrap(), b"<p>x</p>");
        assert!(save_files(&files, &dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
