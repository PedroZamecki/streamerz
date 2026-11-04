//! The MCP page: the AI agents installed here, adding Streamerz's MCP server to each one's config, and
//! a chat with a CLI agent in a terminal (already set up with Streamerz). Read only when the page shows.

use super::fit::Ellipsis;
use super::rows::{Cell, Rows};
use super::{icons, Health, ROW};
use crate::agents::{self, Found, Places, Server, State};
use crate::i18n::{tr, trf};
use fltk::app;
use fltk::button::Button;
use fltk::enums::{Align, CallbackTrigger, Font};
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
use fltk::input::Input;
use fltk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

type Cfg = Arc<tokio::sync::watch::Sender<crate::Config>>;

pub struct Agents {
    pub tab: Flex,
    cfg: Cfg,
    view: Rows,
    /// Where the agents connect: what the inputs below say, in full.
    about: Frame,
    inputs: [Input; 3],
    buttons: Vec<Button>,
    shown: Vec<Found>,
}

/// Where an agent's MCP server reaches this Streamerz (what `Streamerz --mcp` reads from the config).
fn url(c: &crate::Config) -> String {
    let w = &c.websockets;
    let host = match w.address.as_str() {
        "" | "0.0.0.0" | "*" => "127.0.0.1",
        h => h,
    };
    format!("ws://{host}:{}{}", w.port, w.endpoint)
}

/// An agent's logo, or an icon when it has none.
fn lead(name: &str) -> String {
    let key = if super::img::has_logo(name) {
        name
    } else {
        match name {
            "Antigravity" => "rocket",
            "Kiro" => "ghost",
            "Crush" => "terminal",
            _ => "bot",
        }
    };
    format!("{key}|{name}")
}

/// A column's cell for an agent's state: its icon and colour (the same as a connection's), and what it means.
fn status(f: &Found) -> String {
    let (h, icon, tip) = match f.state {
        State::Added => (Health::Up, "circle-check", tr("agents.tip.added")),
        State::Ready => (Health::Off, "circle-dashed", tr("agents.tip.ready")),
        State::Outdated => (Health::Busy, "triangle-alert", tr("agents.outdated")),
    };
    let c = h.color().to_rgb();
    let mut tip = format!("{}: {tip}", state(f.state));
    if f.cli.is_none() {
        tip += " ";
        tip += tr("agents.no_cli");
    }
    format!("{icon}|{:02x}{:02x}{:02x}|{tip}", c.0, c.1, c.2)
}

impl Agents {
    fn server(&self) -> Server {
        let c = self.cfg.borrow();
        Server::this(url(&c))
    }

    /// Looks for the agents again (a few file checks), and the connection's inputs.
    pub fn refresh(&mut self) {
        self.shown = agents::found(&Places::here(), &self.server());
        let keep = self.view.value();
        self.view.clear();
        for f in &self.shown {
            let kind = if f.cli.is_some() {
                format!("terminal|{}", tr("agents.kind.cli"))
            } else {
                format!("app-window|{}", tr("agents.kind.app"))
            };
            self.view.add(&format!(
                "{}\t{kind}\t{}\t{}",
                lead(f.kind.name),
                status(f),
                f.file.display()
            ));
        }
        self.view.select(keep.clamp(1, self.view.size().max(1)));
        self.connection();
        self.describe();
    }

    /// The connection's inputs and the line under them, from the config.
    fn connection(&mut self) {
        let (w, url) = {
            let c = self.cfg.borrow();
            (c.websockets.clone(), url(&c))
        };
        let [address, port, route] = &mut self.inputs;
        for (i, v) in [(address, w.address), (port, w.port.to_string()), (route, w.endpoint)] {
            if !i.has_focus() && i.value() != v {
                i.set_value(&v);
            }
        }
        let text = trf(
            "agents.cfg.about",
            &[("url", &url), ("command", &self.server().command)],
        );
        self.about.set_tooltip(&text.replace('@', "@@"));
        self.about.set_label(&text.replace('@', "@@"));
    }

