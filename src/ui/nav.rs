//! The sidebar: the pages under a few sections, each with an icon; a click shows its page. It folds to
//! its icons (the button on top), sliding.

use super::icons::{self, icon};
use crate::i18n::tr;
use fltk::enums::{Align, Color, Event, Font};
use fltk::frame::Frame;
use fltk::group::Wizard;
use fltk::prelude::*;
use fltk::{app, draw};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

type Cfg = Arc<tokio::sync::watch::Sender<crate::Config>>;

/// Wide, and folded to its icons.
const WIDE: i32 = 176;
const MINI: i32 = 52;
const ITEM: i32 = 27;
const SECTION: i32 = 24;
/// The fold button's line.
const TOP: i32 = 36;
/// The footer's: the logo and the version.
const FOOT: i32 = 44;

/// Sections and their pages: (page label, icon).
type Page = (&'static str, &'static str, &'static str);
type Section = (&'static str, &'static str, &'static [Page]);

const NAV: &[Section] = &[
    (
        "Live",
        "nav.section.live",
        &[
            ("Chat", "nav.page.chat", "message-square"),
            ("Accounts", "nav.page.accounts", "user-round"),
            ("Community", "nav.page.community", "users"),
        ],
    ),
    (
        "Automation",
        "nav.section.automation",
        &[
            ("Actions", "nav.page.actions", "zap"),
            ("Commands", "nav.page.commands", "terminal"),
            ("Timers", "nav.page.timers", "timer"),
            ("Queues", "nav.page.queues", "list-ordered"),
            ("Groups", "nav.page.groups", "folder"),
            ("Plugins", "nav.page.plugins", "puzzle"),
        ],
    ),
    (
        "Connections",
        "nav.section.connections",
        &[
            ("Servers", "nav.page.servers", "server"),
            ("Stream Apps", "nav.page.stream_apps", "monitor-play"),
            ("Donations", "nav.page.donations", "heart"),
        ],
    ),
    (
        "Data",
        "nav.section.data",
        &[
            ("Globals", "nav.page.globals", "braces"),
            ("History", "nav.page.history", "history"),
        ],
    ),
    (
        "App",
        "nav.section.app",
        &[
            ("Settings", "nav.page.settings", "settings"),
            ("Log", "nav.page.log", "scroll-text"),
        ],
    ),
];

/// The sidebar's width as saved.
pub fn width(cfg: &Cfg) -> i32 {
    if cfg.borrow().window.nav_collapsed {
        MINI
    } else {
        WIDE
    }
}

/// Each line of the sidebar from `y`: its top, and the page (none for a section title).
fn lines(y: i32) -> impl Iterator<Item = (i32, (&'static str, &'static str), Option<&'static str>)> {
    let mut y = y + TOP;
    NAV.iter().flat_map(move |(sec_id, sec_tr, pages)| {
        let mut out = vec![(y, (*sec_id, *sec_tr), None)];
        y += SECTION;
        for (page_id, page_tr, icon) in pages.iter() {
            out.push((y, (*page_id, *page_tr), Some(*icon)));
            y += ITEM;
        }
        out
    })
}

fn total_height() -> i32 {
    lines(0).last().map(|(ly, _, _)| ly + ITEM + 10).unwrap_or(TOP + 500)
}

/// The page called `label`.
pub fn page(pages: &Wizard, label: &str) -> Option<fltk::widget::Widget> {
    (0..pages.children())
        .filter_map(|i| pages.child(i))
        .find(|c| c.label() == label)
}

/// Shows the page called `label`, as a click on it would.
pub fn go(pages: &Wizard, label: &str) {
    if let Some(p) = page(pages, label) {
        let mut pages = pages.clone();
        pages.set_current_widget(&p);
        pages.do_callback();
    }
}

/// Slides to `target` wide, a third of the way a frame (timeouts only while it moves).
fn slide(mut f: Frame, mut pages: Wizard, target: i32) {
    let w = f.w();
    if w == target {
        // Resizing the window later scales from here.
        if let Some(mut win) = f.window() {
            win.init_sizes();
        }
        return;
    }
    let step = (target - w) / 3;
    let w = if step == 0 { target } else { w + step };
    let right = pages.x() + pages.w();
    f.resize(f.x(), f.y(), w, f.h());
    pages.resize(f.x() + w, pages.y(), right - f.x() - w, pages.h());
    if let Some(mut win) = f.window() {
        win.redraw();
    }
    app::add_timeout3(0.016, move |_| slide(f.clone(), pages.clone(), target));
}

fn wide(f: &Frame) -> bool {
    f.w() > (WIDE + MINI) / 2
}

/// The sidebar for `pages`, `h` tall at the window's left; redraw it when the page changes.
pub fn sidebar(h: i32, pages: &Wizard, cfg: &Cfg) -> Frame {
    let mut f = Frame::new(0, 0, width(cfg), h, None);
    let hover: Rc<Cell<Option<&'static str>>> = Rc::default();
    let scroll_y: Rc<Cell<i32>> = Rc::new(Cell::new(0));
    f.draw({
        let (pages, hover, scroll_y) = (pages.clone(), hover.clone(), scroll_y.clone());
        move |f| {
            let bg = Color::color_average(Color::Background, Color::Background2, 0.5);
            let fg = Color::Foreground;
            let quiet = Color::color_average(fg, bg, 0.72);
            let lit = |on: bool| Color::color_average(fg, bg, if on { 0.12 } else { 0.06 });
            draw::draw_rect_fill(f.x(), f.y(), f.w(), f.h(), bg);
            draw::set_draw_color(lit(true));
            draw::draw_line(f.x() + f.w() - 1, f.y(), f.x() + f.w() - 1, f.y() + f.h());

            // Pinned fold button at the top
            if hover.get() == Some("") {
                draw::set_draw_color(lit(false));
                draw::draw_rounded_rectf(f.x() + 8, f.y() + 6, MINI - 16, ITEM, 6);
            }
            let fold = if wide(f) { "panel-left-close" } else { "panel-left-open" };
            if let Some(mut i) = icon(fold, quiet, 16) {
                i.draw(f.x() + 18, f.y() + 6 + (ITEM - 16) / 2, 16, 16);
            }

            let tot = total_height();
            let max_s = (tot - f.h() + FOOT).max(0);
            let sy = scroll_y.get().min(max_s);
            if sy != scroll_y.get() {
                scroll_y.set(sy);
            }

            draw::push_clip(f.x(), f.y() + TOP, f.w() - 1, f.h() - TOP - FOOT);
            let up = pages.try_current_widget().map(|w| w.label()).unwrap_or_default();
            for (ly, (id, tr_key), icon_name) in lines(f.y() - sy) {
                let Some(icon_name) = icon_name else {
                    if wide(f) {
                        draw::set_font(Font::HelveticaBold, 11);
                        draw::set_draw_color(super::muted());
                        let t = tr(tr_key).to_uppercase();
                        draw::draw_text2(&t, f.x() + 16, ly + 8, WIDE - 24, SECTION - 8, Align::Left);
                    } else {
                        draw::set_draw_color(lit(true));
                        let y = ly + SECTION / 2 + 2;
                        draw::draw_line(f.x() + 14, y, f.x() + f.w() - 15, y);
                    }
                    continue;
                };
                let on = id == up;
                if on || hover.get() == Some(id) {
                    draw::set_draw_color(lit(on));
                    draw::draw_rounded_rectf(f.x() + 8, ly + 1, f.w() - 16, ITEM - 2, 6);
                }
                let c = if on { fg } else { quiet };
                if let Some(mut i) = icon(icon_name, c, 16) {
                    i.draw(f.x() + 18, ly + (ITEM - 16) / 2, 16, 16);
                }
                // Cut inside the highlight while it slides; gone when folded.
                draw::set_font(if on { Font::HelveticaBold } else { Font::Helvetica }, 14);
                draw::set_draw_color(c);
                draw::push_clip(f.x() + 8, ly, f.w() - 16, ITEM);
                let name = super::fit::ellipsize(tr(tr_key), WIDE - 56, |t| draw::width(t) as i32);
                draw::draw_text2(&name, f.x() + 44, ly, WIDE - 52, ITEM, Align::Left);
                draw::pop_clip();
            }
            draw::pop_clip();

            // The footer: the logo, and with room the name and version.
            let top = f.y() + f.h() - FOOT;
            draw::set_draw_color(lit(true));
            draw::draw_line(f.x() + 12, top, f.x() + f.w() - 13, top);
            if let Some(mut logo) = icons::logo(icons::is_dark(), 24) {
                logo.draw(f.x() + 14, top + (FOOT - 24) / 2, 24, 24);
            }
            if wide(f) {
                draw::push_clip(f.x() + 44, top, f.w() - 52, FOOT);
                draw::set_font(Font::HelveticaBold, 13);
                draw::set_draw_color(fg);
                draw::draw_text2("Streamerz", f.x() + 46, top + 5, WIDE - 56, 18, Align::Left);
                draw::set_font(Font::Helvetica, 11);
                draw::set_draw_color(super::muted());
                draw::draw_text2(crate::brand::VERSION, f.x() + 46, top + 22, WIDE - 56, 16, Align::Left);
                draw::pop_clip();
            }

            if max_s > 0 {
                let avail = f.h() - TOP - FOOT;
                let bar_h = (avail * avail / tot).max(18);
                let bar_y = f.y() + TOP + (sy * (avail - bar_h) / max_s);
                draw::set_draw_color(Color::color_average(fg, bg, 0.25));
                draw::draw_rounded_rectf(f.x() + f.w() - 5, bar_y, 3, bar_h, 1);
            }
        }
    });
    f.handle({
        let (pages, cfg, scroll_y) = (pages.clone(), cfg.clone(), scroll_y.clone());
        move |f, ev| {
            let tot = total_height();
            let max_s = (tot - f.h() + FOOT).max(0);
            // The page under the pointer; "" for the fold button.
            let under = |f: &Frame| {
                let (x, y) = (app::event_x(), app::event_y());
                if (f.y()..f.y() + TOP).contains(&y) && x < f.x() + MINI - 8 {
                    return Some("");
                }
                let sy = scroll_y.get().min(max_s);
                lines(f.y() - sy)
                    .find(|(ly, _, icon)| icon.is_some() && (*ly..ly + ITEM).contains(&y))
                    .map(|l| l.1 .0)
            };
            match ev {
                Event::MouseWheel => {
                    if max_s > 0 {
                        let dy = match app::event_dy() {
                            fltk::app::MouseWheel::Down => 28,
                            fltk::app::MouseWheel::Up => -28,
                            _ => 0,
                        };
                        let cur = scroll_y.get();
                        let next = (cur + dy).clamp(0, max_s);
                        if next != cur {
                            scroll_y.set(next);
                            f.redraw();
                        }
                        return true;
                    }
                    false
                }
                Event::Enter => true,
                Event::Move | Event::Leave => {
                    let h = if ev == Event::Move { under(f) } else { None };
                    if h != hover.get() {
                        hover.set(h);
                        f.set_tooltip(match h {
                            Some("") if wide(f) => tr("nav.fold"),
                            Some("") => tr("nav.unfold"),
                            Some(id) if !wide(f) => NAV
                                .iter()
                                .flat_map(|(_, _, pages)| pages.iter())
                                .find(|(p_id, _, _)| *p_id == id)
                                .map(|(_, tr_key, _)| tr(tr_key))
                                .unwrap_or(id),
                            _ => "",
                        });
                        f.redraw();
                    }
                    true
                }
                Event::Push => {
                    match under(f) {
                        Some("") => {
                            cfg.send_if_modified(|c| {
                                c.window.nav_collapsed = !c.window.nav_collapsed;
                                false
                            });
                            slide(f.clone(), pages.clone(), width(&cfg));
                        }
                        Some(label) => go(&pages, label),
                        None => {}
                    }
                    true
                }
                _ => false,
            }
        }
    });
    f
}
