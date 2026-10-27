//! A list drawn by hand: roomy rows, a highlight under the mouse and a thin scrollbar.
//! Like FLTK's browsers, lines count from 1 (0 is none) and columns are split by tabs; the text is
//! drawn as is (no `@` formatting).

use fltk::enums::{Align, Color, Event, Font, FrameType, Key};
use fltk::frame::Frame;
use fltk::prelude::*;
use fltk::{app, draw};
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

const LINE: i32 = 28;
/// The scrollbar's width, and the space kept for it.
const BAR: i32 = 6;

type Cb = Box<dyn FnMut(&mut Rows)>;
type Keys = Box<dyn FnMut(&mut Rows, Key) -> bool>;

#[derive(Default)]
struct State {
    lines: Vec<String>,
    widths: &'static [i32],
    empty: &'static str,
    sel: i32,
    hover: i32,
    /// Pixels scrolled.
    top: i32,
    /// Where the scrollbar was grabbed, from its top.
    grab: Option<i32>,
}

#[derive(Clone)]
pub struct Rows {
    area: Frame,
    st: Rc<RefCell<State>>,
    cb: Rc<RefCell<Option<Cb>>>,
    keys: Rc<RefCell<Option<Keys>>>,
}

impl Deref for Rows {
    type Target = Frame;
    fn deref(&self) -> &Frame {
        &self.area
    }
}

impl DerefMut for Rows {
    fn deref_mut(&mut self) -> &mut Frame {
        &mut self.area
    }
}

impl Rows {
    /// `empty` is what it says in the middle while it has no lines.
    pub fn new(empty: &'static str) -> Rows {
        let mut area = Frame::default();
        area.set_frame(FrameType::DownBox);
        area.set_color(Color::Background2);
        let r = Rows {
            area: area.clone(),
            st: Rc::new(RefCell::new(State {
                empty,
                ..State::default()
            })),
            cb: Rc::default(),
            keys: Rc::default(),
        };
        area.draw({
            let r = r.clone();
            move |f| r.draw(f)
        });
        area.handle({
            let mut r = r.clone();
            move |f, ev| r.handle(f, ev)
        });
        r
    }

    pub fn set_column_widths(&mut self, w: &'static [i32]) {
        self.st.borrow_mut().widths = w;
    }

    /// Runs on a click (`app::event_clicks()` tells a double click) and when the keys move the selection.
    pub fn set_callback(&mut self, cb: impl FnMut(&mut Rows) + 'static) {
        *self.cb.borrow_mut() = Some(Box::new(cb));
    }

    /// The keys it doesn't use itself (all but the arrows, Page Up/Down, Home and End); true if used.
    pub fn set_keys(&mut self, f: impl FnMut(&mut Rows, Key) -> bool + 'static) {
        *self.keys.borrow_mut() = Some(Box::new(f));
    }

    pub fn add(&mut self, line: &str) {
        self.st.borrow_mut().lines.push(line.to_string());
        self.area.redraw();
    }

    pub fn clear(&mut self) {
        let mut st = self.st.borrow_mut();
        st.lines.clear();
        (st.sel, st.hover) = (0, 0);
        drop(st);
        self.area.redraw();
    }

    pub fn remove(&mut self, n: i32) {
        let mut st = self.st.borrow_mut();
        if n < 1 || n as usize > st.lines.len() {
            return;
        }
        st.lines.remove(n as usize - 1);
        if st.sel == n {
            st.sel = 0;
        } else if st.sel > n {
            st.sel -= 1;
        }
        drop(st);
        self.area.redraw();
    }

    pub fn size(&self) -> i32 {
        self.st.borrow().lines.len() as i32
    }

    pub fn value(&self) -> i32 {
        self.st.borrow().sel
    }

    pub fn text(&self, n: i32) -> Option<String> {
        let i = usize::try_from(n).ok()?.checked_sub(1)?;
        self.st.borrow().lines.get(i).cloned()
    }

    pub fn selected_text(&self) -> Option<String> {
        self.text(self.value())
    }

    /// Selects line `n` and scrolls to it (without running the callback).
    pub fn select(&mut self, n: i32) {
        let mut st = self.st.borrow_mut();
        if n < 1 || n as usize > st.lines.len() {
            return;
        }
        st.sel = n;
        let h = self.area.h() - 4;
        let y = (n - 1) * LINE;
        // Not laid out yet (h 0): its top only.
        st.top = st.top.clamp((y + LINE - h).clamp(0, y), y);
        drop(st);
        self.area.redraw();
    }

    fn fire(&mut self) {
        // Out while it runs: it may use this list.
        let cb = self.cb.borrow_mut().take();
        if let Some(mut cb) = cb {
            cb(self);
            self.cb.borrow_mut().get_or_insert(cb);
        }
    }

    /// How far it scrolls.
    fn max(&self, st: &State) -> i32 {
        (st.lines.len() as i32 * LINE - (self.area.h() - 4)).max(0)
    }

    /// The scrollbar's top and height, when it's needed.
    fn thumb(&self, st: &State) -> Option<(i32, i32)> {
        let max = self.max(st);
        if max == 0 {
            return None;
        }
        let h = self.area.h() - 8;
        let th = (h * h / (h + max)).max(24).min(h);
        Some((self.area.y() + 4 + (h - th) * st.top.clamp(0, max) / max, th))
    }

