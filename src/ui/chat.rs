//! The Chat tab: every set-up platform's chat together or side by side (or one platform), and one box
//! to send to a platform or all of them, as the bot or the broadcaster, with an emote picker. Lines are
//! drawn here (not a text box) so emotes, emoji, logos and badges show as pictures. Clicking a name
//! shows what's known about that user.

use super::img::{self, Pic};
use super::{button, emotes, query, tell, ROW};
use crate::engine::chatlog::{Line, PLATFORMS};
use crate::Config;
use fltk::app;
use fltk::browser::HoldBrowser;
use fltk::button::Button;
use fltk::draw;
use fltk::enums::{Align, Color, Event, Font, FrameType, Key};
use fltk::frame::Frame;
use fltk::group::{Flex, Scroll, Tabs};
use fltk::image::SvgImage;
use fltk::input::Input;
use fltk::menu::Choice;
use fltk::prelude::*;
use fltk::valuator::Scrollbar;
use fltk::window::Window;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc::UnboundedSender, watch};

type Cmds = UnboundedSender<crate::engine::UiCmd>;

const NAMES: [&str; 4] = ["Twitch", "YouTube", "Kick", "TikTok"];
/// Text size, emote height, and a line's least height.
const TEXT: i32 = 14;
const EMOTE: i32 = 28;
const LINE: i32 = 22;
/// Lines kept per view (the engine keeps as many).
const KEEP: usize = 500;

/// The role badges: broadcaster, moderator, VIP, subscriber.
const BADGES: [(&str, &str); 4] = [
    (
        "Broadcaster",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="3" fill="#E91916"/><path d="M3.5 6h7.5v6H3.5zM11 8.2l3.5-2.2v6l-3.5-2.2z" fill="#fff"/></svg>"##,
    ),
    (
        "Moderator",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="3" fill="#00AD03"/><path d="M13.5 3.5 7 10l1 1 6.5-6.5zM5 10.5l2.5 2.5-1 1-1-1-1.5 1.5-1-1L4.5 12l-1-1z" fill="#fff"/></svg>"##,
    ),
    (
        "VIP",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="3" fill="#E005B9"/><path d="M9 3.5 14 8l-5 6.5L4 8z" fill="#fff"/></svg>"##,
    ),
    (
        "Subscriber",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="3" fill="#8205B4"/><path d="m9 3.2 1.7 3.6 3.9.4-2.9 2.7.8 3.9L9 11.8l-3.5 2 .8-3.9-2.9-2.7 3.9-.4z" fill="#fff"/></svg>"##,
    ),
];

