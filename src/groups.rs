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
    /// A colour as `#rrggbb`, or `#rrggbbaa` when it is see-through: picked, not typed.
    Color,
    /// A number between the setting's `min` and `max`, set by dragging.
    Slider,
    /// Text of several lines: a list with one item per line.
    Lines,
    /// The name of a font, picked from the ones installed on this computer (empty: the system's).
    Font,
    /// One of the group's own files (a picture, a sound, a video), or a web address; empty for none.
    /// A page gets it as an address it can load.
    File,
    /// Some of the platforms: a list of `twitch`, `youtube`, `kick`, `tiktok`.
    Platforms,
    /// One of the actions connected to the overlay, by its id (empty: none).
    Action,
    /// Rows of the columns the setting declares: a list of objects.
    List,
}

/// The platforms a `platforms` setting picks from, in the order the form shows them.
pub const PLATFORMS: [&str; 4] = ["twitch", "youtube", "kick", "tiktok"];

impl Kind {
    /// Every kind, in the order the guide lists them (the guide's test goes through them).
    #[cfg(test)]
    pub const ALL: [Kind; 14] = [
        Kind::Text,
        Kind::Lines,
        Kind::Integer,
        Kind::Number,
        Kind::Slider,
        Kind::Boolean,
        Kind::Choice,
        Kind::Color,
        Kind::Font,
        Kind::Duration,
        Kind::File,
        Kind::Platforms,
        Kind::Action,
        Kind::List,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Integer => "integer",
            Kind::Number => "number",
            Kind::Boolean => "boolean",
            Kind::Duration => "duration",
            Kind::Choice => "choice",
            Kind::Color => "color",
            Kind::Slider => "slider",
            Kind::Lines => "lines",
            Kind::Font => "font",
            Kind::File => "file",
            Kind::Platforms => "platforms",
            Kind::Action => "action",
            Kind::List => "list",
        }
    }

    pub fn accepts(self, v: &Value) -> bool {
        match self {
            Kind::Text | Kind::Choice | Kind::Lines | Kind::Font | Kind::File | Kind::Action => v.is_string(),
            Kind::Platforms => v
                .as_array()
                .is_some_and(|list| list.iter().all(|p| p.as_str().is_some_and(|p| PLATFORMS.contains(&p)))),
            // The rows are checked against the columns by the setting (`Def::accepts`).
            Kind::List => v.as_array().is_some_and(|rows| rows.iter().all(Value::is_object)),
            Kind::Color => v.as_str().is_some_and(is_color),
            Kind::Integer => v.as_i64().is_some(),
            Kind::Number | Kind::Slider => v.is_number(),
            Kind::Boolean => v.is_boolean(),
            Kind::Duration => v.as_u64().is_some(),
        }
    }

    /// A value typed as text (in a form), as this kind.
    pub fn parse(self, text: &str) -> Result<Value, String> {
        let t = text.trim();
        match self {
            Kind::Text | Kind::Choice | Kind::Lines | Kind::Font => Ok(Value::String(text.to_string())),
            Kind::File | Kind::Action => Ok(Value::String(t.to_string())),
            Kind::Platforms => {
                let named: Vec<String> = t
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|p| !p.is_empty())
                    .map(str::to_ascii_lowercase)
                    .collect();
                match named.iter().find(|p| !PLATFORMS.contains(&p.as_str())) {
                    Some(unknown) => Err(format!("{unknown} is not one of {}", PLATFORMS.join(", "))),
                    None => Ok(json!(named)),
                }
            }
            // Rows need the setting's columns: `Def::parse` reads them.
            Kind::List => Err("a list is read by its setting".into()),
            Kind::Color => Some(t.to_ascii_lowercase())
                .filter(|c| is_color(c))
                .map(Value::String)
                .ok_or_else(|| "not a colour (#rrggbb)".into()),
            Kind::Integer => t
                .parse::<i64>()
                .map(Value::from)
                .map_err(|_| "not a whole number".into()),
            Kind::Number | Kind::Slider => t
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
            (Kind::Platforms, Value::Array(list)) => {
                list.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")
            }
            (_, v) => v.to_string(),
        }
    }
}

/// `#rrggbb` or `#rrggbbaa`.
pub fn is_color(text: &str) -> bool {
    text.strip_prefix('#')
        .is_some_and(|hex| matches!(hex.len(), 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()))
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
    /// The ends of a slider and how far one step moves it (0 to 100 by 1 when not given).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// A heading drawn above this setting in the form: the settings from here on belong to it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub section: String,
    /// Shown in the form only while another setting (a choice or a check box) has this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_if: Option<ShowIf>,
    /// What a `file` may be: `image`, `sound`, `video` or `font` (empty: any file).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub accept: String,
    /// The columns of a `list`: each a setting of a simple kind (its `default` fills a new row).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<Def>,
}

/// The kinds a column of a `list` can be: what fits in one cell of a line.
pub const COLUMN_KINDS: [Kind; 8] = [
    Kind::Text,
    Kind::Integer,
    Kind::Number,
    Kind::Slider,
    Kind::Boolean,
    Kind::Choice,
    Kind::Color,
    Kind::Duration,
];

/// What separates the cells of a row typed as a line: `Pizza | 2 | #ff0000`.
pub const CELLS: &str = "|";

/// The file extensions each `accept` of a `file` setting stands for.
pub fn accepted(accept: &str) -> &'static [&'static str] {
    match accept {
        "image" => &["png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "apng"],
        "sound" => &["mp3", "ogg", "wav", "m4a", "flac", "opus", "aac"],
        "video" => &["mp4", "webm", "mov", "mkv", "m4v"],
        "font" => &["ttf", "otf", "woff", "woff2"],
        _ => &[],
    }
}

/// Whether a file's name is one a `file` setting with this `accept` takes.
pub fn accepts_file(accept: &str, name: &str) -> bool {
    let wanted = accepted(accept);
    let extension = name.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
    wanted.is_empty() || wanted.contains(&extension.as_str())
}

/// When a setting is worth showing: `{"setting": "look", "is": "Custom color"}`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShowIf {
    pub setting: String,
    pub is: Value,
}

