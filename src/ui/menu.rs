//! Every menu: right-click menus, dropdowns and the Options button. One look (a rounded panel, roomy
//! rows, Lucide icons, the lists' soft highlight) and one behaviour (mouse, arrows, Home/End, Enter,
//! Escape, first letters). FLTK's own popups are square and cramped, and are never used.
//!
//! - Build one with `Menu::new()`, `item`, `add` (maybe greyed), `tick`, `line`, then `show` (at the
//!   pointer, or at the focused control from the keyboard; `under` a widget for a dropdown).
//! - `dropdown` makes a Choice or a MenuButton open its items in this menu instead.

use super::popover::{Popover, RADIUS};
use fltk::enums::{Align, Color, Event, Font, Key};
use fltk::frame::Frame;
use fltk::prelude::*;
use fltk::{app, draw};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The room around the rows, inside the panel.
const PAD: i32 = 5;
/// A row's height.
const ROW: i32 = 30;
/// A dividing line's height.
const LINE: i32 = 9;
const ICON: i32 = 16;
const FONT: i32 = 14;

type Run = Box<dyn FnOnce()>;

struct Item {
    label: String,
    icon: Option<&'static str>,
    enabled: bool,
    /// A tick in front: shown when true, room kept when false.
    tick: Option<bool>,
    run: Option<Run>,
}

enum Entry {
    Item(Item),
    Line,
}

#[derive(Default)]
pub struct Menu {
    entries: Vec<Entry>,
    under: Option<fltk::widget::Widget>,
    current: Option<usize>,
}

impl Menu {
    pub fn new() -> Menu {
        Menu::default()
    }

    /// An item, with a Lucide icon if given; `run` runs when it is picked (after the menu is gone).
    pub fn item(&mut self, label: &str, icon: Option<&'static str>, run: impl FnOnce() + 'static) -> &mut Menu {
        self.add(label, icon, true, run)
    }

    /// An item that is greyed out (it can't be picked) unless `enabled`.
    pub fn add(
        &mut self,
        label: &str,
        icon: Option<&'static str>,
        enabled: bool,
        run: impl FnOnce() + 'static,
    ) -> &mut Menu {
        self.entries.push(Entry::Item(Item {
            label: label.to_string(),
            icon,
            enabled,
            tick: None,
            run: Some(Box::new(run)),
        }));
        self
    }

    /// An item with a tick in front when `on`.
    pub fn tick(&mut self, label: &str, on: bool, run: impl FnOnce() + 'static) -> &mut Menu {
        self.entries.push(Entry::Item(Item {
            label: label.to_string(),
            icon: None,
            enabled: true,
            tick: Some(on),
            run: Some(Box::new(run)),
        }));
        self
    }

    /// A dividing line.
    pub fn line(&mut self) -> &mut Menu {
        if !self.entries.is_empty() {
            self.entries.push(Entry::Line);
        }
        self
    }

    /// Opens below `w` (or above it, when there is no room), at least as wide, as a dropdown does.
    pub fn under<W: WidgetExt>(&mut self, w: &W) -> &mut Menu {
        self.under = Some(w.as_base_widget());
        self
    }

    /// Starts with the cursor on item `i` (a dropdown's current choice).
    pub fn current(&mut self, i: usize) -> &mut Menu {
        self.current = Some(i);
        self
    }

    pub fn is_empty(&self) -> bool {
        !self.entries.iter().any(|e| matches!(e, Entry::Item(_)))
    }

