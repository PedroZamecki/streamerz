//! The Commands, Timers, Queues, History, Globals and Servers tabs. Commands, timers and WebSocket
//! clients/servers are lists of Streamer.bot objects, each edited in a form.

use super::fit::Ellipsis;
use super::forms::{self, Done, Lists, F, W::*};
use super::rows::Cell::{Badge, Lead, Link, Logos, Note, State, Switch};
use super::rows::{Cell as Look, Rows};
use super::{button, dialogs, query, tell, Health, ROW};
use crate::engine::{Engine, Record, DEFAULT_QUEUE};
use crate::i18n::{tr, trf};
use crate::model::Obj;
use crate::Config;
use fltk::button::Button;
use fltk::enums::{Align, Color, Font};
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
use fltk::prelude::*;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::{mpsc::UnboundedSender, watch};

pub(super) type Cfg = Arc<watch::Sender<Config>>;
pub(super) type Cmds = UnboundedSender<crate::engine::UiCmd>;

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

fn on(o: &Obj, k: &str) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// A list line: columns split by tabs.
pub(super) fn line(cols: &[String]) -> String {
    cols.iter()
        .map(|c| c.replace(['\t', '\r', '\n'], " "))
        .collect::<Vec<_>>()
        .join("\t")
}

fn yes(b: bool, t: &str) -> String {
    if b {
        t.into()
    } else {
        String::new()
    }
}

/// Apply one eligibility decision to both the buttons and their reused context menu.
fn set_available(buttons: &[Button], states: &[bool]) {
    for (mut button, enabled) in buttons.iter().cloned().zip(states) {
        if *enabled {
            button.activate();
        } else {
            button.deactivate();
        }
    }
}

fn matches_query(columns: &[String], query: &str) -> bool {
    query.is_empty() || columns.iter().any(|column| column.to_lowercase().contains(query))
}

fn queue_actions(selected: Option<&str>, waiting: bool, any_paused: bool, any_unpaused: bool) -> [bool; 8] {
    let selected_queue = selected.is_some();
    let editable = selected.is_some_and(|id| id != DEFAULT_QUEUE);
    [
        true,
        editable,
        editable,
        selected_queue,
        selected_queue,
        selected_queue && waiting,
        any_unpaused,
        any_paused,
    ]
}

fn history_actions(selected: bool, has_runs: bool) -> [bool; 5] {
    [selected, selected, selected, has_runs, true]
}

fn global_actions(selected: bool, user_global: bool) -> [bool; 4] {
    [true, selected && !user_global, selected && !user_global, true]
}

fn stream_actions(configured: usize, connected: usize) -> [bool; 3] {
    // Disconnect remains available for configured connections whose attempt may still be pending.
    [configured > connected, configured > 0, true]
}

fn validate_global(object: &Obj) -> Result<(), String> {
    if s(object, "variableName").trim().is_empty() {
        Err(tr("lists.globals.needs_name").into())
    } else {
        Ok(())
    }
}

/// Change the config and save it.
pub(super) fn save(cfg: &Cfg, f: impl FnOnce(&mut Config)) {
    cfg.send_modify(|c| {
        f(c);
        // A login (or a channel emptied in Advanced) reads the broadcaster's own channel.
        c.own_channels();
    });
    crate::save_later(&cfg.borrow());
}

/// A section title, with the platform's or app's logo when there's one.
pub(super) fn title(col: &mut Flex, t: &str) {
    let mut f = Frame::default()
        .with_align(Align::Left | Align::Inside | Align::ImageNextToText)
        .ellipsis();
    f.set_label_font(Font::HelveticaBold);
    // The logo of what it's about, or an icon for it.
    let lucide = match t {
        "WebSocket clients" => "plug",
        "WebSocket servers" => "server",
        "Pending" => "hourglass",
        "File and folder watchers" => "folder-open",
        "Voice commands" => "message-square",
        "File tails" => "scroll-text",
        _ => "",
    };
    let pic = super::img::logo(t.split('\'').next().unwrap_or(t), ROW - 8)
        .or_else(|| super::icons::icon(lucide, Color::Foreground, ROW - 10));
    match pic {
        Some(logo) => {
            f.set_label(&format!("  {t}"));
            f.set_image(Some(logo));
            col.fixed(&f, ROW - 2);
        }
        None => {
            f.set_label(t);
            col.fixed(&f, ROW - 4);
        }
    }
}

/// A list with column titles over it (when there are), and `empty` in it while it's empty.
pub(super) fn browser(col: &mut Flex, widths: &'static [i32], titles: &[&'static str], empty: &'static str) -> Rows {
    if !titles.is_empty() {
        let tr_titles: Vec<&'static str> = titles.iter().map(|t| if t.is_empty() { "" } else { tr(t) }).collect();
        super::header(col, widths, &tr_titles);
    }
    let mut b = Rows::new(if empty.is_empty() { "" } else { tr(empty) });
    b.set_column_widths(widths);
    b
}

/// A kind of object kept in a list in the config.
pub struct Kind {
    what: &'static str,
    get: fn(&Config) -> &Vec<Obj>,
    get_mut: fn(&mut Config) -> &mut Vec<Obj>,
    fields: &'static [F],
    /// A new one, with an empty id where Streamer.bot puts it.
    new: fn() -> Value,
    columns: &'static [i32],
    /// The columns' titles, and what an empty list says.
    titles: &'static [&'static str],
    /// How each column draws (text when None).
    cells: &'static [Option<Look>],
    /// Connections: each one's state (`LIVE` in its column), from the engine.
    live: Option<Live>,
    empty: &'static str,
    label: fn(&Obj) -> Vec<String>,
    /// What the engine does after a change (restart the timers...).
    after: Option<fn(&Rc<Engine>)>,
}

/// What a connection list asks the engine about each connection.
#[derive(Clone, Copy, PartialEq)]
pub enum Live {
    Obs,
    Slobs,
}

/// A connection's state goes in this cell (the engine is asked when the list is filled).
const LIVE: &str = "\u{1}live";

/// A switch's cell: whether `key` is on.
fn switch(o: &Obj, key: &str) -> String {
    if on(o, key) { "1" } else { "0" }.into()
}

/// A detail's cell: a Lucide icon and its words.
fn note(icon: &str, words: impl AsRef<str>) -> String {
    format!("{icon}|{}", words.as_ref())
}

/// A state's cell: its health's icon and colour, and its words.
fn state(health: Health, words: &str) -> String {
    let (r, g, b) = health.color().to_rgb();
    format!("{}|{r:02x}{g:02x}{b:02x}|{words}", health.icon())
}

/// Whether a file or folder a list points at is there.
fn found(path: &str) -> String {
    if path.trim().is_empty() {
        state(Health::Off, tr("lists.not_set"))
    } else if std::path::Path::new(path.trim()).exists() {
        state(Health::Up, tr("lists.found"))
    } else {
        state(Health::Down, tr("lists.missing"))
    }
}

/// How a connection reconnects (`key` on: every retry interval).
fn retry(o: &Obj, key: &str) -> String {
    if on(o, key) {
        let every = o.get("retryInterval").and_then(Value::as_u64).unwrap_or(5);
        note(
            "refresh-cw",
            trf("lists.retry_every", &[("seconds", &every.to_string())]),
        )
    } else {
        note("circle-off", tr("lists.no_retry"))
    }
}

/// Whether a password or token is set.
fn locked(set: bool) -> String {
    if set {
        note("lock", tr("lists.secret_set"))
    } else {
        note("lock-open", tr("lists.secret_none"))
    }
}

