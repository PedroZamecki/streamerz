//! The Plugins page: the plugins in the plugins folder, on or off, and their settings.

use super::forms::{self, Done, Lists};
use super::rows::Rows;
use super::{query, tell};
use crate::engine::plugins::{self, Manifest};
use crate::i18n::{tr, trf};
use fltk::enums::Align;
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
use fltk::prelude::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::Arc;

type Cfg = Arc<tokio::sync::watch::Sender<crate::Config>>;
type Cmds = tokio::sync::mpsc::UnboundedSender<crate::engine::UiCmd>;

pub struct Plugins {
    pub tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    view: Rows,
    about: Frame,
    /// What the list shows: id, manifest (None when it doesn't load), on.
    shown: Vec<(String, Option<Manifest>, bool, String)>,
}

impl Plugins {
    /// Reads the folder again. Plugins that are off are loaded only to read their manifest.
    pub fn refresh(&mut self) {
        let on = query(&self.cmds, |e| e.plugins_on()).unwrap_or_default();
        let enabled = |id: &str| self.cfg.borrow().plugins.get(id).is_some_and(|p| p.enabled);
        self.shown = plugins::installed()
            .into_iter()
            .map(|(id, path)| match on.iter().find(|p| p.0 == id) {
                Some((_, m)) => (id, Some(m.clone()), true, String::new()),
                None => match plugins::load(&id, &path, Weak::new()) {
                    Ok(p) => {
                        let on = enabled(&id);
                        (id, Some(p.manifest.clone()), on, String::new())
                    }
                    Err(e) => (id, None, false, e),
                },
            })
            .collect();
        let keep = self.view.value();
        self.view.clear();
        for (id, m, on, err) in &self.shown {
            let (name, version) = m
                .as_ref()
                .map_or((id.clone(), String::new()), |m| (m.name.clone(), m.version.clone()));
            let state = match (on, err.is_empty()) {
                (_, false) => tr("plugins.state.cant_load"),
                (true, _) => tr("plugins.state.on"),
                _ => tr("plugins.state.off"),
            };
            self.view.add(&format!("{name}\t{version}\t{state}"));
        }
        self.view.select(keep);
        self.describe();
    }

    fn selected(&self) -> Option<&(String, Option<Manifest>, bool, String)> {
        self.shown.get(usize::try_from(self.view.value() - 1).ok()?)
    }

    /// The selected plugin's description, and what it adds.
    fn describe(&mut self) {
        let text = match self.selected() {
            Some((_, _, _, err)) if !err.is_empty() => err.clone(),
            Some((_, Some(m), _, _)) => {
                let mut t = m.about.clone();
                let subs: Vec<&str> = m.subactions.iter().map(|x| x.name.as_str()).collect();
                let trig: Vec<&str> = m.triggers.iter().map(|x| x.name.as_str()).collect();
                if !subs.is_empty() {
                    t += &trf("plugins.subactions", &[("list", &subs.join(", "))]);
                }
                if !trig.is_empty() {
                    t += &trf("plugins.triggers", &[("list", &trig.join(", "))]);
                }
                t
            }
            _ => String::new(),
        };
        self.about.set_label(&text.replace('@', "@@"));
    }
}

pub fn tab(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Plugins>> {
    let mut col = super::tab(tabs, "Plugins");
    super::header(
        &mut col,
        &[300, 120],
        &[
            tr("plugins.header.plugin"),
            tr("plugins.header.version"),
            tr("plugins.header.state"),
        ],
    );
    let mut view = Rows::new(tr("plugins.empty"));
    view.set_column_widths(&[300, 120]);
    let mut about = Frame::default().with_align(Align::Left | Align::Inside | Align::Wrap | Align::Top);
    about.set_label_color(super::muted());
    col.fixed(&about, 64);
    let b = super::grid(
        &mut col,
        &[
            tr("btn.on_off"),
            tr("btn.settings"),
            tr("btn.open_folder"),
            tr("btn.refresh"),
        ],
        1,
    );
    view.set_menu(&b);
    col.end();
    let me = Rc::new(RefCell::new(Plugins {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        view: view.clone(),
        about,
        shown: Vec::new(),
    }));
    view.set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.describe();
            }
        }
    });
    let click = |b: &fltk::button::Button, f: Box<dyn Fn(&mut Plugins)>| {
        let me = me.clone();
        b.clone().set_callback(move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                f(&mut m);
            }
        });
    };
    click(
        &b[0],
        Box::new(|m| {
            let Some((id, Some(_), on, _)) = m.selected().cloned() else {
                return;
            };
            tell(&m.cmds, move |e| e.plugin_enable(&id, !on));
            // Loaded on the engine's thread: shown after it.
            let _ = query(&m.cmds, |_| ());
            m.refresh();
        }),
    );
    click(
        &b[1],
        Box::new(|m| {
            let Some((id, Some(man), _, _)) = m.selected().cloned() else {
                return;
            };
            if man.settings.is_empty() {
                return fltk::dialog::message_default(&trf("plugins.no_settings", &[("name", &man.name)]));
            }
            let fields = forms::plugin_fields(&format!("{id}/settings"), &man.settings);
            let mut now = m
                .cfg
                .borrow()
                .plugins
                .get(&id)
                .map(|p| p.settings.clone())
                .unwrap_or_default();
            for f in &man.settings {
                if !now.contains_key(&f.key) && !f.default.is_null() {
                    now.insert(f.key.clone(), f.default.clone());
                }
            }
            let lists = Lists::new(&m.cfg.borrow());
            if let Done::Ok(new) = forms::form(
                &trf("plugins.settings_title", &[("name", &man.name)]),
                fields,
                &now,
                &lists,
            ) {
                tell(&m.cmds, move |e| e.plugin_set(&id, new));
            }
        }),
    );
    click(
        &b[2],
        Box::new(|_| {
            let d = plugins::dir();
            let _ = std::fs::create_dir_all(&d);
            crate::oauth::open_browser(&d.display().to_string());
        }),
    );
    click(&b[3], Box::new(Plugins::refresh));
    me
}
