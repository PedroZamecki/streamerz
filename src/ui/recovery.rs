//! The Crashes and Backups pages: crash reports to read and send, and backups of the settings to make,
//! restore and delete.

use super::dialogs;
use super::lists::{bar, browser, line};
use super::rows::Rows;
use crate::backup::{self, Backup, Kind};
use crate::crash::{self, Report};
use crate::i18n::{tr, trf};
use fltk::button::Button;
use fltk::enums::{Color, Font};
use fltk::group::{Flex, Wizard};
use fltk::prelude::*;
use fltk::text::{TextBuffer, TextDisplay};
use std::cell::RefCell;
use std::rc::Rc;

/// A button calls `f` with the page, when the page isn't busy already.
fn click<T: 'static>(me: &Rc<RefCell<T>>, b: &Button, f: impl Fn(&mut T) + 'static) {
    let me = me.clone();
    b.clone().set_callback(move |_| {
        if let Ok(mut m) = me.try_borrow_mut() {
            f(&mut m);
        }
    });
}

pub struct Crashes {
    pub tab: Flex,
    view: Rows,
    text: TextBuffer,
    display: TextDisplay,
    buttons: Vec<Button>,
    reports: Vec<Report>,
}

fn crash_actions(selected: bool, readable: bool) -> [bool; 4] {
    [selected && readable, true, selected, true]
}

fn backup_actions(selected: bool) -> [bool; 4] {
    [true, selected, true, selected]
}

fn set_available(buttons: &mut [Button], states: [bool; 4]) {
    for (button, available) in buttons.iter_mut().zip(states) {
        if available {
            button.activate();
        } else {
            button.deactivate();
        }
    }
}

fn failure(key: &'static str, error: &str) {
    let message = trf(key, &[("error", error)]);
    crate::error(message.clone());
    dialogs::alert(&message);
}

fn open_folder(path: &std::path::Path) {
    if let Err(error) = std::fs::create_dir_all(path) {
        failure("recovery.folder_failed", &error.to_string());
        return;
    }
    crate::oauth::open_browser(&path.display().to_string());
}

impl Crashes {
    pub fn refresh(&mut self) {
        let sel = self.view.value();
        self.reports = crash::reports();
        self.view.clear();
        for r in &self.reports {
            self.view.add(&line(&[
                crate::fmt::shown_when(r.made),
                r.version.clone(),
                r.what.clone(),
                if r.seen {
                    tr("crashes.reported")
                } else {
                    tr("crashes.new")
                }
                .to_string(),
            ]));
        }
        let n = self.reports.len() as i32;
        if n > 0 {
            self.view.select(sel.clamp(1, n));
        }
        self.show();
    }

    fn selected(&self) -> Option<&Report> {
        self.reports.get((self.view.value() - 1).max(-1) as usize)
    }

    /// The selected report, whole, under the list.
    fn show(&mut self) {
        let selected = self.selected().is_some();
        let result = self.selected().map(|report| std::fs::read_to_string(&report.path));
        let (text, color, readable) = match result {
            Some(Ok(text)) => (text, Color::Foreground, true),
            Some(Err(error)) => (
                trf("crashes.read_failed", &[("error", &error.to_string())]),
                super::Health::Down.color(),
                false,
            ),
            None => (tr("crashes.detail_empty").to_string(), super::muted(), false),
        };
        self.text.set_text(&text);
        self.display.set_text_color(color);
        set_available(&mut self.buttons, crash_actions(selected, readable));
    }
}

pub fn crashes_tab(tabs: &Wizard) -> Rc<RefCell<Crashes>> {
    let mut col = super::tab(tabs, "Crashes");
    let view = browser(
        &mut col,
        &[170, 90, 420, 100],
        &["crashes.when", "crashes.version", "crashes.what", "crashes.status"],
        "crashes.empty",
    );
    col.fixed(&*view, 170);
    let mut shown = TextDisplay::default();
    super::scroll::view(&mut shown);
    let text = TextBuffer::default();
    shown.set_buffer(text.clone());
    shown.set_text_font(Font::Courier);
    shown.set_text_size(12);
    let b = bar(
        &mut col,
        &[
            tr("btn.report_github"),
            tr("btn.open_folder"),
            tr("btn.delete"),
            tr("btn.refresh"),
        ],
    );
    view.clone().set_menu(&b);
    col.end();
    let me = Rc::new(RefCell::new(Crashes {
        tab: col,
        view: view.clone(),
        text,
        display: shown,
        buttons: b.clone(),
        reports: Vec::new(),
    }));
    me.borrow_mut().show();
    view.clone().set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.show();
            }
        }
    });
    click(&me, &b[0], |m| {
        if let Some(r) = m.selected() {
            if let Err(error) = std::fs::read_to_string(&r.path) {
                failure("crashes.read_failed", &error.to_string());
                m.show();
                return;
            }
            crash::report_on_github(r);
            m.refresh();
        }
    });
    click(&me, &b[1], |_| {
        open_folder(&crash::folder());
    });
    click(&me, &b[2], |m| {
        let Some(r) = m.selected() else { return };
        if !dialogs::choice(tr("crashes.delete_ask"), tr("dialogs.cancel"), tr("btn.delete")) {
            return;
        }
        if let Err(e) = crash::delete(r) {
            failure("crashes.delete_failed", &e);
        }
        m.refresh();
    });
    click(&me, &b[3], Crashes::refresh);
    me
}

