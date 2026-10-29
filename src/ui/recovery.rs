//! The Crashes and Backups pages: crash reports to read and send, and backups of the settings to make,
//! restore and delete.

use super::dialogs;
use super::lists::{bar, browser, line};
use super::rows::Rows;
use crate::backup::{self, Backup, Kind};
use crate::crash::{self, Report};
use crate::i18n::{tr, trf};
use fltk::button::Button;
use fltk::enums::Font;
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
    reports: Vec<Report>,
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
        let text = self.selected().map(Report::text).unwrap_or_default();
        self.text.set_text(&text);
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
        reports: Vec::new(),
    }));
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
            crash::report_on_github(r);
            m.refresh();
        }
    });
    click(&me, &b[1], |_| {
        crate::oauth::open_browser(&crash::folder().display().to_string());
    });
    click(&me, &b[2], |m| {
        let Some(r) = m.selected() else { return };
        if !dialogs::choice(tr("crashes.delete_ask"), tr("dialogs.cancel"), tr("btn.delete")) {
            return;
        }
        if let Err(e) = crash::delete(r) {
            crate::error(format!("Delete crash report: {e}"));
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
                    "?".into()
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
    }

    fn selected(&self) -> Option<&Backup> {
        self.backups.get((self.view.value() - 1).max(-1) as usize)
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
        view,
        backups: Vec::new(),
    }));
    click(&me, &b[0], |m| {
        // What's in memory goes to disk first, so the backup has it.
        let _ = crate::save(&m.cfg.borrow());
        let _ = super::query(&m.cmds, |e| e.save_community());
        match backup::make(Kind::Manual, crate::brand::VERSION, "") {
            Ok(b) => crate::say(format!("Backed up the settings ({})", b.name())),
            Err(e) => crate::error(format!("Backup: {e}")),
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
                crate::say(format!("Restored the backup {}; restarting", b.name()));
                crate::update::restart();
            }
            Err(e) => {
                crate::error(format!("Restore: {e}"));
                dialogs::message(&trf("backups.restore_failed", &[("error", &e)]));
            }
        }
    });
    click(&me, &b[2], |_| {
        let _ = std::fs::create_dir_all(backup::root());
        crate::oauth::open_browser(&backup::root().display().to_string());
    });
    click(&me, &b[3], |m| {
        let Some(b) = m.selected() else { return };
        if !dialogs::choice(tr("backups.delete_ask"), tr("dialogs.cancel"), tr("btn.delete")) {
            return;
        }
        if let Err(e) = backup::delete(b) {
            crate::error(format!("Delete backup: {e}"));
        }
        m.refresh();
    });
    me
}

#[cfg(test)]
mod tests {
    #[test]
    fn sizes_read_well() {
        assert_eq!(super::size(512), "512 B");
        assert_eq!(super::size(20 * 1024), "20 KB");
        assert_eq!(super::size(3 * 1024 * 1024 + 1024 * 512), "3.5 MB");
    }
}
