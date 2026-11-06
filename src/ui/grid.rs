//! The emote picker's grid, drawn by hand in one widget: section titles that fold, emotes in rows (the
//! animated ones play), a highlight under the mouse or the keyboard's cursor, and a thin scrollbar. A right
//! click or the Menu key opens a menu to insert or (un)favorite the emote.

use super::icons::icon;
use super::img;
use crate::engine::UiCmd;
use crate::i18n::tr;
use fltk::enums::{Align, Color, Event, Font, FrameType, Key};
use fltk::frame::Frame;
use fltk::prelude::*;
use fltk::{app, draw};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedSender;

const CELL: i32 = 36;
const PIC: i32 = 28;
const HEAD: i32 = 28;
const NOTE: i32 = 40;
const BAR: i32 = super::scroll::BAR;

pub enum Item {
    /// A section's title; its emotes follow unless it's folded.
    Head {
        title: String,
        open: bool,
        count: usize,
        note: Option<String>,
    },
    Emote {
        name: String,
        url: String,
        fav: bool,
    },
}

/// What a click asked for; the picker reads them.
pub enum Asked {
    /// Left click; true with Shift held (pick several).
    Pick(String, String, bool),
    /// From the menu (a right click or the Menu key).
    Favorite(String, String),
    /// A section's title.
    Fold(String),
}

#[derive(Default)]
struct State {
    items: Vec<Item>,
    /// Where each item went at the last draw: its box (relative to the content's top).
    at: Vec<(i32, i32, i32, i32)>,
    height: i32,
    top: i32,
    hover: Option<usize>,
    ticking: bool,
    grab: Option<i32>,
    /// Keep visible images alive even when the shared cache evicts them.
    visible: HashMap<String, img::Pic>,
}

#[derive(Clone)]
pub struct Grid {
    pub area: Frame,
    st: Rc<RefCell<State>>,
    pub asked: Rc<RefCell<Vec<Asked>>>,
}

impl Grid {
    pub fn new(cmds: &UnboundedSender<UiCmd>) -> Grid {
        let mut area = Frame::default();
        area.set_frame(FrameType::FlatBox);
        area.set_color(Color::Background2);
        area.visible_focus(true);
        let g = Grid {
            area: area.clone(),
            st: Rc::default(),
            asked: Rc::default(),
        };
        area.draw({
            let (g, cmds) = (g.clone(), cmds.clone());
            move |f| g.draw(f, &cmds)
        });
        area.handle({
            let g = g.clone();
            move |f, ev| g.handle(f, ev)
        });
        g
    }

    /// New items; `keep` keeps the scroll (a fold or a favorite), else back to the top.
    pub fn set(&self, items: Vec<Item>, keep: bool) {
        let mut st = self.st.borrow_mut();
        st.items = items;
        st.visible.clear();
        st.hover = None;
        if !keep {
            st.top = 0;
        }
        drop(st);
        self.area.clone().redraw();
    }

    /// Lays the items out at width `w`: their boxes, and the whole height.
    fn lay(st: &mut State, w: i32) {
        let per = ((w - 8) / CELL).max(1);
        let (mut y, mut k) = (4, 0);
        let mut at = Vec::with_capacity(st.items.len());
        for it in &st.items {
            match it {
                Item::Head { note, open, .. } => {
                    if k > 0 {
                        y += CELL + 6;
                    }
                    k = 0;
                    at.push((4, y, w - 8, HEAD));
                    y += HEAD + 2;
                    if note.is_some() && *open {
                        y += NOTE;
                    }
                }
                Item::Emote { .. } => {
                    at.push((4 + (k % per) * CELL, y + (k / per) * CELL, CELL, CELL));
                    k += 1;
                    if k % per == 0 {
                        y += CELL;
                        k = 0;
                    }
                }
            }
        }
        if k > 0 {
            y += CELL;
        }
        st.at = at;
        st.height = y + 4;
    }

    fn max(&self, st: &State) -> i32 {
        (st.height - self.area.h()).max(0)
    }

    fn thumb(&self, st: &State) -> Option<(i32, i32)> {
        super::scroll::thumb(self.area.y() + 4, self.area.h() - 8, st.top, self.max(st))
    }

