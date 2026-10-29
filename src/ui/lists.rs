//! The Commands, Timers, Queues, History, Globals and Servers tabs. Commands, timers and WebSocket
//! clients/servers are lists of Streamer.bot objects, each edited in a form.

use super::fit::Ellipsis;
use super::forms::{self, Done, Lists, F, W::*};
use super::rows::Rows;
use super::{button, dialogs, query, tell, ROW};
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
use std::cell::RefCell;
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

/// Change the config and save it.
pub(super) fn save(cfg: &Cfg, f: impl FnOnce(&mut Config)) {
    cfg.send_modify(|c| {
        f(c);
        // A login (or a channel emptied in Advanced) reads the broadcaster's own channel.
        c.own_channels();
    });
    let _ = crate::save(&cfg.borrow());
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
    empty: &'static str,
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
    columns: &[180, 280, 120, 260],
    titles: &[
        "lists.commands.name",
        "lists.commands.commands",
        "lists.commands.group",
        "lists.commands.sources",
    ],
    empty: "lists.commands.empty",
    label: |o| {
        let aliases = crate::commands::aliases(o).join(", ");
        vec![
            s(o, "name").into(),
            aliases,
            s(o, "group").into(),
            if on(o, "enabled") {
                crate::engine::builtins::sources_text(o.get("sources").and_then(Value::as_u64).unwrap_or(1))
            } else {
                tr("lists.disabled").into()
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
    columns: &[220, 180, 120, 120, 80],
    titles: &[
        "lists.timers.name",
        "lists.timers.every",
        "lists.timers.lines",
        "lists.timers.runs",
        "",
    ],
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
            s(o, "name").into(),
            every,
            if n("lines") > 0 {
                trf("lists.timers.lines_count", &[("count", &n("lines").to_string())])
            } else {
                String::new()
            },
            trf("lists.timers.ran_count", &[("count", &n("counter").to_string())]),
            yes(!on(o, "enabled"), tr("lists.disabled")),
        ]
    },
    after: Some(|e| e.timers()),
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
    columns: &[200, 400, 120],
    titles: &["lists.clients.name", "lists.clients.endpoint", ""],
    empty: "lists.clients.empty",
    label: |o| {
        vec![
            s(o, "name").into(),
            s(o, "endpoint").into(),
            yes(!on(o, "autoConnect"), tr("lists.manual")),
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
    titles: &["lists.obs.name", "lists.obs.address", "", ""],
    empty: "lists.obs.empty",
    label: |o| {
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(4455);
        vec![
            s(o, "name").into(),
            format!("{}:{}", s(o, "host"), port),
            yes(!on(o, "autoConnect"), tr("lists.manual")),
            yes(!on(o, "reconnect"), tr("lists.no_retry")),
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
    titles: &["lists.slobs.name", "lists.slobs.address", "", ""],
    empty: "lists.slobs.empty",
    label: |o| {
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(59650);
        vec![
            s(o, "name").into(),
            format!("{}:{port}", s(o, "host")),
            yes(!on(o, "autoConnect"), tr("lists.manual")),
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
    columns: &[200, 400, 120],
    titles: &["lists.custom_servers.name", "lists.custom_servers.address", ""],
    empty: "lists.custom_servers.empty",
    label: |o| {
        let port = o.get("port").and_then(Value::as_u64).unwrap_or(0);
        vec![
            s(o, "name").into(),
            format!("ws://{}:{port}{}", s(o, "address"), s(o, "endpoint")),
            yes(!on(o, "autoStart"), tr("lists.off_badge")),
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
        if super::dialogs::choice(&q.replace('@', "@@"), tr("dialogs.cancel"), tr("btn.delete")) {
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
    let view = browser(col, kind.columns, kind.titles, kind.empty);
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
    view.clone().set_callback({
        let me = me.clone();
        move |_| {
            if fltk::app::event_clicks() {
                if let Ok(mut m) = me.try_borrow_mut() {
                    m.edit();
                }
            }
        }
    });
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
}

impl Views {
    /// New chat lines: added to the Chat tab when it's up (it catches up when shown).
    pub fn chat_refresh(&self, tabs: &Wizard) {
        let up = tabs.try_current_widget().map(|g| g.as_widget_ptr());
        if let Ok(mut c) = self.chat.try_borrow_mut() {
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
            if is(&a.tab) {
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
    }
}

pub fn tabs(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Views {
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
    let community = community_tab(tabs, cfg, cmds);
    let groups = groups_tab(tabs, cfg, cmds);
    let plugins = super::plugins::tab(tabs, cfg, cmds);

    let mut col = tab(tabs, "Servers");
    let servers = servers_box(&mut col, cfg);
    title(&mut col, tr("lists.servers.ws_clients_title"));
    let clients = list(&mut col, cfg, cmds, &CLIENTS, None);
    title(&mut col, tr("lists.servers.ws_servers_title"));
    let custom = list(&mut col, cfg, cmds, &SERVERS, Some("autoStart"));
    col.end();

    let mut col = tab(tabs, "Stream Apps");
    title(&mut col, "OBS Studio");
    let obs_guide = Frame::default()
        .with_label(tr("lists.stream_apps.obs_guide"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&obs_guide, 36);
    let obs = list(&mut col, cfg, cmds, &OBS_CONNECTIONS, Some("autoConnect"));
    let mut b_row = Flex::default().row();
    b_row.set_pad(8);
    let mut b_connect = button(&mut b_row, tr("btn.connect_all"), 120);
    let mut b_disconnect = button(&mut b_row, tr("btn.disconnect_all"), 120);
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
        .with_label(tr("lists.stream_apps.slobs_guide"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&slobs_guide, 36);
    let slobs = list(&mut col, cfg, cmds, &SLOBS_CONNECTIONS, Some("autoConnect"));
    let mut b_row = Flex::default().row();
    b_row.set_pad(8);
    let mut b_connect = button(&mut b_row, tr("btn.connect_all"), 120);
    let mut b_disconnect = button(&mut b_row, tr("btn.disconnect_all"), 120);
    Frame::default();
    b_row.end();
    col.fixed(&b_row, ROW);
    let c = cmds.clone();
    b_connect.set_callback(move |_| tell(&c, |e| e.slobs_connect("")));
    let c = cmds.clone();
    b_disconnect.set_callback(move |_| tell(&c, |e| e.slobs_disconnect("")));
    col.end();
    let donations = donations_tab(tabs, cfg, cmds);
    let crashes = super::recovery::crashes_tab(tabs);
    let polls = super::polls::tab(tabs, cmds);
    let backups = super::recovery::backups_tab(tabs, cfg, cmds);

    Views {
        lists: vec![commands, timers, clients, custom, obs, slobs],
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
        &["lists.queues.queue", "lists.queues.action", ""],
        "lists.queues.pending_empty",
    );
    col.fixed(&*pending, 200);
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
            let q = tr("lists.queues.delete_confirm");
            if super::dialogs::choice(q, tr("dialogs.cancel"), tr("btn.delete")) {
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
}

impl History {
    pub fn refresh(&mut self) {
        let Some(runs) = query(&self.cmds, |e| e.history().iter().rev().cloned().collect::<Vec<_>>()) else {
            return;
        };
        let sel = self.view.value();
        self.view.clear();
        for r in &runs {
            let at = crate::fmt::shown_iso(&r.queued_at, true);
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

fn history_tab(tabs: &Wizard, cmds: &Cmds) -> Rc<RefCell<History>> {
    let mut col = tab(tabs, "History");
    let view = browser(
        &mut col,
        &[180, 420, 100, 120],
        &[
            "lists.history.when",
            "lists.history.action",
            "lists.history.took",
            "lists.history.variables",
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
                let v = if after { &r.after } else { &r.args };
                dialogs::values(&r.name, "", v, false);
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
                if *persisted {
                    tr("lists.globals.persisted")
                } else {
                    tr("lists.globals.temporary")
                }
                .into(),
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
        let Some(o) = ask(tr("lists.globals.global_title"), fields, o, &Lists::default()) else {
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

fn globals_tab(tabs: &Wizard, cmds: &Cmds) -> Rc<RefCell<Globals>> {
    let mut col = tab(tabs, "Globals");
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
            if super::dialogs::choice(&q, "Cancel", "Delete") {
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

pub struct Community {
    tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    show: fltk::menu::Choice,
    search: fltk::input::Input,
    views: [Flex; 3],
    users: Rows,
    /// The users in the list, as shown (the list's line n is `shown[n - 1]`).
    shown: Vec<Obj>,
    quotes: Rows,
    credits: Rows,
    /// The engine's `community_rev` when the users and credits were last read.
    rev: u64,
}

impl Community {
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
                let Some(mut users) = query(&self.cmds, |e| e.users()) else {
                    return;
                };
                users.sort_by(|a, b| s(b, "lastActive").cmp(s(a, "lastActive")));
                let q = self.search.value().to_lowercase();
                self.users.clear();
                self.shown = users
                    .iter()
                    .filter(|u| q.is_empty() || s(u, "display").to_lowercase().contains(&q))
                    .cloned()
                    .collect();
                USERS_SHOWN.store(self.shown.len(), std::sync::atomic::Ordering::Relaxed);
                for u in &self.shown {
                    let role = [
                        "",
                        tr("lists.community.role_viewer"),
                        tr("lists.community.role_vip"),
                        tr("lists.community.role_moderator"),
                        tr("lists.community.role_broadcaster"),
                    ];
                    self.users.add(&line(&[
                        s(u, "type").into(),
                        s(u, "display").into(),
                        role.get(u["role"].as_u64().unwrap_or(1) as usize)
                            .unwrap_or(&"")
                            .to_string(),
                        day(s(u, "firstMessage")),
                        day(s(u, "lastActive")),
                    ]));
                }
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
    }

    /// The card of the user selected: who they are, what they said, their time here.
    fn user_card(&self) {
        let n = self.users.value();
        if let Some(u) = self.shown.get((n - 1).max(0) as usize).filter(|_| n > 0) {
            super::card::open(
                &self.cmds,
                super::card::Who::from_member(u),
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
    top.fixed(&search, 220);
    Frame::default();
    top.end();
    col.fixed(&top, ROW);

    let users_v = Flex::default().column();
    let mut users_v = users_v;
    let mut users = browser(
        &mut users_v,
        &[36, 220, 110, 110, 110],
        &[
            "",
            "lists.community.name",
            "lists.community.role",
            "lists.community.first_seen",
            "lists.community.last_seen",
        ],
        "lists.community.users_empty",
    );
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
        shown: Vec::new(),
        quotes: quotes.clone(),
        credits,
        rev: 0,
    }));
    // Click a user for their card.
    users_click(&me);
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
            if fltk::app::event_clicks() {
                if let Ok(mut m) = me.try_borrow_mut() {
                    let id = m.quote_id();
                    m.edit_quote(id);
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
        let state = |on: bool| {
            if on {
                tr("lists.servers.on")
            } else {
                tr("lists.servers.off")
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
                0 => (tr("lists.servers.websocket_server"), ws, to_obj(&c.websockets)),
                1 => (tr("lists.servers.http_server"), http, to_obj(&c.http)),
                _ => (tr("lists.servers.udp_server"), udp, to_obj(&c.udp)),
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
                _ => serde_json::from_value(v).map(|x| c.udp = x),
            };
            bad = r.err();
        });
        if let Some(e) = bad {
            super::dialogs::alert(&e.to_string());
        }
        self.refresh();
    }
}

fn servers_box(col: &mut Flex, cfg: &Cfg) -> Rc<RefCell<Servers>> {
    title(col, tr("lists.servers.title"));
    let me = Rc::new(RefCell::new(Servers {
        cfg: cfg.clone(),
        lines: Vec::new(),
    }));
    for i in 0..3 {
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
        let (se, sl) = query(&self.cmds, |e| {
            let d = e.donations.borrow();
            (d.streamelements.get(), d.streamlabs.get())
        })
        .unwrap_or_default();
        let c = self.cfg.borrow();
        let d = &c.donations;
        let state = |token: &str, up: bool| match (token.trim().is_empty(), up) {
            (true, _) => tr("lists.donations.off_no_token"),
            (_, true) => tr("lists.donations.connected"),
            _ => tr("lists.donations.not_connected"),
        };
        let texts = [
            trf(
                "lists.donations.status",
                &[("status", state(&d.streamelements_token, se))],
            ),
            trf("lists.donations.status", &[("status", state(&d.streamlabs_token, sl))]),
            match (d.donordrive_instance.trim(), d.donordrive_participant_id.trim()) {
                ("", _) | (_, "") => tr("lists.donations.status_off").to_string(),
                (site, id) => trf("lists.donations.donordrive_status", &[("id", id), ("site", site)]),
            },
        ];
        for (f, t) in self.lines.iter_mut().zip(texts) {
            f.set_label(&t.replace('@', "@@"));
        }
    }

    fn edit(&mut self, which: usize) {
        let fields: &[F] = match which {
            0 => &[("streamelementsToken", "JWT token", Text)],
            1 => &[("streamlabsToken", "Socket API token", Text)],
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
    let (scroll, mut col) = super::tab_scrolled(tabs, "Donations", 400);
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
        let g = Frame::default()
            .with_label(guide)
            .with_align(Align::Left | Align::Inside | Align::Wrap);
        col.fixed(&g, ROW);
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
}

impl Groups {
    pub fn refresh(&mut self) {
        let sel = self.view.value();
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
        if sel > 0 {
            self.view.select(sel);
        }
    }
}

fn groups_tab(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Groups>> {
    let mut col = tab(tabs, "Groups");
    let view = browser(
        &mut col,
        &[300, 120],
        &["lists.groups.group", "lists.groups.users"],
        "lists.groups.empty",
    );
    let b = bar(&mut col, &[tr("btn.add"), tr("btn.delete")]);
    view.clone().set_menu(&b);
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
    b[1].clone().set_callback({
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
