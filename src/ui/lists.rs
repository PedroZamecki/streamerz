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
use std::sync::atomic::{AtomicBool, Ordering};
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
fn save(cfg: &Cfg, f: impl FnOnce(&mut Config)) {
    cfg.send_modify(f);
    let _ = crate::save(&cfg.borrow());
}

fn title(col: &mut Flex, t: &str) {
    let mut f = Frame::default().with_label(t).with_align(Align::Left | Align::Inside);
    f.set_label_font(Font::HelveticaBold);
    col.fixed(&f, ROW - 4);
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
        ("globalCooldown", "Global cooldown (s)", Int),
        ("userCooldown", "User cooldown (s)", Int),
        ("grantType", "Grant type", Pick(&["Allow", "Deny"])),
        ("permittedGroups", "Groups, one per line", Lines),
    ],
    // Streamer.bot's field order (from its exports).
    new: || {
        json!({"permittedUsers": [], "permittedGroups": [], "id": "", "name": "New command", "enabled": true,
            "include": true, "mode": 0, "command": "!new", "regexExplicitCapture": false, "location": 0,
            "ignoreBotAccount": true, "ignoreInternal": true, "sources": 1, "persistCounter": false,
            "persistUserCounter": false, "caseSensitive": false, "globalCooldown": 0, "userCooldown": 0,
            "group": "", "grantType": 0})
    },
    columns: &[200, 330, 150, 80],
    label: |o| {
        let aliases = crate::commands::aliases(o).join(", ");
        vec![
            s(o, "name").into(),
            aliases,
            s(o, "group").into(),
            yes(!on(o, "enabled"), "disabled"),
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
    me.borrow_mut().refresh();
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
    pub twitch: Rc<RefCell<TwitchView>>,
    pub kick: Rc<RefCell<KickView>>,
    pub youtube: Rc<RefCell<YouTubeView>>,
}

impl Views {
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
        if let Ok(mut t) = self.twitch.try_borrow_mut() {
            if is(&t.tab) {
                t.refresh();
            }
        }
        if let Ok(mut k) = self.kick.try_borrow_mut() {
            if is(&k.tab) {
                k.refresh();
            }
        }
        if let Ok(mut y) = self.youtube.try_borrow_mut() {
            if is(&y.tab) {
                y.refresh();
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
    }
}

pub fn tabs(tabs: &Tabs, cfg: &Cfg, cmds: &Cmds) -> Views {
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

    let twitch = twitch_tab(tabs, cfg, cmds);
    let kick = kick_tab(tabs, cfg, cmds);
    let youtube = youtube_tab(tabs, cfg, cmds);

    let mut col = tab(tabs, "OBS");
    title(&mut col, "OBS Studio Connections");
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
    col.end();

    Views {
        lists: vec![commands, timers, clients, custom, obs],
        queues,
        history,
        globals,
        servers,
        groups,
        twitch,
        kick,
        youtube,
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
    me.borrow_mut().refresh();
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
    me.borrow_mut().refresh();
    me
}

fn open_browser(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

pub struct TwitchView {
    pub tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    broadcaster_info: Frame,
    bot_info: Frame,
    status_label: Frame,
    auto_connect: fltk::button::CheckButton,
}

impl TwitchView {
    pub fn refresh(&mut self) {
        // Before borrowing the config: the engine may be waiting to write it.
        let connected = query(&self.cmds, |e| e.twitch_connected()).unwrap_or(false);
        let c = self.cfg.borrow();
        let b_text = match &c.twitch.broadcaster {
            Some(b) => format!("Broadcaster: {} (@{}) [ID: {}]", b.display_name, b.login, b.user_id),
            None => "Broadcaster: Not logged in".to_string(),
        };
        self.broadcaster_info.set_label(&b_text);

        let bot_text = match &c.twitch.bot {
            Some(bot) => format!("Bot: {} (@{}) [ID: {}]", bot.display_name, bot.login, bot.user_id),
            None => "Bot: Not logged in".to_string(),
        };
        self.bot_info.set_label(&bot_text);

        self.auto_connect.set_checked(c.twitch.auto_connect);

        let st = if connected {
            "EventSub: Connected"
        } else if c.twitch.broadcaster.is_none() {
            "EventSub: Disconnected (No Broadcaster)"
        } else {
            "EventSub: Disconnected"
        };
        self.status_label.set_label(st);
    }
}

fn device_login(cfg: &Cfg, cmds: &Cmds, as_bot: bool) {
    let who = if as_bot { "Bot" } else { "Broadcaster" };
    let scopes = if as_bot {
        crate::twitch::BOT_SCOPES
    } else {
        crate::twitch::BROADCASTER_SCOPES
    };

    let (tx, rx) = std::sync::mpsc::channel();
    tell(cmds, move |_| {
        tokio::task::spawn_local(async move {
            let res = crate::twitch::device_flow_start(crate::twitch::CLIENT_ID, scopes).await;
            let _ = tx.send(res);
        });
    });

    let code_resp = match rx.recv() {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            dialog::alert_default(&format!("Failed to start login: {e}"));
            return;
        }
        Err(_) => return,
    };

    let user_code = code_resp.user_code.clone();
    let device_code = code_resp.device_code.clone();
    let interval = code_resp.interval.max(1);

    open_browser(&code_resp.verification_uri);

    let (mut win, answer) = dialogs::window(460, 220, &format!("Twitch Login - {who}"));
    let mut col = Flex::new(10, 10, 440, 200, None).column();
    col.set_pad(8);

    let msg = format!(
        "Please authorize streamerz on Twitch.\n\nActivation code:\n  {}\n\nBrowser opened to:\n  {}\n\nWaiting for authorization...",
        user_code, code_resp.verification_uri
    );
    let msg_frame = Frame::default()
        .with_label(&msg)
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&msg_frame, 140);

    let mut bar = Flex::default().row();
    let mut open_btn = button(&mut bar, "Open Browser", 110);
    open_btn.set_callback({
        let uri = code_resp.verification_uri.clone();
        move |_| open_browser(&uri)
    });
    let mut copy_btn = button(&mut bar, "Copy Code", 90);
    copy_btn.set_callback({
        let code = user_code.clone();
        move |_| fltk::app::copy(&code)
    });
    Frame::default();
    let mut cancel_btn = button(&mut bar, "Cancel", 80);
    cancel_btn.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let is_cancelled = cancelled.clone();
    let cfg_clone = cfg.clone();

    tell(cmds, move |e| {
        let e = e.clone();
        tokio::task::spawn_local(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval));
            for _ in 0..(code_resp.expires_in / interval) {
                ticker.tick().await;
                if is_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match crate::twitch::device_flow_poll(crate::twitch::CLIENT_ID, &device_code).await {
                    Ok(Some(token)) => {
                        match crate::twitch::fetch_user(crate::twitch::CLIENT_ID, &token.access_token, None).await {
                            Ok(mut acct) => {
                                let expires_at = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map_or(0, |d| d.as_secs())
                                    + token.expires_in;
                                acct.token = crate::twitch::TwitchToken {
                                    access_token: token.access_token,
                                    refresh_token: token.refresh_token,
                                    scope: token.scope,
                                    expires_at,
                                };
                                let (name, user_id) = (acct.display_name.clone(), acct.user_id.clone());
                                save(&cfg_clone, |c| {
                                    if as_bot {
                                        c.twitch.bot = Some(acct.clone());
                                    } else {
                                        c.twitch.broadcaster = Some(acct.clone());
                                    }
                                });
                                if !as_bot {
                                    let mut args = Obj::new();
                                    args.insert("user".into(), name.clone().into());
                                    args.insert("userId".into(), user_id.into());
                                    e.fire(
                                        crate::twitch::T_AUTHED,
                                        "Broadcaster Authenticated",
                                        "Twitch/Connections",
                                        "twitch",
                                        &args,
                                        |_| true,
                                    );
                                    e.emit("Twitch", "BroadcasterAuthenticated", || json!({"user": name}));
                                }
                                crate::say(format!("Twitch {who} logged in as {name}"));
                                let _ = done_tx.send(Ok(name));
                            }
                            Err(e) => {
                                let _ = done_tx.send(Err(e));
                            }
                        }
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        let _ = done_tx.send(Err(e));
                        break;
                    }
                }
            }
        });
    });

    win.show();
    while win.shown() && answer.get().is_none() {
        if let Ok(res) = done_rx.try_recv() {
            match res {
                Ok(name) => {
                    win.hide();
                    dialog::message_default(&format!("Successfully logged in {who} as {name}!"));
                    break;
                }
                Err(e) => {
                    win.hide();
                    dialog::alert_default(&format!("Login failed: {e}"));
                    break;
                }
            }
        }
        fltk::app::wait();
    }
    cancelled.store(true, Ordering::Relaxed);
    win.hide();
}