impl Def {
    /// Whether `v` is a value of this setting: of its kind, and for a choice one of its options.
    pub fn accepts(&self, v: &Value) -> bool {
        self.kind.accepts(v)
            && (self.kind != Kind::Choice || v.as_str().is_some_and(|s| self.options.iter().any(|o| o == s)))
            && (self.kind != Kind::Slider
                || v.as_f64()
                    .is_some_and(|n| (self.range().0..=self.range().1).contains(&n)))
            && (self.kind != Kind::List
                || v.as_array().into_iter().flatten().all(|row| {
                    self.columns
                        .iter()
                        .all(|column| row.get(&column.name).is_some_and(|cell| column.accepts(cell)))
                }))
    }

    /// A slider's ends and step.
    pub fn range(&self) -> (f64, f64, f64) {
        let (min, max) = (self.min.unwrap_or(0.0), self.max.unwrap_or(100.0));
        (min, max.max(min), self.step.filter(|s| *s > 0.0).unwrap_or(1.0))
    }

    /// A value typed as text (in a form), as this setting's. A choice is matched to its option, and a
    /// list is read a row a line, its cells between bars (a cell left out takes the column's default).
    pub fn parse(&self, text: &str) -> Result<Value, String> {
        if self.kind == Kind::Choice {
            return self
                .options
                .iter()
                .find(|o| o.eq_ignore_ascii_case(text.trim()))
                .map(|o| Value::String(o.clone()))
                .ok_or_else(|| format!("one of: {}", self.options.join(", ")));
        }
        if self.kind == Kind::List {
            let mut rows = Vec::new();
            for (at, line) in text.lines().enumerate().filter(|(_, line)| !line.trim().is_empty()) {
                let cells: Vec<&str> = line.split(CELLS).map(str::trim).collect();
                if cells.len() > self.columns.len() {
                    return Err(format!("line {}: more than {} columns", at + 1, self.columns.len()));
                }
                let mut row = serde_json::Map::new();
                for (column, cell) in self
                    .columns
                    .iter()
                    .zip(cells.iter().map(Some).chain(std::iter::repeat(None)))
                {
                    let value = match cell.filter(|cell| !cell.is_empty()) {
                        Some(cell) => column
                            .parse(cell)
                            .map_err(|error| format!("line {}, {}: {error}", at + 1, column.label))?,
                        None => column.default.clone(),
                    };
                    row.insert(column.name.clone(), value);
                }
                rows.push(Value::Object(row));
            }
            return Ok(Value::Array(rows));
        }
        let value = self.kind.parse(text)?;
        if self.kind == Kind::Slider && !self.accepts(&value) {
            return Err(format!("a number from {} to {}", self.range().0, self.range().1));
        }
        Ok(value)
    }

