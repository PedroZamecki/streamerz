//! Form pieces for dialogs that ask for a few things: a field with an icon and a hint in it (no label
//! column), a list of fields that grows as it's filled, and a chip (a platform's logo and name, on or
//! off). Short forms ask only what's needed, and show a field only when it's needed.

use super::ROW;
use fltk::button::ToggleButton;
use fltk::enums::{Align, Color, Event, Font, FrameType};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::input::Input;
use fltk::prelude::*;
use fltk::{app, draw};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The room an icon (or a number) takes before a field.
pub const LEAD: i32 = 22;

/// A row with a Lucide `icon` (muted) before an input that says `hint` while empty. Put in `col`.
pub fn field(col: &mut Flex, icon: &str, hint: &str) -> Input {
    let mut row = Flex::default().row();
    row.set_pad(8);
    let mut pic = Frame::default();
    pic.set_image(super::icons::icon(icon, super::muted(), 16));
    row.fixed(&pic, LEAD);
    let mut input = Input::default();
    super::placeholder(&mut input, hint);
    input.set_tooltip(hint);
    row.end();
    col.fixed(&row, ROW);
    input
}

/// What runs when the number of fields shown changes.
type Changed = Box<dyn Fn()>;

/// Fields that grow as they're filled: `min` shown at first, one more as the last one gets text, up to
/// `max`. Each has its number before it and `hint(n)` in it.
pub struct Growing {
    rows: Vec<Flex>,
    pub inputs: Vec<Input>,
    min: usize,
    max: Rc<Cell<usize>>,
    /// Called after the number shown changed (to make the dialog fit).
    changed: Rc<RefCell<Option<Changed>>>,
}

impl Growing {
    pub fn new(col: &mut Flex, min: usize, max: usize, hint: impl Fn(usize) -> String) -> Rc<Growing> {
        let mut rows = Vec::new();
        let mut inputs = Vec::new();
        for i in 0..max {
            let mut row = Flex::default().row();
            row.set_pad(8);
            let mut n = Frame::default()
                .with_label(&(i + 1).to_string())
                .with_align(Align::Center | Align::Inside);
            n.set_label_color(super::muted());
            n.set_label_size(12);
            row.fixed(&n, LEAD);
            let mut input = Input::default();
            let h = hint(i + 1);
            super::placeholder(&mut input, &h);
            input.set_trigger(fltk::enums::CallbackTrigger::Changed);
            row.end();
            col.fixed(&row, ROW);
            rows.push(row);
            inputs.push(input);
        }
        let me = Rc::new(Growing {
            rows,
            inputs,
            min,
            max: Rc::new(Cell::new(max)),
            changed: Rc::default(),
        });
        for i in &me.inputs {
            let weak = Rc::downgrade(&me);
            i.clone().set_callback(move |_| {
                if let Some(g) = weak.upgrade() {
                    g.update();
                }
            });
        }
        me.update();
        me
    }

    /// Runs `f` after the number of fields shown changed.
    pub fn on_change(&self, f: impl Fn() + 'static) {
        *self.changed.borrow_mut() = Some(Box::new(f));
    }

    /// At most `n` now (fields past it are hidden, and not counted in `values`).
    pub fn set_max(&self, n: usize) {
        self.max.set(n.clamp(self.min, self.inputs.len()));
        self.update();
    }

    /// How many are shown.
    pub fn shown(&self) -> usize {
        self.rows.iter().filter(|r| r.visible()).count()
    }