/// Who may use a command: everyone, the groups it allows, or all but those it denies.
fn who(o: &Obj) -> String {
    let groups: Vec<String> = o
        .get("permittedGroups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let users = o.get("permittedUsers").and_then(Value::as_array).map_or(0, Vec::len);
    let deny = o.get("grantType").and_then(Value::as_u64) == Some(1);
    let mut named = groups.join(", ");
    if users > 0 {
        let people = trf("lists.commands.people", &[("count", &users.to_string())]);
        named = if named.is_empty() {
            people
        } else {
            format!("{named}, {people}")
        };
    }
    match (named.is_empty(), deny) {
        (true, _) => note("users", tr("lists.commands.everyone")),
        (false, false) => note("shield", named),
        (false, true) => note("shield-off", trf("lists.commands.all_but", &[("who", &named)])),
    }
}

/// The platforms a command listens on (`twitch,youtube`), from its sources.
fn source_platforms(bits: u64) -> String {
    let mut out: Vec<String> = Vec::new();
    for (name, bit) in SOURCES {
        let platform = name.split(' ').next().unwrap_or("").to_lowercase();
        if bits & bit != 0 && !out.contains(&platform) {
            out.push(platform);
        }
    }
    out.join(",")
}

const SOURCES: &[(&str, u64)] = &[
    ("Twitch message", 1),
    ("Twitch whisper", 2),
    ("Twitch re-sub message", 8),
    ("YouTube message", 1024),
    ("Kick message", 2_097_152),
    ("TikTok message", 4_194_304),
];

pub static COMMANDS: Kind = Kind {
    what: "lists.commands.what",
    get: |c| &c.commands,
    get_mut: |c| &mut c.commands,
    fields: &[
        ("name", "Name", Text),
        ("enabled", "Enabled", Bool),
        ("include", "Include in Get Commands", Bool),
        ("mode", "Mode", Pick(&["Basic", "Regex"])),
        ("location", "Location (Basic)", Pick(&["Start", "Exact", "Anywhere"])),
        ("command", "Commands, one per line", Multi),
        ("group", "Group", CommandGroup),
        ("ignoreBotAccount", "Ignore the bot account", Bool),
        ("ignoreInternal", "Ignore internal messages", Bool),
        ("persistCounter", "Persist counter", Bool),
        ("persistUserCounter", "Persist per-user counter", Bool),
        ("caseSensitive", "Case sensitive", Bool),
        ("sources", SOURCES[0].0, Bit(SOURCES[0].1)),
        ("sources", SOURCES[1].0, Bit(SOURCES[1].1)),
        ("sources", SOURCES[2].0, Bit(SOURCES[2].1)),
        ("sources", SOURCES[3].0, Bit(SOURCES[3].1)),
        ("sources", SOURCES[4].0, Bit(SOURCES[4].1)),
        ("sources", SOURCES[5].0, Bit(SOURCES[5].1)),
        ("globalCooldown", "Global cooldown (s)", Int),
        ("userCooldown", "User cooldown (s)", Int),
        ("grantType", "Grant type", Pick(&["Allow", "Deny"])),
        ("permittedGroups", "Groups, one per line", Lines),
        ("response", "Reply text", Multi),
        ("replyAll", "Reply on every connected platform", Bool),
    ],
    // Streamer.bot's field order (from its exports).
    new: || {
        json!({"permittedUsers": [], "permittedGroups": [], "id": "", "name": "New command", "enabled": true,
            "include": true, "mode": 0, "command": "!new", "regexExplicitCapture": false, "location": 0,
            "ignoreBotAccount": true, "ignoreInternal": true, "sources": 6292481, "persistCounter": false,
            "persistUserCounter": false, "caseSensitive": false, "globalCooldown": 0, "userCooldown": 0,
            "group": "", "grantType": 0, "response": "", "replyAll": false})
    },
    columns: &[52, 160, 200, 140, 100, 120, 120],
    titles: &[
        "lists.on",
        "lists.commands.name",
        "lists.commands.commands",
        "lists.commands.who",
        "lists.commands.sources",
        "lists.commands.cooldown",
        "lists.commands.group",
    ],
    cells: &[
        Some(Switch),
        None,
        None,
        Some(Note),
        Some(Logos),
        Some(Note),
        Some(Note),
    ],
    live: None,
    empty: "lists.commands.empty",
    label: |o| {
        let n = |k: &str| o.get(k).and_then(Value::as_u64).unwrap_or(0);
        let cooldown = match (n("globalCooldown"), n("userCooldown")) {
            (0, 0) => String::new(),
            (all, 0) => note(
                "timer",
                trf("lists.commands.cooldown_all", &[("seconds", &all.to_string())]),
            ),
            (0, each) => note(
                "timer",
                trf("lists.commands.cooldown_each", &[("seconds", &each.to_string())]),
            ),
            (all, each) => note(
                "timer",
                trf(
                    "lists.commands.cooldown_both",
                    &[("all", &all.to_string()), ("each", &each.to_string())],
                ),
            ),
        };
        let group = s(o, "group");
        vec![
            switch(o, "enabled"),
            s(o, "name").into(),
            crate::commands::aliases(o).join(", "),
            who(o),
            source_platforms(n("sources")),
            cooldown,
            if group.is_empty() {
                String::new()
            } else {
                note("folder", group)
            },
        ]
    },
    after: None,
};

pub static TIMERS: Kind = Kind {
    what: "lists.timers.what",
    get: |c| &c.timers,
    get_mut: |c| &mut c.timers,
    fields: &[
        ("name", "Name", Text),
        ("enabled", "Enabled", Bool),
        ("repeat", "Repeat", Bool),
        ("interval", "Interval (s)", Int),
        ("randomInterval", "Random interval", Bool),
        ("upperInterval", "Up to (s, random)", Int),
        ("lines", "Chat lines", Int),
    ],
    new: || {
        json!({"id": "", "name": "New timer", "enabled": false, "repeat": true, "interval": 60,
            "randomInterval": false, "upperInterval": 0, "lines": 0, "counter": 0})
    },
    columns: &[52, 200, 160, 120, 120, 100],
    titles: &[
        "lists.on",
        "lists.timers.name",
        "lists.timers.interval",
        "lists.timers.repeat",
        "lists.timers.lines",
        "lists.timers.runs",
    ],
    cells: &[Some(Switch), None, Some(Note), Some(Note), Some(Note), Some(Note)],
    live: None,
    empty: "lists.timers.empty",
    label: |o| {
        let n = |k: &str| o.get(k).and_then(Value::as_i64).unwrap_or(0);
        let every = if on(o, "randomInterval") {
            trf(
                "lists.timers.every_random",
                &[
                    ("min", &n("interval").to_string()),
                    ("max", &n("upperInterval").to_string()),
                ],
            )
        } else {
            trf("lists.timers.every", &[("seconds", &n("interval").to_string())])
        };
        vec![
            switch(o, "enabled"),
            s(o, "name").into(),
            note(if on(o, "randomInterval") { "shuffle" } else { "timer" }, every),
            if on(o, "repeat") {
                note("repeat", tr("lists.timers.repeats"))
            } else {
                note("play", tr("lists.timers.once"))
            },
            if n("lines") > 0 {
                note(
                    "message-square",
                    trf("lists.timers.lines_count", &[("count", &n("lines").to_string())]),
                )
            } else {
                String::new()
            },
            note(
                "hash",
                trf("lists.timers.ran_count", &[("count", &n("counter").to_string())]),
            ),
        ]
    },
    after: Some(|e| e.timers()),
};

pub static VOICE_COMMANDS: Kind = Kind {
    what: "lists.voice.what",
    get: |c| &c.voice_commands,
    get_mut: |c| &mut c.voice_commands,
    fields: &[
        ("name", "Name", Text),
        ("enabled", "Enabled", Bool),
        ("command", "Phrases (one per line)", Multi),
        (
            "location",
            "Where it's said",
            Pick(&["The whole phrase", "At the start", "Anywhere"]),
        ),
        ("overrideGlobalConfidence", "Its own confidence", Bool),
        ("confidenceThreshold", "Confidence (0 to 1)", Num),
    ],
    new: || {
        json!({"id": "", "name": "New voice command", "enabled": true, "command": "", "location": 0,
            "overrideGlobalConfidence": false, "confidenceThreshold": 0.0})
    },
    columns: &[52, 180, 300, 140, 120],
    titles: &[
        "lists.on",
        "lists.voice.name",
        "lists.voice.phrases",
        "lists.voice.where",
        "lists.voice.confidence",
    ],
    cells: &[Some(Switch), None, None, Some(Note), Some(Note)],
    live: None,
    empty: "lists.voice.empty",
    label: |o| {
        let at = match o.get("location").and_then(Value::as_u64).unwrap_or(0) {
            1 => tr("lists.voice.at_start"),
            2 => tr("lists.voice.anywhere"),
            _ => tr("lists.voice.whole"),
        };
        let confidence = if on(o, "overrideGlobalConfidence") {
            let c = o.get("confidenceThreshold").and_then(Value::as_f64).unwrap_or(0.0);
            format!("{}%", (c * 100.0).round())
        } else {
            tr("lists.voice.default_confidence").into()
        };
        vec![
            switch(o, "enabled"),
            s(o, "name").into(),
            s(o, "command").lines().collect::<Vec<_>>().join(", "),
            note("mic", at),
            note("gauge", confidence),
        ]
    },
    after: None,
};

pub static FILE_WATCHERS: Kind = Kind {
    what: "lists.watchers.what",
    get: |c| &c.file_watchers,
    get_mut: |c| &mut c.file_watchers,
    fields: &[
        ("name", "Name", Text),
        ("enabled", "Enabled", Bool),
        ("folderPath", "Folder (or a file)", Text),
        ("filter", "Filter (*.txt, *.*)", Text),
        ("includeSubdirectories", "Include subfolders", Bool),
        ("addChangedData", "Add the file's lines to Changed", Bool),
        ("asJson", "Read the file as JSON on Changed", Bool),
    ],
    new: || {
        json!({"id": "", "name": "New watcher", "enabled": true, "folderPath": "", "filter": "*.*",
            "includeSubdirectories": false, "addChangedData": false, "asJson": false})
    },
    columns: &[52, 180, 120, 300, 100, 140],
    titles: &[
        "lists.on",
        "lists.watchers.name",
        "lists.state",
        "lists.watchers.folder",
        "lists.watchers.filter",
        "lists.watchers.subfolders",
    ],
    cells: &[Some(Switch), None, Some(State), Some(Link), Some(Badge), Some(Note)],
    live: None,
    empty: "lists.watchers.empty",
    label: |o| {
        vec![
            switch(o, "enabled"),
            s(o, "name").into(),
            found(s(o, "folderPath")),
            s(o, "folderPath").into(),
            s(o, "filter").into(),
            if on(o, "includeSubdirectories") {
                note("folder-tree", tr("lists.watchers.with_subfolders"))
            } else {
                String::new()
            },
        ]
    },
    after: Some(|e| e.watchers()),
};

pub static FILE_TAILS: Kind = Kind {
    what: "lists.tails.what",
    get: |c| &c.file_tails,
    get_mut: |c| &mut c.file_tails,
    fields: &[
        ("name", "Name", Text),
        ("enabled", "Enabled", Bool),
        ("filePath", "File", Text),
    ],
    new: || json!({"id": "", "name": "New file tail", "enabled": true, "filePath": ""}),
    columns: &[52, 200, 120, 400],
    titles: &["lists.on", "lists.tails.name", "lists.state", "lists.tails.file"],
    cells: &[Some(Switch), None, Some(State), Some(Link)],
    live: None,
    empty: "lists.tails.empty",
    label: |o| {
        vec![
            switch(o, "enabled"),
            s(o, "name").into(),
            found(s(o, "filePath")),
            s(o, "filePath").into(),
        ]
    },
    after: Some(|e| e.watchers()),
};

pub static CLIENTS: Kind = Kind {
    what: "lists.clients.what",
    get: |c| &c.websockets.connections,
    get_mut: |c| &mut c.websockets.connections,
    fields: &[
        ("name", "Name", Text),
        ("endpoint", "Endpoint (ws://...)", Text),
        ("autoConnect", "Connect on start", Bool),
        ("reconnectOnDisconnect", "Reconnect", Bool),
        ("retryInterval", "Retry interval (s)", Int),
    ],
    new: || {
        json!({"id": "", "name": "New client", "endpoint": "ws://127.0.0.1:8080/", "autoConnect": false,
            "reconnectOnDisconnect": true, "retryInterval": 5})
    },
    columns: &[72, 200, 320, 140],
    titles: &[
        "lists.on_start",
        "lists.clients.name",
        "lists.clients.endpoint",
        "lists.reconnect",
    ],
    cells: &[Some(Switch), None, Some(Link), Some(Note)],
    live: None,
    empty: "lists.clients.empty",
    label: |o| {
        vec![
            switch(o, "autoConnect"),
            s(o, "name").into(),
            s(o, "endpoint").into(),
            retry(o, "reconnectOnDisconnect"),
        ]
    },
    after: None,
};

pub static OBS_CONNECTIONS: Kind = Kind {
    what: "lists.obs.what",
    get: |c| &c.obs.connections,
    get_mut: |c| &mut c.obs.connections,
    fields: &[
        ("name", "Name", Text),
        ("host", "Host", Text),
        ("port", "Port", Int),
        ("password", "Password", Secret),
        ("autoConnect", "Connect on start", Bool),
        ("reconnect", "Reconnect", Bool),
        ("retryInterval", "Retry interval (s)", Int),
    ],
    new: || {
        json!({
            "id": crate::engine::uuid(),
            "name": "Local",
            "host": "127.0.0.1",
            "port": 4455,
            "password": "",
            "autoConnect": true,
            "reconnect": true,
            "retryInterval": 5,
            "version": 5,
        })
    },
    columns: &[72, 170, 140, 170, 140, 120],
    titles: &[
        "lists.on_start",
        "lists.obs.name",
        "lists.state",
        "lists.obs.address",
        "lists.reconnect",
        "lists.password",
    ],
    cells: &[
        Some(Switch),
        Some(Lead),
        Some(State),
        Some(Link),
        Some(Note),
        Some(Note),
    ],
    live: Some(Live::Obs),
    empty: "lists.obs.empty",
    label: |o| {
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(4455);
        vec![
            switch(o, "autoConnect"),
            format!("OBS Studio|{}", s(o, "name")),
            LIVE.into(),
            format!("{}:{}", s(o, "host"), port),
            retry(o, "reconnect"),
            locked(!s(o, "password").is_empty()),
        ]
    },
    after: Some(|e| e.obs_start().reload()),
};

pub static SLOBS_CONNECTIONS: Kind = Kind {
    what: "lists.slobs.what",
    get: |c| &c.slobs.connections,
    get_mut: |c| &mut c.slobs.connections,
    fields: &[
        ("name", "Name", Text),
        ("host", "Host", Text),
        ("port", "Port", Int),
        ("token", "Token", Secret),
        ("autoConnect", "Connect on start", Bool),
        ("reconnectOnDisconnect", "Reconnect", Bool),
        ("retryInterval", "Retry interval (s)", Int),
        ("default", "Default connection", Bool),
    ],
    // Streamer.bot's field order (its settings.json).
    new: || {
        json!({"id": "", "name": "Local", "host": "127.0.0.1", "port": 59650, "autoConnect": true,
            "reconnectOnDisconnect": true, "token": "", "retryInterval": 30, "default": false})
    },
    columns: &[72, 190, 140, 170, 120, 100],
    titles: &[
        "lists.on_start",
        "lists.slobs.name",
        "lists.state",
        "lists.slobs.address",
        "lists.slobs.token",
        "",
    ],
    cells: &[
        Some(Switch),
        Some(Lead),
        Some(State),
        Some(Link),
        Some(Note),
        Some(Badge),
    ],
    live: Some(Live::Slobs),
    empty: "lists.slobs.empty",
    label: |o| {
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(59650);
        vec![
            switch(o, "autoConnect"),
            format!("Streamlabs Desktop|{}", s(o, "name")),
            LIVE.into(),
            format!("{}:{port}", s(o, "host")),
            locked(!s(o, "token").is_empty()),
            yes(on(o, "default"), tr("lists.default_badge")),
        ]
    },
    after: Some(|e| e.slobs_sync(false)),
};

pub static SERVERS: Kind = Kind {
    what: "lists.custom_servers.what",
    get: |c| &c.websockets.servers,
    get_mut: |c| &mut c.websockets.servers,
    fields: &[
        ("name", "Name", Text),
        ("address", "Address", Text),
        ("port", "Port", Int),
        ("endpoint", "Endpoint", Text),
        ("autoStart", "Start on launch", Bool),
    ],
    new: || {
        json!({"id": "", "name": "New server", "address": "127.0.0.1", "port": 9090, "endpoint": "/",
            "autoStart": false})
    },
    columns: &[72, 200, 400],
    titles: &[
        "lists.on_start",
        "lists.custom_servers.name",
        "lists.custom_servers.address",
    ],
    cells: &[Some(Switch), None, Some(Link)],
    live: None,
    empty: "lists.custom_servers.empty",
    label: |o| {
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(0);
        vec![
            switch(o, "autoStart"),
            s(o, "name").into(),
            format!("ws://{}:{port}{}", s(o, "address"), s(o, "endpoint")),
        ]
    },
    after: None,
};

/// One of those lists, on screen.
pub struct List {
    cfg: Cfg,
    cmds: Cmds,
    view: Rows,
    kind: &'static Kind,
    ids: Vec<String>,
    selection_buttons: Vec<Button>,
    menu_buttons: Vec<Button>,
    search: Option<fltk::input::Input>,
    connection: Option<ConnectionControls>,
    /// The flag its switches (and On/Off) turn.
    toggle: Option<&'static str>,
    /// Which connections are up, for the selection's Connect or Disconnect.
    up: Vec<bool>,
}

struct ConnectionControls {
    obs: bool,
    status: Frame,
    /// Connect (or Disconnect) the selection, Connect all, Disconnect all, Refresh.
    buttons: Vec<Button>,
}

impl List {
    pub fn refresh(&mut self) {
        let sel = self.selected();
        self.view.clear();
        self.ids.clear();
        let query = self
            .search
            .as_ref()
            .map(|search| search.value().trim().to_lowercase())
            .unwrap_or_default();
        self.view.set_empty(if query.is_empty() {
            tr(self.kind.empty)
        } else {
            tr("lists.no_matches")
        });
        let c = self.cfg.borrow();
        let objects = (self.kind.get)(&c).clone();
        drop(c);
        // Connections: which are up (one question to the engine for all).
        let up: Option<Vec<bool>> = self.kind.live.and_then(|live| {
            let ids: Vec<String> = objects.iter().map(|o| s(o, "id").to_string()).collect();
            super::query(&self.cmds, move |engine: &Rc<Engine>| {
                ids.iter()
                    .map(|id| match live {
                        Live::Obs => engine.obs_connected(Some(id)),
                        Live::Slobs => engine.slobs_up(id),
                    })
                    .collect()
            })
        });
        self.up.clear();
        for (i, o) in objects.iter().enumerate() {
            let mut columns = (self.kind.label)(o);
            let connected = up.as_ref().and_then(|u| u.get(i).copied());
            for cell in columns.iter_mut().filter(|c| *c == LIVE) {
                *cell = match connected {
                    Some(true) => state(Health::Up, tr("lists.live.connected")),
                    Some(false) => state(Health::Off, tr("lists.live.disconnected")),
                    None => state(Health::Busy, tr("lists.stream_apps.unknown")),
                };
            }
            if !matches_query(&columns, &query) {
                continue;
            }
            let off = self.toggle.is_some_and(|key| !on(o, key)) && self.kind.live.is_none();
            self.view.add_state(&line(&columns), off);
            self.ids.push(s(o, "id").into());
            self.up.push(connected.unwrap_or(false));
        }
        if let Some(i) = sel.and_then(|id| self.ids.iter().position(|x| *x == id)) {
            self.view.select(i as i32 + 1);
        }
        self.selection_changed();
        self.connection_changed();
    }

    fn connection_changed(&mut self) {
        let picked = self.view.value() - 1;
        let picked_up = usize::try_from(picked).ok().and_then(|i| self.up.get(i).copied());
        let Some(connection) = &mut self.connection else { return };
        // The selection's own button: Connect, or Disconnect when it's up.
        if let Some(b) = connection.buttons.first_mut() {
            let label = if picked_up == Some(true) {
                tr("btn.disconnect")
            } else {
                tr("btn.connect")
            };
            if b.label() != label {
                b.set_label(label);
                super::with_icon(b);
                b.redraw();
            }
            if picked_up.is_some() {
                b.activate();
            } else {
                b.deactivate();
            }
        }
        let ids: Vec<String> = (self.kind.get)(&self.cfg.borrow())
            .iter()
            .map(|object| s(object, "id").to_string())
            .collect();
        let configured = ids.len();
        let obs = connection.obs;
        let connected = query(&self.cmds, move |engine| {
            ids.iter()
                .filter(|id| {
                    if obs {
                        engine.obs_connected(Some(id))
                    } else {
                        engine.slobs_up(id)
                    }
                })
                .count()
        });
        set_available(
            &connection.buttons[1..],
            &stream_actions(configured, connected.unwrap_or(0)),
        );
        let (health, text) = match connected {
            None => (Health::Busy, tr("lists.stream_apps.unknown").to_string()),
            Some(_) if configured == 0 => (Health::Off, tr("lists.stream_apps.not_configured").to_string()),
            Some(0) => (Health::Off, tr("lists.stream_apps.disconnected").to_string()),
            Some(count) => (
                if count == configured {
                    Health::Up
                } else {
                    Health::Standby
                },
                trf(
                    "lists.stream_apps.connected_count",
                    &[("connected", &count.to_string()), ("total", &configured.to_string())],
                ),
            ),
        };
        health.show(&mut connection.status, &text);
        connection.status.set_tooltip(&text.replace('@', "@@"));
    }

    fn selection_changed(&mut self) {
        if self.connection.is_some() {
            self.connection_changed();
        }
        let selected = self.selected().is_some();
        for button in &mut self.selection_buttons {
            if selected {
                button.activate();
            } else {
                button.deactivate();
            }
        }
    }

    fn selected(&self) -> Option<String> {
        self.ids.get((self.view.value() - 1).max(-1) as usize).cloned()
    }

    fn change(&mut self, f: impl FnOnce(&mut Vec<Obj>)) {
        save(&self.cfg, |c| f((self.kind.get_mut)(c)));
        if let Some(after) = self.kind.after {
            tell(&self.cmds, after);
        }
        self.refresh();
    }

    fn form(&self, o: &Obj) -> Option<Obj> {
        ask(tr(self.kind.what), self.kind.fields, o, &Lists::new(&self.cfg.borrow()))
    }

    fn add(&mut self) {
        let Value::Object(mut o) = (self.kind.new)() else {
            return;
        };
        o.insert("id".into(), crate::engine::uuid().into());
        let Some(o) = self.form(&o) else { return };
        let id = s(&o, "id").to_string();
        self.change(|l| l.push(o));
        if let Some(i) = self.ids.iter().position(|x| *x == id) {
            self.view.select(i as i32 + 1);
        }
        self.selection_changed();
    }

    fn edit(&mut self) {
        let Some(id) = self.selected() else { return };
        let Some(o) = (self.kind.get)(&self.cfg.borrow())
            .iter()
            .find(|o| s(o, "id") == id)
            .cloned()
        else {
            return;
        };
        if let Some(new) = self.form(&o) {
            self.change(|l| {
                if let Some(x) = l.iter_mut().find(|o| s(o, "id") == id) {
                    *x = new;
                }
            });
        }
    }

    fn toggle(&mut self, key: &str) {
        let Some(id) = self.selected() else { return };
        self.change(|l| {
            if let Some(x) = l.iter_mut().find(|o| s(o, "id") == id) {
                let v = !on(x, key);
                x.insert(key.into(), v.into());
            }
        });
    }

    fn delete(&mut self) {
        let Some(id) = self.selected() else { return };
        let q = trf("lists.delete_confirm", &[("what", &tr(self.kind.what).to_lowercase())]);
        if super::dialogs::danger(&q.replace('@', "@@"), tr("dialogs.cancel"), tr("btn.delete")) {
            self.change(|l| l.retain(|o| s(o, "id") != id));
        }
    }
}

/// Edit `o` in a form, or as JSON when asked; the result, if OK'd.
fn ask(title: &str, fields: &[F], o: &Obj, lists: &Lists) -> Option<Obj> {
    match forms::form(title, fields, o, lists) {
        Done::Ok(x) => Some(x),
        Done::Cancel => None,
        Done::Json(draft) => json(title, &draft),
    }
}

/// An object as JSON; it must stay an object, with the same id if it has one.
fn json(what: &str, o: &Obj) -> Option<Obj> {
    let id = s(o, "id").to_string();
    let v = dialogs::json_editor(what, "", &Value::Object(o.clone()), |v| match v {
        Value::Object(x) if id.is_empty() || s(x, "id") == id => Ok(()),
        Value::Object(_) => Err(format!("Keep \"id\": \"{id}\".")),
        _ => Err("An object, please.".into()),
    })?;
    match v {
        Value::Object(x) => Some(x),
        _ => None,
    }
}

/// A row of buttons.
pub(super) fn bar(col: &mut Flex, labels: &[&str]) -> Vec<Button> {
    super::grid(col, labels, 1)
}

/// A list of `kind`, with Add, Edit, On/Off (when `toggle` names the flag) and Delete.
fn list(
    col: &mut Flex,
    cfg: &Cfg,
    cmds: &Cmds,
    kind: &'static Kind,
    toggle: Option<&'static str>,
) -> Rc<RefCell<List>> {
    let search = matches!(kind.what, "lists.commands.what" | "lists.timers.what").then(|| {
        let mut search = fltk::input::Input::default();
        super::placeholder(&mut search, tr("lists.search"));
        search.set_tooltip(tr("lists.search"));
        search.set_trigger(fltk::enums::CallbackTrigger::Changed);
        col.fixed(&search, ROW);
        search
    });
    let mut view = browser(col, kind.columns, kind.titles, kind.empty);
    view.set_label(tr(kind.what));
    for (c, look) in kind.cells.iter().enumerate() {
        if let Some(look) = look {
            view.set_cell(c, *look);
        }
    }
    let mut labels = vec![tr("btn.add"), tr("btn.edit")];
    labels.extend(toggle.map(|_| tr("btn.on_off")));
    labels.push(tr("btn.delete"));
    let b = bar(col, &labels);
    view.clone().set_menu(&b);
    let me = Rc::new(RefCell::new(List {
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        view: view.clone(),
        kind,
        ids: Vec::new(),
        selection_buttons: b[1..].to_vec(),
        menu_buttons: b.clone(),
        search: search.clone(),
        connection: None,
        toggle,
        up: Vec::new(),
    }));
    if let Some(mut search) = search {
        let me = me.clone();
        search.set_callback(move |_| {
            if let Ok(mut list) = me.try_borrow_mut() {
                list.refresh();
            }
        });
    }
    me.borrow_mut().selection_changed();
    let click = |b: &Button, f: Box<dyn Fn(&mut List)>| {
        let me = me.clone();
        b.clone().set_callback(move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                f(&mut m);
            }
        });
    };
    click(&b[0], Box::new(List::add));
    click(&b[1], Box::new(List::edit));
    if let Some(key) = toggle {
        click(&b[2], Box::new(move |m| m.toggle(key)));
    }
    click(&b[b.len() - 1], Box::new(List::delete));
    view.clone().set_callback({
        let me = me.clone();
        move |v| {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.selection_changed();
                if fltk::app::event() == fltk::enums::Event::Push
                    && fltk::app::event_clicks()
                    && !super::right_button()
                    && !v.clicked_switch()
                {
                    m.edit();
                }
            }
        }
    });
    if let Some(key) = toggle {
        // A line's switch turns it on or off where it is.
        view.clone().on_switch({
            let me = me.clone();
            move |_, _| {
                if let Ok(mut m) = me.try_borrow_mut() {
                    m.toggle(key);
                }
            }
        });
    }
    me
}

