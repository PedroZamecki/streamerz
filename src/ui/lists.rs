//! The Commands, Timers, Queues, History, Globals and Servers tabs. Commands, timers and WebSocket
//! clients/servers are lists of Streamer.bot objects, each edited in a form.

use super::forms::{self, Done, Lists, F, W::*};
use super::{button, dialogs, query, tell, ROW};
use crate::engine::{Engine, Record, DEFAULT_QUEUE};
use crate::model::Obj;
use crate::Config;
use fltk::browser::HoldBrowser;
use fltk::button::Button;
use fltk::dialog;
use fltk::enums::{Align, Event, Font};
use fltk::frame::Frame;
use fltk::group::{Flex, Tabs};
use fltk::prelude::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::{mpsc::UnboundedSender, watch};

type Cfg = Arc<watch::Sender<Config>>;
type Cmds = UnboundedSender<crate::engine::UiCmd>;

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

fn on(o: &Obj, k: &str) -> bool {
    o.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// A browser line: columns split by tabs, each printed as is ("@." stops FLTK's @ formatting).
fn line(cols: &[String]) -> String {
    cols.iter()
        .map(|c| format!("@.{}", c.replace(['\t', '\r', '\n'], " ")))
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

/// Change the config and save it.
pub(super) fn save(cfg: &Cfg, f: impl FnOnce(&mut Config)) {
    cfg.send_modify(f);
    let _ = crate::save(&cfg.borrow());
}

/// A section title, with the platform's or app's logo when there's one.
pub(super) fn title(col: &mut Flex, t: &str) {
    let mut f = Frame::default().with_align(Align::Left | Align::Inside | Align::ImageNextToText);
    f.set_label_font(Font::HelveticaBold);
    match super::img::logo(t, ROW - 8) {
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

fn browser(widths: &'static [i32]) -> HoldBrowser {
    let mut b = HoldBrowser::default();
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
    label: fn(&Obj) -> Vec<String>,
    /// What the engine does after a change (restart the timers...).
    after: Option<fn(&Rc<Engine>)>,
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
    what: "Command",
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
    columns: &[180, 280, 120, 260],
    label: |o| {
        let aliases = crate::commands::aliases(o).join(", ");
        vec![
            s(o, "name").into(),
            aliases,
            s(o, "group").into(),
            if on(o, "enabled") {
                crate::engine::builtins::sources_text(o.get("sources").and_then(Value::as_u64).unwrap_or(1))
            } else {
                "disabled".into()
            },
        ]
    },
    after: None,
};

pub static TIMERS: Kind = Kind {
    what: "Timer",
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
    columns: &[220, 180, 120, 120, 80],
    label: |o| {
        let n = |k: &str| o.get(k).and_then(Value::as_i64).unwrap_or(0);
        let every = if on(o, "randomInterval") {
            format!("every {}-{} s", n("interval"), n("upperInterval"))
        } else {
            format!("every {} s", n("interval"))
        };
        vec![
            s(o, "name").into(),
            every,
            if n("lines") > 0 {
                format!("{} lines", n("lines"))
            } else {
                String::new()
            },
            format!("ran {}", n("counter")),
            yes(!on(o, "enabled"), "disabled"),
        ]
    },
    after: Some(|e| e.timers()),
};

pub static CLIENTS: Kind = Kind {
    what: "WebSocket client",
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
    columns: &[200, 400, 120],
    label: |o| {
        vec![
            s(o, "name").into(),
            s(o, "endpoint").into(),
            yes(!on(o, "autoConnect"), "manual"),
        ]
    },
    after: None,
};

pub static OBS_CONNECTIONS: Kind = Kind {
    what: "OBS connection",
    get: |c| &c.obs.connections,
    get_mut: |c| &mut c.obs.connections,
    fields: &[
        ("name", "Name", Text),
        ("host", "Host", Text),
        ("port", "Port", Int),
        ("password", "Password", Text),
        ("autoConnect", "Connect on start", Bool),
        ("reconnect", "Reconnect", Bool),
        ("retryInterval", "Retry interval (s)", Int),
    ],
    new: || {
        json!({
            "id": crate::engine::uuid(),
            "name": "OBS",
            "host": "127.0.0.1",
            "port": 4455,
            "password": "",
            "autoConnect": true,
            "reconnect": true,
            "retryInterval": 5,
            "version": 5,
        })
    },
    columns: &[150, 150, 80, 100],
    label: |o| {
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(4455);
        vec![
            s(o, "name").into(),
            format!("{}:{}", s(o, "host"), port),
            yes(!on(o, "autoConnect"), "manual"),
            yes(!on(o, "reconnect"), "no retry"),
        ]
    },
    after: Some(|e| e.obs_start().reload()),
};

pub static SLOBS_CONNECTIONS: Kind = Kind {
    what: "Streamlabs Desktop connection",
    get: |c| &c.slobs.connections,
    get_mut: |c| &mut c.slobs.connections,
    fields: &[
        ("name", "Name", Text),
        ("host", "Host", Text),
        ("port", "Port", Int),
        ("token", "Token", Text),
        ("autoConnect", "Connect on start", Bool),
        ("reconnectOnDisconnect", "Reconnect", Bool),
        ("retryInterval", "Retry interval (s)", Int),
        ("default", "Default connection", Bool),
    ],
    // Streamer.bot's field order (its settings.json).
    new: || {
        json!({"id": "", "name": "Streamlabs Desktop", "host": "127.0.0.1", "port": 59650, "autoConnect": true,
            "reconnectOnDisconnect": true, "token": "", "retryInterval": 30, "default": false})
    },
    columns: &[200, 200, 100, 100],
    label: |o| {
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(59650);
        vec![
            s(o, "name").into(),
            format!("{}:{port}", s(o, "host")),
            yes(!on(o, "autoConnect"), "manual"),
            yes(on(o, "default"), "default"),
        ]
    },
    after: Some(|e| e.slobs_sync(false)),
};

pub static SERVERS: Kind = Kind {
    what: "WebSocket server",
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
    columns: &[200, 400, 120],
    label: |o| {
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(0);
        vec![
            s(o, "name").into(),
            format!("ws://{}:{port}{}", s(o, "address"), s(o, "endpoint")),
            yes(!on(o, "autoStart"), "off"),
        ]
    },
    after: None,
};

/// One of those lists, on screen.
pub struct List {
    cfg: Cfg,
    cmds: Cmds,
    view: HoldBrowser,
    kind: &'static Kind,
    ids: Vec<String>,
}

impl List {
    pub fn refresh(&mut self) {
        let sel = self.selected();
        self.view.clear();
        self.ids.clear();
        let c = self.cfg.borrow();
        for o in (self.kind.get)(&c) {
            self.view.add(&line(&(self.kind.label)(o)));
            self.ids.push(s(o, "id").into());
        }
        if let Some(i) = sel.and_then(|id| self.ids.iter().position(|x| *x == id)) {
            self.view.select(i as i32 + 1);
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
        ask(self.kind.what, self.kind.fields, o, &Lists::new(&self.cfg.borrow()))
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
        let q = format!("Delete this {}?", self.kind.what.to_lowercase());
        if dialog::choice2_default(&q, "Cancel", "Delete", "") == Some(1) {
            self.change(|l| l.retain(|o| s(o, "id") != id));
        }
    }
}

/// Edit `o` in a form, or as JSON when asked; the result, if OK'd.
fn ask(title: &str, fields: &[F], o: &Obj, lists: &Lists) -> Option<Obj> {
    match forms::form(title, fields, o, lists) {
        Done::Ok(x) => Some(x),
        Done::Cancel => None,
        Done::Json => json(title, o),
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
fn bar(col: &mut Flex, labels: &[&str]) -> Vec<Button> {
    let mut row = Flex::default().row();
    row.set_pad(4);
    let out = labels
        .iter()
        .map(|l| button(&mut row, l, (l.len() as i32 * 8 + 24).max(60)))
        .collect();
    Frame::default();
    row.end();
    col.fixed(&row, ROW + 2);
    out
}

/// A list of `kind`, with Add, Edit, On/Off (when `toggle` names the flag) and Delete.
fn list(
    col: &mut Flex,
    cfg: &Cfg,
    cmds: &Cmds,
    kind: &'static Kind,
    toggle: Option<&'static str>,
) -> Rc<RefCell<List>> {
    let view = browser(kind.columns);
    let mut labels = vec!["Add", "Edit"];
    labels.extend(toggle.map(|_| "On/Off"));
    labels.push("Delete");
    let b = bar(col, &labels);
    let me = Rc::new(RefCell::new(List {
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        view: view.clone(),
        kind,
        ids: Vec::new(),
    }));
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
    view.clone().handle({
        let me = me.clone();
        move |_, ev| {
            if ev == Event::Push && fltk::app::event_clicks() {
                if let Ok(mut m) = me.try_borrow_mut() {
                    m.edit();
                }
            }
            false
        }
    });
    me
}

fn tab(tabs: &Tabs, label: &str) -> Flex {
    super::tab(tabs, label)
}

/// What the tabs show that can change while they're up.
pub struct Views {
    pub lists: Vec<Rc<RefCell<List>>>,
    pub queues: Rc<RefCell<Queues>>,
    pub history: Rc<RefCell<History>>,
    pub globals: Rc<RefCell<Globals>>,
    pub servers: Rc<RefCell<Servers>>,
    pub groups: Rc<RefCell<Groups>>,
    pub accounts: Rc<RefCell<super::accounts::Accounts>>,
    pub chat: Rc<RefCell<super::chat::Chat>>,
}

impl Views {
    /// New chat lines: added to the Chat tab when it's up (it catches up when shown).
    pub fn chat_refresh(&self, tabs: &Tabs) {
        let up = tabs.value().map(|g| g.as_widget_ptr());
        if let Ok(mut c) = self.chat.try_borrow_mut() {
            if up == Some(c.tab.as_widget_ptr()) {
                c.refresh();
            }
        }
    }

    /// A picture arrived: redraw what shows pictures.
    pub fn images(&self) {
        if let Ok(mut a) = self.accounts.try_borrow_mut() {
            a.refresh();
        }
        if let Ok(c) = self.chat.try_borrow() {
            c.images();
        }
    }

    /// Redraw what the config shows, and the live views on the tab that's up.
    pub fn refresh(&self, tabs: &Tabs) {
        let up = tabs.value().map(|g| g.as_widget_ptr());
        for l in &self.lists {
            l.borrow_mut().refresh();
        }
        self.servers.borrow_mut().refresh();
        if let Ok(mut g) = self.groups.try_borrow_mut() {
            g.refresh();
        }
        let is = |w: &Flex| up == Some(w.as_widget_ptr());
        if let Ok(mut a) = self.accounts.try_borrow_mut() {
            if is(&a.tab) {
                a.refresh();
            }
        }
        self.chat_refresh(tabs);
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
    }
}

pub fn tabs(tabs: &Tabs, cfg: &Cfg, cmds: &Cmds) -> Views {
    let accounts = super::accounts::tab(tabs, cfg, cmds);
    let chat = super::chat::tab(tabs, cfg, cmds);
    let mut col = tab(tabs, "Commands");
    let commands = list(&mut col, cfg, cmds, &COMMANDS, Some("enabled"));
    col.end();

    let mut col = tab(tabs, "Timers");
    let timers = list(&mut col, cfg, cmds, &TIMERS, Some("enabled"));
    col.end();

    let queues = queues_tab(tabs, cfg, cmds);
    let history = history_tab(tabs, cmds);
    let globals = globals_tab(tabs, cmds);
    let groups = groups_tab(tabs, cfg, cmds);

    let mut col = tab(tabs, "Servers");
    let servers = servers_box(&mut col, cfg);
    title(&mut col, "WebSocket clients");
    let clients = list(&mut col, cfg, cmds, &CLIENTS, None);
    title(&mut col, "WebSocket servers");
    let custom = list(&mut col, cfg, cmds, &SERVERS, Some("autoStart"));
    col.end();

    let mut col = tab(tabs, "Stream Apps");
    title(&mut col, "OBS Studio");
    let obs_guide = Frame::default()
        .with_label("• In OBS Studio: Tools > WebSocket Server Settings > check 'Enable WebSocket server' (port 4455).\n• Add a connection below matching your port & password, then click Connect All.")
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&obs_guide, 36);
    let obs = list(&mut col, cfg, cmds, &OBS_CONNECTIONS, Some("autoConnect"));
    let mut b_row = Flex::default().row();
    let mut b_connect = button(&mut b_row, "Connect All", 120);
    let mut b_disconnect = button(&mut b_row, "Disconnect All", 120);
    Frame::default();
    b_row.end();
    col.fixed(&b_row, ROW);
    let cmds_c = cmds.clone();
    b_connect.set_callback(move |_| {
        tell(&cmds_c, |e| {
            e.obs_start().connect(None);
        });
    });
    let cmds_d = cmds.clone();
    b_disconnect.set_callback(move |_| {
        tell(&cmds_d, |e| {
            e.obs_start().disconnect(None);
        });
    });
    title(&mut col, "Streamlabs Desktop");
    let slobs_guide = Frame::default()
        .with_label("• In Streamlabs Desktop: Settings > API / Remote Control > generate and copy your API Token.\n• Add a connection below (port 59650) with your API Token, then click Connect All.")
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&slobs_guide, 36);
    let slobs = list(&mut col, cfg, cmds, &SLOBS_CONNECTIONS, Some("autoConnect"));
    let mut b_row = Flex::default().row();
    let mut b_connect = button(&mut b_row, "Connect All", 120);
    let mut b_disconnect = button(&mut b_row, "Disconnect All", 120);
    Frame::default();
    b_row.end();
    col.fixed(&b_row, ROW);
    let c = cmds.clone();
    b_connect.set_callback(move |_| tell(&c, |e| e.slobs_connect("")));
    let c = cmds.clone();
    b_disconnect.set_callback(move |_| tell(&c, |e| e.slobs_disconnect("")));
    col.end();

    Views {
        lists: vec![commands, timers, clients, custom, obs, slobs],
        queues,
        history,
        globals,
        servers,
        groups,
        accounts,
        chat,
    }
}

/// Queues: the configured ones (Default first) and what's running and waiting in them.
pub struct Queues {
    tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    view: HoldBrowser,
    pending: HoldBrowser,
    ids: Vec<String>,
}

impl Queues {
    pub fn refresh(&mut self) {
        let Some((paused, pending)) = query(&self.cmds, |e| (e.paused(), e.pending())) else {
            return;
        };
        let sel = self.selected();
        let c = self.cfg.borrow();
        let mut rows = vec![(DEFAULT_QUEUE.to_string(), "Default".to_string(), c.actions.blocking)];
        rows.extend(
            c.actions
                .queues
                .iter()
                .map(|q| (s(q, "id").into(), s(q, "name").into(), on(q, "blocking"))),
        );
        let name = |id: &str| rows.iter().find(|r| r.0 == id).map_or(id, |r| r.1.as_str()).to_string();
        self.view.clear();
        self.ids.clear();
        for (id, n, blocking) in &rows {
            let count = |running: bool| pending.iter().filter(|p| p.0 == *id && p.2 == running).count();
            self.view.add(&line(&[
                n.clone(),
                yes(*blocking, "blocking"),
                yes(paused.contains(id), "paused"),
                format!("{} running", count(true)),
                format!("{} waiting", count(false)),
            ]));
            self.ids.push(id.clone());
        }
        self.pending.clear();
        for (q, action, running) in &pending {
            self.pending.add(&line(&[
                name(q),
                action.clone(),
                if *running { "running" } else { "waiting" }.into(),
            ]));
        }
        drop(c);
        if let Some(i) = sel.and_then(|id| self.ids.iter().position(|x| *x == id)) {
            self.view.select(i as i32 + 1);
        }
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
            None => json!({"id": crate::engine::uuid(), "name": "New queue", "blocking": false})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        };
        let fields: &[F] = &[("name", "Name", Text), ("blocking", "Blocking", Bool)];
        let Some(o) = ask("Queue", fields, &o, &Lists::new(&self.cfg.borrow())) else {
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

fn queues_tab(tabs: &Tabs, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Queues>> {
    let mut col = tab(tabs, "Queues");
    let view = browser(&[220, 100, 80, 100, 100]);
    let b = bar(
        &mut col,
        &[
            "Add",
            "Edit",
            "Delete",
            "Blocking",
            "Pause/Resume",
            "Clear",
            "Pause all",
            "Resume all",
        ],
    );
    title(&mut col, "Pending");
    let pending = browser(&[220, 420, 100]);
    col.fixed(&pending, 200);
    col.end();
    let me = Rc::new(RefCell::new(Queues {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        view,
        pending,
        ids: Vec::new(),
    }));
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
            let q = "Delete this queue? Its actions move to the Default queue.";
            if dialog::choice2_default(q, "Cancel", "Delete", "") == Some(1) {
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
    view: HoldBrowser,
    runs: Vec<Record>,
}

impl History {
    pub fn refresh(&mut self) {
        let Some(runs) = query(&self.cmds, |e| e.history().iter().rev().cloned().collect::<Vec<_>>()) else {
            return;
        };
        let sel = self.view.value();
        self.view.clear();
        for r in &runs {
            // "2026-10-12T14:03:05.123-03:00": the date and time, to the second.
            let at = r.queued_at.get(..19).unwrap_or(&r.queued_at).replace('T', " ");
            self.view.add(&line(&[
                at,
                r.name.clone(),
                format!("{:.0} ms", r.ms),
                format!("{} variables", r.after.len()),
            ]));
        }
        self.runs = runs;
        if sel > 0 {
            self.view.select(sel);
        }
    }

    fn selected(&self) -> Option<&Record> {
        self.runs.get((self.view.value() - 1).max(-1) as usize)
    }
}

fn history_tab(tabs: &Tabs, cmds: &Cmds) -> Rc<RefCell<History>> {
    let mut col = tab(tabs, "History");
    let view = browser(&[180, 420, 100, 120]);
    let b = bar(
        &mut col,
        &[
            "Variables when queued",
            "Variables after run",
            "Run again",
            "Clear",
            "Refresh",
        ],
    );
    col.end();
    let me = Rc::new(RefCell::new(History {
        tab: col,
        cmds: cmds.clone(),
        view,
        runs: Vec::new(),
    }));
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
                let v = Value::Object(if after { r.after.clone() } else { r.args.clone() });
                dialogs::json_editor(&r.name, "", &v, |_| Ok(()));
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
    view: HoldBrowser,
    /// Each row: (name, value, persisted, a user global's user).
    rows: Vec<(String, Value, bool, Option<String>)>,
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
        let sel = self.view.value();
        self.view.clear();
        for (name, v, persisted, user) in &rows {
            self.view.add(&line(&[
                name.clone(),
                crate::vars::text(v),
                if *persisted { "persisted" } else { "temporary" }.into(),
                user.clone().unwrap_or_default(),
            ]));
        }
        self.rows = rows;
        if sel > 0 {
            self.view.select(sel);
        }
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
        let Some(o) = ask("Global", fields, o, &Lists::default()) else {
            return;
        };
        let name = s(&o, "variableName").trim().to_string();
        if name.is_empty() {
            return;
        }
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

fn globals_tab(tabs: &Tabs, cmds: &Cmds) -> Rc<RefCell<Globals>> {
    let mut col = tab(tabs, "Globals");
    let view = browser(&[220, 420, 100, 200]);
    let b = bar(&mut col, &["Add", "Edit", "Delete", "Refresh"]);
    col.end();
    let me = Rc::new(RefCell::new(Globals {
        tab: col,
        cmds: cmds.clone(),
        view,
        rows: Vec::new(),
    }));
    let click = |b: &Button, f: Box<dyn Fn(&mut Globals)>| {
        let me = me.clone();
        b.clone().set_callback(move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                f(&mut m);
            }
        });
    };
    // ponytail: user globals are shown, not edited here; Global (Set) with User does that.
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
            let q = format!("Delete the global {}?", name.replace('@', "@@"));
            if dialog::choice2_default(&q, "Cancel", "Delete", "") == Some(1) {
                let _ = query(&m.cmds, move |e| e.delete_global(&name, persisted));
                m.refresh();
            }
        }),
    );
    click(&b[3], Box::new(Globals::refresh));
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
        let state = |on: bool| if on { "on" } else { "off" };
        let texts = [
            format!(
                "WebSocket server: {}, ws://{}:{}{}{}",
                state(w.auto_start),
                w.address,
                w.port,
                w.endpoint,
                yes(w.enable_auth, ", password")
            ),
            format!("HTTP server: {}, http://{}:{}/", state(h.auto_start), h.address, h.port),
            format!("UDP server: {}, port {}", state(u.auto_start), u.port),
        ];
        for (f, t) in self.lines.iter_mut().zip(texts) {
            f.set_label(&t.replace('@', "@@"));
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
            ("authPassword", "Password", Text),
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
                0 => ("WebSocket server", ws, to_obj(&c.websockets)),
                1 => ("HTTP server", http, to_obj(&c.http)),
                _ => ("UDP server", udp, to_obj(&c.udp)),
            }
        };
        let Some(o) = ask(title, fields, &o, &Lists::default()) else {
            return;
        };
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(0);
        if !(1..=65535).contains(&port) {
            dialog::alert_default("Port: 1 to 65535, please.");
            return;
        }
        let v = Value::Object(o);
        let mut bad = None;
        save(&self.cfg, |c| {
            let r = match which {
                0 => serde_json::from_value(v).map(|x| c.websockets = x),
                1 => serde_json::from_value(v).map(|x| c.http = x),
                _ => serde_json::from_value(v).map(|x| c.udp = x),
            };
            bad = r.err();
        });
        if let Some(e) = bad {
            dialog::alert_default(&e.to_string());
        }
        self.refresh();
    }
}

fn servers_box(col: &mut Flex, cfg: &Cfg) -> Rc<RefCell<Servers>> {
    title(col, "Streamer.bot's servers (changes apply at once)");
    let me = Rc::new(RefCell::new(Servers {
        cfg: cfg.clone(),
        lines: Vec::new(),
    }));
    for i in 0..3 {
        let mut r = Flex::default().row();
        let f = Frame::default().with_align(Align::Left | Align::Inside);
        let mut b = button(&mut r, "Edit", 70);
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

/// User groups: added and deleted through the engine, which fires their triggers. Users get into
/// them from chat (the platforms' Add User to Group).
pub struct Groups {
    cfg: Cfg,
    cmds: Cmds,
    view: HoldBrowser,
    ids: Vec<String>,
}

impl Groups {
    pub fn refresh(&mut self) {
        let sel = self.view.value();
        self.view.clear();
        self.ids.clear();
        for g in &self.cfg.borrow().groups {
            let users = g.get("userIds").and_then(Value::as_array).map_or(0, Vec::len);
            self.view.add(&line(&[s(g, "name").into(), format!("{users} users")]));
            self.ids.push(s(g, "id").into());
        }
        if sel > 0 {
            self.view.select(sel);
        }
    }
}

fn groups_tab(tabs: &Tabs, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Groups>> {
    let mut col = tab(tabs, "Groups");
    let view = browser(&[300, 120]);
    let b = bar(&mut col, &["Add", "Delete"]);
    col.end();
    let me = Rc::new(RefCell::new(Groups {
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        view,
        ids: Vec::new(),
    }));
    b[0].clone().set_callback({
        let me = me.clone();
        move |_| {
            let Some(name) = super::ask("New group", "Name:", "") else {
                return;
            };
            let name = name.trim().to_string();
            if let (false, Ok(mut m)) = (name.is_empty(), me.try_borrow_mut()) {
                let _ = query(&m.cmds, move |e| drop(e.add_group(&name, false)));
                m.refresh();
            }
        }
    });
    b[1].clone().set_callback({
        let me = me.clone();
        move |_| {
            let Ok(mut m) = me.try_borrow_mut() else { return };
            let Some(id) = m.ids.get((m.view.value() - 1).max(-1) as usize).cloned() else {
                return;
            };
            if dialog::choice2_default("Delete this group?", "Cancel", "Delete", "") == Some(1) {
                let _ = query(&m.cmds, move |e| e.remove_group(&id));
                m.refresh();
            }
        }
    });
    me
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