/// A badge or logo, parsed once (it keeps its drawn pixels).
fn svg(key: &'static str) -> Option<SvgImage> {
    thread_local!(static SVGS: RefCell<std::collections::HashMap<&'static str, Option<SvgImage>>> = RefCell::default());
    SVGS.with_borrow_mut(|m| {
        m.entry(key)
            .or_insert_with(|| match BADGES.iter().find(|b| b.0 == key) {
                Some(b) => SvgImage::from_data(b.1).ok().map(|mut s| {
                    s.scale(16, 16, true, true);
                    s
                }),
                None => img::logo(key, 16),
            })
            .clone()
    })
}

/// Milliseconds since start, for animations.
fn now_ms() -> u64 {
    thread_local!(static START: Instant = Instant::now());
    START.with(|s| s.elapsed().as_millis() as u64)
}

/// The name's colour: the platform's, made readable on the dark background, or one picked from the
/// name so each chatter keeps theirs.
fn name_color(l: &Line) -> Color {
    const PALETTE: [(u8, u8, u8); 8] = [
        (0xff, 0x7f, 0x50),
        (0x1e, 0x90, 0xff),
        (0x9a, 0xcd, 0x32),
        (0xff, 0x69, 0xb4),
        (0xda, 0xa5, 0x20),
        (0x5f, 0x9e, 0xa0),
        (0xb2, 0x7c, 0xff),
        (0x2e, 0xcc, 0x71),
    ];
    let hex = l.color.trim_start_matches('#');
    let (r, g, b) = match u32::from_str_radix(hex, 16) {
        Ok(v) if hex.len() == 6 => ((v >> 16) as u8, (v >> 8) as u8, v as u8),
        _ => {
            let h = l
                .user
                .bytes()
                .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
            PALETTE[h as usize % PALETTE.len()]
        }
    };
    // Too dark to read: halfway to white.
    let lum = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    if lum < 90.0 {
        Color::from_rgb(r / 2 + 128, g / 2 + 128, b / 2 + 128)
    } else {
        Color::from_rgb(r, g, b)
    }
}

/// One thing on a line.
enum It {
    Text(String, Font, Color),
    Pic(Pic),
    Svg(SvgImage),
    /// A word's space: where the line may wrap.
    Space,
}

impl It {
    fn size(&self) -> (i32, i32) {
        match self {
            It::Text(t, f, _) => {
                draw::set_font(*f, TEXT);
                (draw::width(t) as i32, TEXT + 4)
            }
            It::Pic(p) => (p.w(), EMOTE),
            It::Svg(s) => (s.w(), s.h()),
            It::Space => {
                draw::set_font(Font::Helvetica, TEXT);
                (draw::width(" ") as i32, 0)
            }
        }
    }
}

/// A line's things, in order.
fn items(cmds: &Cmds, l: &Line, tag: bool) -> Vec<It> {
    let gray = Color::from_rgb(0x88, 0x88, 0x88);
    let mut v = vec![It::Text(l.time.clone(), Font::HelveticaItalic, gray), It::Space];
    if tag {
        let p = PLATFORMS.iter().position(|x| *x == l.platform).unwrap_or(0);
        if let Some(s) = svg(NAMES[p]) {
            v.push(It::Svg(s));
            v.push(It::Space);
        }
    }
    if l.note {
        v.push(It::Text(l.text.clone(), Font::HelveticaItalic, gray));
        return v;
    }
    for (i, on) in [l.broadcaster, l.moderator, l.vip, l.subscriber]
        .into_iter()
        .enumerate()
    {
        if let Some(b) = on.then(|| svg(BADGES[i].0)).flatten() {
            v.push(It::Svg(b));
        }
    }
    v.push(It::Text(format!("{}:", l.user), Font::HelveticaBold, name_color(l)));
    let fg = Color::Foreground;
    let pic = |url: &str| img::get(cmds, url, EMOTE, false);
    for w in l.text.split(' ') {
        v.push(It::Space);
        let known = l.emotes.iter().find(|e| e.0 == w).map(|e| e.1.clone());
        if let Some(p) = known.or_else(|| emotes::third(&l.platform, w)).and_then(|u| pic(&u)) {
            v.push(It::Pic(p));
            continue;
        }
        for (part, url) in emotes::split(w) {
            match url.and_then(|u| pic(&u)) {
                Some(p) => v.push(It::Pic(p)),
                None => v.push(It::Text(part.to_string(), Font::Helvetica, fg)),
            }
        }
    }
    v
}

/// A line laid out: its rows (items with their x, and the row's height).
type Rows = Vec<(Vec<(It, i32)>, i32)>;

/// Lay a line out in `w` px.
fn rows(items: Vec<It>, w: i32) -> Rows {
    let mut out: Rows = vec![(Vec::new(), LINE)];
    let mut x = 0;
    for it in items {
        let (iw, ih) = it.size();
        if matches!(it, It::Space) {
            x += iw;
            continue;
        }
        let row = out.last_mut().expect("a row");
        if x + iw > w && !row.0.is_empty() {
            out.push((Vec::new(), LINE));
            x = 0;
        }
        let row = out.last_mut().expect("a row");
        row.1 = row.1.max(ih + 2);
        row.0.push((it, x));
        x += iw;
    }
    out
}

struct State {
    lines: VecDeque<Line>,
    /// Each line laid out at `width`, and its height.
    laid: VecDeque<(Rows, i32)>,
    width: i32,
    tag: bool,
    /// Scrolled to (px from the top); following the newest lines when at the bottom.
    top: i32,
    follow: bool,
    /// An animation frame is due.
    ticking: bool,
}

/// One chat view: the lines, drawn, with a scrollbar.
#[derive(Clone)]
struct Pane {
    group: Flex,
    area: Frame,
    bar: Scrollbar,
    st: Rc<RefCell<State>>,
    cmds: Cmds,
}

impl Pane {
    fn new(cmds: &Cmds) -> Pane {
        let mut group = Flex::default().row();
        group.set_pad(0);
        let mut area = Frame::default();
        area.set_frame(FrameType::DownBox);
        area.set_color(Color::Background2);
        let mut bar = Scrollbar::default();
        bar.set_type(fltk::valuator::ScrollbarType::VerticalNice);
        group.fixed(&bar, 14);
        group.end();
        let p = Pane {
            group,
            area: area.clone(),
            bar: bar.clone(),
            st: Rc::new(RefCell::new(State {
                lines: VecDeque::new(),
                laid: VecDeque::new(),
                width: -1,
                tag: false,
                top: 0,
                follow: true,
                ticking: false,
            })),
            cmds: cmds.clone(),
        };
        area.draw({
            let p = p.clone();
            move |f| p.draw(f)
        });
        area.handle({
            let p = p.clone();
            move |f, ev| p.handle(f, ev)
        });
        bar.set_callback({
            let p = p.clone();
            move |b| {
                let mut st = p.st.borrow_mut();
                st.top = b.value() as i32;
                st.follow = st.top >= p.total(&st) - p.area.h() + 4;
                drop(st);
                p.area.clone().redraw();
            }
        });
        p
    }

    fn total(&self, st: &State) -> i32 {
        st.laid.iter().map(|l| l.1).sum::<i32>() + 8
    }

    fn lay(&self, l: &Line, tag: bool, w: i32) -> (Rows, i32) {
        let r = rows(items(&self.cmds, l, tag), w);
        let h = r.iter().map(|r| r.1).sum();
        (r, h)
    }

    /// Lay the lines out again when the width changed (or pictures came in).
    fn measure(&self, st: &mut State) {
        let w = self.area.w() - 12;
        if st.width == w && st.laid.len() == st.lines.len() {
            return;
        }
        st.width = w;
        let tag = st.tag;
        st.laid = st.lines.iter().map(|l| self.lay(l, tag, w)).collect();
    }

    /// Scrollbar and position after the lines or the size changed.
    fn settle(&self, st: &mut State) {
        self.measure(st);
        let (total, h) = (self.total(st), self.area.h());
        let max = (total - h).max(0);
        if st.follow {
            st.top = max;
        }
        st.top = st.top.clamp(0, max);
        let mut bar = self.bar.clone();
        bar.set_bounds(0.0, f64::from(max));
        bar.set_slider_size(if total > 0 {
            (h as f32 / total as f32).min(1.0)
        } else {
            1.0
        });
        bar.set_value(f64::from(st.top));
        bar.set_step(1.0, 1);
    }

    fn draw(&self, f: &mut Frame) {
        let mut st = self.st.borrow_mut();
        self.settle(&mut st);
        draw::draw_box(f.frame(), f.x(), f.y(), f.w(), f.h(), f.color());
        draw::push_clip(f.x() + 2, f.y() + 2, f.w() - 4, f.h() - 4);
        let (ms, mut moving) = (now_ms(), false);
        let mut y = f.y() + 4 - st.top;
        for (laid, h) in &st.laid {
            if y + h < f.y() {
                y += h;
                continue;
            }
            if y > f.y() + f.h() {
                break;
            }
            for (row, rh) in laid {
                let rh = *rh;
                for (it, x) in row {
                    let x = f.x() + 6 + x;
                    match it {
                        It::Text(t, font, color) => {
                            draw::set_font(*font, TEXT);
                            draw::set_draw_color(*color);
                            draw::draw_text2(t, x, y, 0, rh, Align::Left);
                        }
                        It::Pic(p) => {
                            moving |= p.animated();
                            let mut i = p.at(ms).clone();
                            i.draw(x, y + (rh - i.h()) / 2, i.w(), i.h());
                        }
                        It::Svg(s) => {
                            let (w, h) = (s.w(), s.h());
                            s.clone().draw(x, y + (rh - h) / 2, w, h);
                        }
                        It::Space => {}
                    }
                }
                y += rh;
            }
        }
        draw::pop_clip();
        // Animated emotes on screen: one redraw ~20 times a second, only while this is drawn.
        if moving && !st.ticking {
            st.ticking = true;
            let p = self.clone();
            app::add_timeout3(0.05, move |_| {
                p.st.borrow_mut().ticking = false;
                p.area.clone().redraw();
            });
        }
    }

    /// The line whose name is under the pointer.
    fn name_at(&self, f: &Frame) -> Option<Line> {
        let (ex, ey) = (app::event_x() - f.x() - 6, app::event_y());
        let st = self.st.borrow();
        let mut y = f.y() + 4 - st.top;
        for (l, (laid, h)) in st.lines.iter().zip(&st.laid) {
            if !(y..y + h).contains(&ey) {
                y += h;
                continue;
            }
            for (row, rh) in laid {
                if (y..y + rh).contains(&ey) {
                    let name = row.iter().find(|(it, x)| {
                        matches!(it, It::Text(_, Font::HelveticaBold, _)) && (*x..x + it.size().0).contains(&ex)
                    });
                    return name.filter(|_| !l.note && !l.user_id.is_empty()).map(|_| l.clone());
                }
                y += rh;
            }
            return None;
        }
        None
    }

    fn handle(&self, f: &mut Frame, ev: Event) -> bool {
        match ev {
            Event::Enter => true,
            Event::Move => {
                let hand = self.name_at(f).is_some();
                if let Some(mut w) = f.window() {
                    w.set_cursor(if hand {
                        fltk::enums::Cursor::Hand
                    } else {
                        fltk::enums::Cursor::Default
                    });
                }
                true
            }
            Event::Leave => {
                if let Some(mut w) = f.window() {
                    w.set_cursor(fltk::enums::Cursor::Default);
                }
                true
            }
            Event::Push if app::event_mouse_button() == app::MouseButton::Left => {
                if let Some(l) = self.name_at(f) {
                    card(&self.cmds, &l);
                }
                true
            }
            Event::MouseWheel => {
                let mut st = self.st.borrow_mut();
                st.top += app::event_dy_value() * 3 * LINE;
                let max = (self.total(&st) - f.h()).max(0);
                st.top = st.top.clamp(0, max);
                st.follow = st.top >= max;
                self.settle(&mut st);
                drop(st);
                f.redraw();
                true
            }
            // Right click: copy the line under the pointer.
            Event::Push if app::event_mouse_button() == app::MouseButton::Right => {
                let st = self.st.borrow();
                let mut y = f.y() + 4 - st.top;
                for (l, (_, h)) in st.lines.iter().zip(&st.laid) {
                    if (y..y + h).contains(&app::event_y()) {
                        app::copy(&format!("{}: {}", l.user, l.text));
                        crate::say(format!("Copied {}'s message", l.user));
                        break;
                    }
                    y += h;
                }
                true
            }
            Event::Resize => {
                self.st.borrow_mut().width = -1;
                false
            }
            _ => false,
        }
    }

    fn add(&self, l: &Line) {
        let mut st = self.st.borrow_mut();
        if st.lines.len() == KEEP {
            st.lines.pop_front();
            st.laid.pop_front();
        }
        st.lines.push_back(l.clone());
        // Laid out on the next draw when the width isn't known yet.
        if st.width > 0 && st.laid.len() + 1 == st.lines.len() {
            let laid = self.lay(l, st.tag, st.width);
            st.laid.push_back(laid);
        }
        drop(st);
        self.area.clone().redraw();
    }

    fn clear(&self) {
        let mut st = self.st.borrow_mut();
        st.lines.clear();
        st.laid.clear();
        st.top = 0;
        st.follow = true;
        drop(st);
        self.area.clone().redraw();
    }

    /// Pictures came in: lines may be taller.
    fn remeasure(&self) {
        self.st.borrow_mut().width = -1;
        self.area.clone().redraw();
    }
}

pub struct Chat {
    pub tab: Flex,
    cfg: Arc<watch::Sender<Config>>,
    cmds: Cmds,
    view: Choice,
    to: Choice,
    /// The set-up platforms (indexes in PLATFORMS), as the view and send lists show them.
    plats: Rc<RefCell<Vec<usize>>>,
    /// All together / one platform, then one per platform for side by side.
    main: Pane,
    side: Vec<Pane>,
    seen: u64,
}

impl Chat {
    /// Add the lines that came since the last refresh (all of them after a view change).
    pub fn refresh(&mut self) {
        let (tw, kick) = {
            let c = self.cfg.borrow();
            let tw = c
                .twitch
                .broadcaster
                .as_ref()
                .map(|a| a.user_id.clone())
                .unwrap_or_default();
            let kick = c
                .kick
                .broadcaster
                .as_ref()
                .map(|a| a.user_id.to_string())
                .unwrap_or_default();
            (tw, kick)
        };
        emotes::load(&self.cmds, tw, kick);
        let plats = set_up(&self.cfg.borrow());
        if *self.plats.borrow() != plats {
            *self.plats.borrow_mut() = plats;
            self.lists();
            self.rebuild();
            return;
        }
        let seen = self.seen;
        let lines = query(&self.cmds, move |e| e.chat_since(seen)).unwrap_or_default();
        let v = self.view.value();
        for l in &lines {
            match v {
                1 => {
                    let p = PLATFORMS.iter().position(|x| *x == l.platform).unwrap_or(0);
                    self.side[p].add(l);
                }
                0 => self.main.add(l),
                _ if self.shown() == Some(l.platform.as_str()) => self.main.add(l),
                _ => {}
            }
            self.seen = l.seq;
        }
    }

    /// Pictures came in.
    pub fn images(&self) {
        self.main.remeasure();
        for p in &self.side {
            p.remeasure();
        }
    }

    /// The one platform the view shows, if it shows one.
    fn shown(&self) -> Option<&'static str> {
        let v = self.view.value();
        (v >= 2)
            .then(|| self.plats.borrow().get((v - 2) as usize).map(|i| PLATFORMS[*i]))
            .flatten()
    }

    /// The view and send lists for the set-up platforms, keeping what was picked when it's still there.
    fn lists(&mut self) {
        let plats = self.plats.borrow();
        let names: Vec<&str> = plats.iter().map(|i| NAMES[*i]).collect();
        let keep = |c: &Choice, fixed: i32| {
            let v = c.value();
            if v < fixed {
                v
            } else {
                c.choice()
                    .and_then(|n| names.iter().position(|x| *x == n))
                    .map_or(0, |i| i as i32 + fixed)
            }
        };
        let (view, to) = (keep(&self.view, 2), keep(&self.to, 1));
        self.view.clear();
        self.view.add_choice(
            &["All together", "Side by side"]
                .iter()
                .chain(&names)
                .copied()
                .collect::<Vec<_>>()
                .join("|"),
        );
        self.view.set_value(view);
        self.to.clear();
        self.to.add_choice(
            &["All connected"]
                .iter()
                .chain(&names)
                .copied()
                .collect::<Vec<_>>()
                .join("|"),
        );
        self.to.set_value(to);
    }

    fn rebuild(&mut self) {
        let side = self.view.value() == 1;
        // One platform shown: what's sent goes there.
        match self
            .shown()
            .and_then(|p| self.plats.borrow().iter().position(|i| PLATFORMS[*i] == p))
        {
            Some(i) => {
                self.to.set_value(i as i32 + 1);
                self.to.deactivate();
            }
            None => {
                if !self.to.active() {
                    self.to.set_value(0);
                }
                self.to.activate();
            }
        }
        self.main.clear();
        self.main.st.borrow_mut().tag = self.view.value() == 0;
        for p in &mut self.side {
            p.clear();
        }
        if side {
            self.main.group.hide();
            let plats = self.plats.borrow();
            for (i, p) in self.side.iter_mut().enumerate() {
                if plats.contains(&i) {
                    p.group.show();
                } else {
                    p.group.hide();
                }
            }
        } else {
            self.main.group.show();
            for p in &mut self.side {
                p.group.hide();
            }
        }
        self.seen = 0;
        self.refresh();
        self.tab.layout();
        self.tab.redraw();
    }
}

