//! The sidebar: the pages under a few sections, each with an icon; a click shows its page.

use fltk::enums::{Align, Color, Event, Font};
use fltk::frame::Frame;
use fltk::group::Wizard;
use fltk::image::SvgImage;
use fltk::prelude::*;
use fltk::{app, draw};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

pub const W: i32 = 168;
const ITEM: i32 = 30;
const SECTION: i32 = 28;

/// Sections and their pages: (page label, icon).
const NAV: &[(&str, &[(&str, &str)])] = &[
    (
        "Live",
        &[
            ("Chat", "message-square"),
            ("Accounts", "user-round"),
            ("Community", "users"),
        ],
    ),
    (
        "Automation",
        &[
            ("Actions", "zap"),
            ("Commands", "terminal"),
            ("Timers", "timer"),
            ("Queues", "list-ordered"),
            ("Groups", "folder"),
        ],
    ),
    (
        "Connections",
        &[
            ("Servers", "server"),
            ("Stream Apps", "monitor-play"),
            ("Donations", "heart"),
        ],
    ),
    ("Data", &[("Globals", "braces"), ("History", "history")]),
    ("App", &[("Settings", "settings"), ("Log", "scroll-text")]),
];

/// Lucide icons (ISC license, lucide.dev), drawn in the text's colour.
const ICONS: &[(&str, &str)] = &[
    (
        "braces",
        r#"<path d="M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5c0 1.1.9 2 2 2h1"/><path d="M16 21h1a2 2 0 0 0 2-2v-5c0-1.1.9-2 2-2a2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1"/>"#,
    ),
    (
        "folder",
        r#"<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>"#,
    ),
    (
        "heart",
        r#"<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>"#,
    ),
    (
        "history",
        r#"<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M12 7v5l4 2"/>"#,
    ),
    (
        "list-ordered",
        r#"<path d="M10 12h11"/><path d="M10 18h11"/><path d="M10 6h11"/><path d="M4 10h2"/><path d="M4 6h1v4"/><path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1"/>"#,
    ),
    (
        "message-square",
        r#"<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>"#,
    ),
    (
        "monitor-play",
        r#"<path d="M10 7.75a.75.75 0 0 1 1.142-.638l3.664 2.249a.75.75 0 0 1 0 1.278l-3.664 2.25a.75.75 0 0 1-1.142-.64z"/><path d="M12 17v4"/><path d="M8 21h8"/><rect x="2" y="3" width="20" height="14" rx="2"/>"#,
    ),
    (
        "scroll-text",
        r#"<path d="M15 12h-5"/><path d="M15 8h-5"/><path d="M19 17V5a2 2 0 0 0-2-2H4"/><path d="M8 21h12a2 2 0 0 0 2-2v-1a1 1 0 0 0-1-1H11a1 1 0 0 0-1 1v1a2 2 0 1 1-4 0V5a2 2 0 1 0-4 0v2a1 1 0 0 0 1 1h3"/>"#,
    ),
    (
        "server",
        r#"<rect width="20" height="8" x="2" y="2" rx="2" ry="2"/><rect width="20" height="8" x="2" y="14" rx="2" ry="2"/><line x1="6" x2="6.01" y1="6" y2="6"/><line x1="6" x2="6.01" y1="18" y2="18"/>"#,
    ),
    (
        "settings",
        r#"<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>"#,
    ),
    (
        "terminal",
        r#"<polyline points="4 17 10 11 4 5"/><line x1="12" x2="20" y1="19" y2="19"/>"#,
    ),
    (
        "timer",
        r#"<line x1="10" x2="14" y1="2" y2="2"/><line x1="12" x2="15" y1="14" y2="11"/><circle cx="12" cy="14" r="8"/>"#,
    ),
    (
        "user-round",
        r#"<circle cx="12" cy="8" r="5"/><path d="M20 21a8 8 0 0 0-16 0"/>"#,
    ),
    (
        "users",
        r#"<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>"#,
    ),
    (
        "zap",
        r#"<path d="M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z"/>"#,
    ),
];

