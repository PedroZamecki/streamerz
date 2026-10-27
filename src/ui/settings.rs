//! The Settings page: sections of settings, each a name, a line on what it does, and its control.
//! A change applies and saves at once.

use super::{button, icons, tell, ROW};
use crate::i18n::{self, tr, trf};
use crate::update::Updates;
use crate::Config;
use fltk::button::ToggleButton;
use fltk::enums::{Align, Color, Font};
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
use fltk::menu::Choice;
use fltk::prelude::*;
use fltk::{app, draw};
use std::sync::Arc;
use tokio::sync::watch;

type Cfg = Arc<watch::Sender<Config>>;
type Cmds = tokio::sync::mpsc::UnboundedSender<crate::engine::UiCmd>;

/// The date patterns offered ("" is the system's).
const DATES: [&str; 8] = [
    "",
    "dd/MM/yyyy",
    "MM/dd/yyyy",
    "yyyy-MM-dd",
    "dd.MM.yyyy",
    "dd/MM/yy",
    "MM/dd/yy",
    "d MMM yyyy",
];
const TIMES: [&str; 3] = ["", "HH:mm", "h:mm tt"];
const UPDATES: [Updates; 3] = [Updates::Ask, Updates::Background, Updates::Ignore];

/// Changes the config and saves it.
fn put(cfg: &Cfg, f: impl FnOnce(&mut Config)) {
    cfg.send_modify(f);
    let _ = crate::save(&cfg.borrow());
}

/// A section's title, with its icon.
fn section(col: &mut Flex, title: &str, icon: &str) {
    let mut f = Frame::default()
        .with_label(title)
        .with_align(Align::Left | Align::Inside | Align::ImageNextToText);
    f.set_label_font(Font::HelveticaBold);
    f.set_label_size(15);
    f.set_image(icons::before_label(icon, Color::Foreground, 16));
    col.fixed(&f, 34);
}

/// A setting's line: its name and what it does on the left; returns the row, open for its control
/// (`w` wide) on the right.
fn setting(col: &mut Flex, name: &str, about: &str, w: i32) -> Flex {
    let mut row = Flex::default().row();
    row.set_pad(16);
    let mut text = Flex::default().column();
    text.set_pad(0);
    let mut n = Frame::default()
        .with_label(name)
        .with_align(Align::Left | Align::Inside | Align::Bottom);
    n.set_label_size(14);
    let mut a = Frame::default()
        .with_label(&about.replace('@', "@@"))
        .with_align(Align::Left | Align::Inside | Align::Top | Align::Clip);
    a.set_label_size(12);
    a.set_label_color(super::muted());
    // Too long for the line: cut, and whole when pointed at.
    a.set_tooltip(&about.replace('@', "@@"));
    text.end();
    col.fixed(&row, 44);
    // The control, centred in the line's height.
    let mut right = Flex::default().column();
    Frame::default();
    row.fixed(&right, w);
    right.set_pad(0);
    right
}

/// Ends a setting's control column.
fn done(mut right: Flex, control: &impl WidgetExt) {
    right.fixed(control, ROW);
    Frame::default();
    right.end();
    if let Some(row) = right.parent() {
        row.end();
    }
}

/// An on/off switch: a pill with a knob, lit when on.
fn switch(on: bool) -> ToggleButton {
    let mut b = ToggleButton::default();
    b.set_value(on);
    b.draw(|b| {
        let (w, h) = (36, 20);
        let (x, y) = (b.x() + b.w() - w, b.y() + (b.h() - h) / 2);
        let bg = Color::Background;
        let track = if b.value() {
            Color::Selection
        } else {
            Color::color_average(Color::Foreground, bg, 0.25)
        };
        draw::draw_rect_fill(b.x(), b.y(), b.w(), b.h(), bg);
        draw::set_draw_color(track);
        draw::draw_rounded_rectf(x, y, w, h, h / 2);
        draw::set_draw_color(Color::White);
        let k = h - 6;
        let kx = if b.value() { x + w - k - 3 } else { x + 3 };
        draw::draw_pie(kx, y + 3, k, k, 0.0, 360.0);
    });
    b.clear_visible_focus();
    b
}

/// A choice's items: "/" would make a submenu.
fn items(names: &[String]) -> String {
    names
        .iter()
        .map(|n| n.replace('/', "\\/"))
        .collect::<Vec<_>>()
        .join("|")
}

