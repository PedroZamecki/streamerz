//! The Chat tab: every platform's chat together or side by side (or one platform), and one box to
//! send to a platform or all of them, as the bot or the broadcaster, with an emote picker.

use super::{button, query, tell, ROW};
use crate::engine::chatlog::{Line, PLATFORMS};
use fltk::button::Button;
use fltk::enums::{Color, Event, Font, Key};
use fltk::frame::Frame;
use fltk::group::{Flex, Tabs};
use fltk::input::Input;
use fltk::menu::{Choice, MenuButton};
use fltk::prelude::*;
use fltk::text::{StyleTableEntry, TextBuffer, TextDisplay, WrapMode};
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedSender;

type Cmds = UnboundedSender<crate::engine::UiCmd>;

const NAMES: [&str; 4] = ["Twitch", "YouTube", "Kick", "TikTok"];
const TAGS: [&str; 4] = ["TW", "YT", "KI", "TT"];
/// Views: 0 all together, 1 side by side, 2.. one platform.
const VIEWS: &str = "All together|Side by side|Twitch|YouTube|Kick|TikTok";
/// Unicode emoji offered in the picker, which every platform takes as is.
const EMOJI: &[&str] = &[
    "😀", "😂", "🤣", "😍", "😎", "🤔", "😭", "😡", "👍", "👏", "🙏", "🔥", "❤️", "💯", "🎉", "👀", "GG",
];

/// Style letters: A text, B-E platform tags, F user, G emote, H streamerz's own notes.
fn styles() -> Vec<StyleTableEntry> {
    let e = |color: Color, font: Font| StyleTableEntry { color, font, size: 14 };
    vec![
        e(Color::Foreground, Font::Helvetica),
        e(Color::from_rgb(0xa9, 0x70, 0xff), Font::HelveticaBold),
        e(Color::from_rgb(0xff, 0x4e, 0x45), Font::HelveticaBold),
        e(Color::from_rgb(0x53, 0xfc, 0x18), Font::HelveticaBold),
        e(Color::from_rgb(0x25, 0xf4, 0xee), Font::HelveticaBold),
        e(Color::from_rgb(0xe0, 0xe0, 0xe0), Font::HelveticaBold),
        e(Color::from_rgb(0xff, 0xc8, 0x3d), Font::HelveticaBoldItalic),
        e(Color::from_rgb(0x99, 0x99, 0x99), Font::HelveticaItalic),
    ]
}

struct Pane {
    view: TextDisplay,
    text: TextBuffer,
    style: TextBuffer,
}

impl Pane {
    fn new() -> Pane {
        let mut view = TextDisplay::default();
        let (text, style) = (TextBuffer::default(), TextBuffer::default());
        view.set_buffer(text.clone());
        view.set_highlight_data(style.clone(), styles());
        view.wrap_mode(WrapMode::AtBounds, 0);
        Pane { view, text, style }
    }

    /// Add a line (with its platform tag when `tag`), styled, and keep the view at the bottom.
    fn add(&mut self, l: &Line, tag: bool) {
        let p = PLATFORMS.iter().position(|x| *x == l.platform).unwrap_or(0);
        let (mut t, mut s) = (String::new(), String::new());
        let mut put = |x: &str, c: char| {
            t.push_str(x);
            s.extend(std::iter::repeat_n(c, x.len()));
        };
        if self.text.length() > 0 {
            put("\n", 'A');
        }
        put(&format!("{} ", l.time), 'H');
        if tag {
            put(&format!("[{}] ", TAGS[p]), (b'B' + p as u8) as char);
        }
        if l.note {
            put(&l.text, 'H');
        } else {
            put(&l.user, 'F');
            put(": ", 'A');
            // Emotes highlighted, word by word (emote names have no spaces).
            for (i, w) in l.text.split(' ').enumerate() {
                if i > 0 {
                    put(" ", 'A');
                }
                put(w, if l.emotes.iter().any(|e| e == w) { 'G' } else { 'A' });
            }
        }
        self.text.append(&t);
        self.style.append(&s);
        // Trim the oldest lines past what the engine keeps.
        if self.text.count_lines(0, self.text.length()) > 500 {
            let cut = self.text.line_end(0) + 1;
            self.text.remove(0, cut);
            self.style.remove(0, cut);
        }
        let n = self.text.count_lines(0, self.text.length());
        self.view.scroll(n, 0);
    }

    fn clear(&mut self) {
        self.text.set_text("");
        self.style.set_text("");
    }
}

pub struct Chat {
    pub tab: Flex,
    cmds: Cmds,
    view: Choice,
    /// All together / one platform, then one per platform for side by side.
    main: Pane,
    side: Vec<Pane>,
    seen: u64,
}