fn twitch_tab(tabs: &Tabs, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<TwitchView>> {
    let mut col = tab(tabs, "Twitch");
    title(&mut col, "Twitch Broadcaster Account");
    let broadcaster_info = Frame::default().with_align(Align::Left | Align::Inside);
    col.fixed(&broadcaster_info, ROW);

    let mut b_row = Flex::default().row();
    let mut b_login = button(&mut b_row, "Login Broadcaster", 150);
    let mut b_logout = button(&mut b_row, "Logout", 90);
    Frame::default();
    b_row.end();
    col.fixed(&b_row, ROW);

    title(&mut col, "Twitch Bot Account");
    let bot_info = Frame::default().with_align(Align::Left | Align::Inside);
    col.fixed(&bot_info, ROW);

    let mut bot_row = Flex::default().row();
    let mut bot_login = button(&mut bot_row, "Login Bot", 150);
    let mut bot_logout = button(&mut bot_row, "Logout", 90);
    Frame::default();
    bot_row.end();
    col.fixed(&bot_row, ROW);

    title(&mut col, "Connection");
    let status_label = Frame::default().with_align(Align::Left | Align::Inside);
    col.fixed(&status_label, ROW);

    let mut conn_row = Flex::default().row();
    let mut auto_connect = fltk::button::CheckButton::default().with_label("Auto-connect on startup");
    conn_row.fixed(&auto_connect, 180);
    let mut connect_btn = button(&mut conn_row, "Connect", 90);
    let mut disconnect_btn = button(&mut conn_row, "Disconnect", 90);
    Frame::default();
    conn_row.end();
    col.fixed(&conn_row, ROW);

    Frame::default();
    col.end();

    let me = Rc::new(RefCell::new(TwitchView {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        broadcaster_info,
        bot_info,
        status_label,
        auto_connect: auto_connect.clone(),
    }));

    b_login.set_callback({
        let cfg = cfg.clone();
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            device_login(&cfg, &cmds, false);
            me.borrow_mut().refresh();
        }
    });

    b_logout.set_callback({
        let cfg = cfg.clone();
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            save(&cfg, |c| c.twitch.broadcaster = None);
            tell(&cmds, |e| {
                if let Some(tw) = e.twitch() {
                    tw.send(crate::twitch::TwitchCmd::Disconnect);
                }
            });
            me.borrow_mut().refresh();
        }
    });

    bot_login.set_callback({
        let cfg = cfg.clone();
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            device_login(&cfg, &cmds, true);
            me.borrow_mut().refresh();
        }
    });

    bot_logout.set_callback({
        let cfg = cfg.clone();
        let me = me.clone();
        move |_| {
            save(&cfg, |c| c.twitch.bot = None);
            me.borrow_mut().refresh();
        }
    });

    auto_connect.set_callback({
        let cfg = cfg.clone();
        let auto_connect = auto_connect.clone();
        move |_| {
            let on = auto_connect.is_checked();
            save(&cfg, |c| c.twitch.auto_connect = on);
        }
    });

    connect_btn.set_callback({
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            tell(&cmds, |e| e.twitch_start().send(crate::twitch::TwitchCmd::Connect));
            me.borrow_mut().refresh();
        }
    });

    disconnect_btn.set_callback({
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            tell(&cmds, |e| {
                if let Some(tw) = e.twitch() {
                    tw.send(crate::twitch::TwitchCmd::Disconnect);
                }
            });
            me.borrow_mut().refresh();
        }
    });

    me.borrow_mut().refresh();
    me
}