fn tab(tabs: &Wizard, label: &str) -> Flex {
    super::tab(tabs, label)
}

/// What the tabs show that can change while they're up.
pub struct Views {
    pub lists: Vec<Rc<RefCell<List>>>,
    pub queues: Rc<RefCell<Queues>>,
    pub history: Rc<RefCell<History>>,
    pub globals: Rc<RefCell<Globals>>,
    pub community: Rc<RefCell<Community>>,
    pub servers: Rc<RefCell<Servers>>,
    pub groups: Rc<RefCell<Groups>>,
    pub plugins: Rc<RefCell<super::plugins::Plugins>>,
    pub donations: Rc<RefCell<Donations>>,
    pub accounts: Rc<RefCell<super::accounts::Accounts>>,
    pub chat: Rc<RefCell<super::chat::Chat>>,
    pub crashes: Rc<RefCell<super::recovery::Crashes>>,
    pub backups: Rc<RefCell<super::recovery::Backups>>,
    pub polls: Rc<RefCell<super::polls::Polls>>,
    pub agents: Rc<RefCell<super::agents::Agents>>,
}

impl Views {
    /// New chat lines: added to the Chat tab when it's up (it catches up when shown).
    pub fn chat_refresh(&self, tabs: &Wizard) {
        // A refresh queued before hide can still arrive after presentation storage was released.
        if !super::SHOWN.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let up = tabs.try_current_widget().map(|g| g.as_widget_ptr());
        if let Ok(mut c) = self.chat.try_borrow_mut() {
            super::chat::set_viewers_visible(up == Some(c.tab.as_widget_ptr()));
            if up == Some(c.tab.as_widget_ptr()) {
                c.refresh();
            }
        }
        // Chat brings new people to the Community tab.
        if let Ok(mut c) = self.community.try_borrow_mut() {
            if up == Some(c.tab.as_widget_ptr()) {
                c.live();
            }
        }
    }