    /// Opens it, waits for a pick, closes it, and runs the item picked. The picked item's number
    /// (counting items only, not lines).
    pub fn show(mut self) -> Option<usize> {
        if self.is_empty() {
            return None;
        }
        let focus = app::focus();
        let (host, x, y, min_w, above) = self.place();
        let (w, h) = size(&self.entries, min_w);
        let host_h = host.h();
        let host_w = host.w();
        // Flip to fit: left of the pointer, or above (the pointer, or the dropdown's widget).
        // A dropdown lines up with its widget's right edge instead.
        let x = match (x + w > host_w - 8, above) {
            (true, Some(_)) => (x + min_w - w).max(8),
            (true, None) => (x - w).max(8),
            (false, _) => x,
        };
        let y = if y + h > host_h - 8 {
            match above {
                Some(top) if top - h - 2 >= 8 => top - h - 2,
                Some(_) => y,
                None => (y - h).max(8),
            }
        } else {
            y
        };
        let mut pop = Popover::start(host, x, y, w, h);
        let (pw, ph) = pop.size();
        let st = Rc::new(RefCell::new(State {
            cursor: self.current.filter(|i| self.enabled(*i)),
            top: 0,
            pressed: false,
            max: (h - ph).max(0),
        }));
        let picked = Rc::new(Cell::new(None::<usize>));
        let mut area = Frame::new(0, 0, pw, ph, None);
        let items: Rc<Vec<Row>> = Rc::new(rows(&self.entries));
        area.draw({
            let (items, st) = (items.clone(), st.clone());
            move |f| draw_rows(f, &items, &st.borrow())
        });
        area.handle({
            let (items, st, picked, done) = (items.clone(), st.clone(), picked.clone(), pop.done.clone());
            move |f, ev| handle(f, ev, &items, &st, &picked, &done)
        });
        pop.finish();
        let _ = area.take_focus();
        pop.run(|| {});
        pop.close();
        if let Some(mut f) = focus.filter(|f| !f.was_deleted()) {
            let _ = f.take_focus();
        }
        let i = picked.get()?;
        if let Some(Entry::Item(item)) = self.entries.iter_mut().filter(|e| matches!(e, Entry::Item(_))).nth(i) {
            crate::crash::step(&format!("menu item {}", item.label));
            if let Some(run) = item.run.take() {
                run();
            }
        }
        Some(i)
    }

    fn enabled(&self, i: usize) -> bool {
        self.entries
            .iter()
            .filter_map(|e| match e {
                Entry::Item(item) => Some(item.enabled),
                Entry::Line => None,
            })
            .nth(i)
            .unwrap_or(false)
    }

    /// The window it opens in, where (in that window), how wide at least, and the top of the widget a
    /// dropdown hangs from.
    fn place(&self) -> (Box<dyn WindowExt>, i32, i32, i32, Option<i32>) {
        if let Some(w) = &self.under {
            if let Some(host) = w.window() {
                return (host, w.x(), w.y() + w.h() + 2, w.w(), Some(w.y()));
            }
        }
        let host: Box<dyn WindowExt> = app::belowmouse::<fltk::widget::Widget>()
            .and_then(|w| w.window())
            .or_else(|| app::first_window().map(|w| Box::new(w) as Box<dyn WindowExt>))
            .expect("a window to open a menu in");
        // From the keyboard (the Menu key, Shift+F10) it opens on the focused control instead.
        match app::focus().filter(|_| app::event() == Event::KeyDown) {
            Some(f) => match f.window() {
                Some(win) => (win, f.x() + 12, f.y() + 12, 0, None),
                None => (host, f.x() + 12, f.y() + 12, 0, None),
            },
            None => (host, app::event_x() + 2, app::event_y() + 2, 0, None),
        }
    }
}

/// A row as laid out: an item's number, or a line.
struct Row {
    item: Option<usize>,
    y: i32,
    h: i32,
    label: String,
    icon: Option<&'static str>,
    enabled: bool,
    tick: Option<bool>,
}

fn rows(entries: &[Entry]) -> Vec<Row> {
    let mut y = PAD;
    let mut n = 0;
    entries
        .iter()
        .map(|e| {
            let row = match e {
                Entry::Item(i) => {
                    n += 1;
                    Row {
                        item: Some(n - 1),
                        y,
                        h: ROW,
                        label: i.label.clone(),
                        icon: i.icon,
                        enabled: i.enabled,
                        tick: i.tick,
                    }
                }
                Entry::Line => Row {
                    item: None,
                    y,
                    h: LINE,
                    label: String::new(),
                    icon: None,
                    enabled: false,
                    tick: None,
                },
            };
            y += row.h;
            row
        })
        .collect()
}

/// Whether any item has an icon or a tick: then every label starts after that room.
fn lead(entries: &[Entry]) -> i32 {
    let any = entries
        .iter()
        .any(|e| matches!(e, Entry::Item(i) if i.icon.is_some() || i.tick.is_some()));
    if any {
        ICON + 10
    } else {
        0
    }
}