    /// Applies what was typed in the connection's inputs (the WebSocket server's own settings).
    fn apply(&mut self) {
        let [address, port, route] = &self.inputs;
        let (address, port, route) = (address.value().trim().to_string(), port.value(), route.value());
        let port = match port.trim().parse::<u16>() {
            Ok(p) if p > 0 => p,
            _ => {
                super::dialogs::alert(tr("lists.servers.port_range_warning"));
                self.connection();
                return;
            }
        };
        let route = match route.trim() {
            "" => "/".to_string(),
            r if r.starts_with('/') => r.to_string(),
            r => format!("/{r}"),
        };
        let address = if address.is_empty() {
            "127.0.0.1".to_string()
        } else {
            address
        };
        let same = {
            let w = &self.cfg.borrow().websockets;
            (w.address.as_str(), w.port, w.endpoint.as_str()) == (address.as_str(), port, route.as_str())
        };
        if !same {
            self.cfg.send_modify(|c| {
                c.websockets.address = address;
                c.websockets.port = port;
                c.websockets.endpoint = route;
            });
            crate::save_later(&self.cfg.borrow());
        }
        self.refresh();
    }

    fn selected(&self) -> Option<&Found> {
        self.shown.get(usize::try_from(self.view.value() - 1).ok()?)
    }

    /// What can be done with the selected agent.
    fn describe(&mut self) {
        let sel = self.selected();
        let (added, cli) = (sel.map(|f| f.state), sel.is_some_and(|f| f.cli.is_some()));
        let file = sel.is_some_and(|f| f.file.is_file());
        let folder = sel.is_some_and(|f| f.file.parent().is_some_and(|d| d.is_dir()));
        let on = [
            added.is_some_and(|s| s != State::Added),
            added.is_some_and(|s| s != State::Ready),
            cli,
            file,
            folder,
        ];
        for (b, on) in self.buttons.iter_mut().zip(on) {
            if on {
                b.activate();
            } else {
                b.deactivate();
            }
        }
    }

    /// Opens the selected agent's config file, or the folder it is in.
    fn open(&self, folder: bool) {
        let Some(f) = self.selected() else {
            return;
        };
        let target = if folder {
            f.file.parent().unwrap_or(&f.file)
        } else {
            &f.file
        };
        crate::oauth::open_browser(&target.display().to_string());
    }

    /// Adds Streamerz to the selected agent (or takes it out): false and a message when it can't.
    fn set(&mut self, on: bool) -> bool {
        let Some(f) = self.selected() else {
            return false;
        };
        let kind = f.kind;
        let r = agents::set(kind, &Places::here(), &self.server(), on);
        self.refresh();
        match r {
            Ok(()) => true,
            Err(e) => {
                super::dialogs::alert(&trf("agents.cant_write", &[("name", kind.name), ("error", &e)]));
                false
            }
        }
    }

    /// Opens the selected CLI agent in a terminal, with Streamerz added first.
    fn chat(&mut self) {
        let Some(f) = self.selected() else {
            return;
        };
        let Some(cli) = f.cli.clone() else {
            return;
        };
        let name = f.kind.name;
        if f.state != State::Added && !self.set(true) {
            return;
        }
        let r = agents::workspace(&self.server()).and_then(|dir| agents::chat(&cli, &dir));
        match r {
            Ok(()) => crate::say(format!("Opened {name} in a terminal")),
            Err(e) => super::dialogs::alert(&trf("agents.cant_open", &[("name", name), ("error", &e)])),
        }
    }
}

fn state(s: State) -> &'static str {
    match s {
        State::Ready => tr("agents.state.ready"),
        State::Added => tr("agents.state.added"),
        State::Outdated => tr("agents.state.outdated"),
    }
}