    /// The window was hidden: drop the chat and Community presentation copies.
    pub fn chat_release(&self) {
        self.community.borrow_mut().release();
        super::chat::set_viewers_visible(false);
        if let Ok(mut c) = self.chat.try_borrow_mut() {
            c.release();
        }
    }

    /// A picture arrived: redraw what shows pictures.
    pub fn images(&self) {
        if !super::SHOWN.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if let Ok(mut a) = self.accounts.try_borrow_mut() {
            a.refresh();
        }
        if let Ok(c) = self.chat.try_borrow() {
            c.images();
        }
    }

    /// Redraw what the config shows, and the live views on the tab that's up.
    pub fn refresh(&self, tabs: &Wizard) {
        let up = tabs.try_current_widget().map(|g| g.as_widget_ptr());
        for l in &self.lists {
            l.borrow_mut().refresh();
        }
        self.servers.borrow_mut().refresh();
        if let Ok(mut g) = self.groups.try_borrow_mut() {
            g.refresh();
        }
        // A page, or the scroll a long page sits in (what the wizard shows then).
        let is =
            |w: &Flex| up.is_some() && (up == Some(w.as_widget_ptr()) || up == w.parent().map(|p| p.as_widget_ptr()));
        if let Ok(mut a) = self.accounts.try_borrow_mut() {
            if is(&a.tab) || is(&a.integrations_tab) {
                a.refresh();
            }
        }
        self.chat_refresh(tabs);
        if let Ok(mut d) = self.donations.try_borrow_mut() {
            if is(&d.tab) {
                d.refresh();
            }
        }
        if let Ok(mut q) = self.queues.try_borrow_mut() {
            if is(&q.tab) {
                q.refresh();
            }
        }
        if let Ok(mut h) = self.history.try_borrow_mut() {
            if is(&h.tab) {
                h.refresh();
            }
        }
        if let Ok(mut g) = self.globals.try_borrow_mut() {
            if is(&g.tab) {
                g.refresh();
            }
        }
        if let Ok(mut c) = self.community.try_borrow_mut() {
            if is(&c.tab) {
                c.refresh();
            } else {
                c.release();
            }
        }
        if let Ok(mut p) = self.plugins.try_borrow_mut() {
            if is(&p.tab) {
                p.refresh();
            }
        }
        if let Ok(mut p) = self.polls.try_borrow_mut() {
            if is(&p.tab) {
                p.refresh();
            }
        }
        if let Ok(mut c) = self.crashes.try_borrow_mut() {
            if is(&c.tab) {
                c.refresh();
            }
        }
        if let Ok(mut b) = self.backups.try_borrow_mut() {
            if is(&b.tab) {
                b.refresh();
            }
        }
        if let Ok(mut a) = self.agents.try_borrow_mut() {
            if is(&a.tab) {
                a.refresh();
            }
        }
    }
}

pub fn tabs(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Views {
    let accounts = super::accounts::tab(tabs, cfg, cmds);
    let chat = super::chat::tab(tabs, cfg, cmds);
    let mut col = tab(tabs, "Commands");
    let commands = list(&mut col, cfg, cmds, &COMMANDS, Some("enabled"));
    title(&mut col, tr("lists.voice.title"));
    super::hint(&mut col, tr("lists.voice.guide"), 36);
    let voice = list(&mut col, cfg, cmds, &VOICE_COMMANDS, Some("enabled"));
    col.end();

    let mut col = tab(tabs, "Timers");
    let timers = list(&mut col, cfg, cmds, &TIMERS, Some("enabled"));
    col.end();

    let (files_scroll, mut col) = super::scroll::page(tabs, "Files");
    title(&mut col, tr("lists.watchers.title"));
    super::hint(&mut col, tr("lists.watchers.guide"), 36);
    let watchers = list(&mut col, cfg, cmds, &FILE_WATCHERS, Some("enabled"));
    title(&mut col, tr("lists.tails.title"));
    super::hint(&mut col, tr("lists.tails.guide"), 36);
    let tails = list(&mut col, cfg, cmds, &FILE_TAILS, Some("enabled"));
    col.end();
    files_scroll.end();

    let queues = queues_tab(tabs, cfg, cmds);
    let history = history_tab(tabs, cmds);
    let globals = globals_tab(tabs, cmds);
    let community = community_tab(tabs, cfg, cmds);
    let groups = groups_tab(tabs, cfg, cmds);
    let plugins = super::plugins::tab(tabs, cfg, cmds);

    let (servers_scroll, mut col) = super::scroll::page(tabs, "Servers");
    let servers = servers_box(&mut col, cfg);
    title(&mut col, tr("lists.servers.ws_clients_title"));
    let clients = list(&mut col, cfg, cmds, &CLIENTS, None);
    title(&mut col, tr("lists.servers.ws_servers_title"));
    let custom = list(&mut col, cfg, cmds, &SERVERS, Some("autoStart"));
    col.end();
    servers_scroll.end();

    let (apps_scroll, mut col) = super::scroll::page(tabs, "Stream Apps");
    title(&mut col, "OBS Studio");
    super::hint(&mut col, tr("lists.stream_apps.obs_guide"), 36);
    let obs = list(&mut col, cfg, cmds, &OBS_CONNECTIONS, Some("autoConnect"));
    connection_controls(&mut col, &obs, true);
    title(&mut col, "Streamlabs Desktop");
    super::hint(&mut col, tr("lists.stream_apps.slobs_guide"), 36);
    let slobs = list(&mut col, cfg, cmds, &SLOBS_CONNECTIONS, Some("autoConnect"));
    connection_controls(&mut col, &slobs, false);
    col.end();
    apps_scroll.end();
    let donations = donations_tab(tabs, cfg, cmds);
    let crashes = super::recovery::crashes_tab(tabs);
    let polls = super::polls::tab(tabs, cmds);
    let backups = super::recovery::backups_tab(tabs, cfg, cmds);
    let agents = super::agents::tab(tabs, cfg);

    Views {
        lists: vec![commands, voice, timers, watchers, tails, clients, custom, obs, slobs],
        queues,
        history,
        globals,
        community,
        servers,
        groups,
        plugins,
        donations,
        accounts,
        chat,
        crashes,
        backups,
        polls,
        agents,
    }
}

/// Connection actions use the list's same menu and observe state without starting integrations.
fn connection_controls(col: &mut Flex, list: &Rc<RefCell<List>>, obs: bool) {
    let status = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
    col.fixed(&status, ROW);
    let buttons = bar(
        col,
        &[
            tr("btn.connect"),
            tr("btn.connect_all"),
            tr("btn.disconnect_all"),
            tr("btn.refresh"),
        ],
    );
    {
        let mut list = list.borrow_mut();
        let mut menu = list.menu_buttons.clone();
        menu.extend(buttons.iter().cloned());
        list.view.set_menu(&menu);
        list.connection = Some(ConnectionControls {
            obs,
            status,
            buttons: buttons.clone(),
        });
        // There are no configured rows before refresh, so connection actions start unavailable.
        set_available(&buttons[1..], &stream_actions(0, 0));
        let mut first = buttons[0].clone();
        first.deactivate();
    }
    for (index, mut button) in buttons.into_iter().enumerate() {
        let list = list.clone();
        button.set_callback(move |_| {
            let Ok(mut list) = list.try_borrow_mut() else { return };
            // The selection alone (index 0), all of them (1, 2), or only a look again (3).
            let (connect, one) = match index {
                0 => {
                    let Some(id) = list.selected() else { return };
                    let i = (list.view.value() - 1).max(0) as usize;
                    (!list.up.get(i).copied().unwrap_or(false), Some(id))
                }
                1 => (true, None),
                2 => (false, None),
                _ => {
                    list.refresh();
                    return;
                }
            };
            tell(&list.cmds, move |engine| {
                let id = one.as_deref();
                if obs {
                    if connect {
                        engine.obs_start().connect(id);
                    } else if let Some(handle) = engine.obs() {
                        handle.disconnect(id);
                    }
                } else if connect {
                    engine.slobs_connect(id.unwrap_or(""));
                } else {
                    engine.slobs_disconnect(id.unwrap_or(""));
                }
            });
            list.refresh();
        });
    }
}

/// Queues: the configured ones (Default first) and what's running and waiting in them.
pub struct Queues {
    tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    view: Rows,
    pending: Rows,
    ids: Vec<String>,
    waiting: Vec<bool>,
    paused: Vec<bool>,
    buttons: Vec<Button>,
}

impl Queues {
    pub fn refresh(&mut self) {
        let Some((paused, pending)) = query(&self.cmds, |e| (e.paused(), e.pending())) else {
            return;
        };
        let sel = self.selected();
        let c = self.cfg.borrow();
        let mut rows = vec![(
            DEFAULT_QUEUE.to_string(),
            tr("lists.queues.default").to_string(),
            c.actions.blocking,
        )];
        rows.extend(
            c.actions
                .queues
                .iter()
                .map(|q| (s(q, "id").into(), s(q, "name").into(), on(q, "blocking"))),
        );
        let name = |id: &str| rows.iter().find(|r| r.0 == id).map_or(id, |r| r.1.as_str()).to_string();
        self.view.clear();
        self.ids.clear();
        self.waiting.clear();
        self.paused.clear();
        for (id, name, blocking) in &rows {
            let count = |running: bool| pending.iter().filter(|item| item.0 == *id && item.2 == running).count();
            self.view.add(&line(&[
                name.clone(),
                yes(*blocking, tr("lists.queues.blocking")),
                yes(paused.contains(id), tr("lists.queues.paused")),
                count(true).to_string(),
                count(false).to_string(),
            ]));
            self.ids.push(id.clone());
            self.waiting.push(count(false) > 0);
            self.paused.push(paused.contains(id));
        }
        self.pending.clear();
        for (q, action, running) in &pending {
            self.pending.add(&line(&[
                name(q),
                action.clone(),
                if *running {
                    tr("lists.queues.running")
                } else {
                    tr("lists.queues.waiting")
                }
                .into(),
            ]));
        }
        drop(c);
        if let Some(i) = sel.and_then(|id| self.ids.iter().position(|x| *x == id)) {
            self.view.select(i as i32 + 1);
        }
        self.selection_changed();
    }

    fn selection_changed(&self) {
        let selected = self.selected();
        let index = usize::try_from(self.view.value() - 1).ok();
        let waiting = index
            .and_then(|index| self.waiting.get(index))
            .copied()
            .unwrap_or(false);
        set_available(
            &self.buttons,
            &queue_actions(
                selected.as_deref(),
                waiting,
                self.paused.iter().any(|paused| *paused),
                self.paused.iter().any(|paused| !*paused),
            ),
        );
    }

    fn selected(&self) -> Option<String> {
        self.ids.get((self.view.value() - 1).max(-1) as usize).cloned()
    }

    /// Add (None) or edit a queue: its name and Blocking.
    fn edit(&mut self, id: Option<String>) {
        let o = match &id {
            Some(id) => match self.cfg.borrow().actions.queues.iter().find(|q| s(q, "id") == id) {
                Some(q) => q.clone(),
                None => return,
            },
            None => json!({"id": crate::engine::uuid(), "name": tr("lists.queues.queue_title"), "blocking": false})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        };
        let fields: &[F] = &[("name", "Name", Text), ("blocking", "Blocking", Bool)];
        let Some(o) = ask(
            tr("lists.queues.queue_title"),
            fields,
            &o,
            &Lists::new(&self.cfg.borrow()),
        ) else {
            return;
        };
        let (qid, blocking) = (s(&o, "id").to_string(), on(&o, "blocking"));
        save(&self.cfg, |c| {
            match c.actions.queues.iter_mut().find(|q| s(q, "id") == qid) {
                Some(q) => *q = o,
                None => c.actions.queues.push(o),
            }
        });
        // Through the engine, so a queue no longer blocking starts what's waiting.
        tell(&self.cmds, move |e| e.set_blocking(&qid, blocking));
        self.refresh();
    }
}

fn queues_tab(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Queues>> {
    let mut col = tab(tabs, "Queues");
    let view = browser(
        &mut col,
        &[220, 100, 80, 100, 100],
        &[
            "lists.queues.queue",
            "",
            "",
            "lists.queues.running",
            "lists.queues.waiting",
        ],
        "",
    );
    let b = bar(
        &mut col,
        &[
            tr("btn.add"),
            tr("btn.edit"),
            tr("btn.delete"),
            tr("btn.blocking"),
            tr("btn.pause_resume"),
            tr("btn.clear"),
            tr("btn.pause_all"),
            tr("btn.resume_all"),
        ],
    );
    view.clone().set_menu(&b);
    title(&mut col, tr("lists.queues.pending_title"));
    let pending = browser(
        &mut col,
        &[220, 420, 100],
        &["lists.queues.queue", "lists.queues.action", "lists.queues.status"],
        "lists.queues.pending_empty",
    );
    col.fixed(&*pending, 200);
    col.end();
    col.resize_callback({
        let pending = pending.clone();
        move |col, _, _, _, height| col.fixed(&*pending, (height / 3).clamp(100, 200))
    });
    let me = Rc::new(RefCell::new(Queues {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        view: view.clone(),
        pending,
        ids: Vec::new(),
        waiting: Vec::new(),
        paused: Vec::new(),
        buttons: b.clone(),
    }));
    me.borrow().selection_changed();
    view.clone().set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(list) = me.try_borrow() {
                list.selection_changed();
            }
        }
    });
    let click = |b: &Button, f: Box<dyn Fn(&mut Queues)>| {
        let me = me.clone();
        b.clone().set_callback(move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                f(&mut m);
                m.refresh();
            }
        });
    };
    click(&b[0], Box::new(|m| m.edit(None)));
    click(
        &b[1],
        Box::new(|m| {
            let id = m.selected().filter(|id| id != DEFAULT_QUEUE);
            if id.is_some() {
                m.edit(id);
            }
        }),
    );
    click(
        &b[2],
        Box::new(|m| {
            let Some(id) = m.selected().filter(|id| id != DEFAULT_QUEUE) else {
                return;
            };
            let q = tr("lists.queues.delete_confirm");
            if super::dialogs::danger(q, tr("dialogs.cancel"), tr("btn.delete")) {
                save(&m.cfg, |c| {
                    c.actions.queues.retain(|q| s(q, "id") != id);
                    for a in c.actions.actions.iter_mut().filter(|a| a.queue.as_deref() == Some(&id)) {
                        a.queue = Some(DEFAULT_QUEUE.into());
                    }
                });
            }
        }),
    );
    click(
        &b[3],
        Box::new(|m| {
            let Some(id) = m.selected() else { return };
            let c = m.cfg.borrow();
            let now = if id == DEFAULT_QUEUE {
                c.actions.blocking
            } else {
                c.actions.queues.iter().any(|q| s(q, "id") == id && on(q, "blocking"))
            };
            drop(c);
            let _ = query(&m.cmds, move |e| e.set_blocking(&id, !now));
        }),
    );
    click(
        &b[4],
        Box::new(|m| {
            let Some(id) = m.selected() else { return };
            let _ = query(&m.cmds, move |e| {
                let paused = e.paused().contains(&id);
                e.pause(&id, !paused, false);
            });
        }),
    );
    click(
        &b[5],
        Box::new(|m| {
            if let Some(id) = m.selected() {
                let _ = query(&m.cmds, move |e| e.clear_queue(&id));
            }
        }),
    );
    click(
        &b[6],
        Box::new(|m| {
            let _ = query(&m.cmds, |e| e.pause("", true, false));
        }),
    );
    click(
        &b[7],
        Box::new(|m| {
            let _ = query(&m.cmds, |e| e.pause("", false, false));
        }),
    );
    me
}