/// The panel's size for `entries`, at least `min_w` wide.
fn size(entries: &[Entry], min_w: i32) -> (i32, i32) {
    draw::set_font(Font::Helvetica, FONT);
    let text = entries
        .iter()
        .filter_map(|e| match e {
            Entry::Item(i) => Some(draw::measure(&i.label, false).0),
            Entry::Line => None,
        })
        .max()
        .unwrap_or(0);
    let h: i32 = entries
        .iter()
        .map(|e| if matches!(e, Entry::Line) { LINE } else { ROW })
        .sum();
    let w = PAD * 2 + 10 + lead(entries) + text + 24;
    (w.clamp(min_w.max(180), 480), h + PAD * 2)
}

struct State {
    /// The item under the mouse or the keyboard's cursor.
    cursor: Option<usize>,
    /// Pixels scrolled, when it is taller than the window.
    top: i32,
    pressed: bool,
    max: i32,
}

fn row_at(items: &[Row], st: &State, f: &Frame) -> Option<usize> {
    let y = app::event_y() - f.y() + st.top;
    if !app::event_inside_widget(f) {
        return None;
    }
    items
        .iter()
        .find(|r| r.item.is_some() && r.enabled && (r.y..r.y + r.h).contains(&y))
        .and_then(|r| r.item)
}

fn draw_rows(f: &Frame, items: &[Row], st: &State) {
    let (x, y, w, h) = (f.x(), f.y(), f.w(), f.h());
    let back = Color::Background;
    let fg = Color::Foreground;
    let quiet = Color::color_average(super::muted(), back, 0.55);
    let lead = if items.iter().any(|r| r.icon.is_some() || r.tick.is_some()) {
        ICON + 10
    } else {
        0
    };
    // Inside the rounded border: the rows never paint over it.
    draw::push_clip(x + 1, y + 2, w - 2, h - 4);
    draw::set_font(Font::Helvetica, FONT);
    for r in items {
        let ry = y + r.y - st.top;
        if ry + r.h < y || ry > y + h {
            continue;
        }
        let Some(n) = r.item else {
            draw::set_draw_color(Color::color_average(fg, back, 0.14));
            draw::draw_line(x + PAD + 4, ry + LINE / 2, x + w - PAD - 5, ry + LINE / 2);
            continue;
        };
        let lit = r.enabled && st.cursor == Some(n);
        if lit {
            draw::set_draw_color(super::rows::selected(back));
            draw::draw_rounded_rectf(x + PAD, ry, w - PAD * 2, r.h, RADIUS - 2);
        }
        let ink = if r.enabled { fg } else { quiet };
        let icon_ink = if r.enabled { super::button_ink() } else { quiet };
        let ix = x + PAD + 10;
        let shown = match r.tick {
            Some(true) => Some("check"),
            Some(false) => None,
            None => r.icon,
        };
        if let Some(mut i) = shown.and_then(|name| super::icons::icon(name, icon_ink, ICON)) {
            i.draw(ix, ry + (r.h - ICON) / 2, ICON, ICON);
        }
        draw::set_draw_color(ink);
        let tx = ix + lead;
        draw::draw_text2(&r.label, tx, ry, x + w - PAD - 10 - tx, r.h, Align::Left | Align::Clip);
    }
    if let Some((ty, th)) = super::scroll::thumb(y + 6, h - 12, st.top, st.max) {
        super::scroll::draw_thumb(x + w - super::scroll::BAR - 3, ty, super::scroll::BAR, th, back, false);
    }
    draw::pop_clip();
}

/// The item after (or before) the cursor that can be picked, wrapping around.
fn step(items: &[Row], cursor: Option<usize>, down: bool) -> Option<usize> {
    let open: Vec<usize> = items.iter().filter(|r| r.enabled).filter_map(|r| r.item).collect();
    if open.is_empty() {
        return None;
    }
    let at = cursor.and_then(|c| open.iter().position(|i| *i == c));
    Some(match (at, down) {
        (None, true) => open[0],
        (None, false) => open[open.len() - 1],
        (Some(p), true) => open[(p + 1) % open.len()],
        (Some(p), false) => open[(p + open.len() - 1) % open.len()],
    })
}