/// `name` at 16 px in `c`; made the first time it's drawn.
fn icon(name: &str, c: Color) -> Option<SvgImage> {
    thread_local!(static CACHE: RefCell<HashMap<(String, u32), Option<SvgImage>>> = RefCell::default());
    let (r, g, b) = c.to_rgb();
    CACHE.with_borrow_mut(|m| {
        m.entry((name.to_string(), c.bits()))
            .or_insert_with(|| {
                let body = ICONS.iter().find(|i| i.0 == name)?.1;
                let svg = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#{r:02x}{g:02x}{b:02x}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"##
                );
                let mut i = SvgImage::from_data(&svg).ok()?;
                i.scale(16, 16, true, true);
                Some(i)
            })
            .clone()
    })
}

/// Each line of the sidebar from `y`: its top, and the page (none for a section title).
fn lines(y: i32) -> impl Iterator<Item = (i32, &'static str, Option<&'static str>)> {
    let mut y = y + 6;
    NAV.iter().flat_map(move |(section, pages)| {
        let mut out = vec![(y, *section, None)];
        y += SECTION;
        for (label, icon) in pages.iter() {
            out.push((y, *label, Some(*icon)));
            y += ITEM;
        }
        out
    })
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

/// The sidebar for `pages`; redraw it when the page changes.
pub fn sidebar(x: i32, y: i32, h: i32, pages: &Wizard) -> Frame {
    let mut f = Frame::new(x, y, W, h, None);
    let hover: Rc<Cell<Option<&'static str>>> = Rc::default();
    f.draw({
        let (pages, hover) = (pages.clone(), hover.clone());
        move |f| {
            let bg = Color::color_average(Color::Background, Color::Background2, 0.5);
            let fg = Color::Foreground;
            draw::draw_rect_fill(f.x(), f.y(), f.w(), f.h(), bg);
            draw::set_draw_color(Color::color_average(fg, bg, 0.12));
            draw::draw_line(f.x() + f.w() - 1, f.y(), f.x() + f.w() - 1, f.y() + f.h());
            let up = pages.try_current_widget().map(|w| w.label()).unwrap_or_default();
            draw::push_clip(f.x(), f.y(), f.w(), f.h());
            for (ly, label, icon_name) in lines(f.y()) {
                let Some(icon_name) = icon_name else {
                    draw::set_font(Font::HelveticaBold, 11);
                    draw::set_draw_color(super::muted());
                    draw::draw_text2(
                        &label.to_uppercase(),
                        f.x() + 16,
                        ly + 8,
                        f.w() - 24,
                        SECTION - 8,
                        Align::Left,
                    );
                    continue;
                };
                let on = label == up;
                if on || hover.get() == Some(label) {
                    draw::set_draw_color(Color::color_average(fg, bg, if on { 0.12 } else { 0.06 }));
                    draw::draw_rounded_rectf(f.x() + 8, ly + 1, f.w() - 16, ITEM - 2, 6);
                }
                let c = if on { fg } else { Color::color_average(fg, bg, 0.72) };
                if let Some(mut i) = icon(icon_name, c) {
                    i.draw(f.x() + 16, ly + (ITEM - 16) / 2, 16, 16);
                }
                draw::set_font(if on { Font::HelveticaBold } else { Font::Helvetica }, 14);
                draw::set_draw_color(c);
                draw::draw_text2(label, f.x() + 42, ly, f.w() - 50, ITEM, Align::Left);
            }
            draw::pop_clip();
        }
    });
    f.handle({
        let pages = pages.clone();
        move |f, ev| {
            let under = || {
                let y = app::event_y();
                lines(f.y())
                    .find(|(ly, _, icon)| icon.is_some() && (*ly..ly + ITEM).contains(&y))
                    .map(|l| l.1)
            };
            match ev {
                Event::Enter => true,
                Event::Move | Event::Leave => {
                    let h = if ev == Event::Move { under() } else { None };
                    if h != hover.get() {
                        hover.set(h);
                        f.redraw();
                    }
                    true
                }
                Event::Push => {
                    if let Some(label) = under() {
                        go(&pages, label);
                    }
                    true
                }
                _ => false,
            }
        }
    });
    f
}