/// The last runs, newest first.
pub struct History {
    tab: Flex,
    cmds: Cmds,
    view: Rows,
    runs: Vec<Record>,
    search: fltk::input::Input,
    buttons: Vec<Button>,
    has_runs: bool,
}

fn same_run(left: &Record, right: &Record) -> bool {
    left.action_id == right.action_id
        && left.queued_at == right.queued_at
        && left.args.get("runningActionId") == right.args.get("runningActionId")
}

impl History {
    pub fn refresh(&mut self) {
        let Some(runs) = query(&self.cmds, |e| e.history().iter().rev().cloned().collect::<Vec<_>>()) else {
            return;
        };
        let selected = self.selected().cloned();
        let query = self.search.value().trim().to_lowercase();
        self.has_runs = !runs.is_empty();
        self.view.clear();
        self.view.set_empty(if query.is_empty() {
            tr("lists.history.empty")
        } else {
            tr("lists.no_matches")
        });
        self.runs.clear();
        for record in runs {
            let columns = [
                crate::fmt::shown_iso(&record.queued_at, true),
                record.name.clone(),
                format!("{:.0} ms", record.ms),
                record.after.len().to_string(),
                tr(match record.outcome.status() {
                    "failed" => "automation.failed",
                    "completed_with_skips" => "automation.completed_with_skips",
                    _ => "automation.completed",
                })
                .into(),
            ];
            if !matches_query(&columns, &query) {
                continue;
            }
            self.view.add(&line(&columns));
            self.runs.push(record);
        }
        if let Some(index) = selected.and_then(|selected| self.runs.iter().position(|run| same_run(run, &selected))) {
            self.view.select(index as i32 + 1);
        }
        self.selection_changed();
    }

    fn selection_changed(&self) {
        set_available(
            &self.buttons,
            &history_actions(self.selected().is_some(), self.has_runs),
        );
    }

    fn selected(&self) -> Option<&Record> {
        self.runs.get((self.view.value() - 1).max(-1) as usize)
    }
}

fn history_tab(tabs: &Wizard, cmds: &Cmds) -> Rc<RefCell<History>> {
    let mut col = tab(tabs, "History");
    let mut search = fltk::input::Input::default();
    super::placeholder(&mut search, tr("lists.history.search"));
    search.set_tooltip(tr("lists.history.search"));
    search.set_trigger(fltk::enums::CallbackTrigger::Changed);
    col.fixed(&search, ROW);
    let view = browser(
        &mut col,
        &[180, 340, 100, 90, 160],
        &[
            "lists.history.when",
            "lists.history.action",
            "lists.history.took",
            "lists.history.variables",
            "automation.result",
        ],
        "lists.history.empty",
    );
    let b = bar(
        &mut col,
        &[
            tr("btn.variables_when_queued"),
            tr("btn.variables_after_run"),
            tr("btn.run_again"),
            tr("btn.clear"),
            tr("btn.refresh"),
        ],
    );
    view.clone().set_menu(&b);
    col.end();
    let me = Rc::new(RefCell::new(History {
        tab: col,
        cmds: cmds.clone(),
        view: view.clone(),
        runs: Vec::new(),
        search: search.clone(),
        buttons: b.clone(),
        has_runs: false,
    }));
    me.borrow().selection_changed();
    view.clone().set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(history) = me.try_borrow() {
                history.selection_changed();
            }
        }
    });
    search.set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(mut history) = me.try_borrow_mut() {
                history.refresh();
            }
        }
    });
    let click = |b: &Button, f: Box<dyn Fn(&mut History)>| {
        let me = me.clone();
        b.clone().set_callback(move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                f(&mut m);
            }
        });
    };
    let show = |after: bool| {
        move |m: &mut History| {
            if let Some(r) = m.selected() {
                let v = if after { &r.after } else { &r.args };
                let hint = r.outcome.error.as_deref().unwrap_or(tr("automation.completed_hint"));
                dialogs::values(&r.name, hint, v, false);
            }
        }
    };
    click(&b[0], Box::new(show(false)));
    click(&b[1], Box::new(show(true)));
    click(
        &b[2],
        Box::new(|m| {
            let Some(r) = m.selected() else { return };
            let (id, mut args) = (r.action_id.clone(), r.args.clone());
            // A new run: new id and time.
            args.shift_remove("runningActionId");
            args.shift_remove("actionQueuedAt");
            tell(&m.cmds, move |e| drop(e.queue(&id, args)));
        }),
    );
    click(
        &b[3],
        Box::new(|m| {
            let _ = query(&m.cmds, |e| e.clear_history());
            m.refresh();
        }),
    );
    click(&b[4], Box::new(History::refresh));
    me
}

/// Globals and user globals, as GetGlobals and TwitchGetUserGlobals list them.
pub struct Globals {
    tab: Flex,
    cmds: Cmds,
    view: Rows,
    /// Each row: (name, value, persisted, a user global's user).
    rows: Vec<(String, Value, bool, Option<String>)>,
    buttons: Vec<Button>,
}

impl Globals {
    pub fn refresh(&mut self) {
        let Some(rows) = query(&self.cmds, |e| {
            let mut out = Vec::new();
            for persisted in [true, false] {
                for (k, v) in e.globals_list(persisted) {
                    out.push((k, v["value"].clone(), persisted, None));
                }
            }
            for platform in ["twitch", "youtube", "kick"] {
                for persisted in [true, false] {
                    for v in e.user_globals_list(platform, None, None, persisted) {
                        let user = format!("{} ({platform})", v["userName"].as_str().unwrap_or(""));
                        let name = v["name"].as_str().unwrap_or("").to_string();
                        out.push((name, v["value"].clone(), persisted, Some(user)));
                    }
                }
            }
            out
        }) else {
            return;
        };
        let selected = self.selected().map(|row| (row.0.clone(), row.2, row.3.clone()));
        self.view.clear();
        for (name, v, persisted, user) in &rows {
            self.view.add(&line(&[
                name.clone(),
                crate::vars::text(v),
                if *persisted {
                    tr("lists.globals.persisted")
                } else {
                    tr("lists.globals.temporary")
                }
                .into(),
                user.as_ref()
                    .map(|user| trf("lists.globals.read_only_user", &[("user", user)]))
                    .unwrap_or_default(),
            ]));
        }
        self.rows = rows;
        if let Some(index) = selected.and_then(|selected| {
            self.rows
                .iter()
                .position(|row| (row.0.as_str(), row.2, &row.3) == (selected.0.as_str(), selected.1, &selected.2))
        }) {
            self.view.select(index as i32 + 1);
        }
        self.selection_changed();
    }

    fn selection_changed(&self) {
        let selected = self.selected();
        set_available(
            &self.buttons,
            &global_actions(selected.is_some(), selected.is_some_and(|row| row.3.is_some())),
        );
    }

    fn selected(&self) -> Option<&(String, Value, bool, Option<String>)> {
        self.rows.get((self.view.value() - 1).max(-1) as usize)
    }

    /// Add (None) or edit a global.
    fn edit(&mut self, old: Option<(String, Value, bool)>) {
        let (name, value, persisted) = old.clone().unwrap_or((String::new(), Value::from(""), true));
        let o = json!({"variableName": name, "value": crate::vars::text(&value), "autoType": !value.is_string(),
            "persisted": persisted});
        let fields: &[F] = &[
            ("variableName", "Name", Text),
            ("value", "Value", Text),
            ("autoType", "Auto type", Bool),
            ("persisted", "Persisted", Bool),
        ];
        let Some(o) = o.as_object() else { return };
        let title = tr("lists.globals.global_title");
        let object = match forms::form_validated(title, fields, o, &Lists::default(), validate_global) {
            Done::Ok(object) => Some(object),
            Done::Cancel => None,
            Done::Json(draft) => dialogs::json_editor(title, "", &Value::Object(draft), |value| {
                let object = value
                    .as_object()
                    .ok_or_else(|| tr("dialogs.values.need_object").to_string())?;
                validate_global(object)
            })
            .and_then(|value| value.as_object().cloned()),
        };
        let Some(o) = object else { return };
        let name = s(&o, "variableName").trim().to_string();
        let v = if on(&o, "autoType") {
            crate::vars::auto_type(s(&o, "value"))
        } else {
            s(&o, "value").into()
        };
        let persisted = on(&o, "persisted");
        let _ = query(&self.cmds, move |e| {
            if let Some((n, _, p)) = old.filter(|(n, _, p)| *n != name || *p != persisted) {
                e.delete_global(&n, p);
            }
            e.put_global(&name, v, persisted);
        });
        self.refresh();
    }
}