pub fn tab(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) {
    let (scroll, mut col) = super::tab_scrolled(tabs, "Settings", 620);
    col.set_pad(2);
    let c = cfg.borrow();
    let (dark, hidden, autostart, updates) = (c.dark, c.start_hidden, c.autostart, c.updates);
    let locale = c.locale.clone();
    let (date_f, time_f, instance) = (c.date_format.clone(), c.time_format.clone(), c.instance_name.clone());
    drop(c);

    section(&mut col, tr("settings.about"), "info");
    let mut row = Flex::default().row();
    row.set_pad(14);
    let mut logo = Frame::default();
    logo.draw(|f| {
        if let Some(mut l) = icons::logo(icons::is_dark(), 56) {
            l.draw(f.x(), f.y() + (f.h() - 56) / 2, 56, 56);
        }
    });
    row.fixed(&logo, 56);
    let mut text = Flex::default().column();
    text.set_pad(0);
    let mut name = Frame::default()
        .with_label(&trf("settings.about.version", &[("version", crate::brand::VERSION)]))
        .with_align(Align::Left | Align::Inside | Align::Bottom);
    name.set_label_font(Font::HelveticaBold);
    name.set_label_size(16);
    let mut tag = Frame::default()
        .with_label(tr("settings.about.tagline"))
        .with_align(Align::Left | Align::Inside | Align::Top);
    tag.set_label_color(super::muted());
    text.end();
    let mut right = Flex::default().column();
    Frame::default();
    let mut github = button(&mut right, tr("btn.open_github"), 130);
    right.fixed(&github, ROW);
    Frame::default();
    right.end();
    row.fixed(&right, 150);
    github.set_callback(|_| crate::oauth::open_browser(&format!("https://github.com/{}", crate::brand::REPO)));
    row.end();
    col.fixed(&row, 64);

    section(&mut col, tr("settings.appearance"), "sun");
    let r = setting(&mut col, tr("settings.theme"), tr("settings.theme.about"), 200);
    let mut theme = Choice::default();
    theme.add_choice(&format!("{}|{}", tr("settings.theme.dark"), tr("settings.theme.light")));
    theme.set_value(i32::from(!dark));
    done(r, &theme);
    theme.set_callback({
        let cfg = cfg.clone();
        move |t| {
            let dark = t.value() == 0;
            put(&cfg, |c| c.dark = dark);
            super::theme(dark);
            super::window_icons(dark);
            if let Some(mut w) = app::first_window() {
                w.redraw();
            }
        }
    });

    let r = setting(&mut col, tr("settings.language"), tr("settings.language.about"), 200);
    let mut language = Choice::default();
    let languages: Vec<(&str, String)> = i18n::all().map(|(code, name)| (code, name.to_string())).collect();
    language.add_choice(&items(&languages.iter().map(|l| l.1.clone()).collect::<Vec<_>>()));
    language.set_value(languages.iter().position(|l| l.0 == locale).unwrap_or(0) as i32);
    done(r, &language);
    language.set_callback({
        let cfg = cfg.clone();
        move |l| {
            if let Some((code, _)) = i18n::all().nth(l.value().max(0) as usize) {
                put(&cfg, |c| c.locale = code.into());
                i18n::set(code);
            }
        }
    });

    section(&mut col, tr("settings.startup"), "power");
    let r = setting(
        &mut col,
        tr("settings.start_hidden"),
        tr("settings.start_hidden.about"),
        44,
    );
    let hide = switch(hidden);
    done(r, &hide);
    hide.clone().set_callback({
        let cfg = cfg.clone();
        move |b| {
            let on = b.value();
            put(&cfg, |c| c.start_hidden = on);
        }
    });
    let about = if cfg!(windows) {
        tr("settings.autostart.about_windows")
    } else {
        tr("settings.autostart.about_linux")
    };
    let r = setting(&mut col, tr("settings.autostart"), about, 44);
    let auto = switch(autostart);
    done(r, &auto);
    auto.clone().set_callback({
        let cfg = cfg.clone();
        move |b| {
            let on = b.value();
            put(&cfg, |c| c.autostart = on);
            if let Err(e) = crate::autostart(on) {
                crate::say(format!("Start automatically: {e}"));
            }
        }
    });

    section(&mut col, tr("settings.datetime"), "calendar");
    let now = crate::fmt::now();
    let system = crate::fmt::locale_patterns();
    let example = |d: &str, fallback: &str| crate::fmt::date(&now, if d.is_empty() { fallback } else { d });
    let r = setting(&mut col, tr("settings.dates"), tr("settings.dates.about"), 200);
    let mut dates = Choice::default();
    let names: Vec<String> = DATES
        .iter()
        .map(|d| match *d {
            "" => trf("settings.system_format", &[("example", &example("", &system.0))]),
            d => example(&d.replace('/', "'/'"), ""),
        })
        .collect();
    dates.add_choice(&items(&names));
    dates.set_value(DATES.iter().position(|d| *d == date_f).unwrap_or(0) as i32);
    done(r, &dates);
    let r = setting(&mut col, tr("settings.times"), tr("settings.times.about"), 200);
    let mut times = Choice::default();
    let names: Vec<String> = TIMES
        .iter()
        .map(|t| match *t {
            "" => trf("settings.system_format", &[("example", &example("", &system.1))]),
            t => example(t, ""),
        })
        .collect();
    times.add_choice(&items(&names));
    times.set_value(TIMES.iter().position(|t| *t == time_f).unwrap_or(0) as i32);
    done(r, &times);
    let both = (dates.clone(), times.clone());
    for mut ch in [dates, times] {
        ch.set_callback({
            let (cfg, (dates, times)) = (cfg.clone(), both.clone());
            move |_| {
                let d = DATES[dates.value().clamp(0, DATES.len() as i32 - 1) as usize];
                let t = TIMES[times.value().clamp(0, TIMES.len() as i32 - 1) as usize];
                put(&cfg, |c| (c.date_format, c.time_format) = (d.into(), t.into()));
                crate::fmt::set_shown_as(d, t);
                // Lists show them as they fill again.
                super::changed();
            }
        });
    }

    section(&mut col, tr("settings.updates"), "download");
    let version = crate::version();
    let r = setting(
        &mut col,
        tr("settings.new_versions"),
        &match crate::update::found() {
            Some(tag) => trf("settings.new_versions.found", &[("version", &tag)]),
            None => trf("settings.new_versions.about", &[("version", &version)]),
        },
        320,
    );
    let mut row = Flex::default().row();
    row.set_pad(8);
    let mut upd = Choice::default();
    upd.add_choice(&format!(
        "{}|{}|{}",
        tr("settings.updates.ask"),
        tr("settings.updates.background"),
        tr("settings.updates.ignore")
    ));
    upd.set_value(UPDATES.iter().position(|u| *u == updates).unwrap_or(0) as i32);
    let mut check = button(&mut row, tr("settings.check_now"), 110);
    check.set_image(icons::before_label("refresh-cw", super::button_ink(), 14));
    check.set_align(Align::ImageNextToText);
    row.end();
    done(r, &row);
    upd.set_callback({
        let cfg = cfg.clone();
        move |u| {
            let u = UPDATES[u.value().clamp(0, 2) as usize];
            put(&cfg, |c| c.updates = u);
        }
    });
    check.set_callback({
        let (cfg, cmds) = (cfg.clone(), cmds.clone());
        move |_| {
            let cfg = cfg.clone();
            tell(&cmds, move |_| {
                tokio::task::spawn_local(crate::update::check(cfg, true));
            });
        }
    });

    section(&mut col, tr("settings.files"), "folder");
    let path = crate::config_path();
    let r = setting(&mut col, tr("settings.settings_file"), &path.display().to_string(), 290);
    let mut row = Flex::default().row();
    row.set_pad(8);
    let mut copy = button(&mut row, tr("btn.copy_path"), 110);
    let mut open = button(&mut row, tr("btn.open_folder"), 120);
    row.end();
    done(r, &row);
    copy.set_callback({
        let p = path.display().to_string();
        move |_| {
            app::copy(&p);
            crate::say("Copied the settings file's path".into());
        }
    });
    open.set_callback({
        let dir = path.parent().map(|d| d.display().to_string()).unwrap_or_default();
        move |_| crate::oauth::open_browser(&dir)
    });

    section(&mut col, tr("settings.tools"), "plug");
    let r = setting(
        &mut col,
        tr("settings.instance_name"),
        tr("settings.instance_name.about"),
        200,
    );
    let mut name = fltk::input::Input::default();
    name.set_value(&instance);
    done(r, &name);
    name.set_trigger(fltk::enums::CallbackTrigger::Changed);
    name.set_callback({
        let cfg = cfg.clone();
        move |i| {
            let v = i.value().trim().to_string();
            put(&cfg, |c| c.instance_name = v);
        }
    });
    Frame::default();
    col.end();
    scroll.end();
}