/// The next item to pick that starts with `c`, after the cursor.
fn by_letter(items: &[Row], cursor: Option<usize>, c: char) -> Option<usize> {
    let c = c.to_lowercase().next()?;
    let open: Vec<&Row> = items.iter().filter(|r| r.enabled && r.item.is_some()).collect();
    let at = cursor.and_then(|cur| open.iter().position(|r| r.item == Some(cur)));
    let start = at.map_or(0, |p| p + 1);
    (0..open.len())
        .map(|k| open[(start + k) % open.len()])
        .find(|r| r.label.to_lowercase().starts_with(c))
        .and_then(|r| r.item)
}

/// Keeps the cursor's row in sight.
fn reveal(items: &[Row], st: &mut State, h: i32) {
    let Some(r) = st.cursor.and_then(|c| items.iter().find(|r| r.item == Some(c))) else {
        return;
    };
    if r.y - PAD < st.top {
        st.top = (r.y - PAD).max(0);
    } else if r.y + r.h + PAD > st.top + h {
        st.top = (r.y + r.h + PAD - h).min(st.max);
    }
}

fn handle(
    f: &mut Frame,
    ev: Event,
    items: &[Row],
    st: &RefCell<State>,
    picked: &Cell<Option<usize>>,
    done: &Cell<bool>,
) -> bool {
    let mut s = st.borrow_mut();
    let pick = |i: Option<usize>| {
        if let Some(i) = i {
            picked.set(Some(i));
            done.set(true);
        }
    };
    match ev {
        Event::Focus | Event::Unfocus | Event::Enter => {
            f.redraw();
            true
        }
        Event::Move | Event::Drag => {
            let at = row_at(items, &s, f);
            if at.is_some() && at != s.cursor {
                s.cursor = at;
                f.redraw();
            }
            true
        }
        Event::Leave => true,
        Event::Push => {
            s.pressed = true;
            true
        }
        Event::Released => {
            if std::mem::take(&mut s.pressed) {
                pick(row_at(items, &s, f));
            }
            true
        }
        Event::MouseWheel => {
            if s.max > 0 {
                s.top = (s.top + app::event_dy_value() * ROW).clamp(0, s.max);
                f.redraw();
            }
            true
        }
        Event::KeyDown | Event::Shortcut => {
            let key = app::event_key();
            let h = f.h();
            match key {
                Key::Escape => done.set(true),
                Key::Down | Key::Up => {
                    s.cursor = step(items, s.cursor, key == Key::Down);
                    reveal(items, &mut s, h);
                }
                Key::Home | Key::End => {
                    s.cursor = step(items, None, key == Key::Home);
                    reveal(items, &mut s, h);
                }
                Key::Enter | Key::KPEnter => pick(s.cursor),
                _ if key == Key::from_char(' ') => pick(s.cursor),
                _ => match app::event_text().chars().next().filter(|c| c.is_alphanumeric()) {
                    Some(c) => {
                        s.cursor = by_letter(items, s.cursor, c).or(s.cursor);
                        reveal(items, &mut s, h);
                    }
                    None => return false,
                },
            }
            f.redraw();
            true
        }
        _ => false,
    }
}

/// A label as a menu item holds it, as shown: FLTK's `&&` is one `&`, a lone `&` underlines.
fn plain(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                chars.next();
                out.push('&');
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Makes `m` (a Choice or a MenuButton of ours) open its items in our menu, under it: on a click, and
/// on Space, Enter or the arrows while it has the keys. A Choice ticks its current item and takes the
/// one picked; a MenuButton flips a picked toggle and runs the item's callback.
pub fn dropdown<M: MenuExt + WidgetBase + Clone + 'static>(m: &mut M) {
    m.super_handle_first(false);
    m.handle(|m, ev| {
        let keys = ev == Event::KeyDown
            && m.has_focus()
            && matches!(app::event_key(), Key::Enter | Key::KPEnter | Key::Down | Key::Up)
            || ev == Event::KeyDown && m.has_focus() && app::event_key() == Key::from_char(' ');
        if !(ev == Event::Push || keys) || !m.active_r() {
            return false;
        }
        open(m);
        true
    });
}