pub fn tab(tabs: &Wizard, cfg: &Cfg) -> Rc<RefCell<Agents>> {
    let mut col = super::tab(tabs, "MCP");
    super::hint(&mut col, tr("agents.intro"), 40);
    super::header(
        &mut col,
        &[230, 110, 80],
        &[
            tr("agents.header.agent"),
            tr("agents.header.kind"),
            tr("agents.header.state"),
            tr("agents.header.config"),
        ],
    );
    let mut view = Rows::new(tr("agents.empty"));
    view.set_column_widths(&[230, 110, 80]);
    view.set_cell(0, Cell::Lead);
    view.set_cell(1, Cell::Lead);
    view.set_cell(2, Cell::Status);
    view.set_cell(3, Cell::Link);

    let mut title = Frame::default()
        .with_label(tr("agents.cfg.title"))
        .with_align(Align::Left | Align::Inside | Align::ImageNextToText)
        .ellipsis();
    title.set_label_font(Font::HelveticaBold);
    title.set_label_size(15);
    title.set_image(icons::before_label("plug", fltk::enums::Color::Foreground, 16));
    col.fixed(&title, 30);
    let mut line = Flex::default().row();
    line.set_pad(8);
    let mut field = |label: &str, w: i32| {
        let l = Frame::default()
            .with_label(label)
            .with_align(Align::Left | Align::Inside)
            .ellipsis();
        line.fixed(&l, 56);
        let input = Input::default();
        line.fixed(&input, w);
        input
    };
    let mut address = field(tr("agents.cfg.address"), 170);
    let mut port = field(tr("agents.cfg.port"), 90);
    let mut route = field(tr("agents.cfg.route"), 170);
    Frame::default();
    line.end();
    col.fixed(&line, ROW);
    for (i, tip) in [
        (&mut address, "agents.cfg.address_tip"),
        (&mut port, "agents.cfg.port_tip"),
        (&mut route, "agents.cfg.route_tip"),
    ] {
        i.set_trigger(CallbackTrigger::EnterKey | CallbackTrigger::Release);
        i.set_tooltip(tr(tip));
    }
    let mut about = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
    about.set_label_color(super::muted());
    about.set_label_size(12);
    col.fixed(&about, 22);

    let b = super::grid(
        &mut col,
        &[
            tr("btn.add_streamerz"),
            tr("btn.remove_streamerz"),
            tr("btn.open_chat"),
            tr("btn.open_file"),
            tr("btn.open_folder"),
            tr("btn.copy_config"),
            tr("btn.refresh"),
        ],
        1,
    );
    view.set_menu(&b);
    col.end();
    let me = Rc::new(RefCell::new(Agents {
        tab: col,
        cfg: cfg.clone(),
        view: view.clone(),
        about,
        inputs: [address.clone(), port.clone(), route.clone()],
        buttons: b.clone(),
        shown: Vec::new(),
    }));
    view.set_callback({
        let me = me.clone();
        let (file, folder) = (b[3].clone(), b[4].clone());
        move |v| {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.describe();
            }
            // A click on the config path: open it, or the folder it is in.
            if v.column() == Some(3) && !super::right_button() {
                super::context_menu(&[file.clone(), folder.clone()]);
            }
        }
    });
    for mut i in [address, port, route] {
        let me = me.clone();
        i.set_callback(move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.apply();
            }
        });
    }
    let click = |b: &Button, f: fn(&mut Agents)| {
        let me = me.clone();
        b.clone().set_callback(move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                f(&mut m);
            }
        });
    };
    click(&b[0], |m| {
        m.set(true);
    });
    click(&b[1], |m| {
        m.set(false);
    });
    click(&b[2], Agents::chat);
    click(&b[3], |m| m.open(false));
    click(&b[4], |m| m.open(true));
    click(&b[5], |m| {
        app::copy(&m.server().snippet());
        super::dialogs::message(tr("agents.copied"));
    });
    click(&b[6], Agents::refresh);
    me
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_has_a_logo_or_an_icon() {
        for k in agents::KINDS {
            let (key, name) = lead(k.name)
                .split_once('|')
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .unwrap();
            assert_eq!(name, k.name);
            assert!(
                super::super::img::has_logo(&key) || icons::icon(&key, fltk::enums::Color::White, 16).is_some(),
                "{}",
                k.name
            );
        }
    }

    #[test]
    fn the_url_reaches_loopback_for_a_wildcard_address() {
        let mut c = crate::Config::default();
        c.websockets.address = "0.0.0.0".into();
        assert_eq!(url(&c), "ws://127.0.0.1:8080/");
    }
}