fn globals_tab(tabs: &Wizard, cmds: &Cmds) -> Rc<RefCell<Globals>> {
    let mut col = tab(tabs, "Globals");
    super::hint(&mut col, tr("lists.globals.read_only_hint"), 36);
    let view = browser(
        &mut col,
        &[220, 420, 100, 200],
        &["lists.globals.name", "lists.globals.value", "", "lists.globals.user"],
        "lists.globals.empty",
    );
    let b = bar(
        &mut col,
        &[tr("btn.add"), tr("btn.edit"), tr("btn.delete"), tr("btn.refresh")],
    );
    view.clone().set_menu(&b);
    col.end();
    let me = Rc::new(RefCell::new(Globals {
        tab: col,
        cmds: cmds.clone(),
        view: view.clone(),
        rows: Vec::new(),
        buttons: b.clone(),
    }));
    me.borrow().selection_changed();
    view.clone().set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(globals) = me.try_borrow() {
                globals.selection_changed();
            }
        }
    });
    let click = |b: &Button, f: Box<dyn Fn(&mut Globals)>| {
        let me = me.clone();
        b.clone().set_callback(move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                f(&mut m);
            }
        });
    };
    // User globals are read only here; Global (Set) with User changes them.
    let global = |m: &Globals| {
        m.selected()
            .filter(|r| r.3.is_none())
            .map(|r| (r.0.clone(), r.1.clone(), r.2))
    };
    click(&b[0], Box::new(|m| m.edit(None)));
    click(
        &b[1],
        Box::new(move |m| {
            if let Some(g) = global(m) {
                m.edit(Some(g));
            }
        }),
    );
    click(
        &b[2],
        Box::new(move |m| {
            let Some((name, _, persisted)) = global(m) else { return };
            let q = trf("lists.globals.delete_confirm", &[("name", &name)]);
            if super::dialogs::danger(&q, tr("dialogs.cancel"), tr("btn.delete")) {
                let _ = query(&m.cmds, move |e| e.delete_global(&name, persisted));
                m.refresh();
            }
        }),
    );
    click(&b[3], Box::new(Globals::refresh));
    me
}

/// Users, quotes and credits, one at a time.
/// How many users the Community tab lists, for dev mode's `community` command.
pub static USERS_SHOWN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// What the funnel calls once a platform is shown or hidden: the list again.
type Refresh = Rc<dyn Fn()>;

/// The platforms in the Community list, in the usual order, and those the funnel hides.
#[derive(Default)]
struct Platforms {
    present: Vec<String>,
    hidden: Vec<String>,
}

impl Platforms {
    /// The platforms of the users listed now.
    fn seen<'a>(&mut self, types: impl Iterator<Item = &'a str>) {
        let mut present: Vec<String> = Vec::new();
        for t in types {
            if !present.iter().any(|p| p == t) {
                present.push(t.to_string());
            }
        }
        let order = |p: &String| {
            ["twitch", "youtube", "kick", "tiktok"]
                .iter()
                .position(|x| p.eq_ignore_ascii_case(x))
                .unwrap_or(9)
        };
        present.sort_by(|a, b| order(a).cmp(&order(b)).then_with(|| a.cmp(b)));
        self.present = present;
    }

    /// Shows or hides the `i`th platform present; one is always left showing.
    fn toggle(&mut self, i: usize) {
        let Some(p) = self.present.get(i).cloned() else { return };
        if let Some(at) = self.hidden.iter().position(|h| *h == p) {
            self.hidden.remove(at);
        } else if self.present.iter().filter(|x| !self.hidden.contains(x)).count() > 1 {
            self.hidden.push(p);
        }
    }
}

/// A platform's name as shown ("twitch" is "Twitch").
fn platform_name(p: &str) -> String {
    ["Twitch", "YouTube", "Kick", "TikTok"]
        .into_iter()
        .find(|n| n.eq_ignore_ascii_case(p))
        .map_or_else(|| p.to_string(), str::to_string)
}

pub struct Community {
    tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    show: fltk::menu::Choice,
    search: fltk::input::Input,
    views: [Flex; 3],
    users: Rows,
    /// The users' sort: the column and whether it goes up.
    sort: Rc<Cell<(usize, bool)>>,
    /// The platform column's funnel, and the platforms in the list and those hidden.
    filter: Rc<super::HeaderFilter>,
    platforms: Rc<RefCell<Platforms>>,
    users_header: Frame,
    /// The users in the list, as shown (the list's line n is `shown[n - 1]`).
    shown: Vec<(String, String)>,
    quotes: Rows,
    quote_buttons: Vec<Button>,
    credits: Rows,
    /// The engine's `community_rev` when the users and credits were last read.
    rev: u64,
}

impl Community {
    /// The page or window is no longer visible; the engine owns all historical data.
    fn release(&mut self) {
        if self.shown.is_empty() {
            return;
        }
        self.shown = Vec::new();
        self.users.release();
        self.rev = u64::MAX;
        USERS_SHOWN.store(0, std::sync::atomic::Ordering::Relaxed);
        crate::trim_memory();
    }

    /// Reloads the users or credits shown if they changed (new chatters, events) since the last look.
    pub fn live(&mut self) {
        let Some(rev) = query(&self.cmds, |e| e.community_rev()) else {
            return;
        };
        if rev != self.rev {
            self.rev = rev;
            self.refresh();
        }
    }

    pub fn refresh(&mut self) {
        let shown = self.show.value().max(0) as usize;
        for (i, v) in self.views.iter_mut().enumerate() {
            if i == shown {
                v.show();
            } else {
                v.hide();
            }
        }
        if shown == 0 {
            self.search.show();
        } else {
            self.search.hide();
        }
        self.tab.layout();
        match shown {
            0 => {
                let Some(mut users) = query(&self.cmds, |e| e.user_rows()) else {
                    return;
                };
                let (column, up) = self.sort.get();
                users.sort_by(|a, b| {
                    let by = match column {
                        0 => a.platform.cmp(&b.platform),
                        1 => a.display.to_lowercase().cmp(&b.display.to_lowercase()),
                        2 => a.role.cmp(&b.role),
                        3 => a.first_message.cmp(&b.first_message),
                        _ => a.last_active.cmp(&b.last_active),
                    }
                    .then_with(|| a.display.cmp(&b.display));
                    if up {
                        by
                    } else {
                        by.reverse()
                    }
                });
                let q = self.search.value().to_lowercase();
                // Sorting and filtering by platform show only when there's more than one.
                let hidden = {
                    let mut p = self.platforms.borrow_mut();
                    p.seen(users.iter().map(|u| u.platform.as_str()));
                    let many = p.present.len() > 1;
                    let hidden: Vec<String> = if many { p.hidden.clone() } else { Vec::new() };
                    if self.filter.shown.get() != many || self.filter.filtering.get() != !hidden.is_empty() {
                        self.filter.shown.set(many);
                        self.filter.filtering.set(!hidden.is_empty());
                        self.users_header.redraw();
                    }
                    hidden
                };
                self.users.clear();
                self.shown = Vec::new();
                for u in users
                    .into_iter()
                    .filter(|u| q.is_empty() || u.display.to_lowercase().contains(&q))
                    .filter(|u| !hidden.iter().any(|h| h == &u.platform))
                {
                    self.shown.push((u.platform.clone(), u.id.clone()));
                    let role = [
                        "",
                        tr("lists.community.role_viewer"),
                        tr("lists.community.role_vip"),
                        tr("lists.community.role_moderator"),
                        tr("lists.community.role_broadcaster"),
                    ];
                    self.users.add(&line(&[
                        u.platform,
                        u.display,
                        role.get(u.role as usize).unwrap_or(&"").to_string(),
                        day(&u.first_message),
                        day(&u.last_active),
                    ]));
                }
                USERS_SHOWN.store(self.shown.len(), std::sync::atomic::Ordering::Relaxed);
            }

            1 => {
                let sel = self.quotes.value();
                self.quotes.clear();
                for q in &self.cfg.borrow().quotes {
                    self.quotes.add(&line(&[
                        format!("#{}", q.get("id").and_then(Value::as_u64).unwrap_or(0)),
                        s(q, "quote").into(),
                        s(q, "user").into(),
                        s(q, "gameName").into(),
                        day(s(q, "timestamp")),
                    ]));
                }
                self.quotes.select(sel);
            }
            _ => {
                let Some(c) = query(&self.cmds, |e| e.credits()) else {
                    return;
                };
                self.credits.clear();
                let mut put = |title: &str, v: &Value| {
                    let names: Vec<&str> = v.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                    if !names.is_empty() {
                        self.credits.add(&line(&[title.into(), names.join(", ")]));
                    }
                };
                for (k, v) in c["events"].as_object().into_iter().flatten() {
                    put(k, v);
                }
                for (k, v) in c["users"].as_object().into_iter().flatten() {
                    put(&format!("{k} (spoke)"), v);
                }
                put("top rewards", &c["top"]["channelRewards"]);
                if self.credits.size() == 0 {
                    self.credits.add("Nothing yet this stream.");
                }
            }
        }
        self.quote_selection_changed();
    }

    fn quote_selection_changed(&mut self) {
        let eligible = self.quote_id().is_some();
        for button in &mut self.quote_buttons {
            if eligible {
                button.activate();
            } else {
                button.deactivate();
            }
        }
    }

    /// The card of the user selected: who they are, what they said, their time here.
    fn user_card(&self) {
        let n = self.users.value();
        if let Some((platform, id)) = self.shown.get((n - 1).max(0) as usize).filter(|_| n > 0) {
            let (platform, id) = (platform.clone(), id.clone());
            let Some(Some(u)) = query(&self.cmds, move |e| e.community_user(&platform, &id)) else {
                return;
            };
            super::card::open(
                &self.cmds,
                super::card::Who::from_member(&u),
                (fltk::app::event_x(), fltk::app::event_y()),
            );
        }
    }

    fn quote_id(&self) -> Option<u64> {
        let c = self.cfg.borrow();
        c.quotes
            .get((self.quotes.value() - 1).max(-1) as usize)?
            .get("id")?
            .as_u64()
    }

    /// Add (None) or edit a quote.
    fn edit_quote(&mut self, id: Option<u64>) {
        let old = id.and_then(|id| self.cfg.borrow().quotes.iter().find(|q| q["id"] == id).cloned());
        let o = old.clone().unwrap_or_else(|| {
            let c = self.cfg.borrow();
            let me = c
                .twitch
                .broadcaster
                .as_ref()
                .map(|a| (a.display_name.clone(), a.user_id.clone()));
            let (user, uid) = me.unwrap_or_default();
            json!({"quote": "", "user": user, "userId": uid, "platform": "twitch"})
                .as_object()
                .cloned()
                .unwrap_or_default()
        });
        let fields: &[F] = &[
            ("quote", "Quote", Multi),
            ("user", "Said by", Text),
            ("gameName", "Game", Text),
        ];
        let Some(o) = ask("Quote", fields, &o, &Lists::default()) else {
            return;
        };
        if s(&o, "quote").trim().is_empty() {
            return;
        }
        match id {
            Some(id) => save(&self.cfg, |c| {
                if let Some(q) = c.quotes.iter_mut().find(|q| q["id"] == id) {
                    *q = o;
                }
            }),
            None => {
                let _ = query(&self.cmds, move |e| {
                    e.add_quote(s(&o, "quote").trim(), s(&o, "user"), s(&o, "userId"), s(&o, "platform"));
                });
            }
        }
        self.refresh();
    }
}

/// The date part of an ISO time ("" for Streamer.bot's never).
/// An ISO 8601 time's date, as the user reads dates.
fn day(iso: &str) -> String {
    crate::fmt::shown_iso(iso, false)
}

