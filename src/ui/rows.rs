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

/// How a column draws its cells (text unless said).
#[derive(Clone, Copy, PartialEq)]
pub enum Cell {
    /// `key|text`: a logo (or else a Lucide icon) named `key`, then the text.
    Lead,
    /// `icon|rrggbb|tip`: only a coloured icon; the tip shows when it is pointed at.
    Status,
    /// A path or address: quiet, cut with an ellipsis, underlined under the mouse (the click is the
    /// page's to use, see `column`).
    Link,
}

type Cb = Box<dyn FnMut(&mut Rows)>;
type Keys = Box<dyn FnMut(&mut Rows, Key) -> bool>;

#[derive(Default)]
struct State {
    lines: Vec<String>,
    /// Which lines are switched off: dimmed, with a power-off icon at the end.
    off: Vec<bool>,
    /// An icon at the start of a line.
    icons: Vec<Option<fltk::image::SharedImage>>,
    widths: &'static [i32],
    empty: &'static str,
    /// The column holding a platform: drawn as its logo, named on hover.
    logo: Option<usize>,
    /// The column holding a picture's address: the picture shows instead (once it's in `pics`).
    pic: Option<usize>,
    pics: std::collections::HashMap<String, fltk::image::RgbImage>,
    cells: Vec<Option<Cell>>,
    /// The column the last click landed in.
    col: Option<usize>,
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
    /// What a right click offers.
    menu: Rc<RefCell<Vec<fltk::button::Button>>>,
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

/// `text` cut to fit `w` px with an ellipsis (in the current font).
fn fit(text: &str, w: i32) -> String {
    if w <= 0 {
        return String::new();
    }
    if draw::width(text) as i32 <= w {
        return text.to_string();
    }
    let mut t: String = text.to_string();
    while !t.is_empty() {
        t.pop();
        let c = format!("{t}…");
        if draw::width(&c) as i32 <= w {
            return c;
        }
    }
    String::new()
}

fn tone(hex: &str) -> Color {
    let n = u32::from_str_radix(hex, 16).unwrap_or(0x999999);
    Color::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)
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
            menu: Rc::default(),
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

    /// Column `c` holds a platform ("twitch"...): its logo shows instead of the text.
    pub fn set_logo_column(&mut self, c: usize) {
        self.st.borrow_mut().logo = Some(c);
    }

    /// Column `c` holds a picture's address: the picture shows instead, once given with `set_pic`.
    pub fn set_pic_column(&mut self, c: usize) {
        self.st.borrow_mut().pic = Some(c);
    }

    /// How column `c` draws its cells.
    pub fn set_cell(&mut self, c: usize, kind: Cell) {
        let mut st = self.st.borrow_mut();
        if st.cells.len() <= c {
            st.cells.resize(c + 1, None);
        }
        st.cells[c] = Some(kind);
    }

    /// The column the last click landed in (none after the keys moved the selection).
    pub fn column(&self) -> Option<usize> {
        self.st.borrow().col
    }

    /// The picture for an address in the picture column.
    pub fn set_pic(&mut self, url: &str, img: fltk::image::RgbImage) {
        self.st.borrow_mut().pics.insert(url.to_string(), img);
        self.area.redraw();
    }

    /// Whether the picture for `url` is in.
    pub fn has_pic(&self, url: &str) -> bool {
        self.st.borrow().pics.contains_key(url)
    }

    /// Runs on a click (`app::event_clicks()` tells a double click) and when the keys move the selection.
    pub fn set_callback(&mut self, cb: impl FnMut(&mut Rows) + 'static) {
        *self.cb.borrow_mut() = Some(Box::new(cb));
    }

    /// The keys it doesn't use itself (all but the arrows, Page Up/Down, Home and End); true if used.
    pub fn set_keys(&mut self, f: impl FnMut(&mut Rows, Key) -> bool + 'static) {
        *self.keys.borrow_mut() = Some(Box::new(f));
    }

