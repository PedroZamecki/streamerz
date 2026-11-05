//! The sidebar: the pages under a few sections, each with an icon; a click shows its page. It folds to
//! its icons using the button on top.

use super::icons::{self, icon};
use crate::i18n::tr;
use fltk::enums::{Align, Color, Event, Font, Key};
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
const ITEM: i32 = 25;
const SECTION: i32 = 18;
/// The fold button's line.
const TOP: i32 = 32;
/// The footer's: the logo and the version.
const FOOT: i32 = 40;

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
            ("Polls", "nav.page.polls", "vote"),
        ],
    ),
    (
        "Automation",
        "nav.section.automation",
        &[
            ("Actions", "nav.page.actions", "zap"),
            ("Commands", "nav.page.commands", "terminal"),
            ("Timers", "nav.page.timers", "timer"),
            ("Files", "nav.page.files", "file-text"),
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
            ("Integrations", "nav.page.integrations", "plug"),
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
            ("MCP", "nav.page.mcp", "bot"),
            ("Log", "nav.page.log", "scroll-text"),
            ("Crashes", "nav.page.crashes", "bug"),
            ("Backups", "nav.page.backups", "archive"),
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

/// Reveal the active page when the sidebar receives focus or keyboard navigation moves it.
fn reveal_page(label: &str, scroll: i32, height: i32) -> i32 {
    let max_scroll = (total_height() - height + FOOT).max(0);
    let Some((top, _, _)) = lines(0).find(|(_, (id, _), _)| *id == label) else {
        return scroll.clamp(0, max_scroll);
    };
    let bottom = height - FOOT;
    let next = if top - scroll < TOP {
        top - TOP
    } else if top + ITEM - scroll > bottom {
        top + ITEM - bottom
    } else {
        scroll
    };
    next.clamp(0, max_scroll)
}

/// Arrow navigation follows the same order as the visible sidebar.
fn keyboard_page(current: &str, key: Key) -> Option<&'static str> {
    let pages = || NAV.iter().flat_map(|(_, _, pages)| pages.iter().map(|p| p.0));
    let last = pages().count() - 1;
    let current = pages().position(|p| p == current).unwrap_or(0);
    let next = match key {
        Key::Up => current.saturating_sub(1),
        Key::Down => (current + 1).min(last),
        Key::Home => 0,
        Key::End => last,
        _ => return None,
    };
    pages().nth(next)
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

/// Apply the latest fold choice immediately, without competing animation callbacks.
fn resize_sidebar(mut f: Frame, mut pages: Wizard, target: i32) {
    let right = pages.x() + pages.w();
    f.resize(f.x(), f.y(), target, f.h());
    pages.resize(f.x() + target, pages.y(), right - f.x() - target, pages.h());
    if let Some(mut win) = f.window() {
        win.init_sizes();
        win.redraw();
    }
}

pub(super) fn fit_window(f: Frame, pages: Wizard, cfg: &Cfg, window_width: i32) {
    resize_sidebar(f, pages, if window_width < 760 { MINI } else { width(cfg) });
}

fn wide(f: &Frame) -> bool {
    f.w() > (WIDE + MINI) / 2
}

/// The sidebar for `pages`, `h` tall at the window's left; redraw it when the page changes.
pub fn sidebar(h: i32, pages: &Wizard, cfg: &Cfg) -> Frame {
    let mut f = Frame::new(0, 0, width(cfg), h, None);
    f.set_label(tr("nav.navigation"));
    f.set_label_type(fltk::enums::LabelType::None);
    let hover: Rc<Cell<Option<&'static str>>> = Rc::default();
    let scroll_y: Rc<Cell<i32>> = Rc::new(Cell::new(0));
    f.draw({
        let (pages, hover, scroll_y) = (pages.clone(), hover.clone(), scroll_y.clone());
        let mut last_page = String::new();
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
            let up = pages.try_current_widget().map(|w| w.label()).unwrap_or_default();
            let sy = if up != last_page {
                last_page.clone_from(&up);
                reveal_page(&up, scroll_y.get(), f.h())
            } else {
                scroll_y.get().min(max_s)
            };
            if sy != scroll_y.get() {
                scroll_y.set(sy);
            }

            draw::push_clip(f.x(), f.y() + TOP, f.w() - 1, f.h() - TOP - FOOT);
            for (ly, (id, tr_key), icon_name) in lines(f.y() - sy) {
                let Some(icon_name) = icon_name else {
                    if wide(f) {
                        draw::set_font(Font::HelveticaBold, 11);
                        draw::set_draw_color(super::muted());
                        let t = tr(tr_key).to_uppercase();
                        draw::draw_text2(&t, f.x() + 16, ly + 4, WIDE - 24, SECTION - 4, Align::Left);
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
                if on && f.has_focus() {
                    draw::set_draw_color(fg);
                    draw::draw_rounded_rect(f.x() + 8, ly + 1, f.w() - 16, ITEM - 2, 6);
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
                Event::Focus | Event::Unfocus => {
                    if ev == Event::Focus {
                        let current = pages.try_current_widget().map(|w| w.label()).unwrap_or_default();
                        scroll_y.set(reveal_page(&current, scroll_y.get(), f.h()));
                    }
                    f.redraw();
                    true
                }
                Event::KeyDown => {
                    let key = app::event_key();
                    let current = pages.try_current_widget().map(|w| w.label()).unwrap_or_default();
                    if let Some(label) = keyboard_page(&current, key) {
                        go(&pages, label);
                        scroll_y.set(reveal_page(label, scroll_y.get(), f.h()));
                        f.redraw();
                        return true;
                    }
                    if matches!(key, Key::Left | Key::Right) {
                        cfg.send_if_modified(|c| {
                            c.window.nav_collapsed = key == Key::Left;
                            false
                        });
                        resize_sidebar(f.clone(), pages.clone(), width(&cfg));
                        return true;
                    }
                    false
                }
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
                    let _ = f.take_focus();
                    match under(f) {
                        Some("") => {
                            let collapsed = wide(f);
                            cfg.send_if_modified(|c| {
                                c.window.nav_collapsed = collapsed;
                                false
                            });
                            resize_sidebar(f.clone(), pages.clone(), width(&cfg));
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
    f.resize_callback({
        let (pages, scroll) = (pages.clone(), scroll_y.clone());
        move |frame, _, _, _, height| {
            let page = pages.try_current_widget().map(|w| w.label()).unwrap_or_default();
            scroll.set(reveal_page(&page, scroll.get(), height));
            frame.redraw();
        }
    });
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_fits_above_the_footer_in_a_standard_tall_window() {
        assert!(total_height() + FOOT <= 800 - 46);
    }

    #[test]
    fn refocusing_reveals_the_current_page_without_changing_it() {
        let bottom = reveal_page("Backups", 0, 700);
        assert!(bottom > 0);
        assert_eq!(reveal_page("Backups", bottom, 700), bottom);
        let first = reveal_page("Chat", bottom, 700);
        let first_top = lines(0).find(|(_, (id, _), _)| *id == "Chat").unwrap().0;
        assert!(first_top - first >= TOP);
        assert!(first_top + ITEM - first <= 700 - FOOT);
        let top = lines(0).find(|(_, (id, _), _)| *id == "Backups").unwrap().0;
        assert!(top - bottom >= TOP);
        assert!(top + ITEM - bottom <= 700 - FOOT);
    }

    #[test]
    #[ignore]
    fn rapid_fold_changes_keep_the_latest_choice() {
        let _app = app::App::default();
        let mut win = fltk::window::DoubleWindow::new(0, 0, 1000, 700, None);
        let sidebar = Frame::new(0, 0, WIDE, 700, None);
        let pages = Wizard::new(WIDE, 0, 1000 - WIDE, 700, None);
        pages.end();
        win.end();
        win.show();
        for target in [MINI, WIDE, MINI, WIDE] {
            resize_sidebar(sidebar.clone(), pages.clone(), target);
        }
        for _ in 0..4 {
            app::wait_for(0.02).unwrap();
            assert_eq!(sidebar.w(), WIDE);
            assert_eq!(pages.x(), WIDE);
            assert_eq!(pages.x() + pages.w(), 1000);
        }
        win.hide();
        app::delete_widget(win);
    }

    #[test]
    fn keyboard_navigation_crosses_sections_and_stops_at_the_ends() {
        assert_eq!(keyboard_page("Polls", Key::Down), Some("Actions"));
        assert_eq!(keyboard_page("Actions", Key::Up), Some("Polls"));
        assert_eq!(keyboard_page("Chat", Key::Up), Some("Chat"));
        assert_eq!(keyboard_page("Backups", Key::Down), Some("Backups"));
        assert_eq!(keyboard_page("Settings", Key::Home), Some("Chat"));
        assert_eq!(keyboard_page("Chat", Key::End), Some("Backups"));
        assert_eq!(keyboard_page("Chat", Key::Tab), None);
    }
}
