//! The list of commands that opens above a chat box while a `/` is typed in Twitch's chat (as twitch.tv's
//! own does): each with what it takes and does, picked with the arrows and Tab or a click.

use crate::i18n::tr;
use crate::slash::{suggestions, Suggestion};
use fltk::app;
use fltk::draw;
use fltk::enums::{Align, Color, Event, Font, FrameType, Key};
use fltk::input::Input;
use fltk::prelude::*;
use fltk::window::Window;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const HEAD: i32 = 28;
const ROW: i32 = 46;
const MOST: usize = 7;

/// A chat box's list. Cloned into its handlers.
#[derive(Clone, Default)]
pub struct Suggest {
    win: Rc<RefCell<Option<Window>>>,
    rows: Rc<RefCell<Vec<Suggestion>>>,
    at: Rc<Cell<usize>>,
}

impl Suggest {
    pub fn is_open(&self) -> bool {
        self.win.borrow().is_some()
    }

    pub fn hide(&self) {
        if let Some(w) = self.win.borrow_mut().take() {
            app::delete_widget(w);
            app::redraw();
        }
        self.rows.borrow_mut().clear();
    }

    /// Shows what `input` holds fits (or takes the list away when nothing does), for our role `me`.
    pub fn update(&self, input: &Input, me: crate::roles::Role) {
        let mut rows = suggestions(&input.value(), me);
        rows.truncate(MOST);
        if rows.is_empty() {
            return self.hide();
        }
        // Keep the pick when the list is the same one narrowed down.
        let keep = self.rows.borrow().get(self.at.get()).map(|r| r.fill.clone());
        self.at
            .set(keep.and_then(|k| rows.iter().position(|r| r.fill == k)).unwrap_or(0));
        *self.rows.borrow_mut() = rows;
        self.show(input);
    }

    fn show(&self, input: &Input) {
        // The app's own window, and the box's place in it.
        let (mut x, mut y) = (input.x(), input.y());
        let mut cur = input.window();
        let mut top = None;
        while let Some(w) = cur {
            if w.window().is_none() {
                top = Some(w);
                break;
            }
            x += w.x();
            y += w.y();
            cur = w.window();
        }
        let Some(top) = top else { return };
        let n = self.rows.borrow().len() as i32;
        let (w, h) = (input.w().clamp(260, 560), HEAD + n * ROW + 6);
        let at = (x, (y - h - 6).max(4));
        self.hide_window();
        top.begin();
        let mut win = Window::new(at.0, at.1, w, h, None);
        win.set_frame(FrameType::FlatBox);
        win.set_color(Color::Background);
        let me = self.clone();
        win.draw(move |win| me.draw(win.w(), win.h()));
        let (me, input) = (self.clone(), input.clone());
        win.handle(move |win, ev| match ev {
            Event::Move => {
                let k = ((app::event_y() - HEAD).max(0) / ROW) as usize;
                if k < me.rows.borrow().len() && k != me.at.get() {
                    me.at.set(k);
                    win.redraw();
                }
                true
            }
            Event::Push => {
                let k = ((app::event_y() - HEAD).max(0) / ROW) as usize;
                if k < me.rows.borrow().len() {
                    me.at.set(k);
                    me.complete(&mut input.clone());
                }
                true
            }
            _ => false,
        });
        win.end();
        top.end();
        win.show();
        *self.win.borrow_mut() = Some(win);
        app::redraw();
    }

    fn hide_window(&self) {
        if let Some(w) = self.win.borrow_mut().take() {
            app::delete_widget(w);
        }
    }

    fn draw(&self, w: i32, h: i32) {
        let (fg, bg) = (Color::Foreground, Color::Background);
        draw::set_draw_color(Color::color_average(fg, bg, 0.2));
        draw::draw_rounded_rect(0, 0, w, h, 8);
        draw::set_font(Font::HelveticaBold, 11);
        draw::set_draw_color(super::muted());
        draw::draw_text2(&tr("slash.header").to_uppercase(), 14, 6, w - 28, 18, Align::Left);
        let rows = self.rows.borrow();
        for (i, r) in rows.iter().enumerate() {
            let y = HEAD + i as i32 * ROW;
            if i == self.at.get() {
                draw::set_draw_color(Color::color_average(fg, bg, 0.12));
                draw::draw_rounded_rectf(6, y, w - 12, ROW - 4, 6);
            }
            draw::set_font(Font::HelveticaBold, 14);
            draw::set_draw_color(fg);
            let tw = draw::width(&r.title) as i32;
            draw::draw_text2(&r.title, 14, y + 3, tw, 20, Align::Left);
            draw::set_font(Font::Helvetica, 13);
            draw::set_draw_color(super::muted());
            draw::draw_text2(r.args, 14 + tw + 6, y + 3, w - tw - 34, 20, Align::Left);
            draw::set_font(Font::Helvetica, 12);
            draw::draw_text2(tr(r.about), 14, y + 23, w - 28, 18, Align::Left);
        }
    }

    /// Puts the picked command in the box. True when it did.
    fn complete(&self, input: &mut Input) -> bool {
        let Some(r) = self.rows.borrow().get(self.at.get()).cloned() else {
            return false;
        };
        let text = if r.more { format!("{} ", r.fill) } else { r.fill };
        let same = input.value() == text;
        input.set_value(&text);
        let _ = input.set_position(text.len() as i32);
        let _ = input.take_focus();
        self.hide();
        same
    }

    /// A key in the box while the list is up: true when the list took it (the arrows, Tab, Escape, and
    /// Enter when it finishes a command instead of sending).
    pub fn key(&self, input: &mut Input, me: crate::roles::Role) -> bool {
        if !self.is_open() {
            return false;
        }
        let n = self.rows.borrow().len();
        let key = app::event_key();
        let done = |s: &Self, input: &mut Input| {
            s.complete(input);
            s.update(input, me);
            true
        };
        match key {
            Key::Down => {
                self.at.set((self.at.get() + 1) % n);
                self.redraw();
                true
            }
            Key::Up => {
                self.at.set((self.at.get() + n - 1) % n);
                self.redraw();
                true
            }
            Key::Tab => done(self, input),
            Key::Escape => {
                self.hide();
                true
            }
            // Enter on a command not typed in full finishes it; on a finished one it sends.
            Key::Enter => {
                let rows = self.rows.borrow();
                let typed = input.value();
                let sendable = rows.get(self.at.get()).is_some_and(|r| r.fill == typed && !r.more);
                drop(rows);
                if sendable {
                    self.hide();
                    false
                } else {
                    done(self, input)
                }
            }
            _ => false,
        }
    }

    fn redraw(&self) {
        if let Some(w) = self.win.borrow().as_ref() {
            w.clone().redraw();
        }
    }
}