pub fn tab(tabs: &Tabs, cfg: &Arc<watch::Sender<Config>>, cmds: &Cmds) -> Rc<RefCell<Chat>> {
    let mut col = super::tab(tabs, "Chat");
    let mut top = Flex::default().row();
    let l = Frame::default().with_label("Show:");
    top.fixed(&l, 50);
    let mut view = Choice::default();
    top.fixed(&view, 140);
    let hint = Frame::default()
        .with_label("Click a name for that user, right click a message to copy it")
        .with_align(Align::Right | Align::Inside);
    let _ = hint;
    let mut clear = button(&mut top, "Clear", 70);
    top.end();
    col.fixed(&top, ROW);

    let mut panes = Flex::default().row();
    panes.set_pad(4);
    let main = Pane::new(cmds);
    main.st.borrow_mut().tag = true;
    let mut side = Vec::new();
    for name in NAMES {
        let mut p = Pane::new(cmds);
        p.group.set_label(&format!("  {name}"));
        p.group.set_image(img::logo(name, 16));
        p.group.set_align(Align::Top | Align::ImageNextToText);
        p.group.hide();
        side.push(p);
    }
    panes.end();

    let mut bottom = Flex::default().row();
    let l = Frame::default().with_label("To:");
    bottom.fixed(&l, 30);
    let to = Choice::default();
    bottom.fixed(&to, 120);
    let mut who = Choice::default();
    who.add_choice("as bot|as broadcaster");
    who.set_value(0);
    who.set_tooltip("As the bot (or the broadcaster when no bot is logged in), or as the broadcaster");
    bottom.fixed(&who, 120);
    let mut input = Input::default();
    let mut pick = button(&mut bottom, "Emotes", 70);
    pick.set_tooltip("Emoji, and the emotes of the platform you send to");
    let mut send = button(&mut bottom, "Send", 70);
    bottom.end();
    col.fixed(&bottom, ROW);
    col.end();

    let me = Rc::new(RefCell::new(Chat {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        view: view.clone(),
        to: to.clone(),
        plats: Rc::default(),
        main,
        side,
        seen: 0,
    }));
    {
        let mut m = me.borrow_mut();
        *m.plats.borrow_mut() = set_up(&cfg.borrow());
        m.lists();
        m.view.set_value(0);
        m.to.set_value(0);
    }
    // Side by side needs room: titles above each pane.
    panes.set_margins(0, if view.value() == 1 { 20 } else { 0 }, 0, 0);

    view.set_callback({
        let me = me.clone();
        let mut panes = panes.clone();
        move |v| {
            panes.set_margins(0, if v.value() == 1 { 20 } else { 0 }, 0, 0);
            me.borrow_mut().rebuild();
        }
    });
    clear.set_callback({
        let me = me.clone();
        move |_| {
            let m = me.borrow();
            m.main.clear();
            for p in &m.side {
                p.clear();
            }
        }
    });

    let target = {
        let to = to.clone();
        let plats = me.borrow().plats.clone();
        move || match plats.borrow().get((to.value() - 1).max(-1) as usize) {
            Some(i) if to.value() > 0 => PLATFORMS[*i],
            _ => "all",
        }
    };
    let do_send = {
        let cmds = cmds.clone();
        let input = input.clone();
        let who = who.clone();
        let target = target.clone();
        move || {
            let text = input.value().trim().to_string();
            if text.is_empty() {
                return;
            }
            let (p, bot) = (target(), who.value() == 0);
            tell(&cmds, move |e| drop(e.send_chat(p, &text, bot)));
            input.clone().set_value("");
        }
    };
    send.set_callback({
        let s = do_send.clone();
        move |_| s()
    });
    input.handle({
        let s = do_send;
        move |_, ev| {
            if ev == Event::KeyDown && app::event_key() == Key::Enter {
                s();
                return true;
            }
            false
        }
    });
    pick.set_callback({
        let cmds = cmds.clone();
        let mut input = input.clone();
        move |_| {
            if let Some(word) = picker(&cmds, target()) {
                let mut v = input.value();
                if !v.is_empty() && !v.ends_with(' ') {
                    v.push(' ');
                }
                v.push_str(&word);
                v.push(' ');
                input.set_value(&v);
                let _ = input.take_focus();
            }
        }
    });
    me
}