pub struct Backups {
    pub tab: Flex,
    cfg: super::lists::Cfg,
    cmds: super::lists::Cmds,
    view: Rows,
    backups: Vec<Backup>,
    buttons: Vec<Button>,
}

/// What made a backup, as the list says it.
fn why(b: &Backup) -> String {
    match b.info.kind {
        Kind::Update => trf("backups.kind_update", &[("version", &b.info.to)]),
        Kind::Manual => tr("backups.kind_manual").into(),
        Kind::Restore => tr("backups.kind_restore").into(),
    }
}

fn size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

impl Backups {
    pub fn refresh(&mut self) {
        let sel = self.view.value();
        self.backups = backup::list();
        self.view.clear();
        for b in &self.backups {
            self.view.add(&line(&[
                crate::fmt::shown_when(b.info.made),
                if b.info.version.is_empty() {
                    tr("backups.version_unknown").into()
                } else {
                    b.info.version.clone()
                },
                why(b),
                size(b.size),
            ]));
        }
        let n = self.backups.len() as i32;
        if n > 0 {
            self.view.select(sel.clamp(1, n));
        }
        self.selection_changed();
    }

    fn selected(&self) -> Option<&Backup> {
        self.backups.get((self.view.value() - 1).max(-1) as usize)
    }

    fn selection_changed(&mut self) {
        let selected = self.selected().is_some();
        set_available(&mut self.buttons, backup_actions(selected));
    }
}

pub fn backups_tab(tabs: &Wizard, cfg: &super::lists::Cfg, cmds: &super::lists::Cmds) -> Rc<RefCell<Backups>> {
    let mut col = super::tab(tabs, "Backups");
    let view = browser(
        &mut col,
        &[170, 90, 340, 90],
        &["backups.when", "backups.version", "backups.why", "backups.size"],
        "backups.empty",
    );
    super::hint(&mut col, tr("backups.hint"), 34);
    let b = bar(
        &mut col,
        &[
            tr("btn.backup_now"),
            tr("btn.restore"),
            tr("btn.open_folder"),
            tr("btn.delete"),
        ],
    );
    view.clone().set_menu(&b);
    col.end();
    let me = Rc::new(RefCell::new(Backups {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        view: view.clone(),
        backups: Vec::new(),
        buttons: b.clone(),
    }));
    me.borrow_mut().selection_changed();
    view.clone().set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.selection_changed();
            }
        }
    });
    click(&me, &b[0], |m| {
        // What's in memory goes to disk first, so the backup has it.
        if let Err(error) = crate::save(&m.cfg.borrow()) {
            failure("backups.save_failed", &error.to_string());
            return;
        }
        match super::query(&m.cmds, |e| e.flush_community()) {
            Some(Ok(())) => {}
            Some(Err(error)) => {
                failure("backups.save_failed", &error.to_string());
                return;
            }
            None => {
                dialogs::alert(tr("backups.busy"));
                return;
            }
        }
        match backup::make(Kind::Manual, crate::brand::VERSION, "") {
            Ok(b) => crate::say(trf("backups.created", &[("name", &b.name())])),
            Err(e) => failure("backups.create_failed", &e),
        }
        m.refresh();
    });
    click(&me, &b[1], |m| {
        let Some(b) = m.selected() else { return };
        let ask = trf("backups.restore_ask", &[("when", &crate::fmt::shown_when(b.info.made))]);
        if !dialogs::choice(&ask, tr("dialogs.cancel"), tr("btn.restore")) {
            return;
        }
        match backup::restore(b) {
            Ok(()) => {
                crate::say(trf("backups.restored", &[("name", &b.name())]));
                crate::update::restart();
            }
            Err(e) => {
                failure("backups.restore_failed", &e);
            }
        }
    });
    click(&me, &b[2], |_| {
        open_folder(&backup::root());
    });
    click(&me, &b[3], |m| {
        let Some(b) = m.selected() else { return };
        if !dialogs::choice(tr("backups.delete_ask"), tr("dialogs.cancel"), tr("btn.delete")) {
            return;
        }
        if let Err(e) = backup::delete(b) {
            failure("backups.delete_failed", &e);
        }
        m.refresh();
    });
    me
}

#[cfg(test)]
mod tests {
    #[test]
    fn crash_actions_require_a_selected_readable_report_before_reporting() {
        assert_eq!(super::crash_actions(false, false), [false, true, false, true]);
        assert_eq!(super::crash_actions(false, true), [false, true, false, true]);
        assert_eq!(super::crash_actions(true, false), [false, true, true, true]);
        assert_eq!(super::crash_actions(true, true), [true; 4]);
    }

    #[test]
    fn empty_backup_lists_keep_creation_and_folder_actions_available() {
        assert_eq!(super::backup_actions(false), [true, false, true, false]);
        assert_eq!(super::backup_actions(true), [true; 4]);
    }

    #[test]
    fn sizes_read_well() {
        assert_eq!(super::size(512), "512 B");
        assert_eq!(super::size(20 * 1024), "20 KB");
        assert_eq!(super::size(3 * 1024 * 1024 + 1024 * 512), "3.5 MB");
    }
}
