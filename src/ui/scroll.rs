//! Scrolling, one look and one behaviour everywhere: a thin rounded bar with no arrows.
//!
//! - Lists drawn by hand (`Rows`, the emote grid, the chat) place their bar with `thumb`, drag it with
//!   `dragged` and paint it with `draw_thumb`.
//! - FLTK's own bars (text views, trees, browsers) are painted over the same way: `view` for a widget
//!   with no draw of its own, else `paint_bars` at the end of its draw. `area` and `page` do theirs.
//! - A page that may not fit is a `page`: it is exactly as tall as its content (lists get a minimum),
//!   so there is never blank room to scroll into, and the wheel scrolls what is under the pointer.

use fltk::enums::{Color, Event, FrameType};
use fltk::group::{Flex, Group, Scroll, ScrollType, Wizard};
use fltk::prelude::*;
use fltk::valuator::Scrollbar;
use fltk::{app, draw};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The bar's width.
pub const BAR: i32 = 6;
/// The room a scroll area keeps on its right for the bar.
pub const GUTTER: i32 = 14;
/// How far a page moves for one step of the wheel.
const STEP: i32 = 40;
/// The least a list (any boxed widget that grows) gets on a page that scrolls.
const LIST_MIN: i32 = 140;

/// The thumb's top and height on a track `len` long from `y`, with `top` of `max` scrolled; none when
/// nothing scrolls.
pub fn thumb(y: i32, len: i32, top: i32, max: i32) -> Option<(i32, i32)> {
    if max <= 0 || len <= 0 {
        return None;
    }
    let th = (len * len / (len + max)).max(24).min(len);
    Some((y + (len - th) * top.clamp(0, max) / max, th))
}

/// Where a track `len` long from `y` is scrolled to when its thumb (`th` high) was grabbed `grab` below
/// its top and the pointer is at `py`.
pub fn dragged(py: i32, grab: i32, y: i32, len: i32, th: i32, max: i32) -> i32 {
    ((py - grab - y) * max / (len - th).max(1)).clamp(0, max)
}

/// Paints a thumb `w` wide over `back`: quiet, stronger while it is held.
pub fn draw_thumb(x: i32, y: i32, w: i32, h: i32, back: Color, held: bool) {
    draw::set_draw_color(Color::color_average(
        Color::Foreground,
        back,
        if held { 0.5 } else { 0.3 },
    ));
    draw::draw_rounded_rectf(x, y, w, h, w / 2);
}

/// Where FLTK puts a bar's thumb, by the same sums as `Fl_Scrollbar` and `Fl_Slider` (so it is dragged
/// where it is drawn): `(x, y, w, h)`, or none when everything shows.
fn native_thumb(s: &Scrollbar) -> Option<(i32, i32, i32, i32)> {
    let b = s.frame();
    let (x, y, w, h) = (s.x() + b.dx(), s.y() + b.dy(), s.w() - b.dw(), s.h() - b.dh());
    let horizontal = (s.get_type::<fltk::valuator::ScrollbarType>() as i32) & 1 == 1;
    let (along, across) = if horizontal { (w, h) } else { (h, w) };
    // Long enough, it has arrow buttons at both ends; the thumb moves between them.
    let (start, len) = if along < 3 * across {
        (0, along)
    } else {
        (across, along - 2 * across)
    };
    let size = ((s.slider_size() * len as f32) + 0.5) as i32;
    if len <= 0 || size >= len {
        return None;
    }
    let size = size.max(across / 2 + 1);
    let range = s.maximum() - s.minimum();
    let val = if range == 0.0 {
        0.5
    } else {
        ((s.value() - s.minimum()) / range).clamp(0.0, 1.0)
    };
    let at = start + (val * f64::from(len - size) + 0.5) as i32;
    Some(if horizontal {
        (x + at, y, size, h)
    } else {
        (x, y + at, w, size)
    })
}

/// Paints the FLTK bars inside `w` (a text view, a tree, a browser, a scroll area) as ours, over what
/// FLTK drew: no arrows, no grips, a thin rounded thumb. For a widget with a draw of its own, call it at
/// the end of that draw; else `view` does it. (FLTK's own bars can't be given a draw of ours.)
pub fn paint_bars<W: WidgetExt>(w: &W) {
    let back = w.color();
    let Some(g) = w.as_group() else { return };
    for i in 0..g.children() {
        let Some(c) = g.child(i) else { continue };
        let Some(bar) = Scrollbar::from_dyn_widget(&c).filter(|b| b.visible()) else {
            continue;
        };
        draw::draw_rect_fill(bar.x(), bar.y(), bar.w(), bar.h(), back);
        let Some((x, y, tw, th)) = native_thumb(&bar) else {
            continue;
        };
        let held = app::pushed().is_some_and(|p| p.as_widget_ptr() == bar.as_widget_ptr());
        if tw < th {
            draw_thumb(x + (tw - BAR) / 2, y + 2, BAR, (th - 4).max(BAR), back, held);
        } else {
            draw::set_draw_color(Color::color_average(
                Color::Foreground,
                back,
                if held { 0.5 } else { 0.3 },
            ));
            draw::draw_rounded_rectf(x + 2, y + (th - BAR) / 2, (tw - 4).max(BAR), BAR, BAR / 2);
        }
    }
}