/// The emote picker: emotes seen in chat and the channel's third-party ones (for the platform sent
/// to), and every emoji by group. What was picked.
fn picker(cmds: &Cmds, p: &'static str) -> Option<String> {
    // Sending to all: the emotes seen anywhere (each platform gets its own format).
    let seen: Vec<(String, String)> = query(cmds, move |e| {
        let mut v: Vec<(String, String)> = Vec::new();
        for x in PLATFORMS.iter().filter(|x| p == "all" || **x == p) {
            for e in e.chat_emotes(x) {
                if !v.iter().any(|y| y.0 == e.0) {
                    v.push(e);
                }
            }
        }
        v
    })
    .unwrap_or_default();
    let mut groups: Vec<(String, Vec<(String, String)>)> = Vec::new();
    if !seen.is_empty() {
        groups.push(("Seen in chat".into(), seen));
    }
    for (name, plat) in [("Twitch: BTTV, FFZ, 7TV", "twitch"), ("Kick: 7TV", "kick")] {
        if p == "all" || p == plat {
            let all = emotes::third_all(plat);
            if !all.is_empty() {
                groups.push((name.into(), all));
            }
        }
    }
    for (g, es) in emotes::groups() {
        groups.push((
            g.into(),
            es.into_iter().map(|e| (e.to_string(), emotes::twemoji(e))).collect(),
        ));
    }

    let mut win = Window::default().with_size(520, 420).with_label("Emotes and emoji");
    win.set_xclass("streamerz");
    win.make_modal(true);
    let mut col = Flex::new(8, 8, 504, 404, None).column();
    col.set_pad(6);
    let mut which = Choice::default();
    for (g, _) in &groups {
        which.add_choice(&g.replace('&', "&&").replace('/', "\\/"));
    }
    which.set_value(0);
    col.fixed(&which, ROW);
    let mut scroll = Scroll::default();
    scroll.set_type(fltk::group::ScrollType::Vertical);
    scroll.end();
    col.end();
    win.end();
    let picked: Rc<RefCell<Option<String>>> = Rc::default();
    let done = Rc::new(Cell::new(false));
    let groups = Rc::new(groups);

    // One page of buttons; pictures fill in as they arrive.
    let fill = {
        let (cmds, picked, done, groups) = (cmds.clone(), picked.clone(), done.clone(), groups.clone());
        let scroll = scroll.clone();
        move |g: usize| {
            let mut scroll = scroll.clone();
            scroll.clear();
            scroll.begin();
            let (cell, per) = (40, ((scroll.w() - 20) / 40).max(1));
            for (i, (name, url)) in groups[g].1.iter().enumerate() {
                let (x, y) = (
                    scroll.x() + (i as i32 % per) * cell,
                    scroll.y() + (i as i32 / per) * cell,
                );
                let mut b = Button::new(x, y, cell - 2, cell - 2, None);
                b.set_frame(FrameType::FlatBox);
                b.set_tooltip(&name.replace('@', "@@"));
                match img::get(&cmds, url, EMOTE, false) {
                    Some(p) => b.set_image(Some(p.first())),
                    None => b.set_label(&name.replace('@', "@@").replace('&', "&&")),
                }
                let (picked, done, name) = (picked.clone(), done.clone(), name.clone());
                b.set_callback(move |_| {
                    *picked.borrow_mut() = Some(name.clone());
                    done.set(true);
                });
            }
            scroll.end();
            scroll.scroll_to(0, 0);
            scroll.redraw();
        }
    };
    fill(0);
    which.set_callback({
        let fill = fill.clone();
        move |c| fill(c.value().max(0) as usize)
    });
    win.set_callback({
        let done = done.clone();
        move |_| done.set(true)
    });
    win.show();
    // Refill when pictures came in (checked twice a second), until something's picked.
    let arrived = || img::ARRIVED.load(std::sync::atomic::Ordering::Relaxed);
    let mut seen = arrived();
    while win.shown() && !done.get() {
        let _ = app::wait_for(0.5);
        if arrived() != seen {
            seen = arrived();
            let y = scroll.yposition();
            fill(which.value().max(0) as usize);
            scroll.scroll_to(0, y);
        }
    }
    win.hide();
    app::delete_widget(win);
    picked.take()
}