fn community_tab(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Community>> {
    let mut col = tab(tabs, "Community");
    let mut top = Flex::default().row();
    top.set_pad(6);
    let l = Frame::default().with_label(tr("lists.community.show"));
    top.fixed(&l, 44);
    let mut show = super::fit::choice();
    show.set_tooltip(tr("lists.community.show"));
    show.add_choice(&format!(
        "{}|{}|{}",
        tr("lists.community.users"),
        tr("lists.community.quotes"),
        tr("lists.community.credits")
    ));
    show.set_value(0);
    top.fixed(&show, 120);
    let mut search = fltk::input::Input::default();
    search.set_tooltip(tr("lists.community.search_tooltip"));
    super::placeholder(&mut search, tr("lists.community.search_users"));
    search.set_trigger(fltk::enums::CallbackTrigger::Changed);
    // Search takes the remaining width instead of running beyond narrow windows.
    top.end();
    col.fixed(&top, ROW);

    let users_v = Flex::default().column();
    let mut users_v = users_v;
    // Newest activity first, until a title is clicked.
    let sort = Rc::new(Cell::new((4, false)));
    let titles = [
        "",
        tr("lists.community.name"),
        tr("lists.community.role"),
        tr("lists.community.first_seen"),
        tr("lists.community.last_seen"),
    ];
    // The platform column filters too: a funnel lists the platforms in the list to show or hide.
    let platforms: Rc<RefCell<Platforms>> = Rc::default();
    let after_filter: Rc<RefCell<Option<Refresh>>> = Rc::default();
    let filter = Rc::new(super::HeaderFilter {
        column: 0,
        shown: Cell::new(false),
        filtering: Cell::new(false),
        open: Box::new({
            let (platforms, after) = (platforms.clone(), after_filter.clone());
            move || {
                let items: Vec<(String, bool)> = {
                    let p = platforms.borrow();
                    p.present
                        .iter()
                        .map(|x| (platform_name(x), !p.hidden.contains(x)))
                        .collect()
                };
                let (platforms, after) = (platforms.clone(), after.clone());
                super::tick_menu(&items, move |i| {
                    platforms.borrow_mut().toggle(i);
                    let f = after.borrow().clone();
                    if let Some(f) = f {
                        f();
                    }
                });
            }
        }),
    });
    let mut users_header = super::sort_header(
        &mut users_v,
        &[36, 220, 110, 110, 110],
        &titles,
        sort.clone(),
        Some(filter.clone()),
    );
    let mut users = Rows::new(tr("lists.community.users_empty"));
    users.set_column_widths(&[36, 220, 110, 110, 110]);
    users.set_logo_column(0);
    users_v.end();

    let mut quotes_v = Flex::default().column();
    let qb = bar(&mut quotes_v, &[tr("btn.add"), tr("btn.edit"), tr("btn.delete")]);
    let quotes = browser(
        &mut quotes_v,
        &[60, 460, 140, 140, 100],
        &[
            "lists.community.number",
            "lists.community.quote",
            "lists.community.said_by",
            "lists.community.game",
            "lists.community.date",
        ],
        "lists.community.quotes_empty",
    );
    quotes.clone().set_menu(&qb);
    quotes_v.end();

    let mut credits_v = Flex::default().column();
    let cb = bar(&mut credits_v, &[tr("btn.reset"), tr("btn.fill_with_test_names")]);
    let credits = browser(&mut credits_v, &[180, 600], &[], "");
    credits.clone().set_menu(&cb);
    credits_v.end();
    col.end();

    let me = Rc::new(RefCell::new(Community {
        tab: col.clone(),
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        show: show.clone(),
        search: search.clone(),
        views: [users_v, quotes_v, credits_v],
        users,
        sort,
        filter,
        platforms,
        users_header: users_header.clone(),
        shown: Vec::new(),
        quotes: quotes.clone(),
        quote_buttons: qb[1..].to_vec(),
        credits,
        rev: 0,
    }));
    me.borrow_mut().quote_selection_changed();
    *after_filter.borrow_mut() = Some(Rc::new({
        let me = me.clone();
        move || {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.refresh();
            }
        }
    }));
    // Click a user for their card.
    users_click(&me);
    users_header.set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.refresh();
            }
        }
    });
    for (w, f) in [
        (
            show.as_base_widget(),
            Box::new(Community::refresh) as Box<dyn Fn(&mut Community)>,
        ),
        (search.as_base_widget(), Box::new(Community::refresh)),
        (qb[0].as_base_widget(), Box::new(|m: &mut Community| m.edit_quote(None))),
        (
            qb[1].as_base_widget(),
            Box::new(|m: &mut Community| m.edit_quote(m.quote_id())),
        ),
        (
            qb[2].as_base_widget(),
            Box::new(|m: &mut Community| {
                let Some(id) = m.quote_id() else { return };
                if super::dialogs::choice(
                    &trf("lists.community.delete_quote_confirm", &[("id", &id.to_string())]),
                    tr("dialogs.cancel"),
                    tr("btn.delete"),
                ) {
                    save(&m.cfg, |c| c.quotes.retain(|q| q["id"] != id));
                    m.refresh();
                }
            }),
        ),
        (
            cb[0].as_base_widget(),
            Box::new(|m: &mut Community| {
                tell(&m.cmds, |e| e.reset_credits());
                m.refresh();
            }),
        ),
        (
            cb[1].as_base_widget(),
            Box::new(|m: &mut Community| {
                let _ = query(&m.cmds, |e| drop(e.test_credits()));
                m.refresh();
            }),
        ),
    ] {
        let me = me.clone();
        let mut w = w;
        w.set_callback(move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                f(&mut m);
            }
        });
    }
    quotes.clone().set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.quote_selection_changed();
                if fltk::app::event() == fltk::enums::Event::Push && fltk::app::event_clicks() && !super::right_button()
                {
                    if let Some(id) = m.quote_id() {
                        m.edit_quote(Some(id));
                    }
                }
            }
        }
    });
    me
}

/// Streamer.bot's own servers: WebSocket, HTTP and UDP, each a line and an Edit button.
pub struct Servers {
    cfg: Cfg,
    lines: Vec<Frame>,
}

/// The settings as an object, for the form; and back.
fn to_obj(v: &impl serde::Serialize) -> Obj {
    match serde_json::to_value(v) {
        Ok(Value::Object(o)) => o,
        _ => Obj::new(),
    }
}

impl Servers {
    pub fn refresh(&mut self) {
        let c = self.cfg.borrow();
        let (w, h, u) = (&c.websockets, &c.http, &c.udp);
        let state = |enabled: bool| {
            if enabled {
                tr("lists.servers.enabled")
            } else {
                tr("lists.servers.disabled")
            }
        };
        let texts = [
            format!(
                "{}: {}, ws://{}:{}{}{}",
                tr("lists.servers.websocket_server"),
                state(w.auto_start),
                w.address,
                w.port,
                w.endpoint,
                yes(w.enable_auth, tr("lists.servers.password_tag"))
            ),
            format!(
                "{}: {}, http://{}:{}/",
                tr("lists.servers.http_server"),
                state(h.auto_start),
                h.address,
                h.port
            ),
            format!(
                "{}: {}, port {}",
                tr("lists.servers.udp_server"),
                state(u.auto_start),
                u.port
            ),
            format!(
                "{}: {}, ws://{}:{}/",
                tr("lists.servers.stream_deck_server"),
                state(c.stream_deck.auto_start),
                c.stream_deck.address,
                c.stream_deck.port
            ),
        ];
        let enabled = [w.auto_start, h.auto_start, u.auto_start, c.stream_deck.auto_start];
        for ((frame, text), enabled) in self.lines.iter_mut().zip(texts).zip(enabled) {
            (if enabled { Health::Standby } else { Health::Off }).show(frame, &text);
            frame.set_tooltip(&text.replace('@', "@@"));
        }
    }

    fn edit(&mut self, which: usize) {
        let ws: &[F] = &[
            ("autoStart", "On", Bool),
            ("address", "Address", Text),
            ("port", "Port", Int),
            ("endpoint", "Endpoint", Text),
            ("enableAuth", "Password required", Bool),
            ("authEnforce", "Password for every request", Bool),
            ("authPassword", "Password", Secret),
            ("allowedOrigins", "Websites allowed (also HTTP), one per line", Multi),
        ];
        let http: &[F] = &[
            ("autoStart", "On", Bool),
            ("address", "Address", Text),
            ("port", "Port", Int),
        ];
        let udp: &[F] = &[("autoStart", "On", Bool), ("port", "Port", Int)];
        let (title, fields, o) = {
            let c = self.cfg.borrow();
            match which {
                0 => (tr("lists.servers.websocket_server"), ws, to_obj(&c.websockets)),
                1 => (tr("lists.servers.http_server"), http, to_obj(&c.http)),
                2 => (tr("lists.servers.udp_server"), udp, to_obj(&c.udp)),
                _ => (tr("lists.servers.stream_deck_server"), http, to_obj(&c.stream_deck)),
            }
        };
        let Some(o) = ask(title, fields, &o, &Lists::default()) else {
            return;
        };
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(0);
        if !(1..=65535).contains(&port) {
            super::dialogs::alert(tr("lists.servers.port_range_warning"));
            return;
        }
        let v = Value::Object(o);
        let mut bad = None;
        save(&self.cfg, |c| {
            let r = match which {
                0 => serde_json::from_value(v).map(|x| c.websockets = x),
                1 => serde_json::from_value(v).map(|x| c.http = x),
                2 => serde_json::from_value(v).map(|x| c.udp = x),
                _ => serde_json::from_value(v).map(|x| c.stream_deck = x),
            };
            bad = r.err();
        });
        if let Some(e) = bad {
            super::dialogs::error(&e.to_string());
        }
        self.refresh();
    }
}

fn servers_box(col: &mut Flex, cfg: &Cfg) -> Rc<RefCell<Servers>> {
    title(col, tr("lists.servers.title"));
    super::hint(col, tr("lists.servers.state_hint"), 36);
    let me = Rc::new(RefCell::new(Servers {
        cfg: cfg.clone(),
        lines: Vec::new(),
    }));
    for i in 0..4 {
        let mut r = Flex::default().row();
        r.set_pad(8);
        let f = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
        let mut b = button(&mut r, tr("btn.edit"), 70);
        r.end();
        col.fixed(&r, ROW);
        b.set_callback({
            let me = me.clone();
            move |_| {
                if let Ok(mut m) = me.try_borrow_mut() {
                    m.edit(i);
                }
            }
        });
        me.borrow_mut().lines.push(f);
    }
    me
}

/// The donation services that connect out (StreamElements, Streamlabs) or poll (DonorDrive).
pub struct Donations {
    tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    lines: Vec<Frame>,
}

impl Donations {
    pub fn refresh(&mut self) {
        let runtime = query(&self.cmds, |engine| {
            let donations = engine.donations.borrow();
            (donations.streamelements.get(), donations.streamlabs.get())
        });
        let config = self.cfg.borrow();
        let donations = &config.donations;
        for (index, token) in [&donations.streamelements_token, &donations.streamlabs_token]
            .into_iter()
            .enumerate()
        {
            let (health, text) = if token.trim().is_empty() {
                (Health::Off, tr("lists.donations.off_no_token"))
            } else {
                match runtime.map(|states| if index == 0 { states.0 } else { states.1 }) {
                    Some(true) => (Health::Up, tr("lists.donations.connected")),
                    Some(false) => (Health::Down, tr("lists.donations.not_connected")),
                    None => (Health::Busy, tr("lists.stream_apps.unknown")),
                }
            };
            health.show(&mut self.lines[index], text);
        }
        let (health, text) = match (
            donations.donordrive_instance.trim(),
            donations.donordrive_participant_id.trim(),
        ) {
            ("", _) | (_, "") => (Health::Off, tr("lists.donations.status_off").to_string()),
            (site, id) => (
                Health::Standby,
                trf("lists.donations.donordrive_status", &[("id", id), ("site", site)]),
            ),
        };
        health.show(&mut self.lines[2], &text);
        self.lines[2].set_tooltip(&text.replace('@', "@@"));
    }