/// A text view, tree or browser of ours with no draw of its own: its bars drawn like ours.
pub fn view<W: WidgetBase>(w: &mut W) {
    w.draw(|w| paint_bars(w));
}

/// The innermost visible widget under the pointer.
fn under_pointer(g: &Group) -> Option<fltk::widget::Widget> {
    for i in (0..g.children()).rev() {
        let Some(c) = g.child(i) else { continue };
        if !c.visible() || !app::event_inside_widget(&c) {
            continue;
        }
        return c.as_group().and_then(|inner| under_pointer(&inner)).or(Some(c));
    }
    None
}

/// Whether `w` may get the wheel before the area around it: anything but a layout box. (A layout box
/// would hand it to every child it holds, so a list beside the pointer would scroll.)
fn takes_wheel(w: &fltk::widget::Widget) -> bool {
    use fltk::browser::{Browser, CheckBrowser};
    use fltk::text::TextDisplay;
    use fltk::tree::Tree;
    let p = w.as_widget_ptr();
    w.as_group().is_none()
        || fltk::utils::is_ptr_of::<Tree>(p)
        || fltk::utils::is_ptr_of::<TextDisplay>(p)
        || fltk::utils::is_ptr_of::<Browser>(p)
        || fltk::utils::is_ptr_of::<CheckBrowser>(p)
        || fltk::utils::is_ptr_of::<Scroll>(p)
}

/// The wheel over a scroll area: what is under the pointer scrolls if it can (a list, a text view),
/// else the area does. Never a widget elsewhere in the area.
fn wheel(s: &Scroll) -> bool {
    if let Some(mut w) = under_pointer(&s.as_group().expect("a scroll area is a group")) {
        if takes_wheel(&w) && w.handle_event(Event::MouseWheel) {
            return true;
        }
    }
    let mut bar = s.scrollbar();
    if bar.visible() {
        bar.handle_event(Event::MouseWheel);
        // The thumb moved: paint the whole area (a copied strip would carry the old thumb along).
        s.clone().redraw();
    }
    true
}

/// A scroll area at `(x, y)` that scrolls up and down, with our bar and our wheel.
pub fn area(x: i32, y: i32, w: i32, h: i32) -> Scroll {
    let mut s = Scroll::new(x, y, w, h, None);
    s.set_type(ScrollType::Vertical);
    s.set_scrollbar_size(12);
    s.scrollbar().set_linesize(STEP);
    s.draw(|s| paint_bars(s));
    s.super_handle_first(false);
    s.handle(|s, ev| wheeled(s, ev));
    s
}

/// For an area's own `handle` (which replaces the one `area` gave it): true when `ev` was the wheel, and
/// it scrolled what is under the pointer or the area.
pub fn wheeled(s: &Scroll, ev: Event) -> bool {
    ev == Event::MouseWheel && app::event_dy() != app::MouseWheel::None && wheel(s)
}

/// Which children of `col` grow, and the heights of the others, as laid out now.
#[derive(Default, PartialEq)]
struct Shape {
    widths: i32,
    /// Each child's widget, whether it shows, and its height (0 when it grows).
    kids: Vec<(usize, bool, i32)>,
}

fn shape(col: &Flex, grows: &[bool]) -> Shape {
    Shape {
        widths: col.w(),
        kids: (0..col.children())
            .filter_map(|i| col.child(i))
            .enumerate()
            .map(|(i, c)| {
                let grow = grows.get(i).copied().unwrap_or(false);
                (c.as_widget_ptr() as usize, c.visible(), if grow { 0 } else { c.h() })
            })
            .collect(),
    }
}

/// The least room a growing child needs: none for a spacer (an empty frame with no box), else a list's.
fn least(c: &fltk::widget::Widget) -> i32 {
    let spacer = c.as_group().is_none() && c.frame() == FrameType::NoBox && c.label().is_empty() && c.image().is_none();
    if spacer {
        0
    } else {
        LIST_MIN
    }
}