pub struct KickView {
    pub tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    channel_input: fltk::input::Input,
    status_label: Frame,
    auto_connect: fltk::button::CheckButton,
}

impl KickView {
    pub fn refresh(&mut self) {
        // Before borrowing the config: the engine may be waiting to write it.
        let connected = query(&self.cmds, |e| e.kick_connected()).unwrap_or(false);
        let c = self.cfg.borrow();
        if self.channel_input.value() != c.kick.channel {
            self.channel_input.set_value(&c.kick.channel);
        }
        self.auto_connect.set_checked(c.kick.auto_connect);
        let st = if connected {
            format!(
                "Pusher WebSocket: Connected (channel: {})",
                if c.kick.channel.is_empty() {
                    "none"
                } else {
                    &c.kick.channel
                }
            )
        } else if c.kick.channel.is_empty() {
            "Pusher WebSocket: Disconnected (No channel configured)".to_string()
        } else {
            "Pusher WebSocket: Disconnected".to_string()
        };
        self.status_label.set_label(&st);
    }
}

fn kick_tab(tabs: &Tabs, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<KickView>> {
    let mut col = tab(tabs, "Kick");
    title(&mut col, "Kick Channel Configuration");

    let mut ch_row = Flex::default().row();
    let ch_lbl = Frame::default().with_label("Channel:");
    ch_row.fixed(&ch_lbl, 80);
    let mut channel_input = fltk::input::Input::default();
    ch_row.end();
    col.fixed(&ch_row, ROW);

    title(&mut col, "Connection");
    let status_label = Frame::default().with_align(Align::Left | Align::Inside);
    col.fixed(&status_label, ROW);

    let mut conn_row = Flex::default().row();
    let mut auto_connect = fltk::button::CheckButton::default().with_label("Auto-connect on startup");
    conn_row.fixed(&auto_connect, 180);
    let mut connect_btn = button(&mut conn_row, "Connect", 90);
    let mut disconnect_btn = button(&mut conn_row, "Disconnect", 90);
    Frame::default();
    conn_row.end();
    col.fixed(&conn_row, ROW);

    Frame::default();
    col.end();

    let me = Rc::new(RefCell::new(KickView {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        channel_input: channel_input.clone(),
        status_label,
        auto_connect: auto_connect.clone(),
    }));

    channel_input.set_callback({
        let cfg = cfg.clone();
        let channel_input = channel_input.clone();
        move |_| {
            let val = channel_input.value().trim().to_string();
            save(&cfg, |c| c.kick.channel = val);
        }
    });

    auto_connect.set_callback({
        let cfg = cfg.clone();
        let auto_connect = auto_connect.clone();
        move |_| {
            let on = auto_connect.is_checked();
            save(&cfg, |c| c.kick.auto_connect = on);
        }
    });

    connect_btn.set_callback({
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            tell(&cmds, |e| {
                e.kick_start().send(crate::kick::KickCmd::Connect);
            });
            me.borrow_mut().refresh();
        }
    });

    disconnect_btn.set_callback({
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            tell(&cmds, |e| {
                if let Some(k) = e.kick() {
                    k.send(crate::kick::KickCmd::Disconnect);
                }
            });
            me.borrow_mut().refresh();
        }
    });

    me.borrow_mut().refresh();
    me
}