    fn draw(&self, f: &Frame, cmds: &UnboundedSender<UiCmd>) {
        let mut st = self.st.borrow_mut();
        Self::lay(&mut st, f.w() - BAR - 6);
        st.top = st.top.clamp(0, self.max(&st));
        let (fx, fy) = (f.x(), f.y() - st.top);
        let (fg, bg) = (Color::Foreground, f.color());
        let quiet = super::muted();
        draw::push_clip(f.x(), f.y(), f.w(), f.h());
        // The custom draw callback replaces the frame's normal background drawing. Clear the old
        // contents before drawing a new scroll position or animation frame.
        draw::draw_rect_fill(f.x(), f.y(), f.w(), f.h(), bg);
        let mut moving = false;
        let mut previous = std::mem::take(&mut st.visible);
        let mut visible = HashMap::new();
        for (k, (it, &(x, y, w, h))) in st.items.iter().zip(&st.at).enumerate() {
            let (x, y) = (fx + x, fy + y);
            if y + h < f.y() || y > f.y() + f.h() {
                continue;
            }
            let hover = st.hover == Some(k);
            if hover && f.has_focus() {
                draw::set_draw_color(Color::color_average(fg, bg, 0.5));
                draw::draw_rounded_rect(x, y, w, h, 6);
            }
            match it {
                Item::Head {
                    title,
                    open,
                    count,
                    note,
                } => {
                    if hover {
                        draw::set_draw_color(Color::color_average(fg, bg, 0.06));
                        draw::draw_rounded_rectf(x, y, w, h, 6);
                    }
                    let chevron = if *open { "chevron-down" } else { "chevron-right" };
                    if let Some(mut i) = icon(chevron, quiet, 14) {
                        i.draw(x + 6, y + (h - 14) / 2, 14, 14);
                    }
                    draw::set_font(Font::HelveticaBold, 13);
                    draw::set_draw_color(fg);
                    let t = if *count > 0 {
                        format!("{title}  {count}")
                    } else {
                        title.clone()
                    };
                    draw::draw_text2(&t, x + 26, y, w - 30, h, Align::Left | Align::Clip);
                    if let (Some(n), true) = (note, *open) {
                        draw::set_font(Font::Helvetica, 12);
                        draw::set_draw_color(quiet);
                        draw::draw_text2(n, x + 8, y + h, w - 16, NOTE, Align::Center | Align::Wrap);
                    }
                }
                Item::Emote { name, url, fav } => {
                    if hover || *fav {
                        let c = if hover {
                            Color::color_average(fg, bg, 0.14)
                        } else {
                            Color::color_average(Color::Selection, bg, 0.18)
                        };
                        draw::set_draw_color(c);
                        draw::draw_rounded_rectf(x + 1, y + 1, w - 2, h - 2, 6);
                    }
                    let pic = visible
                        .get(url)
                        .cloned()
                        .or_else(|| previous.remove(url))
                        .or_else(|| img::get(cmds, url, PIC, false));
                    match pic {
                        Some(p) => {
                            // Every animated emote on screen plays; the grid is only open while picking.
                            let mut i = if p.animated() {
                                moving = true;
                                p.at(now_ms()).clone()
                            } else {
                                p.first()
                            };
                            i.draw(x + (w - i.w()) / 2, y + (h - i.h()) / 2, i.w(), i.h());
                            visible.insert(url.clone(), p);
                        }
                        None => {
                            draw::set_font(Font::Helvetica, 10);
                            draw::set_draw_color(quiet);
                            draw::draw_text2(name, x, y, w, h, Align::Center | Align::Clip);
                        }
                    }
                }
            }
        }
        st.visible = visible;
        if let Some((ty, th)) = self.thumb(&st) {
            super::scroll::draw_thumb(f.x() + f.w() - BAR - 3, ty, BAR, th, bg, st.grab.is_some());
        }
        draw::pop_clip();
        if moving && !st.ticking {
            st.ticking = true;
            let g = self.clone();
            app::add_timeout3(0.05, move |_| {
                g.st.borrow_mut().ticking = false;
                // The picker may have closed (and its widgets with it) in the meantime.
                if !g.area.was_deleted() {
                    g.area.clone().redraw();
                }
            });
        }
    }

    fn under(&self, st: &State) -> Option<usize> {
        let (ex, ey) = (app::event_x() - self.area.x(), app::event_y() - self.area.y() + st.top);
        st.at
            .iter()
            .position(|&(x, y, w, h)| (x..x + w).contains(&ex) && (y..y + h).contains(&ey))
    }