impl Chat {
    /// Add the lines that came since the last refresh (all of them after a view change).
    pub fn refresh(&mut self) {
        let seen = self.seen;
        let lines = query(&self.cmds, move |e| e.chat_since(seen)).unwrap_or_default();
        let v = self.view.value();
        for l in &lines {
            match v {
                1 => {
                    let p = PLATFORMS.iter().position(|x| *x == l.platform).unwrap_or(0);
                    self.side[p].add(l, false);
                }
                0 => self.main.add(l, true),
                _ if PLATFORMS[(v - 2) as usize] == l.platform => self.main.add(l, false),
                _ => {}
            }
            self.seen = l.seq;
        }
    }

    fn rebuild(&mut self) {
        self.main.clear();
        for p in &mut self.side {
            p.clear();
        }
        let side = self.view.value() == 1;
        if side {
            self.main.view.hide();
            for p in &mut self.side {
                p.view.show();
            }
        } else {
            self.main.view.show();
            for p in &mut self.side {
                p.view.hide();
            }
        }
        self.seen = 0;
        self.refresh();
        self.tab.layout();
        self.tab.redraw();
    }
}

pub fn tab(tabs: &Tabs, cmds: &Cmds) -> Rc<RefCell<Chat>> {
    let mut col = super::tab(tabs, "Chat");
    let mut top = Flex::default().row();
    let l = Frame::default().with_label("Show:");
    top.fixed(&l, 50);
    let mut view = Choice::default();
    view.add_choice(VIEWS);
    view.set_value(0);
    top.fixed(&view, 140);
    Frame::default();
    let mut clear = button(&mut top, "Clear", 70);
    top.end();
    col.fixed(&top, ROW);

    let mut panes = Flex::default().row();
    panes.set_pad(4);
    let main = Pane::new();
    let mut side = Vec::new();
    for _ in PLATFORMS {
        let mut p = Pane::new();
        p.view.hide();
        side.push(p);
    }
    panes.end();
    for (p, name) in side.iter_mut().zip(NAMES) {
        p.view.set_label(name);
        p.view.set_align(fltk::enums::Align::Top);
    }

    let mut bottom = Flex::default().row();
    let l = Frame::default().with_label("To:");
    bottom.fixed(&l, 30);
    let mut to = Choice::default();
    to.add_choice("All connected|Twitch|YouTube|Kick|TikTok");
    to.set_value(0);
    bottom.fixed(&to, 120);
    let mut who = Choice::default();
    who.add_choice("as bot|as broadcaster");
    who.set_value(0);
    who.set_tooltip("As the bot (or the broadcaster when no bot is logged in), or as the broadcaster");
    bottom.fixed(&who, 120);
    let mut input = Input::default();
    let mut emotes = button(&mut bottom, "Emotes", 70);
    emotes.set_tooltip("Emoji, and the emotes seen in chat on the platform you send to");
    let mut send = button(&mut bottom, "Send", 70);
    bottom.end();
    col.fixed(&bottom, ROW);
    col.end();

    let me = Rc::new(RefCell::new(Chat {
        tab: col,
        cmds: cmds.clone(),
        view: view.clone(),
        main,
        side,
        seen: 0,
    }));
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
            let mut m = me.borrow_mut();
            m.main.clear();
            for p in &mut m.side {
                p.clear();
            }
        }
    });

    let target = {
        let to = to.clone();
        move || match to.value() {
            1..=4 => PLATFORMS[(to.value() - 1) as usize],
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
            if ev == Event::KeyDown && fltk::app::event_key() == Key::Enter {
                s();
                return true;
            }
            false
        }
    });
    emotes.set_callback({
        let cmds = cmds.clone();
        let mut input = input.clone();
        move |b: &mut Button| {
            let p = target();
            // Sending to all: the emotes seen anywhere (each platform gets its own format).
            let seen: Vec<String> = query(&cmds, move |e| {
                let mut v = Vec::new();
                for x in PLATFORMS.iter().filter(|x| p == "all" || **x == p) {
                    for name in e.chat_emotes(x) {
                        if !v.contains(&name) {
                            v.push(name);
                        }
                    }
                }
                v
            })
            .unwrap_or_default();
            let words: Vec<String> = EMOJI
                .iter()
                .map(|s| s.to_string())
                .chain(seen.into_iter().take(60))
                .collect();
            // Menu labels: '/', '&', '_' and '@' are special.
            let labels: Vec<String> = words
                .iter()
                .map(|w| {
                    w.replace('&', "&&")
                        .replace('/', "\\/")
                        .replace('_', "\\_")
                        .replace('@', "@@")
                })
                .collect();
            // A throwaway menu button: popup() shows it at the pointer.
            let mut menu = MenuButton::new(b.x(), b.y() + b.h(), 1, 1, None);
            for l in &labels {
                menu.add_choice(&l.replace('|', "\\|"));
            }
            let picked = menu.popup().map(|_| menu.value());
            fltk::app::delete_widget(menu);
            if let Some(at) = picked.filter(|&i| i >= 0) {
                let mut v = input.value();
                if !v.is_empty() && !v.ends_with(' ') {
                    v.push(' ');
                }
                v.push_str(&words[at as usize]);
                v.push(' ');
                input.set_value(&v);
                let _ = input.take_focus();
            }
        }
    });
    me
}