pub struct YouTubeView {
    pub tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    channel_input: fltk::input::Input,
    video_input: fltk::input::Input,
    status_label: Frame,
    auto_connect: fltk::button::CheckButton,
}

impl YouTubeView {
    pub fn refresh(&mut self) {
        // Before borrowing the config: the engine may be waiting to write it.
        let connected = query(&self.cmds, |e| e.youtube_connected()).unwrap_or(false);
        let c = self.cfg.borrow();
        if self.channel_input.value() != c.youtube.channel_id {
            self.channel_input.set_value(&c.youtube.channel_id);
        }
        if self.video_input.value() != c.youtube.video_id {
            self.video_input.set_value(&c.youtube.video_id);
        }
        self.auto_connect.set_checked(c.youtube.auto_connect);
        let st = if connected {
            format!(
                "InnerTube / Live Chat: Monitoring Active ({})",
                if !c.youtube.video_id.is_empty() {
                    format!("video: {}", c.youtube.video_id)
                } else if !c.youtube.channel_id.is_empty() {
                    format!("channel: {}", c.youtube.channel_id)
                } else {
                    "no broadcast configured".into()
                }
            )
        } else if c.youtube.video_id.is_empty() && c.youtube.channel_id.is_empty() {
            "InnerTube / Live Chat: Disconnected (No channel or video ID)".to_string()
        } else {
            "InnerTube / Live Chat: Disconnected".to_string()
        };
        self.status_label.set_label(&st);
    }
}

