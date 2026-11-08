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
use fltk::group::{Flex, FlexType, Wizard};
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
    detail: Frame,
    /// Where the agents connect: what the inputs below say, in full.
    about: Frame,
    inputs: [Input; 3],
    buttons: Vec<Button>,
    shown: Vec<Found>,
    connection_error: bool,
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
    let key = if super::img::has_logo(name) { name } else { "bot" };
    format!("{key}|{name}")
}

/// The connection fields share one row when each has room for its label and value.
fn connection_layout(content_width: i32) -> (FlexType, i32) {
    if content_width < 720 {
        (FlexType::Column, 3 * ROW + 16)
    } else {
        (FlexType::Row, ROW)
    }
}

/// Selection actions depend only on the selected item; copying setup and refreshing do not.
fn available(selected: Option<State>, cli: bool, file: bool, folder: bool) -> [bool; 7] {
    let present = selected.is_some();
    [
        selected.is_some_and(|state| state != State::Added),
        selected.is_some_and(|state| state != State::Ready),
        present && cli,
        present && file,
        present && folder,
        true,
        true,
    ]
}

/// Setup states share one colour and explanation in the table and selected-item detail.
fn state_details(state: State) -> (Health, &'static str, &'static str) {
    match state {
        State::Added => (Health::Up, "circle-check", tr("agents.tip.added")),
        State::Ready => (Health::Off, "circle-dashed", tr("agents.tip.ready")),
        State::Outdated => (Health::Busy, "triangle-alert", tr("agents.outdated")),
    }
}

/// A column's cell for an agent's state: its icon, colour and explanation.
fn status(f: &Found) -> String {
    let (health, icon, explanation) = state_details(f.state);
    let color = health.color().to_rgb();
    let mut tip = format!("{}: {explanation}", state(f.state));
    if f.cli.is_none() {
        tip += " ";
        tip += tr("agents.no_cli");
    }
    format!("{icon}|{:02x}{:02x}{:02x}|{tip}", color.0, color.1, color.2)
}

/// The same address and route normalization as the WebSocket connection form.
fn connection_values(address: &str, port: &str, route: &str) -> Option<(String, u16, String)> {
    let port = port.trim().parse::<u16>().ok().filter(|port| *port > 0)?;
    let address = match address.trim() {
        "" => "127.0.0.1",
        address => address,
    };
    let route = match route.trim() {
        "" => "/".to_string(),
        route if route.starts_with('/') => route.to_string(),
        route => format!("/{route}"),
    };
    Some((address.to_string(), port, route))
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
        if self.connection_error {
            self.connection_warning();
            return;
        }
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
        self.about.set_image(None::<fltk::image::SvgImage>);
        self.about.set_label_color(super::muted());
    }

    fn connection_warning(&mut self) {
        let text = tr("lists.servers.port_range_warning");
        Health::Down.show(&mut self.about, text);
        self.about.set_tooltip(text);
    }

    /// Applies what was typed in the connection's inputs (the WebSocket server's own settings).
    fn apply(&mut self, explicit: bool) {
        let [address, port, route] = &self.inputs;
        let Some((address, port, route)) = connection_values(&address.value(), &port.value(), &route.value()) else {
            self.connection_error = true;
            self.connection_warning();
            if explicit {
                super::dialogs::alert(tr("lists.servers.port_range_warning"));
                let _ = self.inputs[1].take_focus();
            }
            return;
        };
        self.connection_error = false;
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
        let on = available(added, cli, file, folder);
        let (health, detail) = match sel {
            Some(found) => {
                let (health, _, explanation) = state_details(found.state);
                (health, format!("{}: {explanation}", state(found.state)))
            }
            None => (Health::Off, tr("agents.selection_hint").to_string()),
        };
        health.show(&mut self.detail, &detail);
        self.detail
            .set_align(Align::Left | Align::Inside | Align::Wrap | Align::ImageNextToText);
        self.detail.set_tooltip(&detail.replace('@', "@@"));
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
                super::dialogs::error(&trf("agents.cant_write", &[("name", kind.name), ("error", &e)]));
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
            Err(e) => super::dialogs::error(&trf("agents.cant_open", &[("name", name), ("error", &e)])),
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
    let detail = super::hint(&mut col, tr("agents.selection_hint"), 60);

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
    let field = |label: &str| {
        let mut row = super::dialogs::row(label);
        if let Some(label) = row.child(0) {
            row.fixed(&label, 64);
        }
        let input = Input::default();
        row.end();
        input
    };
    let mut address = field(tr("agents.cfg.address"));
    let mut port = field(tr("agents.cfg.port"));
    let mut route = field(tr("agents.cfg.route"));
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
    let mut place = move |col: &mut Flex, width: i32| {
        // The page's two 10px margins are outside the connection fields.
        let (direction, height) = connection_layout(width.saturating_sub(20));
        line.set_type(direction);
        col.fixed(&line, height);
        col.redraw();
    };
    let width = col.w();
    place(&mut col, width);
    col.resize_callback(move |col, _, _, width, _| place(col, width));
    let me = Rc::new(RefCell::new(Agents {
        tab: col,
        cfg: cfg.clone(),
        view: view.clone(),
        detail,
        about,
        inputs: [address.clone(), port.clone(), route.clone()],
        buttons: b.clone(),
        shown: Vec::new(),
        connection_error: false,
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
                m.apply(app::event() == fltk::enums::Event::KeyDown && app::event_key() == fltk::enums::Key::Enter);
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
    me.borrow_mut().describe();
    me
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_fields_stack_before_labels_and_values_become_cramped() {
        assert_eq!(connection_layout(344), (FlexType::Column, 94));
        assert_eq!(connection_layout(719), (FlexType::Column, 94));
        assert_eq!(connection_layout(720), (FlexType::Row, ROW));
        assert_eq!(connection_layout(904), (FlexType::Row, ROW));
        assert_eq!(connection_layout(0), (FlexType::Column, 94));
    }

    #[test]
    fn selection_actions_match_setup_state_and_available_targets() {
        assert_eq!(
            available(None, true, true, true),
            [false, false, false, false, false, true, true]
        );
        assert_eq!(
            available(Some(State::Ready), false, false, false),
            [true, false, false, false, false, true, true]
        );
        assert_eq!(
            available(Some(State::Added), true, true, true),
            [false, true, true, true, true, true, true]
        );
        assert_eq!(
            available(Some(State::Outdated), true, false, true),
            [true, true, true, false, true, true, true]
        );
    }

    #[test]
    fn connection_validation_retains_existing_normalization_and_rejects_invalid_ports() {
        assert_eq!(
            connection_values("  ", " 8090 ", " path "),
            Some(("127.0.0.1".into(), 8090, "/path".into()))
        );
        assert_eq!(
            connection_values(" localhost ", "65535", ""),
            Some(("localhost".into(), 65535, "/".into()))
        );
        assert_eq!(
            connection_values("0.0.0.0", "1", "/mcp"),
            Some(("0.0.0.0".into(), 1, "/mcp".into()))
        );
        for port in ["", "0", "65536", "-1", "no"] {
            assert_eq!(connection_values("localhost", port, "/"), None);
        }
    }

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
