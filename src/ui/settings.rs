//! The Settings page: sections of settings, each a name, a line on what it does, and its control.
//! A change applies and saves at once.

use super::fit::Ellipsis;
use super::Theme;
use super::{button, icons, tell, ROW};
use crate::i18n::{self, tr, trf};
use crate::update::Updates;
use crate::Config;
use fltk::button::Button;
use fltk::button::ToggleButton;
use fltk::enums::{Align, Color, Event, Font, FrameType};
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
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
        .with_align(Align::Left | Align::Inside | Align::ImageNextToText)
        .ellipsis();
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
        .with_align(Align::Left | Align::Inside | Align::Bottom)
        .ellipsis();
    n.set_label_size(14);
    let mut a = Frame::default()
        .with_label(&about.replace('@', "@@"))
        .with_align(Align::Left | Align::Inside | Align::Top | Align::Clip)
        .ellipsis();
    a.set_label_size(12);
    a.set_label_color(super::muted());
    // Too long for the line: cut, and whole when pointed at.
    a.set_tooltip(&about.replace('@', "@@"));
    text.end();
    col.fixed(&row, 44);
    // The control, centred in the line's height.
    let mut right = Flex::default().column();
    Frame::default();
    WidgetExt::set_size(&mut right, w, ROW);
    row.fixed(&right, w);
    right.set_pad(0);
    right
}

/// A setting whose description is a file's path: pointing at it shows buttons to open the file and to
/// copy the path, right after it. Returns the row, open for its control (`w` wide) on the right.
fn path_setting(col: &mut Flex, name: &str, path: &std::path::Path, w: i32) -> Flex {
    let shown = path.display().to_string();
    let mut row = Flex::default().row();
    row.set_pad(16);
    let mut text = Flex::default().column();
    text.set_pad(0);
    let mut n = Frame::default()
        .with_label(name)
        .with_align(Align::Left | Align::Inside | Align::Bottom)
        .ellipsis();
    n.set_label_size(14);
    let mut line = Flex::default().row();
    line.set_pad(2);
    let mut about = Frame::default()
        .with_label(&shown.replace('@', "@@"))
        .with_align(Align::Left | Align::Inside | Align::Top | Align::Clip)
        .ellipsis();
    about.set_label_size(12);
    about.set_label_color(super::muted());
    about.set_tooltip(&shown.replace('@', "@@"));
    let small = |icon: &str, tip: &str| {
        let mut b = Button::default();
        b.set_frame(FrameType::NoBox);
        b.set_image(super::icons::icon(icon, super::muted(), 14));
        // Level with the path's text, which sits at the top of its line.
        b.set_align(Align::Top | Align::Inside);
        b.set_tooltip(tip);
        b.clear_visible_focus();
        b.hide();
        b
    };
    let mut open = small("external-link", tr("btn.open_file"));
    let mut copy = small("copy", tr("btn.copy_path"));
    line.fixed(&open, 18);
    line.fixed(&copy, 18);
    Frame::default();
    line.end();
    text.end();
    // The path as wide as its text (up to what's left), so the buttons sit right after it.
    line.resize_callback({
        let (about, open) = (about.clone(), open.clone());
        move |l, _, _, w, _| {
            let text = if super::SHOWN.load(std::sync::atomic::Ordering::Relaxed) {
                draw::set_font(Font::Helvetica, 12);
                draw::width(&shown) as i32 + 6
            } else {
                shown.len() as i32 * 7
            };
            l.fixed(&about, text.min(w - 2 * 20).max(0));
            l.fixed(&open, 18);
        }
    });
    // Shown while the pointer is on the setting's text (FLTK tells the group it left only when the
    // pointer leaves all of it, buttons included).
    text.handle({
        let (open, copy, line) = (open.clone(), copy.clone(), line.clone());
        move |t, ev| {
            let on = match ev {
                Event::Enter | Event::Move => true,
                Event::Leave => false,
                _ => return false,
            };
            if open.visible() != on {
                for mut b in [open.clone(), copy.clone()] {
                    if on {
                        b.show();
                    } else {
                        b.hide();
                    }
                }
                // A Flex leaves out hidden widgets: place them again.
                line.layout();
                t.redraw();
            }
            false
        }
    });
    open.set_callback({
        let p = path.display().to_string();
        move |_| crate::oauth::open_browser(&p)
    });
    copy.set_callback({
        let p = path.display().to_string();
        move |_| {
            app::copy(&p);
            crate::say("Copied the settings file's path".into());
        }
    });
    col.fixed(&row, 44);
    let mut right = Flex::default().column();
    Frame::default();
    WidgetExt::set_size(&mut right, w, ROW);
    row.fixed(&right, w);
    right.set_pad(0);
    right
}