    fn edit(&mut self, which: usize) {
        let fields: &[F] = match which {
            0 => &[("streamelementsToken", "JWT token", Secret)],
            1 => &[("streamlabsToken", "Socket API token", Secret)],
            _ => &[
                ("donordriveInstance", "Site", Text),
                (
                    "donordriveParticipantId",
                    "Participant ID (the number in your page's address)",
                    Text,
                ),
            ],
        };
        let title = ["StreamElements", "Streamlabs", "DonorDrive"][which.min(2)];
        let o = to_obj(&self.cfg.borrow().donations);
        let Some(o) = ask(title, fields, &o, &Lists::default()) else {
            return;
        };
        if let Ok(d) = serde_json::from_value(Value::Object(o)) {
            save(&self.cfg, |c| c.donations = d);
            tell(&self.cmds, |e| e.donations_sync());
        }
        self.refresh();
    }
}

fn donations_tab(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Donations>> {
    let (scroll, mut col) = super::scroll::page(tabs, "Donations");
    let me = Rc::new(RefCell::new(Donations {
        tab: col.clone(),
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        lines: Vec::new(),
    }));
    let guides = [
        tr("lists.donations.streamelements_guide"),
        tr("lists.donations.streamlabs_guide"),
        tr("lists.donations.donordrive_guide"),
    ];
    for (i, (name, guide)) in ["StreamElements", "Streamlabs", "DonorDrive"]
        .iter()
        .zip(guides)
        .enumerate()
    {
        title(&mut col, name);
        super::hint(&mut col, guide, ROW);
        let mut r = Flex::default().row();
        r.set_pad(8);
        let f = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
        let mut b = button(&mut r, tr("btn.edit"), 70);
        r.end();
        col.fixed(&r, ROW);
        b.set_callback({
            let me = me.clone();
            move |_| {
                if let Ok(mut m) = me.try_borrow_mut() {
                    m.edit(i);
                }
            }
        });
        me.borrow_mut().lines.push(f);
    }
    Frame::default();
    col.end();
    scroll.end();
    me
}

/// User groups: added and deleted through the engine, which fires their triggers. Users get into
/// them from chat (the platforms' Add User to Group).
pub struct Groups {
    cfg: Cfg,
    cmds: Cmds,
    view: Rows,
    ids: Vec<String>,
    buttons: Vec<Button>,
}

impl Groups {
    pub fn refresh(&mut self) {
        let selected = self.selected();
        self.view.clear();
        self.ids.clear();
        for g in &self.cfg.borrow().groups {
            let users = g.get("userIds").and_then(Value::as_array).map_or(0, Vec::len);
            self.view.add(&line(&[
                s(g, "name").into(),
                trf("lists.groups.user_count", &[("count", &users.to_string())]),
            ]));
            self.ids.push(s(g, "id").into());
        }
        if let Some(index) = selected.and_then(|id| self.ids.iter().position(|candidate| *candidate == id)) {
            self.view.select(index as i32 + 1);
        }
        self.selection_changed();
    }

    fn selected(&self) -> Option<String> {
        self.ids.get(usize::try_from(self.view.value() - 1).ok()?).cloned()
    }

    fn selection_changed(&self) {
        let selected = self.selected().is_some();
        set_available(&self.buttons, &[true, selected, selected]);
    }

    fn members(&self) {
        let Some(id) = self.selected() else { return };
        // Each member: their platform, their name as last seen, and their key (platform-id).
        let read = {
            let (cfg, id) = (self.cfg.clone(), id.clone());
            move || -> (String, Vec<(String, String, String)>) {
                let config = cfg.borrow();
                let Some(group) = config.groups.iter().find(|group| s(group, "id") == id) else {
                    return (String::new(), Vec::new());
                };
                let people = crate::usergroups::members(group)
                    .into_iter()
                    .map(|key| {
                        let (platform, uid) = crate::usergroups::split(&key).unwrap_or(("twitch", key.as_str()));
                        let name = crate::usergroups::name(group, &key).unwrap_or(uid).to_string();
                        (platform.to_string(), name, key.clone())
                    })
                    .collect();
                (s(group, "name").to_string(), people)
            }
        };
        let (title, _) = read();
        let (mut window, answer) = dialogs::window(520, 380, &title);
        let mut column = Flex::new(16, 0, 488, 372, None).column();
        column.set_pad(6);
        super::header(
            &mut column,
            &[34, 260],
            &[tr("lists.groups.platform"), tr("lists.groups.user_id"), ""],
        );
        let mut members = Rows::new(tr("lists.groups.members_empty"));
        members.set_column_widths(&[34, 260]);
        members.set_logo_column(0);
        let keys: Rc<RefCell<Vec<String>>> = Rc::default();
        let fill = {
            let (members, keys, read) = (members.clone(), keys.clone(), read.clone());
            move || {
                let mut members = members.clone();
                let (_, people) = read();
                members.clear();
                keys.borrow_mut().clear();
                for (platform, name, key) in people {
                    let uid = crate::usergroups::split(&key).map_or(key.as_str(), |k| k.1).to_string();
                    members.add(&line(&[platform, name, uid]));
                    keys.borrow_mut().push(key);
                }
            }
        };
        fill();
        let mut row = Flex::default().row();
        row.set_pad(8);
        let mut remove = button(&mut row, tr("btn.remove_streamerz"), 100);
        Frame::default();
        let mut close = button(&mut row, tr("dialogs.close"), 90);
        close.set_shortcut(fltk::enums::Shortcut::from_key(fltk::enums::Key::Escape));
        close.set_callback({
            let answer = answer.clone();
            move |_| answer.set(Some(false))
        });
        row.end();
        column.fixed(&row, ROW + 4);
        column.end();
        window.end();
        members.set_menu(std::slice::from_ref(&remove));
        remove.set_callback({
            let (members, keys, cmds, gid) = (members.clone(), keys.clone(), self.cmds.clone(), id.clone());
            let fill = fill.clone();
            move |_| {
                let Some(at) = usize::try_from(members.value() - 1).ok() else {
                    return;
                };
                let Some(key) = keys.borrow().get(at).cloned() else {
                    return;
                };
                let (platform, uid) = crate::usergroups::split(&key).unwrap_or(("twitch", key.as_str()));
                let mut u = crate::model::Obj::new();
                u.insert("userId".into(), uid.into());
                u.insert("userType".into(), platform.into());
                let gid = gid.clone();
                let _ = query(&cmds, move |e| e.group_users(&gid, &[u], false));
                fill();
            }
        });
        dialogs::run(&mut window, &answer, || true);
    }
}

fn groups_tab(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Groups>> {
    let mut col = tab(tabs, "Groups");
    super::hint(&mut col, tr("lists.groups.guide"), 48);
    let view = browser(
        &mut col,
        &[300, 120],
        &["lists.groups.group", "lists.groups.users"],
        "lists.groups.empty",
    );
    let b = bar(&mut col, &[tr("btn.add"), tr("btn.members"), tr("btn.delete")]);
    view.clone().set_menu(&b);
    col.end();
    let me = Rc::new(RefCell::new(Groups {
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        view: view.clone(),
        ids: Vec::new(),
        buttons: b.clone(),
    }));
    me.borrow().selection_changed();
    view.clone().set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(groups) = me.try_borrow() {
                groups.selection_changed();
            }
        }
    });
    b[1].clone().set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(groups) = me.try_borrow() {
                groups.members();
            }
        }
    });
    b[0].clone().set_callback({
        let me = me.clone();
        move |_| {
            let Some(name) = super::ask(tr("lists.groups.new_group"), tr("lists.ask_name"), "") else {
                return;
            };
            let name = name.trim().to_string();
            if let (false, Ok(mut m)) = (name.is_empty(), me.try_borrow_mut()) {
                let _ = query(&m.cmds, move |e| drop(e.add_group(&name, false)));
                m.refresh();
            }
        }
    });
    b[2].clone().set_callback({
        let me = me.clone();
        move |_| {
            let Ok(mut m) = me.try_borrow_mut() else { return };
            let Some(id) = m.ids.get((m.view.value() - 1).max(-1) as usize).cloned() else {
                return;
            };
            if super::dialogs::choice(
                tr("lists.groups.delete_confirm"),
                tr("dialogs.cancel"),
                tr("btn.delete"),
            ) {
                let _ = query(&m.cmds, move |e| e.remove_group(&id));
                m.refresh();
            }
        }
    });
    me
}

/// A click on a user opens their card.
fn users_click(me: &Rc<RefCell<Community>>) {
    let weak = Rc::downgrade(me);
    let mut users = me.borrow().users.clone();
    users.set_callback(move |_| {
        if fltk::app::event() == fltk::enums::Event::Push {
            if let Some(m) = weak.upgrade() {
                if let Ok(m) = m.try_borrow() {
                    m.user_card();
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_list_says_how_each_of_its_columns_draws() {
        for kind in [
            &COMMANDS,
            &TIMERS,
            &VOICE_COMMANDS,
            &FILE_WATCHERS,
            &FILE_TAILS,
            &CLIENTS,
            &OBS_CONNECTIONS,
            &SLOBS_CONNECTIONS,
            &SERVERS,
        ] {
            let Value::Object(o) = (kind.new)() else {
                panic!("{}", kind.what)
            };
            let cells = (kind.label)(&o);
            assert_eq!(cells.len(), kind.titles.len(), "{}: a title per column", kind.what);
            assert_eq!(cells.len(), kind.cells.len(), "{}: a look per column", kind.what);
            assert_eq!(cells.len(), kind.columns.len(), "{}: a width per column", kind.what);
            // The switch comes first, and says what the flag is.
            assert_eq!(kind.cells[0], Some(Switch), "{}", kind.what);
            assert!(cells[0] == "1" || cells[0] == "0", "{}: {:?}", kind.what, cells[0]);
            assert_eq!(kind.live.is_some(), cells.iter().any(|c| c == LIVE), "{}", kind.what);
        }
    }

    #[test]
    fn a_commands_row_says_who_where_and_how_often() {
        let o = json!({"permittedGroups": ["Moderators"], "grantType": 0, "sources": 1 | 2 | 1024,
            "globalCooldown": 30, "userCooldown": 10})
        .as_object()
        .cloned()
        .unwrap();
        assert_eq!(who(&o), "shield|Moderators");
        assert_eq!(source_platforms(1 | 2 | 1024), "twitch,youtube");
        let deny = json!({"permittedGroups": ["VIP"], "permittedUsers": ["a"], "grantType": 1})
            .as_object()
            .cloned()
            .unwrap();
        assert!(who(&deny).starts_with("shield-off|") && who(&deny).contains("VIP"));
        assert!(who(&Obj::new()).starts_with("users|"));
        let row = (COMMANDS.label)(&o);
        assert!(row[5].starts_with("timer|"), "{row:?}");
    }

    #[test]
    fn a_files_row_says_whether_the_file_is_there() {
        assert!(found(env!("CARGO_MANIFEST_DIR")).contains(tr("lists.found")));
        assert!(found("/no/such/streamerz/file").contains(tr("lists.missing")));
        assert!(found("  ").contains(tr("lists.not_set")));
    }

    #[test]
    fn list_headers_do_not_contain_row_format_parameters() {
        for kind in [
            &COMMANDS,
            &TIMERS,
            &CLIENTS,
            &SERVERS,
            &OBS_CONNECTIONS,
            &SLOBS_CONNECTIONS,
        ] {
            for key in kind.titles.iter().filter(|key| !key.is_empty()) {
                assert!(
                    !tr(key).contains('{'),
                    "{} has a row template as its heading",
                    kind.what
                );
            }
        }
    }

    /// A new one already has every field its form sets, so none lands out of Streamer.bot's order.
    #[test]
    fn new_objects_have_their_form_fields() {
        for k in [
            &COMMANDS,
            &TIMERS,
            &CLIENTS,
            &SERVERS,
            &OBS_CONNECTIONS,
            &SLOBS_CONNECTIONS,
        ] {
            let new = (k.new)();
            for f in k.fields {
                assert!(new.get(f.0).is_some(), "{}: {} missing", k.what, f.0);
            }
        }
    }

    #[test]
    fn the_platform_filter_keeps_one_showing() {
        let mut p = Platforms::default();
        p.seen(["kick", "twitch", "kick", "tiktok"].into_iter());
        assert_eq!(p.present, ["twitch", "kick", "tiktok"]);
        p.toggle(0);
        p.toggle(1);
        assert_eq!(p.hidden, ["twitch", "kick"]);
        p.toggle(2);
        assert_eq!(p.hidden.len(), 2, "the last one showing stays");
        p.toggle(0);
        assert_eq!(p.hidden, ["kick"]);
        assert_eq!(platform_name("youtube"), "YouTube");
    }
}