/// How tall `col` must be to show everything (its lists at their least), at its current width. Lays it
/// out twice to tell the children that grow from the fixed ones; returns which grow.
fn natural(col: &mut Flex) -> (i32, Vec<bool>) {
    let (x, y, w) = (col.x(), col.y(), col.w());
    let heights = |col: &mut Flex, h: i32| {
        col.resize(x, y, w, h);
        (0..col.children())
            .filter_map(|i| col.child(i))
            .map(|c| (c.visible(), c.h(), least(&c)))
            .collect::<Vec<_>>()
    };
    let a = heights(col, 4000);
    let b = heights(col, 6000);
    let grows: Vec<bool> = a.iter().zip(&b).map(|(a, b)| a.0 && a.1 != b.1).collect();
    let shown = a.iter().filter(|c| c.0).count() as i32;
    let (_, top, _, bottom) = col.margins();
    let body: i32 = a
        .iter()
        .zip(&grows)
        .filter(|(c, _)| c.0)
        .map(|(c, grow)| if *grow { c.2 } else { c.1.max(0) })
        .sum();
    (top + bottom + (shown - 1).max(0) * col.pad() + body, grows)
}

struct Fit {
    grows: Vec<bool>,
    shape: Shape,
    /// A re-fit is on its way.
    queued: bool,
}

/// Makes `col` as tall as its content (at least the page), keeping the scroll where it was.
fn fit(s: &mut Scroll, col: &mut Flex, state: &RefCell<Fit>) {
    let (x, y, w, h) = (s.x(), s.y(), s.w(), s.h());
    let width = (w - GUTTER).max(50);
    col.resize(x, y - s.yposition(), width, col.h());
    let (need, grows) = natural(col);
    let content = need.max(h);
    col.resize(x, y - s.yposition(), width, content);
    let top = s.yposition().clamp(0, content - h);
    s.scroll_to(0, top);
    let mut st = state.borrow_mut();
    st.shape = shape(col, &grows);
    st.grows = grows;
    s.redraw();
}

/// A page that scrolls when it does not fit: a column to fill, as tall as what is in it. `label` is its
/// name in the sidebar.
pub fn page(tabs: &Wizard, label: &str) -> (Scroll, Flex) {
    let (mut s, col) = bare(tabs, label);
    let state = Rc::new(RefCell::new(Fit {
        grows: Vec::new(),
        shape: Shape::default(),
        queued: false,
    }));
    s.resize_callback({
        let (mut col, state) = (col.clone(), state.clone());
        move |s, _, _, _, _| fit(s, &mut col, &state)
    });
    // A hint that wrapped to more lines, or a row shown or hidden, changes the height: fit again after
    // the draw that shows it.
    s.draw({
        let (col, state) = (col.clone(), state.clone());
        move |s| {
            paint_bars(s);
            let mut st = state.borrow_mut();
            if st.queued || shape(&col, &st.grows) == st.shape {
                return;
            }
            st.queued = true;
            let (mut s, mut col, state) = (s.clone(), col.clone(), state.clone());
            app::awake_callback(move || {
                state.borrow_mut().queued = false;
                if !s.was_deleted() {
                    fit(&mut s, &mut col, &state);
                }
            });
        }
    });
    let shown = Rc::new(Cell::new(false));
    s.handle({
        let (col, state) = (col.clone(), state.clone());
        move |s, ev| match ev {
            Event::MouseWheel => wheeled(s, ev),
            // Fit once it first shows, with the fonts and hints measured.
            Event::Show if !shown.replace(true) => {
                let (mut s, mut col, state) = (s.clone(), col.clone(), state.clone());
                app::awake_callback(move || {
                    if !s.was_deleted() {
                        fit(&mut s, &mut col, &state);
                    }
                });
                false
            }
            _ => false,
        }
    });
    (s, col)
}

/// A page that scrolls, with a column whose height its owner sets (Settings lays its rows out itself).
pub fn bare(tabs: &Wizard, label: &str) -> (Scroll, Flex) {
    let mut s = area(tabs.x(), tabs.y(), tabs.w(), tabs.h());
    s.set_label(label);
    let mut col = Flex::new(s.x(), s.y(), s.w() - GUTTER, s.h(), None).column();
    col.set_margin(10);
    col.set_pad(4);
    (s, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thumb_spans_the_track_and_follows_the_scroll() {
        assert_eq!(thumb(0, 100, 0, 0), None);
        // Half the content shows: half the track.
        assert_eq!(thumb(10, 100, 0, 100), Some((10, 50)));
        assert_eq!(thumb(10, 100, 100, 100), Some((60, 50)));
        // Long content: never thinner than 24.
        assert_eq!(thumb(0, 100, 0, 100_000).map(|t| t.1), Some(24));
    }

    #[test]
    fn dragging_the_thumb_maps_back_to_the_scroll() {
        let (y, len, max) = (10, 100, 100);
        let (ty, th) = thumb(y, len, 40, max).unwrap();
        assert_eq!(dragged(ty + 5, 5, y, len, th, max), 40);
        assert_eq!(dragged(-500, 0, y, len, th, max), 0);
        assert_eq!(dragged(5000, 0, y, len, th, max), max);
    }
}