    /// What's written in the fields shown, trimmed, empty ones left out.
    pub fn values(&self) -> Vec<String> {
        self.inputs
            .iter()
            .take(self.shown())
            .map(|i| i.value().trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    }

    fn update(&self) {
        let values: Vec<String> = self.inputs.iter().map(|i| i.value()).collect();
        let want = wanted(&values, self.min, self.max.get());
        if want == self.shown() {
            return;
        }
        for (i, row) in self.rows.iter().enumerate() {
            let mut row = row.clone();
            if i < want {
                row.show();
            } else {
                row.hide();
            }
        }
        if let Some(f) = self.changed.borrow().as_ref() {
            f();
        }
    }
}

/// How many fields to show: one past the last one filled, at least `min`, at most `max`.
fn wanted(values: &[String], min: usize, max: usize) -> usize {
    let filled = values.iter().rposition(|v| !v.trim().is_empty()).map_or(0, |i| i + 1);
    (filled + 1).clamp(min, max.max(min))
}

/// A chip: a toggle with a logo (a platform's, a brand's) or a Lucide icon and a name. On, it's filled
/// with the accent colour. `fixed` in `row` at its width.
pub fn chip(row: &mut Flex, label: &str, logo: Option<fltk::image::SvgImage>, on: bool) -> ToggleButton {
    draw::set_font(Font::Helvetica, 14);
    let w = draw::width(label) as i32 + if logo.is_some() { 52 } else { 30 };
    let mut b = ToggleButton::default().with_label(label);
    b.set_value(on);
    b.set_frame(FrameType::NoBox);
    b.set_down_frame(FrameType::NoBox);
    b.clear_visible_focus();
    let logo = Rc::new(logo);
    b.draw(move |b| {
        let back = Color::Background;
        let accent = Color::Selection;
        let hover = app::belowmouse::<fltk::widget::Widget>().is_some_and(|w| w.as_widget_ptr() == b.as_widget_ptr());
        let on = b.value();
        let (fill, edge) = match (on, b.active_r()) {
            (true, _) => (
                Color::color_average(accent, back, 0.22),
                Color::color_average(accent, back, 0.8),
            ),
            (false, true) if hover => (
                super::rows::selected(back),
                Color::color_average(Color::Foreground, back, 0.35),
            ),
            _ => (back, Color::color_average(Color::Foreground, back, 0.2)),
        };
        let (x, y, w, h) = (b.x(), b.y(), b.w(), b.h());
        // Its parent's background first: a hover repaints only the chip.
        draw::draw_rect_fill(x, y, w, h, back);
        draw::set_draw_color(fill);
        draw::draw_rounded_rectf(x, y, w, h, h / 2);
        draw::set_draw_color(edge);
        draw::draw_rounded_rect(x, y, w, h, h / 2);
        let mut tx = x + 14;
        if let Some(logo) = logo.as_ref() {
            let mut l = logo.clone();
            if !b.active_r() {
                l.inactive();
            }
            l.draw(x + 12, y + (h - 16) / 2, 16, 16);
            tx = x + 34;
        }
        draw::set_font(Font::Helvetica, 14);
        draw::set_draw_color(if b.active_r() {
            Color::Foreground
        } else {
            Color::color_average(super::muted(), back, 0.6)
        });
        draw::draw_text2(&b.label(), tx, y, w - (tx - x) - 10, h, Align::Left | Align::Clip);
    });
    b.handle(|b, ev| match ev {
        Event::Enter | Event::Leave => {
            b.redraw();
            false
        }
        _ => false,
    });
    row.fixed(&b, w);
    b
}

/// A small icon before a control in a row (a timer before a duration...).
pub fn lead_icon(row: &mut Flex, icon: &str) -> Frame {
    let mut f = Frame::default();
    f.set_image(super::icons::icon(icon, super::muted(), 16));
    row.fixed(&f, LEAD);
    f
}

#[cfg(test)]
mod tests {
    use super::wanted;

    #[test]
    fn the_list_grows_one_past_the_last_filled() {
        let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(wanted(&v(&["", "", "", ""]), 2, 5), 2);
        assert_eq!(wanted(&v(&["a", "", "", ""]), 2, 5), 2);
        assert_eq!(wanted(&v(&["a", "b", "", ""]), 2, 5), 3);
        assert_eq!(wanted(&v(&["a", "b", "c", "d", "e"]), 2, 5), 5);
        // A gap in the middle still keeps the ones after it.
        assert_eq!(wanted(&v(&["a", "", "c", "", ""]), 2, 5), 4);
        // Fewer allowed (YouTube): never past it.
        assert_eq!(wanted(&v(&["a", "b", "c", "d", "e"]), 2, 4), 4);
    }
}