/// The platforms set up (logged in, or a TikTok channel), as indexes in PLATFORMS.
fn set_up(c: &Config) -> Vec<usize> {
    let on = [
        c.twitch.broadcaster.is_some(),
        c.youtube.broadcaster.is_some() || !c.youtube.channel_id.is_empty(),
        c.kick.broadcaster.is_some() || !c.kick.channel.is_empty(),
        c.tiktok.broadcaster.is_some() || !c.tiktok.channel.is_empty(),
    ];
    (0..4).filter(|i| on[*i]).collect()
}

/// Label and value rows.
type Facts = Vec<(String, String)>;

/// A chatter's card: what chat said about them this session, then what their platform says (asked
/// when it opens). Double click a row to copy its value.
fn card(cmds: &Cmds, l: &Line) {
    let (p, id) = (l.platform.clone(), l.user_id.clone());
    let seen = query(cmds, move |e| e.chatter(&p, &id)).flatten().unwrap_or_default();
    let platform = NAMES[PLATFORMS.iter().position(|x| *x == l.platform).unwrap_or(0)];
    let mut win = Window::default()
        .with_size(460, 480)
        .with_label(&format!("{} on {platform}", l.user));
    win.set_xclass("streamerz");
    let mut col = Flex::new(10, 10, 440, 460, None).column();
    col.set_pad(8);
    let mut head = Flex::default().row();
    let mut pic = Frame::default();
    head.fixed(&pic, 64);
    let mut name = Frame::default()
        .with_label(&format!("  {}", l.user.replace('@', "@@")))
        .with_align(Align::Left | Align::Inside | Align::ImageNextToText);
    name.set_label_font(Font::HelveticaBold);
    name.set_label_size(20);
    name.set_label_color(name_color(l));
    name.set_image(img::logo(platform, 20));
    head.end();
    col.fixed(&head, 64);
    let mut rows = HoldBrowser::default();
    rows.set_column_widths(&[150]);
    rows.set_column_char('\t');
    rows.set_format_char('\u{1}');
    let mut bottom = Flex::default().row();
    Frame::default();
    let mut open = button(&mut bottom, "Open profile", 110);
    let mut close = button(&mut bottom, "Close", 80);
    bottom.end();
    col.fixed(&bottom, ROW);
    col.end();
    win.end();
    win.resizable(&rows);

    let add = |rows: &mut HoldBrowser, k: &str, v: &str| {
        rows.add(&format!("{k}\t{}", v.replace(['\n', '\t'], " ")));
    };
    let roles: Vec<&str> = [
        (l.broadcaster, "broadcaster"),
        (l.moderator, "moderator"),
        (l.vip, "VIP"),
        (l.subscriber, "subscriber"),
    ]
    .iter()
    .filter(|r| r.0)
    .map(|r| r.1)
    .collect();
    add(
        &mut rows,
        "Roles",
        &if roles.is_empty() {
            "viewer".into()
        } else {
            roles.join(", ")
        },
    );
    add(&mut rows, "Messages this session", &seen.messages.to_string());
    add(&mut rows, "First message at", &seen.first);
    if !seen.args.contains_key("userId") {
        add(&mut rows, "userId", &l.user_id);
    }
    let s = |v: &serde_json::Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
    // As the actions get them (%userName%...); "user" is the name above.
    for (k, v) in seen.args.iter().filter(|a| a.0 != "user") {
        add(&mut rows, k, &s(v));
    }
    let asks = matches!(l.platform.as_str(), "twitch" | "kick");
    if asks {
        add(&mut rows, &format!("{platform} says"), "looking up...");
    }
    rows.set_callback(|b| {
        if app::event_clicks() {
            if let Some(t) = b.selected_text() {
                app::copy(t.split_once('\t').map_or(t.as_str(), |x| x.1));
                crate::say("Copied".into());
            }
        }
    });
    let login = seen.args.get("userName").map(s).unwrap_or_else(|| l.user.clone());
    let url: Rc<RefCell<String>> = Rc::new(RefCell::new(match l.platform.as_str() {
        "twitch" => format!("https://twitch.tv/{}", login.to_lowercase()),
        "youtube" => format!("https://youtube.com/channel/{}", l.user_id),
        "kick" => format!("https://kick.com/{}", login.to_lowercase()),
        _ => format!("https://tiktok.com/@{login}"),
    }));
    open.set_callback({
        let url = url.clone();
        move |_| crate::oauth::open_browser(&url.borrow())
    });
    close.set_callback({
        let mut win = win.clone();
        move |_| win.hide()
    });
    win.show();

    // The platform's answer and the profile picture fill in as they come (checked 5 times a second,
    // for 15 s at most, while the card is open).
    let picture = seen
        .args
        .iter()
        .find(|(k, v)| k.contains("rofile") && s(v).starts_with("http"))
        .map(|(_, v)| s(v))
        .unwrap_or_default();
    let picture = Rc::new(RefCell::new(picture));
    let answer: Arc<std::sync::Mutex<Option<Facts>>> = Arc::default();
    if asks {
        let (p, id, answer) = (l.platform.clone(), l.user_id.clone(), answer.clone());
        tell(cmds, move |e| {
            let e = e.clone();
            tokio::task::spawn_local(async move {
                let r = e.chatter_lookup(&p, &id).await;
                if let Ok(mut a) = answer.lock() {
                    *a = Some(r);
                }
            });
        });
    }
    let (cmds, tries) = (cmds.clone(), Rc::new(Cell::new(0)));
    let mut asked = asks;
    app::add_timeout3(0.0, move |h| {
        if !win.shown() {
            return;
        }
        tries.set(tries.get() + 1);
        if asked {
            if let Some(found) = answer.lock().ok().and_then(|mut a| a.take()) {
                asked = false;
                rows.remove(rows.size());
                if found.is_empty() {
                    add(&mut rows, &format!("{platform} says"), "nothing (not connected?)");
                }
                for (k, v) in found {
                    match k.as_str() {
                        "Picture" => *picture.borrow_mut() = v,
                        "Channel" => *url.borrow_mut() = format!("https://kick.com/{v}"),
                        _ => add(&mut rows, &k, &v),
                    }
                }
                rows.redraw();
            }
        }
        let mut waiting = asked;
        if pic.image().is_none() && !picture.borrow().is_empty() {
            match img::get(&cmds, &picture.borrow(), 64, true) {
                Some(p) => {
                    pic.set_image(Some(p.first()));
                    pic.redraw();
                }
                None => waiting = true,
            }
        }
        if waiting && tries.get() < 75 {
            app::repeat_timeout3(0.2, h);
        }
    });
}