    fn draw(&self, f: &Frame) {
        let mut st = self.st.borrow_mut();
        st.top = st.top.clamp(0, self.max(&st));
        let (x, y, w, h) = (f.x(), f.y(), f.w(), f.h());
        draw::set_font(Font::Helvetica, 14);
        if st.lines.is_empty() {
            draw::set_draw_color(super::muted());
            draw::draw_text2(st.empty, x + 8, y, w - 16, h, Align::Center | Align::Wrap);
            return;
        }
        let thumb = self.thumb(&st);
        let rw = w - 8 - if thumb.is_some() { BAR + 4 } else { 0 };
        draw::push_clip(x + 2, y + 2, w - 4, h - 4);
        let fg = Color::Foreground;
        let first = st.top / LINE;
        for (i, l) in st.lines.iter().enumerate().skip(first as usize) {
            let n = i as i32 + 1;
            let ly = y + 2 + i as i32 * LINE - st.top;
            if ly > y + h {
                break;
            }
            let text = if n == st.sel {
                draw::set_draw_color(Color::Selection);
                draw::draw_rounded_rectf(x + 4, ly + 1, rw, LINE - 2, 5);
                Color::contrast(fg, Color::Selection)
            } else {
                if n == st.hover {
                    draw::set_draw_color(Color::color_average(fg, f.color(), 0.08));
                    draw::draw_rounded_rectf(x + 4, ly + 1, rw, LINE - 2, 5);
                }
                fg
            };
            draw::set_draw_color(text);
            let base = ly + (LINE + draw::height()) / 2 - draw::descent();
            let mut cx = x + 12;
            let mut cols = l.split('\t').peekable();
            let mut c = 0;
            while let Some(col) = cols.next() {
                let cw = match (st.widths.get(c), cols.peek()) {
                    (Some(&cw), Some(_)) => cw,
                    _ => x + 4 + rw - cx,
                };
                draw::push_clip(cx, ly, cw - 8, LINE);
                draw::draw_text(col, cx, base);
                draw::pop_clip();
                (cx, c) = (cx + cw, c + 1);
            }
        }
        if let Some((ty, th)) = thumb {
            draw::set_draw_color(Color::color_average(
                fg,
                f.color(),
                if st.grab.is_some() { 0.5 } else { 0.3 },
            ));
            draw::draw_rounded_rectf(x + w - BAR - 4, ty, BAR, th, BAR / 2);
        }
        draw::pop_clip();
    }

    /// The line under the mouse, or 0.
    fn line_at(&self, st: &State) -> i32 {
        let n = (app::event_y() - self.area.y() - 2 + st.top) / LINE + 1;
        if n >= 1 && n as usize <= st.lines.len() {
            n
        } else {
            0
        }
    }

    fn handle(&mut self, f: &mut Frame, ev: Event) -> bool {
        match ev {
            Event::Enter | Event::Focus | Event::Unfocus => return true,
            // Before borrowing: it sends Focus here at once.
            Event::Push => {
                let _ = f.take_focus();
            }
            _ => {}
        }
        let mut st = self.st.borrow_mut();
        match ev {
            Event::Move => {
                let n = self.line_at(&st);
                if n != st.hover {
                    st.hover = n;
                    f.redraw();
                }
                true
            }
            Event::Leave => {
                st.hover = 0;
                f.redraw();
                true
            }
            Event::MouseWheel => {
                st.top = (st.top + app::event_dy_value() * LINE).clamp(0, self.max(&st));
                st.hover = self.line_at(&st);
                f.redraw();
                true
            }
            Event::Push => {
                if let Some((ty, th)) = self.thumb(&st).filter(|_| app::event_x() >= f.x() + f.w() - BAR - 8) {
                    let ey = app::event_y();
                    // Off the bar: jump there, then drag from its middle.
                    st.grab = Some(if (ty..ty + th).contains(&ey) { ey - ty } else { th / 2 });
                    drop(st);
                    self.drag(f);
                    return true;
                }
                let n = self.line_at(&st);
                if n > 0 {
                    st.sel = n;
                    drop(st);
                    f.redraw();
                    self.fire();
                }
                true
            }
            Event::Drag if st.grab.is_some() => {
                drop(st);
                self.drag(f);
                true
            }
            Event::Released => {
                if st.grab.take().is_some() {
                    f.redraw();
                }
                true
            }
            Event::KeyDown => {
                let len = st.lines.len() as i32;
                let page = ((f.h() - 4) / LINE).max(1);
                let n = match app::event_key() {
                    Key::Up => st.sel - 1,
                    Key::Down => st.sel + 1,
                    Key::PageUp => st.sel - page,
                    Key::PageDown => st.sel + page,
                    Key::Home => 1,
                    Key::End => len,
                    k => {
                        drop(st);
                        let f = self.keys.borrow_mut().take();
                        let Some(mut f) = f else { return false };
                        let used = f(self, k);
                        self.keys.borrow_mut().get_or_insert(f);
                        return used;
                    }
                }
                .clamp(1.min(len), len);
                if n != st.sel {
                    drop(st);
                    self.select(n);
                    self.fire();
                }
                true
            }
            _ => false,
        }
    }

    /// The scrollbar follows the mouse.
    fn drag(&self, f: &mut Frame) {
        let mut st = self.st.borrow_mut();
        let (Some(grab), Some((_, th)), max) = (st.grab, self.thumb(&st), self.max(&st)) else {
            return;
        };
        let room = (f.h() - 8 - th).max(1);
        st.top = ((app::event_y() - grab - f.y() - 4) * max / room).clamp(0, max);
        f.redraw();
    }
}