fn youtube_tab(tabs: &Tabs, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<YouTubeView>> {
    let mut col = tab(tabs, "YouTube");
    title(&mut col, "YouTube Broadcast Configuration");

    let mut ch_row = Flex::default().row();
    let ch_lbl = Frame::default().with_label("Channel ID:");
    ch_row.fixed(&ch_lbl, 90);
    let mut channel_input = fltk::input::Input::default();
    ch_row.end();
    col.fixed(&ch_row, ROW);

    let mut vid_row = Flex::default().row();
    let vid_lbl = Frame::default().with_label("Video ID:");
    vid_row.fixed(&vid_lbl, 90);
    let mut video_input = fltk::input::Input::default();
    vid_row.end();
    col.fixed(&vid_row, ROW);

    title(&mut col, "Connection");
    let status_label = Frame::default().with_align(Align::Left | Align::Inside);
    col.fixed(&status_label, ROW);

    let mut conn_row = Flex::default().row();
    let mut auto_connect = fltk::button::CheckButton::default().with_label("Auto-connect on startup");
    conn_row.fixed(&auto_connect, 180);
    let mut connect_btn = button(&mut conn_row, "Connect", 90);
    let mut disconnect_btn = button(&mut conn_row, "Disconnect", 90);
    Frame::default();
    conn_row.end();
    col.fixed(&conn_row, ROW);

    Frame::default();
    col.end();

    let me = Rc::new(RefCell::new(YouTubeView {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        channel_input: channel_input.clone(),
        video_input: video_input.clone(),
        status_label,
        auto_connect: auto_connect.clone(),
    }));

    channel_input.set_callback({
        let cfg = cfg.clone();
        let channel_input = channel_input.clone();
        move |_| {
            let val = channel_input.value().trim().to_string();
            save(&cfg, |c| c.youtube.channel_id = val);
        }
    });

    video_input.set_callback({
        let cfg = cfg.clone();
        let video_input = video_input.clone();
        move |_| {
            let val = video_input.value().trim().to_string();
            save(&cfg, |c| c.youtube.video_id = val);
        }
    });

    auto_connect.set_callback({
        let cfg = cfg.clone();
        let auto_connect = auto_connect.clone();
        move |_| {
            let on = auto_connect.is_checked();
            save(&cfg, |c| c.youtube.auto_connect = on);
        }
    });

    connect_btn.set_callback({
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            tell(&cmds, |e| {
                e.youtube_start().send(crate::youtube::YouTubeCmd::Connect);
            });
            me.borrow_mut().refresh();
        }
    });

    disconnect_btn.set_callback({
        let cmds = cmds.clone();
        let me = me.clone();
        move |_| {
            tell(&cmds, |e| {
                if let Some(y) = e.youtube() {
                    y.send(crate::youtube::YouTubeCmd::Disconnect);
                }
            });
            me.borrow_mut().refresh();
        }
    });

    me.borrow_mut().refresh();
    me
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new one already has every field its form sets, so none lands out of Streamer.bot's order.
    #[test]
    fn new_objects_have_their_form_fields() {
        for k in [&COMMANDS, &TIMERS, &CLIENTS, &SERVERS, &OBS_CONNECTIONS] {
            let new = (k.new)();
            for f in k.fields {
                assert!(new.get(f.0).is_some(), "{}: {} missing", k.what, f.0);
            }
        }
    }
}