    /// A value as the text a form shows: a list as its rows, a line each.
    pub fn show(&self, v: &Value) -> String {
        if self.kind != Kind::List {
            return self.kind.show(v);
        }
        v.as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                self.columns
                    .iter()
                    .map(|column| {
                        row.get(&column.name)
                            .map(|cell| column.kind.show(cell))
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join(&format!(" {CELLS} "))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A file that belongs to a group (an overlay, say). It lives on disk, in the group's folder (see
/// [`folder`]); `name` may have folders (`img/wheel.png`). `content` (base64, so any file round-trips)
/// is only filled on the way in and out: in an export, in a template, in a request that adds the file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct File {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// Bytes on disk.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub size: u64,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// A page of a group that is meant to be shown: what the Overlays page lists and adds to OBS.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// One of the group's files.
    pub file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// The size of the browser source, in pixels (0: OBS's own).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub width: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub height: u32,
    /// Made for a dock of OBS (a panel with buttons), not for a source in a scene.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dock: bool,
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

/// What a group says about itself when it is shared as a template or an overlay (`package.rs`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    /// The same in every version of the template, so a newer file updates the one installed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// Where it comes from: its page, its repository.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
    /// One of the group's files: a picture of it, shown before it is imported.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub picture: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<Page>,
    /// What has to be set up for it to work: twitch, youtube, kick, tiktok, obs, spotify.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// What Test on the Overlays page sends its pages besides the usual made-up chat, events and
    /// song: `{"topics": {name: value}, "events": [[name, data]]}`.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub test: Value,
}

pub const REQUIRES: &[&str] = &["twitch", "youtube", "kick", "tiktok", "obs", "spotify"];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    /// The name of the group's folder on disk, given here when it gets its first file. It stays through
    /// a rename and never travels in an export.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub store: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Def>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<File>,
    /// What the group's pages may ask Streamerz to do (`overlays::PERMISSIONS`). Nothing by default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub template: Template,
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

/// A file of a group: portable names, in up to eight folders (`img/wheel.png`).
pub fn valid_file_path(name: &str) -> bool {
    name.len() <= 255 && name.split('/').count() <= 8 && name.split('/').all(valid_file_name)
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
            if d.kind == Kind::List {
                if d.columns.is_empty() {
                    return Err(format!("{} is a list with no columns", d.name));
                }
                let mut columns = HashSet::new();
                for column in &d.columns {
                    if !valid_variable_name(&column.name) || !columns.insert(column.name.as_str()) {
                        return Err(format!("{}: invalid or duplicate column {}", d.name, column.name));
                    }
                    if column.label.trim().is_empty() || column.label.contains(CELLS) {
                        return Err(format!(
                            "{}: the column {} needs a label without a bar",
                            d.name, column.name
                        ));
                    }
                    if !COLUMN_KINDS.contains(&column.kind) {
                        return Err(format!(
                            "{}: a column can't be a {} ({})",
                            d.name,
                            column.kind.name(),
                            column.name
                        ));
                    }
                    if column.kind == Kind::Choice && column.options.is_empty() {
                        return Err(format!("{}: the column {} has no options", d.name, column.name));
                    }
                    if !column.accepts(&column.default) {
                        return Err(format!(
                            "{}: the default of the column {} is not a {}",
                            d.name,
                            column.name,
                            column.kind.name()
                        ));
                    }
                }
            } else if !d.columns.is_empty() {
                return Err(format!("{} has columns but is not a list", d.name));
            }
            if !d.accept.is_empty() && (d.kind != Kind::File || accepted(&d.accept).is_empty()) {
                return Err(format!("{}: accept is image, sound, video or font, on a file", d.name));
            }
            if !d.accepts(&d.default) {
                return Err(format!("default of {} is not a {}", d.name, d.kind.name()));
            }
            if let Some(when) = &d.show_if {
                let known = self
                    .variables
                    .iter()
                    .any(|other| other.name == when.setting && other.name != d.name && other.accepts(&when.is));
                if !known {
                    return Err(format!(
                        "{} is shown by {}, which is not a setting with that value",
                        d.name, when.setting
                    ));
                }
            }
        }
        if let Some(p) = self
            .permissions
            .iter()
            .find(|p| !crate::overlays::PERMISSIONS.iter().any(|(k, _)| k == p))
        {
            return Err(format!("unknown permission: {p}"));
        }
        let mut files = HashSet::new();
        for f in &self.files {
            if !valid_file_path(&f.name) || !files.insert(f.name.to_ascii_lowercase()) {
                return Err(format!("invalid or duplicate file name: {}", f.name));
            }
            if base64::engine::general_purpose::STANDARD.decode(&f.content).is_err() {
                return Err(format!("invalid file content: {}", f.name));
            }
        }
        let t = &self.template;
        let listed = |name: &str| self.files.iter().any(|f| f.name == name);
        if let Some(p) = t.pages.iter().find(|p| !listed(&p.file)) {
            return Err(format!("the page {} is not one of the files", p.file));
        }
        if !t.picture.is_empty() && !listed(&t.picture) {
            return Err(format!("the picture {} is not one of the files", t.picture));
        }
        if let Some(r) = t.requires.iter().find(|r| !REQUIRES.contains(&r.as_str())) {
            return Err(format!("unknown requirement: {r}"));
        }
        if !t.link.is_empty() && !t.link.starts_with("https://") && !t.link.starts_with("http://") {
            return Err("the link must be a web address".into());
        }
        if [&t.id, &t.version, &t.author, &t.link].iter().any(|s| s.len() > 300) {
            return Err("a template's id, version, author and link are short texts".into());
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

/// Where the files of every group are: `groups/<store>/` beside the settings.
#[cfg(not(test))]
pub fn root() -> PathBuf {
    crate::config_path().with_file_name("groups")
}

/// In tests the settings' place can change while they run (a test starts a development copy): the
/// files of every test stay in one folder of this run.
#[cfg(test)]
pub fn root() -> PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| std::env::temp_dir().join(format!("streamerz-test-groups-{}", std::process::id())))
        .clone()
}

/// The folder of a group's files, when it has any.
pub fn folder(c: &Config, group: &str) -> Option<PathBuf> {
    Some(meta(c, group).store)
        .filter(|s| valid_file_name(s))
        .map(|s| root().join(s))
}

/// Where a file of a group is on disk.
pub fn file_path(c: &Config, group: &str, name: &str) -> Option<PathBuf> {
    let m = meta(c, group);
    if !valid_file_name(&m.store) {
        return None;
    }
    let path = root().join(m.store).join(name);
    // One of its files, or one put in its folder by hand that the list has not caught up with
    // (`sync_files`): a portable name can't leave the folder.
    let listed = m.files.iter().any(|f| f.name == name);
    (listed || (valid_file_path(name) && path.is_file())).then_some(path)
}

/// A file's bytes: from disk, or from `content` when it has not been written yet.
pub fn read_file(c: &Config, group: &str, name: &str) -> Option<Vec<u8>> {
    let m = meta(c, group);
    let Some(f) = m.files.iter().find(|f| f.name == name) else {
        // Put in the folder by hand, and not yet in the list.
        return std::fs::read(file_path(c, group, name)?).ok();
    };
    if !f.content.is_empty() {
        return base64::engine::general_purpose::STANDARD.decode(&f.content).ok();
    }
    if !valid_file_name(&m.store) {
        return Some(Vec::new());
    }
    std::fs::read(root().join(&m.store).join(name)).ok()
}

/// The group's meta with every file's content inside, as an export or a client that asks carries it.
pub fn meta_with_files(c: &Config, group: &str) -> Meta {
    let mut m = meta(c, group);
    let dir = Some(&m.store).filter(|s| valid_file_name(s)).map(|s| root().join(s));
    for f in &mut m.files {
        if f.content.is_empty() {
            if let Some(bytes) = dir.as_ref().and_then(|d| std::fs::read(d.join(&f.name)).ok()) {
                f.content = base64::engine::general_purpose::STANDARD.encode(bytes);
            }
        }
    }
    m.store.clear();
    m
}

/// The files in a folder and the folders under it, as names with `/` between: only the ones a group
/// can hold (portable names), not an editor's scratch files.
fn files_under(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<(String, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.ends_with('~') || name.ends_with(".streamerz-writing") {
            continue;
        }
        let path = format!("{prefix}{name}");
        match entry.metadata() {
            Ok(m) if m.is_dir() && depth < 7 => files_under(&entry.path(), &format!("{path}/"), depth + 1, out),
            Ok(m) if m.is_file() && valid_file_path(&path) => out.push((path, m.len())),
            _ => {}
        }
    }
}

/// The group's meta as its folder says it should be, when the list of files differs from what is
/// there: a file put there by hand (a picture dropped in, a page written in an editor) is one of the
/// group's, and one deleted there is forgotten. None when they agree.
pub fn synced(c: &Config, group: &str) -> Option<Meta> {
    let mut m = meta(c, group);
    let dir = folder(c, group).filter(|dir| dir.is_dir())?;
    let mut found = Vec::new();
    files_under(&dir, "", 0, &mut found);
    found.sort();
    let before: Vec<(String, u64)> = m.files.iter().map(|f| (f.name.clone(), f.size)).collect();
    // The order the files have stays; new ones go at the end.
    m.files.retain(|f| found.iter().any(|(name, _)| *name == f.name));
    for (name, size) in &found {
        match m.files.iter_mut().find(|f| f.name == *name) {
            Some(f) => f.size = *size,
            None => m.files.push(File {
                name: name.clone(),
                content: String::new(),
                size: *size,
            }),
        }
    }
    let after: Vec<(String, u64)> = m.files.iter().map(|f| (f.name.clone(), f.size)).collect();
    if before == after {
        return None;
    }
    // A page that is gone is listed no longer; what the list still names is as it was.
    let have = |name: &str| m.files.iter().any(|f| f.name == name);
    let pages: Vec<Page> = m.template.pages.iter().filter(|p| have(&p.file)).cloned().collect();
    m.template.pages = pages;
    if !have(&m.template.picture) {
        m.template.picture.clear();
    }
    Some(m)
}

/// Bring the group's list of files in step with its folder (see `synced`). True when it changed.
pub fn sync_files(c: &mut Config, group: &str) -> bool {
    let Some(m) = synced(c, group) else { return false };
    match serde_json::to_value(&m) {
        Ok(v) => {
            c.action_groups.insert(group.into(), v);
            true
        }
        Err(_) => false,
    }
}

/// Write the files that came with content to the group's folder, and take away the ones it listed
/// before (`was`) and no longer does. A file named without content is one already there (an empty one
/// when it isn't). What someone put in the folder by hand is never deleted here: it was not listed.
fn store_files(meta: &mut Meta, was: &[File]) -> Result<(), String> {
    if meta.files.is_empty() {
        if valid_file_name(&meta.store) {
            let _ = std::fs::remove_dir_all(root().join(&meta.store));
        }
        meta.store.clear();
        return Ok(());
    }
    if !valid_file_name(&meta.store) {
        meta.store = crate::engine::uuid();
    }
    let dir = root().join(&meta.store);
    for f in &mut meta.files {
        let path = dir.join(&f.name);
        let failed = |e: std::io::Error| format!("cannot write {}: {e}", f.name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        if !f.content.is_empty() {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&f.content)
                .map_err(|_| format!("invalid file content: {}", f.name))?;
            // Whole or not at all: a page being served is never read half written.
            let pending = path.with_extension("streamerz-writing");
            std::fs::write(&pending, &bytes)
                .and_then(|_| std::fs::rename(&pending, &path))
                .map_err(failed)?;
            f.size = bytes.len() as u64;
            f.content = String::new();
        } else if let Ok(m) = std::fs::metadata(&path) {
            f.size = m.len();
        } else {
            std::fs::write(&path, b"").map_err(failed)?;
            f.size = 0;
        }
    }
    for f in was {
        if valid_file_path(&f.name) && !meta.files.iter().any(|kept| kept.name == f.name) {
            let _ = std::fs::remove_file(dir.join(&f.name));
        }
    }
    prune(&dir);
    Ok(())
}

/// Delete the folders under `dir` that are left empty.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            prune(&entry.path());
            let _ = std::fs::remove_dir(entry.path());
        }
    }
}