/// Ends a setting's control column.
fn done(mut right: Flex, control: &impl WidgetExt) {
    right.fixed(control, ROW);
    Frame::default();
    right.end();
    if let Some(mut row) = right.parent().and_then(|p| fltk::group::Flex::from_dyn_widget(&p)) {
        // The control gives way before the text does: at most what it asked for, at least a stub.
        let want = right.w();
        row.resize_callback(move |r, _, _, w, _| {
            r.fixed(&right, want.min((w - 190).max(64)));
        });
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
    let (scroll, mut col) = super::tab_scrolled(tabs, "Settings", 840);
    col.set_pad(2);
    let c = cfg.borrow();
    let chosen = c.theme.unwrap_or(if c.dark { Theme::Dark } else { Theme::Light });
    let (hidden, autostart, updates, debug_log) = (c.start_hidden, c.autostart, c.updates, c.debug_log);
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
        .with_align(Align::Left | Align::Inside | Align::Bottom)
        .ellipsis();
    name.set_label_font(Font::HelveticaBold);
    name.set_label_size(16);
    let mut tag = Frame::default()
        .with_label(tr("settings.about.tagline"))
        .with_align(Align::Left | Align::Inside | Align::Top)
        .ellipsis();
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
    let mut theme = super::fit::choice();
    let system = if super::system_is_dark() {
        tr("settings.theme.dark")
    } else {
        tr("settings.theme.light")
    };
    theme.add_choice(&format!(
        "{}|{}|{}",
        trf("settings.theme.system", &[("theme", system)]),
        tr("settings.theme.dark"),
        tr("settings.theme.light")
    ));
    theme.set_value(match chosen {
        Theme::System => 0,
        Theme::Dark => 1,
        Theme::Light => 2,
    });
    done(r, &theme);
    theme.set_callback({
        let cfg = cfg.clone();
        move |t| {
            let picked = [Theme::System, Theme::Dark, Theme::Light][t.value().clamp(0, 2) as usize];
            put(&cfg, |c| c.theme = Some(picked));
            // The window applies it (and keeps `dark` in step) as it would a change of the desktop's.
            super::theme_changed();
        }
    });

    let r = setting(&mut col, tr("settings.language"), tr("settings.language.about"), 200);
    let mut language = super::fit::choice();
    let languages: Vec<(&str, String)> = i18n::all().map(|(code, name)| (code, name.to_string())).collect();
    let detected = languages
        .iter()
        .find(|l| l.0 == i18n::detect())
        .map_or("", |l| l.1.as_str());
    let mut names = vec![trf("settings.language.auto", &[("language", detected)])];
    names.extend(languages.iter().map(|l| l.1.clone()));
    language.add_choice(&items(&names));
    language.set_value(languages.iter().position(|l| l.0 == locale).map_or(0, |i| i as i32 + 1));
    done(r, &language);
    language.set_callback({
        let cfg = cfg.clone();
        move |l| {
            // The first is "automatic": no language saved, the system's followed.
            let code = i18n::all()
                .nth((l.value().max(0) as usize).wrapping_sub(1))
                .map_or("", |l| l.0);
            put(&cfg, |c| c.locale = code.into());
            i18n::set(code);
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
                crate::error(format!("Start automatically: {e}"));
            }
        }
    });

    section(&mut col, tr("settings.datetime"), "calendar");
    let now = crate::fmt::now();
    let system = crate::fmt::locale_patterns();
    let example = |d: &str, fallback: &str| crate::fmt::date(&now, if d.is_empty() { fallback } else { d });
    let r = setting(&mut col, tr("settings.dates"), tr("settings.dates.about"), 200);
    let mut dates = super::fit::choice();
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
    let mut times = super::fit::choice();
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
    let mut upd = super::fit::choice();
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
    // Little room: the button keeps only its icon.
    let full = super::fit::grown(110, tr("settings.check_now"), true);
    row.resize_callback({
        let check = check.clone();
        move |r, _, _, w, _| {
            r.fixed(&check, if w >= 140 + full { full } else { 34 });
        }
    });
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

    section(&mut col, tr("settings.log"), "scroll-text");
    let r = setting(&mut col, tr("settings.debug_log"), tr("settings.debug_log.about"), 44);
    let debug = switch(debug_log);
    done(r, &debug);
    debug.clone().set_callback({
        let cfg = cfg.clone();
        move |b| {
            let on = b.value();
            put(&cfg, |c| c.debug_log = on);
            crate::set_debug_log(on);
        }
    });

    section(&mut col, tr("settings.files"), "folder");
    let path = crate::config_path();
    let r = path_setting(&mut col, tr("settings.settings_file"), &path, 210);
    let mut row = Flex::default().row();
    row.set_pad(8);
    // Right-aligned: the space before the buttons takes what they don't.
    Frame::default();
    let mut open = button(&mut row, tr("btn.open"), 90);
    let mut change = button(&mut row, tr("btn.change"), 100);
    row.end();
    done(r, &row);
    change.set_callback(move |_| change_location());
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

/// Settings > Files > Change: copies the settings file to a folder picked and makes that the place from
/// the next start on (the old file stays where it was).
fn change_location() {
    let mut c = fltk::dialog::NativeFileChooser::new(fltk::dialog::NativeFileChooserType::BrowseDir);
    let _ = c.set_directory(
        &crate::config_path()
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default(),
    );
    c.show();
    let dir = c.filename();
    if dir.as_os_str().is_empty() {
        return;
    }
    let (from, to) = (crate::config_path(), dir.join("streamerz.json"));
    if to == from {
        return;
    }
    let done = std::fs::create_dir_all(&dir)
        .and_then(|()| if to.exists() { Ok(0) } else { std::fs::copy(&from, &to) })
        .and_then(|_| crate::remember_location(&to));
    match done {
        Ok(()) => {
            let extra = if cfg!(windows) {
                ""
            } else {
                tr("settings.location.relogin")
            };
            super::dialogs::message(&format!(
                "{}{extra}",
                trf("settings.location.moved", &[("path", &to.display().to_string())])
            ));
        }
        Err(e) => super::dialogs::alert(&trf("location.cant_use", &[("error", &e.to_string())])),
    }
}