    /// A right click selects the line under it and offers these buttons' actions.
    pub fn set_menu(&mut self, buttons: &[fltk::button::Button]) {
        *self.menu.borrow_mut() = buttons.to_vec();
    }

    pub fn add(&mut self, line: &str) {
        self.add_state(line, false);
    }

    /// A line that is switched off: dimmed, with a power-off icon at the end.
    pub fn add_state(&mut self, line: &str, off: bool) {
        self.add_icon(line, off, None);
    }

    /// A line with an icon at its start, and perhaps switched off.
    pub fn add_icon(&mut self, line: &str, off: bool, icon: Option<fltk::image::SharedImage>) {
        let mut st = self.st.borrow_mut();
        st.lines.push(line.to_string());
        st.off.push(off);
        st.icons.push(icon);
        drop(st);
        self.area.redraw();
    }

    pub fn clear(&mut self) {
        let mut st = self.st.borrow_mut();
        st.lines.clear();
        st.off.clear();
        st.icons.clear();
        (st.sel, st.hover) = (0, 0);
        st.col = None;
        drop(st);
        self.area.redraw();
    }

    /// Drop backing storage when its page is hidden, rather than retaining its high-water capacity.
    pub fn release(&mut self) {
        self.clear();
        let mut st = self.st.borrow_mut();
        st.lines = Vec::new();
        st.off = Vec::new();
        st.icons = Vec::new();
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
            let (text, back) = if n == st.sel {
                draw::set_draw_color(Color::Selection);
                draw::draw_rounded_rectf(x + 4, ly + 1, rw, LINE - 2, 5);
                (Color::contrast(fg, Color::Selection), Color::Selection)
            } else {
                if n == st.hover {
                    draw::set_draw_color(Color::color_average(fg, f.color(), 0.08));
                    draw::draw_rounded_rectf(x + 4, ly + 1, rw, LINE - 2, 5);
                }
                (fg, f.color())
            };
            let off = st.off.get(i).copied().unwrap_or(false);
            // Off: the text fades into the row, whatever its colour.
            let text = if off {
                Color::color_average(text, back, 0.6)
            } else {
                text
            };
            if off {
                if let Some(mut ic) = super::icons::icon("power-off", text, 14) {
                    ic.draw(x + 4 + rw - 22, ly + (LINE - 14) / 2, 14, 14);
                }
            }
            draw::set_draw_color(text);
            let base = ly + (LINE + draw::height()) / 2 - draw::descent();
            let mut cx = x + 12;
            if let Some(mut ic) = st.icons.get(i).cloned().flatten() {
                ic.draw(cx, ly + (LINE - 16) / 2, 16, 16);
                cx += 24;
            }
            let mut cols = l.split('\t').peekable();
            let mut c = 0;
            while let Some(col) = cols.next() {
                let cw = match (st.widths.get(c), cols.peek()) {
                    (Some(&cw), Some(_)) => cw,
                    _ => x + 4 + rw - cx - if off { 24 } else { 0 },
                };
                let kind = st.cells.get(c).copied().flatten();
                if let Some(kind) = kind {
                    let hovered = n == st.hover;
                    match kind {
                        Cell::Lead => {
                            let (key, label) = col.split_once('|').unwrap_or(("", col));
                            let mut at = cx;
                            if let Some(mut i) = super::img::logo(key, 16) {
                                i.draw(at, ly + (LINE - 16) / 2, 16, 16);
                                at += 24;
                            } else if let Some(mut i) = super::icons::icon(key, text, 16) {
                                i.draw(at, ly + (LINE - 16) / 2, 16, 16);
                                at += 24;
                            }
                            draw::push_clip(at, ly, (cx + cw - 8 - at).max(0), LINE);
                            draw::draw_text(label, at, base);
                            draw::pop_clip();
                        }
                        Cell::Status => {
                            let mut p = col.splitn(3, '|');
                            let (icon, color) = (p.next().unwrap_or(""), tone(p.next().unwrap_or("")));
                            if let Some(mut i) = super::icons::icon(icon, color, 16) {
                                i.draw(cx, ly + (LINE - 16) / 2, 16, 16);
                            }
                        }
                        Cell::Link => {
                            let shown = fit(col, cw - 12);
                            let ink = if n == st.sel {
                                text
                            } else {
                                Color::color_average(text, back, 0.65)
                            };
                            draw::set_draw_color(ink);
                            draw::draw_text(&shown, cx, base);
                            if hovered {
                                let w = draw::width(&shown) as i32;
                                draw::draw_line(cx, base + 2, cx + w, base + 2);
                            }
                            draw::set_draw_color(text);
                        }
                    }
                    (cx, c) = (cx + cw, c + 1);
                    continue;
                }
                match super::img::logo(super::img::platform_name(col), 16).filter(|_| st.logo == Some(c)) {
                    Some(mut i) => i.draw(cx, ly + (LINE - 16) / 2, 16, 16),
                    None if st.pic == Some(c) => {
                        if let Some(p) = st.pics.get(col) {
                            let mut p = p.clone();
                            p.draw(cx, ly + (LINE - p.h()) / 2, p.w(), p.h());
                        }
                    }
                    None => {
                        draw::push_clip(cx, ly, cw - 8, LINE);
                        draw::draw_text(col, cx, base);
                        draw::pop_clip();
                    }
                }
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

    /// The column under the mouse (the last one for all that is past the widths).
    fn column_at(&self, st: &State, f: &Frame) -> usize {
        let mut x = f.x() + 12;
        for (i, w) in st.widths.iter().enumerate() {
            x += w;
            if app::event_x() < x {
                return i;
            }
        }
        st.widths.len()
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
                // The logo's platform, by name.
                let tip = st.logo.and_then(|c| {
                    let x0: i32 = f.x() + 12 + st.widths.iter().take(c).sum::<i32>();
                    let on = (x0..x0 + 20).contains(&app::event_x());
                    let line = st.lines.get(usize::try_from(n - 1).ok()?)?;
                    on.then(|| super::img::platform_name(line.split('\t').nth(c).unwrap_or("")).to_string())
                });
                let tip = tip.or_else(|| {
                    let line = st.lines.get(usize::try_from(n - 1).ok()?)?;
                    let c = self.column_at(&st, f);
                    let cell = line.split('\t').nth(c)?;
                    match st.cells.get(c).copied().flatten()? {
                        Cell::Status => cell.splitn(3, '|').nth(2).map(str::to_string),
                        Cell::Link => Some(cell.to_string()),
                        Cell::Lead => None,
                    }
                });
                let off = st.off.get(usize::try_from(n - 1).unwrap_or(usize::MAX)) == Some(&true)
                    && app::event_x() >= f.x() + f.w() - 36 - if self.thumb(&st).is_some() { BAR + 4 } else { 0 };
                let tip = if off {
                    Some(crate::i18n::tr("lists.disabled").to_string())
                } else {
                    tip
                };
                f.set_tooltip(tip.as_deref().unwrap_or(""));
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
                st.col = (n > 0).then(|| self.column_at(&st, f));
                if n > 0 {
                    st.sel = n;
                }
                // Not held over what follows: the menu going away moves the focus, which comes back here.
                drop(st);
                if n > 0 {
                    f.redraw();
                    self.fire();
                }
                if super::right_button() {
                    let menu = self.menu.borrow().clone();
                    super::context_menu(&menu);
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
                st.col = None;
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

#[cfg(test)]
mod tests {
    #[test]
    fn link_text_does_not_overflow_a_zero_width_column() {
        assert_eq!(super::fit("a long address", 0), "");
        assert_eq!(super::fit("a long address", -1), "");
    }
}