/// Files kept inside the settings by older versions go to disk. True when something moved.
pub fn files_to_disk(c: &mut Config) -> bool {
    let mut moved = false;
    for group in c.action_groups.keys().cloned().collect::<Vec<_>>() {
        let m = meta(c, &group);
        if m.files.iter().any(|f| !f.content.is_empty()) {
            match set_meta(c, &group, m) {
                Ok(()) => moved = true,
                Err(e) => crate::warn(format!("Files of group {group} stay in the settings: {e}")),
            }
        }
    }
    moved
}

pub fn set_meta(c: &mut Config, group: &str, mut meta: Meta) -> Result<(), String> {
    meta.validate()?;
    // The folder is this installation's: whatever came in, the group keeps its own.
    let was = self::meta(c, group);
    meta.store = was.store;
    store_files(&mut meta, &was.files)?;
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

/// A new, empty group of actions. False if the name is empty or a group of actions has it. An overlay
/// of that name becomes connected to the new group.
pub fn create(c: &mut Config, group: &str) -> bool {
    if group.trim().is_empty() || listed(c, group) {
        return false;
    }
    c.actions.groups.push(group.into());
    true
}

/// Whether the name is a group of actions: what the Actions page lists.
pub fn listed(c: &Config, group: &str) -> bool {
    c.actions.groups.iter().any(|v| v.as_str() == Some(group)) || c.actions.actions.iter().any(|a| a.group == group)
}

/// Whether the name is taken: by a group of actions, or by an overlay that has none.
pub fn exists(c: &Config, group: &str) -> bool {
    listed(c, group) || c.action_groups.contains_key(group)
}

/// Whether the group has a page: it is an overlay, with or without actions.
pub fn has_overlay(c: &Config, group: &str) -> bool {
    meta(c, group).files.iter().any(|f| is_page(&f.name))
}

/// A file that is shown as a page.
pub fn is_page(file: &str) -> bool {
    file.to_ascii_lowercase().ends_with(".html")
}

/// The overlay and the group of actions of the same name are one thing in two places: the pages run
/// those actions and both read the same settings. This says so for a name.
pub fn connected(c: &Config, group: &str) -> bool {
    listed(c, group) && has_overlay(c, group)
}

/// How many actions, commands and timers a group holds.
pub fn counts(c: &Config, group: &str) -> (usize, usize, usize) {
    let (commands, timers) = members(c, group);
    (
        c.actions.actions.iter().filter(|a| a.group == group).count(),
        commands.len(),
        timers.len(),
    )
}

/// Every group of actions, by name.
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

/// Every name in use: the groups of actions, and the overlays that have none.
pub fn all(c: &Config) -> Vec<String> {
    let mut all = names(c);
    all.extend(c.action_groups.keys().cloned());
    all.sort();
    all.dedup();
    all
}

/// Rename a group everywhere it is named. False if `old` is unknown or `new` is empty or taken.
pub fn rename(c: &mut Config, old: &str, new: &str) -> bool {
    if new.trim().is_empty() || old == new || !exists(c, old) || exists(c, new) {
        return false;
    }
    let was_listed = listed(c, old);
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
    if was_listed {
        c.actions.groups.push(new.into());
    }
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

/// Take a name off the groups of actions: its actions, commands and timers stay, without a group.
fn unlist(c: &mut Config, group: &str) {
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
    c.action_group_icons.remove(group);
}

/// Delete a group and all that is its own: its actions, commands and timers stay, without a group;
/// its settings, files (an overlay's pages too) and icon go.
pub fn remove(c: &mut Config, group: &str) {
    unlist(c, group);
    c.action_group_variables.remove(group);
    if let Some(dir) = folder(c, group) {
        let _ = std::fs::remove_dir_all(dir);
    }
    c.action_groups.remove(group);
}

/// Delete a group of actions, as the Actions page does. A connected overlay is not the group's to
/// delete: it stays, with its pages and settings, as an overlay with no actions (true is returned).
pub fn ungroup(c: &mut Config, group: &str) -> bool {
    if has_overlay(c, group) {
        unlist(c, group);
        let mut m = meta(c, group);
        m.commands.clear();
        m.timers.clear();
        // What is left was valid before and still is.
        let _ = set_meta(c, group, m);
        true
    } else {
        remove(c, group);
        false
    }
}

/// Connect an overlay to a group of actions. With no `group` (or the overlay's own name) the overlay
/// gets an empty group of its name. Else that group's actions, commands and timers move under the
/// overlay's name (a page's address has the name in it, so the overlay keeps it), with the settings
/// the overlay doesn't already have.
pub fn connect(c: &mut Config, overlay: &str, group: &str) -> Result<(), String> {
    if !has_overlay(c, overlay) {
        return Err(format!("no overlay named {overlay}"));
    }
    if group.is_empty() || group == overlay {
        if !listed(c, overlay) {
            c.actions.groups.push(overlay.into());
        }
        return Ok(());
    }
    if !listed(c, group) {
        return Err(format!("no group named {group}"));
    }
    if has_overlay(c, group) {
        return Err(format!("{group} is connected to an overlay of its own"));
    }
    let (from, mut into) = (meta(c, group), meta(c, overlay));
    for d in from.variables {
        if !into.variables.iter().any(|have| have.name == d.name) {
            into.variables.push(d);
        }
    }
    for (list, more) in [(&mut into.commands, from.commands), (&mut into.timers, from.timers)] {
        for id in more {
            if !list.contains(&id) {
                list.push(id);
            }
        }
    }
    if into.description.is_empty() {
        into.description = from.description;
    }
    // The group's values come along where the overlay has none of its own (a setting both declare
    // keeps the overlay's kind, so a value of another kind is left behind).
    let mut merged = values(c, group);
    merged.retain(|name, v| {
        into.variables
            .iter()
            .find(|d| d.name == *name)
            .is_none_or(|d| d.accepts(v))
    });
    if let Some(own) = c.action_group_variables.get(overlay).and_then(Value::as_object) {
        merged.extend(own.clone());
    }
    set_meta(c, overlay, into)?;
    set_values(c, overlay, merged)?;
    for a in &mut c.actions.actions {
        if a.group == group {
            a.group = overlay.into();
        }
    }
    for o in c.commands.iter_mut().chain(c.timers.iter_mut()) {
        if o.get("group").and_then(Value::as_str) == Some(group) {
            o.insert("group".into(), overlay.into());
        }
    }
    if let Some(icon) = c.action_group_icons.remove(group) {
        c.action_group_icons.entry(overlay).or_insert(icon);
    }
    c.actions
        .groups
        .retain(|g| g.as_str() != Some(group) && g.as_str() != Some(overlay));
    c.actions.groups.push(overlay.into());
    c.action_group_variables.remove(group);
    c.action_groups.remove(group);
    Ok(())
}

/// Write a group's files into an existing folder. A file that is already there is never replaced.
pub fn save_files(c: &Config, group: &str, folder: &Path) -> Result<Vec<PathBuf>, String> {
    use std::io::Write;
    let mut written = Vec::new();
    for f in &meta(c, group).files {
        if !valid_file_path(&f.name) {
            return Err(format!("invalid file name: {}", f.name));
        }
        let bytes = read_file(c, group, &f.name).ok_or_else(|| format!("cannot read {}", f.name))?;
        let path = folder.join(&f.name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", f.name))?;
        }
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
        size: 0,
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
        .filter(|g| c.action_groups.contains_key(*g))
        .filter_map(|g| Some((g.clone(), serde_json::to_value(meta_with_files(c, g)).ok()?)))
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
        // An overlay is listed among the groups of actions only once something is in it (`edit::import`
        // lists the group of every action, command and timer it brings).
        let overlay = m.files.iter().any(|f| is_page(&f.name));
        if !overlay && !c.actions.groups.iter().any(|v| v.as_str() == Some(&g)) {
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
    fn colours_sliders_and_lines_are_kinds_of_setting() {
        assert!(is_color("#ffffff") && is_color("#18181be6"));
        assert!(!is_color("ffffff") && !is_color("#fff") && !is_color("#gggggg") && !is_color("red"));
        assert_eq!(Kind::Color.parse(" #FFAA00 "), Ok(json!("#ffaa00")));
        assert!(Kind::Color.parse("blue").is_err());
        assert!(Kind::Color.accepts(&json!("#ffaa0080")) && !Kind::Color.accepts(&json!("orange")));
        assert!(Kind::Lines.accepts(&json!("a\nb")));
        let slider = Def {
            name: "size".into(),
            label: "Size".into(),
            kind: Kind::Slider,
            default: json!(24),
            min: Some(12.0),
            max: Some(64.0),
            ..Default::default()
        };
        assert_eq!(slider.range(), (12.0, 64.0, 1.0));
        assert!(slider.accepts(&json!(12)) && slider.accepts(&json!(40.5)) && !slider.accepts(&json!(65)));
        assert_eq!(slider.parse("30"), Ok(json!(30)));
        assert!(slider.parse("300").is_err() && slider.parse("big").is_err());
        // A setting keeps its ends, step and heading through a template file.
        let text = serde_json::to_value(Def {
            step: Some(2.0),
            section: "Look".into(),
            ..slider.clone()
        })
        .unwrap();
        assert_eq!(text["kind"], "slider");
        assert_eq!(
            (&text["min"], &text["step"], &text["section"]),
            (&json!(12.0), &json!(2.0), &json!("Look"))
        );
        let plain = serde_json::to_value(Def::default()).unwrap();
        assert!(plain.get("min").is_none() && plain.get("section").is_none() && plain.get("showIf").is_none());
        // A font is a name; a setting may show only while another one has a value it can have.
        assert!(Kind::Font.accepts(&json!("Inter")) && Kind::Font.accepts(&json!("")));
        let look = Def {
            name: "look".into(),
            label: "Look".into(),
            kind: Kind::Choice,
            default: json!("Dark"),
            options: vec!["Dark".into(), "Custom".into()],
            ..Default::default()
        };
        let tint = |setting: &str, is: Value| Def {
            name: "tint".into(),
            label: "Tint".into(),
            kind: Kind::Color,
            default: json!("#000000"),
            show_if: Some(ShowIf {
                setting: setting.into(),
                is,
            }),
            ..Default::default()
        };
        let meta = |tint: Def| Meta {
            variables: vec![look.clone(), tint],
            ..Default::default()
        };
        assert_eq!(meta(tint("look", json!("Custom"))).validate(), Ok(()));
        assert!(
            meta(tint("look", json!("Pink"))).validate().is_err(),
            "not one of its options"
        );
        assert!(meta(tint("nope", json!("Custom"))).validate().is_err(), "not a setting");
        assert!(meta(tint("tint", json!("#000000"))).validate().is_err(), "not itself");
        let text = serde_json::to_value(tint("look", json!("Custom"))).unwrap();
        assert_eq!(text["showIf"], json!({"setting": "look", "is": "Custom"}));
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
            "../x",
            "/x",
            "C:x",
            "a\\b",
            "a//b",
            "a/../b",
            "a/NUL.html",
            "NUL.html",
            "COM1.txt",
            "x.",
            "..",
            "",
            "1/2/3/4/5/6/7/8/9.txt",
        ] {
            let mut bad = ok.clone();
            bad.files[0].name = name.into();
            assert!(bad.validate().is_err(), "{name}");
        }
        let mut deep = ok.clone();
        deep.files[0].name = "img/wheel-1.png".into();
        deep.validate().unwrap();
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
    fn an_overlay_is_not_a_group_of_actions_until_it_is_connected() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut c = Config::default();
        let page = |name: &str| Meta {
            variables: vec![def("label", Kind::Text, json!("hi"))],
            files: vec![text_file(name, "<p>x</p>")],
            ..Default::default()
        };
        // An overlay alone: the name is taken, and Actions has no group for it.
        set_meta(&mut c, "Clock", page("index.html")).unwrap();
        assert!(exists(&c, "Clock") && has_overlay(&c, "Clock"));
        assert!(!listed(&c, "Clock") && !connected(&c, "Clock"));
        assert_eq!((names(&c), all(&c)), (vec![], vec!["Clock".to_string()]));
        // A rename keeps it an overlay only.
        assert!(rename(&mut c, "Clock", "Watch") && !listed(&c, "Watch") && has_overlay(&c, "Watch"));
        assert!(rename(&mut c, "Watch", "Clock"));
        // Connected to a group of its own name: listed, nothing else changes.
        assert!(connect(&mut c, "Nope", "").is_err());
        connect(&mut c, "Clock", "").unwrap();
        assert!(connected(&c, "Clock") && names(&c) == ["Clock"]);
        assert_eq!(counts(&c, "Clock"), (0, 0, 0));
        // Deleting the group of actions leaves the overlay, its files and its settings.
        c.action_group_variables
            .insert("Clock".into(), json!({"label": "mine"}));
        assert!(ungroup(&mut c, "Clock"));
        assert!(!listed(&c, "Clock") && has_overlay(&c, "Clock"));
        assert_eq!(values(&c, "Clock")["label"], "mine");
        assert!(read_file(&c, "Clock", "index.html").is_some());
        // Creating a group of the overlay's name connects them again.
        assert!(create(&mut c, "Clock") && connected(&c, "Clock") && !create(&mut c, "Clock"));
        ungroup(&mut c, "Clock");

        // Connected to a group that is there: its actions, commands, timers, settings and icon move
        // under the overlay's name, so the page's address stays.
        assert!(create(&mut c, "Votes"));
        c.actions.actions.push(crate::model::Action {
            id: "a1".into(),
            group: "Votes".into(),
            ..Default::default()
        });
        let item = |v: Value| v.as_object().unwrap().clone();
        c.commands.push(item(json!({"id": "c1", "group": "Votes"})));
        c.timers.push(item(json!({"id": "t1"})));
        let m = Meta {
            variables: vec![
                def("label", Kind::Text, json!("theirs")),
                def("limit", Kind::Integer, json!(3)),
            ],
            timers: vec!["t1".into()],
            ..Default::default()
        };
        set_meta(&mut c, "Votes", m).unwrap();
        c.action_group_variables
            .insert("Votes".into(), json!({"limit": 9, "label": "x"}));
        c.action_group_icons.insert("Votes".into(), json!("icon"));
        connect(&mut c, "Clock", "Votes").unwrap();
        assert!(!exists(&c, "Votes") && connected(&c, "Clock"));
        assert_eq!(counts(&c, "Clock"), (1, 1, 1));
        assert_eq!(c.actions.actions[0].group, "Clock");
        assert_eq!(c.commands[0]["group"], "Clock");
        let have = values(&c, "Clock");
        assert_eq!(
            (&have["label"], &have["limit"]),
            (&json!("mine"), &json!(9)),
            "the overlay's own values win"
        );
        assert_eq!(c.action_group_icons["Clock"], "icon");
        // A group with an overlay of its own, or no such group, can't be connected to.
        set_meta(&mut c, "Other", page("index.html")).unwrap();
        connect(&mut c, "Other", "").unwrap();
        assert!(connect(&mut c, "Clock", "Other").is_err());
        assert!(connect(&mut c, "Clock", "Nope").is_err());
        // A group with no overlay is deleted as before: settings and all.
        assert!(create(&mut c, "Plain"));
        c.action_group_variables.insert("Plain".into(), json!({"a": 1}));
        assert!(!ungroup(&mut c, "Plain"));
        assert!(!exists(&c, "Plain") && !c.action_group_variables.contains_key("Plain"));
        // Deleted whole, an overlay takes its files along; its actions stay, without a group.
        ungroup(&mut c, "Clock");
        assert_eq!(
            (c.actions.actions[0].group.as_str(), c.commands[0]["group"].as_str()),
            ("", Some(""))
        );
        assert!(meta(&c, "Clock").timers.is_empty());
        remove(&mut c, "Clock");
        remove(&mut c, "Other");
        assert!(all(&c).is_empty());
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
        assert_eq!(meta_with_files(&dst, "Timer"), meta_with_files(&src, "Timer"));
        assert_ne!(
            meta(&dst, "Timer").store,
            meta(&src, "Timer").store,
            "each has its own folder"
        );
        let mut bad = exported;
        bad["Timer"]["variables"][0]["default"] = json!("x");
        assert!(import_meta(&mut dst, &bad).is_err());
    }

    #[test]
    fn files_are_saved_once_inside_the_folder() {
        let dir = std::env::temp_dir().join(format!("streamerz-groups-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let c = config();
        assert_eq!(save_files(&c, "Timer", &dir).unwrap(), vec![dir.join("a.html")]);
        assert_eq!(std::fs::read(dir.join("a.html")).unwrap(), b"<p>x</p>");
        assert!(save_files(&c, "Timer", &dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn files_platforms_actions_and_lists_are_kinds_of_setting() {
        // A file is a name (or a web address), filtered by what the setting accepts.
        assert!(Kind::File.accepts(&json!("img/a.png")) && Kind::File.accepts(&json!("")));
        assert!(accepts_file("image", "A.PNG") && !accepts_file("image", "a.mp3") && accepts_file("", "a.xyz"));
        assert!(
            accepts_file("sound", "ding.ogg") && accepts_file("video", "x.webm") && accepts_file("font", "f.woff2")
        );
        // Platforms are a list of the four.
        assert!(Kind::Platforms.accepts(&json!(["twitch", "kick"])) && Kind::Platforms.accepts(&json!([])));
        assert!(!Kind::Platforms.accepts(&json!(["facebook"])) && !Kind::Platforms.accepts(&json!("twitch")));
        assert_eq!(
            Kind::Platforms.parse("Twitch, tiktok").unwrap(),
            json!(["twitch", "tiktok"])
        );
        assert!(Kind::Platforms.parse("twitch, mixer").is_err());
        assert_eq!(Kind::Platforms.show(&json!(["twitch", "kick"])), "twitch, kick");
        // A list is rows of its columns: typed a line each, a cell left out takes the default.
        let mut color = def("color", Kind::Color, json!("#ffffff"));
        color.label = "Color".into();
        let mut list = def("slices", Kind::List, json!([]));
        list.columns = vec![
            def("label", Kind::Text, json!("")),
            def("weight", Kind::Integer, json!(1)),
            color,
        ];
        let rows = list
            .parse("Pizza | 3 | #ff0000\n\n  Nothing  \nWater||#00f0f0")
            .unwrap();
        assert_eq!(
            rows,
            json!([{"label": "Pizza", "weight": 3, "color": "#ff0000"},
                {"label": "Nothing", "weight": 1, "color": "#ffffff"},
                {"label": "Water", "weight": 1, "color": "#00f0f0"}])
        );
        assert!(list.accepts(&rows));
        assert_eq!(
            list.show(&rows),
            "Pizza | 3 | #ff0000\nNothing | 1 | #ffffff\nWater | 1 | #00f0f0"
        );
        assert_eq!(
            list.parse(&list.show(&rows)).unwrap(),
            rows,
            "what is shown reads back the same"
        );
        let error = list.parse("a | two").unwrap_err();
        assert!(error.contains("line 1") && error.contains("weight"), "{error}");
        assert!(list
            .parse("a | 1 | #fff000 | more")
            .unwrap_err()
            .contains("more than 3"));
        assert!(!list.accepts(&json!([{"label": "x"}])), "a row has every column");
        assert!(!list.accepts(&json!([{"label": "x", "weight": "many", "color": "#ffffff"}])));
        // What can't be a setting is refused when the group is saved.
        let with = |d: Def| Meta {
            variables: vec![d],
            ..Default::default()
        };
        assert!(with(list.clone()).validate().is_ok());
        let mut empty = list.clone();
        empty.columns.clear();
        assert!(with(empty).validate().unwrap_err().contains("no columns"));
        let mut nested = list.clone();
        nested.columns[0].kind = Kind::List;
        nested.columns[0].default = json!([]);
        assert!(with(nested).validate().unwrap_err().contains("can't be a list"));
        let mut twice = list.clone();
        twice.columns[1].name = "label".into();
        assert!(with(twice).validate().unwrap_err().contains("duplicate column"));
        let mut barred = list.clone();
        barred.columns[0].label = "a|b".into();
        assert!(with(barred).validate().is_err());
        let mut stray = def("x", Kind::Text, json!(""));
        stray.columns = list.columns.clone();
        assert!(with(stray).validate().unwrap_err().contains("not a list"));
        let mut picture = def("picture", Kind::File, json!(""));
        picture.accept = "image".into();
        assert!(with(picture.clone()).validate().is_ok());
        picture.accept = "anything".into();
        assert!(with(picture).validate().is_err());
        let mut wrong = def("n", Kind::Integer, json!(1));
        wrong.accept = "image".into();
        assert!(with(wrong).validate().is_err());
        assert!(with(def(
            "who",
            Kind::Platforms,
            json!(["twitch", "youtube", "kick", "tiktok"])
        ))
        .validate()
        .is_ok());
        assert!(with(def("who", Kind::Platforms, json!(["nope"]))).validate().is_err());
        assert!(with(def("run", Kind::Action, json!(""))).validate().is_ok());
    }

    #[test]
    fn a_file_put_in_the_folder_by_hand_is_one_of_the_groups() {
        let mut c = config();
        let dir = folder(&c, "Timer").unwrap();
        assert!(synced(&c, "Timer").is_none(), "nothing changed yet");
        // Dropped in with a file manager, written by an editor: served at once, listed after a sync.
        std::fs::create_dir_all(dir.join("img")).unwrap();
        std::fs::write(dir.join("img/logo.png"), b"png!").unwrap();
        std::fs::write(dir.join("second.html"), b"<p>two</p>").unwrap();
        std::fs::write(dir.join(".second.html.swp"), b"scratch").unwrap();
        std::fs::write(dir.join("bad name.txt"), b"no").unwrap();
        assert_eq!(read_file(&c, "Timer", "img/logo.png").unwrap(), b"png!");
        assert_eq!(file_path(&c, "Timer", "second.html"), Some(dir.join("second.html")));
        assert_eq!(file_path(&c, "Timer", "../second.html"), None);
        assert!(sync_files(&mut c, "Timer") && !sync_files(&mut c, "Timer"));
        let names: Vec<String> = meta(&c, "Timer").files.iter().map(|f| f.name.clone()).collect();
        assert_eq!(names, ["a.html", "img/logo.png", "second.html"]);
        assert_eq!(meta(&c, "Timer").files[1].size, 4);
        // Saving the settings as they were read never deletes what the list did not know of.
        std::fs::write(dir.join("later.css"), b"p{}").unwrap();
        let m = meta(&c, "Timer");
        set_meta(&mut c, "Timer", m).unwrap();
        assert!(dir.join("later.css").is_file() && dir.join(".second.html.swp").is_file());
        // Deleted by hand: forgotten, with the page it was listed as.
        let mut m = meta(&c, "Timer");
        m.template.pages = vec![Page {
            file: "second.html".into(),
            ..Default::default()
        }];
        set_meta(&mut c, "Timer", m).unwrap();
        std::fs::remove_file(dir.join("second.html")).unwrap();
        assert!(sync_files(&mut c, "Timer"));
        let m = meta(&c, "Timer");
        assert!(m.template.pages.is_empty() && m.files.iter().all(|f| f.name != "second.html"));
        assert!(m.files.iter().any(|f| f.name == "later.css"));
        remove(&mut c, "Timer");
    }

    #[test]
    fn files_live_on_disk_not_in_the_settings() {
        let mut c = config();
        let m = meta(&c, "Timer");
        assert!(m.files[0].content.is_empty() && m.files[0].size == 8, "{m:?}");
        assert!(!c.action_groups["Timer"].to_string().contains("content"));
        let dir = folder(&c, "Timer").unwrap();
        assert_eq!(std::fs::read(dir.join("a.html")).unwrap(), b"<p>x</p>");
        assert_eq!(file_path(&c, "Timer", "a.html"), Some(dir.join("a.html")));
        assert_eq!(file_path(&c, "Timer", "nope.html"), None);
        // Saving the meta as it was read keeps the file; a new one in a folder is added beside it.
        let mut more = m.clone();
        more.files.push(text_file("img/dot.svg", "<svg/>"));
        more.store = "../elsewhere".into();
        set_meta(&mut c, "Timer", more).unwrap();
        assert_eq!(
            folder(&c, "Timer").unwrap(),
            dir,
            "the folder is never taken from outside"
        );
        assert_eq!(read_file(&c, "Timer", "a.html").unwrap(), b"<p>x</p>");
        assert_eq!(read_file(&c, "Timer", "img/dot.svg").unwrap(), b"<svg/>");
        // A rename keeps the folder; a file taken off the list leaves the disk, with its folder.
        assert!(rename(&mut c, "Timer", "Clock"));
        assert_eq!(folder(&c, "Clock").unwrap(), dir);
        let mut less = meta(&c, "Clock");
        less.files.truncate(1);
        set_meta(&mut c, "Clock", less).unwrap();
        assert!(dir.join("a.html").is_file() && !dir.join("img").exists());
        remove(&mut c, "Clock");
        assert!(!dir.exists());
    }

    #[test]
    fn files_inside_older_settings_move_to_disk_once() {
        let mut c = Config::default();
        c.action_groups.insert(
            "Old".into(),
            json!({"files": [{"name": "o.html", "content": "PGI+"}], "description": "d"}),
        );
        assert!(files_to_disk(&mut c));
        assert!(!files_to_disk(&mut c), "nothing left to move");
        let m = meta(&c, "Old");
        assert_eq!((m.description.as_str(), m.files[0].size), ("d", 3));
        assert_eq!(read_file(&c, "Old", "o.html").unwrap(), b"<b>");
        remove(&mut c, "Old");
    }
}