    fn handle(&self, f: &mut Frame, ev: Event) -> bool {
        let mut st = self.st.borrow_mut();
        match ev {
            Event::Enter => true,
            Event::Move | Event::Leave => {
                let h = if ev == Event::Move { self.under(&st) } else { None };
                if h != st.hover {
                    st.hover = h;
                    let tip = match h.and_then(|k| st.items.get(k)) {
                        Some(Item::Emote { name, .. }) => name.replace('@', "@@"),
                        _ => String::new(),
                    };
                    f.set_tooltip(&tip);
                    f.redraw();
                }
                true
            }
            Event::MouseWheel if !app::event_inside_widget(f) || self.max(&st) == 0 => false,
            Event::MouseWheel => {
                st.top = (st.top + app::event_dy_value() * CELL).clamp(0, self.max(&st));
                st.hover = self.under(&st);
                f.redraw();
                true
            }
            Event::Push => {
                if let Some((ty, th)) = self.thumb(&st).filter(|_| app::event_x() >= f.x() + f.w() - BAR - 6) {
                    let ey = app::event_y();
                    st.grab = Some(if (ty..ty + th).contains(&ey) { ey - ty } else { th / 2 });
                    drop(st);
                    self.drag(f);
                    return true;
                }
                let _ = f.take_focus();
                let Some(k) = self.under(&st) else { return true };
                st.hover = Some(k);
                drop(st);
                if super::right_button() {
                    self.menu(k);
                } else {
                    self.act(k, app::event_state().contains(fltk::enums::Shortcut::Shift));
                }
                true
            }
            Event::Focus | Event::Unfocus => {
                f.redraw();
                true
            }
            Event::KeyDown => {
                let Some(k) = st
                    .hover
                    .filter(|&k| k < st.items.len())
                    .or((!st.items.is_empty()).then_some(0))
                else {
                    return false;
                };
                if super::menu_key() {
                    drop(st);
                    self.menu(k);
                    return true;
                }
                let key = app::event_key();
                if key == Key::Enter || key == Key::KPEnter || key == Key::from_char(' ') {
                    drop(st);
                    self.act(k, app::is_event_shift());
                    return true;
                }
                let per = ((f.w() - BAR - 6 - 8) / CELL).max(1) as usize;
                let Some(next) = step(&st.items, k, key, per) else {
                    return false;
                };
                st.hover = Some(next);
                // Keep the cursor in sight.
                if let Some(&(_, y, _, h)) = st.at.get(next) {
                    if y < st.top {
                        st.top = (y - 4).max(0);
                    } else if y + h > st.top + f.h() {
                        st.top = (y + h + 4 - f.h()).clamp(0, self.max(&st));
                    }
                }
                f.redraw();
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
            _ => false,
        }
    }

    /// What a click or Enter does on item `k`: fold a section or pick an emote (`several` keeps the
    /// picker open).
    fn act(&self, k: usize, several: bool) {
        let asked = match self.st.borrow().items.get(k) {
            Some(Item::Head { title, .. }) => Asked::Fold(title.clone()),
            Some(Item::Emote { name, url, .. }) => Asked::Pick(name.clone(), url.clone(), several),
            None => return,
        };
        self.asked.borrow_mut().push(asked);
    }

    /// The emote's menu: insert it, or add it to (remove it from) the favorites.
    fn menu(&self, k: usize) {
        let Some((name, url, fav)) = (match self.st.borrow().items.get(k) {
            Some(Item::Emote { name, url, fav }) => Some((name.clone(), url.clone(), *fav)),
            _ => None,
        }) else {
            return;
        };
        let mut m = super::menu::Menu::new();
        let entries = [
            (
                tr("emotes.insert"),
                "plus",
                Asked::Pick(name.clone(), url.clone(), false),
            ),
            if fav {
                (tr("emotes.remove_favorite"), "star-off", Asked::Favorite(name, url))
            } else {
                (tr("emotes.add_favorite"), "star", Asked::Favorite(name, url))
            },
        ];
        for (label, icon_name, asked) in entries {
            let queue = self.asked.clone();
            m.item(label, Some(icon_name), move || queue.borrow_mut().push(asked));
        }
        m.show();
    }

    fn drag(&self, f: &mut Frame) {
        let mut st = self.st.borrow_mut();
        let (Some(grab), Some((_, th)), max) = (st.grab, self.thumb(&st), self.max(&st)) else {
            return;
        };
        st.top = super::scroll::dragged(app::event_y(), grab, f.y() + 4, f.h() - 8, th, max);
        f.redraw();
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fltk::prelude::SurfaceDevice;
    use fltk::surface::ImageSurface;

    /// On a display, by hand (`DISPLAY=:99 cargo test grid -- --ignored`): it opens a connection to the
    /// display that outlives it, and FLTK used on it from other tests' threads then hangs them.
    #[test]
    #[ignore]
    fn redraw_clears_previous_grid_contents() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        let _app = app::App::default();
        let (cmds, _rx) = tokio::sync::mpsc::unbounded_channel();
        let grid = Grid::new(&cmds);
        let mut frame = grid.area.clone();
        frame.resize(0, 0, 100, 100);
        frame.set_color(Color::White);
        let surface = ImageSurface::new(100, 100, false);
        ImageSurface::push_current(&surface);
        draw::draw_rect_fill(0, 0, 100, 100, Color::Red);
        grid.draw(&frame, &cmds);
        let pixels = surface.image().unwrap().to_rgb_data();
        ImageSurface::pop_current();
        assert_eq!(&pixels[..3], &[255, 255, 255]);
        visible_picture_survives_redraw_and_is_released_when_items_change();
    }

    fn visible_picture_survives_redraw_and_is_released_when_items_change() {
        let (cmds, _rx) = tokio::sync::mpsc::unbounded_channel();
        let grid = Grid::new(&cmds);
        let mut frame = grid.area.clone();
        frame.resize(0, 0, 100, 36);
        grid.set(
            vec![Item::Emote {
                name: "test".into(),
                url: "test".into(),
                fav: false,
            }],
            false,
        );
        grid.st.borrow_mut().visible.insert("test".into(), img::test_pic());
        let surface = ImageSurface::new(100, 36, false);
        ImageSurface::push_current(&surface);
        grid.draw(&frame, &cmds);
        assert!(grid.st.borrow().visible.contains_key("test"));
        grid.set(Vec::new(), false);
        grid.draw(&frame, &cmds);
        assert!(grid.st.borrow().visible.is_empty());
        ImageSurface::pop_current();
    }
}

/// Where the keyboard's cursor goes from item `k` with `key`, `per` emotes to a row: left and right
/// through every item, up and down a row (a section's title is a row of its own).
fn step(items: &[Item], k: usize, key: Key, per: usize) -> Option<usize> {
    let last = items.len().checked_sub(1)?;
    // Each item's row and column, as laid out.
    let mut place = Vec::with_capacity(items.len());
    let (mut row, mut col) = (0usize, 0usize);
    for (i, it) in items.iter().enumerate() {
        match it {
            Item::Head { .. } => {
                if i > 0 {
                    row += 1;
                }
                place.push((row, 0));
                row += 1;
                col = 0;
            }
            Item::Emote { .. } => {
                if col == per {
                    row += 1;
                    col = 0;
                }
                place.push((row, col));
                col += 1;
            }
        }
    }
    let (r, c) = place[k];
    let near = |want: usize| {
        let row: Vec<usize> = (0..items.len()).filter(|&i| place[i].0 == want).collect();
        row.iter().copied().min_by_key(|&i| place[i].1.abs_diff(c))
    };
    match key {
        Key::Left => k.checked_sub(1),
        Key::Right => (k < last).then_some(k + 1),
        Key::Home => Some(0),
        Key::End => Some(last),
        Key::Up => (0..r).rev().find_map(near),
        Key::Down => (r + 1..=place[last].0).find_map(near),
        _ => None,
    }
}

#[cfg(test)]
mod keys {
    use super::*;

    fn emote(n: &str) -> Item {
        Item::Emote {
            name: n.into(),
            url: n.into(),
            fav: false,
        }
    }
    fn head() -> Item {
        Item::Head {
            title: "t".into(),
            open: true,
            count: 0,
            note: None,
        }
    }

    #[test]
    fn arrows_walk_rows_and_sections() {
        // A title, five emotes three to a row, another title, one emote.
        let items = vec![
            head(),
            emote("a"),
            emote("b"),
            emote("c"),
            emote("d"),
            emote("e"),
            head(),
            emote("f"),
        ];
        assert_eq!(step(&items, 0, Key::Down, 3), Some(1));
        assert_eq!(step(&items, 2, Key::Down, 3), Some(5));
        assert_eq!(step(&items, 5, Key::Up, 3), Some(2));
        assert_eq!(step(&items, 4, Key::Down, 3), Some(6));
        assert_eq!(step(&items, 6, Key::Down, 3), Some(7));
        assert_eq!(step(&items, 1, Key::Up, 3), Some(0));
        assert_eq!(step(&items, 0, Key::Up, 3), None);
        assert_eq!(step(&items, 7, Key::Right, 3), None);
        assert_eq!(step(&items, 3, Key::Right, 3), Some(4));
    }
}
