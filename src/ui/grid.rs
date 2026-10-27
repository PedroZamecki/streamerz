//! The emote picker's grid, drawn by hand in one widget: section titles that fold, emotes in rows, a
//! highlight under the mouse (an animated emote plays there), and a thin scrollbar.

use super::icons::icon;
use super::img;
use crate::engine::UiCmd;
use fltk::enums::{Align, Color, Event, Font, FrameType};
use fltk::frame::Frame;
use fltk::prelude::*;
use fltk::{app, draw};
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedSender;

const CELL: i32 = 36;
const PIC: i32 = 28;
const HEAD: i32 = 28;
const NOTE: i32 = 40;
const BAR: i32 = 6;

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
    /// Right click.
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
        let max = self.max(st);
        if max == 0 {
            return None;
        }
        let h = self.area.h() - 8;
        let th = (h * h / (h + max)).max(24).min(h);
        Some((self.area.y() + 4 + (h - th) * st.top.clamp(0, max) / max, th))
    }

    fn draw(&self, f: &Frame, cmds: &UnboundedSender<UiCmd>) {
        let mut st = self.st.borrow_mut();
        Self::lay(&mut st, f.w() - BAR - 6);
        st.top = st.top.clamp(0, self.max(&st));
        let (fx, fy) = (f.x(), f.y() - st.top);
        let (fg, bg) = (Color::Foreground, f.color());
        let quiet = super::muted();
        draw::push_clip(f.x(), f.y(), f.w(), f.h());
        let mut moving = false;
        for (k, (it, &(x, y, w, h))) in st.items.iter().zip(&st.at).enumerate() {
            let (x, y) = (fx + x, fy + y);
            if y + h < f.y() || y > f.y() + f.h() {
                continue;
            }
            let hover = st.hover == Some(k);
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
                    match img::get(cmds, url, PIC, false) {
                        Some(p) => {
                            // Still, but playing under the mouse.
                            let mut i = if hover && p.animated() {
                                moving = true;
                                p.at(now_ms()).clone()
                            } else {
                                p.first()
                            };
                            i.draw(x + (w - i.w()) / 2, y + (h - i.h()) / 2, i.w(), i.h());
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
        if let Some((ty, th)) = self.thumb(&st) {
            draw::set_draw_color(Color::color_average(fg, bg, if st.grab.is_some() { 0.5 } else { 0.3 }));
            draw::draw_rounded_rectf(f.x() + f.w() - BAR - 3, ty, BAR, th, BAR / 2);
        }
        draw::pop_clip();
        if moving && !st.ticking {
            st.ticking = true;
            let g = self.clone();
            app::add_timeout3(0.05, move |_| {
                g.st.borrow_mut().ticking = false;
                g.area.clone().redraw();
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
                        Some(Item::Emote { name, fav, .. }) => {
                            let what = if *fav { "unfavorite" } else { "favorite" };
                            format!("{} (right-click to {what})", name.replace('@', "@@"))
                        }
                        _ => String::new(),
                    };
                    f.set_tooltip(&tip);
                    f.redraw();
                }
                true
            }
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
                let right = app::event_mouse_button() == app::MouseButton::Right;
                let asked = match self.under(&st).and_then(|k| st.items.get(k)) {
                    Some(Item::Head { title, .. }) => Asked::Fold(title.clone()),
                    Some(Item::Emote { name, url, .. }) if right => Asked::Favorite(name.clone(), url.clone()),
                    Some(Item::Emote { name, url, .. }) => {
                        let shift = app::event_state().contains(fltk::enums::Shortcut::Shift);
                        Asked::Pick(name.clone(), url.clone(), shift)
                    }
                    None => return true,
                };
                self.asked.borrow_mut().push(asked);
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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