/// Makes an input with a list beside it (an InputChoice) open the list in our menu: a pick fills the
/// input, as FLTK's would.
pub fn input_choice(c: &mut fltk::misc::InputChoice) {
    c.super_handle_first(false);
    c.handle(|c, ev| {
        let button = c.menu_button();
        let keys = ev == Event::KeyDown && app::event_key() == Key::Down && c.input().has_focus();
        if !(ev == Event::Push && app::event_inside_widget(&button) || keys) || !c.active_r() {
            return false;
        }
        let mut menu = Menu::new();
        menu.under(&*c);
        let now = c.value().unwrap_or_default();
        for i in 0..button.size() {
            let Some(label) = button.at(i).and_then(|item| item.label()) else {
                continue;
            };
            let label = plain(&label);
            if label == now {
                menu.current(i as usize);
            }
            let mut c = c.clone();
            let picked = label.clone();
            menu.tick(&label, label == now, move || {
                c.set_value(&picked);
                c.set_changed();
                c.do_callback();
            });
        }
        menu.show();
        true
    });
}

fn open<M: MenuExt + WidgetBase + Clone + 'static>(m: &mut M) {
    let choice = fltk::menu::Choice::from_dyn_widget(&*m).is_some();
    let mut menu = Menu::new();
    menu.under(&*m);
    let current = m.value();
    for i in 0..m.size() {
        let Some(item) = m.at(i) else { continue };
        let Some(label) = item.label() else { continue };
        if item.is_submenu() || !item.visible() {
            continue;
        }
        let label = plain(&label);
        let mut target = m.clone();
        if choice {
            if i == current {
                menu.current(i as usize);
            }
            menu.entries.push(Entry::Item(Item {
                label,
                icon: None,
                enabled: item.active(),
                tick: Some(i == current),
                run: Some(Box::new(move || {
                    if target.value() != i && target.set_value(i) {
                        target.set_changed();
                        target.do_callback();
                    }
                })),
            }));
        } else {
            let toggle = item.is_checkbox();
            menu.entries.push(Entry::Item(Item {
                label,
                icon: None,
                enabled: item.active(),
                tick: toggle.then(|| item.value()),
                run: Some(Box::new(move || {
                    let Some(mut item) = target.at(i) else { return };
                    if toggle {
                        if item.value() {
                            item.clear();
                        } else {
                            item.set();
                        }
                    }
                    target.set_value(i);
                    item.do_callback(&target);
                })),
            }));
        }
    }
    menu.show();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(item: Option<usize>, label: &str, enabled: bool) -> Row {
        Row {
            item,
            y: 0,
            h: ROW,
            label: label.into(),
            icon: None,
            enabled,
            tick: None,
        }
    }

    #[test]
    fn arrows_skip_greyed_items_and_lines_and_wrap() {
        let items = [
            row(Some(0), "Add", true),
            row(Some(1), "Edit", false),
            row(None, "", false),
            row(Some(2), "Delete", true),
        ];
        assert_eq!(step(&items, None, true), Some(0));
        assert_eq!(step(&items, Some(0), true), Some(2));
        assert_eq!(step(&items, Some(2), true), Some(0));
        assert_eq!(step(&items, Some(0), false), Some(2));
        assert_eq!(by_letter(&items, None, 'd'), Some(2));
        assert_eq!(by_letter(&items, None, 'e'), None);
    }

    /// Labels show as written: an outcome named "Yes/No" isn't a submenu, "&" isn't an underline.
    #[test]
    fn labels_are_drawn_as_written() {
        let mut m = Menu::new();
        m.item("Yes/No & maybe", None, || {}).line().tick("On", true, || {});
        let laid = rows(&m.entries);
        assert_eq!(laid[0].label, "Yes/No & maybe");
        assert_eq!(laid.iter().filter_map(|r| r.item).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(laid[2].y, PAD + ROW + LINE);
    }

    #[test]
    fn labels_show_as_fltk_would() {
        assert_eq!(plain("Save && quit"), "Save & quit");
        assert_eq!(plain("&File"), "File");
    }
}
